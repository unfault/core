use crate::parse::ast::{AstLocation, ParsedFile};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tree_sitter::Node;

use crate::semantics::common::http::{HttpUrlExpr, HttpUrlExprKind, RetryMechanism};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HttpClientKind {
    NetHttp,
    Resty,
    RetryableHttp,
    Fasthttp,
    Fiber,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HttpFramework {
    NetHttp,
    Gin,
    Echo,
    Fiber,
    Chi,
    Mux,
    Beego,
    Other(String),
}

#[derive(Default, Clone)]
struct GoHttpBindings {
    ctx_timeouts: HashMap<String, f64>,
    req_contexts: HashMap<String, String>,
    req_urls: HashMap<String, (Option<String>, Option<HttpUrlExpr>)>,
    client_timeouts: HashMap<String, f64>,
}

fn lookup_ctx_timeout(stack: &[GoHttpBindings], name: &str) -> Option<f64> {
    for scope in stack.iter().rev() {
        if let Some(v) = scope.ctx_timeouts.get(name) {
            return Some(*v);
        }
    }
    None
}

fn lookup_req_url(
    stack: &[GoHttpBindings],
    name: &str,
) -> Option<(Option<String>, Option<HttpUrlExpr>)> {
    for scope in stack.iter().rev() {
        if let Some(v) = scope.req_urls.get(name) {
            return Some(v.clone());
        }
    }
    None
}

fn lookup_req_ctx(stack: &[GoHttpBindings], name: &str) -> Option<String> {
    for scope in stack.iter().rev() {
        if let Some(v) = scope.req_contexts.get(name) {
            return Some(v.clone());
        }
    }
    None
}

fn lookup_client_timeout(stack: &[GoHttpBindings], name: &str) -> Option<f64> {
    for scope in stack.iter().rev() {
        if let Some(v) = scope.client_timeouts.get(name) {
            return Some(*v);
        }
    }
    None
}

/// A single HTTP client call in Go code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpCallSite {
    /// Library: net/http, resty, etc.
    pub client_kind: HttpClientKind,

    /// Method name, e.g., "Get", "Post", "Do".
    pub method_name: String,

    /// URL if statically determinable as a literal.
    pub url_literal: Option<String>,

    /// URL expression metadata when the URL is not a literal.
    pub url_expr: Option<HttpUrlExpr>,

    /// Exact text of the call expression.
    pub call_text: String,

    /// Whether this call has a timeout configured (context with timeout or client timeout).
    pub has_timeout: bool,

    /// Timeout value in seconds (if statically determinable).
    #[serde(default)]
    pub timeout_value: Option<f64>,

    /// Retry mechanism detected for this call.
    #[serde(default)]
    pub retry_mechanism: Option<RetryMechanism>,

    /// Whether there's error handling for this call.
    pub error_handled: bool,

    /// Where in the file this call is (line/col).
    pub location: AstLocation,

    /// Name of enclosing function, if we know it.
    pub function_name: Option<String>,

    /// Whether this call occurs inside a loop.
    #[serde(default)]
    pub in_loop: bool,

    /// Byte range of the call in the original source file.
    pub start_byte: usize,
    pub end_byte: usize,
}

/// HTTP handler function detected in the code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpHandler {
    /// The framework being used
    pub framework: HttpFramework,

    /// Handler function name
    pub function_name: String,

    /// HTTP method (GET, POST, etc.), if known
    pub http_method: Option<String>,

    /// Route path, if known
    pub route_path: Option<String>,

    /// Whether the handler has proper error handling
    pub has_error_handling: bool,

    /// Whether the handler respects context
    pub uses_context: bool,

    /// Location
    pub location: AstLocation,

    /// Byte range
    pub start_byte: usize,
    pub end_byte: usize,
}

/// Build a list of HTTP client calls in this Go file.
pub fn summarize_http_clients(file: &ParsedFile) -> Vec<HttpCallSite> {
    let root = file.tree.root_node();
    let mut calls = Vec::new();
    let const_string_bindings = collect_string_const_bindings(file, root);
    collect_http_calls(file, root, &mut calls, &const_string_bindings);
    calls
}

