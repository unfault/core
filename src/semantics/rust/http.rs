//! HTTP client abstractions for Rust.
//!
//! This module provides analysis of HTTP client calls in Rust code,
//! detecting patterns using reqwest, ureq, hyper, and other libraries.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::parse::ast::{AstLocation, ParsedFile};

use crate::semantics::common::http::{HttpUrlExpr, HttpUrlExprKind, RetryMechanism};

/// Rust HTTP client library classification
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HttpClientKind {
    /// reqwest - the most popular async HTTP client for Rust
    Reqwest,
    /// ureq - simple synchronous HTTP client
    Ureq,
    /// hyper - low-level HTTP library
    Hyper,
    /// surf - async HTTP client
    Surf,
    /// awc - Actix Web Client
    Awc,
    /// isahc - async HTTP client
    Isahc,
    /// Reqwest blocking (using `blocking` feature)
    ReqwestBlocking,
    /// Other/unknown HTTP client
    Other(String),
}

/// A single HTTP client call in Rust code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpCallSite {
    /// Library being used
    pub client_kind: HttpClientKind,

    /// HTTP method name (get, post, put, etc.)
    pub method_name: String,

    /// URL if statically determinable as a literal.
    pub url_literal: Option<String>,

    /// URL expression metadata when the URL is not a literal.
    pub url_expr: Option<HttpUrlExpr>,

    /// Full text of the call expression
    pub call_text: String,

    /// Whether this call has an explicit timeout configured
    pub has_timeout: bool,

    /// Timeout value in seconds if detectable
    pub timeout_value: Option<f64>,

    /// Retry mechanism detected for this call
    #[serde(default)]
    pub retry_mechanism: Option<RetryMechanism>,

    /// Location in source
    pub location: AstLocation,

    /// Name of enclosing function, if known
    pub function_name: Option<String>,

    /// Whether the enclosing function is async
    pub in_async_function: bool,

    /// Whether this call uses `.await`
    pub has_await: bool,

    /// Whether this call is inside a loop
    #[serde(default)]
    pub in_loop: bool,

    /// Byte range in original source
    pub start_byte: usize,
    pub end_byte: usize,
}

/// Build a list of HTTP client calls in this Rust file.
pub fn summarize_http_clients(file: &ParsedFile) -> Vec<HttpCallSite> {
    let root = file.tree.root_node();
    let const_string_bindings = collect_string_const_bindings(file, root);
    let loop_infos = collect_loop_infos(file, root);
    let retry_client_vars = collect_retry_client_vars(file, root);
    let mut calls = Vec::new();
    collect_http_calls(
        file,
        root,
        &mut calls,
        None,
        false,
        None,
        &const_string_bindings,
    );

    // Post-process loop context and best-effort retry detection.
    for call in &mut calls {
        if let Some(loop_info) =
            innermost_loop_containing(&loop_infos, call.start_byte, call.end_byte)
        {
            call.in_loop = true;
            if call.retry_mechanism.is_none() && loop_info.has_sleep {
                call.retry_mechanism = Some(RetryMechanism::ManualLoop);
            }
        }

        if call.retry_mechanism.is_none() {
            if let Some(recv) = extract_receiver_ident_from_call_text(&call.call_text) {
                if let Some(mech) = retry_client_vars.get(&recv) {
                    call.retry_mechanism = Some(mech.clone());
                }
            }
        }
    }

    calls
}

#[derive(Debug, Clone)]
struct LoopInfo {
    start_byte: usize,
    end_byte: usize,
    has_sleep: bool,
}

fn collect_loop_infos(file: &ParsedFile, root: tree_sitter::Node) -> Vec<LoopInfo> {
    let mut out = Vec::new();

    fn walk(file: &ParsedFile, node: tree_sitter::Node, out: &mut Vec<LoopInfo>) {
        if super::is_inline_test_subtree_root(file, &node) {
            return;
        }

        if matches!(
            node.kind(),
            "for_expression" | "while_expression" | "loop_expression"
        ) {
            let text = file.text_for_node(&node);
            let has_sleep = text.contains("sleep(")
                || text.contains("tokio::time::sleep")
                || text.contains("std::thread::sleep");
            let r = node.byte_range();
            out.push(LoopInfo {
                start_byte: r.start,
                end_byte: r.end,
                has_sleep,
            });
        }

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                walk(file, child, out);
            }
        }
    }

    walk(file, root, &mut out);
    out
}

