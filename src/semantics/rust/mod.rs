//! Rust semantic model and analysis.
//!
//! This module provides semantic analysis for Rust source files,
//! extracting information about functions, types, async operations,
//! error handling patterns, and more.

pub mod frameworks;
pub mod http;
pub mod model;

pub use frameworks::{
    RustFrameworkRoute, RustFrameworkSummary, RustFrameworkType, RustMiddlewareInfo, RustRouteScope,
};
pub use model::RustFileSemantics;

use anyhow::Result;

use crate::parse::ast::ParsedFile;
use crate::semantics::common::calls::FunctionCall;
use crate::semantics::common::db::{DbOperation, DbLibrary, DbOperationType};
use crate::semantics::common::CommonLocation;

/// Build the semantic model for a single Rust file.
///
/// This is the entry point the engine will call after parsing.
pub fn build_rust_semantics(parsed: &ParsedFile) -> Result<RustFileSemantics> {
    let mut sem = RustFileSemantics::from_parsed(parsed);
    collect_semantics(parsed, &mut sem);
    analyze_async_patterns(parsed, &mut sem);
    analyze_error_handling(parsed, &mut sem);
    analyze_unsafe_patterns(parsed, &mut sem);
    analyze_frameworks(parsed, &mut sem);
    analyze_http_calls(parsed, &mut sem);
    Ok(sem)
}

/// Analyze HTTP framework usage (Axum, Actix-web, Rocket, Warp, etc.).
fn analyze_frameworks(parsed: &ParsedFile, sem: &mut RustFileSemantics) {
    let summary = frameworks::extract_rust_routes(parsed);
    if summary.has_framework() {
        sem.rust_framework = Some(summary);
    }
}

/// Analyze HTTP client calls (reqwest, ureq, hyper, etc.).
fn analyze_http_calls(parsed: &ParsedFile, sem: &mut RustFileSemantics) {
    let http_calls = http::summarize_http_clients(parsed);
    sem.http_calls = http_calls
        .into_iter()
        .map(|call_site| convert_http_call_site(call_site, parsed))
        .collect();
}

/// Convert Rust-specific HttpCallSite to common HttpCall.
fn convert_http_call_site(site: http::HttpCallSite, _parsed: &ParsedFile) -> crate::semantics::common::http::HttpCall {
    let library = match site.client_kind {
        http::HttpClientKind::Reqwest => crate::semantics::common::http::HttpClientLibrary::Reqwest,
        http::HttpClientKind::ReqwestBlocking => crate::semantics::common::http::HttpClientLibrary::Reqwest,
        http::HttpClientKind::Ureq => crate::semantics::common::http::HttpClientLibrary::Ureq,
        http::HttpClientKind::Hyper => crate::semantics::common::http::HttpClientLibrary::Hyper,
        http::HttpClientKind::Surf => crate::semantics::common::http::HttpClientLibrary::Other("surf".to_string()),
        http::HttpClientKind::Awc => crate::semantics::common::http::HttpClientLibrary::Other("awc".to_string()),
        http::HttpClientKind::Isahc => crate::semantics::common::http::HttpClientLibrary::Other("isahc".to_string()),
        http::HttpClientKind::Other(name) => crate::semantics::common::http::HttpClientLibrary::Other(name),
    };

    let method = match site.method_name.to_lowercase().as_str() {
        "get" => crate::semantics::common::http::HttpMethod::Get,
        "post" => crate::semantics::common::http::HttpMethod::Post,
        "put" => crate::semantics::common::http::HttpMethod::Put,
        "patch" => crate::semantics::common::http::HttpMethod::Patch,
        "delete" => crate::semantics::common::http::HttpMethod::Delete,
        "head" => crate::semantics::common::http::HttpMethod::Head,
        "options" => crate::semantics::common::http::HttpMethod::Options,
        _ => crate::semantics::common::http::HttpMethod::Other(site.method_name),
    };

    let location = CommonLocation {
        file_id: site.location.file_id,
        line: site.location.range.start_line + 1,
        column: site.location.range.start_col + 1,
        start_byte: site.start_byte,
        end_byte: site.end_byte,
    };

    crate::semantics::common::http::HttpCall {
        library,
        method,
        url: None,
        has_timeout: site.has_timeout,
        timeout_value: site.timeout_value,
        retry_mechanism: None,
        call_text: site.call_text,
        location,
        enclosing_function: site.function_name,
        in_async_context: site.in_async_function,
        in_loop: false,
        start_byte: site.start_byte,
        end_byte: site.end_byte,
    }
}

/// Context for tracking state during AST traversal.
#[derive(Default, Clone)]
struct TraversalContext {
    in_loop: bool,
    in_async_fn: bool,
    in_test: bool,
    in_main: bool,
    in_closure: bool,
    in_unsafe: bool,
    /// Whether we're inside a static/const initializer with LazyLock, OnceLock, or similar.
    /// This is the correct pattern for compile-once initialization.
    in_static_init: bool,
    /// Whether we're inside an impl block (methods should go to impl.methods, not sem.functions)
    in_impl: bool,
    current_function: Option<String>,
    current_qualified_name: Option<String>,
}

/// Collect basic semantics by walking the tree-sitter AST.
fn collect_semantics(parsed: &ParsedFile, sem: &mut RustFileSemantics) {
    let root = parsed.tree.root_node();
    let ctx = TraversalContext::default();
    walk_nodes(root, parsed, sem, ctx);
}

/// Walk nodes and collect semantic information.
fn walk_nodes(
    node: tree_sitter::Node,
    parsed: &ParsedFile,
    sem: &mut RustFileSemantics,
    ctx: TraversalContext,
) {
    // Update context based on current node
    let new_ctx = update_context(&node, parsed, &ctx);

    // Process current node
    match node.kind() {
        "use_declaration" => {
            if let Some(use_stmt) = build_use(parsed, &node) {
                // Check if this enables tokio or async-std
                if use_stmt.path.contains("tokio") {
                    sem.async_info.uses_tokio = true;
                }
                if use_stmt.path.contains("async_std") {
                    sem.async_info.uses_async_std = true;
                }
                sem.uses.push(use_stmt);
            }
        }
        "function_item" => {
            // Only add to sem.functions if NOT inside an impl block
            // (methods in impl blocks are handled separately in build_impl)
            if !ctx.in_impl {
                if let Some(func) = build_function(parsed, &node, &new_ctx) {
                    if func.is_async {
                        sem.async_info.async_fn_count += 1;
                    }
                    sem.functions.push(func);
                }
            }
        }
        "struct_item" => {
            if let Some(s) = build_struct(parsed, &node) {
                sem.structs.push(s);
            }
        }
        "enum_item" => {
            if let Some(e) = build_enum(parsed, &node) {
                sem.enums.push(e);
            }
        }
        "trait_item" => {
            if let Some(t) = build_trait(parsed, &node) {
                sem.traits.push(t);
            }
        }
        "impl_item" => {
            if let Some(i) = build_impl(parsed, &node, &new_ctx) {
                sem.impls.push(i);
            }
        }
        "static_item" | "const_item" => {
            if let Some(s) = build_static(parsed, &node) {
                sem.statics.push(s);
            }
        }
        "macro_invocation" => {
            if let Some(m) = build_macro_invocation(parsed, &node, &new_ctx) {
                sem.macro_invocations.push(m);
            }
        }
        "call_expression" => {
            if let Some(call) = build_call_site(parsed, &node, &new_ctx) {
                sem.calls.push(call);
            }
            // Check for database operations (Diesel, SeaORM, sqlx, etc.)
            if let Some(db_op) = detect_db_operation_from_call(parsed, &node, &new_ctx) {
                sem.db_operations.push(db_op);
            }
        }
        "field_expression" => {
            // Only collect field accesses that are not part of a method call
            // (method calls are handled in call_expression)
            if !is_method_call_receiver(parsed, &node) {
                if let Some(field_access) = build_field_access(parsed, &node, &new_ctx) {
                    sem.field_accesses.push(field_access);
                }
            }
        }
        "let_declaration" => {
            if let Some(binding) = build_variable_binding(parsed, &node, &new_ctx, false) {
                sem.variable_bindings.push(binding);
            }
        }
        "for_expression" => {
            // Capture loop variable bindings
            if let Some(binding) = build_loop_variable_binding(parsed, &node, &new_ctx) {
                sem.variable_bindings.push(binding);
            }
        }
        _ => {}
    }

    // Recurse into children
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_nodes(child, parsed, sem, new_ctx.clone());
        }
    }
}

