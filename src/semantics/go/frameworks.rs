//! HTTP framework route extraction for Go.
//!
//! This module extracts HTTP route information from common Go web frameworks:
//! - Gin: `r.GET("/path", handler)`
//! - Echo: `e.GET("/path", handler)`
//! - Fiber: `app.Get("/path", handler)`
//! - Chi: `r.Get("/path", handler)`
//! - Gorilla Mux: `r.HandleFunc("/path", handler).Methods("GET")`

use crate::parse::ast::{AstLocation, ParsedFile};
use serde::{Deserialize, Serialize};
use tree_sitter::Node;

/// A single HTTP route registration from any Go framework.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoRoute {
    /// Framework that registered this route
    pub framework: GoHttpFramework,
    /// HTTP method (GET, POST, PUT, DELETE, PATCH, etc.)
    pub http_method: String,
    /// Route path (e.g., "/users/:id")
    pub path: String,
    /// Handler function name (if identifiable)
    pub handler_name: Option<String>,
    /// Location in source
    pub location: AstLocation,
    /// Start byte offset
    pub start_byte: usize,
    /// End byte offset
    pub end_byte: usize,
}

/// Supported Go HTTP frameworks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoHttpFramework {
    Gin,
    Echo,
    Fiber,
    Chi,
    Mux,
    NetHttp,
}

/// Summary of Go HTTP framework usage in a file.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GoFrameworkSummary {
    /// All routes found in this file
    pub routes: Vec<GoRoute>,
    /// Detected frameworks used
    pub frameworks: Vec<GoHttpFramework>,
}

impl GoFrameworkSummary {
    /// Check if any framework was detected
    pub fn has_framework(&self) -> bool {
        !self.frameworks.is_empty() || !self.routes.is_empty()
    }
}

/// Extract routes from all supported Go HTTP frameworks.
pub fn extract_go_routes(parsed: &ParsedFile) -> GoFrameworkSummary {
    let mut summary = GoFrameworkSummary::default();
    let root = parsed.tree.root_node();
    
    // First pass: detect which frameworks are imported
    detect_frameworks(parsed, root, &mut summary);
    
    // Second pass: extract routes
    collect_routes(parsed, root, &mut summary);
    
    summary
}

/// HTTP methods for method-style routing (Gin, Echo style).
const HTTP_METHODS_UPPER: &[&str] = &[
    "GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS",
];

/// HTTP methods for lowercase style (Fiber, Chi style).
const HTTP_METHODS_LOWER: &[&str] = &[
    "Get", "Post", "Put", "Delete", "Patch", "Head", "Options",
];

fn detect_frameworks(parsed: &ParsedFile, node: Node, summary: &mut GoFrameworkSummary) {
    if node.kind() == "import_declaration" || node.kind() == "import_spec" {
        let text = parsed.text_for_node(&node);
        
        if text.contains("github.com/gin-gonic/gin") {
            if !summary.frameworks.contains(&GoHttpFramework::Gin) {
                summary.frameworks.push(GoHttpFramework::Gin);
            }
        }
        if text.contains("github.com/labstack/echo") {
            if !summary.frameworks.contains(&GoHttpFramework::Echo) {
                summary.frameworks.push(GoHttpFramework::Echo);
            }
        }
        if text.contains("github.com/gofiber/fiber") {
            if !summary.frameworks.contains(&GoHttpFramework::Fiber) {
                summary.frameworks.push(GoHttpFramework::Fiber);
            }
        }
        if text.contains("github.com/go-chi/chi") {
            if !summary.frameworks.contains(&GoHttpFramework::Chi) {
                summary.frameworks.push(GoHttpFramework::Chi);
            }
        }
        if text.contains("github.com/gorilla/mux") {
            if !summary.frameworks.contains(&GoHttpFramework::Mux) {
                summary.frameworks.push(GoHttpFramework::Mux);
            }
        }
        if text.contains("net/http") {
            if !summary.frameworks.contains(&GoHttpFramework::NetHttp) {
                summary.frameworks.push(GoHttpFramework::NetHttp);
            }
        }
    }
    
    // Recurse
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            detect_frameworks(parsed, child, summary);
        }
    }
}