fn innermost_loop_containing(loops: &[LoopInfo], start: usize, end: usize) -> Option<&LoopInfo> {
    loops
        .iter()
        .filter(|l| l.start_byte <= start && end <= l.end_byte)
        .min_by_key(|l| l.end_byte - l.start_byte)
}

fn collect_retry_client_vars(
    file: &ParsedFile,
    root: tree_sitter::Node,
) -> HashMap<String, RetryMechanism> {
    let mut out: HashMap<String, RetryMechanism> = HashMap::new();

    fn walk(file: &ParsedFile, node: tree_sitter::Node, out: &mut HashMap<String, RetryMechanism>) {
        if super::is_inline_test_subtree_root(file, &node) {
            return;
        }

        if node.kind() == "let_declaration" {
            // best-effort: `let client = ...;`
            let pat = node.child_by_field_name("pattern");
            let value = node.child_by_field_name("value");
            if let (Some(pat), Some(value)) = (pat, value) {
                if pat.kind() == "identifier" {
                    let name = file.text_for_node(&pat);
                    let v = file.text_for_node(&value);
                    if v.contains("ClientBuilder::new")
                        && (v.contains("Retry")
                            || v.contains("reqwest_retry")
                            || v.contains("retry"))
                    {
                        out.insert(
                            name,
                            RetryMechanism::Middleware("reqwest-middleware".to_string()),
                        );
                    } else if v.contains("tower::retry") || v.contains("RetryLayer") {
                        out.insert(name, RetryMechanism::Middleware("tower".to_string()));
                    }
                }
            }
        }

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                walk(file, child, out);
            }
        }
    }

    walk(file, root, &mut out);
    out
}

fn extract_receiver_ident_from_call_text(call_text: &str) -> Option<String> {
    let t = call_text.trim();
    if t.starts_with("self.") {
        let rest = &t["self.".len()..];
        let ident: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if !ident.is_empty() {
            return Some(ident);
        }
    }

    let ident: String = t
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if ident.is_empty() {
        return None;
    }
    Some(ident)
}

/// Context for tracking during AST traversal
#[derive(Default, Clone)]
struct HttpCallContext {
    current_function: Option<String>,
    in_async_fn: bool,
}

fn collect_http_calls(
    file: &ParsedFile,
    node: tree_sitter::Node,
    out: &mut Vec<HttpCallSite>,
    ctx: Option<HttpCallContext>,
    has_await: bool,
    _parent_fn: Option<String>,
    const_string_bindings: &HashMap<String, String>,
) {
    let ctx = ctx.unwrap_or_default();

    if super::is_inline_test_subtree_root(file, &node) {
        return;
    }

    if node.kind() == "function_item" {
        let fn_text = file.text_for_node(&node);
        let is_async = fn_text.contains("async fn");
        let name = node
            .child_by_field_name("name")
            .map(|n| file.text_for_node(&n));

        let mut new_ctx = ctx.clone();
        new_ctx.current_function = name;
        new_ctx.in_async_fn = is_async;

        walk_http_calls(file, node, out, &new_ctx, false, const_string_bindings);
        return;
    }

    if node.kind() == "await_expression" {
        walk_http_calls(file, node, out, &ctx, true, const_string_bindings);
        return;
    }

    walk_http_calls(file, node, out, &ctx, has_await, const_string_bindings);
}

fn walk_http_calls(
    file: &ParsedFile,
    node: tree_sitter::Node,
    out: &mut Vec<HttpCallSite>,
    ctx: &HttpCallContext,
    has_await: bool,
    const_string_bindings: &HashMap<String, String>,
) {
    if super::is_inline_test_subtree_root(file, &node) {
        return;
    }

    if node.kind() == "call_expression" {
        if let Some(call) = extract_http_call(file, &node, ctx, has_await, const_string_bindings) {
            out.push(call);
            return;
        }
    }

    if node.kind() == "function_item" {
        let fn_text = file.text_for_node(&node);
        let is_async = fn_text.contains("async fn");
        let name = node
            .child_by_field_name("name")
            .map(|n| file.text_for_node(&n));

        let mut new_ctx = ctx.clone();
        new_ctx.current_function = name;
        new_ctx.in_async_fn = is_async;

        let child_count = node.child_count();
        for i in 0..child_count {
            if let Some(child) = node.child(i) {
                walk_http_calls(file, child, out, &new_ctx, false, const_string_bindings);
            }
        }
        return;
    }

    if node.kind() == "await_expression" {
        let child_count = node.child_count();
        for i in 0..child_count {
            if let Some(child) = node.child(i) {
                walk_http_calls(file, child, out, ctx, true, const_string_bindings);
            }
        }
        return;
    }

    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            walk_http_calls(file, child, out, ctx, has_await, const_string_bindings);
        }
    }
}