fn collect_http_calls(
    file: &ParsedFile,
    root: Node,
    out: &mut Vec<HttpCallSite>,
    const_string_bindings: &HashMap<String, String>,
) {
    fn track_bindings_in_assignment(
        file: &ParsedFile,
        node: Node,
        stack: &mut Vec<GoHttpBindings>,
        const_string_bindings: &HashMap<String, String>,
    ) {
        // context.WithTimeout(...) -> ctx var
        if let Some(call) =
            find_call_expression_matching(node, |c| is_context_timeout_call(file, c))
        {
            if let Some(ctx_var) = first_identifier_before(file, node, call.start_byte()) {
                if let Some(secs) = extract_timeout_seconds_from_context_call(file, call) {
                    if let Some(scope) = stack.last_mut() {
                        scope.ctx_timeouts.insert(ctx_var, secs);
                    }
                }
            }
        }

        // http.NewRequest*(...) -> req var + url + ctx
        if let Some(call) =
            find_call_expression_matching(node, |c| is_http_new_request_call(file, c))
        {
            if let Some(req_var) = first_identifier_before(file, node, call.start_byte()) {
                if let Some((u_lit, u_expr, ctx_name)) =
                    extract_request_info(file, call, const_string_bindings)
                {
                    if let Some(scope) = stack.last_mut() {
                        scope.req_urls.insert(req_var.clone(), (u_lit, u_expr));
                        if let Some(ctx_name) = ctx_name {
                            scope.req_contexts.insert(req_var, ctx_name);
                        }
                    }
                }
            }
        }

        // http.Client{Timeout: ...} assigned to a var.
        let stmt_text = file.text_for_node(&node);
        if stmt_text.contains("http.Client{") && stmt_text.contains("Timeout:") {
            if let Some(var) = first_identifier_before_text(&stmt_text) {
                if let (true, Some(secs)) = detect_timeout_and_value_go(&stmt_text) {
                    if let Some(scope) = stack.last_mut() {
                        scope.client_timeouts.insert(var, secs);
                    }
                }
            }
        }
    }

    fn walk(
        file: &ParsedFile,
        node: Node,
        out: &mut Vec<HttpCallSite>,
        enclosing_fn_name: &mut Option<String>,
        in_loop: bool,
        stack: &mut Vec<GoHttpBindings>,
        const_string_bindings: &HashMap<String, String>,
    ) {
        // Track function boundaries
        if matches!(node.kind(), "function_declaration" | "method_declaration") {
            if let Some(name_node) = node.child_by_field_name("name") {
                *enclosing_fn_name = Some(file.text_for_node(&name_node));
            }

            stack.push(GoHttpBindings::default());
        }

        // Track loop context.
        let in_loop = in_loop || matches!(node.kind(), "for_statement" | "range_clause");

        if matches!(
            node.kind(),
            "short_var_declaration" | "assignment_statement"
        ) {
            track_bindings_in_assignment(file, node, stack, const_string_bindings);
        }

        if node.kind() == "call_expression" {
            if let Some(site) = extract_http_call(
                file,
                node,
                enclosing_fn_name.clone(),
                in_loop,
                stack,
                const_string_bindings,
            ) {
                out.push(site);
            }
        }

        let mut child = node.child(0);
        while let Some(c) = child {
            walk(
                file,
                c,
                out,
                enclosing_fn_name,
                in_loop,
                stack,
                const_string_bindings,
            );
            child = c.next_sibling();
        }

        // Leaving function scope
        if matches!(node.kind(), "function_declaration" | "method_declaration") {
            *enclosing_fn_name = None;
            stack.pop();
        }
    }

    let mut enclosing_fn_name: Option<String> = None;
    let mut stack: Vec<GoHttpBindings> = vec![GoHttpBindings::default()];
    walk(
        file,
        root,
        out,
        &mut enclosing_fn_name,
        false,
        &mut stack,
        const_string_bindings,
    );
}

