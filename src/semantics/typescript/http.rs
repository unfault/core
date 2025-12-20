//! HTTP client detection for TypeScript/JavaScript code.

use serde::{Deserialize, Serialize};

use crate::parse::ast::{AstLocation, ParsedFile};

/// Represents an HTTP client call in TypeScript code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpCallSite {
    /// The HTTP client library being used
    pub client_kind: HttpClientKind,
    /// HTTP method (get, post, etc.)
    pub method: String,
    /// URL if statically determinable
    pub url: Option<String>,
    /// Whether a timeout is configured
    pub has_timeout: bool,
    /// Whether error handling is present (try-catch or .catch())
    pub has_error_handling: bool,
    /// Whether retry logic is configured
    pub has_retry: bool,
    /// Name of the enclosing function
    pub function_name: Option<String>,
    /// Whether this call is in an async context
    pub in_async_context: bool,
    /// Location in the source file
    pub location: AstLocation,
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

    let root = parsed.tree.root_node();
    walk_for_http_calls(root, parsed, &mut calls, None, false);

    calls
}

fn walk_for_http_calls(
    node: tree_sitter::Node,
    parsed: &ParsedFile,
    calls: &mut Vec<HttpCallSite>,
    current_function: Option<&str>,
    in_async: bool,
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

    // Check for HTTP calls
    if node.kind() == "call_expression" {
        if let Some(call) = detect_http_call(parsed, &node, effective_function, effective_async) {
            calls.push(call);
        }
    }

    // Recurse
    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            walk_for_http_calls(child, parsed, calls, effective_function, effective_async);
        }
    }
}

fn detect_http_call(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    function_name: Option<&str>,
    in_async: bool,
) -> Option<HttpCallSite> {
    let func_node = node.child_by_field_name("function")?;
    let callee = parsed.text_for_node(&func_node);
    let location = parsed.location_for_node(node);

    // Detect HTTP client and method
    let (client_kind, method) = detect_client_and_method(&callee)?;

    // Exclude route handlers (e.g., app.get('/path'), router.post('/path'))
    if client_kind == HttpClientKind::Unknown && is_route_handler(parsed, node) {
        return None;
    }

    // Get URL from first argument if available
    let url = node
        .child_by_field_name("arguments")
        .and_then(|args| args.named_child(0))
        .map(|arg| {
            let text = parsed.text_for_node(&arg);
            text.trim_matches(|c| c == '\'' || c == '"' || c == '`')
                .to_string()
        });

    // Check for timeout configuration
    let has_timeout = check_timeout(parsed, node, &callee);

    // Check for error handling
    let has_error_handling = check_error_handling(node);

    // Check for retry logic
    let has_retry = check_retry(parsed, node);

    Some(HttpCallSite {
        client_kind,
        method,
        url,
        has_timeout,
        has_error_handling,
        has_retry,
        function_name: function_name.map(|s| s.to_string()),
        in_async_context: in_async,
        location,
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
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

fn check_timeout(parsed: &ParsedFile, node: &tree_sitter::Node, callee: &str) -> bool {
    // Check arguments for timeout configuration
    if let Some(args_node) = node.child_by_field_name("arguments") {
        let args_text = parsed.text_for_node(&args_node);

        // Common timeout patterns
        if args_text.contains("timeout") {
            return true;
        }

        // Fetch with AbortController/signal
        if callee == "fetch" && args_text.contains("signal") {
            return true;
        }

        // Axios timeout option
        if callee.starts_with("axios") && args_text.contains("timeout") {
            return true;
        }
    }

    false
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
    }

    #[test]
    fn detects_timeout_in_axios() {
        let src = r#"
axios.get('https://api.example.com', { timeout: 5000 });
"#;
        let calls = parse_and_summarize(src);
        assert_eq!(calls.len(), 1);
        assert!(calls[0].has_timeout);
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