/// Extract an HTTP call from a call_expression node
fn extract_http_call(
    file: &ParsedFile,
    node: &tree_sitter::Node,
    ctx: &HttpCallContext,
    has_await: bool,
    const_string_bindings: &HashMap<String, String>,
) -> Option<HttpCallSite> {
    let func_node = node.child_by_field_name("function")?;
    let callee_expr = file.text_for_node(&func_node);

    if func_node.kind() == "field_expression" {
        let value_node = func_node.child_by_field_name("value")?;
        let field_node = func_node.child_by_field_name("field")?;

        let method_name = file.text_for_node(&field_node);
        let call_text = file.text_for_node(node);
        let location = file.location_for_node(node);
        let byte_range = node.byte_range();

        let (http_method, client_kind, url_literal, url_expr) =
            if value_node.kind() == "call_expression" {
                extract_http_method_client_and_url(file, &value_node, const_string_bindings)
            } else {
                let object = file.text_for_node(&value_node);
                let client = detect_client_kind(&object, &callee_expr)?;
                let (url_literal, url_expr) =
                    extract_url_from_first_arg(file, node, const_string_bindings);
                (method_name.clone(), client, url_literal, url_expr)
            };

        if http_method.is_empty() {
            return None;
        }

        let (has_timeout, timeout_value) = detect_timeout_in_chain(file, node);

        return Some(HttpCallSite {
            client_kind,
            method_name: http_method,
            url_literal,
            url_expr,
            call_text,
            has_timeout,
            timeout_value,
            retry_mechanism: None,
            location,
            function_name: ctx.current_function.clone(),
            in_async_function: ctx.in_async_fn,
            has_await,
            in_loop: false,
            start_byte: byte_range.start,
            end_byte: byte_range.end,
        });
    }

    if func_node.kind() == "path_expression" || func_node.kind() == "scoped_identifier" {
        let path_text = file.text_for_node(&func_node);

        if path_text.contains("reqwest::blocking::") {
            let method_name = extract_method_from_blocking_call(&path_text);
            let call_text = file.text_for_node(node);
            let location = file.location_for_node(node);
            let byte_range = node.byte_range();
            let (has_timeout, timeout_value) = detect_timeout(&call_text);

            let (url_literal, url_expr) =
                extract_url_from_first_arg(file, node, const_string_bindings);

            return Some(HttpCallSite {
                client_kind: HttpClientKind::ReqwestBlocking,
                method_name,
                url_literal,
                url_expr,
                call_text,
                has_timeout,
                timeout_value,
                retry_mechanism: None,
                location,
                function_name: ctx.current_function.clone(),
                in_async_function: false,
                has_await,
                in_loop: false,
                start_byte: byte_range.start,
                end_byte: byte_range.end,
            });
        }

        if path_text.starts_with("ureq::") {
            let method_name = path_text
                .strip_prefix("ureq::")
                .and_then(|s| s.split('(').next())
                .map(|s| s.to_string())
                .unwrap_or_else(|| path_text.to_string());

            if is_http_method(&method_name) {
                let call_text = file.text_for_node(node);
                let location = file.location_for_node(node);
                let byte_range = node.byte_range();
                let (has_timeout, timeout_value) = detect_timeout(&call_text);

                let (url_literal, url_expr) =
                    extract_url_from_first_arg(file, node, const_string_bindings);

                return Some(HttpCallSite {
                    client_kind: HttpClientKind::Ureq,
                    method_name,
                    url_literal,
                    url_expr,
                    call_text,
                    has_timeout,
                    timeout_value,
                    retry_mechanism: None,
                    location,
                    function_name: ctx.current_function.clone(),
                    in_async_function: false,
                    has_await,
                    in_loop: false,
                    start_byte: byte_range.start,
                    end_byte: byte_range.end,
                });
            }
        }
    }

    None
}