fn collect_routes(parsed: &ParsedFile, node: Node, summary: &mut GoFrameworkSummary) {
    if node.kind() == "call_expression" {
        if let Some(route) = extract_route(parsed, node, &summary.frameworks) {
            summary.routes.push(route);
        }
    }
    
    // Recurse
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_routes(parsed, child, summary);
        }
    }
}

fn extract_route(parsed: &ParsedFile, call_node: Node, frameworks: &[GoHttpFramework]) -> Option<GoRoute> {
    let func = call_node.child_by_field_name("function")?;
    
    // Check for selector expression: router.GET, e.POST, app.Get, etc.
    if func.kind() == "selector_expression" {
        let field = func.child_by_field_name("field")?;
        let method_name = parsed.text_for_node(&field);
        
        // Determine HTTP method and framework
        let (http_method, framework) = if HTTP_METHODS_UPPER.contains(&method_name.as_str()) {
            // Gin or Echo style: .GET, .POST
            let fw = if frameworks.contains(&GoHttpFramework::Echo) {
                GoHttpFramework::Echo
            } else {
                GoHttpFramework::Gin // Default to Gin for uppercase methods
            };
            (method_name.clone(), fw)
        } else if HTTP_METHODS_LOWER.contains(&method_name.as_str()) {
            // Fiber or Chi style: .Get, .Post
            let fw = if frameworks.contains(&GoHttpFramework::Fiber) {
                GoHttpFramework::Fiber
            } else {
                GoHttpFramework::Chi // Default to Chi for lowercase methods
            };
            (method_name.to_uppercase(), fw)
        } else if method_name == "Any" || method_name == "Handle" {
            // Gin's Any and Handle methods
            ("ANY".to_string(), GoHttpFramework::Gin)
        } else if method_name == "HandleFunc" {
            // Gorilla mux or net/http style
            return extract_mux_route(parsed, call_node);
        } else {
            return None;
        };
        
        // Get arguments: first should be path, second should be handler(s)
        let args = call_node.child_by_field_name("arguments")?;
        
        let mut path: Option<String> = None;
        let mut handler_name: Option<String> = None;
        
        let mut arg_index = 0;
        for i in 0..args.child_count() {
            if let Some(arg) = args.child(i) {
                // Skip commas and parentheses
                if arg.kind() == "," || arg.kind() == "(" || arg.kind() == ")" {
                    continue;
                }
                
                if arg_index == 0 {
                    // First argument is the path
                    let path_text = parsed.text_for_node(&arg);
                    path = Some(path_text.trim_matches('"').to_string());
                } else if arg_index == 1 && handler_name.is_none() {
                    // Second argument is the handler
                    let handler_text = parsed.text_for_node(&arg);
                    handler_name = extract_handler_name(&handler_text);
                }
                arg_index += 1;
            }
        }
        
        let route_path = path?;
        
        Some(GoRoute {
            framework,
            http_method,
            path: route_path,
            handler_name,
            location: parsed.location_for_node(&call_node),
            start_byte: call_node.start_byte(),
            end_byte: call_node.end_byte(),
        })
    } else if func.kind() == "identifier_expression" || func.kind() == "identifier" {
        // Check for net/http: http.HandleFunc("/path", handler) or http.Handle("/path", handler)
        extract_net_http_route(parsed, call_node)
    } else {
        None
    }
}

