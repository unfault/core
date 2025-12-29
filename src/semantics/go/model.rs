use serde::{Deserialize, Serialize};

use crate::parse::ast::{AstLocation, FileId, ParsedFile};
use crate::types::context::Language;
use crate::semantics::common::{calls::FunctionCall, CommonLocation};
use crate::semantics::common::db::{DbOperation, DbLibrary, DbOperationType};

use super::frameworks::{extract_go_routes, GoFrameworkSummary};
use super::http::HttpCallSite;

/// Information about an unchecked error in Go code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncheckedError {
    /// 1-based line number
    pub line: u32,
    /// 1-based column number
    pub column: u32,
    /// The text of the call expression
    pub call_text: String,
    /// Name of the enclosing function, if any
    pub function_name: Option<String>,
    /// Start byte offset
    pub start_byte: usize,
    /// End byte offset
    pub end_byte: usize,
    /// Location information
    pub location: AstLocation,
}

/// Information about a goroutine spawn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoroutineSpawn {
    /// 1-based line number
    pub line: u32,
    /// 1-based column number
    pub column: u32,
    /// The text of the go statement
    pub text: String,
    /// Whether this goroutine has error handling (recover)
    pub has_recover: bool,
    /// Whether the goroutine receives a context parameter
    pub has_context_param: bool,
    /// Whether the goroutine uses a done channel for cancellation
    pub has_done_channel: bool,
    /// Whether the goroutine has an unbounded channel send
    pub has_unbounded_channel_send: bool,
    /// Whether this is an anonymous function goroutine
    pub is_anonymous: bool,
    /// Name of the enclosing function
    pub function_name: Option<String>,
    /// Start byte offset
    pub start_byte: usize,
    /// End byte offset
    pub end_byte: usize,
    /// Location information
    pub location: AstLocation,
}

/// Information about a channel operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelOp {
    /// Kind of channel operation
    pub kind: ChannelOpKind,
    /// 1-based line number
    pub line: u32,
    /// 1-based column number
    pub column: u32,
    /// The text of the channel operation
    pub text: String,
    /// Whether this is inside a select statement
    pub in_select: bool,
    /// Name of the enclosing function
    pub function_name: Option<String>,
    /// Start byte offset
    pub start_byte: usize,
    /// End byte offset
    pub end_byte: usize,
    /// Location information
    pub location: AstLocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChannelOpKind {
    Send,
    Receive,
    Close,
}

/// Information about a defer statement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeferStatement {
    /// 1-based line number
    pub line: u32,
    /// 1-based column number
    pub column: u32,
    /// The text of the defer statement
    pub text: String,
    /// The text of the call expression inside defer
    pub call_text: String,
    /// Whether this defer is inside a loop
    pub in_loop: bool,
    /// Whether this defer closes a resource (file, connection, etc.)
    pub is_resource_cleanup: bool,
    /// Name of the enclosing function
    pub function_name: Option<String>,
    /// Start byte offset
    pub start_byte: usize,
    /// End byte offset
    pub end_byte: usize,
    /// Location information
    pub location: AstLocation,
}

/// Information about a context usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextUsage {
    /// 1-based line number
    pub line: u32,
    /// 1-based column number
    pub column: u32,
    /// The text of the context usage
    pub text: String,
    /// Type of context call: "Background", "TODO", "WithTimeout", etc.
    pub context_type: String,
    /// Whether context.Background() or context.TODO() is used
    pub is_background_or_todo: bool,
    /// Whether there's a timeout/deadline set
    pub has_timeout: bool,
    /// Whether this context usage is inside an HTTP/RPC handler
    pub in_handler: bool,
    /// Type of handler if in_handler is true: "http", "gin", "echo", "fiber", "grpc"
    pub handler_type: Option<String>,
    /// Name of the enclosing function
    pub function_name: String,
    /// Start byte offset
    pub start_byte: usize,
    /// End byte offset
    pub end_byte: usize,
    /// Location information
    pub location: AstLocation,
}

/// Semantic model for a single Go file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoFileSemantics {
    pub file_id: FileId,
    pub path: String,
    pub language: Language,

    /// Package name
    pub package_name: String,

    /// Imports
    pub imports: Vec<GoImport>,

    /// Function declarations
    pub functions: Vec<GoFunction>,

    /// Type declarations (struct, interface)
    pub types: Vec<GoTypeDecl>,

    /// Method declarations
    pub methods: Vec<GoMethod>,

    /// Variable/constant declarations
    pub declarations: Vec<GoDeclaration>,

    /// Call sites
    pub calls: Vec<GoCallSite>,

    /// HTTP client calls (net/http, etc.)
    pub http_calls: Vec<HttpCallSite>,

    /// Unchecked errors
    pub unchecked_errors: Vec<UncheckedError>,

    /// Goroutine spawns
    pub goroutines: Vec<GoroutineSpawn>,

    /// Channel operations
    pub channel_ops: Vec<ChannelOp>,

    /// Defer statements
    pub defers: Vec<DeferStatement>,

    /// Context usages
    pub context_usages: Vec<ContextUsage>,

    /// Mutex operations
    pub mutex_operations: Vec<MutexOperation>,

    /// Go HTTP framework routes (Gin, Echo, Fiber, Chi, etc.)
    pub go_framework: Option<GoFrameworkSummary>,

    /// Database operations (database/sql, GORM, sqlx, etc.)
    pub db_operations: Vec<DbOperation>,
}

