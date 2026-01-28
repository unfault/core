use crate::parse::ast::{AstLocation, ParsedFile};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tree_sitter::Node;

use crate::semantics::common::http::{HttpUrlExpr, HttpUrlExprKind};

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

    /// URL if statically determinable as a literal.
    pub url_literal: Option<String>,

    /// URL expression metadata when the URL is not a literal.
    pub url_expr: Option<HttpUrlExpr>,

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
    fn walk(
        file: &ParsedFile,
        node: Node,
        out: &mut Vec<HttpCallSite>,
        enclosing_fn_name: &mut Option<String>,
        const_string_bindings: &HashMap<String, String>,
    ) {
        // Track function boundaries
        if matches!(node.kind(), "function_declaration" | "method_declaration") {
            if let Some(name_node) = node.child_by_field_name("name") {
                *enclosing_fn_name = Some(file.text_for_node(&name_node));
            }
        }

        if node.kind() == "call_expression" {
            if let Some(site) =
                extract_http_call(file, node, enclosing_fn_name.clone(), const_string_bindings)
            {
                out.push(site);
            }
        }

        let mut child = node.child(0);
        while let Some(c) = child {
            walk(file, c, out, enclosing_fn_name, const_string_bindings);
            child = c.next_sibling();
        }

        // Leaving function scope
        if matches!(node.kind(), "function_declaration" | "method_declaration") {
            *enclosing_fn_name = None;
        }
    }

    let mut enclosing_fn_name: Option<String> = None;
    walk(
        file,
        root,
        out,
        &mut enclosing_fn_name,
        const_string_bindings,
    );
}

fn extract_http_call(
    file: &ParsedFile,
    call_node: Node,
    enclosing_fn_name: Option<String>,
    const_string_bindings: &HashMap<String, String>,
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

    let (url_literal, url_expr) =
        extract_url_from_first_arg(file, call_node, const_string_bindings);

    Some(HttpCallSite {
        client_kind,
        method_name,
        url_literal,
        url_expr,
        call_text,
        has_timeout,
        error_handled,
        location,
        function_name: enclosing_fn_name,
        start_byte: byte_range.start,
        end_byte: byte_range.end,
    })
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