fn extract_http_call(
    file: &ParsedFile,
    call_node: Node,
    enclosing_fn_name: Option<String>,
    in_loop: bool,
    stack: &[GoHttpBindings],
    const_string_bindings: &HashMap<String, String>,
) -> Option<HttpCallSite> {
    let func = call_node.child_by_field_name("function")?;
    let call_text = file.text_for_node(&call_node);

    // Check for http.Get, http.Post, http.Do, etc.
    let (client_kind, method_name, receiver_ident) = if func.kind() == "selector_expression" {
        let object = func.child_by_field_name("operand")?;
        let field = func.child_by_field_name("field")?;

        let object_text = file.text_for_node(&object);
        let method_name = file.text_for_node(&field);
        let receiver_ident = Some(object_text.clone());

        // Heuristic: `x.Do(req)` where `req` was built via http.NewRequest* in this scope.
        // This avoids missing common client variable names like `c`, `cli`, etc.
        let is_net_http_do_on_req = if method_name == "Do" {
            call_node
                .child_by_field_name("arguments")
                .and_then(|args| args.named_child(0))
                .is_some_and(|first| {
                    first.kind() == "identifier"
                        && lookup_req_url(stack, &file.text_for_node(&first)).is_some()
                })
        } else {
            false
        };

        // Check for net/http client calls
        if is_net_http_do_on_req {
            (HttpClientKind::NetHttp, method_name, receiver_ident)
        } else if object_text == "http" {
            match method_name.as_str() {
                "Get" | "Post" | "PostForm" | "Head" => {
                    (HttpClientKind::NetHttp, method_name, receiver_ident)
                }
                _ => return None,
            }
        } else if object_text == "retryablehttp" {
            match method_name.as_str() {
                "Get" | "Post" | "Do" => {
                    (HttpClientKind::RetryableHttp, method_name, receiver_ident)
                }
                _ => return None,
            }
        } else if object_text.ends_with("Client") || object_text.contains("client") {
            // Likely an http.Client instance
            if matches!(method_name.as_str(), "Do" | "Get" | "Post" | "Head") {
                (HttpClientKind::NetHttp, method_name, receiver_ident)
            } else {
                return None;
            }
        } else if object_text.contains("resty") {
            (HttpClientKind::Resty, method_name, receiver_ident)
        } else if object_text.contains("fasthttp") {
            (HttpClientKind::Fasthttp, method_name, receiver_ident)
        } else {
            return None;
        }
    } else {
        return None;
    };

    let (mut has_timeout, mut timeout_value) = detect_timeout_and_value_go(&call_text);

    let retry_mechanism = detect_retry_mechanism_go(&call_text);

    // Check if the result is being handled (assigned to a variable or used)
    let parent = call_node.parent();
    let error_handled = parent.is_some_and(|p| {
        matches!(
            p.kind(),
            "short_var_declaration" | "assignment_statement" | "if_statement"
        )
    });

    let location = file.location_for_node(&call_node);
    let byte_range = call_node.byte_range();

    let (mut url_literal, mut url_expr) =
        extract_url_from_first_arg(file, call_node, const_string_bindings);

    // net/http: client.Do(req) - resolve URL/timeout from request bindings.
    if matches!(client_kind, HttpClientKind::NetHttp) && method_name == "Do" {
        if let Some(args) = call_node.child_by_field_name("arguments") {
            if let Some(first) = args.named_child(0) {
                if first.kind() == "identifier" {
                    let req_name = file.text_for_node(&first);
                    if let Some((u_lit, u_expr)) = lookup_req_url(stack, &req_name) {
                        url_literal = u_lit;
                        url_expr = u_expr;
                    }
                    if !has_timeout {
                        if let Some(ctx_name) = lookup_req_ctx(stack, &req_name) {
                            if let Some(secs) = lookup_ctx_timeout(stack, &ctx_name) {
                                has_timeout = true;
                                timeout_value = Some(secs);
                            }
                        }
                    }
                }
            }
        }

        if !has_timeout {
            if let Some(recv) = receiver_ident.as_deref() {
                if let Some(secs) = lookup_client_timeout(stack, recv) {
                    has_timeout = true;
                    timeout_value = Some(secs);
                }
            }
        }
    }

    Some(HttpCallSite {
        client_kind,
        method_name,
        url_literal,
        url_expr,
        call_text,
        has_timeout,
        timeout_value,
        retry_mechanism,
        error_handled,
        location,
        function_name: enclosing_fn_name,
        in_loop,
        start_byte: byte_range.start,
        end_byte: byte_range.end,
    })
}

fn find_call_expression_matching<F>(node: Node, predicate: F) -> Option<Node>
where
    F: Fn(Node) -> bool + Copy,
{
    if node.kind() == "call_expression" && predicate(node) {
        return Some(node);
    }
    let mut child = node.child(0);
    while let Some(c) = child {
        if let Some(found) = find_call_expression_matching(c, predicate) {
            return Some(found);
        }
        child = c.next_sibling();
    }
    None
}

