//! Rust HTTP framework route extraction.
//!
//! Supports Axum, Actix-web, Rocket, Warp, and Poem.

use serde::{Deserialize, Serialize};

use crate::parse::ast::{AstLocation, ParsedFile};

/// Summary of Rust HTTP framework usage in a file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RustFrameworkSummary {
    /// Detected framework type
    pub framework: Option<RustFrameworkType>,

    /// HTTP routes registered in this file
    pub routes: Vec<RustFrameworkRoute>,

    /// Middleware registered in this file
    pub middleware: Vec<RustMiddlewareInfo>,

    /// Router/scope nesting
    pub route_scopes: Vec<RustRouteScope>,
}

impl RustFrameworkSummary {
    /// Check if any framework was detected.
    pub fn has_framework(&self) -> bool {
        self.framework.is_some()
    }
}

/// Supported Rust HTTP frameworks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum RustFrameworkType {
    /// Axum (https://github.com/tokio-rs/axum)
    Axum,
    /// Actix-web (https://github.com/actix/actix-web)
    ActixWeb,
    /// Rocket (https://rocket.rs)
    Rocket,
    /// Warp (https://github.com/seanmonstar/warp)
    Warp,
    /// Poem (https://github.com/poem-web/poem)
    Poem,
    /// Tide (https://github.com/http-rs/tide)
    Tide,
}

/// A route registered with a Rust HTTP framework.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustFrameworkRoute {
    /// HTTP method (GET, POST, etc.) - may be empty for wildcard routes
    pub method: String,

    /// Route path pattern (e.g., "/users/:id" or "/users/{id}")
    pub path: String,

    /// Handler function name
    pub handler_name: String,

    /// Whether this route is async
    pub is_async: bool,

    /// Router scope/nest prefix if any
    pub scope_prefix: Option<String>,

    /// Source location
    pub location: AstLocation,
}

/// Information about registered middleware.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustMiddlewareInfo {
    /// Middleware name or type
    pub name: String,

    /// Whether this is a layer (tower Layer for Axum)
    pub is_layer: bool,

    /// Whether this is global (applied to all routes)
    pub is_global: bool,

    /// Source location
    pub location: AstLocation,
}

/// A route scope/nest (for grouping routes with a prefix).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustRouteScope {
    /// Prefix path for this scope
    pub prefix: String,

    /// Parent scope prefix, if nested
    pub parent_prefix: Option<String>,

    /// Source location
    pub location: AstLocation,
}

/// Detect framework and extract routes from a parsed Rust file.
pub fn extract_rust_routes(parsed: &ParsedFile) -> RustFrameworkSummary {
    let mut summary = RustFrameworkSummary::default();

    // Detect framework from use statements
    let source = &parsed.source;
    summary.framework = detect_framework(source);

    if summary.framework.is_none() {
        return summary;
    }

    // Walk AST to find route registrations
    let root = parsed.tree.root_node();
    walk_for_routes(root, parsed, &mut summary);

    summary
}

/// Detect which Rust HTTP framework is being used based on imports.
fn detect_framework(source: &str) -> Option<RustFrameworkType> {
    // Check use statements in order of popularity
    if source.contains("use axum::") || source.contains("axum::Router") {
        return Some(RustFrameworkType::Axum);
    }
    if source.contains("use actix_web::") || source.contains("actix_web::") {
        return Some(RustFrameworkType::ActixWeb);
    }
    if source.contains("use rocket::") || source.contains("#[rocket::") || source.contains("#[get(")
    {
        return Some(RustFrameworkType::Rocket);
    }
    if source.contains("use warp::") || source.contains("warp::Filter") {
        return Some(RustFrameworkType::Warp);
    }
    if source.contains("use poem::") || source.contains("poem::Route") {
        return Some(RustFrameworkType::Poem);
    }
    if source.contains("use tide::") {
        return Some(RustFrameworkType::Tide);
    }
    None
}

