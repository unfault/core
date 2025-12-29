//! HTTP client abstractions for Rust.
//!
//! This module provides analysis of HTTP client calls in Rust code,
//! detecting patterns using reqwest, ureq, hyper, and other libraries.

use serde::{Deserialize, Serialize};

use crate::parse::ast::{AstLocation, ParsedFile};

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

    /// Full text of the call expression
    pub call_text: String,

    /// Whether this call has an explicit timeout configured
    pub has_timeout: bool,

    /// Timeout value in seconds if detectable
    pub timeout_value: Option<f64>,

    /// Location in source
    pub location: AstLocation,

    /// Name of enclosing function, if known
    pub function_name: Option<String>,

    /// Whether the enclosing function is async
    pub in_async_function: bool,

    /// Whether this call uses `.await`
    pub has_await: bool,

    /// Byte range in original source
    pub start_byte: usize,
    pub end_byte: usize,
}

/// Build a list of HTTP client calls in this Rust file.
pub fn summarize_http_clients(file: &ParsedFile) -> Vec<HttpCallSite> {
    let root = file.tree.root_node();
    let mut calls = Vec::new();
    collect_http_calls(file, root, &mut calls, None, false, None);
    calls
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
) {
    let ctx = ctx.unwrap_or_default();

    if node.kind() == "function_item" {
        let fn_text = file.text_for_node(&node);
        let is_async = fn_text.contains("async fn");
        let name = node
            .child_by_field_name("name")
            .map(|n| file.text_for_node(&n));

        let mut new_ctx = ctx.clone();
        new_ctx.current_function = name;
        new_ctx.in_async_fn = is_async;

        walk_http_calls(file, node, out, &new_ctx, false);
        return;
    }

    if node.kind() == "await_expression" {
        walk_http_calls(file, node, out, &ctx, true);
        return;
    }

    walk_http_calls(file, node, out, &ctx, has_await);
}

fn walk_http_calls(
    file: &ParsedFile,
    node: tree_sitter::Node,
    out: &mut Vec<HttpCallSite>,
    ctx: &HttpCallContext,
    has_await: bool,
) {
    if node.kind() == "call_expression" {
        if let Some(call) = extract_http_call(file, &node, ctx, has_await) {
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
                walk_http_calls(file, child, out, &new_ctx, false);
            }
        }
        return;
    }

    if node.kind() == "await_expression" {
        let child_count = node.child_count();
        for i in 0..child_count {
            if let Some(child) = node.child(i) {
                walk_http_calls(file, child, out, ctx, true);
            }
        }
        return;
    }

    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            walk_http_calls(file, child, out, ctx, has_await);
        }
    }
}

/// Extract an HTTP call from a call_expression node
fn extract_http_call(
    file: &ParsedFile,
    node: &tree_sitter::Node,
    ctx: &HttpCallContext,
    has_await: bool,
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

        let (http_method, client_kind) = if value_node.kind() == "call_expression" {
            extract_http_method_and_client(file, &value_node)
        } else {
            let object = file.text_for_node(&value_node);
            let client = detect_client_kind(&object, &callee_expr)?;
            (method_name.clone(), client)
        };

        if http_method.is_empty() {
            return None;
        }

        let _args_node = node.child_by_field_name("arguments");
        let (has_timeout, timeout_value) = detect_timeout_in_chain(file, node);

        return Some(HttpCallSite {
            client_kind,
            method_name: http_method,
            call_text,
            has_timeout,
            timeout_value,
            location,
            function_name: ctx.current_function.clone(),
            in_async_function: ctx.in_async_fn,
            has_await,
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

            return Some(HttpCallSite {
                client_kind: HttpClientKind::ReqwestBlocking,
                method_name,
                call_text,
                has_timeout,
                timeout_value,
                location,
                function_name: ctx.current_function.clone(),
                in_async_function: false,
                has_await,
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

                return Some(HttpCallSite {
                    client_kind: HttpClientKind::Ureq,
                    method_name,
                    call_text,
                    has_timeout,
                    timeout_value,
                    location,
                    function_name: ctx.current_function.clone(),
                    in_async_function: false,
                    has_await,
                    start_byte: byte_range.start,
                    end_byte: byte_range.end,
                });
            }
        }
    }

    None
}

fn extract_http_method_and_client(
    file: &ParsedFile,
    node: &tree_sitter::Node,
) -> (String, HttpClientKind) {
    let func_node = node.child_by_field_name("function");
    if func_node.is_none() {
        return ("".to_string(), HttpClientKind::Other("unknown".to_string()));
    }
    let func_node = func_node.unwrap();

    if func_node.kind() == "field_expression" {
        let value_node = func_node.child_by_field_name("value");
        let field_node = func_node.child_by_field_name("field");
        if value_node.is_none() || field_node.is_none() {
            return ("".to_string(), HttpClientKind::Other("unknown".to_string()));
        }

        let inner_method = file.text_for_node(&field_node.unwrap());
        let callee_expr = file.text_for_node(&func_node);
        let object = file.text_for_node(&value_node.unwrap());

        let client_kind = detect_client_kind(&object, &callee_expr);
        if let Some(kind) = client_kind {
            if is_http_method(&inner_method) {
                return (inner_method, kind);
            }
            if value_node.unwrap().kind() == "call_expression" {
                return extract_http_method_and_client(file, &value_node.unwrap());
            }
            return (inner_method, kind);
        }

        if value_node.unwrap().kind() == "call_expression" {
            return extract_http_method_and_client(file, &value_node.unwrap());
        }
    }

    if func_node.kind() == "path_expression" || func_node.kind() == "scoped_identifier" {
        let path_text = file.text_for_node(&func_node);

        if path_text.contains("reqwest::blocking::") {
            let method_name = extract_method_from_blocking_call(&path_text);
            return (method_name, HttpClientKind::ReqwestBlocking);
        }

        if path_text.starts_with("ureq::") {
            let method_name = path_text
                .strip_prefix("ureq::")
                .and_then(|s| s.split('(').next())
                .map(|s| s.to_string())
                .unwrap_or_else(|| path_text.to_string());
            return (method_name, HttpClientKind::Ureq);
        }
    }

    ("".to_string(), HttpClientKind::Other("unknown".to_string()))
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
        let calls =
            parse_and_summarize_http("reqwest::blocking::get(\"https://example.com\")");
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