/// Detect database operations from call expressions.
fn detect_db_operation_from_call(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    ctx: &TraversalContext,
) -> Option<DbOperation> {
    let func_node = node.child_by_field_name("function")?;
    let callee_expr = parsed.text_for_node(&func_node);

    let (library, operation_type) = match callee_expr.as_str() {
        // Diesel ORM patterns
        s if s.contains("schema::") && s.contains(".execute") => (DbLibrary::Diesel, DbOperationType::Update),
        s if s.contains("schema::") && s.contains(".load") => (DbLibrary::Diesel, DbOperationType::Select),
        s if s.contains("schema::") && s.contains(".delete") => (DbLibrary::Diesel, DbOperationType::Delete),
        s if s.contains(".insert_into") => (DbLibrary::Diesel, DbOperationType::Insert),
        s if s.contains("select(") && s.contains("::") => (DbLibrary::Diesel, DbOperationType::Select),
        s if s.contains(".update(") => (DbLibrary::Diesel, DbOperationType::Update),

        // SeaORM patterns
        s if s.contains("Entity::") && s.contains(".find") => (DbLibrary::SeaOrm, DbOperationType::Select),
        s if s.contains("Entity::") && s.contains(".insert") => (DbLibrary::SeaOrm, DbOperationType::Insert),
        s if s.contains("Entity::") && s.contains(".update") => (DbLibrary::SeaOrm, DbOperationType::Update),
        s if s.contains("Entity::") && s.contains(".delete") => (DbLibrary::SeaOrm, DbOperationType::Delete),

        // sqlx patterns
        s if s.contains("query_as") && s.contains("PgPool") => (DbLibrary::Sqlx, DbOperationType::Select),
        s if s.contains("query_as") && s.contains("execute") && s.contains("PgPool") => (DbLibrary::Sqlx, DbOperationType::Update),

        // tokio-postgres patterns
        s if s.contains("PgPool") && s.contains("query") => (DbLibrary::TokioPostgres, DbOperationType::Select),
        s if s.contains("PgPool") && s.contains("execute") => (DbLibrary::TokioPostgres, DbOperationType::Update),

        _ => return None,
    };

    let text = parsed.text_for_node(node);
    let ast_location = parsed.location_for_node(node);

    Some(DbOperation {
        library,
        operation_type,
        has_timeout: false,
        timeout_value: None,
        in_transaction: false,
        eager_loading: None,
        in_loop: ctx.in_loop,
        in_iteration: false,
        model_name: None,
        relationship_field: None,
        operation_text: text,
        location: CommonLocation {
            file_id: ast_location.file_id,
            line: ast_location.range.start_line + 1,
            column: ast_location.range.start_col + 1,
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
        },
        enclosing_function: ctx.current_function.clone(),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

/// Update the traversal context based on the current node.
fn update_context(
    node: &tree_sitter::Node,
    parsed: &ParsedFile,
    ctx: &TraversalContext,
) -> TraversalContext {
    let mut new_ctx = ctx.clone();

    match node.kind() {
        "for_expression" | "while_expression" | "loop_expression" => {
            new_ctx.in_loop = true;
        }
        "function_item" => {
            let name = node
                .child_by_field_name("name")
                .map(|n| parsed.text_for_node(&n));
            new_ctx.current_function = name.clone();
            new_ctx.in_main = name.as_deref() == Some("main");
            // Check if async
            let fn_text = parsed.text_for_node(node);
            new_ctx.in_async_fn = fn_text.contains("async fn");
            // Check for #[test] attribute or if already in test context
            new_ctx.in_test = ctx.in_test || has_test_attribute(parsed, node);
        }
        "mod_item" => {
            // Check if this module has a #[cfg(test)] attribute
            if has_cfg_test_attribute(parsed, node) {
                new_ctx.in_test = true;
            }
        }
        "impl_item" => {
            new_ctx.in_impl = true;
        }
        "closure_expression" => {
            new_ctx.in_closure = true;
        }
        "unsafe_block" => {
            new_ctx.in_unsafe = true;
        }
        "attribute_item" => {
            let text = parsed.text_for_node(node);
            if text.contains("cfg(test)") {
                new_ctx.in_test = true;
            }
        }
        "static_item" | "const_item" => {
            // Check if this is a LazyLock, OnceLock, or lazy_static initialization
            let text = parsed.text_for_node(node);
            if text.contains("LazyLock")
                || text.contains("OnceLock")
                || text.contains("Lazy<")
                || text.contains("OnceCell")
                || text.contains("lazy_static!")
            {
                new_ctx.in_static_init = true;
            }
        }
        "call_expression" => {
            // Check for OnceLock.get_or_init() / once_cell.get_or_init() patterns
            // These are the correct patterns for compile-once initialization
            let text = parsed.text_for_node(node);
            if text.contains(".get_or_init(") || text.contains(".get_or_try_init(") {
                new_ctx.in_static_init = true;
            }
        }
        _ => {}
    }

    new_ctx
}

/// Check if a function has a #[test] attribute.
fn has_test_attribute(parsed: &ParsedFile, func_node: &tree_sitter::Node) -> bool {
    // Look at previous sibling for attribute
    if let Some(prev) = func_node.prev_sibling() {
        if prev.kind() == "attribute_item" {
            let text = parsed.text_for_node(&prev);
            return text.contains("#[test]") || text.contains("#[tokio::test]");
        }
    }
    false
}

/// Check if a module has a #[cfg(test)] attribute.
///
/// This handles the common pattern:
/// ```rust
/// #[cfg(test)]
/// mod tests {
///     // test code here
/// }
/// ```
fn has_cfg_test_attribute(parsed: &ParsedFile, mod_node: &tree_sitter::Node) -> bool {
    // Look at previous siblings for #[cfg(test)] attribute
    // There might be multiple attributes, so check a few siblings
    let mut prev = mod_node.prev_sibling();
    let mut siblings_checked = 0;
    const MAX_SIBLINGS: usize = 5;

    while let Some(p) = prev {
        if p.kind() == "attribute_item" {
            let text = parsed.text_for_node(&p);
            if text.contains("cfg(test)") || text.contains("cfg( test )") {
                return true;
            }
        } else if p.kind() != "line_comment" && p.kind() != "block_comment" {
            // Stop if we hit a non-attribute, non-comment node
            break;
        }

        siblings_checked += 1;
        if siblings_checked >= MAX_SIBLINGS {
            break;
        }
        prev = p.prev_sibling();
    }

    false
}

/// Build a RustUse from a use_declaration node.
fn build_use(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<model::RustUse> {
    let text = parsed.text_for_node(node);
    let is_pub = text.starts_with("pub ");
    let is_glob = text.contains("::*");

    // Extract the path - simplified extraction
    let path = extract_use_path(parsed, node);

    Some(model::RustUse {
        path,
        alias: extract_use_alias(parsed, node),
        is_glob,
        is_pub,
        items: extract_use_items(parsed, node),
        location: parsed.location_for_node(node),
    })
}

/// Extract the path from a use declaration.
fn extract_use_path(parsed: &ParsedFile, node: &tree_sitter::Node) -> String {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "use_list"
                || child.kind() == "scoped_identifier"
                || child.kind() == "identifier"
                || child.kind() == "scoped_use_list"
            {
                let text = parsed.text_for_node(&child);
                return text.trim().to_string();
            }
        }
    }
    let text = parsed.text_for_node(node);
    let text = text.trim_start_matches("pub ");
    let text = text.trim_start_matches("use ");
    let text = text.trim_end_matches(';');
    text.trim().to_string()
}

/// Extract grouped items from a use declaration like `use std::{io, fs}`.
fn extract_use_items(parsed: &ParsedFile, node: &tree_sitter::Node) -> Vec<String> {
    let mut items = Vec::new();

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "use_list" || child.kind() == "scoped_use_list" {
                for j in 0..child.child_count() {
                    if let Some(item) = child.child(j) {
                        if item.kind() == "identifier" || item.kind() == "use_as_clause" || item.kind() == "use_prelude_clause" {
                            let text = parsed.text_for_node(&item);
                            if !text.is_empty() {
                                items.push(text.trim().to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    items
}

/// Extract generics parameters from a node.
fn extract_generics(parsed: &ParsedFile, node: &tree_sitter::Node) -> Vec<String> {
    let text = parsed.text_for_node(node);

    if let Some(start) = text.find('<') {
        let after = &text[start + 1..];
        let mut depth = 0;
        let mut end = 0;
        for (i, c) in after.char_indices() {
            match c {
                '<' => depth += 1,
                '>' => {
                    if depth == 0 {
                        end = i;
                        break;
                    }
                    depth -= 1;
                }
                _ => {}
            }
        }
        if end > 0 {
            let generics_text = &after[..end];
            return generics_text
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }

    Vec::new()
}

/// Extract attributes from preceding attribute items.
fn extract_attributes(parsed: &ParsedFile, node: &tree_sitter::Node) -> Vec<String> {
    extract_attributes_for_node(parsed, node)
}

/// Extract attributes for any node by looking at previous siblings.
fn extract_attributes_for_node(parsed: &ParsedFile, node: &tree_sitter::Node) -> Vec<String> {
    let mut attributes = Vec::new();

    let mut prev = node.prev_sibling();
    while let Some(p) = prev {
        if p.kind() == "attribute_item" {
            let text = parsed.text_for_node(&p);
            attributes.push(text);
        } else {
            break;
        }
        prev = p.prev_sibling();
    }

    attributes.reverse();
    attributes
}

/// Extract alias from use declaration if present.
fn extract_use_alias(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<String> {
    let text = parsed.text_for_node(node);
    if text.contains(" as ") {
        let parts: Vec<&str> = text.split(" as ").collect();
        if parts.len() == 2 {
            return Some(parts[1].trim_end_matches(';').trim().to_string());
        }
    }
    None
}

/// Build a RustFunction from a function_item node.
fn build_function(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    ctx: &TraversalContext,
) -> Option<model::RustFunction> {
    let name_node = node.child_by_field_name("name")?;
    let name = parsed.text_for_node(&name_node);

    let fn_text = parsed.text_for_node(node);
    let is_async = fn_text.contains("async fn") || fn_text.contains("async unsafe fn");
    let is_unsafe = fn_text.contains("unsafe fn") || fn_text.contains("async unsafe fn");
    let is_const = fn_text.contains("const fn");
    let is_extern = fn_text.contains("extern ");

    let visibility = extract_visibility(&fn_text);
    let return_type = extract_return_type(parsed, node);
    let returns_result = return_type.as_ref().is_some_and(|t| t.contains("Result"));
    let returns_option = return_type.as_ref().is_some_and(|t| t.contains("Option"));

    let generics = extract_generics(parsed, node);
    let attributes = extract_attributes(parsed, node);

    Some(model::RustFunction {
        name: name.clone(),
        visibility,
        is_async,
        is_unsafe,
        is_const,
        is_extern,
        generics,
        params: extract_params(parsed, node),
        return_type,
        returns_result,
        returns_option,
        is_test: ctx.in_test || has_test_attribute(parsed, node),
        is_main: name == "main",
        has_test_attribute: has_test_attribute(parsed, node),
        attributes,
        location: parsed.location_for_node(node),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

/// Extract visibility from text.
fn extract_visibility(text: &str) -> model::Visibility {
    if text.starts_with("pub(crate)") {
        model::Visibility::PubCrate
    } else if text.starts_with("pub(super)") {
        model::Visibility::PubSuper
    } else if let Some(after_prefix) = text.strip_prefix("pub(in ") {
        // Use strip_prefix instead of starts_with + find().unwrap()
        let end = after_prefix.find(')').unwrap_or(0);
        model::Visibility::PubIn(after_prefix[..end].to_string())
    } else if text.starts_with("pub ") {
        model::Visibility::Pub
    } else {
        model::Visibility::Private
    }
}

/// Extract return type from function.
fn extract_return_type(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<String> {
    // Look for return_type field
    if let Some(ret) = node.child_by_field_name("return_type") {
        return Some(parsed.text_for_node(&ret));
    }
    None
}

/// Extract parameters from function.
fn extract_params(parsed: &ParsedFile, node: &tree_sitter::Node) -> Vec<model::RustParam> {
    let mut params = Vec::new();

    if let Some(params_node) = node.child_by_field_name("parameters") {
        for i in 0..params_node.child_count() {
            if let Some(child) = params_node.child(i) {
                if child.kind() == "parameter" || child.kind() == "self_parameter" {
                    let text = parsed.text_for_node(&child);
                    let is_self = text.contains("self");
                    let is_mut = text.contains("mut ");
                    let is_ref = text.contains('&');

                    // Extract name and type
                    let (name, param_type) = if is_self {
                        ("self".to_string(), text.clone())
                    } else {
                        let parts: Vec<&str> = text.splitn(2, ':').collect();
                        if parts.len() == 2 {
                            (parts[0].trim().to_string(), parts[1].trim().to_string())
                        } else {
                            (text.clone(), String::new())
                        }
                    };

                    params.push(model::RustParam {
                        name,
                        param_type,
                        is_self,
                        is_mut,
                        is_ref,
                    });
                }
            }
        }
    }

    params
}

/// Build a RustStruct from a struct_item node.
fn build_struct(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<model::RustStruct> {
    let name_node = node.child_by_field_name("name")?;
    let name = parsed.text_for_node(&name_node);

    let text = parsed.text_for_node(node);
    let visibility = extract_visibility(&text);

    let is_tuple = text.contains('(') && !text.contains('{');
    let is_unit = !text.contains('(') && !text.contains('{');

    let derives = extract_derives(parsed, node);
    let generics = extract_generics(parsed, node);
    let fields = extract_struct_fields(parsed, node);
    let attributes = extract_attributes(parsed, node);

    Some(model::RustStruct {
        name,
        visibility,
        generics,
        fields,
        is_tuple,
        is_unit,
        derives,
        attributes,
        location: parsed.location_for_node(node),
    })
}

/// Extract struct fields.
fn extract_struct_fields(parsed: &ParsedFile, node: &tree_sitter::Node) -> Vec<model::RustField> {
    let mut fields = Vec::new();

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "field_declaration_list" {
                for j in 0..child.child_count() {
                    if let Some(field) = child.child(j) {
                        if field.kind() == "field_declaration" {
                            if let Some(name_node) = field.child_by_field_name("name") {
                                let name = parsed.text_for_node(&name_node);
                                let field_text = parsed.text_for_node(&field);
                                let visibility = extract_visibility(&field_text);

                                let field_type = field
                                    .child_by_field_name("type")
                                    .map(|n| parsed.text_for_node(&n))
                                    .unwrap_or_default();

                                let field_attrs = extract_attributes_for_node(parsed, &field);

                                fields.push(model::RustField {
                                    name,
                                    field_type,
                                    visibility,
                                    attributes: field_attrs,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    fields
}

/// Extract derive macros from preceding attributes.
fn extract_derives(parsed: &ParsedFile, node: &tree_sitter::Node) -> Vec<String> {
    let mut derives = Vec::new();

    // Look at previous siblings for attribute items
    let mut prev = node.prev_sibling();
    while let Some(p) = prev {
        if p.kind() == "attribute_item" {
            let text = parsed.text_for_node(&p);
            if text.contains("derive(") {
                // Extract derive names
                if let Some(start) = text.find("derive(") {
                    let after = &text[start + 7..];
                    if let Some(end) = after.find(')') {
                        let derive_list = &after[..end];
                        for d in derive_list.split(',') {
                            derives.push(d.trim().to_string());
                        }
                    }
                }
            }
        } else {
            break;
        }
        prev = p.prev_sibling();
    }

    derives
}

/// Build a RustEnum from an enum_item node.
fn build_enum(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<model::RustEnum> {
    let name_node = node.child_by_field_name("name")?;
    let name = parsed.text_for_node(&name_node);

    let text = parsed.text_for_node(node);
    let visibility = extract_visibility(&text);
    let derives = extract_derives(parsed, node);
    let generics = extract_generics(parsed, node);
    let variants = extract_enum_variants(parsed, node);
    let attributes = extract_attributes(parsed, node);

    Some(model::RustEnum {
        name,
        visibility,
        generics,
        variants,
        derives,
        attributes,
        location: parsed.location_for_node(node),
    })
}

/// Extract enum variants.
fn extract_enum_variants(parsed: &ParsedFile, node: &tree_sitter::Node) -> Vec<model::EnumVariant> {
    let mut variants = Vec::new();

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "enum_variant_list" {
                for j in 0..child.child_count() {
                    if let Some(variant) = child.child(j) {
                        if variant.kind() == "enum_variant" {
                            if let Some(name_node) = variant.child_by_field_name("name") {
                                let name = parsed.text_for_node(&name_node);
                                let _variant_text = parsed.text_for_node(&variant);

                                let mut tuple_fields = Vec::new();
                                let mut struct_fields = Vec::new();
                                let mut discriminant = None;

                                for k in 0..variant.child_count() {
                                    if let Some(field) = variant.child(k) {
                                        if field.kind() == "tuple_field"
                                            || field.kind() == "positional_field"
                                        {
                                            let field_type = parsed.text_for_node(&field);
                                            tuple_fields.push(field_type);
                                        } else if field.kind() == "struct_field"
                                            || field.kind() == "named_field"
                                        {
                                            if let Some(name) = field.child_by_field_name("name") {
                                                let field_name = parsed.text_for_node(&name);
                                                let field_type = field
                                                    .child_by_field_name("type")
                                                    .map(|n| parsed.text_for_node(&n))
                                                    .unwrap_or_default();
                                                struct_fields.push(model::RustField {
                                                    name: field_name,
                                                    field_type,
                                                    visibility: model::Visibility::Private,
                                                    attributes: Vec::new(),
                                                });
                                            }
                                        } else if field.kind() == "discriminant_value" {
                                            discriminant = Some(parsed.text_for_node(&field));
                                        }
                                    }
                                }

                                variants.push(model::EnumVariant {
                                    name,
                                    tuple_fields,
                                    struct_fields,
                                    discriminant,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    variants
}

/// Build a RustTrait from a trait_item node.
fn build_trait(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<model::RustTrait> {
    let name_node = node.child_by_field_name("name")?;
    let name = parsed.text_for_node(&name_node);

    let text = parsed.text_for_node(node);
    let visibility = extract_visibility(&text);

    let generics = extract_generics(parsed, node);
    let attributes = extract_attributes(parsed, node);

    Some(model::RustTrait {
        name,
        visibility,
        generics,
        bounds: Vec::new(),
        associated_types: Vec::new(),
        methods: Vec::new(),
        attributes,
        location: parsed.location_for_node(node),
    })
}

/// Build a RustImpl from an impl_item node.
fn build_impl(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    ctx: &TraversalContext,
) -> Option<model::RustImpl> {
    let text = parsed.text_for_node(node);

    // Extract self type (the type being implemented)
    let self_type_node = node.child_by_field_name("type")?;
    let self_type = parsed.text_for_node(&self_type_node);

    // Check for trait impl
    let trait_name = node
        .child_by_field_name("trait")
        .map(|n| parsed.text_for_node(&n));

    let is_unsafe = text.starts_with("unsafe impl");

    // Collect methods in the impl block
    let mut methods = Vec::new();
    if let Some(body) = node.child_by_field_name("body") {
        for i in 0..body.child_count() {
            if let Some(child) = body.child(i) {
                if child.kind() == "function_item" {
                    if let Some(func) = build_function(parsed, &child, ctx) {
                        methods.push(func);
                    }
                }
            }
        }
    }

    Some(model::RustImpl {
        self_type,
        trait_name,
        generics: extract_generics(parsed, node),
        methods,
        is_unsafe,
        location: parsed.location_for_node(node),
    })
}

/// Build a StaticDecl from a static_item or const_item node.
fn build_static(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<model::StaticDecl> {
    let name_node = node.child_by_field_name("name")?;
    let name = parsed.text_for_node(&name_node);

    let text = parsed.text_for_node(node);
    let is_const = node.kind() == "const_item";
    let is_mut = text.contains("static mut ");
    let visibility = extract_visibility(&text);

    let decl_type = node
        .child_by_field_name("type")
        .map(|n| parsed.text_for_node(&n))
        .unwrap_or_default();

    // Check if it's thread-safe
    let is_thread_safe = decl_type.contains("Mutex")
        || decl_type.contains("RwLock")
        || decl_type.contains("Atomic")
        || decl_type.contains("OnceCell")
        || decl_type.contains("OnceLock");

    Some(model::StaticDecl {
        name,
        decl_type,
        is_const,
        is_mut,
        visibility,
        is_thread_safe,
        location: parsed.location_for_node(node),
    })
}

/// Build a MacroInvocation from a macro_invocation node.
fn build_macro_invocation(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    ctx: &TraversalContext,
) -> Option<model::MacroInvocation> {
    // Get the macro name
    let macro_node = node.child_by_field_name("macro")?;
    let name = parsed.text_for_node(&macro_node);

    let text = parsed.text_for_node(node);

    let is_debug_macro = matches!(
        name.as_str(),
        "println" | "print" | "eprintln" | "eprint" | "dbg"
    );

    let should_be_tracing = is_debug_macro && !ctx.in_test;

    // Extract arguments
    let args = text
        .strip_prefix(&format!("{}!", name))
        .unwrap_or(&text)
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .to_string();

    Some(model::MacroInvocation {
        name,
        is_debug_macro,
        should_be_tracing,
        args,
        in_test: ctx.in_test,
        function_name: ctx.current_function.clone(),
        location: parsed.location_for_node(node),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

/// Build a RustCallSite from a call_expression node.
fn build_call_site(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    ctx: &TraversalContext,
) -> Option<model::RustCallSite> {
    let func_node = node.child_by_field_name("function")?;
    let callee_expr = parsed.text_for_node(&func_node);

    // Parse callee into parts (e.g., "obj.method" -> ["obj", "method"])
    let callee_parts: Vec<String> = callee_expr.split('.').map(String::from).collect();
    let first_part = callee_parts.first().cloned().unwrap_or_default();

    // Detect if method call (has receiver)
    let is_method_call = callee_parts.len() > 1;
    let _receiver = if is_method_call {
        Some(first_part.clone())
    } else {
        None
    };

    // Detect self call (Rust uses 'self', not 'self')
    let is_self_call = first_part == "self";

    let location = parsed.location_for_node(node);

    let function_call = FunctionCall {
        callee_expr: callee_expr.clone(),
        callee_parts,
        caller_function: ctx.current_function.clone().unwrap_or_default(),
        caller_qualified_name: ctx.current_qualified_name.clone().unwrap_or_default(),
        location: CommonLocation {
            file_id: parsed.file_id,
            line: location.range.start_line + 1,
            column: location.range.start_col + 1,
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
        },
        is_self_call,
        is_import_call: false, // Will be enhanced with import analysis
        import_alias: None,
    };

    // Get arguments representation
    let args_repr = if let Some(args_node) = node.child_by_field_name("arguments") {
        parsed.text_for_node(&args_node)
    } else {
        String::new()
    };

    Some(model::RustCallSite {
        function_call,
        args_repr,
        in_loop: ctx.in_loop,
        in_async: ctx.in_async_fn,
        in_static_init: ctx.in_static_init,
    })
}

/// Check if a field_expression is the receiver of a method call.
///
/// We don't want to collect field accesses that are part of method calls
/// (e.g., `obj.method()` - we only want `obj.field`).
fn is_method_call_receiver(_parsed: &ParsedFile, node: &tree_sitter::Node) -> bool {
    if let Some(parent) = node.parent() {
        if parent.kind() == "call_expression" {
            // This field_expression is the "function" of a call_expression
            if let Some(func_node) = parent.child_by_field_name("function") {
                return func_node.id() == node.id();
            }
        }
    }
    false
}

/// Build a FieldAccess from a field_expression node.
fn build_field_access(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    ctx: &TraversalContext,
) -> Option<model::FieldAccess> {
    let value_node = node.child_by_field_name("value")?;
    let field_node = node.child_by_field_name("field")?;

    let receiver = parsed.text_for_node(&value_node);
    let field = parsed.text_for_node(&field_node);
    let full_expr = parsed.text_for_node(node);

    Some(model::FieldAccess {
        receiver,
        field,
        full_expr,
        in_loop: ctx.in_loop,
        function_name: ctx.current_function.clone(),
        location: parsed.location_for_node(node),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

/// Build a VariableBinding from a let_declaration node.
fn build_variable_binding(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    ctx: &TraversalContext,
    is_loop_variable: bool,
) -> Option<model::VariableBinding> {
    let pattern_node = node.child_by_field_name("pattern")?;
    let name = extract_binding_name(parsed, &pattern_node)?;

    let value_node = node.child_by_field_name("value");
    let init_expr = value_node.as_ref().map(|n| parsed.text_for_node(n));

    // Check if the initialization involves a clone
    let init_has_clone = init_expr
        .as_ref()
        .is_some_and(|expr| expr.contains(".clone()"));

    // Check if the initialization is a consuming function call
    let (init_is_consuming_call, consumed_variable) = detect_consuming_call(init_expr.as_deref());

    // Find the end of the enclosing block for scope
    let scope_end_byte = find_scope_end(node);

    // Check if the binding is mutable
    let is_mut = parsed.text_for_node(node).contains("let mut ");

    Some(model::VariableBinding {
        name,
        init_expr,
        is_loop_variable,
        is_mut,
        init_has_clone,
        init_is_consuming_call,
        consumed_variable,
        function_name: ctx.current_function.clone(),
        in_loop: ctx.in_loop,
        location: parsed.location_for_node(node),
        scope_start_byte: node.start_byte(),
        scope_end_byte,
    })
}

/// Build a VariableBinding for a for loop's pattern variable.
fn build_loop_variable_binding(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    ctx: &TraversalContext,
) -> Option<model::VariableBinding> {
    // for_expression has a "pattern" field for the loop variable
    let pattern_node = node.child_by_field_name("pattern")?;
    let name = extract_binding_name(parsed, &pattern_node)?;

    // The loop body is the scope
    let body_node = node.child_by_field_name("body");
    let scope_end_byte = body_node.map(|b| b.end_byte()).unwrap_or(node.end_byte());

    Some(model::VariableBinding {
        name,
        init_expr: None,
        is_loop_variable: true,
        is_mut: false,
        init_has_clone: false,
        init_is_consuming_call: false,
        consumed_variable: None,
        function_name: ctx.current_function.clone(),
        in_loop: true, // Loop variable is always in a loop context
        location: parsed.location_for_node(&pattern_node),
        scope_start_byte: pattern_node.start_byte(),
        scope_end_byte,
    })
}

/// Extract the variable name from a pattern node.
fn extract_binding_name(parsed: &ParsedFile, pattern: &tree_sitter::Node) -> Option<String> {
    match pattern.kind() {
        "identifier" => Some(parsed.text_for_node(pattern)),
        "mut_pattern" => {
            // mut x -> get the identifier inside
            pattern.child(1).map(|n| parsed.text_for_node(&n))
        }
        "tuple_pattern" | "struct_pattern" | "slice_pattern" => {
            // For complex patterns, we skip for now
            // Could be extended to handle destructuring
            None
        }
        _ => {
            // Try to get text as fallback
            let text = parsed.text_for_node(pattern);
            if text.chars().all(|c| c.is_alphanumeric() || c == '_') {
                Some(text)
            } else {
                None
            }
        }
    }
}

/// Detect if an initialization expression is a consuming function call.
///
/// Returns (is_consuming, consumed_variable) where consumed_variable is
/// the variable being consumed (e.g., "rf" from "Finding::from(rf.clone())").
fn detect_consuming_call(init_expr: Option<&str>) -> (bool, Option<String>) {
    let expr = match init_expr {
        Some(e) => e,
        None => return (false, None),
    };

    // Patterns that consume their argument:
    // - SomeType::from(x) or From::from(x)
    // - x.into()
    // - SomeType::try_from(x)
    // - Box::new(x), Rc::new(x), Arc::new(x)

    // Check for From::from or Type::from pattern
    if let Some(consumed) = extract_consumed_from_from(expr) {
        return (true, Some(consumed));
    }

    // Check for .into() pattern
    if let Some(consumed) = extract_consumed_from_into(expr) {
        return (true, Some(consumed));
    }

    // Check for Box/Rc/Arc::new pattern
    if let Some(consumed) = extract_consumed_from_wrapper_new(expr) {
        return (true, Some(consumed));
    }

    (false, None)
}

/// Extract the consumed variable from a From::from(x) or Type::from(x) pattern.
fn extract_consumed_from_from(expr: &str) -> Option<String> {
    // Match patterns like:
    // - Finding::from(rf.clone())
    // - From::from(value)
    // - <Type>::from(x)

    let from_patterns = ["::from(", "From::from("];

    for pattern in from_patterns {
        if let Some(pos) = expr.find(pattern) {
            let after = &expr[pos + pattern.len()..];
            if let Some(arg) = extract_function_arg(after) {
                // Strip .clone() if present
                let consumed = arg.strip_suffix(".clone()").unwrap_or(&arg).to_string();
                // Only return simple identifiers
                if is_simple_identifier(&consumed) {
                    return Some(consumed);
                }
            }
        }
    }

    None
}

/// Extract the consumed variable from an x.into() pattern.
fn extract_consumed_from_into(expr: &str) -> Option<String> {
    // Match pattern: variable.into()
    if let Some(pos) = expr.find(".into()") {
        let before = expr[..pos].trim();
        // Handle x.clone().into()
        let consumed = before.strip_suffix(".clone()").unwrap_or(before);
        if is_simple_identifier(consumed) {
            return Some(consumed.to_string());
        }
    }
    None
}

/// Extract the consumed variable from Box::new(x), Rc::new(x), Arc::new(x).
fn extract_consumed_from_wrapper_new(expr: &str) -> Option<String> {
    let wrapper_patterns = ["Box::new(", "Rc::new(", "Arc::new(", "RefCell::new("];

    for pattern in wrapper_patterns {
        if let Some(pos) = expr.find(pattern) {
            let after = &expr[pos + pattern.len()..];
            if let Some(arg) = extract_function_arg(after) {
                let consumed = arg.strip_suffix(".clone()").unwrap_or(&arg).to_string();
                if is_simple_identifier(&consumed) {
                    return Some(consumed);
                }
            }
        }
    }

    None
}

/// Extract the first argument from a function call (text after opening paren).
fn extract_function_arg(after_paren: &str) -> Option<String> {
    let mut depth = 1;
    let mut end = 0;

    for (i, c) in after_paren.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            ',' if depth == 1 => {
                // Multiple arguments - take only the first
                end = i;
                break;
            }
            _ => {}
        }
    }

    if end > 0 {
        Some(after_paren[..end].trim().to_string())
    } else {
        None
    }
}

/// Check if a string is a simple identifier (variable name).
fn is_simple_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_alphanumeric() || c == '_')
        && s.chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
}

/// Find the end byte of the enclosing scope (block).
fn find_scope_end(node: &tree_sitter::Node) -> usize {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent.kind() == "block" || parent.kind() == "function_item" {
            return parent.end_byte();
        }
        current = parent.parent();
    }
    node.end_byte()
}

/// Extract the method name from a call expression if it's a method call.
///
/// For `data.unwrap()`, this returns `Some("unwrap")`.
/// For `foo()` (not a method call), this returns `None`.
fn extract_method_name(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<String> {
    let func_node = node.child_by_field_name("function")?;
    if func_node.kind() == "field_expression" {
        // Method call: the "field" is the method name
        let field_node = func_node.child_by_field_name("field")?;
        Some(parsed.text_for_node(&field_node))
    } else {
        None
    }
}

/// Analyze async-specific patterns.
fn analyze_async_patterns(parsed: &ParsedFile, sem: &mut RustFileSemantics) {
    let root = parsed.tree.root_node();
    let ctx = TraversalContext::default();
    walk_for_async(root, parsed, sem, ctx);
}

/// Walk AST looking for async patterns.
fn walk_for_async(
    node: tree_sitter::Node,
    parsed: &ParsedFile,
    sem: &mut RustFileSemantics,
    ctx: TraversalContext,
) {
    let new_ctx = update_context(&node, parsed, &ctx);

    // Look for spawn calls
    if node.kind() == "call_expression" {
        let text = parsed.text_for_node(&node);
        if let Some(spawn_call) = detect_spawn_call(parsed, &node, &text, &new_ctx) {
            sem.async_info.spawn_calls.push(spawn_call);
        }
    }

    // Look for await expressions
    if node.kind() == "await_expression" {
        let expr = parsed.text_for_node(&node);
        sem.async_info.await_points.push(model::AwaitPoint {
            expr,
            in_loop: new_ctx.in_loop,
            function_name: new_ctx.current_function.clone(),
            location: parsed.location_for_node(&node),
        });
    }

    // Look for select! macro
    if node.kind() == "macro_invocation" {
        let text = parsed.text_for_node(&node);
        if text.contains("select!") || text.contains("tokio::select!") {
            if let Some(select) = analyze_select_usage(parsed, &node, &text, &new_ctx) {
                sem.async_info.select_usages.push(select);
            }
        }
    }

    // Recurse
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_for_async(child, parsed, sem, new_ctx.clone());
        }
    }
}

/// Detect a spawn call.
///
/// Note: We check the callee expression directly rather than doing string
/// matching on the full node text. This prevents false positives when spawn
/// calls appear inside string literals (e.g., in documentation or fix previews).
fn detect_spawn_call(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    _text: &str,
    ctx: &TraversalContext,
) -> Option<model::SpawnCall> {
    // Get the function/callee being called - this is the actual callable, not string content
    let func_node = node.child_by_field_name("function")?;
    let callee = parsed.text_for_node(&func_node);

    // Check callee patterns - must be an actual spawn call, not text inside a string
    let spawn_type =
        if callee == "tokio::spawn" || callee.ends_with("::spawn") && callee.contains("tokio") {
            model::SpawnType::TokioSpawn
        } else if callee.ends_with("spawn_blocking") {
            model::SpawnType::TokioSpawnBlocking
        } else if callee.ends_with("spawn_local") {
            model::SpawnType::TokioSpawnLocal
        } else if callee == "async_std::task::spawn"
            || (callee.ends_with("::spawn") && callee.contains("async_std"))
        {
            model::SpawnType::AsyncStdSpawn
        } else {
            return None;
        };

    // Check if handle is captured
    let parent = node.parent();
    let handle_captured = parent
        .is_some_and(|p| p.kind() == "let_declaration" || p.kind() == "assignment_expression");

    let has_error_handling = analyze_join_handle_error_handling(parsed, node, &callee);

    Some(model::SpawnCall {
        spawn_type,
        handle_captured,
        has_error_handling,
        spawned_expr: parsed.text_for_node(node),
        function_name: ctx.current_function.clone(),
        location: parsed.location_for_node(node),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

/// Analyze if JoinHandle is properly awaited or error is handled.
fn analyze_join_handle_error_handling(parsed: &ParsedFile, spawn_node: &tree_sitter::Node, _callee: &str) -> bool {
    let parent = match spawn_node.parent() {
        Some(p) => p,
        None => return false,
    };

    if parent.kind() == "let_declaration" {
        let pattern = match parent.child_by_field_name("pattern") {
            Some(p) => p,
            None => return false,
        };
        let pattern_text = parsed.text_for_node(&pattern);

        if let Some(value_node) = parent.child_by_field_name("value") {
            if value_node.id() == spawn_node.id() {
                let handle_var = if pattern_text.starts_with("let ") {
                    match pattern_text[4..].trim().split_whitespace().next() {
                        Some(v) => v,
                        None => return false,
                    }
                } else {
                    &pattern_text
                };

                return check_handle_usage(parsed, &parent, handle_var);
            }
        }
    } else if parent.kind() == "assignment_expression" {
        let left = match parent.child_by_field_name("left") {
            Some(l) => l,
            None => return false,
        };
        let left_text = parsed.text_for_node(&left);
        let handle_var = left_text.trim();

        return check_handle_usage(parsed, &parent, handle_var);
    }

    false
}

/// Check if a JoinHandle variable is properly awaited.
fn check_handle_usage(parsed: &ParsedFile, start_node: &tree_sitter::Node, handle_var: &str) -> bool {
    let mut current = start_node.next_sibling();
    let mut max_nodes = 50;

    while let Some(node) = current {
        if max_nodes == 0 {
            break;
        }
        max_nodes -= 1;

        if node.kind() == "call_expression" {
            if let Some(func_node) = node.child_by_field_name("function") {
                let func_text = parsed.text_for_node(&func_node);

                if func_text.contains(&format!("{}.await", handle_var))
                    || func_text.contains(&format!("{}.join().await", handle_var))
                {
                    return true;
                }

                if func_text.contains(&format!("{}.abort()", handle_var)) {
                    return true;
                }

                if func_text.contains("tokio::spawn")
                    && func_text.contains(&format!("await {}", handle_var))
                {
                    return true;
                }
            }
        }

        if node.kind() == "expression_statement" {
            if let Some(child) = node.child(0) {
                let text = parsed.text_for_node(&child);
                if text.contains(&format!("{}.await", handle_var))
                    || text.contains(&format!("{}.join()", handle_var))
                {
                    return true;
                }
            }
        }

        current = node.next_sibling();
    }

    false
}

/// Analyze select! macro usage.
fn analyze_select_usage(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    text: &str,
    ctx: &TraversalContext,
) -> Option<model::SelectUsage> {
    // Count branches by counting => in the text
    let branch_count = text.matches("=>").count();
    let has_default = text.contains("else =>") || text.contains("default =>");
    let has_timeout =
        text.contains("sleep") || text.contains("timeout") || text.contains("interval");

    Some(model::SelectUsage {
        branch_count,
        has_default,
        has_timeout,
        function_name: ctx.current_function.clone(),
        location: parsed.location_for_node(node),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

/// Analyze error handling patterns.
fn analyze_error_handling(parsed: &ParsedFile, sem: &mut RustFileSemantics) {
    let root = parsed.tree.root_node();
    let ctx = TraversalContext::default();
    walk_for_error_handling(root, parsed, sem, ctx);
}

/// Walk AST looking for error handling patterns.
///
/// Note: We check the actual method name from the AST rather than doing string
/// matching on the full node text. This prevents false positives when unwrap/expect
/// calls appear inside string literals (e.g., in documentation or fix previews).
fn walk_for_error_handling(
    node: tree_sitter::Node,
    parsed: &ParsedFile,
    sem: &mut RustFileSemantics,
    ctx: TraversalContext,
) {
    let new_ctx = update_context(&node, parsed, &ctx);

    // Look for method calls (for unwrap/expect)
    if node.kind() == "call_expression" {
        // Get the actual method name from the AST to avoid false positives from string content
        if let Some(method_name) = extract_method_name(parsed, &node) {
            let text = parsed.text_for_node(&node);

            // Check for unwrap calls - using actual method name, not string matching
            if method_name == "unwrap"
                || method_name == "unwrap_or"
                || method_name == "unwrap_or_default"
                || method_name == "unwrap_or_else"
            {
                if let Some(unwrap) =
                    build_unwrap_call(parsed, &node, &text, &method_name, &new_ctx)
                {
                    sem.unwrap_calls.push(unwrap);
                }
            }

            // Check for expect calls - using actual method name, not string matching
            if method_name == "expect" {
                if let Some(expect) = build_expect_call(parsed, &node, &text, &new_ctx) {
                    sem.expect_calls.push(expect);
                }
            }
        }
    }

    // Look for ignored results (expression statements that might return Result)
    if node.kind() == "expression_statement" {
        if let Some(child) = node.child(0) {
            let text = parsed.text_for_node(&child);
            // Heuristic: if calling a function that likely returns Result/Option
            if is_likely_fallible_call(&text) && !text.contains('?') {
                sem.result_ignores.push(model::ResultIgnore {
                    ignore_style: model::ResultIgnoreStyle::Statement,
                    expr_text: text,
                    in_test: new_ctx.in_test,
                    function_name: new_ctx.current_function.clone(),
                    location: parsed.location_for_node(&child),
                    start_byte: child.start_byte(),
                    end_byte: child.end_byte(),
                });
            }
        }
    }

    // Look for let _ = ... patterns
    if node.kind() == "let_declaration" {
        let text = parsed.text_for_node(&node);
        if text.starts_with("let _") && !text.starts_with("let __") {
            if let Some(value_node) = node.child_by_field_name("value") {
                let value_text = parsed.text_for_node(&value_node);
                if is_likely_fallible_call(&value_text) {
                    sem.result_ignores.push(model::ResultIgnore {
                        ignore_style: model::ResultIgnoreStyle::LetUnderscore,
                        expr_text: value_text,
                        in_test: new_ctx.in_test,
                        function_name: new_ctx.current_function.clone(),
                        location: parsed.location_for_node(&node),
                        start_byte: node.start_byte(),
                        end_byte: node.end_byte(),
                    });
                }
            }
        }
    }

    // Recurse
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_for_error_handling(child, parsed, sem, new_ctx.clone());
        }
    }
}

/// Build an UnwrapCall from analysis.
fn build_unwrap_call(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    text: &str,
    method_name: &str,
    ctx: &TraversalContext,
) -> Option<model::UnwrapCall> {
    // Use the actual method name extracted from AST
    let method = method_name;

    // Extract the receiver expression (what .unwrap() is called on)
    let receiver_expr = extract_receiver_expr(parsed, node);

    // Detect the pattern for smart fixes
    let pattern = detect_unwrap_pattern(parsed, node, &receiver_expr, text);

    // Try to determine if it's Result or Option based on pattern and text
    let on_type = infer_unwrap_type(&pattern, &receiver_expr, text);

    Some(model::UnwrapCall {
        on_type,
        method: method.to_string(),
        in_test: ctx.in_test,
        in_main: ctx.in_main,
        in_closure: ctx.in_closure,
        in_static_init: ctx.in_static_init,
        function_name: ctx.current_function.clone(),
        expr_text: text.to_string(),
        receiver_expr,
        pattern,
        location: parsed.location_for_node(node),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

/// Extract the receiver expression from an unwrap call (what .unwrap() is called on).
///
/// For `text.find("foo").unwrap()`, returns `text.find("foo")`.
/// For `Some(x).unwrap()`, returns `Some(x)`.
fn extract_receiver_expr(parsed: &ParsedFile, node: &tree_sitter::Node) -> String {
    // The node is a call_expression. Its "function" child is a field_expression.
    // The field_expression's "value" child is the receiver.
    if let Some(func_node) = node.child_by_field_name("function") {
        if func_node.kind() == "field_expression" {
            if let Some(receiver) = func_node.child_by_field_name("value") {
                return parsed.text_for_node(&receiver);
            }
        }
    }
    // Fallback: extract from text
    let text = parsed.text_for_node(node);
    if let Some(pos) = text.rfind(".unwrap") {
        return text[..pos].to_string();
    }
    text
}

/// Detect the pattern of an unwrap call for smart fix suggestions.
fn detect_unwrap_pattern(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    receiver_expr: &str,
    _full_text: &str,
) -> model::UnwrapPattern {
    // Pattern: env::var("...").unwrap()
    if receiver_expr.contains("env::var(") || receiver_expr.contains("std::env::var(") {
        if let Some(var_name) = extract_string_literal(receiver_expr) {
            return model::UnwrapPattern::EnvVar { var_name };
        }
    }

    // Pattern: .parse().unwrap() or .parse::<T>().unwrap()
    if receiver_expr.contains(".parse()") || receiver_expr.contains(".parse::<") {
        let target_type = extract_parse_type(receiver_expr);
        return model::UnwrapPattern::Parse { target_type };
    }

    // Pattern: Regex::new(...).unwrap()
    if receiver_expr.contains("Regex::new(") {
        return model::UnwrapPattern::RegexNew;
    }

    // Pattern: .get(idx).unwrap() on collections
    if receiver_expr.ends_with(')') && receiver_expr.contains(".get(") {
        if let Some(index_expr) = extract_method_arg(receiver_expr, "get") {
            return model::UnwrapPattern::CollectionGet { index_expr };
        }
    }

    // Pattern: .first().unwrap() / .last().unwrap()
    if receiver_expr.ends_with(".first()") {
        return model::UnwrapPattern::FirstOrLast { is_first: true };
    }
    if receiver_expr.ends_with(".last()") {
        return model::UnwrapPattern::FirstOrLast { is_first: false };
    }

    // Pattern: .next().unwrap() on iterators
    if receiver_expr.ends_with(".next()") {
        return model::UnwrapPattern::IteratorNext;
    }

    // Pattern: .lock().unwrap() / .read().unwrap() / .write().unwrap()
    if receiver_expr.ends_with(".lock()") {
        return model::UnwrapPattern::LockUnwrap {
            lock_method: "lock".to_string(),
        };
    }
    if receiver_expr.ends_with(".read()") {
        return model::UnwrapPattern::LockUnwrap {
            lock_method: "read".to_string(),
        };
    }
    if receiver_expr.ends_with(".write()") {
        return model::UnwrapPattern::LockUnwrap {
            lock_method: "write".to_string(),
        };
    }

    // Pattern: .find("x").unwrap() - check for starts_with/contains guard
    if receiver_expr.contains(".find(") {
        if let Some(needle) = extract_method_arg(receiver_expr, "find") {
            // Check if we're inside an if with a starts_with/contains guard
            if let Some(guard_info) = find_guard_condition(parsed, node, &needle) {
                return guard_info;
            }
            // No guard found - still might suggest if let pattern
            return model::UnwrapPattern::ContainsFind { needle };
        }
    }

    model::UnwrapPattern::Generic
}

/// Find guard conditions (like starts_with/contains) in parent if-expression.
fn find_guard_condition(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    needle: &str,
) -> Option<model::UnwrapPattern> {
    // Walk up to find an if_expression
    let mut current = node.parent();
    let mut depth = 0;
    const MAX_DEPTH: usize = 15;

    while let Some(parent) = current {
        depth += 1;
        if depth > MAX_DEPTH {
            break;
        }

        if parent.kind() == "if_expression" {
            // Get the condition
            if let Some(condition) = parent.child_by_field_name("condition") {
                let cond_text = parsed.text_for_node(&condition);

                // Check for starts_with guard with same needle
                if cond_text.contains(".starts_with(") {
                    if let Some(sw_needle) = extract_method_arg(&cond_text, "starts_with") {
                        // Check if the needles match (allowing for quotes)
                        let clean_needle = needle.trim_matches('"').trim_matches('\'');
                        let clean_sw_needle = sw_needle.trim_matches('"').trim_matches('\'');
                        if clean_needle == clean_sw_needle || sw_needle.contains(clean_needle) {
                            return Some(model::UnwrapPattern::StartsWithFind {
                                needle: needle.to_string(),
                                guard_start_byte: Some(parent.start_byte()),
                            });
                        }
                    }
                }

                // Check for contains guard with same needle
                if cond_text.contains(".contains(") {
                    if let Some(c_needle) = extract_method_arg(&cond_text, "contains") {
                        let clean_needle = needle.trim_matches('"').trim_matches('\'');
                        let clean_c_needle = c_needle.trim_matches('"').trim_matches('\'');
                        if clean_needle == clean_c_needle || c_needle.contains(clean_needle) {
                            return Some(model::UnwrapPattern::ContainsFind {
                                needle: needle.to_string(),
                            });
                        }
                    }
                }

                // Check for is_some() guard
                if cond_text.contains(".is_some()") {
                    return Some(model::UnwrapPattern::IsSomeUnwrap);
                }

                // Check for is_ok() guard
                if cond_text.contains(".is_ok()") {
                    return Some(model::UnwrapPattern::IsOkUnwrap);
                }
            }
        }

        current = parent.parent();
    }

    None
}

/// Extract a string literal from an expression like `env::var("FOO")`.
fn extract_string_literal(expr: &str) -> Option<String> {
    let start = expr.find('"')?;
    let rest = &expr[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Extract the type from a parse expression like `.parse::<i32>()`.
fn extract_parse_type(expr: &str) -> Option<String> {
    if let Some(start) = expr.find(".parse::<") {
        let rest = &expr[start + 9..]; // Skip ".parse::<"
        if let Some(end) = rest.find('>') {
            return Some(rest[..end].to_string());
        }
    }
    None
}

/// Extract the argument from a method call like `.find("foo")` → `"foo"`.
fn extract_method_arg(expr: &str, method: &str) -> Option<String> {
    let pattern = format!(".{}(", method);
    let start = expr.find(&pattern)?;
    let rest = &expr[start + pattern.len()..];
    // Find matching closing paren
    let mut depth = 1;
    let mut end = 0;
    for (i, c) in rest.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }
    if end > 0 {
        Some(rest[..end].to_string())
    } else {
        None
    }
}

/// Infer the unwrap type (Result, Option, Unknown) from context.
fn infer_unwrap_type(
    pattern: &model::UnwrapPattern,
    receiver_expr: &str,
    text: &str,
) -> model::UnwrapType {
    // Patterns that always return Option
    match pattern {
        model::UnwrapPattern::CollectionGet { .. }
        | model::UnwrapPattern::FirstOrLast { .. }
        | model::UnwrapPattern::IteratorNext
        | model::UnwrapPattern::StartsWithFind { .. }
        | model::UnwrapPattern::ContainsFind { .. }
        | model::UnwrapPattern::IsSomeUnwrap => {
            return model::UnwrapType::Option;
        }
        // Patterns that always return Result
        model::UnwrapPattern::EnvVar { .. }
        | model::UnwrapPattern::Parse { .. }
        | model::UnwrapPattern::RegexNew
        | model::UnwrapPattern::LockUnwrap { .. }
        | model::UnwrapPattern::IsOkUnwrap => {
            return model::UnwrapType::Result;
        }
        model::UnwrapPattern::Generic => {}
    }

    // Fall back to text-based heuristics
    if text.contains("Ok(") || text.contains("Err(") || text.contains("Result") {
        model::UnwrapType::Result
    } else if text.contains("Some(") || text.contains("None") || text.contains("Option") {
        model::UnwrapType::Option
    } else if receiver_expr.contains(".parse(")
        || receiver_expr.contains("read(")
        || receiver_expr.contains("write(")
        || receiver_expr.contains("lock(")
    {
        model::UnwrapType::Result
    } else if receiver_expr.contains(".get(")
        || receiver_expr.contains(".find(")
        || receiver_expr.contains(".first(")
        || receiver_expr.contains(".last(")
        || receiver_expr.contains(".next(")
    {
        model::UnwrapType::Option
    } else {
        model::UnwrapType::Unknown
    }
}

/// Build an ExpectCall from analysis.
fn build_expect_call(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    text: &str,
    ctx: &TraversalContext,
) -> Option<model::ExpectCall> {
    // Extract the message from .expect("...")
    let message = if let Some(start) = text.find(".expect(\"") {
        let after = &text[start + 9..];
        if let Some(end) = after.find('"') {
            after[..end].to_string()
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    // Check if the message is meaningful
    let has_meaningful_message = !message.is_empty()
        && !message.to_lowercase().contains("should")
        && !message.to_lowercase().contains("failed")
        && message.len() > 10;

    let on_type = if text.contains("Ok(") || text.contains("Err(") {
        model::UnwrapType::Result
    } else if text.contains("Some(") || text.contains("None") {
        model::UnwrapType::Option
    } else {
        model::UnwrapType::Unknown
    };

    Some(model::ExpectCall {
        on_type,
        message,
        has_meaningful_message,
        in_test: ctx.in_test,
        in_main: ctx.in_main,
        function_name: ctx.current_function.clone(),
        expr_text: text.to_string(),
        location: parsed.location_for_node(node),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

/// Heuristic to detect likely fallible calls.
fn is_likely_fallible_call(text: &str) -> bool {
    // Common patterns that return Result or Option
    text.contains(".read(")
        || text.contains(".write(")
        || text.contains(".open(")
        || text.contains(".parse(")
        || text.contains(".send(")
        || text.contains(".recv(")
        || text.contains(".connect(")
        || text.contains(".accept(")
        || text.contains(".bind(")
        || text.contains(".query(")
        || text.contains(".execute(")
        || text.contains(".fetch(")
        || text.contains("fs::")
        || text.contains("io::")
        || text.contains("::new(")
        || text.contains("::from_str(")
}

/// Analyze unsafe patterns.
fn analyze_unsafe_patterns(parsed: &ParsedFile, sem: &mut RustFileSemantics) {
    let root = parsed.tree.root_node();
    let ctx = TraversalContext::default();
    walk_for_unsafe(root, parsed, sem, ctx);
}

/// Walk AST looking for unsafe patterns.
fn walk_for_unsafe(
    node: tree_sitter::Node,
    parsed: &ParsedFile,
    sem: &mut RustFileSemantics,
    ctx: TraversalContext,
) {
    let new_ctx = update_context(&node, parsed, &ctx);

    if node.kind() == "unsafe_block" {
        let text = parsed.text_for_node(&node);

        // Check for SAFETY comment
        let (has_safety_comment, safety_comment) = check_safety_comment(parsed, &node);

        // Detect operations inside the unsafe block
        let operations = detect_unsafe_operations(&text);

        sem.unsafe_blocks.push(model::UnsafeBlock {
            kind: model::UnsafeKind::Block,
            has_safety_comment,
            safety_comment,
            function_name: new_ctx.current_function.clone(),
            operations,
            location: parsed.location_for_node(&node),
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
        });
    }

    // Recurse
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_for_unsafe(child, parsed, sem, new_ctx.clone());
        }
    }
}

/// Check for a SAFETY comment before the unsafe block.
fn check_safety_comment(parsed: &ParsedFile, node: &tree_sitter::Node) -> (bool, Option<String>) {
    // Look at multiple previous siblings for comment (tree-sitter might have intervening nodes)
    let mut prev_opt = node.prev_sibling();
    let mut siblings_checked = 0;
    while let Some(prev) = prev_opt {
        if prev.kind() == "line_comment" || prev.kind() == "block_comment" {
            let comment = parsed.text_for_node(&prev);
            if comment.to_uppercase().contains("SAFETY") {
                return (true, Some(comment));
            }
        }
        // Only check a few siblings back
        siblings_checked += 1;
        if siblings_checked > 5 {
            break;
        }
        prev_opt = prev.prev_sibling();
    }

    // Also check if there's a comment inside at the start
    let text = parsed.text_for_node(node);
    if text.to_uppercase().contains("// SAFETY") || text.to_uppercase().contains("/* SAFETY") {
        return (true, None);
    }

    // Check lines preceding the unsafe block in the source
    let start_line = node.start_position().row;
    if start_line > 0 {
        // Look at a few preceding lines
        let source = &parsed.source;
        let lines: Vec<&str> = source.lines().collect();
        for i in (0..start_line).rev().take(3) {
            let line = lines.get(i).unwrap_or(&"").trim();
            if line.to_uppercase().contains("// SAFETY") || line.to_uppercase().contains("SAFETY:")
            {
                return (true, Some(line.to_string()));
            }
            // Stop if we hit a non-empty, non-comment line
            if !line.is_empty() && !line.starts_with("//") {
                break;
            }
        }
    }

    (false, None)
}

/// Detect what unsafe operations are in the block.
fn detect_unsafe_operations(text: &str) -> Vec<model::UnsafeOp> {
    let mut ops = Vec::new();

    if text.contains('*')
        && (text.contains("*const") || text.contains("*mut") || text.contains("*ptr"))
    {
        ops.push(model::UnsafeOp::RawPointerDeref);
    }
    if text.contains("transmute") {
        ops.push(model::UnsafeOp::Transmute);
    }
    if text.contains("static mut") {
        ops.push(model::UnsafeOp::MutableStaticAccess);
    }
    if text.contains("extern ") {
        ops.push(model::UnsafeOp::ExternCall);
    }

    ops
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::rust::parse_rust_file;
    use crate::types::context::SourceFile;

    fn parse_and_build_semantics(source: &str) -> RustFileSemantics {
        let sf = SourceFile {
            path: "test.rs".to_string(),
            language: crate::types::context::Language::Rust,
            content: source.to_string(),
        };
        let parsed = parse_rust_file(FileId(1), &sf).expect("parsing should succeed");
        build_rust_semantics(&parsed).expect("semantics should build")
    }

    #[test]
    fn collects_use_statements() {
        let src = r#"
use std::collections::HashMap;
use tokio::sync::mpsc;
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.uses.len(), 2);
        assert!(sem.async_info.uses_tokio);
    }

    #[test]
    fn collects_functions() {
        let src = r#"
fn sync_fn() {}
async fn async_fn() {}
pub fn public_fn() {}
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.functions.len(), 3);
        assert_eq!(sem.async_info.async_fn_count, 1);
    }

    #[test]
    fn detects_async_functions() {
        let src = r#"
async fn fetch_data() -> Result<String, Error> {
    Ok("data".to_string())
}
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.functions.len(), 1);
        assert!(sem.functions[0].is_async);
        assert!(sem.functions[0].returns_result);
    }

    #[test]
    fn detects_unwrap_calls() {
        let src = r#"
fn process() {
    let x = Some(1).unwrap();
    let y = result.unwrap_or_default();
}
"#;
        let sem = parse_and_build_semantics(src);
        assert!(!sem.unwrap_calls.is_empty());
    }

    #[test]
    fn detects_structs() {
        let src = r#"
#[derive(Debug, Clone)]
pub struct User {
    name: String,
}
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.structs.len(), 1);
        assert_eq!(sem.structs[0].name, "User");
        assert!(sem.structs[0].derives.contains(&"Debug".to_string()));
    }

    #[test]
    fn detects_enums() {
        let src = r#"
pub enum Status {
    Active,
    Inactive,
}
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.enums.len(), 1);
        assert_eq!(sem.enums[0].name, "Status");
    }

    #[test]
    fn detects_traits() {
        let src = r#"
pub trait Repository {
    fn find(&self, id: u64) -> Option<Self>;
}
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.traits.len(), 1);
        assert_eq!(sem.traits[0].name, "Repository");
    }

    #[test]
    fn detects_impl_blocks() {
        let src = r#"
struct Counter { value: i32 }

impl Counter {
    fn new() -> Self { Self { value: 0 } }
}
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.impls.len(), 1);
        assert_eq!(sem.impls[0].self_type, "Counter");
    }

    #[test]
    fn detects_macros() {
        let src = r#"
fn debug() {
    println!("debug info");
    dbg!(value);
}
"#;
        let sem = parse_and_build_semantics(src);
        assert!(!sem.macro_invocations.is_empty());
    }

    #[test]
    fn detects_unsafe_blocks() {
        let src = r#"
fn dangerous() {
    // SAFETY: we know the pointer is valid
    unsafe {
        *ptr = 42;
    }
}
"#;
        let sem = parse_and_build_semantics(src);
        assert!(!sem.unsafe_blocks.is_empty());
    }

    #[test]
    fn detects_statics() {
        let src = r#"
static COUNTER: AtomicUsize = AtomicUsize::new(0);
const MAX_SIZE: usize = 100;
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.statics.len(), 2);
    }

    #[test]
    fn empty_file_returns_empty_semantics() {
        let sem = parse_and_build_semantics("");
        assert!(sem.functions.is_empty());
        assert!(sem.uses.is_empty());
    }

    #[test]
    fn main_function_detected() {
        let src = "fn main() {}";
        let sem = parse_and_build_semantics(src);
        assert!(sem.functions[0].is_main);
    }

    #[test]
    fn test_function_detected() {
        let src = r#"
#[test]
fn test_something() {
    assert!(true);
}
"#;
        let sem = parse_and_build_semantics(src);
        assert!(!sem.functions.is_empty());
        // Test attribute detection
        assert!(sem.functions[0].has_test_attribute || sem.functions[0].is_test);
    }

    #[test]
    fn visibility_extraction() {
        let src = r#"
pub fn public_fn() {}
pub(crate) fn crate_fn() {}
fn private_fn() {}
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.functions.len(), 3);
    }

    #[test]
    fn cfg_test_module_marks_unwraps_as_test() {
        let src = r#"
fn production_code() {
    let x = Some(1).unwrap();
}

#[cfg(test)]
mod tests {
    fn test_helper() {
        let x = Some(1).unwrap();
    }

    #[test]
    fn actual_test() {
        let x = Some(1).unwrap();
    }
}
"#;
        let sem = parse_and_build_semantics(src);

        // Should have unwrap calls
        assert!(sem.unwrap_calls.len() >= 1, "Should detect unwrap calls");

        // The unwrap in production_code should NOT be marked as in_test
        let prod_unwraps: Vec<_> = sem
            .unwrap_calls
            .iter()
            .filter(|u| u.function_name.as_deref() == Some("production_code"))
            .collect();
        assert!(
            !prod_unwraps.is_empty(),
            "Should have unwrap in production_code"
        );
        assert!(
            !prod_unwraps[0].in_test,
            "production_code unwrap should not be in_test"
        );

        // The unwrap in test_helper (inside #[cfg(test)] mod) should be marked as in_test
        let helper_unwraps: Vec<_> = sem
            .unwrap_calls
            .iter()
            .filter(|u| u.function_name.as_deref() == Some("test_helper"))
            .collect();
        assert!(
            !helper_unwraps.is_empty(),
            "Should have unwrap in test_helper"
        );
        assert!(
            helper_unwraps[0].in_test,
            "test_helper unwrap should be in_test (inside #[cfg(test)] mod)"
        );

        // The unwrap in actual_test should also be marked as in_test
        let test_unwraps: Vec<_> = sem
            .unwrap_calls
            .iter()
            .filter(|u| u.function_name.as_deref() == Some("actual_test"))
            .collect();
        assert!(
            !test_unwraps.is_empty(),
            "Should have unwrap in actual_test"
        );
        assert!(
            test_unwraps[0].in_test,
            "actual_test unwrap should be in_test"
        );
    }

    #[test]
    fn build_rust_semantics_populates_http_calls() {
        let src = r#"
use reqwest;

async fn fetch_data() -> Result<String, reqwest::Error> {
    let client = reqwest::Client::new();
    let response = client.get("https://api.example.com/data").send().await?;
    response.text().await
}
"#;
        let sem = parse_and_build_semantics(src);

        // HTTP calls should be populated by analyze_http_calls
        assert_eq!(sem.http_calls.len(), 1);
        assert!(sem.http_calls[0].in_async_context);
        assert_eq!(sem.http_calls[0].method, crate::semantics::common::http::HttpMethod::Get);
    }

    #[test]
    fn rust_http_calls_via_common_semantics() {
        use crate::semantics::common::CommonSemantics;

        let src = r#"
use reqwest;

async fn fetch() -> Result<String, reqwest::Error> {
    reqwest::Client::new().get("https://example.com").send().await?.text().await
}
"#;
        let sem = parse_and_build_semantics(src);

        // Verify via CommonSemantics trait
        let http_calls = sem.http_calls();
        assert_eq!(http_calls.len(), 1);
    }
}