/// Walk AST to find route registrations.
fn walk_for_routes(node: tree_sitter::Node, parsed: &ParsedFile, summary: &mut RustFrameworkSummary) {
    let framework = match &summary.framework {
        Some(f) => f.clone(),
        None => return,
    };

    match node.kind() {
        // Look for function items with Rocket-style attributes
        "function_item" => {
            if framework == RustFrameworkType::Rocket {
                if let Some(route) = extract_rocket_route(parsed, &node) {
                    summary.routes.push(route);
                }
            }
        }
        // Look for method calls for route registration
        "call_expression" => {
            match framework {
                RustFrameworkType::Axum => {
                    if let Some(route) = extract_axum_route(parsed, &node) {
                        summary.routes.push(route);
                    }
                    if let Some(middleware) = extract_axum_layer(parsed, &node) {
                        summary.middleware.push(middleware);
                    }
                }
                RustFrameworkType::ActixWeb => {
                    if let Some(route) = extract_actix_route(parsed, &node) {
                        summary.routes.push(route);
                    }
                }
                RustFrameworkType::Warp => {
                    if let Some(route) = extract_warp_route(parsed, &node) {
                        summary.routes.push(route);
                    }
                }
                RustFrameworkType::Poem => {
                    if let Some(route) = extract_poem_route(parsed, &node) {
                        summary.routes.push(route);
                    }
                }
                RustFrameworkType::Tide => {
                    if let Some(route) = extract_tide_route(parsed, &node) {
                        summary.routes.push(route);
                    }
                }
                _ => {}
            }
        }
        // Look for attribute items for Actix-web and Rocket macros
        "attribute_item" => {
            // Handled at function level
        }
        _ => {}
    }

    // Recurse into children
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_for_routes(child, parsed, summary);
        }
    }
}