fn first_identifier_before(file: &ParsedFile, node: Node, byte: usize) -> Option<String> {
    let mut child = node.child(0);
    while let Some(c) = child {
        if c.kind() == "identifier" && c.end_byte() <= byte {
            return Some(file.text_for_node(&c));
        }
        if c.end_byte() <= byte {
            if let Some(found) = first_identifier_before(file, c, byte) {
                return Some(found);
            }
        }
        child = c.next_sibling();
    }
    None
}

fn first_identifier_before_text(stmt_text: &str) -> Option<String> {
    let before = stmt_text
        .split(":=")
        .next()
        .or_else(|| stmt_text.split('=').next())?;
    let name = before
        .split(',')
        .next()?
        .trim()
        .split_whitespace()
        .last()?
        .to_string();
    if name.is_empty() { None } else { Some(name) }
}

fn is_context_timeout_call(file: &ParsedFile, call: Node) -> bool {
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "selector_expression" {
        return false;
    }
    let Some(operand) = func.child_by_field_name("operand") else {
        return false;
    };
    let Some(field) = func.child_by_field_name("field") else {
        return false;
    };
    file.text_for_node(&operand) == "context"
        && matches!(
            file.text_for_node(&field).as_str(),
            "WithTimeout" | "WithDeadline"
        )
}

fn extract_timeout_seconds_from_context_call(file: &ParsedFile, call: Node) -> Option<f64> {
    let func = call.child_by_field_name("function")?;
    let field = func.child_by_field_name("field")?;
    let field_text = file.text_for_node(&field);
    let args = call.child_by_field_name("arguments")?;

    if field_text == "WithTimeout" {
        let dur = args.named_child(1)?;
        return parse_go_duration_seconds(&file.text_for_node(&dur));
    }

    if field_text == "WithDeadline" {
        let deadline = args.named_child(1)?;
        let txt = file.text_for_node(&deadline);
        if let Some(idx) = txt.find(".Add(") {
            let after = &txt[idx + ".Add(".len()..];
            if let Some(end) = after.find(')') {
                return parse_go_duration_seconds(&after[..end]);
            }
        }
    }

    None
}

fn is_http_new_request_call(file: &ParsedFile, call: Node) -> bool {
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    if func.kind() != "selector_expression" {
        return false;
    }
    let Some(operand) = func.child_by_field_name("operand") else {
        return false;
    };
    let Some(field) = func.child_by_field_name("field") else {
        return false;
    };
    if file.text_for_node(&operand) != "http" {
        return false;
    }
    matches!(
        file.text_for_node(&field).as_str(),
        "NewRequest" | "NewRequestWithContext"
    )
}

fn extract_request_info(
    file: &ParsedFile,
    call: Node,
    const_string_bindings: &HashMap<String, String>,
) -> Option<(Option<String>, Option<HttpUrlExpr>, Option<String>)> {
    let func = call.child_by_field_name("function")?;
    let field = func.child_by_field_name("field")?;
    let field_text = file.text_for_node(&field);
    let args = call.child_by_field_name("arguments")?;

    if field_text == "NewRequestWithContext" {
        let ctx = args.named_child(0)?;
        let url = args.named_child(2)?;
        let ctx_name = if ctx.kind() == "identifier" {
            Some(file.text_for_node(&ctx))
        } else {
            None
        };
        let (u_lit, u_expr) = extract_url_from_expr(file, url, const_string_bindings);
        return Some((u_lit, u_expr, ctx_name));
    }

    if field_text == "NewRequest" {
        let url = args.named_child(1)?;
        let (u_lit, u_expr) = extract_url_from_expr(file, url, const_string_bindings);
        return Some((u_lit, u_expr, None));
    }

    None
}

