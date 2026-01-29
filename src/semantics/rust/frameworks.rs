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

    /// Start byte offset
    pub start_byte: usize,

    /// End byte offset
    pub end_byte: usize,
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

    // Detect framework from AST (prefer over raw text heuristics)
    summary.framework = detect_framework(parsed);

    if summary.framework.is_none() {
        return summary;
    }

    // Walk AST to find route registrations
    let root = parsed.tree.root_node();
    walk_for_routes(root, parsed, &mut summary);

    // Post-process Rocket mounts: mount prefix + routes![...] mapping.
    if summary.framework == Some(RustFrameworkType::Rocket) {
        apply_rocket_mount_prefixes(parsed, &mut summary);
    }

    summary
}

fn apply_rocket_mount_prefixes(parsed: &ParsedFile, summary: &mut RustFrameworkSummary) {
    let root = parsed.tree.root_node();
    let mounts = collect_rocket_mounts(parsed, root);
    if mounts.is_empty() {
        return;
    }

    for route in &mut summary.routes {
        if let Some(prefix) = mounts.get(&route.handler_name) {
            route.scope_prefix = Some(prefix.clone());
            route.path = join_paths(prefix, &route.path);
        }
    }
}

fn collect_rocket_mounts(
    parsed: &ParsedFile,
    node: tree_sitter::Node,
) -> std::collections::HashMap<String, String> {
    use std::collections::HashMap;
    let mut out: HashMap<String, String> = HashMap::new();

    fn walk(parsed: &ParsedFile, node: tree_sitter::Node, out: &mut HashMap<String, String>) {
        if node.kind() == "call_expression" {
            // IMPORTANT: `parsed.text_for_node(&node)` may include the *entire* method-call chain
            // (e.g. `rocket::build().mount(...).mount(...)`). Avoid string-searching.
            let Some(func) = node.child_by_field_name("function") else {
                return;
            };
            if func.kind() != "field_expression" {
                return;
            }
            let Some(field) = func.child_by_field_name("field") else {
                return;
            };
            if parsed.text_for_node(&field) != "mount" {
                return;
            }

            let Some(args) = node.child_by_field_name("arguments") else {
                return;
            };
            if args.named_child_count() < 2 {
                return;
            }

            let Some(prefix_arg) = args.named_child(0) else {
                return;
            };
            let Some(routes_arg) = args.named_child(1) else {
                return;
            };

            let prefix = match extract_string_literal(&parsed.text_for_node(&prefix_arg)) {
                Some(p) => p,
                None => return,
            };
            let routes_text = parsed.text_for_node(&routes_arg);
            if !routes_text.contains("routes![") {
                return;
            }
            for name in extract_rocket_routes_list(&routes_text) {
                out.insert(name, prefix.clone());
            }
        }

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                walk(parsed, child, out);
            }
        }
    }

    walk(parsed, node, &mut out);
    out
}

fn extract_rocket_routes_list(text: &str) -> Vec<String> {
    // Extract identifiers in routes![a, b, c]
    let Some(start) = text.find("routes![") else {
        return Vec::new();
    };
    let after = &text[start + 8..];
    let Some(end) = after.find(']') else {
        return Vec::new();
    };
    let inner = &after[..end];
    inner
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.split("::").last().unwrap_or(s).trim().to_string())
        .collect()
}

