use serde::{Deserialize, Serialize};

use crate::parse::ast::{AstLocation, FileId, ParsedFile};
use crate::types::context::Language;

use super::http::HttpCallSite;

/// Information about an empty catch block found in the code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmptyCatchBlock {
    /// 1-based line number where the catch block starts
    pub line: u32,
    /// 1-based column number
    pub column: u32,
    /// The text of the catch block
    pub text: String,
    /// Name of the enclosing function, if any
    pub function_name: Option<String>,
    /// Start byte offset of the entire catch clause
    pub start_byte: usize,
    /// End byte offset of the entire catch clause
    pub end_byte: usize,
    /// Location information
    pub location: AstLocation,
}

/// Information about a bare catch block (catch without error parameter).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BareCatchClause {
    /// 1-based line number where the catch clause starts
    pub line: u32,
    /// 1-based column number
    pub column: u32,
    /// The text of the catch clause (just the catch line)
    pub text: String,
    /// Name of the enclosing function, if any
    pub function_name: Option<String>,
    /// Start byte offset of the entire catch clause
    pub start_byte: usize,
    /// End byte offset of the entire catch clause
    pub end_byte: usize,
    /// Start byte offset of just the "catch" keyword
    pub catch_keyword_start: usize,
    /// End byte offset of just the "catch" keyword
    pub catch_keyword_end: usize,
    /// Location information
    pub location: AstLocation,
}

/// Semantic model for a single TypeScript file.
/// Framework-agnostic core + optional framework-specific views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsFileSemantics {
    pub file_id: FileId,
    pub path: String,
    pub language: Language,

    /// Original source content (for suppression comment checking)
    pub source: String,

    /// Raw imports
    pub imports: Vec<TsImport>,

    /// Top-level functions and methods
    pub functions: Vec<TsFunction>,

    /// Classes defined in the file
    pub classes: Vec<TsClass>,

    /// Top-level variable declarations
    pub variables: Vec<TsVariable>,

    /// Call sites we care about (function/method calls)
    pub calls: Vec<TsCallSite>,

    /// HTTP client calls
    pub http_calls: Vec<HttpCallSite>,

    /// Empty catch blocks
    pub empty_catches: Vec<EmptyCatchBlock>,

    /// Bare catch clauses (catch without error parameter)
    pub bare_catches: Vec<BareCatchClause>,

    /// Express.js related semantics
    pub express: Option<ExpressFileSummary>,

    /// Async functions without proper error handling
    pub async_without_error_handling: Vec<AsyncWithoutErrorHandling>,

    /// Global mutable state (module-level let/var)
    pub global_mutable_state: Vec<GlobalMutableState>,
}

/// Representation of a TypeScript import statement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsImport {
    /// The module path, e.g. "express", "./utils", "@nestjs/common"
    pub module: String,

    /// Default import name, e.g. "express" in `import express from 'express'`
    pub default_import: Option<String>,

    /// Named imports, e.g. ["Router", "Request"] in `import { Router, Request } from 'express'`
    pub named_imports: Vec<String>,

    /// Namespace import, e.g. "fs" in `import * as fs from 'fs'`
    pub namespace_import: Option<String>,

    /// Whether this is a type-only import
    pub is_type_only: bool,

    pub location: AstLocation,
}

/// Representation of a TypeScript function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsFunction {
    pub name: String,
    pub is_async: bool,
    pub is_generator: bool,
    pub is_exported: bool,
    pub params: Vec<TsParam>,
    pub return_type: Option<String>,
    pub location: AstLocation,
    /// Whether this function has a try-catch block
    pub has_try_catch: bool,
    /// Calls made inside this function
    pub inner_calls: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsParam {
    pub name: String,
    pub type_annotation: Option<String>,
    pub default_value: Option<String>,
    pub is_optional: bool,
    pub is_rest: bool,
}

