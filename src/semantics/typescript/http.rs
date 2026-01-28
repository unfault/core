//! HTTP client detection for TypeScript/JavaScript code.

use serde::{Deserialize, Serialize};

use std::collections::HashMap;

use crate::parse::ast::{AstLocation, ParsedFile};

use crate::semantics::common::http::{HttpUrlExpr, HttpUrlExprKind};

/// Represents an HTTP client call in TypeScript code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpCallSite {
    /// The HTTP client library being used
    pub client_kind: HttpClientKind,
    /// HTTP method (get, post, etc.)
    pub method: String,
    /// URL if statically determinable
    pub url: Option<String>,

    /// URL if statically determinable as a literal
    pub url_literal: Option<String>,

    /// URL expression metadata when the URL is not a literal
    pub url_expr: Option<HttpUrlExpr>,
    /// Whether a timeout is configured
    pub has_timeout: bool,

    /// Timeout value in seconds (if statically determinable)
    pub timeout_value: Option<f64>,

    /// Whether error handling is present (try-catch or .catch())
    pub has_error_handling: bool,
    /// Whether retry logic is configured
    pub has_retry: bool,
    /// Name of the enclosing function
    pub function_name: Option<String>,
    /// Whether this call is in an async context
    pub in_async_context: bool,

    /// Whether this call is inside a loop
    #[serde(default)]
    pub in_loop: bool,
    /// Location in the source file
    pub location: AstLocation,

    /// Full text of the call expression
    #[serde(default)]
    pub call_text: String,
    /// Start byte offset
    pub start_byte: usize,
    /// End byte offset
    pub end_byte: usize,
}

/// Known HTTP client libraries in the TypeScript/JavaScript ecosystem.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum HttpClientKind {
    /// Native fetch API
    Fetch,
    /// Axios HTTP client
    Axios,
    /// Node.js http/https modules
    NodeHttp,
    /// Got HTTP client
    Got,
    /// Node-fetch
    NodeFetch,
    /// Undici
    Undici,
    /// ky HTTP client
    Ky,
    /// Superagent
    Superagent,
    /// Unknown HTTP client
    Unknown,
}

/// Summarize HTTP client calls in a TypeScript file.
pub fn summarize_http_clients(parsed: &ParsedFile) -> Vec<HttpCallSite> {
    let mut calls = Vec::new();

    let mut client_bindings: HashMap<String, HttpClientBinding> = HashMap::new();

    let root = parsed.tree.root_node();
    walk_for_http_calls(
        root,
        parsed,
        &mut calls,
        None,
        false,
        false,
        &mut client_bindings,
    );

    calls
}

fn walk_for_http_calls(
    node: tree_sitter::Node,
    parsed: &ParsedFile,
    calls: &mut Vec<HttpCallSite>,
    current_function: Option<&str>,
    in_async: bool,
    in_loop: bool,
    client_bindings: &mut HashMap<String, HttpClientBinding>,
) {
    // Track function context
    let (func_name, is_async) = match node.kind() {
        "function_declaration" | "function" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| parsed.text_for_node(&n));
            let text = parsed.text_for_node(&node);
            let async_fn = text.trim_start().starts_with("async");
            (name, async_fn)
        }
        "arrow_function" | "method_definition" => {
            let text = parsed.text_for_node(&node);
            let async_fn = text.trim_start().starts_with("async");
            (None, async_fn)
        }
        _ => (None, in_async),
    };

    let effective_function = func_name.as_deref().or(current_function);
    let effective_async = is_async || in_async;
    let effective_loop = in_loop
        || matches!(
            node.kind(),
            "for_statement"
                | "while_statement"
                | "do_statement"
                | "for_in_statement"
                | "for_of_statement"
        );

    // Track client bindings and retry configuration.
    maybe_track_http_client_binding(parsed, &node, client_bindings);

    // Check for HTTP calls
    if node.kind() == "call_expression" {
        if let Some(call) = detect_http_call(
            parsed,
            &node,
            effective_function,
            effective_async,
            effective_loop,
            client_bindings,
        ) {
            calls.push(call);
        }
    }

    // Recurse
    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            walk_for_http_calls(
                child,
                parsed,
                calls,
                effective_function,
                effective_async,
                effective_loop,
                client_bindings,
            );
        }
    }
}