/// Information about a mutex operation (Lock/Unlock, RLock/RUnlock).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutexOperation {
    /// 1-based line number of the lock
    pub lock_line: u32,
    /// 1-based column number of the lock
    pub lock_column: u32,
    /// The text of the mutex lock operation
    pub text: String,
    /// Name of the mutex variable
    pub mutex_var: String,
    /// Type of operation: "Lock", "Unlock", "RLock", "RUnlock"
    pub operation_type: String,
    /// Whether this is an RLock (vs Lock)
    pub is_rlock: bool,
    /// Whether this lock uses defer for unlock
    pub uses_defer_unlock: bool,
    /// Whether this is an empty critical section (lock followed immediately by unlock)
    pub is_empty_critical_section: bool,
    /// Name of the enclosing function
    pub function_name: Option<String>,
    /// Start byte offset of the lock
    pub lock_start_byte: usize,
    /// End byte offset of the lock
    pub lock_end_byte: usize,
    /// Location information
    pub location: AstLocation,
}

/// Representation of a Go import.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoImport {
    /// The import path, e.g., "net/http", "github.com/gin-gonic/gin"
    pub path: String,
    /// Optional alias, e.g., "mux" in `import mux "github.com/gorilla/mux"`
    pub alias: Option<String>,
    /// Whether this is a blank import (import _ "...")
    pub is_blank: bool,
    /// Whether this is a dot import (import . "...")
    pub is_dot: bool,
    /// Location
    pub location: AstLocation,
}

/// Representation of a Go function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoFunction {
    pub name: String,
    pub params: Vec<GoParam>,
    pub return_types: Vec<String>,
    /// Whether this function returns an error (last return type is "error")
    pub returns_error: bool,
    /// Location
    pub location: AstLocation,
    /// Start byte offset for patching
    pub start_byte: usize,
    /// End byte offset for patching
    pub end_byte: usize,
}

/// Representation of a Go method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoMethod {
    pub name: String,
    /// Receiver type, e.g., "*Server" or "Server"
    pub receiver_type: String,
    /// Whether receiver is a pointer
    pub receiver_is_pointer: bool,
    pub params: Vec<GoParam>,
    pub return_types: Vec<String>,
    /// Whether this method returns an error
    pub returns_error: bool,
    /// Location
    pub location: AstLocation,
    /// Start byte offset for patching
    pub start_byte: usize,
    /// End byte offset for patching
    pub end_byte: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoParam {
    pub name: String,
    pub param_type: String,
}

/// Representation of a Go type declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoTypeDecl {
    pub name: String,
    pub kind: GoTypeKind,
    /// Fields for struct types
    pub fields: Vec<GoField>,
    /// Methods for interface types
    pub interface_methods: Vec<String>,
    /// Location
    pub location: AstLocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GoTypeKind {
    Struct,
    Interface,
    Alias,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoField {
    pub name: String,
    pub field_type: String,
    pub tag: Option<String>,
}

/// Representation of a variable or constant declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoDeclaration {
    pub name: String,
    pub is_const: bool,
    pub value_repr: Option<String>,
    pub decl_type: Option<String>,
    /// Location
    pub location: AstLocation,
}

/// Type alias for backward compatibility.
pub type CallSite = GoCallSite;

/// Representation of a function/method call in the file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoCallSite {
    /// Resolution information for this call site.
    pub function_call: FunctionCall,
    /// Arguments (simple text representation)
    pub args_repr: String,
    /// Whether this call is inside a loop
    pub in_loop: bool,
    /// Whether this call's error return is checked
    pub error_checked: bool,
    /// Start byte offset
    pub start_byte: usize,
    /// End byte offset
    pub end_byte: usize,
}

impl GoFileSemantics {
    /// Build the semantic model from a parsed Go file.
    pub fn from_parsed(parsed: &ParsedFile) -> Self {
        let mut sem = GoFileSemantics {
            file_id: parsed.file_id,
            path: parsed.path.clone(),
            language: parsed.language,
            package_name: String::new(),
            imports: Vec::new(),
            functions: Vec::new(),
            types: Vec::new(),
            methods: Vec::new(),
            declarations: Vec::new(),
            calls: Vec::new(),
            http_calls: Vec::new(),
            unchecked_errors: Vec::new(),
            goroutines: Vec::new(),
            channel_ops: Vec::new(),
            defers: Vec::new(),
            context_usages: Vec::new(),
            mutex_operations: Vec::new(),
            go_framework: None,
            db_operations: Vec::new(),
        };

        if parsed.language == Language::Go {
            collect_semantics(parsed, &mut sem);
        }

        sem
    }

    /// Run framework-specific analysis (Gin, Echo, net/http, etc.).
    pub fn analyze_frameworks(&mut self, parsed: &ParsedFile) -> anyhow::Result<()> {
        self.http_calls = super::http::summarize_http_clients(parsed);
        
        // Extract Go HTTP framework routes (Gin, Echo, Fiber, Chi)
        let framework_summary = extract_go_routes(parsed);
        if framework_summary.has_framework() {
            self.go_framework = Some(framework_summary);
        }
        
        collect_error_handling(parsed, self);
        collect_concurrency(parsed, self);
        collect_context_usage(parsed, self);
        Ok(())
    }
}

/// Context for tracking state during AST traversal.
#[derive(Default, Clone)]
struct TraversalContext {
    in_loop: bool,
    in_select: bool,
    current_function: Option<String>,
    current_qualified_name: Option<String>,
    /// Whether currently in an HTTP/RPC handler
    _in_handler: bool,
    /// Type of handler if in_handler is true
    _handler_type: Option<String>,
}

/// Collect semantics by walking the tree-sitter AST.
fn collect_semantics(parsed: &ParsedFile, sem: &mut GoFileSemantics) {
    let root = parsed.tree.root_node();
    let ctx = TraversalContext::default();
    walk_nodes_with_context(root, parsed, sem, ctx);
}