fn extract_mux_route(parsed: &ParsedFile, call_node: Node) -> Option<GoRoute> {
    // Gorilla Mux pattern: r.HandleFunc("/path", handler).Methods("GET")
    // This is more complex - we need to look for the .Methods() chaining
    
    let args = call_node.child_by_field_name("arguments")?;
    
    let mut path: Option<String> = None;
    let mut handler_name: Option<String> = None;
    
    let mut arg_index = 0;
    for i in 0..args.child_count() {
        if let Some(arg) = args.child(i) {
            if arg.kind() == "," || arg.kind() == "(" || arg.kind() == ")" {
                continue;
            }
            
            if arg_index == 0 {
                let path_text = parsed.text_for_node(&arg);
                path = Some(path_text.trim_matches('"').to_string());
            } else if arg_index == 1 {
                let handler_text = parsed.text_for_node(&arg);
                handler_name = extract_handler_name(&handler_text);
            }
            arg_index += 1;
        }
    }
    
    let route_path = path?;
    
    // Try to find .Methods() call by looking at parent
    let http_method = find_methods_chain(parsed, call_node).unwrap_or_else(|| "ANY".to_string());
    
    Some(GoRoute {
        framework: GoHttpFramework::Mux,
        http_method,
        path: route_path,
        handler_name,
        location: parsed.location_for_node(&call_node),
        start_byte: call_node.start_byte(),
        end_byte: call_node.end_byte(),
    })
}

fn extract_net_http_route(parsed: &ParsedFile, call_node: Node) -> Option<GoRoute> {
    // net/http pattern: http.HandleFunc("/path", handler) or http.Handle("/path", handler)
    // The function is an identifier like "http.HandleFunc" or "http.Handle"
    
    let func = call_node.child_by_field_name("function")?;
    let func_text = parsed.text_for_node(&func);
    
    // Check if this is http.HandleFunc or http.Handle
    if !func_text.starts_with("http.HandleFunc(") && !func_text.starts_with("http.Handle(") {
        return None;
    }
    
    let args = call_node.child_by_field_name("arguments")?;
    
    let mut path: Option<String> = None;
    let mut handler_name: Option<String> = None;
    
    let mut arg_index = 0;
    for i in 0..args.child_count() {
        if let Some(arg) = args.child(i) {
            if arg.kind() == "," || arg.kind() == "(" || arg.kind() == ")" {
                continue;
            }
            
            if arg_index == 0 {
                let path_text = parsed.text_for_node(&arg);
                path = Some(path_text.trim_matches('"').to_string());
            } else if arg_index == 1 {
                let handler_text = parsed.text_for_node(&arg);
                handler_name = extract_handler_name(&handler_text);
            }
            arg_index += 1;
        }
    }
    
    let route_path = path?;
    
    Some(GoRoute {
        framework: GoHttpFramework::NetHttp,
        http_method: "ANY".to_string(),
        path: route_path,
        handler_name,
        location: parsed.location_for_node(&call_node),
        start_byte: call_node.start_byte(),
        end_byte: call_node.end_byte(),
    })
}

fn find_methods_chain(parsed: &ParsedFile, node: Node) -> Option<String> {
    // Look for .Methods("GET") call in the chain
    let full_text = parsed.text_for_node(&node);
    
    // Check if this is part of a chain that includes .Methods()
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "call_expression" {
            let parent_text = parsed.text_for_node(&parent);
            if parent_text.contains(".Methods(") {
                // Extract the method from .Methods("GET")
                if let Some(start) = parent_text.find(".Methods(\"") {
                    let rest = &parent_text[start + 10..];
                    if let Some(end) = rest.find('"') {
                        return Some(rest[..end].to_string());
                    }
                }
            }
        }
        current = parent.parent();
    }
    
    // Also check siblings in expression statement
    if full_text.contains(".Methods(") {
        if let Some(start) = full_text.find(".Methods(\"") {
            let rest = &full_text[start + 10..];
            if let Some(end) = rest.find('"') {
                return Some(rest[..end].to_string());
            }
        }
    }
    
    None
}