/// Extract Axum route from method call like `.route("/path", get(handler))`.
fn extract_axum_route(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<RustFrameworkRoute> {
    let text = parsed.text_for_node(node);

    // Match patterns like:
    // .route("/users", get(list_users))
    // .route("/users/:id", get(get_user).post(create_user))
    // Router::new().route("/", get(index))

    if !text.contains(".route(") {
        return None;
    }

    // Extract path
    let path = extract_string_arg(&text, ".route(")?;

    // Extract method and handler
    // Methods are get, post, put, delete, patch, head, options, trace
    let (method, handler) = extract_axum_method_handler(&text)?;

    Some(RustFrameworkRoute {
        method: method.to_uppercase(),
        path,
        handler_name: handler,
        is_async: true, // Axum handlers are always async
        scope_prefix: None,
        location: parsed.location_for_node(node),
    })
}

/// Extract Axum method and handler from route definition.
fn extract_axum_method_handler(text: &str) -> Option<(String, String)> {
    let methods = ["get", "post", "put", "delete", "patch", "head", "options", "trace"];

    for method in methods {
        let pattern = format!("{}(", method);
        if let Some(pos) = text.find(&pattern) {
            // Find the handler name inside the method call
            let after = &text[pos + pattern.len()..];
            if let Some(end) = after.find(')') {
                let handler = after[..end].trim().to_string();
                // Clean up handler name (remove any additional method chaining)
                let handler = handler.split('.').next().unwrap_or(&handler).to_string();
                return Some((method.to_string(), handler));
            }
        }
    }
    None
}

/// Extract Axum layer/middleware from `.layer()` call.
fn extract_axum_layer(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<RustMiddlewareInfo> {
    let text = parsed.text_for_node(node);

    if !text.contains(".layer(") {
        return None;
    }

    // Extract layer name
    let name = extract_string_arg(&text, ".layer(")
        .or_else(|| extract_type_from_layer(&text))?;

    Some(RustMiddlewareInfo {
        name,
        is_layer: true,
        is_global: text.contains("Router::new()"), // Heuristic: if on Router::new(), it's global
        location: parsed.location_for_node(node),
    })
}

/// Extract type name from a layer expression like `.layer(TraceLayer::new_for_http())`.
fn extract_type_from_layer(text: &str) -> Option<String> {
    if let Some(pos) = text.find(".layer(") {
        let after = &text[pos + 7..];
        // Look for Type::method pattern
        if let Some(double_colon) = after.find("::") {
            let type_name = after[..double_colon].trim();
            if !type_name.is_empty() && type_name.chars().next()?.is_uppercase() {
                return Some(type_name.to_string());
            }
        }
        // Look for just a type name
        if let Some(paren) = after.find('(') {
            let name = after[..paren].trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Extract Actix-web route from method call like `.route("/path", web::get().to(handler))`.
fn extract_actix_route(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<RustFrameworkRoute> {
    let text = parsed.text_for_node(node);

    // Match patterns like:
    // .route("/users", web::get().to(list_users))
    // App::new().service(web::resource("/users").route(web::get().to(handler)))

    if text.contains(".route(") && text.contains("web::") {
        let path = extract_string_arg(&text, ".route(")?;
        let (method, handler) = extract_actix_method_handler(&text)?;

        return Some(RustFrameworkRoute {
            method: method.to_uppercase(),
            path,
            handler_name: handler,
            is_async: true,
            scope_prefix: None,
            location: parsed.location_for_node(node),
        });
    }

    // Match .service(web::resource(...).route(...))
    if text.contains(".service(") && text.contains("web::resource(") {
        let path = extract_string_arg(&text, "web::resource(")?;
        let (method, handler) = extract_actix_method_handler(&text)?;

        return Some(RustFrameworkRoute {
            method: method.to_uppercase(),
            path,
            handler_name: handler,
            is_async: true,
            scope_prefix: None,
            location: parsed.location_for_node(node),
        });
    }

    None
}

/// Extract Actix-web method and handler.
fn extract_actix_method_handler(text: &str) -> Option<(String, String)> {
    let methods = ["get", "post", "put", "delete", "patch", "head"];

    for method in methods {
        let pattern = format!("web::{}()", method);
        if text.contains(&pattern) {
            // Find .to(handler) pattern
            if let Some(pos) = text.find(".to(") {
                let after = &text[pos + 4..];
                if let Some(end) = after.find(')') {
                    let handler = after[..end].trim().to_string();
                    return Some((method.to_string(), handler));
                }
            }
        }
    }
    None
}

/// Extract Rocket route from function with attribute like `#[get("/path")]`.
fn extract_rocket_route(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<RustFrameworkRoute> {
    // Look for preceding attribute
    let mut prev = node.prev_sibling();
    while let Some(p) = prev {
        if p.kind() == "attribute_item" {
            let attr_text = parsed.text_for_node(&p);

            // Check for Rocket route macros
            let route_macros = ["#[get(", "#[post(", "#[put(", "#[delete(", "#[patch(", "#[head(", "#[options("];

            for macro_pattern in route_macros {
                if attr_text.starts_with(macro_pattern) {
                    let method = macro_pattern
                        .trim_start_matches("#[")
                        .trim_end_matches('(')
                        .to_uppercase();

                    let path = extract_string_from_attr(&attr_text)?;

                    // Get handler name from function
                    let handler_name = node
                        .child_by_field_name("name")
                        .map(|n| parsed.text_for_node(&n))?;

                    let fn_text = parsed.text_for_node(node);
                    let is_async = fn_text.contains("async fn");

                    return Some(RustFrameworkRoute {
                        method,
                        path,
                        handler_name,
                        is_async,
                        scope_prefix: None,
                        location: parsed.location_for_node(node),
                    });
                }
            }
        }
        prev = p.prev_sibling();
    }
    None
}

/// Extract string from Rocket attribute like `#[get("/path")]`.
fn extract_string_from_attr(attr_text: &str) -> Option<String> {
    let start = attr_text.find('"')?;
    let rest = &attr_text[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Extract Warp route from filter chain like `warp::path("users").and(warp::get()).and_then(handler)`.
fn extract_warp_route(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<RustFrameworkRoute> {
    let text = parsed.text_for_node(node);

    // Warp uses filter combinators
    // warp::path("users").and(warp::get()).and_then(list_users)

    if !text.contains("warp::") {
        return None;
    }

    // Try to extract path
    let path = if text.contains("warp::path(") {
        extract_string_arg(&text, "warp::path(")
            .map(|p| format!("/{}", p))
    } else if text.contains("warp::path!") {
        // warp::path!("users" / "all") style
        extract_warp_path_macro(&text)
    } else {
        None
    }?;

    // Extract method
    let method = if text.contains("warp::get()") {
        "GET"
    } else if text.contains("warp::post()") {
        "POST"
    } else if text.contains("warp::put()") {
        "PUT"
    } else if text.contains("warp::delete()") {
        "DELETE"
    } else if text.contains("warp::patch()") {
        "PATCH"
    } else {
        "ANY"
    };

    // Extract handler from .and_then() or .map()
    let handler = extract_warp_handler(&text)?;

    Some(RustFrameworkRoute {
        method: method.to_string(),
        path,
        handler_name: handler,
        is_async: true,
        scope_prefix: None,
        location: parsed.location_for_node(node),
    })
}

/// Extract path from warp::path! macro.
fn extract_warp_path_macro(text: &str) -> Option<String> {
    if let Some(pos) = text.find("warp::path!(") {
        let after = &text[pos + 12..];
        if let Some(end) = after.find(')') {
            let path_parts = &after[..end];
            // Convert "users" / "all" to /users/all
            let path = path_parts
                .split('/')
                .map(|p| p.trim().trim_matches('"'))
                .collect::<Vec<_>>()
                .join("/");
            return Some(format!("/{}", path));
        }
    }
    None
}

/// Extract handler from Warp filter chain.
fn extract_warp_handler(text: &str) -> Option<String> {
    if let Some(pos) = text.find(".and_then(") {
        let after = &text[pos + 10..];
        if let Some(end) = after.find(')') {
            return Some(after[..end].trim().to_string());
        }
    }
    if let Some(pos) = text.find(".map(") {
        let after = &text[pos + 5..];
        if let Some(end) = after.find(')') {
            return Some(after[..end].trim().to_string());
        }
    }
    None
}

/// Extract Poem route from Route::at() or similar.
fn extract_poem_route(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<RustFrameworkRoute> {
    let text = parsed.text_for_node(node);

    // Poem patterns:
    // Route::new().at("/users", get(handler))
    // .at("/users/:id", get(get_user).post(create_user))

    if !text.contains(".at(") {
        return None;
    }

    let path = extract_string_arg(&text, ".at(")?;

    // Extract method and handler (similar to Axum)
    let (method, handler) = extract_axum_method_handler(&text)?;

    Some(RustFrameworkRoute {
        method: method.to_uppercase(),
        path,
        handler_name: handler,
        is_async: true,
        scope_prefix: None,
        location: parsed.location_for_node(node),
    })
}

/// Extract Tide route from app.at().
fn extract_tide_route(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<RustFrameworkRoute> {
    let text = parsed.text_for_node(node);

    // Tide patterns:
    // app.at("/users").get(list_users)
    // app.at("/users/:id").get(get_user).post(update_user)

    if !text.contains(".at(") {
        return None;
    }

    let path = extract_string_arg(&text, ".at(")?;

    // Extract method
    let methods = ["get", "post", "put", "delete", "patch"];
    for method in methods {
        let pattern = format!(".{}(", method);
        if let Some(pos) = text.find(&pattern) {
            let after = &text[pos + pattern.len()..];
            if let Some(end) = after.find(')') {
                let handler = after[..end].trim().to_string();
                return Some(RustFrameworkRoute {
                    method: method.to_uppercase(),
                    path,
                    handler_name: handler,
                    is_async: true,
                    scope_prefix: None,
                    location: parsed.location_for_node(node),
                });
            }
        }
    }

    None
}

/// Extract first string argument from a pattern like `.method("value"`.
fn extract_string_arg(text: &str, pattern: &str) -> Option<String> {
    let pos = text.find(pattern)?;
    let after = &text[pos + pattern.len()..];

    // Find opening quote
    let quote_start = after.find('"')?;
    let rest = &after[quote_start + 1..];

    // Find closing quote
    let quote_end = rest.find('"')?;

    Some(rest[..quote_end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::rust::parse_rust_file;
    use crate::types::context::{Language, SourceFile};

    fn parse_and_extract(source: &str) -> RustFrameworkSummary {
        let sf = SourceFile {
            path: "test.rs".to_string(),
            language: Language::Rust,
            content: source.to_string(),
        };
        let parsed = parse_rust_file(FileId(1), &sf).expect("parsing should succeed");
        extract_rust_routes(&parsed)
    }

    #[test]
    fn detects_axum_framework() {
        let src = r#"
use axum::{Router, routing::get};

async fn handler() -> &'static str { "Hello" }

fn main() {
    let app = Router::new().route("/", get(handler));
}
"#;
        let summary = parse_and_extract(src);
        assert_eq!(summary.framework, Some(RustFrameworkType::Axum));
    }

    #[test]
    fn extracts_axum_route() {
        let src = r#"
use axum::{Router, routing::get};

async fn list_users() -> String { String::new() }

fn main() {
    let app = Router::new().route("/users", get(list_users));
}
"#;
        let summary = parse_and_extract(src);
        assert_eq!(summary.routes.len(), 1);
        assert_eq!(summary.routes[0].method, "GET");
        assert_eq!(summary.routes[0].path, "/users");
        assert_eq!(summary.routes[0].handler_name, "list_users");
    }

    #[test]
    fn detects_actix_web_framework() {
        let src = r#"
use actix_web::{web, App, HttpServer};

async fn handler() -> impl Responder { "Hello" }
"#;
        let summary = parse_and_extract(src);
        assert_eq!(summary.framework, Some(RustFrameworkType::ActixWeb));
    }

    #[test]
    fn detects_rocket_framework() {
        let src = r#"
use rocket::get;

#[get("/")]
fn index() -> &'static str { "Hello" }
"#;
        let summary = parse_and_extract(src);
        assert_eq!(summary.framework, Some(RustFrameworkType::Rocket));
    }

    #[test]
    fn extracts_rocket_route() {
        let src = r#"
use rocket::get;

#[get("/users")]
fn list_users() -> String { String::new() }
"#;
        let summary = parse_and_extract(src);
        assert_eq!(summary.routes.len(), 1);
        assert_eq!(summary.routes[0].method, "GET");
        assert_eq!(summary.routes[0].path, "/users");
        assert_eq!(summary.routes[0].handler_name, "list_users");
    }

    #[test]
    fn detects_warp_framework() {
        let src = r#"
use warp::Filter;

fn main() {
    let routes = warp::path("users").and(warp::get()).and_then(handler);
}
"#;
        let summary = parse_and_extract(src);
        assert_eq!(summary.framework, Some(RustFrameworkType::Warp));
    }

    #[test]
    fn no_framework_for_plain_rust() {
        let src = r#"
fn main() {
    println!("Hello, world!");
}
"#;
        let summary = parse_and_extract(src);
        assert!(summary.framework.is_none());
        assert!(summary.routes.is_empty());
    }

    #[test]
    fn extracts_axum_layer() {
        let src = r#"
use axum::{Router, routing::get};
use tower_http::trace::TraceLayer;

fn main() {
    let app = Router::new()
        .route("/", get(handler))
        .layer(TraceLayer::new_for_http());
}
"#;
        let summary = parse_and_extract(src);
        assert!(!summary.middleware.is_empty());
        assert!(summary.middleware[0].name.contains("TraceLayer"));
    }
}