/// Detect which Rust HTTP framework is being used based on imports.
fn detect_framework(parsed: &ParsedFile) -> Option<RustFrameworkType> {
    let root = parsed.tree.root_node();

    let mut has_axum = false;
    let mut has_actix = false;
    let mut has_rocket = false;
    let mut has_warp = false;
    let mut has_poem = false;
    let mut has_tide = false;

    fn mark_from_text(
        text: &str,
        has_axum: &mut bool,
        has_actix: &mut bool,
        has_rocket: &mut bool,
        has_warp: &mut bool,
        has_poem: &mut bool,
        has_tide: &mut bool,
    ) {
        let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        if t.contains("axum::") {
            *has_axum = true;
        }
        if t.contains("actix_web::") {
            *has_actix = true;
        }
        if t.contains("warp::") {
            *has_warp = true;
        }
        if t.contains("poem::") {
            *has_poem = true;
        }
        if t.contains("tide::") {
            *has_tide = true;
        }

        // Rocket is trickier because `#[get("/")]` is also common in actix-web.
        // Only treat it as Rocket if we see Rocket namespacing or launch/build usage.
        if t.contains("rocket::") || t.contains("#[rocket::") || t.contains("#[launch]") {
            *has_rocket = true;
        }
    }

    fn walk(
        parsed: &ParsedFile,
        node: tree_sitter::Node,
        has_axum: &mut bool,
        has_actix: &mut bool,
        has_rocket: &mut bool,
        has_warp: &mut bool,
        has_poem: &mut bool,
        has_tide: &mut bool,
    ) {
        if super::is_inline_test_subtree_root(parsed, &node) {
            return;
        }

        match node.kind() {
            "use_declaration" | "scoped_identifier" | "path_expression" | "attribute_item" => {
                mark_from_text(
                    &parsed.text_for_node(&node),
                    has_axum,
                    has_actix,
                    has_rocket,
                    has_warp,
                    has_poem,
                    has_tide,
                );
            }
            _ => {}
        }

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                walk(
                    parsed, child, has_axum, has_actix, has_rocket, has_warp, has_poem, has_tide,
                );
            }
        }
    }

    walk(
        parsed,
        root,
        &mut has_axum,
        &mut has_actix,
        &mut has_rocket,
        &mut has_warp,
        &mut has_poem,
        &mut has_tide,
    );

    // Choose in order of popularity.
    if has_axum {
        return Some(RustFrameworkType::Axum);
    }
    if has_actix {
        return Some(RustFrameworkType::ActixWeb);
    }
    if has_rocket {
        return Some(RustFrameworkType::Rocket);
    }
    if has_warp {
        return Some(RustFrameworkType::Warp);
    }
    if has_poem {
        return Some(RustFrameworkType::Poem);
    }
    if has_tide {
        return Some(RustFrameworkType::Tide);
    }

    None
}