/// Representation of a TypeScript class
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsClass {
    pub name: String,
    pub is_exported: bool,
    pub is_abstract: bool,
    pub extends: Option<String>,
    pub implements: Vec<String>,
    pub decorators: Vec<String>,
    pub methods: Vec<TsMethod>,
    pub properties: Vec<TsProperty>,
    pub location: AstLocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsMethod {
    pub name: String,
    pub is_async: bool,
    pub is_static: bool,
    pub is_private: bool,
    pub is_protected: bool,
    pub decorators: Vec<String>,
    pub params: Vec<TsParam>,
    pub return_type: Option<String>,
    pub location: AstLocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsProperty {
    pub name: String,
    pub is_static: bool,
    pub is_private: bool,
    pub is_protected: bool,
    pub is_readonly: bool,
    pub type_annotation: Option<String>,
    pub location: AstLocation,
}

/// Representation of a TypeScript variable declaration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsVariable {
    pub name: String,
    pub kind: VariableKind,
    pub type_annotation: Option<String>,
    pub value_repr: String,
    pub is_exported: bool,
    pub location: AstLocation,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum VariableKind {
    Const,
    Let,
    Var,
}

/// Representation of a function/method call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsCallSite {
    /// e.g. "express", "app.get", "fetch"
    pub callee: String,

    /// Arguments for the call
    pub args: Vec<TsCallArg>,

    /// Full text representation of the arguments
    pub args_repr: String,

    /// Whether this call is inside a loop
    pub in_loop: bool,

    /// Whether this call is awaited
    pub is_awaited: bool,

    /// Start byte offset of the call
    pub start_byte: usize,

    /// End byte offset of the call
    pub end_byte: usize,

    pub location: AstLocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TsCallArg {
    pub name: Option<String>,
    pub value_repr: String,
}

/// Express.js-specific semantics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpressFileSummary {
    /// Express app instances (e.g., `const app = express()`)
    pub apps: Vec<ExpressApp>,
    /// Router instances
    pub routers: Vec<ExpressRouter>,
    /// Route handlers
    pub routes: Vec<ExpressRoute>,
    /// Middleware registrations
    pub middlewares: Vec<ExpressMiddleware>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpressApp {
    pub variable_name: String,
    pub location: AstLocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpressRouter {
    pub variable_name: String,
    pub location: AstLocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpressRoute {
    pub method: String,
    pub path: Option<String>,
    pub handler_name: Option<String>,
    pub is_async: bool,
    pub has_error_handler: bool,
    pub location: AstLocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpressMiddleware {
    pub middleware_name: String,
    pub location: AstLocation,
}

/// Async operation without proper error handling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsyncWithoutErrorHandling {
    pub function_name: String,
    pub is_promise_based: bool,
    pub location: AstLocation,
}

/// Global mutable state detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalMutableState {
    pub variable_name: String,
    pub kind: VariableKind,
    pub location: AstLocation,
    /// The initial value representation (empty string if no initializer)
    pub value_repr: String,
    /// Whether the variable has a type annotation
    pub has_type_annotation: bool,
    /// Start byte offset of the keyword (let/var)
    pub keyword_start_byte: usize,
    /// End byte offset of the keyword (let/var)
    pub keyword_end_byte: usize,
}

impl TsFileSemantics {
    /// Build the semantic model from a parsed TypeScript file.
    pub fn from_parsed(parsed: &ParsedFile) -> Self {
        let mut sem = TsFileSemantics {
            file_id: parsed.file_id,
            path: parsed.path.clone(),
            language: parsed.language,
            source: (*parsed.source).clone(),
            imports: Vec::new(),
            functions: Vec::new(),
            classes: Vec::new(),
            variables: Vec::new(),
            calls: Vec::new(),
            http_calls: Vec::new(),
            empty_catches: Vec::new(),
            bare_catches: Vec::new(),
            express: None,
            async_without_error_handling: Vec::new(),
            global_mutable_state: Vec::new(),
        };

        if parsed.language == Language::Typescript {
            collect_semantics(parsed, &mut sem);
        }

        sem
    }

    /// Run framework-specific analysis (Express, NestJS, etc.)
    pub fn analyze_frameworks(&mut self, parsed: &ParsedFile) -> anyhow::Result<()> {
        // Express.js analysis
        let express_summary = super::express::summarize_express(parsed);
        if express_summary.is_some() {
            self.express = express_summary;
        }

        // HTTP client calls
        self.http_calls = super::http::summarize_http_clients(parsed);

        Ok(())
    }
}

/// Context for tracking loop nesting during AST traversal.
#[derive(Default, Clone)]
struct TraversalContext {
    in_loop: bool,
    current_function: Option<String>,
}

/// Collect semantics by walking the tree-sitter AST.
fn collect_semantics(parsed: &ParsedFile, sem: &mut TsFileSemantics) {
    let root = parsed.tree.root_node();
    let ctx = TraversalContext::default();
    walk_nodes_with_context(root, parsed, sem, ctx);
}

/// Walk nodes while tracking loop context.
fn walk_nodes_with_context(
    node: tree_sitter::Node,
    parsed: &ParsedFile,
    sem: &mut TsFileSemantics,
    ctx: TraversalContext,
) {
    // Update context based on current node
    let new_ctx = match node.kind() {
        "for_statement" | "for_in_statement" | "while_statement" | "do_statement" => {
            TraversalContext {
                in_loop: true,
                ..ctx.clone()
            }
        }
        "function_declaration" | "function" | "arrow_function" | "method_definition" => {
            let func_name = node
                .child_by_field_name("name")
                .map(|n| parsed.text_for_node(&n));
            TraversalContext {
                current_function: func_name,
                ..ctx.clone()
            }
        }
        _ => ctx.clone(),
    };

    // Process current node
    match node.kind() {
        "import_statement" => {
            if let Some(imp) = build_import(parsed, &node) {
                sem.imports.push(imp);
            }
        }
        "lexical_declaration" | "variable_declaration" => {
            if node.parent().map(|p| p.kind()) == Some("program") {
                // Top-level variable
                if let Some(var) = build_variable(parsed, &node) {
                    // Check for global mutable state
                    if var.kind != VariableKind::Const {
                        // Find the keyword byte positions
                        let _text = parsed.text_for_node(&node);
                        let keyword = if var.kind == VariableKind::Let {
                            "let"
                        } else {
                            "var"
                        };
                        let keyword_start = node.start_byte();
                        let keyword_end = keyword_start + keyword.len();

                        sem.global_mutable_state.push(GlobalMutableState {
                            variable_name: var.name.clone(),
                            kind: var.kind,
                            location: var.location.clone(),
                            value_repr: var.value_repr.clone(),
                            has_type_annotation: var.type_annotation.is_some(),
                            keyword_start_byte: keyword_start,
                            keyword_end_byte: keyword_end,
                        });
                    }
                    sem.variables.push(var);
                }
            }
        }
        "function_declaration" => {
            if let Some(fun) = build_function(parsed, &node) {
                sem.functions.push(fun);
            }
        }
        "class_declaration" => {
            if let Some(class) = build_class(parsed, &node) {
                sem.classes.push(class);
            }
        }
        "call_expression" => {
            if let Some(call) = build_callsite(parsed, &node, &new_ctx) {
                sem.calls.push(call);
            }
        }
        "new_expression" => {
            if let Some(call) = build_new_expression(parsed, &node, &new_ctx) {
                sem.calls.push(call);
            }
        }
        "catch_clause" => {
            // Check for empty catch block and bare catch
            check_catch_clause(parsed, &node, sem, &new_ctx);
        }
        _ => {}
    }

    // Recurse into children
    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            walk_nodes_with_context(child, parsed, sem, new_ctx.clone());
        }
    }
}

fn build_import(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<TsImport> {
    let location = parsed.location_for_node(node);
    let text = parsed.text_for_node(node);

    // Parse the import statement
    let mut module = String::new();
    let mut default_import = None;
    let mut named_imports = Vec::new();
    let mut namespace_import = None;
    let is_type_only = text.contains("import type");

    // Find the source (module path)
    if let Some(source_node) = node.child_by_field_name("source") {
        module = parsed.text_for_node(&source_node);
        // Remove quotes
        module = module.trim_matches(|c| c == '\'' || c == '"').to_string();
    }

    // Find imports
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            match child.kind() {
                "import_clause" => {
                    // Parse import clause for default and named imports
                    for j in 0..child.child_count() {
                        if let Some(import_child) = child.child(j) {
                            match import_child.kind() {
                                "identifier" => {
                                    default_import = Some(parsed.text_for_node(&import_child));
                                }
                                "named_imports" => {
                                    named_imports = extract_named_imports(parsed, &import_child);
                                }
                                "namespace_import" => {
                                    // import * as name
                                    // Try field "name" first, then look for identifier child
                                    if let Some(name_node) =
                                        import_child.child_by_field_name("name")
                                    {
                                        namespace_import = Some(parsed.text_for_node(&name_node));
                                    } else {
                                        // Fallback: look for identifier child after "* as"
                                        for k in 0..import_child.child_count() {
                                            if let Some(ns_child) = import_child.child(k) {
                                                if ns_child.kind() == "identifier" {
                                                    namespace_import =
                                                        Some(parsed.text_for_node(&ns_child));
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if module.is_empty() {
        return None;
    }

    Some(TsImport {
        module,
        default_import,
        named_imports,
        namespace_import,
        is_type_only,
        location,
    })
}

fn extract_named_imports(parsed: &ParsedFile, node: &tree_sitter::Node) -> Vec<String> {
    let mut imports = Vec::new();

    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            if child.kind() == "import_specifier" {
                // Get the name being imported
                if let Some(name_node) = child.child_by_field_name("name") {
                    imports.push(parsed.text_for_node(&name_node));
                } else {
                    // Fallback: get text of first identifier child
                    for j in 0..child.child_count() {
                        if let Some(id) = child.child(j) {
                            if id.kind() == "identifier" {
                                imports.push(parsed.text_for_node(&id));
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    imports
}

fn build_variable(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<TsVariable> {
    let location = parsed.location_for_node(node);
    let text = parsed.text_for_node(node);

    // Determine kind
    let kind = if text.starts_with("const") {
        VariableKind::Const
    } else if text.starts_with("let") {
        VariableKind::Let
    } else if text.starts_with("var") {
        VariableKind::Var
    } else {
        return None;
    };

    // Check if exported
    let is_exported = node
        .parent()
        .map(|p| p.kind() == "export_statement")
        .unwrap_or(false);

    // Find the variable declarator
    let mut name = String::new();
    let mut type_annotation = None;
    let mut value_repr = String::new();

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "variable_declarator" {
                // Get name
                if let Some(name_node) = child.child_by_field_name("name") {
                    name = parsed.text_for_node(&name_node);
                }
                // Get type annotation
                if let Some(type_node) = child.child_by_field_name("type") {
                    type_annotation = Some(parsed.text_for_node(&type_node));
                }
                // Get value
                if let Some(value_node) = child.child_by_field_name("value") {
                    value_repr = parsed.text_for_node(&value_node);
                }
            }
        }
    }

    if name.is_empty() {
        return None;
    }

    Some(TsVariable {
        name,
        kind,
        type_annotation,
        value_repr,
        is_exported,
        location,
    })
}

fn build_function(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<TsFunction> {
    let location = parsed.location_for_node(node);
    let text = parsed.text_for_node(node);

    let name_node = node.child_by_field_name("name")?;
    let name = parsed.text_for_node(&name_node);

    let is_async = text.trim_start().starts_with("async");
    let is_generator = text.contains("function*") || text.contains("function *");
    let is_exported = node
        .parent()
        .map(|p| p.kind() == "export_statement")
        .unwrap_or(false);

    // Extract parameters
    let params = if let Some(params_node) = node.child_by_field_name("parameters") {
        extract_params(parsed, &params_node)
    } else {
        Vec::new()
    };

    // Extract return type
    let return_type = node
        .child_by_field_name("return_type")
        .map(|n| parsed.text_for_node(&n));

    // Check for try-catch
    let has_try_catch = has_try_catch_in_body(node);

    Some(TsFunction {
        name,
        is_async,
        is_generator,
        is_exported,
        params,
        return_type,
        location,
        has_try_catch,
        inner_calls: Vec::new(),
    })
}

fn extract_params(parsed: &ParsedFile, params_node: &tree_sitter::Node) -> Vec<TsParam> {
    let mut params = Vec::new();

    for i in 0..params_node.named_child_count() {
        if let Some(param_node) = params_node.named_child(i) {
            let mut name = String::new();
            let mut type_annotation = None;
            let mut default_value = None;
            let mut is_optional = false;
            let mut is_rest = false;

            match param_node.kind() {
                "required_parameter" | "optional_parameter" => {
                    is_optional = param_node.kind() == "optional_parameter";

                    if let Some(pattern) = param_node.child_by_field_name("pattern") {
                        name = parsed.text_for_node(&pattern);
                    }
                    if let Some(type_node) = param_node.child_by_field_name("type") {
                        type_annotation = Some(parsed.text_for_node(&type_node));
                    }
                    if let Some(value_node) = param_node.child_by_field_name("value") {
                        default_value = Some(parsed.text_for_node(&value_node));
                    }
                }
                "rest_parameter" => {
                    is_rest = true;
                    if let Some(pattern) = param_node.child_by_field_name("pattern") {
                        name = parsed.text_for_node(&pattern);
                    }
                    if let Some(type_node) = param_node.child_by_field_name("type") {
                        type_annotation = Some(parsed.text_for_node(&type_node));
                    }
                }
                "identifier" => {
                    name = parsed.text_for_node(&param_node);
                }
                _ => {
                    name = parsed.text_for_node(&param_node);
                }
            }

            if !name.is_empty() {
                params.push(TsParam {
                    name,
                    type_annotation,
                    default_value,
                    is_optional,
                    is_rest,
                });
            }
        }
    }

    params
}

fn has_try_catch_in_body(node: &tree_sitter::Node) -> bool {
    fn check_node(node: tree_sitter::Node) -> bool {
        if node.kind() == "try_statement" {
            return true;
        }
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if check_node(child) {
                    return true;
                }
            }
        }
        false
    }

    if let Some(body) = node.child_by_field_name("body") {
        check_node(body)
    } else {
        false
    }
}

fn build_class(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<TsClass> {
    let location = parsed.location_for_node(node);
    let text = parsed.text_for_node(node);

    let name_node = node.child_by_field_name("name")?;
    let name = parsed.text_for_node(&name_node);

    let is_exported = node
        .parent()
        .map(|p| p.kind() == "export_statement")
        .unwrap_or(false);
    let is_abstract = text.contains("abstract class");

    // Extract extends
    let extends = node.child_by_field_name("heritage").and_then(|h| {
        for i in 0..h.child_count() {
            if let Some(child) = h.child(i) {
                if child.kind() == "extends_clause" {
                    return child
                        .child_by_field_name("value")
                        .map(|n| parsed.text_for_node(&n));
                }
            }
        }
        None
    });

    // Extract implements
    let mut implements = Vec::new();
    if let Some(heritage) = node.child_by_field_name("heritage") {
        for i in 0..heritage.child_count() {
            if let Some(child) = heritage.child(i) {
                if child.kind() == "implements_clause" {
                    for j in 0..child.named_child_count() {
                        if let Some(type_node) = child.named_child(j) {
                            implements.push(parsed.text_for_node(&type_node));
                        }
                    }
                }
            }
        }
    }

    // Extract decorators
    let decorators = extract_decorators(parsed, node);

    // Extract methods and properties
    let (methods, properties) = extract_class_members(parsed, node);

    Some(TsClass {
        name,
        is_exported,
        is_abstract,
        extends,
        implements,
        decorators,
        methods,
        properties,
        location,
    })
}

fn extract_decorators(parsed: &ParsedFile, node: &tree_sitter::Node) -> Vec<String> {
    let mut decorators = Vec::new();

    // Check parent for decorator
    if let Some(parent) = node.parent() {
        if parent.kind() == "export_statement" {
            if let Some(grandparent) = parent.parent() {
                // Look for decorators in siblings before this node
                for i in 0..grandparent.child_count() {
                    if let Some(sibling) = grandparent.child(i) {
                        if sibling.kind() == "decorator" {
                            decorators.push(parsed.text_for_node(&sibling));
                        }
                    }
                }
            }
        }
    }

    // Also check direct children
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "decorator" {
                decorators.push(parsed.text_for_node(&child));
            }
        }
    }

    decorators
}

fn extract_class_members(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
) -> (Vec<TsMethod>, Vec<TsProperty>) {
    let mut methods = Vec::new();
    let mut properties = Vec::new();

    if let Some(body) = node.child_by_field_name("body") {
        for i in 0..body.named_child_count() {
            if let Some(member) = body.named_child(i) {
                match member.kind() {
                    "method_definition" => {
                        if let Some(method) = build_method(parsed, &member) {
                            methods.push(method);
                        }
                    }
                    "public_field_definition" | "private_field_definition" => {
                        if let Some(prop) = build_property(parsed, &member) {
                            properties.push(prop);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    (methods, properties)
}

fn build_method(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<TsMethod> {
    let location = parsed.location_for_node(node);
    let text = parsed.text_for_node(node);

    let name_node = node.child_by_field_name("name")?;
    let name = parsed.text_for_node(&name_node);

    let is_async = text.contains("async ");
    let is_static = text.contains("static ");
    let is_private = name.starts_with('#') || text.contains("private ");
    let is_protected = text.contains("protected ");

    let decorators = extract_decorators(parsed, node);

    let params = if let Some(params_node) = node.child_by_field_name("parameters") {
        extract_params(parsed, &params_node)
    } else {
        Vec::new()
    };

    let return_type = node
        .child_by_field_name("return_type")
        .map(|n| parsed.text_for_node(&n));

    Some(TsMethod {
        name,
        is_async,
        is_static,
        is_private,
        is_protected,
        decorators,
        params,
        return_type,
        location,
    })
}

fn build_property(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<TsProperty> {
    let location = parsed.location_for_node(node);
    let text = parsed.text_for_node(node);

    let name_node = node.child_by_field_name("name")?;
    let name = parsed.text_for_node(&name_node);

    let is_static = text.contains("static ");
    let is_private = node.kind() == "private_field_definition" || text.contains("private ");
    let is_protected = text.contains("protected ");
    let is_readonly = text.contains("readonly ");

    let type_annotation = node
        .child_by_field_name("type")
        .map(|n| parsed.text_for_node(&n));

    Some(TsProperty {
        name,
        is_static,
        is_private,
        is_protected,
        is_readonly,
        type_annotation,
        location,
    })
}

fn build_callsite(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    ctx: &TraversalContext,
) -> Option<TsCallSite> {
    let location = parsed.location_for_node(node);

    // Get the function being called
    let func_node = node.child_by_field_name("function")?;
    let callee = parsed.text_for_node(&func_node);

    // Get arguments
    let args_repr = if let Some(args_node) = node.child_by_field_name("arguments") {
        parsed.text_for_node(&args_node)
    } else {
        String::new()
    };

    let mut args = Vec::new();
    if let Some(args_node) = node.child_by_field_name("arguments") {
        for i in 0..args_node.named_child_count() {
            if let Some(arg_node) = args_node.named_child(i) {
                let value_repr = parsed.text_for_node(&arg_node);
                args.push(TsCallArg {
                    name: None,
                    value_repr,
                });
            }
        }
    }

    // Check if this call is awaited
    let is_awaited = node
        .parent()
        .map(|p| p.kind() == "await_expression")
        .unwrap_or(false);

    Some(TsCallSite {
        callee,
        args,
        args_repr,
        in_loop: ctx.in_loop,
        is_awaited,
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        location,
    })
}

fn build_new_expression(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    ctx: &TraversalContext,
) -> Option<TsCallSite> {
    let location = parsed.location_for_node(node);

    // Get the constructor being called
    let constructor_node = node.child_by_field_name("constructor")?;
    let constructor_name = parsed.text_for_node(&constructor_node);
    let callee = format!("new {}", constructor_name);

    // Get arguments
    let args_repr = if let Some(args_node) = node.child_by_field_name("arguments") {
        parsed.text_for_node(&args_node)
    } else {
        String::new()
    };

    let mut args = Vec::new();
    if let Some(args_node) = node.child_by_field_name("arguments") {
        for i in 0..args_node.named_child_count() {
            if let Some(arg_node) = args_node.named_child(i) {
                let value_repr = parsed.text_for_node(&arg_node);
                args.push(TsCallArg {
                    name: None,
                    value_repr,
                });
            }
        }
    }

    // Check if this call is awaited
    let is_awaited = node
        .parent()
        .map(|p| p.kind() == "await_expression")
        .unwrap_or(false);

    Some(TsCallSite {
        callee,
        args,
        args_repr,
        in_loop: ctx.in_loop,
        is_awaited,
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        location,
    })
}

fn check_catch_clause(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    sem: &mut TsFileSemantics,
    ctx: &TraversalContext,
) {
    let location = parsed.location_for_node(node);
    let text = parsed.text_for_node(node);
    let range = node.range();

    // Check if catch has parameter (not bare)
    let has_parameter = node.child_by_field_name("parameter").is_some();

    // Check if catch body is empty
    let is_empty = if let Some(body) = node.child_by_field_name("body") {
        body.named_child_count() == 0
    } else {
        false
    };

    if is_empty {
        sem.empty_catches.push(EmptyCatchBlock {
            line: range.start_point.row as u32 + 1,
            column: range.start_point.column as u32 + 1,
            text: text.lines().next().unwrap_or(&text).to_string(),
            function_name: ctx.current_function.clone(),
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            location: location.clone(),
        });
    }

    if !has_parameter {
        sem.bare_catches.push(BareCatchClause {
            line: range.start_point.row as u32 + 1,
            column: range.start_point.column as u32 + 1,
            text: text.lines().next().unwrap_or(&text).to_string(),
            function_name: ctx.current_function.clone(),
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            catch_keyword_start: node.start_byte(),
            catch_keyword_end: node.start_byte() + 5, // "catch"
            location,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::typescript::parse_typescript_file;
    use crate::types::context::SourceFile;

    fn parse_and_build_semantics(source: &str) -> TsFileSemantics {
        let sf = SourceFile {
            path: "test.ts".to_string(),
            language: Language::Typescript,
            content: source.to_string(),
        };
        let parsed = parse_typescript_file(FileId(1), &sf).expect("parsing should succeed");
        TsFileSemantics::from_parsed(&parsed)
    }

    #[test]
    fn collects_simple_import() {
        let sem = parse_and_build_semantics("import express from 'express';");
        assert_eq!(sem.imports.len(), 1);
        assert_eq!(sem.imports[0].module, "express");
        assert_eq!(sem.imports[0].default_import, Some("express".to_string()));
    }

    #[test]
    fn collects_named_imports() {
        let sem = parse_and_build_semantics("import { Router, Request, Response } from 'express';");
        assert_eq!(sem.imports.len(), 1);
        assert_eq!(sem.imports[0].module, "express");
        assert!(sem.imports[0].named_imports.contains(&"Router".to_string()));
    }

    #[test]
    fn collects_namespace_import() {
        let sem = parse_and_build_semantics("import * as fs from 'fs';");
        assert_eq!(sem.imports.len(), 1);
        assert_eq!(sem.imports[0].module, "fs");
        assert_eq!(sem.imports[0].namespace_import, Some("fs".to_string()));
    }

    #[test]
    fn collects_function() {
        let sem = parse_and_build_semantics("function hello() {}");
        assert_eq!(sem.functions.len(), 1);
        assert_eq!(sem.functions[0].name, "hello");
        assert!(!sem.functions[0].is_async);
    }

    #[test]
    fn collects_async_function() {
        let sem = parse_and_build_semantics("async function fetchData() {}");
        assert_eq!(sem.functions.len(), 1);
        assert_eq!(sem.functions[0].name, "fetchData");
        assert!(sem.functions[0].is_async);
    }

    #[test]
    fn collects_class() {
        let src = r#"
class MyClass {
    private value: number;
    
    constructor(value: number) {
        this.value = value;
    }
    
    getValue(): number {
        return this.value;
    }
}
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.classes.len(), 1);
        assert_eq!(sem.classes[0].name, "MyClass");
        assert!(!sem.classes[0].methods.is_empty());
    }

    #[test]
    fn collects_calls() {
        let sem = parse_and_build_semantics("fetch('https://api.example.com');");
        assert!(!sem.calls.is_empty());
        assert!(sem.calls.iter().any(|c| c.callee == "fetch"));
    }

    #[test]
    fn detects_global_mutable_state() {
        let sem = parse_and_build_semantics("let globalState = {};");
        assert_eq!(sem.global_mutable_state.len(), 1);
        assert_eq!(sem.global_mutable_state[0].variable_name, "globalState");
    }

    #[test]
    fn const_is_not_mutable() {
        let sem = parse_and_build_semantics("const config = {};");
        assert!(sem.global_mutable_state.is_empty());
        assert_eq!(sem.variables.len(), 1);
        assert_eq!(sem.variables[0].kind, VariableKind::Const);
    }

    #[test]
    fn detects_empty_catch() {
        let src = r#"
try {
    risky();
} catch (e) {
}
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.empty_catches.len(), 1);
    }

    #[test]
    fn handles_empty_file() {
        let sem = parse_and_build_semantics("");
        assert!(sem.imports.is_empty());
        assert!(sem.functions.is_empty());
        assert!(sem.classes.is_empty());
        assert!(sem.variables.is_empty());
    }
}

/// Determines if a TypeScript file is server-side code.
///
/// This is used by rules that only apply to server-side TypeScript/JavaScript
/// (e.g., structured logging requirements, HTTP timeout requirements).
/// Client-side code (browser) has different constraints and built-in behaviors.
///
/// Server-side indicators:
/// - Express.js framework detected
/// - Node.js built-in module imports (http, https, fs, path, crypto, etc.)
/// - Server framework imports (express, fastify, koa, hapi, nestjs, etc.)
///
/// Excluded contexts (not server-side for these rules):
/// - VS Code extensions (vscode import)
/// - Browser extension APIs
/// - Electron renderer processes
/// - Development tools
pub fn is_server_side_code(ts: &TsFileSemantics) -> bool {
    // First, check for contexts that should NOT be considered server-side,
    // even though they run in Node.js. These are development tools where
    // console.log is standard practice.
    const EXCLUDED_TOOL_IMPORTS: &[&str] = &[
        "vscode",                // VS Code extensions - console.log is standard
        "electron",              // Electron apps (renderer process)
        "@electron/remote",      // Electron remote
        "webextension-polyfill", // Browser extensions
        "jest",                  // Test frameworks - console.log is fine
        "mocha",
        "vitest",
    ];

    // Check if this is a VS Code extension or other excluded tool
    for import in &ts.imports {
        let module = import.module.as_str();
        if EXCLUDED_TOOL_IMPORTS.contains(&module) {
            return false;
        }
    }

    // Check if Express.js framework is detected
    if ts.express.is_some() {
        return true;
    }

    // Server framework imports - these are strong indicators of server-side code
    const SERVER_FRAMEWORKS: &[&str] = &[
        "express",
        "fastify",
        "koa",
        "hapi",
        "@hapi/hapi",
        "@nestjs/common",
        "@nestjs/core",
        "restify",
        "polka",
        "micro",
    ];

    // HTTP-related Node.js modules - strong indicators of server-side code
    // Note: fs, path, os, crypto are NOT included because they're commonly used
    // in CLI tools, VS Code extensions, build scripts, etc.
    const HTTP_SERVER_MODULES: &[&str] = &[
        "http",
        "https",
        "http2",
        "net",
        "tls",
        "dgram",
        "cluster",
        "node:http",
        "node:https",
        "node:http2",
        "node:net",
        "node:tls",
    ];

    // Check imports for server-side indicators
    for import in &ts.imports {
        let module = import.module.as_str();

        // Check server frameworks
        if SERVER_FRAMEWORKS.contains(&module) {
            return true;
        }

        // Check HTTP server modules
        if HTTP_SERVER_MODULES.contains(&module) {
            return true;
        }
    }

    false
}
