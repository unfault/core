use crate::parse::ast::AstLocation;
use serde::{Deserialize, Serialize};

use crate::parse::ast::ParsedFile;
use tree_sitter::Node;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HttpClientKind {
    Requests,
    Httpx,
    Aiohttp,
    Other(String),
}

/// Source of retry behavior detected around an HTTP call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RetrySource {
    /// Function decorated with @tenacity.retry or @retry
    TenacityDecorator,
    /// HTTP call inside a loop with sleep/backoff pattern
    LoopWithSleep,
    /// Session configured with HTTPAdapter and Retry
    SessionConfiguredRetry,
    /// Using backoff library decorator
    BackoffDecorator,
    /// Using stamina library
    StaminaDecorator,
    /// Other retry mechanism
    Other(String),
}

/// A single HTTP client call in Python code (best-effort).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpCallSite {
    /// Library: requests, httpx, etc.
    pub client_kind: HttpClientKind,

    /// Retry behavior detected for this call (filled in post-pass).
    pub retry_source: Option<RetrySource>,

    /// Method name, e.g. "get", "post".
    pub method_name: String,

    /// Exact text of the call expression.
    pub call_text: String,

    /// Whether this call has an explicit `timeout=` kwarg.
    pub has_timeout: bool,

    /// Where in the file this call is (line/col).
    pub location: AstLocation,

    /// Name of enclosing function, if we know it.
    pub function_name: Option<String>,

    /// Whether the enclosing function is async (you already have this).
    pub in_async_function: bool,

    /// Whether the call is wrapped in asyncio.to_thread(), loop.run_in_executor(), etc.
    /// This makes blocking calls safe in async context.
    pub is_thread_offloaded: bool,

    /// Byte range of the call in the original source file.
    /// These are absolute byte offsets into `ParsedFile.source`.
    pub start_byte: usize,
    pub end_byte: usize,
}

/// Build a list of HTTP client calls in this Python file.
///
/// This is best-effort and conservative:
/// - detects module-level `requests.*` and `httpx.*` calls
/// - uses a simple textual "timeout=" check
/// - detects retry patterns (tenacity, backoff, stamina decorators, session retry config)
pub fn summarize_http_clients(file: &ParsedFile) -> Vec<HttpCallSite> {
    let root = file.tree.root_node();
    let mut calls = Vec::new();
    collect_http_calls(file, root, &mut calls);
    calls
}

/// Detect retry-related decorators on a decorated_definition node.
///
/// In tree-sitter Python, decorated functions are represented as:
/// ```text
/// decorated_definition
///   decorator
///     @
///     identifier/attribute/call
///   function_definition
///     ...
/// ```
///
/// Returns Some(RetrySource) if a retry decorator is found, None otherwise.
fn detect_retry_decorator(file: &ParsedFile, decorated_def_node: Node) -> Option<RetrySource> {
    // Look for decorator nodes in the decorated_definition
    let mut cursor = decorated_def_node.walk();
    for child in decorated_def_node.children(&mut cursor) {
        if child.kind() == "decorator" {
            let decorator_text = file.text_for_node(&child);

            // Check for tenacity patterns
            // @retry, @tenacity.retry, @retry(...)
            if decorator_text.contains("@retry")
                || decorator_text.contains("@tenacity.retry")
                || decorator_text.contains("@tenacity.Retrying")
            {
                return Some(RetrySource::TenacityDecorator);
            }

            // Check for backoff library patterns
            // @backoff.on_exception, @backoff.on_predicate
            if decorator_text.contains("@backoff.on_exception")
                || decorator_text.contains("@backoff.on_predicate")
            {
                return Some(RetrySource::BackoffDecorator);
            }

            // Check for stamina library patterns
            // @stamina.retry
            if decorator_text.contains("@stamina.retry") {
                return Some(RetrySource::StaminaDecorator);
            }
        }
    }
    None
}