/// Walk nodes while tracking context.
fn walk_nodes_with_context(
    node: tree_sitter::Node,
    parsed: &ParsedFile,
    sem: &mut GoFileSemantics,
    ctx: TraversalContext,
) {
    // Update context based on current node
    let new_ctx = match node.kind() {
        "for_statement" | "range_clause" => TraversalContext {
            in_loop: true,
            ..ctx.clone()
        },
        "select_statement" => TraversalContext {
            in_select: true,
            ..ctx.clone()
        },
        "function_declaration" => {
            let func_name = node
                .child_by_field_name("name")
                .map(|n| parsed.text_for_node(&n));
            TraversalContext {
                current_function: func_name.clone(),
                current_qualified_name: func_name,
                ..ctx.clone()
            }
        }
        "method_declaration" => {
            let func_name = node
                .child_by_field_name("name")
                .map(|n| parsed.text_for_node(&n));
            let receiver = node.child_by_field_name("receiver");
            let receiver_type = receiver.map(|r| extract_receiver_type(parsed, &r).0).unwrap_or_default();
            let qualified = func_name.as_ref().map(|n| format!("{}.{}", receiver_type, n));
            TraversalContext {
                current_function: func_name,
                current_qualified_name: qualified,
                ..ctx.clone()
            }
        }
        _ => ctx.clone(),
    };

    // Process current node
    match node.kind() {
        "package_clause" => {
            // In tree-sitter-go, package name is a package_identifier child, not a field
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    if child.kind() == "package_identifier" {
                        sem.package_name = parsed.text_for_node(&child);
                        break;
                    }
                }
            }
        }
        "import_declaration" => {
            collect_imports(parsed, &node, sem);
        }
        "function_declaration" => {
            if let Some(func) = build_function(parsed, &node) {
                sem.functions.push(func);
            }
        }
        "method_declaration" => {
            if let Some(method) = build_method(parsed, &node) {
                sem.methods.push(method);
            }
        }
        "type_declaration" => {
            collect_type_declarations(parsed, &node, sem);
        }
        "var_declaration" | "const_declaration" => {
            collect_declarations(parsed, &node, sem, node.kind() == "const_declaration");
        }
        "call_expression" => {
            if let Some(call) = build_callsite(parsed, &node, &new_ctx) {
                sem.calls.push(call);
            }
            // Check for database operations (database/sql, GORM, sqlx, etc.)
            if let Some(db_op) = detect_db_operation_from_call(parsed, &node, &new_ctx) {
                sem.db_operations.push(db_op);
            }
        }
        "go_statement" => {
            if let Some(goroutine) = build_goroutine(parsed, &node, &new_ctx) {
                sem.goroutines.push(goroutine);
            }
        }
        "defer_statement" => {
            if let Some(defer_stmt) = build_defer(parsed, &node, &new_ctx) {
                sem.defers.push(defer_stmt);
            }
        }
        "send_statement" => {
            if let Some(ch_op) = build_channel_send(parsed, &node, &new_ctx) {
                sem.channel_ops.push(ch_op);
            }
        }
        "receive_statement" => {
            if let Some(ch_op) = build_channel_receive(parsed, &node, &new_ctx) {
                sem.channel_ops.push(ch_op);
            }
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

/// Collect imports from an import declaration.
fn collect_imports(parsed: &ParsedFile, node: &tree_sitter::Node, sem: &mut GoFileSemantics) {
    fn process_import_spec(
        parsed: &ParsedFile,
        spec: tree_sitter::Node,
        sem: &mut GoFileSemantics,
    ) {
        let mut path = String::new();
        let mut alias = None;
        let mut is_blank = false;
        let mut is_dot = false;

        for i in 0..spec.child_count() {
            if let Some(child) = spec.child(i) {
                match child.kind() {
                    "interpreted_string_literal" | "raw_string_literal" => {
                        path = parsed
                            .text_for_node(&child)
                            .trim_matches('"')
                            .trim_matches('`')
                            .to_string();
                    }
                    "package_identifier" | "identifier" => {
                        let name = parsed.text_for_node(&child);
                        if name == "_" {
                            is_blank = true;
                        } else if name == "." {
                            is_dot = true;
                        } else {
                            alias = Some(name);
                        }
                    }
                    "blank_identifier" => {
                        is_blank = true;
                    }
                    "dot" => {
                        is_dot = true;
                    }
                    _ => {}
                }
            }
        }

        if !path.is_empty() {
            sem.imports.push(GoImport {
                path,
                alias,
                is_blank,
                is_dot,
                location: parsed.location_for_node(&spec),
            });
        }
    }

    // Handle both single import and grouped imports
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            match child.kind() {
                "import_spec" => {
                    process_import_spec(parsed, child, sem);
                }
                "import_spec_list" => {
                    for j in 0..child.child_count() {
                        if let Some(spec) = child.child(j) {
                            if spec.kind() == "import_spec" {
                                process_import_spec(parsed, spec, sem);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

/// Build a GoFunction from a function_declaration node.
fn build_function(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<GoFunction> {
    let name_node = node.child_by_field_name("name")?;
    let name = parsed.text_for_node(&name_node);

    let (params, return_types) = extract_signature(parsed, node);
    let returns_error = return_types.last().is_some_and(|t| t == "error");

    Some(GoFunction {
        name,
        params,
        return_types,
        returns_error,
        location: parsed.location_for_node(node),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

/// Build a GoMethod from a method_declaration node.
fn build_method(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<GoMethod> {
    let name_node = node.child_by_field_name("name")?;
    let name = parsed.text_for_node(&name_node);

    // Extract receiver
    let receiver = node.child_by_field_name("receiver")?;
    let (receiver_type, receiver_is_pointer) = extract_receiver_type(parsed, &receiver);

    let (params, return_types) = extract_signature(parsed, node);
    let returns_error = return_types.last().is_some_and(|t| t == "error");

    Some(GoMethod {
        name,
        receiver_type,
        receiver_is_pointer,
        params,
        return_types,
        returns_error,
        location: parsed.location_for_node(node),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

/// Extract receiver type from a parameter list node.
fn extract_receiver_type(parsed: &ParsedFile, receiver: &tree_sitter::Node) -> (String, bool) {
    let text = parsed.text_for_node(receiver);
    // Remove parentheses and extract type
    let trimmed = text.trim_matches(|c| c == '(' || c == ')' || c == ' ');

    // Check for pointer receiver
    if let Some(ptr_pos) = trimmed.find('*') {
        let type_name = trimmed[ptr_pos + 1..].trim().to_string();
        // Remove any variable name prefix
        let type_name = type_name
            .split_whitespace()
            .last()
            .unwrap_or(&type_name)
            .to_string();
        (type_name, true)
    } else {
        // Non-pointer receiver - extract type name after variable name if present
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        let type_name = parts.last().unwrap_or(&"").to_string();
        (type_name, false)
    }
}

/// Extract parameters and return types from a function/method declaration.
fn extract_signature(parsed: &ParsedFile, node: &tree_sitter::Node) -> (Vec<GoParam>, Vec<String>) {
    let mut params = Vec::new();
    let mut return_types = Vec::new();

    // Extract parameters
    if let Some(params_node) = node.child_by_field_name("parameters") {
        params = extract_params(parsed, &params_node);
    }

    // Extract return types
    if let Some(result_node) = node.child_by_field_name("result") {
        return_types = extract_return_types(parsed, &result_node);
    }

    (params, return_types)
}

/// Extract parameters from a parameter_list node.
fn extract_params(parsed: &ParsedFile, params_node: &tree_sitter::Node) -> Vec<GoParam> {
    let mut params = Vec::new();

    for i in 0..params_node.child_count() {
        if let Some(child) = params_node.child(i) {
            if child.kind() == "parameter_declaration" {
                // Get the type (last type_identifier or qualified_type in the declaration)
                let mut param_type = String::new();
                let mut names = Vec::new();

                for j in 0..child.child_count() {
                    if let Some(param_child) = child.child(j) {
                        match param_child.kind() {
                            "identifier" => {
                                names.push(parsed.text_for_node(&param_child));
                            }
                            "type_identifier" | "qualified_type" | "pointer_type"
                            | "slice_type" | "array_type" | "map_type" | "channel_type"
                            | "function_type" | "interface_type" | "struct_type" => {
                                param_type = parsed.text_for_node(&param_child);
                            }
                            _ => {}
                        }
                    }
                }

                // If no names, use empty string
                if names.is_empty() {
                    names.push(String::new());
                }

                for name in names {
                    params.push(GoParam {
                        name,
                        param_type: param_type.clone(),
                    });
                }
            }
        }
    }

    params
}

/// Extract return types from a result node.
fn extract_return_types(parsed: &ParsedFile, result_node: &tree_sitter::Node) -> Vec<String> {
    let mut types = Vec::new();

    match result_node.kind() {
        "parameter_list" => {
            // Multiple return values: (int, error)
            for i in 0..result_node.child_count() {
                if let Some(child) = result_node.child(i) {
                    if child.kind() == "parameter_declaration" {
                        // Extract type from each parameter declaration
                        for j in 0..child.child_count() {
                            if let Some(type_node) = child.child(j) {
                                if matches!(
                                    type_node.kind(),
                                    "type_identifier"
                                        | "qualified_type"
                                        | "pointer_type"
                                        | "slice_type"
                                        | "array_type"
                                        | "map_type"
                                        | "channel_type"
                                        | "function_type"
                                        | "interface_type"
                                        | "struct_type"
                                ) {
                                    types.push(parsed.text_for_node(&type_node));
                                }
                            }
                        }
                    }
                }
            }
        }
        "type_identifier" | "qualified_type" | "pointer_type" | "slice_type" | "array_type"
        | "map_type" | "channel_type" | "function_type" | "interface_type" | "struct_type" => {
            // Single return value
            types.push(parsed.text_for_node(result_node));
        }
        _ => {}
    }

    types
}

/// Collect type declarations.
fn collect_type_declarations(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    sem: &mut GoFileSemantics,
) {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "type_spec" {
                if let Some(type_decl) = build_type_decl(parsed, &child) {
                    sem.types.push(type_decl);
                }
            }
        }
    }
}

/// Build a GoTypeDecl from a type_spec node.
fn build_type_decl(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<GoTypeDecl> {
    let name_node = node.child_by_field_name("name")?;
    let name = parsed.text_for_node(&name_node);

    let type_node = node.child_by_field_name("type")?;
    let (kind, fields, interface_methods) = match type_node.kind() {
        "struct_type" => {
            let fields = extract_struct_fields(parsed, &type_node);
            (GoTypeKind::Struct, fields, Vec::new())
        }
        "interface_type" => {
            let methods = extract_interface_methods(parsed, &type_node);
            (GoTypeKind::Interface, Vec::new(), methods)
        }
        _ => (GoTypeKind::Alias, Vec::new(), Vec::new()),
    };

    Some(GoTypeDecl {
        name,
        kind,
        fields,
        interface_methods,
        location: parsed.location_for_node(node),
    })
}

/// Extract struct fields.
fn extract_struct_fields(parsed: &ParsedFile, struct_node: &tree_sitter::Node) -> Vec<GoField> {
    let mut fields = Vec::new();

    // In tree-sitter-go, struct_type has child nodes directly, including field_declaration_list
    // or individual field_declaration nodes. We iterate through all children.
    for i in 0..struct_node.child_count() {
        if let Some(child) = struct_node.child(i) {
            // Handle field_declaration_list (for multiple fields) or direct field_declaration
            let field_decls: Vec<tree_sitter::Node> = if child.kind() == "field_declaration_list" {
                (0..child.child_count())
                    .filter_map(|j| child.child(j))
                    .filter(|n| n.kind() == "field_declaration")
                    .collect()
            } else if child.kind() == "field_declaration" {
                vec![child]
            } else {
                continue;
            };

            for field_decl in field_decls {
                let mut names = Vec::new();
                let mut field_type = String::new();
                let mut tag = None;

                for j in 0..field_decl.child_count() {
                    if let Some(field_child) = field_decl.child(j) {
                        match field_child.kind() {
                            "field_identifier" => {
                                names.push(parsed.text_for_node(&field_child));
                            }
                            "type_identifier" | "qualified_type" | "pointer_type"
                            | "slice_type" | "array_type" | "map_type" | "channel_type"
                            | "function_type" | "interface_type" | "struct_type" => {
                                field_type = parsed.text_for_node(&field_child);
                            }
                            "interpreted_string_literal" | "raw_string_literal" => {
                                tag = Some(parsed.text_for_node(&field_child));
                            }
                            _ => {}
                        }
                    }
                }

                // Handle embedded fields (no name, just type)
                if names.is_empty() && !field_type.is_empty() {
                    names.push(field_type.clone());
                }

                for name in names {
                    fields.push(GoField {
                        name,
                        field_type: field_type.clone(),
                        tag: tag.clone(),
                    });
                }
            }
        }
    }

    fields
}

/// Extract interface methods.
fn extract_interface_methods(
    parsed: &ParsedFile,
    interface_node: &tree_sitter::Node,
) -> Vec<String> {
    let mut methods = Vec::new();

    for i in 0..interface_node.child_count() {
        if let Some(child) = interface_node.child(i) {
            if child.kind() == "method_spec" {
                if let Some(name_node) = child.child_by_field_name("name") {
                    methods.push(parsed.text_for_node(&name_node));
                }
            }
        }
    }

    methods
}

/// Collect variable/constant declarations.
fn collect_declarations(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    sem: &mut GoFileSemantics,
    is_const: bool,
) {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "var_spec" || child.kind() == "const_spec" {
                // Extract name(s) and value(s)
                let mut names = Vec::new();
                let mut decl_type = None;
                let mut value = None;

                for j in 0..child.child_count() {
                    if let Some(spec_child) = child.child(j) {
                        match spec_child.kind() {
                            "identifier" => {
                                names.push(parsed.text_for_node(&spec_child));
                            }
                            "type_identifier" | "qualified_type" | "pointer_type"
                            | "slice_type" | "array_type" | "map_type" | "channel_type"
                            | "function_type" | "interface_type" | "struct_type" => {
                                decl_type = Some(parsed.text_for_node(&spec_child));
                            }
                            "expression_list" => {
                                value = Some(parsed.text_for_node(&spec_child));
                            }
                            _ if spec_child.is_named() && value.is_none() => {
                                // Catch other expression types
                                let text = parsed.text_for_node(&spec_child);
                                if !text.is_empty() && spec_child.kind() != "comment" {
                                    value = Some(text);
                                }
                            }
                            _ => {}
                        }
                    }
                }

                for name in names {
                    sem.declarations.push(GoDeclaration {
                        name,
                        is_const,
                        value_repr: value.clone(),
                        decl_type: decl_type.clone(),
                        location: parsed.location_for_node(&child),
                    });
                }
            }
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
        // sqlx patterns - check for sqlx-specific method names (they don't contain "sqlx" in callee)
        s if s.to_lowercase().contains("queryx") || s.to_lowercase().contains("queryrowx") => (DbLibrary::Sqlx, DbOperationType::Select),
        s if s.to_lowercase().contains("mustexec") => (DbLibrary::Sqlx, DbOperationType::Update),

        // database/sql patterns (stdlib) - case insensitive matching
        s if s.to_lowercase().contains("sql.open") => (DbLibrary::DatabaseSql, DbOperationType::Connect),
        s if s.to_lowercase().contains("query") && !s.to_lowercase().contains("queryx") && !s.to_lowercase().contains("queryrowx") => (DbLibrary::DatabaseSql, DbOperationType::Select),
        s if s.to_lowercase().contains("exec") && !s.to_lowercase().contains("mustexec") => (DbLibrary::DatabaseSql, DbOperationType::Update),
        s if s.to_lowercase().contains("begin") => (DbLibrary::DatabaseSql, DbOperationType::TransactionBegin),
        s if s.to_lowercase().contains("commit") && s.to_lowercase().contains("tx") => (DbLibrary::DatabaseSql, DbOperationType::TransactionCommit),
        s if s.to_lowercase().contains("rollback") && s.to_lowercase().contains("tx") => (DbLibrary::DatabaseSql, DbOperationType::TransactionRollback),

        // GORM patterns
        s if s.to_lowercase().contains("gorm") && s.to_lowercase().contains("open") => (DbLibrary::Gorm, DbOperationType::Connect),
        s if s.to_lowercase().contains(".find") => (DbLibrary::Gorm, DbOperationType::Select),
        s if s.to_lowercase().contains(".first") || s.to_lowercase().contains(".last") || s.to_lowercase().contains(".take") => (DbLibrary::Gorm, DbOperationType::Select),
        s if s.to_lowercase().contains(".create") => (DbLibrary::Gorm, DbOperationType::Insert),
        s if s.to_lowercase().contains(".save") => (DbLibrary::Gorm, DbOperationType::Update),
        s if s.to_lowercase().contains(".update") => (DbLibrary::Gorm, DbOperationType::Update),
        s if s.to_lowercase().contains(".delete") => (DbLibrary::Gorm, DbOperationType::Delete),
        s if s.to_lowercase().contains(".raw") => (DbLibrary::Gorm, DbOperationType::RawSql),

        // sqlc patterns
        s if s.to_lowercase().contains("querier") => (DbLibrary::Sqlc, DbOperationType::Select),

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

/// Build a GoCallSite from a call_expression node.
fn build_callsite(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    ctx: &TraversalContext,
) -> Option<GoCallSite> {
    let func_node = node.child_by_field_name("function")?;
    let callee_expr = parsed.text_for_node(&func_node);

    let args_repr = if let Some(args_node) = node.child_by_field_name("arguments") {
        parsed.text_for_node(&args_node)
    } else {
        String::new()
    };

    // Parse callee into parts (e.g., "pkg.Func" -> ["pkg", "Func"])
    let callee_parts: Vec<String> = callee_expr.split('.').map(String::from).collect();
    
    // Detect if this is an import call (e.g., "http.Get" where "http" is from "net/http")
    // For now, if first part could be an import alias, mark as import call
    let is_import_call = callee_parts.len() > 1 && callee_parts[0].chars().next().is_some_and(|c| c.is_lowercase());
    
    // Get import alias if applicable
    let import_alias = if is_import_call {
        Some(callee_parts[0].clone())
    } else {
        None
    };

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
        is_self_call: false, // Go doesn't have self in the same way
        is_import_call,
        import_alias,
    };

    Some(GoCallSite {
        function_call,
        args_repr,
        in_loop: ctx.in_loop,
        error_checked: false, // Will be determined by error analysis pass
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

/// Build a GoroutineSpawn from a go_statement node.
fn build_goroutine(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    ctx: &TraversalContext,
) -> Option<GoroutineSpawn> {
    let text = parsed.text_for_node(node);
    let range = node.range();

    // Check if goroutine has recover() somewhere inside
    let has_recover = text.contains("recover()");

    // Check if goroutine function receives a context parameter
    let has_context_param = text.contains("ctx") || text.contains("context.Context");

    // Check if goroutine uses a done channel pattern
    let has_done_channel = text.contains("done")
        || text.contains("quit")
        || text.contains("stop")
        || text.contains("<-ctx.Done()");

    // Check for unbounded channel send (send without select with default)
    let has_unbounded_channel_send = text.contains("<-") && !text.contains("select");

    // Check if this is an anonymous goroutine (go func() {...}())
    let is_anonymous = text.contains("go func(");

    Some(GoroutineSpawn {
        line: range.start_point.row as u32 + 1,
        column: range.start_point.column as u32 + 1,
        text,
        has_recover,
        has_context_param,
        has_done_channel,
        has_unbounded_channel_send,
        is_anonymous,
        function_name: ctx.current_function.clone(),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        location: parsed.location_for_node(node),
    })
}

/// Build a DeferStatement from a defer_statement node.
fn build_defer(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    ctx: &TraversalContext,
) -> Option<DeferStatement> {
    let text = parsed.text_for_node(node);
    let range = node.range();

    // Extract the call expression text (everything after "defer ")
    let call_text = text
        .strip_prefix("defer ")
        .unwrap_or(&text)
        .trim()
        .to_string();

    // Check if defer is for resource cleanup
    let is_resource_cleanup = text.contains(".Close()")
        || text.contains(".Unlock()")
        || text.contains(".RUnlock()")
        || text.contains("cancel()")
        || text.contains(".Release()")
        || text.contains(".Done()")
        || text.contains(".Stop()");

    Some(DeferStatement {
        line: range.start_point.row as u32 + 1,
        column: range.start_point.column as u32 + 1,
        text,
        call_text,
        in_loop: ctx.in_loop,
        is_resource_cleanup,
        function_name: ctx.current_function.clone(),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        location: parsed.location_for_node(node),
    })
}

/// Build a ChannelOp for send operations.
fn build_channel_send(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    ctx: &TraversalContext,
) -> Option<ChannelOp> {
    let text = parsed.text_for_node(node);
    let range = node.range();

    Some(ChannelOp {
        kind: ChannelOpKind::Send,
        line: range.start_point.row as u32 + 1,
        column: range.start_point.column as u32 + 1,
        text,
        in_select: ctx.in_select,
        function_name: ctx.current_function.clone(),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        location: parsed.location_for_node(node),
    })
}

/// Build a ChannelOp for receive operations.
fn build_channel_receive(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    ctx: &TraversalContext,
) -> Option<ChannelOp> {
    let text = parsed.text_for_node(node);
    let range = node.range();

    Some(ChannelOp {
        kind: ChannelOpKind::Receive,
        line: range.start_point.row as u32 + 1,
        column: range.start_point.column as u32 + 1,
        text,
        in_select: ctx.in_select,
        function_name: ctx.current_function.clone(),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        location: parsed.location_for_node(node),
    })
}

/// Collect error handling information.
fn collect_error_handling(parsed: &ParsedFile, sem: &mut GoFileSemantics) {
    // Look for calls that return error but don't check it
    let root = parsed.tree.root_node();
    collect_unchecked_errors(parsed, root, sem, None);
}

/// Recursively collect unchecked errors.
fn collect_unchecked_errors(
    parsed: &ParsedFile,
    node: tree_sitter::Node,
    sem: &mut GoFileSemantics,
    current_fn: Option<String>,
) {
    let current_fn = if matches!(node.kind(), "function_declaration" | "method_declaration") {
        node.child_by_field_name("name")
            .map(|n| parsed.text_for_node(&n))
    } else {
        current_fn
    };

    // Check for expression statements that are just call expressions
    // These are potential unchecked error returns
    if node.kind() == "expression_statement" {
        if let Some(child) = node.child(0) {
            if child.kind() == "call_expression" {
                let call_text = parsed.text_for_node(&child);
                // Heuristic: if the call looks like it returns an error (common patterns)
                if is_likely_error_returning_call(&call_text) {
                    let range = child.range();
                    sem.unchecked_errors.push(UncheckedError {
                        line: range.start_point.row as u32 + 1,
                        column: range.start_point.column as u32 + 1,
                        call_text,
                        function_name: current_fn.clone(),
                        start_byte: child.start_byte(),
                        end_byte: child.end_byte(),
                        location: parsed.location_for_node(&child),
                    });
                }
            }
        }
    }

    // Recurse
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_unchecked_errors(parsed, child, sem, current_fn.clone());
        }
    }
}

/// Heuristic to detect calls that likely return an error.
fn is_likely_error_returning_call(call_text: &str) -> bool {
    // Common patterns that return errors
    call_text.contains(".Write(")
        || call_text.contains(".Read(")
        || call_text.contains(".Close(")
        || call_text.contains(".Open(")
        || call_text.contains("os.")
        || call_text.contains("io.")
        || call_text.contains("ioutil.")
        || call_text.contains("json.")
        || call_text.contains("http.")
        || call_text.contains("sql.")
        || call_text.contains("db.")
        || call_text.contains(".Scan(")
        || call_text.contains(".Exec(")
        || call_text.contains(".Query(")
}

/// Collect concurrency-related information.
fn collect_concurrency(_parsed: &ParsedFile, _sem: &mut GoFileSemantics) {
    // Additional concurrency analysis can be added here
    // For now, goroutines and channel ops are collected in the main pass
}

/// Collect context usage information.
fn collect_context_usage(parsed: &ParsedFile, sem: &mut GoFileSemantics) {
    let root = parsed.tree.root_node();
    walk_for_context_usage(parsed, root, sem, ContextAnalysisState::default());
}

/// Context for tracking handler state during context usage analysis.
#[derive(Default, Clone)]
struct ContextAnalysisState {
    current_fn: Option<String>,
    in_handler: bool,
    handler_type: Option<String>,
}

/// Walk AST to find context usage patterns.
fn walk_for_context_usage(
    parsed: &ParsedFile,
    node: tree_sitter::Node,
    sem: &mut GoFileSemantics,
    state: ContextAnalysisState,
) {
    let state = if matches!(node.kind(), "function_declaration" | "method_declaration") {
        let fn_name = node
            .child_by_field_name("name")
            .map(|n| parsed.text_for_node(&n));

        // Check if this is an HTTP handler based on signature
        let (in_handler, handler_type) = detect_handler_type(parsed, &node);

        ContextAnalysisState {
            current_fn: fn_name,
            in_handler,
            handler_type,
        }
    } else {
        state
    };

    if node.kind() == "call_expression" {
        let call_text = parsed.text_for_node(&node);

        // Check for context patterns
        if call_text.contains("context.") {
            let is_background = call_text.contains("context.Background()");
            let is_todo = call_text.contains("context.TODO()");
            let is_background_or_todo = is_background || is_todo;
            let has_timeout = call_text.contains("WithTimeout")
                || call_text.contains("WithDeadline")
                || call_text.contains("WithCancel");

            // Determine context type
            let context_type = if is_background {
                "Background"
            } else if is_todo {
                "TODO"
            } else if call_text.contains("WithTimeout") {
                "WithTimeout"
            } else if call_text.contains("WithDeadline") {
                "WithDeadline"
            } else if call_text.contains("WithCancel") {
                "WithCancel"
            } else if call_text.contains("WithValue") {
                "WithValue"
            } else {
                "Unknown"
            };

            let range = node.range();
            sem.context_usages.push(ContextUsage {
                line: range.start_point.row as u32 + 1,
                column: range.start_point.column as u32 + 1,
                text: call_text,
                context_type: context_type.to_string(),
                is_background_or_todo,
                has_timeout,
                in_handler: state.in_handler,
                handler_type: state.handler_type.clone(),
                function_name: state.current_fn.clone().unwrap_or_default(),
                start_byte: node.start_byte(),
                end_byte: node.end_byte(),
                location: parsed.location_for_node(&node),
            });
        }
    }

    // Recurse
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_for_context_usage(parsed, child, sem, state.clone());
        }
    }
}

/// Detect if a function/method declaration is an HTTP/RPC handler.
fn detect_handler_type(parsed: &ParsedFile, node: &tree_sitter::Node) -> (bool, Option<String>) {
    let fn_text = parsed.text_for_node(node);

    // Check for net/http handler: func(w http.ResponseWriter, r *http.Request)
    if fn_text.contains("http.ResponseWriter") && fn_text.contains("http.Request") {
        return (true, Some("http".to_string()));
    }

    // Check for Gin handler: func(c *gin.Context)
    if fn_text.contains("*gin.Context") {
        return (true, Some("gin".to_string()));
    }

    // Check for Echo handler: func(c echo.Context)
    if fn_text.contains("echo.Context") {
        return (true, Some("echo".to_string()));
    }

    // Check for Fiber handler: func(c *fiber.Ctx)
    if fn_text.contains("*fiber.Ctx") {
        return (true, Some("fiber".to_string()));
    }

    // Check for gRPC handler: (ctx context.Context, req *pb.Something)
    if fn_text.contains("context.Context") && fn_text.contains("*pb.") {
        return (true, Some("grpc".to_string()));
    }

    (false, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::go::parse_go_file;
    use crate::types::context::{Language, SourceFile};

    /// Helper to parse Go source and build semantics.
    fn parse_and_build_semantics(source: &str) -> GoFileSemantics {
        let sf = SourceFile {
            path: "test.go".to_string(),
            language: Language::Go,
            content: source.to_string(),
        };
        let parsed = parse_go_file(FileId(1), &sf).expect("parsing should succeed");
        GoFileSemantics::from_parsed(&parsed)
    }

    #[test]
    fn collects_package_name() {
        let sem = parse_and_build_semantics("package main");
        assert_eq!(sem.package_name, "main");
    }

    #[test]
    fn collects_simple_import() {
        let src = r#"
package main

import "fmt"
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.imports.len(), 1);
        assert_eq!(sem.imports[0].path, "fmt");
    }

    #[test]
    fn collects_grouped_imports() {
        let src = r#"
package main

import (
    "fmt"
    "net/http"
)
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.imports.len(), 2);
    }

    #[test]
    fn collects_aliased_import() {
        let src = r#"
package main

import mux "github.com/gorilla/mux"
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.imports.len(), 1);
        assert_eq!(sem.imports[0].alias, Some("mux".to_string()));
    }

    #[test]
    fn collects_function() {
        let src = r#"
package main

func hello(name string) string {
    return "Hello, " + name
}
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.functions.len(), 1);
        assert_eq!(sem.functions[0].name, "hello");
        assert_eq!(sem.functions[0].params.len(), 1);
    }

    #[test]
    fn collects_function_returning_error() {
        let src = r#"
package main

func readFile(path string) ([]byte, error) {
    return nil, nil
}
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.functions.len(), 1);
        assert!(sem.functions[0].returns_error);
    }

    #[test]
    fn collects_method() {
        let src = r#"
package main

type Server struct {}

func (s *Server) Start() error {
    return nil
}
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.methods.len(), 1);
        assert_eq!(sem.methods[0].name, "Start");
        assert!(sem.methods[0].receiver_is_pointer);
        assert!(sem.methods[0].returns_error);
    }

    #[test]
    fn collects_struct_type() {
        let src = r#"
package main

type User struct {
    ID   int
    Name string
}
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.types.len(), 1);
        assert_eq!(sem.types[0].name, "User");
        assert!(matches!(sem.types[0].kind, GoTypeKind::Struct));
    }

    #[test]
    fn collects_goroutine() {
        let src = r#"
package main

func main() {
    go func() {
        fmt.Println("goroutine")
    }()
}
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.goroutines.len(), 1);
    }

    #[test]
    fn collects_defer() {
        let src = r#"
package main

func main() {
    defer cleanup()
}
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.defers.len(), 1);
    }

    #[test]
    fn collects_const_declaration() {
        let src = r#"
package main

const MaxSize = 100
"#;
        let sem = parse_and_build_semantics(src);
        assert!(
            sem.declarations
                .iter()
                .any(|d| d.name == "MaxSize" && d.is_const)
        );
    }

    #[test]
    fn collects_var_declaration() {
        let src = r#"
package main

var count int = 0
"#;
        let sem = parse_and_build_semantics(src);
        assert!(
            sem.declarations
                .iter()
                .any(|d| d.name == "count" && !d.is_const)
        );
    }

    #[test]
    fn detects_database_sql_query() {
        let src = r#"
package main

import "database/sql"

func getUser(db *sql.DB, id int) error {
    rows, err := db.Query("SELECT * FROM users WHERE id = ?", id)
    if err != nil {
        return err
    }
    defer rows.Close()
    return nil
}
"#;
        let sem = parse_and_build_semantics(src);
        // Check if any db_operations are detected
        println!("Go DB operations found: {:?}", sem.db_operations);
        assert!(!sem.db_operations.is_empty(), "Expected DB operations to be detected");
        assert_eq!(sem.db_operations[0].library.as_str(), "database/sql");
        assert_eq!(sem.db_operations[0].operation_type.as_str(), "SELECT");
    }

    #[test]
    fn detects_database_sql_exec() {
        let src = r#"
package main

import "database/sql"

func createUser(db *sql.DB, name string) error {
    _, err := db.Exec("INSERT INTO users (name) VALUES (?)", name)
    return err
}
"#;
        let sem = parse_and_build_semantics(src);
        assert!(!sem.db_operations.is_empty());
        assert_eq!(sem.db_operations[0].library.as_str(), "database/sql");
        assert_eq!(sem.db_operations[0].operation_type.as_str(), "UPDATE");
    }

    #[test]
    fn detects_gorm_operations() {
        let src = r#"
package main

import "github.com/jinzhu/gorm"

type User struct {
    ID   uint
    Name string
}

func findUsers(db *gorm.DB) []User {
    var users []User
    db.Find(&users)
    return users
}

func createUser(db *gorm.DB, name string) error {
    return db.Create(&User{Name: name}).Error
}

func deleteUser(db *gorm.DB, id uint) error {
    return db.Delete(&User{ID: id}).Error
}
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.db_operations.len(), 3);
        assert_eq!(sem.db_operations[0].library.as_str(), "GORM");
        assert_eq!(sem.db_operations[0].operation_type.as_str(), "SELECT");
        assert_eq!(sem.db_operations[1].library.as_str(), "GORM");
        assert_eq!(sem.db_operations[1].operation_type.as_str(), "INSERT");
        assert_eq!(sem.db_operations[2].library.as_str(), "GORM");
        assert_eq!(sem.db_operations[2].operation_type.as_str(), "DELETE");
    }

    #[test]
    fn detects_sqlx_operations() {
        let src = r#"
package main

import "github.com/jmoiron/sqlx"

func queryUsers(db *sqlx.DB) ([]User, error) {
    var users []User
    err := db.Queryx("SELECT * FROM users").Map(&users)
    return users, err
}

func executeQuery(db *sqlx.DB) error {
    _, err := db.MustExec("INSERT INTO users (name) VALUES ('test')")
    return err
}
"#;
        let sem = parse_and_build_semantics(src);
        assert!(!sem.db_operations.is_empty());
        assert_eq!(sem.db_operations[0].library.as_str(), "sqlx");
    }
}