/// Extract handler name from various patterns.
fn extract_handler_name(handler_text: &str) -> Option<String> {
    let trimmed = handler_text.trim();
    
    // Direct function reference: getUsers, CreateUser
    if trimmed.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Some(trimmed.to_string());
    }
    
    // Method reference: controller.GetUsers, h.HandleUsers
    if trimmed.contains('.') && !trimmed.contains('(') {
        let parts: Vec<&str> = trimmed.split('.').collect();
        if parts.len() == 2 {
            return Some(parts[1].to_string());
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
    
    fn parse_and_extract(source: &str) -> GoFrameworkSummary {
        let sf = SourceFile {
            path: "test.go".to_string(),
            language: Language::Go,
            content: source.to_string(),
        };
        let parsed = parse_go_file(FileId(1), &sf).expect("parsing should succeed");
        extract_go_routes(&parsed)
    }
    
    #[test]
    fn detects_gin_routes() {
        let src = r#"
package main

import "github.com/gin-gonic/gin"

func main() {
    r := gin.Default()
    r.GET("/users", getUsers)
    r.POST("/users", createUser)
    r.PUT("/users/:id", updateUser)
    r.DELETE("/users/:id", deleteUser)
}
"#;
        let summary = parse_and_extract(src);
        assert!(summary.frameworks.contains(&GoHttpFramework::Gin));
        assert_eq!(summary.routes.len(), 4);
        
        assert_eq!(summary.routes[0].http_method, "GET");
        assert_eq!(summary.routes[0].path, "/users");
        assert_eq!(summary.routes[0].handler_name, Some("getUsers".to_string()));
    }
    
    #[test]
    fn detects_echo_routes() {
        let src = r#"
package main

import "github.com/labstack/echo/v4"

func main() {
    e := echo.New()
    e.GET("/users", getUsers)
    e.POST("/users", createUser)
}
"#;
        let summary = parse_and_extract(src);
        assert!(summary.frameworks.contains(&GoHttpFramework::Echo));
        assert_eq!(summary.routes.len(), 2);
    }
    
    #[test]
    fn detects_fiber_routes() {
        let src = r#"
package main

import "github.com/gofiber/fiber/v2"

func main() {
    app := fiber.New()
    app.Get("/users", getUsers)
    app.Post("/users", createUser)
}
"#;
        let summary = parse_and_extract(src);
        assert!(summary.frameworks.contains(&GoHttpFramework::Fiber));
        assert_eq!(summary.routes.len(), 2);
        assert_eq!(summary.routes[0].http_method, "GET");
    }
    
    #[test]
    fn detects_chi_routes() {
        let src = r#"
package main

import "github.com/go-chi/chi/v5"

func main() {
    r := chi.NewRouter()
    r.Get("/users", getUsers)
    r.Post("/users", createUser)
}
"#;
        let summary = parse_and_extract(src);
        assert!(summary.frameworks.contains(&GoHttpFramework::Chi));
        assert_eq!(summary.routes.len(), 2);
    }
    
    #[test]
    fn handles_method_handlers() {
        let src = r#"
package main

import "github.com/gin-gonic/gin"

func main() {
    r := gin.Default()
    r.GET("/users", controller.GetUsers)
}
"#;
        let summary = parse_and_extract(src);
        assert_eq!(summary.routes.len(), 1);
        assert_eq!(summary.routes[0].handler_name, Some("GetUsers".to_string()));
    }

    #[test]
    fn detects_net_http_routes() {
        let src = r#"
package main

import (
    "fmt"
    "net/http"
)

func handler(w http.ResponseWriter, r *http.Request) {
    fmt.Fprintf(w, "Hello, World!")
}

func main() {
    http.HandleFunc("/", handler)
    http.ListenAndServe(":8080", nil)
}
"#;
        let summary = parse_and_extract(src);
        assert!(summary.frameworks.contains(&GoHttpFramework::NetHttp));
        assert_eq!(summary.routes.len(), 1);
        assert_eq!(summary.routes[0].path, "/");
        assert_eq!(summary.routes[0].handler_name, Some("handler".to_string()));
    }

    #[test]
    fn detects_multiple_net_http_routes() {
        let src = r#"
package main

import (
    "net/http"
)

func homeHandler(w http.ResponseWriter, r *http.Request) {}
func usersHandler(w http.ResponseWriter, r *http.Request) {}

func main() {
    http.HandleFunc("/", homeHandler)
    http.HandleFunc("/users", usersHandler)
    http.ListenAndServe(":8080", nil)
}
"#;
        let summary = parse_and_extract(src);
        assert!(summary.frameworks.contains(&GoHttpFramework::NetHttp));
        assert_eq!(summary.routes.len(), 2);
        assert_eq!(summary.routes[0].path, "/");
        assert_eq!(summary.routes[1].path, "/users");
    }
}