fn detect_http_call(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    function_name: Option<&str>,
    in_async: bool,
    in_loop: bool,
    client_bindings: &HashMap<String, HttpClientBinding>,
) -> Option<HttpCallSite> {
    let func_node = node.child_by_field_name("function")?;
    let callee = parsed.text_for_node(&func_node);
    let callee_norm = normalize_ts_chain(callee.as_str());
    let location = parsed.location_for_node(node);

    // Exclude client construction / extension calls.
    if matches!(
        callee_norm.as_str(),
        "axios.create" | "got.extend" | "ky.create" | "ky.extend"
    ) {
        return None;
    }

    // Detect HTTP client and method
    let (mut client_kind, method) =
        detect_client_and_method_with_bindings(&callee, client_bindings)?;

    // Exclude route handlers (e.g., app.get('/path'), router.post('/path'))
    if client_kind == HttpClientKind::Unknown && is_route_handler(parsed, node) {
        return None;
    }

    let (url_literal, url_expr) = extract_url_from_first_arg(parsed, node);
    // Backwards-compatible field: only populate when the URL is a literal.
    let mut url_literal = url_literal;
    let mut url_expr = url_expr;
    let mut url = url_literal.clone();

    // Check for timeout configuration
    let (mut has_timeout, mut timeout_value) = extract_timeout(parsed, node, &callee, client_kind);

    // Check for error handling
    let has_error_handling = check_error_handling(node);

    // Check for retry logic
    let mut has_retry = extract_retry(parsed, node);

    // Apply instance-level bindings (axios.create / got.extend / ky.create + axios-retry)
    if let Some(binding) = binding_for_callee(&callee, client_bindings) {
        if client_kind == HttpClientKind::Unknown {
            client_kind = binding.kind;
        }

        if !has_timeout {
            if let Some(secs) = binding.default_timeout {
                has_timeout = true;
                timeout_value = Some(secs);
            }
        }

        if !has_retry && binding.has_retry {
            has_retry = true;
        }

        if let Some(base) = &binding.base_url {
            if let Some(u) = &url_literal {
                if is_relative_url_literal(u) {
                    let joined = join_url(base, u);
                    url_literal = Some(joined.clone());
                    url = Some(joined);
                    url_expr = None;
                }
            }
        }
    }

    Some(HttpCallSite {
        client_kind,
        method,
        url,
        url_literal,
        url_expr,
        has_timeout,
        timeout_value,
        has_error_handling,
        has_retry,
        function_name: function_name.map(|s| s.to_string()),
        in_async_context: in_async,
        in_loop,
        location,
        call_text: parsed.text_for_node(node),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

#[derive(Debug, Clone)]
struct HttpClientBinding {
    kind: HttpClientKind,
    base_url: Option<String>,
    default_timeout: Option<f64>,
    has_retry: bool,
}

fn maybe_track_http_client_binding(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    bindings: &mut HashMap<String, HttpClientBinding>,
) {
    // const api = axios.create({ baseURL: '...', timeout: 5000 })
    // const api = got.extend({ prefixUrl: '...', timeout: { request: 5000 }, retry: { limit: 2 } })
    if node.kind() == "variable_declarator" {
        let name = node.child_by_field_name("name");
        let value = node.child_by_field_name("value");
        if let (Some(name), Some(value)) = (name, value) {
            if let Some(var) = extract_identifier_from_pattern(parsed, &name) {
                if let Some(binding) = extract_http_client_binding_from_value(parsed, &value) {
                    bindings.insert(var, binding);
                }
            }
        }
    }

    // api = axios.create(...)
    if node.kind() == "assignment_expression" {
        let left = node.child_by_field_name("left");
        let right = node.child_by_field_name("right");
        if let (Some(left), Some(right)) = (left, right) {
            if let Some(var) = extract_identifier_from_pattern(parsed, &left) {
                if let Some(binding) = extract_http_client_binding_from_value(parsed, &right) {
                    bindings.insert(var, binding);
                }
            }
        }
    }

    // axiosRetry(api, { retries: 3 })
    if node.kind() == "call_expression" {
        let func = match node.child_by_field_name("function") {
            Some(f) => f,
            None => return,
        };
        let callee_norm = normalize_ts_chain(parsed.text_for_node(&func).as_str());
        if callee_norm != "axiosRetry" {
            return;
        }

        let args = match node.child_by_field_name("arguments") {
            Some(a) => a,
            None => return,
        };
        let target = match args.named_child(0) {
            Some(t) => t,
            None => return,
        };
        if target.kind() != "identifier" {
            return;
        }
        let name = parsed.text_for_node(&target);

        // Mark the target binding (or create one for axios itself).
        let entry = bindings.entry(name).or_insert(HttpClientBinding {
            kind: HttpClientKind::Axios,
            base_url: None,
            default_timeout: None,
            has_retry: false,
        });
        entry.has_retry = true;
    }
}

fn extract_identifier_from_pattern(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
) -> Option<String> {
    if node.kind() == "identifier" {
        return Some(parsed.text_for_node(node));
    }
    None
}

fn extract_http_client_binding_from_value(
    parsed: &ParsedFile,
    value: &tree_sitter::Node,
) -> Option<HttpClientBinding> {
    if value.kind() != "call_expression" {
        return None;
    }

    let func = value.child_by_field_name("function")?;
    let callee_norm = normalize_ts_chain(parsed.text_for_node(&func).as_str());

    let (kind, base_key) = if callee_norm == "axios.create" {
        (HttpClientKind::Axios, "baseURL")
    } else if callee_norm == "got.extend" {
        (HttpClientKind::Got, "prefixUrl")
    } else if callee_norm == "ky.create" || callee_norm == "ky.extend" {
        (HttpClientKind::Ky, "prefixUrl")
    } else {
        return None;
    };

    let args = value.child_by_field_name("arguments")?;
    // Find an options object literal.
    let mut opts: Option<tree_sitter::Node> = None;
    for i in 0..args.named_child_count() {
        if let Some(arg) = args.named_child(i) {
            if arg.kind() == "object" {
                opts = Some(arg);
                break;
            }
        }
    }

    let mut base_url: Option<String> = None;
    let mut timeout_secs: Option<f64> = None;
    let mut has_retry = false;

    if let Some(obj) = opts {
        base_url = extract_string_prop(parsed, &obj, base_key);
        timeout_secs = extract_default_timeout_seconds(parsed, &obj, kind);
        has_retry = extract_retry_from_object(parsed, &obj);
    }

    Some(HttpClientBinding {
        kind,
        base_url,
        default_timeout: timeout_secs,
        has_retry,
    })
}

fn extract_string_prop(
    parsed: &ParsedFile,
    obj: &tree_sitter::Node,
    key_name: &str,
) -> Option<String> {
    let mut cursor = obj.walk();
    for child in obj.named_children(&mut cursor) {
        if child.kind() != "pair" {
            continue;
        }
        let key = child.child_by_field_name("key")?;
        let value = child.child_by_field_name("value")?;
        let key_text = parsed
            .text_for_node(&key)
            .trim()
            .trim_matches(|c| c == '\'' || c == '"')
            .to_string();
        if key_text != key_name {
            continue;
        }
        if value.kind() == "string" {
            let v = parsed.text_for_node(&value);
            return Some(v.trim().trim_matches(|c| c == '\'' || c == '"').to_string());
        }
        if value.kind() == "template_string" {
            let v = parsed.text_for_node(&value);
            if !v.contains("${") {
                return Some(v.trim().trim_matches('`').to_string());
            }
        }
    }
    None
}

fn extract_default_timeout_seconds(
    parsed: &ParsedFile,
    obj: &tree_sitter::Node,
    kind: HttpClientKind,
) -> Option<f64> {
    // axios/ky: timeout: 5000 (ms)
    if let Some(ms) = extract_number_prop(parsed, obj, "timeout") {
        return Some(ms / 1000.0);
    }

    // got: timeout: { request: 5000 }
    if kind == HttpClientKind::Got {
        if let Some(timeout_obj) = extract_object_prop(parsed, *obj, "timeout") {
            if let Some(ms) = extract_number_prop(parsed, &timeout_obj, "request") {
                return Some(ms / 1000.0);
            }
        }
    }

    None
}

fn extract_number_prop(
    parsed: &ParsedFile,
    obj: &tree_sitter::Node,
    key_name: &str,
) -> Option<f64> {
    let mut cursor = obj.walk();
    for child in obj.named_children(&mut cursor) {
        if child.kind() != "pair" {
            continue;
        }
        let key = child.child_by_field_name("key")?;
        let value = child.child_by_field_name("value")?;
        let key_text = parsed
            .text_for_node(&key)
            .trim()
            .trim_matches(|c| c == '\'' || c == '"')
            .to_string();
        if key_text != key_name {
            continue;
        }
        if value.kind() != "number" {
            return None;
        }
        return parsed.text_for_node(&value).trim().parse::<f64>().ok();
    }
    None
}

fn extract_object_prop<'a>(
    parsed: &ParsedFile,
    obj: tree_sitter::Node<'a>,
    key_name: &str,
) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = obj.walk();
    for child in obj.named_children(&mut cursor) {
        if child.kind() != "pair" {
            continue;
        }
        let key = child.child_by_field_name("key")?;
        let value = child.child_by_field_name("value")?;
        let key_text = parsed
            .text_for_node(&key)
            .trim()
            .trim_matches(|c| c == '\'' || c == '"')
            .to_string();
        if key_text != key_name {
            continue;
        }
        if value.kind() == "object" {
            return Some(value);
        }
    }
    None
}

fn extract_retry_from_object(parsed: &ParsedFile, obj: &tree_sitter::Node) -> bool {
    // retry: ..., retries: ...
    extract_object_prop(parsed, *obj, "retry").is_some()
        || extract_number_prop(parsed, obj, "retries").is_some()
        || extract_number_prop(parsed, obj, "retry").is_some()
}

fn binding_for_callee<'a>(
    callee: &str,
    bindings: &'a HashMap<String, HttpClientBinding>,
) -> Option<&'a HttpClientBinding> {
    // identifier call: api('/path')
    if let Some(b) = bindings.get(callee) {
        return Some(b);
    }
    // member call: api.get('/path')
    if let Some((recv, _method)) = callee.rsplit_once('.') {
        return bindings.get(recv);
    }
    None
}