fn detect_timeout_and_value_go(call_text: &str) -> (bool, Option<f64>) {
    let text: String = call_text.chars().filter(|c| !c.is_whitespace()).collect();

    // net/http struct literal: http.Client{Timeout: 5*time.Second}
    if let Some(idx) = text.find("Timeout:") {
        let after = &text[idx + "Timeout:".len()..];
        let end = after
            .find(|c| c == ',' || c == '}' || c == ')')
            .unwrap_or(after.len());
        let expr = &after[..end];
        return (true, parse_go_duration_seconds(expr));
    }

    // context.WithTimeout(ctx, 5*time.Second)
    if let Some(idx) = text.find("WithTimeout(") {
        let after = &text[idx + "WithTimeout(".len()..];
        // best-effort: take last argument up to ')'
        if let Some(end_paren) = after.find(')') {
            let inside = &after[..end_paren];
            if let Some(last) = inside.rsplit(',').next() {
                return (true, parse_go_duration_seconds(last));
            }
        }
        return (true, None);
    }

    // resty: SetTimeout(5*time.Second)
    if let Some(idx) = text.find("SetTimeout(") {
        let after = &text[idx + "SetTimeout(".len()..];
        if let Some(end_paren) = after.find(')') {
            let inside = &after[..end_paren];
            return (true, parse_go_duration_seconds(inside));
        }
        return (true, None);
    }

    if text.contains("WithDeadline(") {
        return (true, None);
    }

    (false, None)
}

fn parse_go_duration_seconds(expr: &str) -> Option<f64> {
    let s: String = expr.chars().filter(|c| !c.is_whitespace()).collect();
    let s = s.trim_matches(|c| c == '(' || c == ')');

    let (unit, mult) = if s.contains("time.Millisecond") {
        ("time.Millisecond", 0.001)
    } else if s.contains("time.Second") {
        ("time.Second", 1.0)
    } else if s.contains("time.Minute") {
        ("time.Minute", 60.0)
    } else if s.contains("time.Hour") {
        ("time.Hour", 3600.0)
    } else {
        return None;
    };

    // unit alone
    if s == unit {
        return Some(mult);
    }

    // n*unit or unit*n
    if let Some((a, b)) = s.split_once('*') {
        let (num, other) = if b.contains(unit) {
            (a, b)
        } else if a.contains(unit) {
            (b, a)
        } else {
            return None;
        };
        if !other.contains(unit) {
            return None;
        }
        if let Ok(n) = num.trim().parse::<f64>() {
            return Some(n * mult);
        }
    }

    None
}

fn detect_retry_mechanism_go(call_text: &str) -> Option<RetryMechanism> {
    let text: String = call_text.chars().filter(|c| !c.is_whitespace()).collect();
    // resty retry configuration in call chains: SetRetryCount/SetRetryWaitTime/etc.
    if text.contains("SetRetry") || text.contains("RetryCount") || text.contains("RetryWait") {
        return Some(RetryMechanism::ClientConfig);
    }
    // hashicorp/go-retryablehttp patterns
    if text.contains("retryablehttp") {
        return Some(RetryMechanism::Middleware("go-retryablehttp".to_string()));
    }
    None
}

fn collect_string_const_bindings(file: &ParsedFile, root: Node) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();

    fn walk(file: &ParsedFile, node: Node, out: &mut HashMap<String, String>) {
        // tree-sitter-go represents const declarations via const_declaration/const_spec.
        // We keep this best-effort and only capture direct string literals.
        if node.kind() == "const_spec" {
            let mut name: Option<String> = None;
            let mut value: Option<String> = None;

            // Prefer field-based extraction when present.
            if let Some(n) = node.child_by_field_name("name") {
                name = Some(file.text_for_node(&n));
            }
            if let Some(v) = node.child_by_field_name("value") {
                value = extract_string_literal_go(file, v);
            }

            // Fallback: scan children for first identifier and first string literal.
            if name.is_none() || value.is_none() {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if name.is_none() && child.kind() == "identifier" {
                        name = Some(file.text_for_node(&child));
                    }
                    if value.is_none() {
                        value = extract_string_literal_go(file, child);
                    }
                }
            }

            if let (Some(k), Some(v)) = (name, value) {
                out.insert(k, v);
            }
        }

        let mut child = node.child(0);
        while let Some(c) = child {
            walk(file, c, out);
            child = c.next_sibling();
        }
    }

    walk(file, root, &mut out);

    // Fallback: best-effort text scan for common const patterns.
    // This helps when the Go grammar shape differs across versions.
    extend_const_bindings_from_text(file, &mut out);
    out
}