fn collect_string_const_bindings(
    file: &ParsedFile,
    root: tree_sitter::Node,
) -> HashMap<String, String> {
    let mut out: HashMap<String, String> = HashMap::new();

    fn walk(file: &ParsedFile, node: tree_sitter::Node, out: &mut HashMap<String, String>) {
        if node.kind() == "const_item" {
            let name = node
                .child_by_field_name("name")
                .map(|n| file.text_for_node(&n));
            let value_node = node.child_by_field_name("value");
            let mut value = value_node.and_then(|n| extract_string_literal_rust(file, &n));

            // Fallback: scan for a direct string literal under the const item.
            if value.is_none() {
                value = first_string_literal_in_subtree(file, node);
            }

            if let (Some(k), Some(v)) = (name, value) {
                out.insert(k, v);
            }
        }

        let child_count = node.child_count();
        for i in 0..child_count {
            if let Some(child) = node.child(i) {
                walk(file, child, out);
            }
        }
    }

    walk(file, root, &mut out);
    out
}

fn extract_url_from_first_arg(
    file: &ParsedFile,
    call: &tree_sitter::Node,
    const_string_bindings: &HashMap<String, String>,
) -> (Option<String>, Option<HttpUrlExpr>) {
    let args = match call.child_by_field_name("arguments") {
        Some(a) => a,
        None => return (None, None),
    };
    let first = match args.named_child(0) {
        Some(a) => a,
        None => return (None, None),
    };
    extract_url_from_expr(file, &first, const_string_bindings)
}

fn extract_url_from_expr(
    file: &ParsedFile,
    expr: &tree_sitter::Node,
    const_string_bindings: &HashMap<String, String>,
) -> (Option<String>, Option<HttpUrlExpr>) {
    // Unwrap references and parentheses.
    if expr.kind() == "reference_expression" {
        if let Some(v) = expr.child_by_field_name("value") {
            return extract_url_from_expr(file, &v, const_string_bindings);
        }
    }
    if expr.kind() == "parenthesized_expression" {
        // First named child should be the inner expression.
        if let Some(inner) = expr.named_child(0) {
            return extract_url_from_expr(file, &inner, const_string_bindings);
        }
    }

    if let Some(lit) = extract_string_literal_rust(file, expr) {
        return (Some(lit), None);
    }

    let env_var = detect_env_var_name_rust_deep(file, *expr, const_string_bindings);
    let kind = match expr.kind() {
        "identifier" => HttpUrlExprKind::Identifier,
        "field_expression" => HttpUrlExprKind::Member,
        "call_expression" => HttpUrlExprKind::Call,
        "macro_invocation" => macro_invocation_kind(file, expr),
        _ => HttpUrlExprKind::Unknown,
    };

    let text = file.text_for_node(expr).trim().to_string();
    (
        None,
        Some(HttpUrlExpr {
            text,
            kind,
            env_var,
        }),
    )
}

fn macro_invocation_kind(file: &ParsedFile, node: &tree_sitter::Node) -> HttpUrlExprKind {
    let name = node
        .child_by_field_name("macro")
        .map(|n| file.text_for_node(&n))
        .unwrap_or_default();
    match name.as_str() {
        "format" | "format_args" | "concat" => HttpUrlExprKind::Template,
        _ => HttpUrlExprKind::Call,
    }
}

fn extract_string_literal_rust(file: &ParsedFile, node: &tree_sitter::Node) -> Option<String> {
    // tree-sitter-rust uses string_literal and raw_string_literal.
    if node.kind() != "string_literal" && node.kind() != "raw_string_literal" {
        return None;
    }

    let raw = file.text_for_node(node);
    let text = raw.trim();
    if node.kind() == "string_literal" {
        return Some(text.trim_matches('"').to_string());
    }

    // Raw string literals look like: r"...", r#"..."#, r##"..."##, ...
    let mut s = text;
    if let Some(rest) = s.strip_prefix('r') {
        s = rest;
    }
    let hash_count = s.chars().take_while(|c| *c == '#').count();
    s = &s[hash_count..];
    s = s.strip_prefix('"').unwrap_or(s);

    // Strip trailing hashes then the closing quote.
    let suffix = "#".repeat(hash_count);
    if hash_count > 0 {
        if let Some(rest) = s.strip_suffix(&suffix) {
            s = rest;
        }
    }
    s = s.strip_suffix('"').unwrap_or(s);

    Some(s.to_string())
}