/// Check if the file contains session-level retry configuration.
///
/// Looks for patterns like:
/// - `HTTPAdapter(max_retries=Retry(...))`
/// - `session.mount(..., HTTPAdapter(...))`
fn detect_session_retry_config(file: &ParsedFile) -> bool {
    let source = &file.source;

    // Check for urllib3 Retry configuration with HTTPAdapter
    if source.contains("HTTPAdapter") && source.contains("Retry(") {
        return true;
    }

    // Check for httpx retry transport configuration
    if source.contains("httpx.HTTPTransport") && source.contains("retries") {
        return true;
    }

    false
}

fn collect_http_calls(file: &ParsedFile, root: Node, out: &mut Vec<HttpCallSite>) {
    // Pre-check if file has session-level retry config
    let has_session_retry = detect_session_retry_config(file);

    fn walk(
        file: &ParsedFile,
        node: Node,
        out: &mut Vec<HttpCallSite>,
        enclosing_fn_name: &mut Option<String>,
        enclosing_fn_is_async: &mut bool,
        enclosing_fn_retry: &mut Option<RetrySource>,
        has_session_retry: bool,
    ) {
        // Detect decorated function definitions (for retry decorator detection)
        // In tree-sitter Python, decorated functions are:
        //   decorated_definition
        //     decorator
        //     function_definition
        if node.kind() == "decorated_definition" {
            // Check for retry decorators on this decorated definition
            *enclosing_fn_retry = detect_retry_decorator(file, node);
        }

        // Detect (async) function boundaries.
        if node.kind() == "function_definition" {
            if let Some(name_node) = node.child_by_field_name("name") {
                *enclosing_fn_name = Some(file.text_for_node(&name_node));
            }

            // Very simple heuristic: does the function text start with "async def"?
            let fn_text = file.text_for_node(&node);
            *enclosing_fn_is_async = fn_text.trim_start().starts_with("async def");

            // For non-decorated functions, retry is None (already set by decorated_definition if present)
        }

        if node.kind() == "call" {
            // Check if this call is wrapped in asyncio.to_thread/run_in_executor
            let is_thread_offloaded = check_thread_offload(file, node);

            if let Some(mut site) = extract_http_call(
                file,
                node,
                enclosing_fn_name.clone(),
                *enclosing_fn_is_async,
                is_thread_offloaded,
            ) {
                // Set retry source based on context
                if enclosing_fn_retry.is_some() {
                    site.retry_source = enclosing_fn_retry.clone();
                } else if has_session_retry {
                    site.retry_source = Some(RetrySource::SessionConfiguredRetry);
                }
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
                enclosing_fn_is_async,
                enclosing_fn_retry,
                has_session_retry,
            );
            child = c.next_sibling();
        }

        // Leaving the function scope.
        if node.kind() == "function_definition" {
            *enclosing_fn_name = None;
            *enclosing_fn_is_async = false;
        }

        // Clear retry when leaving decorated_definition scope
        if node.kind() == "decorated_definition" {
            *enclosing_fn_retry = None;
        }
    }

    let mut enclosing_fn_name: Option<String> = None;
    let mut enclosing_fn_is_async = false;
    let mut enclosing_fn_retry: Option<RetrySource> = None;
    walk(
        file,
        root,
        out,
        &mut enclosing_fn_name,
        &mut enclosing_fn_is_async,
        &mut enclosing_fn_retry,
        has_session_retry,
    );
}