fn is_relative_url_literal(u: &str) -> bool {
    let s = u.trim();
    !(s.starts_with("http://") || s.starts_with("https://"))
}

fn join_url(base: &str, path: &str) -> String {
    let mut b = base.trim().trim_end_matches('/').to_string();
    let mut p = path.trim().to_string();
    if !p.starts_with('/') {
        p = format!("/{}", p);
    }
    b.push_str(&p);
    b
}

fn extract_url_from_first_arg(
    parsed: &ParsedFile,
    call: &tree_sitter::Node,
) -> (Option<String>, Option<HttpUrlExpr>) {
    let args = match call.child_by_field_name("arguments") {
        Some(a) => a,
        None => return (None, None),
    };
    let first = match args.named_child(0) {
        Some(a) => a,
        None => return (None, None),
    };

    extract_url_from_expr(parsed, &first)
}

fn extract_url_from_expr(
    parsed: &ParsedFile,
    expr: &tree_sitter::Node,
) -> (Option<String>, Option<HttpUrlExpr>) {
    let text = parsed.text_for_node(expr);
    let trimmed = text.trim();

    // String literal
    if expr.kind() == "string" {
        return (
            Some(trimmed.trim_matches(|c| c == '\'' || c == '"').to_string()),
            None,
        );
    }

    // Template literal
    if expr.kind() == "template_string" {
        let has_subst = trimmed.contains("${");
        if !has_subst {
            return (Some(trimmed.trim_matches('`').to_string()), None);
        }

        return (
            None,
            Some(HttpUrlExpr {
                text: trimmed.to_string(),
                kind: HttpUrlExprKind::Template,
                env_var: None,
            }),
        );
    }

    let kind = match expr.kind() {
        "identifier" => HttpUrlExprKind::Identifier,
        "member_expression" | "subscript_expression" => HttpUrlExprKind::Member,
        "call_expression" => HttpUrlExprKind::Call,
        _ => HttpUrlExprKind::Unknown,
    };

    let env_var = detect_env_var_name_ts(parsed, expr);
    (
        None,
        Some(HttpUrlExpr {
            text: trimmed.to_string(),
            kind,
            env_var,
        }),
    )
}