fn extend_const_bindings_from_text(file: &ParsedFile, out: &mut HashMap<String, String>) {
    let mut in_block = false;
    for line in file.source.lines() {
        let l = line.trim();
        if l.starts_with("const (") {
            in_block = true;
            continue;
        }
        if in_block && l.starts_with(')') {
            in_block = false;
            continue;
        }

        if l.starts_with("const ") {
            // const NAME = "VALUE"
            if let Some((name, value)) = parse_const_binding_line(&l[6..]) {
                out.entry(name).or_insert(value);
            }
            continue;
        }

        if in_block {
            // NAME = "VALUE"
            if let Some((name, value)) = parse_const_binding_line(l) {
                out.entry(name).or_insert(value);
            }
        }
    }
}

fn parse_const_binding_line(s: &str) -> Option<(String, String)> {
    // Accept NAME = "..." or NAME="..." or raw string with backticks.
    let s = s.trim();
    let eq = s.find('=')?;
    let name = s[..eq].trim();
    if name.is_empty() {
        return None;
    }
    let rhs = s[eq + 1..].trim();
    if rhs.is_empty() {
        return None;
    }
    let end_quote = if rhs.starts_with('"') {
        '"'
    } else if rhs.starts_with('`') {
        '`'
    } else {
        return None;
    };
    let rest = &rhs[1..];
    let end = rest.find(end_quote)?;
    let value = rest[..end].to_string();
    Some((name.to_string(), value))
}

fn extract_url_from_first_arg(
    file: &ParsedFile,
    call_node: Node,
    const_string_bindings: &HashMap<String, String>,
) -> (Option<String>, Option<HttpUrlExpr>) {
    let Some(args) = call_node.child_by_field_name("arguments") else {
        return (None, None);
    };
    let Some(first) = args.named_child(0) else {
        return (None, None);
    };
    extract_url_from_expr(file, first, const_string_bindings)
}

fn extract_url_from_expr(
    file: &ParsedFile,
    expr: Node,
    const_string_bindings: &HashMap<String, String>,
) -> (Option<String>, Option<HttpUrlExpr>) {
    if let Some(lit) = extract_string_literal_go(file, expr) {
        return (Some(lit), None);
    }

    let env_var = detect_env_var_name_go_deep(file, expr, const_string_bindings);
    let kind = match expr.kind() {
        "identifier" => HttpUrlExprKind::Identifier,
        "selector_expression" | "index_expression" => HttpUrlExprKind::Member,
        "call_expression" => HttpUrlExprKind::Call,
        "binary_expression" => HttpUrlExprKind::Template,
        _ => HttpUrlExprKind::Unknown,
    };

    let text = file.text_for_node(&expr).trim().to_string();
    (
        None,
        Some(HttpUrlExpr {
            text,
            kind,
            env_var,
        }),
    )
}

fn extract_string_literal_go(file: &ParsedFile, node: Node) -> Option<String> {
    let kind = node.kind();
    if kind != "interpreted_string_literal" && kind != "raw_string_literal" {
        return None;
    }
    let text = file.text_for_node(&node);
    // interpreted: "..." ; raw: `...`
    Some(
        text.trim()
            .trim_matches(|c| c == '"' || c == '`')
            .to_string(),
    )
}

fn detect_env_var_name_go_deep(
    file: &ParsedFile,
    node: Node,
    const_string_bindings: &HashMap<String, String>,
) -> Option<String> {
    if let Some(v) = detect_env_var_name_go(file, node, const_string_bindings) {
        return Some(v);
    }

    let mut child = node.child(0);
    while let Some(c) = child {
        if let Some(v) = detect_env_var_name_go_deep(file, c, const_string_bindings) {
            return Some(v);
        }
        child = c.next_sibling();
    }
    None
}