fn first_string_literal_in_subtree(file: &ParsedFile, node: tree_sitter::Node) -> Option<String> {
    if let Some(lit) = extract_string_literal_rust(file, &node) {
        return Some(lit);
    }
    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            if let Some(lit) = first_string_literal_in_subtree(file, child) {
                return Some(lit);
            }
        }
    }
    None
}

fn detect_env_var_name_rust_deep(
    file: &ParsedFile,
    node: tree_sitter::Node,
    const_string_bindings: &HashMap<String, String>,
) -> Option<String> {
    if let Some(v) = detect_env_var_name_rust(file, node, const_string_bindings) {
        return Some(v);
    }
    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            if let Some(v) = detect_env_var_name_rust_deep(file, child, const_string_bindings) {
                return Some(v);
            }
        }
    }
    None
}

fn detect_env_var_name_rust(
    file: &ParsedFile,
    node: tree_sitter::Node,
    const_string_bindings: &HashMap<String, String>,
) -> Option<String> {
    // env!("FOO") / option_env!("FOO")
    if node.kind() == "macro_invocation" {
        let name = node
            .child_by_field_name("macro")
            .map(|n| file.text_for_node(&n))
            .unwrap_or_default();
        if name == "env" || name == "option_env" {
            return first_string_literal_in_subtree(file, node);
        }
    }

    if node.kind() != "call_expression" {
        return None;
    }

    let func_node = node.child_by_field_name("function")?;
    let callee = file.text_for_node(&func_node);
    let callee_norm: String = callee.chars().filter(|c| !c.is_whitespace()).collect();

    let is_env_var_call = callee_norm.ends_with("env::var")
        || callee_norm.ends_with("env::var_os")
        || callee_norm.ends_with("dotenv::var")
        || callee_norm.ends_with("dotenvy::var")
        || callee_norm.ends_with("dotenv::var_os")
        || callee_norm.ends_with("dotenvy::var_os");
    if !is_env_var_call {
        return None;
    }

    let args = node.child_by_field_name("arguments")?;
    let key_expr = args.named_child(0)?;
    if let Some(k) = extract_string_literal_rust(file, &key_expr) {
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

fn extract_http_method_client_and_url(
    file: &ParsedFile,
    node: &tree_sitter::Node,
    const_string_bindings: &HashMap<String, String>,
) -> (String, HttpClientKind, Option<String>, Option<HttpUrlExpr>) {
    let func_node = node.child_by_field_name("function");
    if func_node.is_none() {
        return (
            "".to_string(),
            HttpClientKind::Other("unknown".to_string()),
            None,
            None,
        );
    }
    let func_node = func_node.unwrap();

    if func_node.kind() == "field_expression" {
        let value_node = func_node.child_by_field_name("value");
        let field_node = func_node.child_by_field_name("field");
        if value_node.is_none() || field_node.is_none() {
            return (
                "".to_string(),
                HttpClientKind::Other("unknown".to_string()),
                None,
                None,
            );
        }

        let inner_method = file.text_for_node(&field_node.unwrap());
        let callee_expr = file.text_for_node(&func_node);
        let object = file.text_for_node(&value_node.unwrap());

        let client_kind = detect_client_kind(&object, &callee_expr);
        if let Some(kind) = client_kind {
            if is_http_method(&inner_method) {
                let (url_literal, url_expr) =
                    extract_url_from_first_arg(file, node, const_string_bindings);
                return (inner_method, kind, url_literal, url_expr);
            }
            if value_node.unwrap().kind() == "call_expression" {
                return extract_http_method_client_and_url(
                    file,
                    &value_node.unwrap(),
                    const_string_bindings,
                );
            }
            let (url_literal, url_expr) =
                extract_url_from_first_arg(file, node, const_string_bindings);
            return (inner_method, kind, url_literal, url_expr);
        }

        if value_node.unwrap().kind() == "call_expression" {
            return extract_http_method_client_and_url(
                file,
                &value_node.unwrap(),
                const_string_bindings,
            );
        }
    }

    if func_node.kind() == "path_expression" || func_node.kind() == "scoped_identifier" {
        let path_text = file.text_for_node(&func_node);

        if path_text.contains("reqwest::blocking::") {
            let method_name = extract_method_from_blocking_call(&path_text);
            let (url_literal, url_expr) =
                extract_url_from_first_arg(file, node, const_string_bindings);
            return (
                method_name,
                HttpClientKind::ReqwestBlocking,
                url_literal,
                url_expr,
            );
        }

        if path_text.starts_with("ureq::") {
            let method_name = path_text
                .strip_prefix("ureq::")
                .and_then(|s| s.split('(').next())
                .map(|s| s.to_string())
                .unwrap_or_else(|| path_text.to_string());
            let (url_literal, url_expr) =
                extract_url_from_first_arg(file, node, const_string_bindings);
            return (method_name, HttpClientKind::Ureq, url_literal, url_expr);
        }
    }

    (
        "".to_string(),
        HttpClientKind::Other("unknown".to_string()),
        None,
        None,
    )
}

/// Detect the HTTP client library from the object expression
fn detect_client_kind(object: &str, callee_expr: &str) -> Option<HttpClientKind> {
    // Reqwest patterns
    if object == "reqwest"
        || callee_expr.contains("reqwest::Client")
        || callee_expr.contains("reqwest::blocking::Client")
        || object.contains("client")
    {
        // Check if it's the blocking API
        if callee_expr.contains("blocking::") {
            return Some(HttpClientKind::ReqwestBlocking);
        }
        return Some(HttpClientKind::Reqwest);
    }

    // Ureq patterns
    if object == "ureq" || callee_expr.starts_with("ureq::") {
        return Some(HttpClientKind::Ureq);
    }

    // Hyper patterns
    if object == "hyper" || callee_expr.contains("hyper::") {
        return Some(HttpClientKind::Hyper);
    }

    // Surf patterns
    if object == "surf" || callee_expr.contains("surf::") {
        return Some(HttpClientKind::Surf);
    }

    // Awc patterns (Actix Web Client)
    if callee_expr.contains("awc::") || object == "awc" {
        return Some(HttpClientKind::Awc);
    }

    // Isahc patterns
    if callee_expr.contains("isahc::") || object == "isahc" {
        return Some(HttpClientKind::Isahc);
    }

    None
}

/// Check if a string is an HTTP method name
fn is_http_method(s: &str) -> bool {
    matches!(
        s,
        "get" | "post" | "put" | "patch" | "delete" | "head" | "options"
    )
}

/// Extract method name from blocking call like `reqwest::blocking::get(...)`
fn extract_method_from_blocking_call(path: &str) -> String {
    if let Some(method) = path
        .rsplitn(2, "::")
        .nth(1)
        .and_then(|s| s.split('(').next())
    {
        if is_http_method(method) {
            return method.to_string();
        }
    }
    path.to_string()
}

/// Detect timeout configuration in function arguments
fn detect_timeout(args_text: &str) -> (bool, Option<f64>) {
    let timeout_patterns = [
        ".timeout(",
        "timeout(Duration::from_secs",
        "timeout(Duration::from_millis",
        "timeout=",
        "Duration::from_secs",
        "Duration::from_millis",
    ];

    for pattern in &timeout_patterns {
        if args_text.contains(pattern) {
            if let Some(value) = extract_timeout_value(args_text, pattern) {
                return (true, Some(value));
            }
            return (true, None);
        }
    }

    (false, None)
}

fn detect_timeout_in_chain(file: &ParsedFile, node: &tree_sitter::Node) -> (bool, Option<f64>) {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "field_expression" {
                let field_node = child.child_by_field_name("field");
                if let Some(field) = field_node {
                    let method_name = file.text_for_node(&field);
                    if method_name == "timeout" {
                        let args_node = node.child_by_field_name("arguments");
                        if let Some(args) = args_node {
                            let args_text = file.text_for_node(&args);
                            return detect_timeout(&args_text);
                        }
                    }
                    let value_node = child.child_by_field_name("value");
                    if let Some(value) = value_node {
                        if value.kind() == "call_expression" {
                            let result = detect_timeout_in_chain(file, &value);
                            if result.0 {
                                return result;
                            }
                        }
                    }
                }
            }
        }
    }
    (false, None)
}