fn detect_env_var_name_ts(parsed: &ParsedFile, expr: &tree_sitter::Node) -> Option<String> {
    // process.env.FOO / Bun.env.FOO / import.meta.env.VITE_FOO
    if expr.kind() == "member_expression" {
        let object = expr.child_by_field_name("object")?;
        let property = expr.child_by_field_name("property")?;

        if is_process_env_object(parsed, &object)
            || is_bun_env_object(parsed, &object)
            || is_import_meta_env_object(parsed, &object)
        {
            return Some(parsed.text_for_node(&property));
        }
    }

    // process.env["FOO"] / Bun.env["FOO"] / import.meta.env["VITE_FOO"]
    if expr.kind() == "subscript_expression" {
        let object = expr.child_by_field_name("object")?;
        let index = expr.child_by_field_name("index")?;
        if index.kind() == "string"
            && (is_process_env_object(parsed, &object)
                || is_bun_env_object(parsed, &object)
                || is_import_meta_env_object(parsed, &object))
        {
            let idx = parsed.text_for_node(&index);
            return Some(
                idx.trim()
                    .trim_matches(|c| c == '\'' || c == '"')
                    .to_string(),
            );
        }
    }

    // Deno.env.get("FOO")
    if expr.kind() == "call_expression" {
        let func = expr.child_by_field_name("function")?;
        let callee_norm = normalize_ts_chain(parsed.text_for_node(&func).as_str());
        if callee_norm == "Deno.env.get" {
            let args = expr.child_by_field_name("arguments")?;
            let first = args.named_child(0)?;
            if first.kind() == "string" {
                let s = parsed.text_for_node(&first);
                return Some(s.trim().trim_matches(|c| c == '\'' || c == '"').to_string());
            }
        }
    }

    None
}

fn is_process_env_object(parsed: &ParsedFile, expr: &tree_sitter::Node) -> bool {
    // process.env
    if expr.kind() != "member_expression" {
        return false;
    }
    let object = match expr.child_by_field_name("object") {
        Some(o) => o,
        None => return false,
    };
    let property = match expr.child_by_field_name("property") {
        Some(p) => p,
        None => return false,
    };
    parsed.text_for_node(&object) == "process" && parsed.text_for_node(&property) == "env"
}

fn is_bun_env_object(parsed: &ParsedFile, expr: &tree_sitter::Node) -> bool {
    // Bun.env
    if expr.kind() != "member_expression" {
        return false;
    }
    let object = match expr.child_by_field_name("object") {
        Some(o) => o,
        None => return false,
    };
    let property = match expr.child_by_field_name("property") {
        Some(p) => p,
        None => return false,
    };
    parsed.text_for_node(&object) == "Bun" && parsed.text_for_node(&property) == "env"
}

fn is_import_meta_env_object(parsed: &ParsedFile, expr: &tree_sitter::Node) -> bool {
    // import.meta.env
    normalize_ts_chain(parsed.text_for_node(expr).as_str()) == "import.meta.env"
}