fn detect_env_var_name_go(
    file: &ParsedFile,
    node: Node,
    const_string_bindings: &HashMap<String, String>,
) -> Option<String> {
    if node.kind() != "call_expression" {
        return None;
    }

    let func = node.child_by_field_name("function")?;
    if func.kind() != "selector_expression" {
        return None;
    }

    let operand = func.child_by_field_name("operand")?;
    let field = func.child_by_field_name("field")?;
    let operand_text = file.text_for_node(&operand);
    let field_text = file.text_for_node(&field);

    // Standard env var reads.
    if operand_text != "os" {
        return None;
    }
    if field_text != "Getenv" {
        return None;
    }

    let args = node.child_by_field_name("arguments")?;
    let key_expr = args.named_child(0)?;
    if let Some(k) = extract_string_literal_go(file, key_expr) {
        return Some(k);
    }
    if key_expr.kind() == "identifier" {
        let k = file.text_for_node(&key_expr);
        if let Some(v) = const_string_bindings.get(&k) {
            return Some(v.clone());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::go::parse_go_file;
    use crate::types::context::{Language, SourceFile};

    fn parse_and_summarize_http(source: &str) -> Vec<HttpCallSite> {
        let sf = SourceFile {
            path: "test.go".to_string(),
            language: Language::Go,
            content: source.to_string(),
        };
        let parsed = parse_go_file(FileId(1), &sf).expect("parsing should succeed");
        summarize_http_clients(&parsed)
    }

    #[test]
    fn detects_http_get() {
        let src = r#"
package main

import "net/http"

func fetch() {
    http.Get("https://example.com")
}
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert!(matches!(calls[0].client_kind, HttpClientKind::NetHttp));
        assert_eq!(calls[0].method_name, "Get");
        assert_eq!(
            calls[0].url_literal,
            Some("https://example.com".to_string())
        );
        assert!(calls[0].url_expr.is_none());
    }

    #[test]
    fn extracts_env_var_from_os_getenv_literal() {
        let src = r#"
package main

import (
    "net/http"
    "os"
)

func fetch() {
    http.Get(os.Getenv("API_URL"))
}
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert!(calls[0].url_literal.is_none());
        let expr = calls[0]
            .url_expr
            .clone()
            .expect("url_expr should be present");
        assert_eq!(expr.env_var, Some("API_URL".to_string()));
    }

    #[test]
    fn extracts_env_var_from_os_getenv_via_const_binding() {
        let src = r#"
package main

import (
    "net/http"
    "os"
)

const API_URL_KEY = "API_URL"

func fetch() {
    http.Get(os.Getenv(API_URL_KEY))
}
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        let expr = calls[0]
            .url_expr
            .clone()
            .expect("url_expr should be present");
        assert_eq!(expr.env_var, Some("API_URL".to_string()));
    }

    #[test]
    fn detects_http_post() {
        let src = r#"
package main

import "net/http"

func sendData() {
    http.Post("https://example.com", "application/json", nil)
}
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method_name, "Post");
    }

    #[test]
    fn captures_function_name() {
        let src = r#"
package main

import "net/http"

func fetchData() {
    http.Get("https://example.com")
}
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, Some("fetchData".to_string()));
    }

    #[test]
    fn resolves_url_and_timeout_for_client_do_new_request_with_context() {
        let src = r#"
package main

import (
  "context"
  "net/http"
  "time"
)

func fetch() {
  ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
  defer cancel()
  req, _ := http.NewRequestWithContext(ctx, "GET", "https://example.com", nil)
  c := &http.Client{}
  c.Do(req)
}
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method_name, "Do");
        assert_eq!(
            calls[0].url_literal,
            Some("https://example.com".to_string())
        );
        assert!(calls[0].has_timeout);
        assert_eq!(calls[0].timeout_value, Some(5.0));
    }

    #[test]
    fn resolves_timeout_for_client_do_from_client_timeout_field() {
        let src = r#"
package main

import (
  "net/http"
  "time"
)

func fetch() {
  req, _ := http.NewRequest("GET", "https://example.com", nil)
  client := &http.Client{Timeout: 2 * time.Second}
  client.Do(req)
}
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert!(calls[0].has_timeout);
        assert_eq!(calls[0].timeout_value, Some(2.0));
    }

    #[test]
    fn detects_retryablehttp_client() {
        let src = r#"
package main

import "github.com/hashicorp/go-retryablehttp"

func fetch() {
  retryablehttp.Get("https://example.com")
}
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert!(matches!(
            calls[0].client_kind,
            HttpClientKind::RetryableHttp
        ));
    }

    #[test]
    fn handles_empty_file() {
        let calls = parse_and_summarize_http("");
        assert!(calls.is_empty());
    }

    #[test]
    fn ignores_non_http_calls() {
        let src = r#"
package main

import "fmt"

func main() {
    fmt.Println("hello")
}
"#;
        let calls = parse_and_summarize_http(src);
        assert!(calls.is_empty());
    }
}