/// Walk AST to find route registrations.
fn walk_for_routes(
    node: tree_sitter::Node,
    parsed: &ParsedFile,
    summary: &mut RustFrameworkSummary,
) {
    let framework = match &summary.framework {
        Some(f) => f.clone(),
        None => return,
    };

    if super::is_inline_test_subtree_root(parsed, &node) {
        return;
    }

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
        "call_expression" => match framework {
            RustFrameworkType::Axum => {
                if let Some(route) = extract_axum_route(parsed, &node) {
                    summary.routes.push(route);
                }
                if let Some(route) = extract_axum_route_service(parsed, &node) {
                    summary.routes.push(route);
                }
                if let Some(route) = extract_axum_nest_service(parsed, &node) {
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
        },
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
    let func = node.child_by_field_name("function")?;
    if func.kind() != "field_expression" {
        return None;
    }
    let field = func.child_by_field_name("field")?;
    if parsed.text_for_node(&field) != "route" {
        return None;
    }

    // Extract arguments directly from the AST.
    //
    // IMPORTANT: `parsed.text_for_node(node)` may include the *entire* method-call chain
    // (e.g. `Router::new().route(...).route(...).with_state(...)`).
    // String-searching that text will repeatedly match the first `.route(`.
    let args = node.child_by_field_name("arguments")?;
    let path_arg = args.named_child(0)?;
    let mut path = extract_string_literal(&parsed.text_for_node(&path_arg))?;

    let method_handler_arg = args.named_child(1)?;
    let (method, handler) = extract_axum_method_handler_from_node(parsed, &method_handler_arg)?;

    // If this .route call is nested under `.nest("/prefix", ...)`, apply the prefix.
    if let Some(prefix) = find_enclosing_nest_prefix(parsed, node) {
        path = join_paths(&prefix, &path);
    }

    Some(RustFrameworkRoute {
        method: method.to_uppercase(),
        path,
        handler_name: handler,
        is_async: true, // Axum handlers are always async
        scope_prefix: find_enclosing_nest_prefix(parsed, node),
        location: parsed.location_for_node(node),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

/// Extract Axum method and handler from the second argument of `.route(...)`.
///
/// Supports patterns like `get(handler)` and `axum::routing::get(handler)`.
fn extract_axum_method_handler_from_node(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
) -> Option<(String, String)> {
    let methods = [
        "get", "post", "put", "delete", "patch", "head", "options", "trace",
    ];

    if node.kind() != "call_expression" {
        return None;
    }

    let call_func = node.child_by_field_name("function")?;
    let call_func_text = parsed.text_for_node(&call_func);
    let method = extract_last_ident(&call_func_text)?;
    if !methods.contains(&method.as_str()) {
        return None;
    }

    let args = node.child_by_field_name("arguments")?;
    let handler_arg = args.named_child(0)?;
    let handler_text = parsed.text_for_node(&handler_arg);
    let handler = extract_last_ident(&handler_text)?;
    Some((method, handler))
}

/// Extract the inner string content for a Rust string literal.
///
/// Supports normal string literals (`"/path"`) and raw string literals (`r"/path"`, `r#"/path"#`).
fn extract_string_literal(text: &str) -> Option<String> {
    let s = text.trim();
    if s.is_empty() {
        return None;
    }
    // Find first quote and last quote.
    let start = s.find('"')?;
    let end = s.rfind('"')?;
    if end <= start {
        return None;
    }
    Some(s[start + 1..end].to_string())
}

/// Extract the last identifier-like token from a Rust expression string.
///
/// Examples:
/// - `axum::routing::get` -> `get`
/// - `handlers::list_users` -> `list_users`
/// - `list_users` -> `list_users`
fn extract_last_ident(text: &str) -> Option<String> {
    let s = text.trim();
    if s.is_empty() {
        return None;
    }

    let mut end = None;
    for (i, ch) in s.char_indices().rev() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            end = Some(i + ch.len_utf8());
            break;
        }
    }
    let end = end?;

    let mut start = 0;
    for (i, ch) in s[..end].char_indices().rev() {
        if !(ch.is_ascii_alphanumeric() || ch == '_') {
            start = i + ch.len_utf8();
            break;
        }
    }
    let ident = &s[start..end];
    if ident.is_empty() {
        None
    } else {
        Some(ident.to_string())
    }
}

/// Extract Axum layer/middleware from `.layer()` call.
fn extract_axum_layer(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<RustMiddlewareInfo> {
    let text = parsed.text_for_node(node);

    if !text.contains(".layer(") {
        return None;
    }

    // Extract layer name
    let name = extract_string_arg(&text, ".layer(").or_else(|| extract_type_from_layer(&text))?;

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
fn extract_actix_route(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
) -> Option<RustFrameworkRoute> {
    let func = node.child_by_field_name("function")?;
    if func.kind() != "field_expression" {
        return None;
    }
    let field = func.child_by_field_name("field")?;
    if parsed.text_for_node(&field) != "route" {
        return None;
    }

    // Extract arguments directly from AST.
    //
    // IMPORTANT: `parsed.text_for_node(node)` can include the entire chained expression, so
    // string-searching it will repeatedly match the first `.route(`.
    let args = node.child_by_field_name("arguments")?;
    let arg_count = args.named_child_count();

    // Actix handler expression is either:
    // - `.route("/path", web::get().to(handler))` (2 args)
    // - `web::resource("/path").route(web::get().to(handler))` (1 arg)
    let (path, handler_expr) = if arg_count >= 2 {
        let path_arg = args.named_child(0)?;
        let handler_arg = args.named_child(1)?;
        (
            extract_string_literal(&parsed.text_for_node(&path_arg))?,
            handler_arg,
        )
    } else if arg_count == 1 {
        let handler_arg = args.named_child(0)?;

        // For resource routes, get the path from the receiver call (web::resource("/path")).
        let receiver = func
            .child_by_field_name("value")
            .or_else(|| func.child_by_field_name("object"))
            .or_else(|| func.child_by_field_name("argument"))?;
        if receiver.kind() != "call_expression" {
            return None;
        }
        let recv_args = receiver.child_by_field_name("arguments")?;
        let recv_path_arg = recv_args.named_child(0)?;
        let recv_path = extract_string_literal(&parsed.text_for_node(&recv_path_arg))?;
        (recv_path, handler_arg)
    } else {
        return None;
    };

    // Method+handler lives in the handler expression.
    let handler_text = parsed.text_for_node(&handler_expr);
    let (method, handler) = extract_actix_method_handler(&handler_text)?;

    // Apply scope prefix if we find `web::scope("/prefix")` in the receiver/ancestor chain.
    let scope_prefix = find_enclosing_actix_scope_prefix(parsed, node);
    let full_path = if let Some(ref pfx) = scope_prefix {
        join_paths(pfx, &path)
    } else {
        path
    };

    Some(RustFrameworkRoute {
        method: method.to_uppercase(),
        path: full_path,
        handler_name: handler,
        is_async: true,
        scope_prefix,
        location: parsed.location_for_node(node),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

fn extract_axum_route_service(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
) -> Option<RustFrameworkRoute> {
    // Match `.route_service("/path", service)`.
    let func = node.child_by_field_name("function")?;
    if func.kind() != "field_expression" {
        return None;
    }
    let field = func.child_by_field_name("field")?;
    if parsed.text_for_node(&field) != "route_service" {
        return None;
    }

    let args = node.child_by_field_name("arguments")?;
    let path_arg = args.named_child(0)?;
    let mut path = extract_string_literal(&parsed.text_for_node(&path_arg))?;

    if let Some(prefix) = find_enclosing_nest_prefix(parsed, node) {
        path = join_paths(&prefix, &path);
        return Some(RustFrameworkRoute {
            method: "ANY".to_string(),
            path,
            handler_name: extract_second_arg_handler_name(parsed, node)
                .unwrap_or_else(|| "service".to_string()),
            is_async: true,
            scope_prefix: Some(prefix),
            location: parsed.location_for_node(node),
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
        });
    }

    Some(RustFrameworkRoute {
        method: "ANY".to_string(),
        path,
        handler_name: extract_second_arg_handler_name(parsed, node)
            .unwrap_or_else(|| "service".to_string()),
        is_async: true,
        scope_prefix: None,
        location: parsed.location_for_node(node),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

fn extract_axum_nest_service(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
) -> Option<RustFrameworkRoute> {
    // Match `.nest_service("/prefix", svc)`.
    let func = node.child_by_field_name("function")?;
    if func.kind() != "field_expression" {
        return None;
    }
    let field = func.child_by_field_name("field")?;
    if parsed.text_for_node(&field) != "nest_service" {
        return None;
    }

    let args = node.child_by_field_name("arguments")?;
    let prefix_arg = args.named_child(0)?;
    let mut prefix = extract_string_literal(&parsed.text_for_node(&prefix_arg))?;

    if let Some(outer) = find_enclosing_nest_prefix(parsed, node) {
        prefix = join_paths(&outer, &prefix);
        return Some(RustFrameworkRoute {
            method: "ANY".to_string(),
            path: prefix.clone(),
            handler_name: extract_second_arg_handler_name(parsed, node)
                .unwrap_or_else(|| "service".to_string()),
            is_async: true,
            scope_prefix: Some(outer),
            location: parsed.location_for_node(node),
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
        });
    }

    Some(RustFrameworkRoute {
        method: "ANY".to_string(),
        path: prefix,
        handler_name: extract_second_arg_handler_name(parsed, node)
            .unwrap_or_else(|| "service".to_string()),
        is_async: true,
        scope_prefix: None,
        location: parsed.location_for_node(node),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

fn extract_second_arg_handler_name(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
) -> Option<String> {
    let args = node.child_by_field_name("arguments")?;
    let second = args.named_child(1)?;
    match second.kind() {
        "identifier" => Some(parsed.text_for_node(&second)),
        "path_expression" | "scoped_identifier" => Some(
            parsed
                .text_for_node(&second)
                .split("::")
                .last()?
                .to_string(),
        ),
        _ => {
            // Best-effort: try inner call like get_service(handler)
            if second.kind() == "call_expression" {
                let text = parsed.text_for_node(&second);
                if let Some(start) = text.find('(') {
                    let after = &text[start + 1..];
                    if let Some(end) = after.find(')') {
                        let inner = after[..end].trim();
                        if !inner.is_empty() {
                            return Some(inner.split("::").last().unwrap_or(inner).to_string());
                        }
                    }
                }
            }
            None
        }
    }
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
fn extract_rocket_route(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
) -> Option<RustFrameworkRoute> {
    // Look for immediately preceding attribute items.
    let mut prev = node.prev_sibling();
    while let Some(p) = prev {
        if p.kind() == "attribute_item" {
            let attr_text = parsed.text_for_node(&p);

            // Check for Rocket route macros
            let route_macros = [
                "#[get(",
                "#[post(",
                "#[put(",
                "#[delete(",
                "#[patch(",
                "#[head(",
                "#[options(",
            ];

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
                        start_byte: node.start_byte(),
                        end_byte: node.end_byte(),
                    });
                }
            }

            prev = p.prev_sibling();
            continue;
        }

        if p.is_named() {
            break;
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
        extract_string_arg(&text, "warp::path(").map(|p| format!("/{}", p))
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
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
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
    // Poem patterns:
    // - Route::new().at("/users", get(handler))
    // - .at("/users/:id", get(get_user).post(create_user))
    //
    // We build one route per method call (get/post/...) and resolve the path
    // from the enclosing `.at("/path", ...)` call.

    let func = node.child_by_field_name("function")?;
    let method = extract_last_ident(&parsed.text_for_node(&func))?;
    let methods = [
        "get", "post", "put", "delete", "patch", "head", "options", "trace",
    ];
    if !methods.contains(&method.as_str()) {
        return None;
    }

    let args = node.child_by_field_name("arguments")?;
    let handler_arg = args.named_child(0)?;
    let handler_text = parsed.text_for_node(&handler_arg);
    let handler = extract_last_ident(&handler_text).unwrap_or(handler_text);

    let path = find_enclosing_field_call_path(parsed, node, "at")?;

    Some(RustFrameworkRoute {
        method: method.to_uppercase(),
        path,
        handler_name: handler,
        is_async: true,
        scope_prefix: None,
        location: parsed.location_for_node(node),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

/// Extract Tide route from app.at().
fn extract_tide_route(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<RustFrameworkRoute> {
    // Tide patterns:
    // - app.at("/users").get(list_users)
    // - app.at("/users/:id").get(get_user).post(update_user)
    //
    // We build one route per method call (get/post/...) and resolve the path
    // from the enclosing `.at("/path")` call.

    let func = node.child_by_field_name("function")?;
    let method = extract_last_ident(&parsed.text_for_node(&func))?;
    let methods = ["get", "post", "put", "delete", "patch"];
    if !methods.contains(&method.as_str()) {
        return None;
    }

    let args = node.child_by_field_name("arguments")?;
    let handler_arg = args.named_child(0)?;
    let handler_text = parsed.text_for_node(&handler_arg);
    let handler = extract_last_ident(&handler_text).unwrap_or(handler_text);

    let path = find_enclosing_field_call_path(parsed, node, "at")?;

    Some(RustFrameworkRoute {
        method: method.to_uppercase(),
        path,
        handler_name: handler,
        is_async: true,
        scope_prefix: None,
        location: parsed.location_for_node(node),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

fn find_enclosing_field_call_path(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    field_name: &str,
) -> Option<String> {
    // Receiver chain: e.g. `app.at("/x").get(handler)` (at() is the receiver call).
    let mut cur_call = *node;
    loop {
        let Some(func) = cur_call.child_by_field_name("function") else {
            break;
        };
        if func.kind() != "field_expression" {
            break;
        }
        let receiver = func
            .child_by_field_name("value")
            .or_else(|| func.child_by_field_name("object"))
            .or_else(|| func.child_by_field_name("argument"));
        let Some(receiver) = receiver else {
            break;
        };
        if receiver.kind() != "call_expression" {
            break;
        }

        if let Some(recv_func) = receiver.child_by_field_name("function") {
            if recv_func.kind() == "field_expression" {
                if let Some(field) = recv_func.child_by_field_name("field") {
                    if parsed.text_for_node(&field) == field_name {
                        let args = receiver.child_by_field_name("arguments")?;
                        let arg0 = args.named_child(0)?;
                        return extract_string_literal(&parsed.text_for_node(&arg0));
                    }
                }
            }
        }

        cur_call = receiver;
    }

    // Ancestor chain: e.g. `.at("/x", get(handler))` (at() is an ancestor call).
    let mut cur = node.parent();
    while let Some(n) = cur {
        if n.kind() == "call_expression" {
            if let Some(func) = n.child_by_field_name("function") {
                if func.kind() == "field_expression" {
                    if let Some(field) = func.child_by_field_name("field") {
                        if parsed.text_for_node(&field) == field_name {
                            let args = n.child_by_field_name("arguments")?;
                            let arg0 = args.named_child(0)?;
                            return extract_string_literal(&parsed.text_for_node(&arg0));
                        }
                    }
                }
            }
        }
        cur = n.parent();
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

fn find_enclosing_nest_prefix(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<String> {
    // Find the nearest ancestor call expression like `.nest("/prefix", ...)` or
    // `.nest_service("/prefix", ...)`.
    //
    // IMPORTANT: avoid parsing `text_for_node` for chained expressions.
    let mut cur = node.parent();
    while let Some(n) = cur {
        if n.kind() == "call_expression" {
            let Some(func) = n.child_by_field_name("function") else {
                cur = n.parent();
                continue;
            };
            if func.kind() == "field_expression" {
                let Some(field) = func.child_by_field_name("field") else {
                    cur = n.parent();
                    continue;
                };
                let field_name = parsed.text_for_node(&field);
                if field_name == "nest" || field_name == "nest_service" {
                    let args = n.child_by_field_name("arguments")?;
                    let arg0 = args.named_child(0)?;
                    return extract_string_literal(&parsed.text_for_node(&arg0));
                }
            }
        }
        cur = n.parent();
    }
    None
}

fn find_enclosing_actix_scope_prefix(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
) -> Option<String> {
    // Find the nearest `web::scope("/prefix")` call in either:
    // - the receiver chain (e.g. `web::scope("/api").route(...)`), or
    // - the ancestor chain (e.g. `.service(web::scope("/api").service(...))`).

    fn scope_from_receiver_chain(parsed: &ParsedFile, start: tree_sitter::Node) -> Option<String> {
        let mut cur_call = start;
        loop {
            let Some(func) = cur_call.child_by_field_name("function") else {
                break;
            };
            if func.kind() != "field_expression" {
                break;
            }
            let receiver = func
                .child_by_field_name("value")
                .or_else(|| func.child_by_field_name("object"))
                .or_else(|| func.child_by_field_name("argument"));
            let Some(receiver) = receiver else {
                break;
            };
            if receiver.kind() != "call_expression" {
                break;
            }

            let recv_func = receiver.child_by_field_name("function")?;
            let recv_func_text = parsed.text_for_node(&recv_func);
            let recv_ident = extract_last_ident(&recv_func_text);
            if recv_ident.as_deref() == Some("scope") && recv_func_text.contains("web::scope") {
                let args = receiver.child_by_field_name("arguments")?;
                let arg0 = args.named_child(0)?;
                return extract_string_literal(&parsed.text_for_node(&arg0));
            }

            cur_call = receiver;
        }
        None
    }

    // Check this call's receiver chain.
    if let Some(pfx) = scope_from_receiver_chain(parsed, *node) {
        return Some(pfx);
    }

    // Walk ancestors; check both the ancestor itself and its receiver chain.
    let mut cur = node.parent();
    while let Some(n) = cur {
        if n.kind() == "call_expression" {
            let func = n.child_by_field_name("function")?;
            let func_text = parsed.text_for_node(&func);
            let ident = extract_last_ident(&func_text);
            if ident.as_deref() == Some("scope") && func_text.contains("web::scope") {
                let args = n.child_by_field_name("arguments")?;
                let arg0 = args.named_child(0)?;
                return extract_string_literal(&parsed.text_for_node(&arg0));
            }
            if let Some(pfx) = scope_from_receiver_chain(parsed, n) {
                return Some(pfx);
            }
        }
        cur = n.parent();
    }

    None
}

fn join_paths(prefix: &str, path: &str) -> String {
    let mut pfx = prefix.trim().to_string();
    let mut p = path.trim().to_string();

    if pfx.is_empty() {
        pfx = "/".to_string();
    }
    if p.is_empty() {
        p = "/".to_string();
    }
    if !pfx.starts_with('/') {
        pfx = format!("/{}", pfx);
    }
    if !p.starts_with('/') {
        p = format!("/{}", p);
    }
    if pfx == "/" {
        return p;
    }
    if p == "/" {
        return pfx;
    }
    format!("{}{}", pfx.trim_end_matches('/'), p)
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
    fn detects_axum_framework_without_use_stmt() {
        let src = r#"
async fn handler() -> &'static str { "Hello" }

fn main() {
    let app = axum::Router::new().route("/", axum::routing::get(handler));
    let _ = app;
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
    fn extracts_axum_nested_route_with_prefix() {
        let src = r#"
use axum::{Router, routing::get};

async fn list_users() -> String { String::new() }

fn main() {
    let app = Router::new().nest("/api", Router::new().route("/users", get(list_users)));
}
"#;
        let summary = parse_and_extract(src);
        assert_eq!(summary.routes.len(), 1);
        assert_eq!(summary.routes[0].path, "/api/users");
        assert_eq!(summary.routes[0].scope_prefix, Some("/api".to_string()));
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
    fn detects_actix_web_framework_without_use_stmt() {
        let src = r#"
fn main() {
    let _app = actix_web::App::new();
}
"#;
        let summary = parse_and_extract(src);
        assert_eq!(summary.framework, Some(RustFrameworkType::ActixWeb));
    }

    #[test]
    fn extracts_actix_scope_prefix_routes() {
        let src = r#"
use actix_web::{web, App};

async fn list_users() -> String { String::new() }

fn main() {
    let _app = App::new().service(
        web::scope("/api").route("/users", web::get().to(list_users))
    );
}
"#;
        let summary = parse_and_extract(src);
        assert_eq!(summary.framework, Some(RustFrameworkType::ActixWeb));
        assert_eq!(summary.routes.len(), 1);
        assert_eq!(summary.routes[0].path, "/api/users");
        assert_eq!(summary.routes[0].scope_prefix, Some("/api".to_string()));
    }

    #[test]
    fn extracts_actix_chained_routes() {
        let src = r#"
use actix_web::{web, App};

async fn list_users() -> String { String::new() }
async fn create_user() -> String { String::new() }

fn main() {
    let _app = App::new()
        .route("/users", web::get().to(list_users))
        .route("/users", web::post().to(create_user));
}
"#;
        let summary = parse_and_extract(src);
        assert_eq!(summary.framework, Some(RustFrameworkType::ActixWeb));
        assert_eq!(summary.routes.len(), 2);

        assert!(
            summary
                .routes
                .iter()
                .any(|r| r.method == "GET" && r.path == "/users" && r.handler_name == "list_users")
        );
        assert!(
            summary.routes.iter().any(|r| r.method == "POST"
                && r.path == "/users"
                && r.handler_name == "create_user")
        );
    }

    #[test]
    fn extracts_actix_scope_prefix_routes_inside_service_resource() {
        let src = r#"
use actix_web::{web, App};

async fn list_users() -> String { String::new() }

fn main() {
    let _app = App::new().service(
        web::scope("/api").service(
            web::resource("/users").route(web::get().to(list_users))
        )
    );
}
"#;
        let summary = parse_and_extract(src);
        assert_eq!(summary.framework, Some(RustFrameworkType::ActixWeb));
        assert_eq!(summary.routes.len(), 1);
        assert_eq!(summary.routes[0].path, "/api/users");
        assert_eq!(summary.routes[0].scope_prefix, Some("/api".to_string()));
    }

    #[test]
    fn extracts_axum_route_service() {
        let src = r#"
use axum::Router;

fn svc() {}

fn main() {
    let _app = Router::new().route_service("/static", svc);
}
"#;
        let summary = parse_and_extract(src);
        assert_eq!(summary.framework, Some(RustFrameworkType::Axum));
        assert_eq!(summary.routes.len(), 1);
        assert_eq!(summary.routes[0].method, "ANY");
        assert_eq!(summary.routes[0].path, "/static");
    }

    #[test]
    fn extracts_axum_nested_nest_service_with_prefix() {
        let src = r#"
use axum::Router;

fn svc() {}

fn main() {
    let _app = Router::new().nest("/api", Router::new().nest_service("/static", svc));
}
"#;
        let summary = parse_and_extract(src);
        assert_eq!(summary.framework, Some(RustFrameworkType::Axum));
        assert_eq!(summary.routes.len(), 1);
        assert_eq!(summary.routes[0].method, "ANY");
        assert_eq!(summary.routes[0].path, "/api/static");
        assert_eq!(summary.routes[0].scope_prefix, Some("/api".to_string()));
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
    fn applies_rocket_mount_prefix_to_attribute_routes() {
        let src = r#"
use rocket::get;

#[get("/health")]
fn health() -> &'static str { "ok" }

fn main() {
    rocket::build().mount("/api", routes![health]);
}
"#;
        let summary = parse_and_extract(src);
        assert_eq!(summary.framework, Some(RustFrameworkType::Rocket));
        assert_eq!(summary.routes.len(), 1);
        assert_eq!(summary.routes[0].path, "/api/health");
        assert_eq!(summary.routes[0].scope_prefix, Some("/api".to_string()));
    }

    #[test]
    fn applies_rocket_mount_prefixes_for_chained_mounts() {
        let src = r#"
use rocket::get;

#[get("/health")]
fn health() -> &'static str { "ok" }

#[get("/users")]
fn users() -> &'static str { "ok" }

fn main() {
    rocket::build()
        .mount("/api", routes![health])
        .mount("/v1", routes![users]);
}
"#;
        let summary = parse_and_extract(src);
        assert_eq!(summary.framework, Some(RustFrameworkType::Rocket));
        assert_eq!(summary.routes.len(), 2);

        assert!(
            summary
                .routes
                .iter()
                .any(|r| r.path == "/api/health" && r.scope_prefix.as_deref() == Some("/api"))
        );
        assert!(
            summary
                .routes
                .iter()
                .any(|r| r.path == "/v1/users" && r.scope_prefix.as_deref() == Some("/v1"))
        );
    }

    #[test]
    fn extracts_tide_chained_methods() {
        let src = r#"
use tide::Server;

async fn get_user(_: tide::Request<()>) -> tide::Result { Ok("ok".into()) }
async fn update_user(_: tide::Request<()>) -> tide::Result { Ok("ok".into()) }

fn main() {
    let mut app: Server<()> = tide::new();
    app.at("/users/:id").get(get_user).post(update_user);
}
"#;
        let summary = parse_and_extract(src);
        assert_eq!(summary.framework, Some(RustFrameworkType::Tide));
        assert_eq!(summary.routes.len(), 2);
        assert!(
            summary.routes.iter().any(|r| r.method == "GET"
                && r.path == "/users/:id"
                && r.handler_name == "get_user")
        );
        assert!(summary.routes.iter().any(|r| r.method == "POST"
            && r.path == "/users/:id"
            && r.handler_name == "update_user"));
    }

    #[test]
    fn extracts_poem_methods_under_at() {
        let src = r#"
use poem::{get, post, Route};

async fn list_users() -> String { String::new() }
async fn create_user() -> String { String::new() }

fn main() {
    let _app = Route::new().at("/users", get(list_users).post(create_user));
}
"#;
        let summary = parse_and_extract(src);
        assert_eq!(summary.framework, Some(RustFrameworkType::Poem));
        assert_eq!(summary.routes.len(), 2);
        assert!(
            summary
                .routes
                .iter()
                .any(|r| r.method == "GET" && r.path == "/users" && r.handler_name == "list_users")
        );
        assert!(
            summary.routes.iter().any(|r| r.method == "POST"
                && r.path == "/users"
                && r.handler_name == "create_user")
        );
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
    fn detects_warp_framework_without_use_stmt() {
        let src = r#"
fn main() {
    let routes = warp::path("users").and(warp::get()).and_then(handler);
    let _ = routes;
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

    #[test]
    fn extracts_axum_chained_routes() {
        let src = r#"
use axum::{Router, routing::{get, post}};

async fn index() -> &'static str { "ok" }
async fn get_recipes() -> &'static str { "ok" }
async fn create_recipe() -> &'static str { "ok" }

fn main() {
    let _app = Router::new()
        .route("/", get(index))
        .route("/api/recipes", get(get_recipes))
        .route("/api/recipes", post(create_recipe));
}
"#;
        let summary = parse_and_extract(src);

        assert_eq!(summary.framework, Some(RustFrameworkType::Axum));
        assert_eq!(summary.routes.len(), 3);

        assert!(
            summary
                .routes
                .iter()
                .any(|r| r.method == "GET" && r.path == "/" && r.handler_name == "index")
        );
        assert!(summary.routes.iter().any(|r| {
            r.method == "GET" && r.path == "/api/recipes" && r.handler_name == "get_recipes"
        }));
        assert!(summary.routes.iter().any(|r| {
            r.method == "POST" && r.path == "/api/recipes" && r.handler_name == "create_recipe"
        }));
    }
}