fn normalize_ts_chain(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

fn detect_client_and_method(callee: &str) -> Option<(HttpClientKind, String)> {
    // Fetch API
    if callee == "fetch" {
        return Some((HttpClientKind::Fetch, "fetch".to_string()));
    }

    // Axios
    if callee.starts_with("axios.") {
        let method = callee.strip_prefix("axios.").unwrap_or("request");
        return Some((HttpClientKind::Axios, method.to_string()));
    }
    if callee == "axios" {
        return Some((HttpClientKind::Axios, "request".to_string()));
    }

    // Got
    if callee.starts_with("got.") {
        let method = callee.strip_prefix("got.").unwrap_or("request");
        return Some((HttpClientKind::Got, method.to_string()));
    }
    if callee == "got" {
        return Some((HttpClientKind::Got, "request".to_string()));
    }

    // Ky
    if callee.starts_with("ky.") {
        let method = callee.strip_prefix("ky.").unwrap_or("request");
        return Some((HttpClientKind::Ky, method.to_string()));
    }
    if callee == "ky" {
        return Some((HttpClientKind::Ky, "request".to_string()));
    }

    // Node http/https
    if callee == "http.get" || callee == "http.request" {
        let method = callee.strip_prefix("http.").unwrap_or("request");
        return Some((HttpClientKind::NodeHttp, method.to_string()));
    }
    if callee == "https.get" || callee == "https.request" {
        let method = callee.strip_prefix("https.").unwrap_or("request");
        return Some((HttpClientKind::NodeHttp, method.to_string()));
    }

    // Superagent
    if callee.starts_with("superagent.") {
        let method = callee.strip_prefix("superagent.").unwrap_or("request");
        return Some((HttpClientKind::Superagent, method.to_string()));
    }

    // Undici
    if callee == "undici.fetch" || callee == "undici.request" {
        let method = callee.strip_prefix("undici.").unwrap_or("fetch");
        return Some((HttpClientKind::Undici, method.to_string()));
    }

    // Instance method calls (e.g., client.get(), httpClient.post())
    // Only match if the receiver looks like an HTTP client
    if is_http_method_call(callee) && is_likely_http_client_receiver(callee) {
        let parts: Vec<&str> = callee.rsplitn(2, '.').collect();
        if parts.len() == 2 {
            let method = parts[0];
            return Some((HttpClientKind::Unknown, method.to_string()));
        }
    }

    None
}

fn detect_client_and_method_with_bindings(
    callee: &str,
    bindings: &HashMap<String, HttpClientBinding>,
) -> Option<(HttpClientKind, String)> {
    // First, try known global entrypoints (fetch/axios/got/ky/etc.), but don't
    // early-return Unknown instance heuristics (bindings may refine those).
    if let Some((kind, method)) = detect_client_and_method(callee) {
        if kind != HttpClientKind::Unknown {
            return Some((kind, method));
        }
    }

    // Then, try instance bindings.
    if let Some(b) = bindings.get(callee) {
        return Some((b.kind, "request".to_string()));
    }
    if let Some((recv, method)) = callee.rsplit_once('.') {
        if let Some(b) = bindings.get(recv) {
            return Some((b.kind, method.to_string()));
        }
    }

    // Finally, fall back to full heuristics (including Unknown instance calls).
    detect_client_and_method(callee)
}

fn is_http_method_call(callee: &str) -> bool {
    let http_methods = ["get", "post", "put", "patch", "delete", "head", "options"];
    for method in http_methods {
        if callee.ends_with(&format!(".{}", method)) {
            return true;
        }
    }
    false
}

/// Check if the receiver (part before the method) looks like an HTTP client.
/// This prevents false positives from unrelated APIs like `config.get()`, `map.get()`.
fn is_likely_http_client_receiver(callee: &str) -> bool {
    let callee_lower = callee.to_lowercase();

    // Extract the receiver (everything before the last '.')
    let receiver = match callee_lower.rfind('.') {
        Some(pos) => &callee_lower[..pos],
        None => return false,
    };

    // Allowlist of patterns that suggest HTTP client usage
    let http_client_patterns = [
        "client",   // httpClient, apiClient, client
        "http",     // http, httpService
        "api",      // api, apiService
        "service",  // someService (when combined with HTTP methods)
        "request",  // request instance
        "instance", // axios instance
        "agent",    // superagent instance
        "fetch",    // fetch wrapper
    ];

    // Check if receiver contains any HTTP client pattern
    for pattern in http_client_patterns {
        if receiver.contains(pattern) {
            return true;
        }
    }

    // Also allow if receiver ends with common HTTP client suffixes
    let http_suffixes = ["client", "api", "http", "service"];
    for suffix in http_suffixes {
        if receiver.ends_with(suffix) {
            return true;
        }
    }

    false
}

/// Check if a call looks like a route handler (e.g., app.get('/path', handler))
/// rather than an HTTP client call.
fn is_route_handler(parsed: &ParsedFile, node: &tree_sitter::Node) -> bool {
    let func_node = match node.child_by_field_name("function") {
        Some(f) => f,
        None => return false,
    };
    let callee = parsed.text_for_node(&func_node);
    let receiver = callee
        .rsplit_once('.')
        .map(|(r, _)| r.to_lowercase())
        .unwrap_or_default();

    // Avoid filtering client calls like `api.get('/v1/health')`.
    // Only treat as route handler for typical server/router receivers.
    let looks_like_router = receiver == "app"
        || receiver.contains("router")
        || receiver.contains("express")
        || receiver.contains("fastify")
        || receiver.contains("server");
    if !looks_like_router {
        return false;
    }

    // Route handlers typically have a string path as the first argument starting with '/'
    if let Some(args_node) = node.child_by_field_name("arguments") {
        if let Some(first_arg) = args_node.named_child(0) {
            let arg_text = parsed.text_for_node(&first_arg);
            let trimmed = arg_text.trim_matches(|c| c == '\'' || c == '"' || c == '`');
            // If first argument is a route path, this is a route handler not an HTTP call
            if trimmed.starts_with('/') {
                return true;
            }
        }
    }
    false
}

fn extract_timeout(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    callee: &str,
    client_kind: HttpClientKind,
) -> (bool, Option<f64>) {
    let args_node = match node.child_by_field_name("arguments") {
        Some(a) => a,
        None => return (false, None),
    };

    // Look for a config/options object literal among arguments.
    // - axios.get(url, { timeout: 5000 }) => milliseconds
    // - got(url, { timeout: { request: 5000 } }) => milliseconds
    // - fetch(url, { signal: AbortSignal.timeout(5000) }) => milliseconds
    for i in 0..args_node.named_child_count() {
        let arg = match args_node.named_child(i) {
            Some(a) => a,
            None => continue,
        };
        if arg.kind() != "object" {
            continue;
        }

        if let Some((has, secs)) =
            extract_timeout_from_object_literal(parsed, &arg, callee, client_kind)
        {
            return (has, secs);
        }
    }

    (false, None)
}

fn extract_timeout_from_object_literal(
    parsed: &ParsedFile,
    obj: &tree_sitter::Node,
    callee: &str,
    client_kind: HttpClientKind,
) -> Option<(bool, Option<f64>)> {
    // tree-sitter TS object: (object (pair key value) ...)
    let mut cursor = obj.walk();
    for child in obj.named_children(&mut cursor) {
        if child.kind() != "pair" {
            continue;
        }
        let key = child.child_by_field_name("key")?;
        let value = child.child_by_field_name("value")?;

        let key_text = parsed
            .text_for_node(&key)
            .trim()
            .trim_matches(|c| c == '\'' || c == '"')
            .to_string();

        if key_text == "timeout" {
            // axios/ky: timeout: 5000 (ms)
            if value.kind() == "number" {
                if let Ok(ms) = parsed.text_for_node(&value).trim().parse::<f64>() {
                    return Some((true, Some(ms / 1000.0)));
                }
                return Some((true, None));
            }

            // got: timeout: { request: 5000 }
            if value.kind() == "object" {
                if let Some(ms) = extract_got_request_timeout_ms(parsed, &value) {
                    return Some((true, Some(ms / 1000.0)));
                }
                return Some((true, None));
            }

            return Some((true, None));
        }

        if key_text == "signal" && (callee == "fetch" || client_kind == HttpClientKind::Fetch) {
            // fetch(url, { signal: AbortSignal.timeout(5000) })
            if value.kind() == "call_expression" {
                if let Some(ms) = extract_abort_signal_timeout_ms(parsed, &value) {
                    return Some((true, Some(ms / 1000.0)));
                }
                return Some((true, None));
            }
        }
    }

    None
}

fn extract_got_request_timeout_ms(parsed: &ParsedFile, obj: &tree_sitter::Node) -> Option<f64> {
    let mut cursor = obj.walk();
    for child in obj.named_children(&mut cursor) {
        if child.kind() != "pair" {
            continue;
        }
        let key = child.child_by_field_name("key")?;
        let value = child.child_by_field_name("value")?;
        let key_text = parsed
            .text_for_node(&key)
            .trim()
            .trim_matches(|c| c == '\'' || c == '"')
            .to_string();
        if key_text != "request" {
            continue;
        }
        if value.kind() != "number" {
            return None;
        }
        return parsed.text_for_node(&value).trim().parse::<f64>().ok();
    }
    None
}

fn extract_abort_signal_timeout_ms(parsed: &ParsedFile, call: &tree_sitter::Node) -> Option<f64> {
    let func = call.child_by_field_name("function")?;
    let callee_norm = normalize_ts_chain(parsed.text_for_node(&func).as_str());
    if callee_norm != "AbortSignal.timeout" {
        return None;
    }
    let args = call.child_by_field_name("arguments")?;
    let first = args.named_child(0)?;
    if first.kind() != "number" {
        return None;
    }
    parsed.text_for_node(&first).trim().parse::<f64>().ok()
}

fn check_error_handling(node: &tree_sitter::Node) -> bool {
    // Check if call is in a try block
    let mut current = Some(*node);
    while let Some(n) = current {
        if n.kind() == "try_statement" {
            return true;
        }
        current = n.parent();
    }

    // Check for .catch() chaining
    if let Some(parent) = node.parent() {
        if parent.kind() == "member_expression" {
            if let Some(grandparent) = parent.parent() {
                if grandparent.kind() == "call_expression" {
                    // This is chained, could be .catch()
                    return true;
                }
            }
        }
    }

    false
}

fn check_retry(parsed: &ParsedFile, node: &tree_sitter::Node) -> bool {
    if let Some(args_node) = node.child_by_field_name("arguments") {
        let args_text = parsed.text_for_node(&args_node);

        // Check for retry configuration
        if args_text.contains("retry") || args_text.contains("retries") {
            return true;
        }
    }

    false
}

fn extract_retry(parsed: &ParsedFile, node: &tree_sitter::Node) -> bool {
    let args_node = match node.child_by_field_name("arguments") {
        Some(a) => a,
        None => return false,
    };

    for i in 0..args_node.named_child_count() {
        let arg = match args_node.named_child(i) {
            Some(a) => a,
            None => continue,
        };
        if arg.kind() != "object" {
            continue;
        }
        if extract_retry_from_object(parsed, &arg) {
            return true;
        }
    }

    check_retry(parsed, node)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::typescript::parse_typescript_file;
    use crate::types::context::{Language, SourceFile};

    fn parse_and_summarize(source: &str) -> Vec<HttpCallSite> {
        let sf = SourceFile {
            path: "test.ts".to_string(),
            language: Language::Typescript,
            content: source.to_string(),
        };
        let parsed = parse_typescript_file(FileId(1), &sf).expect("parsing should succeed");
        summarize_http_clients(&parsed)
    }

    #[test]
    fn detects_fetch_call() {
        let calls = parse_and_summarize("fetch('https://api.example.com');");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].client_kind, HttpClientKind::Fetch);
    }

    #[test]
    fn extracts_url_literal_for_fetch() {
        let calls = parse_and_summarize("fetch('https://api.example.com');");
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].url_literal,
            Some("https://api.example.com".to_string())
        );
        assert!(calls[0].url_expr.is_none());
        assert_eq!(calls[0].url, calls[0].url_literal);
    }

    #[test]
    fn extracts_process_env_url_expr_member() {
        let calls = parse_and_summarize("fetch(process.env.API_URL);");
        assert_eq!(calls.len(), 1);
        assert!(calls[0].url_literal.is_none());
        assert!(calls[0].url.is_none());
        let expr = calls[0]
            .url_expr
            .clone()
            .expect("url_expr should be present");
        assert_eq!(expr.kind, HttpUrlExprKind::Member);
        assert_eq!(expr.env_var, Some("API_URL".to_string()));
    }

    #[test]
    fn extracts_process_env_url_expr_subscript() {
        let calls = parse_and_summarize("fetch(process.env['API_URL']);");
        assert_eq!(calls.len(), 1);
        let expr = calls[0]
            .url_expr
            .clone()
            .expect("url_expr should be present");
        assert_eq!(expr.kind, HttpUrlExprKind::Member);
        assert_eq!(expr.env_var, Some("API_URL".to_string()));
    }

    #[test]
    fn extracts_import_meta_env_url_expr_member() {
        let calls = parse_and_summarize("fetch(import.meta.env.VITE_API_URL);");
        assert_eq!(calls.len(), 1);
        let expr = calls[0]
            .url_expr
            .clone()
            .expect("url_expr should be present");
        assert_eq!(expr.env_var, Some("VITE_API_URL".to_string()));
    }

    #[test]
    fn extracts_bun_env_url_expr_member() {
        let calls = parse_and_summarize("fetch(Bun.env.API_URL);");
        assert_eq!(calls.len(), 1);
        let expr = calls[0]
            .url_expr
            .clone()
            .expect("url_expr should be present");
        assert_eq!(expr.env_var, Some("API_URL".to_string()));
    }

    #[test]
    fn extracts_deno_env_get_call() {
        let calls = parse_and_summarize("fetch(Deno.env.get('API_URL'));");
        assert_eq!(calls.len(), 1);
        let expr = calls[0]
            .url_expr
            .clone()
            .expect("url_expr should be present");
        assert_eq!(expr.env_var, Some("API_URL".to_string()));
    }

    #[test]
    fn extracts_template_literal_with_substitution_as_expr() {
        let calls = parse_and_summarize("fetch(`${baseUrl}/v1`);");
        assert_eq!(calls.len(), 1);
        assert!(calls[0].url_literal.is_none());
        let expr = calls[0]
            .url_expr
            .clone()
            .expect("url_expr should be present");
        assert_eq!(expr.kind, HttpUrlExprKind::Template);
    }

    #[test]
    fn detects_axios_get() {
        let calls = parse_and_summarize("axios.get('https://api.example.com');");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].client_kind, HttpClientKind::Axios);
        assert_eq!(calls[0].method, "get");
    }

    #[test]
    fn detects_axios_post() {
        let calls = parse_and_summarize("axios.post('https://api.example.com', { data: 'test' });");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].client_kind, HttpClientKind::Axios);
        assert_eq!(calls[0].method, "post");
    }

    #[test]
    fn detects_timeout_in_fetch() {
        let src = r#"
fetch('https://api.example.com', { signal: AbortSignal.timeout(5000) });
"#;
        let calls = parse_and_summarize(src);
        assert_eq!(calls.len(), 1);
        assert!(calls[0].has_timeout);
        assert_eq!(calls[0].timeout_value, Some(5.0));
    }

    #[test]
    fn detects_timeout_in_axios() {
        let src = r#"
axios.get('https://api.example.com', { timeout: 5000 });
"#;
        let calls = parse_and_summarize(src);
        assert_eq!(calls.len(), 1);
        assert!(calls[0].has_timeout);
        assert_eq!(calls[0].timeout_value, Some(5.0));
    }

    #[test]
    fn axios_instance_base_url_and_default_timeout_applied() {
        let src = r#"
import axios from 'axios';
const api = axios.create({ baseURL: 'https://api.example.com', timeout: 2500 });
api.get('/v1/health');
"#;
        let calls = parse_and_summarize(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].client_kind, HttpClientKind::Axios);
        assert_eq!(
            calls[0].url_literal,
            Some("https://api.example.com/v1/health".to_string())
        );
        assert!(calls[0].has_timeout);
        assert_eq!(calls[0].timeout_value, Some(2.5));
    }

    #[test]
    fn got_instance_prefix_url_applied() {
        let src = r#"
import got from 'got';
const api = got.extend({ prefixUrl: 'https://api.example.com', timeout: { request: 5000 } });
api.get('v1/health');
"#;
        let calls = parse_and_summarize(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].client_kind, HttpClientKind::Got);
        assert_eq!(
            calls[0].url_literal,
            Some("https://api.example.com/v1/health".to_string())
        );
        assert!(calls[0].has_timeout);
        assert_eq!(calls[0].timeout_value, Some(5.0));
    }

    #[test]
    fn detects_in_loop_context() {
        let src = r#"
async function f() {
  for (let i = 0; i < 3; i++) {
    await fetch('https://api.example.com');
  }
}
"#;
        let calls = parse_and_summarize(src);
        assert_eq!(calls.len(), 1);
        assert!(calls[0].in_loop);
    }

    #[test]
    fn detects_no_timeout() {
        let calls = parse_and_summarize("fetch('https://api.example.com');");
        assert_eq!(calls.len(), 1);
        assert!(!calls[0].has_timeout);
    }

    #[test]
    fn detects_async_context() {
        let src = r#"
async function fetchData() {
    const response = await fetch('https://api.example.com');
}
"#;
        let calls = parse_and_summarize(src);
        assert_eq!(calls.len(), 1);
        assert!(calls[0].in_async_context);
    }

    #[test]
    fn detects_error_handling_with_try_catch() {
        let src = r#"
try {
    fetch('https://api.example.com');
} catch (e) {
    console.error(e);
}
"#;
        let calls = parse_and_summarize(src);
        assert_eq!(calls.len(), 1);
        assert!(calls[0].has_error_handling);
    }

    #[test]
    fn detects_got_client() {
        let calls = parse_and_summarize("got.get('https://api.example.com');");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].client_kind, HttpClientKind::Got);
    }

    #[test]
    fn detects_node_http() {
        let calls = parse_and_summarize("http.get('https://api.example.com');");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].client_kind, HttpClientKind::NodeHttp);
    }

    #[test]
    fn ignores_config_get() {
        // config.get() is not an HTTP call - it's a configuration getter
        let src = r#"
const config = vscode.workspace.getConfiguration("unfault");
config.get<boolean>("enable", true);
"#;
        let calls = parse_and_summarize(src);
        assert!(
            calls.is_empty(),
            "config.get should not be detected as HTTP call. Found: {:?}",
            calls
        );
    }

    #[test]
    fn ignores_map_get() {
        // Map.get() is not an HTTP call
        let src = r#"
const map = new Map();
map.get("key");
"#;
        let calls = parse_and_summarize(src);
        assert!(
            calls.is_empty(),
            "map.get should not be detected as HTTP call. Found: {:?}",
            calls
        );
    }

    #[test]
    fn detects_http_client_get() {
        // httpClient.get() should be detected as HTTP call
        let calls = parse_and_summarize("httpClient.get('https://api.example.com');");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].client_kind, HttpClientKind::Unknown);
        assert_eq!(calls[0].method, "get");
    }

    #[test]
    fn detects_api_client_post() {
        // apiClient.post() should be detected as HTTP call
        let calls = parse_and_summarize("apiClient.post('https://api.example.com', data);");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].client_kind, HttpClientKind::Unknown);
        assert_eq!(calls[0].method, "post");
    }
}