/// Check if a call node is wrapped in asyncio.to_thread() or similar thread offloading patterns.
fn check_thread_offload(file: &ParsedFile, call_node: Node) -> bool {
    // Walk up the tree to find if this call is wrapped in:
    // - asyncio.to_thread(lambda: call())
    // - loop.run_in_executor(None, lambda: call())
    // - await asyncio.to_thread(...)

    let mut current = call_node;
    let mut depth = 0;
    const MAX_DEPTH: i32 = 10; // Limit traversal depth

    while let Some(parent) = current.parent() {
        depth += 1;
        if depth > MAX_DEPTH {
            break;
        }

        // Check if parent is a call expression
        if parent.kind() == "call" {
            if let Some(func) = parent.child_by_field_name("function") {
                let func_text = file.text_for_node(&func);

                // Check for asyncio.to_thread
                if func_text == "asyncio.to_thread" {
                    return true;
                }

                // Check for run_in_executor (could be loop.run_in_executor or asyncio.run_in_executor)
                if func_text.ends_with("run_in_executor") {
                    return true;
                }

                // Check for sync_to_async (Django channels)
                if func_text.ends_with("sync_to_async") || func_text == "database_sync_to_async" {
                    return true;
                }

                // Check for anyio.to_thread.run_sync
                if func_text.contains("to_thread") && func_text.contains("run_sync") {
                    return true;
                }
            }
        }

        current = parent;
    }

    false
}