fn extract_timeout_value(args_text: &str, pattern: &str) -> Option<f64> {
    if let Some(start) = args_text.find(pattern) {
        let after = &args_text[start + pattern.len()..];
        let after_stripped = after.trim_start_matches('(').trim_start();

        if let Some(end) = after_stripped.find(|c| c == ')' || c == ',') {
            let value_str = &after_stripped[..end];
            if let Ok(value) = value_str.trim().parse::<f64>() {
                if pattern.contains("millis") {
                    return Some(value / 1000.0);
                }
                return Some(value);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::rust::parse_rust_file;
    use crate::types::context::{Language, SourceFile};

    fn parse_and_summarize_http(source: &str) -> Vec<HttpCallSite> {
        let sf = SourceFile {
            path: "test.rs".to_string(),
            language: Language::Rust,
            content: source.to_string(),
        };
        let parsed = parse_rust_file(FileId(1), &sf).expect("parsing should succeed");
        summarize_http_clients(&parsed)
    }

    // ==================== Client Detection Tests ====================

    #[test]
    fn detects_reqwest_client() {
        let calls = parse_and_summarize_http("client.get(\"https://example.com\")");
        assert_eq!(calls.len(), 1);
        assert!(matches!(calls[0].client_kind, HttpClientKind::Reqwest));
    }

    #[test]
    fn detects_reqwest_blocking() {
        let calls = parse_and_summarize_http("reqwest::blocking::get(\"https://example.com\")");
        assert_eq!(calls.len(), 1);
        assert!(matches!(
            calls[0].client_kind,
            HttpClientKind::ReqwestBlocking
        ));
    }

    #[test]
    fn detects_ureq_get() {
        let calls = parse_and_summarize_http("ureq::get(\"https://example.com\")");
        assert_eq!(calls.len(), 1);
        assert!(matches!(calls[0].client_kind, HttpClientKind::Ureq));
        assert_eq!(calls[0].method_name, "get");
    }

    #[test]
    fn detects_ureq_post() {
        let calls = parse_and_summarize_http("ureq::post(\"https://example.com\")");
        assert_eq!(calls.len(), 1);
        assert!(matches!(calls[0].client_kind, HttpClientKind::Ureq));
        assert_eq!(calls[0].method_name, "post");
    }

    #[test]
    fn detects_ureq_put() {
        let calls = parse_and_summarize_http("ureq::put(\"https://example.com\")");
        assert_eq!(calls.len(), 1);
        assert!(matches!(calls[0].client_kind, HttpClientKind::Ureq));
        assert_eq!(calls[0].method_name, "put");
    }

    #[test]
    fn detects_ureq_delete() {
        let calls = parse_and_summarize_http("ureq::delete(\"https://example.com\")");
        assert_eq!(calls.len(), 1);
        assert!(matches!(calls[0].client_kind, HttpClientKind::Ureq));
        assert_eq!(calls[0].method_name, "delete");
    }

    // ==================== Method Name Tests ====================

    #[test]
    fn captures_get_method() {
        let calls = parse_and_summarize_http("client.get(\"https://example.com\")");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method_name, "get");
        assert_eq!(
            calls[0].url_literal,
            Some("https://example.com".to_string())
        );
        assert!(calls[0].url_expr.is_none());
    }

    #[test]
    fn extracts_env_var_from_std_env_var_literal() {
        let src = r#"
 async fn f() {
     client.get(std::env::var("API_URL").unwrap()).send().await;
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
    fn extracts_env_var_from_env_macro() {
        let calls = parse_and_summarize_http("client.get(env!(\"API_URL\"))");
        assert_eq!(calls.len(), 1);
        let expr = calls[0]
            .url_expr
            .clone()
            .expect("url_expr should be present");
        assert_eq!(expr.env_var, Some("API_URL".to_string()));
    }

    #[test]
    fn extracts_env_var_from_std_env_var_via_const_binding() {
        let src = r#"
 const API_URL_KEY: &str = "API_URL";

fn f() {
    client.get(std::env::var(API_URL_KEY).unwrap());
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
    fn captures_post_method() {
        let calls = parse_and_summarize_http("client.post(\"https://example.com\")");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method_name, "post");
    }

    #[test]
    fn captures_put_method() {
        let calls = parse_and_summarize_http("client.put(\"https://example.com\")");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method_name, "put");
    }

    #[test]
    fn captures_patch_method() {
        let calls = parse_and_summarize_http("client.patch(\"https://example.com\")");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method_name, "patch");
    }

    #[test]
    fn captures_delete_method() {
        let calls = parse_and_summarize_http("client.delete(\"https://example.com\")");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method_name, "delete");
    }

    // ==================== Timeout Detection Tests ====================

    #[test]
    fn detects_timeout_with_duration() {
        let calls = parse_and_summarize_http(
            r#"client.get("https://example.com")
                .timeout(Duration::from_secs(30))"#,
        );
        assert_eq!(calls.len(), 1);
        assert!(calls[0].has_timeout);
        assert_eq!(calls[0].timeout_value, Some(30.0));
    }

    #[test]
    fn detects_timeout_with_millis() {
        let calls = parse_and_summarize_http(
            r#"client.get("https://example.com")
                .timeout(Duration::from_millis(5000))"#,
        );
        assert_eq!(calls.len(), 1);
        assert!(calls[0].has_timeout);
        assert_eq!(calls[0].timeout_value, Some(5.0));
    }

    #[test]
    fn detects_missing_timeout() {
        let calls = parse_and_summarize_http("client.get(\"https://example.com\")");
        assert_eq!(calls.len(), 1);
        assert!(!calls[0].has_timeout);
    }

    // ==================== Function Context Tests ====================

    #[test]
    fn captures_enclosing_function_name() {
        let src = r#"
async fn fetch_data() -> Result<String, reqwest::Error> {
    client.get("https://example.com").send().await?.text().await
}
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, Some("fetch_data".to_string()));
        assert!(calls[0].in_async_function);
    }

    #[test]
    fn detects_await_on_call() {
        let src = r#"
async fn fetch_data() -> Result<String, reqwest::Error> {
    client.get("https://example.com").await?.text().await
}
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert!(calls[0].has_await);
    }

    #[test]
    fn module_level_call_has_no_function_name() {
        let calls = parse_and_summarize_http("let response = client.get(\"https://example.com\");");
        assert_eq!(calls.len(), 1);
        assert!(calls[0].function_name.is_none());
    }

    // ==================== Multiple Calls Tests ====================

    #[test]
    fn collects_multiple_http_calls() {
        let src = r#"
async fn fetch_all() {
    let a = client.get("https://example.com/a").await?;
    let b = client.post("https://example.com/b").await?;
    let c = client.delete("https://example.com/c").await?;
}
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 3);
    }

    #[test]
    fn collects_calls_from_different_functions() {
        let src = r#"
async fn func_a() {
    client.get("https://example.com/a").await?;
}

fn func_b() {
    ureq::get("https://example.com/b");
}
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 2);

        let func_a_call = calls
            .iter()
            .find(|c| c.function_name == Some("func_a".to_string()))
            .unwrap();
        let func_b_call = calls
            .iter()
            .find(|c| c.function_name == Some("func_b".to_string()))
            .unwrap();

        assert!(matches!(func_a_call.client_kind, HttpClientKind::Reqwest));
        assert!(matches!(func_b_call.client_kind, HttpClientKind::Ureq));
    }

    // ==================== Edge Cases ====================

    #[test]
    fn handles_empty_file() {
        let calls = parse_and_summarize_http("");
        assert!(calls.is_empty());
    }

    #[test]
    fn ignores_non_http_calls() {
        let calls = parse_and_summarize_http("println!(\"hello\")");
        assert!(calls.is_empty());
    }

    #[test]
    fn handles_class_methods() {
        let src = r#"
struct ApiClient {
    client: reqwest::Client,
}

impl ApiClient {
    async fn fetch(&self) -> Result<String, reqwest::Error> {
        self.client.get("https://example.com").await?.text().await
    }
}
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, Some("fetch".to_string()));
        assert!(calls[0].in_async_function);
    }

    // ==================== Real-World Scenarios ====================

    #[test]
    fn handles_real_world_reqwest_client() {
        let src = r#"
use reqwest;

async fn fetch_user(user_id: u64) -> Result<User, reqwest::Error> {
    let client = reqwest::Client::new();
    let response = client
        .get(&format!("https://api.example.com/users/{}", user_id))
        .header("Authorization", format!("Bearer {}", token))
        .timeout(Duration::from_secs(10))
        .send()
        .await?;
    response.json().await
}
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert!(matches!(calls[0].client_kind, HttpClientKind::Reqwest));
        assert_eq!(calls[0].method_name, "get");
        assert!(calls[0].has_timeout);
        assert_eq!(calls[0].timeout_value, Some(10.0));
    }

    #[test]
    fn handles_ureq_sync_client() {
        let src = r#"
fn fetch_sync() -> Result<String, ureq::Error> {
    let response = ureq::get("https://api.example.com/data")
        .set("Authorization", &format!("Bearer {}", token))
        .call()?;
    response.into_string().map_err(|e| ureq::Error::from(e))
}
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert!(matches!(calls[0].client_kind, HttpClientKind::Ureq));
        assert!(!calls[0].in_async_function);
        assert!(!calls[0].has_await);
    }
}
