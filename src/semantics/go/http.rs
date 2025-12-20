use crate::parse::ast::{AstLocation, ParsedFile};
use serde::{Deserialize, Serialize};
use tree_sitter::Node;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HttpClientKind {
    NetHttp,
    Resty,
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

/// A single HTTP client call in Go code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpCallSite {
    /// Library: net/http, resty, etc.
    pub client_kind: HttpClientKind,

    /// Method name, e.g., "Get", "Post", "Do".
    pub method_name: String,

    /// Exact text of the call expression.
    pub call_text: String,

    /// Whether this call has a timeout configured (context with timeout or client timeout).
    pub has_timeout: bool,

    /// Whether there's error handling for this call.
    pub error_handled: bool,

    /// Where in the file this call is (line/col).
    pub location: AstLocation,

    /// Name of enclosing function, if we know it.
    pub function_name: Option<String>,

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
    collect_http_calls(file, root, &mut calls);
    calls
}

fn collect_http_calls(file: &ParsedFile, root: Node, out: &mut Vec<HttpCallSite>) {
    fn walk(
        file: &ParsedFile,
        node: Node,
        out: &mut Vec<HttpCallSite>,
        enclosing_fn_name: &mut Option<String>,
    ) {
        // Track function boundaries
        if matches!(node.kind(), "function_declaration" | "method_declaration") {
            if let Some(name_node) = node.child_by_field_name("name") {
                *enclosing_fn_name = Some(file.text_for_node(&name_node));
            }
        }

        if node.kind() == "call_expression" {
            if let Some(site) = extract_http_call(file, node, enclosing_fn_name.clone()) {
                out.push(site);
            }
        }

        let mut child = node.child(0);
        while let Some(c) = child {
            walk(file, c, out, enclosing_fn_name);
            child = c.next_sibling();
        }

        // Leaving function scope
        if matches!(node.kind(), "function_declaration" | "method_declaration") {
            *enclosing_fn_name = None;
        }
    }

    let mut enclosing_fn_name: Option<String> = None;
    walk(file, root, out, &mut enclosing_fn_name);
}

fn extract_http_call(
    file: &ParsedFile,
    call_node: Node,
    enclosing_fn_name: Option<String>,
) -> Option<HttpCallSite> {
    let func = call_node.child_by_field_name("function")?;
    let call_text = file.text_for_node(&call_node);

    // Check for http.Get, http.Post, http.Do, etc.
    let (client_kind, method_name) = if func.kind() == "selector_expression" {
        let object = func.child_by_field_name("operand")?;
        let field = func.child_by_field_name("field")?;

        let object_text = file.text_for_node(&object);
        let method_name = file.text_for_node(&field);

        // Check for net/http client calls
        if object_text == "http" {
            match method_name.as_str() {
                "Get" | "Post" | "PostForm" | "Head" => (HttpClientKind::NetHttp, method_name),
                _ => return None,
            }
        } else if object_text.ends_with("Client") || object_text.contains("client") {
            // Likely an http.Client instance
            if matches!(method_name.as_str(), "Do" | "Get" | "Post" | "Head") {
                (HttpClientKind::NetHttp, method_name)
            } else {
                return None;
            }
        } else if object_text.contains("resty") {
            (HttpClientKind::Resty, method_name)
        } else if object_text.contains("fasthttp") {
            (HttpClientKind::Fasthttp, method_name)
        } else {
            return None;
        }
    } else {
        return None;
    };

    // Check for timeout in the call context
    // This is a heuristic - we look for context.WithTimeout or client.Timeout patterns
    let has_timeout = call_text.contains("WithTimeout")
        || call_text.contains("WithDeadline")
        || call_text.contains("Timeout:");

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

    Some(HttpCallSite {
        client_kind,
        method_name,
        call_text,
        has_timeout,
        error_handled,
        location,
        function_name: enclosing_fn_name,
        start_byte: byte_range.start,
        end_byte: byte_range.end,
    })
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