fn extract_http_call(
    file: &ParsedFile,
    call_node: Node,
    enclosing_fn_name: Option<String>,
    in_async_function: bool,
    is_thread_offloaded: bool,
) -> Option<HttpCallSite> {
    let func = call_node.child_by_field_name("function")?;

    // Only handle attribute calls: `requests.get(...)`, `httpx.post(...)`
    if func.kind() != "attribute" {
        return None;
    }

    let object = func.child_by_field_name("object")?;
    let attr = func.child_by_field_name("attribute")?;

    let object_text = file.text_for_node(&object);
    let method_name = file.text_for_node(&attr);

    let client_kind = match object_text.as_str() {
        "requests" => HttpClientKind::Requests,
        "httpx" => HttpClientKind::Httpx,
        _ => return None,
    };

    let call_text = file.text_for_node(&call_node);
    let args_text = if let Some(args) = call_node.child_by_field_name("arguments") {
        file.text_for_node(&args)
    } else {
        String::new()
    };

    let has_timeout = args_text.contains("timeout=");

    let location = file.location_for_node(&call_node);
    let byte_range = call_node.byte_range();

    Some(HttpCallSite {
        client_kind,
        method_name,
        call_text,
        has_timeout,
        location,
        function_name: enclosing_fn_name,
        in_async_function,
        is_thread_offloaded,
        start_byte: byte_range.start,
        end_byte: byte_range.end,
        retry_source: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::python::parse_python_file;
    use crate::types::context::{Language, SourceFile};

    /// Helper to parse Python source and summarize HTTP clients
    fn parse_and_summarize_http(source: &str) -> Vec<HttpCallSite> {
        let sf = SourceFile {
            path: "test.py".to_string(),
            language: Language::Python,
            content: source.to_string(),
        };
        let parsed = parse_python_file(FileId(1), &sf).expect("parsing should succeed");
        summarize_http_clients(&parsed)
    }

    // ==================== HttpClientKind Tests ====================

    #[test]
    fn detects_requests_library() {
        let calls = parse_and_summarize_http("requests.get('https://example.com')");
        assert_eq!(calls.len(), 1);
        assert!(matches!(calls[0].client_kind, HttpClientKind::Requests));
    }

    #[test]
    fn detects_httpx_library() {
        let calls = parse_and_summarize_http("httpx.get('https://example.com')");
        assert_eq!(calls.len(), 1);
        assert!(matches!(calls[0].client_kind, HttpClientKind::Httpx));
    }

    #[test]
    fn ignores_unknown_http_libraries() {
        let calls = parse_and_summarize_http("urllib.request.urlopen('https://example.com')");
        assert!(calls.is_empty());
    }

    #[test]
    fn ignores_non_http_calls() {
        let calls = parse_and_summarize_http("print('hello')");
        assert!(calls.is_empty());
    }

    // ==================== Method Name Tests ====================

    #[test]
    fn captures_get_method() {
        let calls = parse_and_summarize_http("requests.get('https://example.com')");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method_name, "get");
    }

    #[test]
    fn captures_post_method() {
        let calls = parse_and_summarize_http("requests.post('https://example.com', data={})");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method_name, "post");
    }

    #[test]
    fn captures_put_method() {
        let calls = parse_and_summarize_http("httpx.put('https://example.com', json={})");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method_name, "put");
    }

    #[test]
    fn captures_delete_method() {
        let calls = parse_and_summarize_http("requests.delete('https://example.com/item/1')");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method_name, "delete");
    }

    #[test]
    fn captures_patch_method() {
        let calls = parse_and_summarize_http("httpx.patch('https://example.com', json={})");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method_name, "patch");
    }

    #[test]
    fn captures_head_method() {
        let calls = parse_and_summarize_http("requests.head('https://example.com')");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method_name, "head");
    }

    #[test]
    fn captures_options_method() {
        let calls = parse_and_summarize_http("requests.options('https://example.com')");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method_name, "options");
    }

    // ==================== Timeout Detection Tests ====================

    #[test]
    fn detects_timeout_keyword_argument() {
        let calls = parse_and_summarize_http("requests.get('https://example.com', timeout=30)");
        assert_eq!(calls.len(), 1);
        assert!(calls[0].has_timeout);
    }

    #[test]
    fn detects_timeout_with_float_value() {
        let calls = parse_and_summarize_http("requests.get('https://example.com', timeout=2.5)");
        assert_eq!(calls.len(), 1);
        assert!(calls[0].has_timeout);
    }

    #[test]
    fn detects_timeout_with_tuple_value() {
        let calls =
            parse_and_summarize_http("requests.get('https://example.com', timeout=(3.05, 27))");
        assert_eq!(calls.len(), 1);
        assert!(calls[0].has_timeout);
    }

    #[test]
    fn detects_timeout_with_none_value() {
        let calls = parse_and_summarize_http("requests.get('https://example.com', timeout=None)");
        assert_eq!(calls.len(), 1);
        assert!(calls[0].has_timeout);
    }

    #[test]
    fn detects_missing_timeout() {
        let calls = parse_and_summarize_http("requests.get('https://example.com')");
        assert_eq!(calls.len(), 1);
        assert!(!calls[0].has_timeout);
    }

    #[test]
    fn detects_missing_timeout_with_other_kwargs() {
        let calls = parse_and_summarize_http(
            "requests.get('https://example.com', headers={'X-Custom': 'value'})",
        );
        assert_eq!(calls.len(), 1);
        assert!(!calls[0].has_timeout);
    }

    // ==================== Function Context Tests ====================

    #[test]
    fn captures_enclosing_function_name() {
        let src = r#"
def fetch_data():
    return requests.get('https://example.com')
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, Some("fetch_data".to_string()));
    }

    #[test]
    fn captures_enclosing_async_function() {
        let src = r#"
async def fetch_data():
    return requests.get('https://example.com')
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, Some("fetch_data".to_string()));
        assert!(calls[0].in_async_function);
    }

    #[test]
    fn detects_sync_function_context() {
        let src = r#"
def sync_fetch():
    return requests.get('https://example.com')
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert!(!calls[0].in_async_function);
    }

    #[test]
    fn module_level_call_has_no_function_name() {
        let calls = parse_and_summarize_http("response = requests.get('https://example.com')");
        assert_eq!(calls.len(), 1);
        assert!(calls[0].function_name.is_none());
    }

    #[test]
    fn module_level_call_is_not_async() {
        let calls = parse_and_summarize_http("response = requests.get('https://example.com')");
        assert_eq!(calls.len(), 1);
        assert!(!calls[0].in_async_function);
    }

    // ==================== Call Text Tests ====================

    #[test]
    fn captures_full_call_text() {
        let calls = parse_and_summarize_http("requests.get('https://example.com', timeout=30)");
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].call_text,
            "requests.get('https://example.com', timeout=30)"
        );
    }

    #[test]
    fn captures_multiline_call_text() {
        let src = r#"requests.post(
    'https://example.com',
    json={'key': 'value'},
    timeout=30
)"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert!(calls[0].call_text.contains("requests.post"));
        assert!(calls[0].call_text.contains("json="));
    }

    // ==================== Byte Range Tests ====================

    #[test]
    fn captures_byte_range() {
        let src = "requests.get('https://example.com')";
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].start_byte, 0);
        assert_eq!(calls[0].end_byte, src.len());
    }

    #[test]
    fn byte_range_is_correct_with_leading_content() {
        let src = "x = 1\nrequests.get('https://example.com')";
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        // The call starts after "x = 1\n"
        assert!(calls[0].start_byte > 0);
        assert_eq!(calls[0].end_byte, src.len());
    }

    // ==================== Location Tests ====================

    #[test]
    fn captures_correct_line_number() {
        let src = r#"
import requests

def fetch():
    return requests.get('https://example.com')
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        // The call is on line 4 (0-indexed)
        assert_eq!(calls[0].location.range.start_line, 4);
    }

    // ==================== Multiple Calls Tests ====================

    #[test]
    fn collects_multiple_http_calls() {
        let src = r#"
def fetch_all():
    a = requests.get('https://example.com/a')
    b = requests.get('https://example.com/b')
    c = httpx.post('https://example.com/c')
    return a, b, c
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 3);
    }

    #[test]
    fn collects_calls_from_different_functions() {
        let src = r#"
def func_a():
    return requests.get('https://example.com/a')

def func_b():
    return httpx.get('https://example.com/b')
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

        assert!(matches!(func_a_call.client_kind, HttpClientKind::Requests));
        assert!(matches!(func_b_call.client_kind, HttpClientKind::Httpx));
    }

    // ==================== Edge Cases ====================

    #[test]
    fn handles_empty_file() {
        let calls = parse_and_summarize_http("");
        assert!(calls.is_empty());
    }

    #[test]
    fn handles_file_with_only_imports() {
        let src = r#"
import requests
import httpx
"#;
        let calls = parse_and_summarize_http(src);
        assert!(calls.is_empty());
    }

    #[test]
    fn ignores_non_attribute_calls() {
        // Direct function calls (not attribute access) should be ignored
        let calls = parse_and_summarize_http("get('https://example.com')");
        assert!(calls.is_empty());
    }

    #[test]
    fn ignores_requests_on_different_object() {
        // Should not match if 'requests' is an attribute of something else
        let calls = parse_and_summarize_http("self.requests.get('https://example.com')");
        assert!(calls.is_empty());
    }

    #[test]
    fn handles_nested_function_calls() {
        let src = r#"
def outer():
    def inner():
        return requests.get('https://example.com')
    return inner()
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        // Note: due to simple scope tracking, this might show "outer" or "inner"
        // depending on implementation details
    }

    #[test]
    fn handles_class_methods() {
        let src = r#"
class Client:
    def fetch(self):
        return requests.get('https://example.com')
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, Some("fetch".to_string()));
    }

    #[test]
    fn handles_async_class_methods() {
        let src = r#"
class Client:
    async def fetch(self):
        return httpx.get('https://example.com')
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].function_name, Some("fetch".to_string()));
        assert!(calls[0].in_async_function);
    }

    // ==================== Mixed Timeout Scenarios ====================

    #[test]
    fn mixed_timeout_scenarios() {
        let src = r#"
def good():
    return requests.get('https://example.com', timeout=30)

def bad():
    return requests.post('https://example.com')
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 2);

        let good_call = calls
            .iter()
            .find(|c| c.function_name == Some("good".to_string()))
            .unwrap();
        let bad_call = calls
            .iter()
            .find(|c| c.function_name == Some("bad".to_string()))
            .unwrap();

        assert!(good_call.has_timeout);
        assert!(!bad_call.has_timeout);
    }

    // ==================== Complex Real-World Scenarios ====================

    #[test]
    fn handles_real_world_api_client() {
        let src = r#"
import requests

class APIClient:
    def __init__(self, base_url):
        self.base_url = base_url
    
    def get_user(self, user_id):
        return requests.get(
            f"{self.base_url}/users/{user_id}",
            headers={"Authorization": "Bearer token"},
            timeout=30
        )
    
    def create_user(self, data):
        return requests.post(
            f"{self.base_url}/users",
            json=data
        )
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 2);

        let get_call = calls.iter().find(|c| c.method_name == "get").unwrap();
        let post_call = calls.iter().find(|c| c.method_name == "post").unwrap();

        assert!(get_call.has_timeout);
        assert!(!post_call.has_timeout);
        assert_eq!(get_call.function_name, Some("get_user".to_string()));
        assert_eq!(post_call.function_name, Some("create_user".to_string()));
    }

    #[test]
    fn handles_httpx_async_client() {
        let src = r#"
import httpx

async def fetch_data():
    response = httpx.get('https://api.example.com/data', timeout=10)
    return response.json()

async def post_data(payload):
    response = httpx.post('https://api.example.com/data', json=payload)
    return response.json()
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 2);

        for call in &calls {
            assert!(call.in_async_function);
            assert!(matches!(call.client_kind, HttpClientKind::Httpx));
        }

        let get_call = calls.iter().find(|c| c.method_name == "get").unwrap();
        let post_call = calls.iter().find(|c| c.method_name == "post").unwrap();

        assert!(get_call.has_timeout);
        assert!(!post_call.has_timeout);
    }

    // ==================== Edge Case: No Arguments Node ====================

    #[test]
    fn handles_call_without_arguments_node() {
        // This tests line 134 - when there's no arguments node
        // In practice, tree-sitter always provides an arguments node for calls,
        // but we test the fallback path
        let calls = parse_and_summarize_http("requests.get()");
        assert_eq!(calls.len(), 1);
        // Even with empty args, should still detect the call
        assert!(!calls[0].has_timeout);
    }

    #[test]
    fn handles_call_with_empty_parentheses() {
        let calls = parse_and_summarize_http("httpx.post()");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].method_name, "post");
        assert!(!calls[0].has_timeout);
    }

    // ==================== Thread Offload Detection Tests ====================

    #[test]
    fn detects_asyncio_to_thread_wrapper() {
        let src = r#"
import asyncio

async def fetch():
    return await asyncio.to_thread(lambda: requests.get('https://example.com'))
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert!(
            calls[0].is_thread_offloaded,
            "Request inside asyncio.to_thread should be marked as thread offloaded"
        );
    }

    #[test]
    fn detects_run_in_executor_wrapper() {
        let src = r#"
import asyncio

async def fetch():
    loop = asyncio.get_event_loop()
    return await loop.run_in_executor(None, lambda: requests.get('https://example.com'))
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert!(
            calls[0].is_thread_offloaded,
            "Request inside run_in_executor should be marked as thread offloaded"
        );
    }

    #[test]
    fn regular_async_call_is_not_thread_offloaded() {
        let src = r#"
async def fetch():
    return requests.get('https://example.com')
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert!(
            !calls[0].is_thread_offloaded,
            "Direct request in async function should NOT be marked as thread offloaded"
        );
    }

    #[test]
    fn sync_function_call_is_not_thread_offloaded() {
        let src = r#"
def fetch():
    return requests.get('https://example.com')
"#;
        let calls = parse_and_summarize_http(src);
        assert_eq!(calls.len(), 1);
        assert!(
            !calls[0].is_thread_offloaded,
            "Request in sync function should NOT be marked as thread offloaded"
        );
    }
}
