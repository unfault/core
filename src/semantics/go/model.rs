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

/// Information about a defer-recover pattern in Go code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeferRecover {
    /// 1-based line number
    pub line: u32,
    /// 1-based column number
    pub column: u32,
    /// The text of the defer statement
    pub defer_text: String,
    /// Whether the recover has logging/context
    pub has_logging: bool,
    /// Name of the enclosing function
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

/// Information about a select statement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectStatement {
    /// 1-based line number
    pub line: u32,
    /// 1-based column number
    pub column: u32,
    /// The text of the select statement
    pub text: String,
    /// Number of cases in the select
    pub case_count: u32,
    /// Whether there's a default case
    pub has_default: bool,
    /// Whether there are send operations
    pub has_send_cases: bool,
    /// Whether there are receive operations
    pub has_receive_cases: bool,
    /// Whether this select is used for cancellation (ctx.Done())
    pub is_cancellation_pattern: bool,
    /// Name of the enclosing function
    pub function_name: Option<String>,
    /// Start byte offset
    pub start_byte: usize,
    /// End byte offset
    pub end_byte: usize,
    /// Location information
    pub location: AstLocation,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GoAnnotationType {
    Json,
    Yaml,
    Xml,
    Protobuf,
    Validation,
    Orm,
    Sql,
    Generate,
    BuildConstraint,
    Linkname,
    Embed,
    Linter,
    Other(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoAnnotation {
    pub name: String,
    pub value: String,
    pub annotation_type: GoAnnotationType,
    pub target_field: Option<String>,
    pub target_type: Option<String>,
    pub line: u32,
    pub column: u32,
    pub start_byte: usize,
    pub end_byte: usize,
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

    /// Select statements
    pub select_statements: Vec<SelectStatement>,

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

    /// Defer-recover patterns (error handling in Go)
    pub defer_recovers: Vec<DeferRecover>,

    /// Annotations from struct tags, directives, etc.
    pub annotations: Vec<GoAnnotation>,
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
            select_statements: Vec::new(),
            defers: Vec::new(),
            context_usages: Vec::new(),
            mutex_operations: Vec::new(),
            go_framework: None,
            db_operations: Vec::new(),
            defer_recovers: Vec::new(),
            annotations: Vec::new(),
        };

        if parsed.language == Language::Go {
            collect_semantics(parsed, &mut sem);
            collect_annotations(parsed, &mut sem);
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
        collect_mutex_operations(parsed, self);
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

    match node.kind() {
        "package_clause" => {
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
            if let Some(db_op) = detect_db_operation_from_call(parsed, &node, &new_ctx) {
                sem.db_operations.push(db_op);
            }
            if let Some(ch_op) = detect_channel_close(parsed, &node, &new_ctx) {
                sem.channel_ops.push(ch_op);
            }
        }
        "unary_expression" => {
            if let Some(ch_op) = detect_channel_receive_unary(parsed, &node, &new_ctx) {
                sem.channel_ops.push(ch_op);
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
        "select_statement" => {
            if let Some(select_stmt) = build_select_statement(parsed, &node, &new_ctx) {
                sem.select_statements.push(select_stmt);
            }
        }
        _ => {}
    }

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
    let trimmed = text.trim_matches(|c| c == '(' || c == ')' || c == ' ');

    if let Some(ptr_pos) = trimmed.find('*') {
        let type_name = trimmed[ptr_pos + 1..].trim().to_string();
        let type_name = type_name
            .split_whitespace()
            .last()
            .unwrap_or(&type_name)
            .to_string();
        (type_name, true)
    } else {
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        let type_name = parts.last().unwrap_or(&"").to_string();
        (type_name, false)
    }
}

/// Extract parameters and return types from a function/method declaration.
fn extract_signature(parsed: &ParsedFile, node: &tree_sitter::Node) -> (Vec<GoParam>, Vec<String>) {
    let mut params = Vec::new();
    let mut return_types = Vec::new();

    if let Some(params_node) = node.child_by_field_name("parameters") {
        params = extract_params(parsed, &params_node);
    }

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
            for i in 0..result_node.child_count() {
                if let Some(child) = result_node.child(i) {
                    if child.kind() == "parameter_declaration" {
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

    for i in 0..struct_node.child_count() {
        if let Some(child) = struct_node.child(i) {
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
        s if s.to_lowercase().contains("queryx") || s.to_lowercase().contains("queryrowx") => (DbLibrary::Sqlx, DbOperationType::Select),
        s if s.to_lowercase().contains("mustexec") => (DbLibrary::Sqlx, DbOperationType::Update),
        s if s.to_lowercase().contains("sql.open") => (DbLibrary::DatabaseSql, DbOperationType::Connect),
        s if s.to_lowercase().contains("query") && !s.to_lowercase().contains("queryx") && !s.to_lowercase().contains("queryrowx") => (DbLibrary::DatabaseSql, DbOperationType::Select),
        s if s.to_lowercase().contains("exec") && !s.to_lowercase().contains("mustexec") => (DbLibrary::DatabaseSql, DbOperationType::Update),
        s if s.to_lowercase().contains("begin") => (DbLibrary::DatabaseSql, DbOperationType::TransactionBegin),
        s if s.to_lowercase().contains("commit") && s.to_lowercase().contains("tx") => (DbLibrary::DatabaseSql, DbOperationType::TransactionCommit),
        s if s.to_lowercase().contains("rollback") && s.to_lowercase().contains("tx") => (DbLibrary::DatabaseSql, DbOperationType::TransactionRollback),
        s if s.to_lowercase().contains("gorm") && s.to_lowercase().contains("open") => (DbLibrary::Gorm, DbOperationType::Connect),
        s if s.to_lowercase().contains(".find") => (DbLibrary::Gorm, DbOperationType::Select),
        s if s.to_lowercase().contains(".first") || s.to_lowercase().contains(".last") || s.to_lowercase().contains(".take") => (DbLibrary::Gorm, DbOperationType::Select),
        s if s.to_lowercase().contains(".create") => (DbLibrary::Gorm, DbOperationType::Insert),
        s if s.to_lowercase().contains(".save") => (DbLibrary::Gorm, DbOperationType::Update),
        s if s.to_lowercase().contains(".update") => (DbLibrary::Gorm, DbOperationType::Update),
        s if s.to_lowercase().contains(".delete") => (DbLibrary::Gorm, DbOperationType::Delete),
        s if s.to_lowercase().contains(".raw") => (DbLibrary::Gorm, DbOperationType::RawSql),
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

    let callee_parts: Vec<String> = callee_expr.split('.').map(String::from).collect();
    let is_import_call = callee_parts.len() > 1 && callee_parts[0].chars().next().is_some_and(|c| c.is_lowercase());
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
        is_self_call: false,
        is_import_call,
        import_alias,
    };

    Some(GoCallSite {
        function_call,
        args_repr,
        in_loop: ctx.in_loop,
        error_checked: false,
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

    let has_recover = text.contains("recover()");
    let has_context_param = text.contains("ctx") || text.contains("context.Context");
    let has_done_channel = text.contains("done")
        || text.contains("quit")
        || text.contains("stop")
        || text.contains("<-ctx.Done()");
    let has_unbounded_channel_send = text.contains("<-") && !text.contains("select");
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

    let call_text = text
        .strip_prefix("defer ")
        .unwrap_or(&text)
        .trim()
        .to_string();

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

/// Detect a channel close operation (close(ch)).
fn detect_channel_close(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    ctx: &TraversalContext,
) -> Option<ChannelOp> {
    let func_node = node.child_by_field_name("function")?;
    let callee = parsed.text_for_node(&func_node);
    
    if callee != "close" {
        return None;
    }
    
    let text = parsed.text_for_node(node);
    let range = node.range();

    Some(ChannelOp {
        kind: ChannelOpKind::Close,
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

/// Detect a channel receive from a unary expression (<-ch).
/// Only detects unary expressions that are NOT inside a receive_statement.
fn detect_channel_receive_unary(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    ctx: &TraversalContext,
) -> Option<ChannelOp> {
    // Skip if inside a receive_statement (to avoid double-counting)
    let mut ancestor = node.parent();
    while let Some(parent) = ancestor {
        if parent.kind() == "receive_statement" {
            return None;
        }
        ancestor = parent.parent();
    }

    let operator_node = node.child_by_field_name("operator")?;
    let operator = parsed.text_for_node(&operator_node);
    
    if operator != "<-" {
        return None;
    }
    
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

/// Build a SelectStatement from a select_statement node.
fn build_select_statement(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    ctx: &TraversalContext,
) -> Option<SelectStatement> {
    let text = parsed.text_for_node(node);
    let range = node.range();

    let case_count = node
        .children(&mut node.walk())
        .filter(|n| n.kind() == "communication_case" || n.kind() == "default_case")
        .count() as u32;

    let has_default = text.contains("default:");
    let has_send_cases = text.contains("case ") && text.contains(" <- ");
    let has_receive_cases = text.contains("case ") && (text.contains(" <- ") || text.contains(":="));
    let is_cancellation_pattern = text.contains("ctx.Done()") || text.contains("ctx.Done");

    Some(SelectStatement {
        line: range.start_point.row as u32 + 1,
        column: range.start_point.column as u32 + 1,
        text,
        case_count,
        has_default,
        has_send_cases,
        has_receive_cases,
        is_cancellation_pattern,
        function_name: ctx.current_function.clone(),
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        location: parsed.location_for_node(node),
    })
}

/// Collect error handling information.
fn collect_error_handling(parsed: &ParsedFile, sem: &mut GoFileSemantics) {
    let root = parsed.tree.root_node();
    collect_unchecked_errors(parsed, root, sem, None);
    collect_defer_recover(parsed, root, sem, None);
}

/// Collect defer-recover patterns for error handling.
fn collect_defer_recover(
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

    if node.kind() == "defer_statement" {
        let defer_text = parsed.text_for_node(&node);
        let range = node.range();

        if defer_text.contains("recover()") {
            let has_logging = defer_text.contains("log.")
                || defer_text.contains("fmt.")
                || defer_text.contains("zap")
                || defer_text.contains("zerolog")
                || defer_text.contains("slog");

            sem.defer_recovers.push(DeferRecover {
                line: range.start_point.row as u32 + 1,
                column: range.start_point.column as u32 + 1,
                defer_text: defer_text.lines().next().unwrap_or("").to_string(),
                has_logging,
                function_name: current_fn.clone(),
                start_byte: node.start_byte(),
                end_byte: node.end_byte(),
                location: parsed.location_for_node(&node),
            });
        }
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_defer_recover(parsed, child, sem, current_fn.clone());
        }
    }
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

    if node.kind() == "expression_statement" {
        if let Some(child) = node.child(0) {
            if child.kind() == "call_expression" {
                let call_text = parsed.text_for_node(&child);
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

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_unchecked_errors(parsed, child, sem, current_fn.clone());
        }
    }
}

/// Heuristic to detect calls that likely return an error.
fn is_likely_error_returning_call(call_text: &str) -> bool {
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

/// Collect context usage information.
fn collect_context_usage(parsed: &ParsedFile, sem: &mut GoFileSemantics) {
    let root = parsed.tree.root_node();
    walk_for_context_usage(parsed, root, sem, ContextAnalysisState::default());
}

#[derive(Default, Clone)]
struct ContextAnalysisState {
    current_fn: Option<String>,
    in_handler: bool,
    handler_type: Option<String>,
}

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

        if call_text.contains("context.") {
            let is_background = call_text.contains("context.Background()");
            let is_todo = call_text.contains("context.TODO()");
            let is_background_or_todo = is_background || is_todo;
            let has_timeout = call_text.contains("WithTimeout")
                || call_text.contains("WithDeadline")
                || call_text.contains("WithCancel");

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

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_for_context_usage(parsed, child, sem, state.clone());
        }
    }
}

/// Detect if a function/method declaration is an HTTP/RPC handler.
fn detect_handler_type(parsed: &ParsedFile, node: &tree_sitter::Node) -> (bool, Option<String>) {
    let fn_text = parsed.text_for_node(node);

    if fn_text.contains("http.ResponseWriter") && fn_text.contains("http.Request") {
        return (true, Some("http".to_string()));
    }
    if fn_text.contains("*gin.Context") {
        return (true, Some("gin".to_string()));
    }
    if fn_text.contains("echo.Context") {
        return (true, Some("echo".to_string()));
    }
    if fn_text.contains("*fiber.Ctx") {
        return (true, Some("fiber".to_string()));
    }
    if fn_text.contains("context.Context") && fn_text.contains("*pb.") {
        return (true, Some("grpc".to_string()));
    }

    (false, None)
}

/// Collect concurrency-related information.
fn collect_concurrency(_parsed: &ParsedFile, _sem: &mut GoFileSemantics) {
    // Additional concurrency analysis can be added here
    // For now, goroutines and channel ops are collected in the main pass
}

/// Collect mutex operations.
fn collect_mutex_operations(parsed: &ParsedFile, sem: &mut GoFileSemantics) {
    let root = parsed.tree.root_node();
    collect_mutex_ops_recursive(parsed, root, sem, None);
}

fn collect_mutex_ops_recursive(
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

    if node.kind() == "call_expression" {
        let call_text = parsed.text_for_node(&node);
        let range = node.range();

        let (_is_lock, is_rlock) = if call_text.contains(".Lock(") {
            (true, false)
        } else if call_text.contains(".RLock(") {
            (true, true)
        } else {
            // Not a lock operation, continue to children
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    collect_mutex_ops_recursive(parsed, child, sem, current_fn.clone());
                }
            }
            return;
        };

        if let Some(func_node) = node.child_by_field_name("function") {
            let callee = parsed.text_for_node(&func_node);
            if let Some(dot_pos) = callee.rfind('.') {
                let mutex_var = callee[..dot_pos].to_string();
                let operation_type = if is_rlock { "RLock" } else { "Lock" };

                let uses_defer_unlock = check_for_defer_unlock_in_scope(node, parsed);
                let is_empty_critical_section = check_for_empty_critical_section(node, parsed);

                sem.mutex_operations.push(MutexOperation {
                    lock_line: range.start_point.row as u32 + 1,
                    lock_column: range.start_point.column as u32 + 1,
                    text: call_text,
                    mutex_var,
                    operation_type: operation_type.to_string(),
                    is_rlock,
                    uses_defer_unlock,
                    is_empty_critical_section,
                    function_name: current_fn.clone(),
                    lock_start_byte: node.start_byte(),
                    lock_end_byte: node.end_byte(),
                    location: parsed.location_for_node(&node),
                });
            }
        }
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_mutex_ops_recursive(parsed, child, sem, current_fn.clone());
        }
    }
}

fn check_for_defer_unlock_in_scope(node: tree_sitter::Node, parsed: &ParsedFile) -> bool {
    let mut ancestor = node.parent();
    while let Some(parent) = ancestor {
        if parent.kind() == "function_declaration" || parent.kind() == "method_declaration" {
            let text = parsed.text_for_node(&parent);
            return text.contains("defer ") && (text.contains(".Unlock()") || text.contains(".RUnlock()"));
        }
        ancestor = parent.parent();
    }
    false
}

fn check_for_empty_critical_section(node: tree_sitter::Node, parsed: &ParsedFile) -> bool {
    let parent = node.parent();
    if parent.is_none() {
        return false;
    }
    let parent = parent.unwrap();
    if parent.kind() != "expression_statement" {
        return false;
    }

    let grandparent = parent.parent();
    if grandparent.is_none() {
        return false;
    }
    let grandparent = grandparent.unwrap();
    if grandparent.kind() != "block" {
        return false;
    }

    let siblings: Vec<_> = grandparent.children(&mut grandparent.walk()).collect();
    let node_idx = match siblings.iter().position(|n| n.id() == node.id()) {
        Some(idx) => idx,
        None => return false,
    };
    let next_sibling = match siblings.get(node_idx + 1) {
        Some(sibling) => sibling,
        None => return false,
    };

    let next_text = parsed.text_for_node(next_sibling);
    next_text.contains(".Unlock()") || next_text.contains(".RUnlock()")
}

fn collect_annotations(parsed: &ParsedFile, sem: &mut GoFileSemantics) {
    let root = parsed.tree.root_node();
    collect_struct_tag_annotations(parsed, root, sem);
    collect_directive_annotations(parsed, root, sem);
}

fn collect_struct_tag_annotations(parsed: &ParsedFile, node: tree_sitter::Node, sem: &mut GoFileSemantics) {
    if node.kind() == "struct_type" {
        let parent = node.parent();
        if let Some(type_spec) = parent {
            let type_name = type_spec
                .child_by_field_name("name")
                .map(|n| parsed.text_for_node(&n))
                .unwrap_or_default();

            collect_tags_for_struct(parsed, &node, &type_name, sem);
        }
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_struct_tag_annotations(parsed, child, sem);
        }
    }
}

fn collect_tags_for_struct(parsed: &ParsedFile, struct_node: &tree_sitter::Node, type_name: &str, sem: &mut GoFileSemantics) {
    for i in 0..struct_node.child_count() {
        if let Some(child) = struct_node.child(i) {
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
                let mut field_name = String::new();
                let mut tag = None;

                for j in 0..field_decl.child_count() {
                    if let Some(field_child) = field_decl.child(j) {
                        match field_child.kind() {
                            "field_identifier" => {
                                field_name = parsed.text_for_node(&field_child);
                            }
                            "interpreted_string_literal" | "raw_string_literal" => {
                                tag = Some(parsed.text_for_node(&field_child));
                            }
                            _ => {}
                        }
                    }
                }

                if let Some(tag_str) = tag {
                    let range = field_decl.range();
                    let location = AstLocation {
                        file_id: parsed.file_id,
                        range: crate::parse::ast::TextRange {
                            start_line: range.start_point.row as u32,
                            start_col: range.start_point.column as u32,
                            end_line: range.end_point.row as u32,
                            end_col: range.end_point.column as u32,
                        },
                    };
                    parse_and_add_tag(parsed, &tag_str, &field_name, type_name, range, location, sem);
                }
            }
        }
    }
}

fn parse_and_add_tag(_parsed: &ParsedFile, tag: &str, field_name: &str, type_name: &str, range: tree_sitter::Range, location: AstLocation, sem: &mut GoFileSemantics) {
    let tag_content = if (tag.starts_with('`') && tag.ends_with('`')) ||
                         (tag.starts_with('"') && tag.ends_with('"')) {
        &tag[1..tag.len()-1]
    } else {
        tag
    };

    let re = regex::Regex::new(r#"(\w+):"([^"]*)""#).unwrap();
    
    for cap in re.captures_iter(tag_content) {
        let key = cap.get(1).unwrap().as_str();
        let value = cap.get(2).unwrap().as_str();
        
        let annotation_type = determine_annotation_type(key);

        sem.annotations.push(GoAnnotation {
            name: key.to_string(),
            value: value.to_string(),
            annotation_type,
            target_field: if field_name.is_empty() { None } else { Some(field_name.to_string()) },
            target_type: Some(type_name.to_string()),
            line: range.start_point.row as u32 + 1,
            column: range.start_point.column as u32 + 1,
            start_byte: range.start_byte,
            end_byte: range.end_byte,
            location: location.clone(),
        });
    }
}

fn determine_annotation_type(key: &str) -> GoAnnotationType {
    match key.to_lowercase().as_str() {
        "json" => GoAnnotationType::Json,
        "yaml" => GoAnnotationType::Yaml,
        "xml" => GoAnnotationType::Xml,
        "protobuf" | "proto" => GoAnnotationType::Protobuf,
        "validate" => GoAnnotationType::Validation,
        "gorm" => GoAnnotationType::Orm,
        "sql" => GoAnnotationType::Sql,
        _ => GoAnnotationType::Other(key.to_string()),
    }
}

fn collect_directive_annotations(parsed: &ParsedFile, node: tree_sitter::Node, sem: &mut GoFileSemantics) {
    if node.kind() == "comment" {
        let text = parsed.text_for_node(&node);
        let range = node.range();

        if let Some((name, value)) = parse_go_directive(&text) {
            let annotation_type = match name.as_str() {
                "go:generate" => GoAnnotationType::Generate,
                "go:linkname" => GoAnnotationType::Linkname,
                "go:embed" => GoAnnotationType::Embed,
                "+build" | "build" => GoAnnotationType::BuildConstraint,
                "nolint" | "lint-ignore" | "exported" => GoAnnotationType::Linter,
                _ => GoAnnotationType::Other(name.clone()),
            };

            sem.annotations.push(GoAnnotation {
                name,
                value,
                annotation_type,
                target_field: None,
                target_type: None,
                line: range.start_point.row as u32 + 1,
                column: range.start_point.column as u32 + 1,
                start_byte: range.start_byte,
                end_byte: range.end_byte,
                location: parsed.location_for_node(&node),
            });
        }
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_directive_annotations(parsed, child, sem);
        }
    }
}

fn parse_go_directive(comment: &str) -> Option<(String, String)> {
    let comment = comment.trim();

    if !comment.starts_with("//") && !comment.starts_with("/*") {
        return None;
    }

    let content = if comment.starts_with("//") {
        &comment[2..]
    } else if comment.starts_with("/*") {
        &comment[2..comment.len().saturating_sub(2)]
    } else {
        return None;
    };

    let content = content.trim();

    if content.starts_with("go:") {
        let parts: Vec<&str> = content.splitn(2, ' ').collect();
        if parts.len() >= 2 {
            return Some((parts[0].to_string(), parts[1].trim().to_string()));
        }
        return Some((content.to_string(), String::new()));
    }

    if content.starts_with("+build") || content.starts_with("build ") {
        let content = content.trim_start_matches("build ");
        let parts: Vec<&str> = content.split_whitespace().collect();
        return Some(("+build".to_string(), parts.join(" ")));
    }

    if content.starts_with("nolint") || content.starts_with("lint-ignore") || content.starts_with("exported") {
        let parts: Vec<&str> = content.splitn(2, ' ').collect();
        if parts.len() >= 2 {
            return Some((parts[0].to_string(), parts[1].trim().to_string()));
        }
        let colon_parts: Vec<&str> = content.splitn(2, ':').collect();
        if colon_parts.len() >= 2 {
            return Some((colon_parts[0].to_string(), colon_parts[1].trim().to_string()));
        }
        return Some((content.to_string(), String::new()));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::go::parse_go_file;
    use crate::types::context::{Language, SourceFile};

    fn parse_and_build_semantics(source: &str) -> GoFileSemantics {
        let sf = SourceFile {
            path: "test.go".to_string(),
            language: Language::Go,
            content: source.to_string(),
        };
        let parsed = parse_go_file(FileId(1), &sf).expect("parsing should succeed");
        let mut sem = GoFileSemantics::from_parsed(&parsed);
        sem.analyze_frameworks(&parsed).expect("analysis should succeed");
        sem
    }

    #[test]
    fn collects_package_name() {
        let sem = parse_and_build_semantics("package main");
        assert_eq!(sem.package_name, "main");
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
    fn collects_channel_operations() {
        let src = r#"
package main

func main() {
    ch := make(chan int)
    go func() { ch <- 42 }()
    v := <-ch
    _ = v
    close(ch)
}
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.channel_ops.len(), 3);
        assert!(sem.channel_ops.iter().any(|op| matches!(op.kind, ChannelOpKind::Send)));
        assert!(sem.channel_ops.iter().any(|op| matches!(op.kind, ChannelOpKind::Receive)));
        assert!(sem.channel_ops.iter().any(|op| matches!(op.kind, ChannelOpKind::Close)));
    }

    #[test]
    fn collects_select_statement() {
        let src = r#"
package main

func main() {
    select {
    case <-done:
        fmt.Println("done")
    default:
        fmt.Println("default")
    }
}
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.select_statements.len(), 1);
        let select = &sem.select_statements[0];
        assert_eq!(select.case_count, 2);
        assert!(select.has_default);
    }

    #[test]
    fn channel_ops_track_in_select() {
        let src = r#"
package main

func main() {
    ch := make(chan int)
    select {
    case ch <- 1:
        fmt.Println("sent")
    case v := <-ch:
        fmt.Println(v)
    }
}
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.channel_ops.len(), 2);
        assert!(sem.channel_ops.iter().all(|op| op.in_select));
    }

    #[test]
    fn detects_mutex_operations() {
        let src = r#"
package main

func main() {
    mu.Lock()
    mu.Unlock()
}
"#;
        let sem = parse_and_build_semantics(src);
        assert!(!sem.mutex_operations.is_empty());
    }

    #[test]
    fn detects_rlock_operations() {
        let src = r#"
package main

func main() {
    mu.RLock()
    mu.RUnlock()
}
"#;
        let sem = parse_and_build_semantics(src);
        assert!(!sem.mutex_operations.is_empty());
        let rlock_op = sem.mutex_operations.iter().find(|op| op.is_rlock);
        assert!(rlock_op.is_some());
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
        assert!(!sem.db_operations.is_empty());
        assert_eq!(sem.db_operations[0].library.as_str(), "database/sql");
        assert_eq!(sem.db_operations[0].operation_type.as_str(), "SELECT");
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
"#;
        let sem = parse_and_build_semantics(src);
        assert!(!sem.db_operations.is_empty());
        assert_eq!(sem.db_operations[0].library.as_str(), "GORM");
        assert_eq!(sem.db_operations[0].operation_type.as_str(), "SELECT");
    }

    #[test]
    fn collects_struct_tag_annotations() {
        let src = r#"
package main

type User struct {
    ID   int    `json:"id" validate:"required"`
    Name string `yaml:"name" gorm:"primaryKey"`
    Email string `xml:"email"`
}
"#;
        let sem = parse_and_build_semantics(src);
        assert!(!sem.annotations.is_empty());
        assert!(sem.annotations.iter().any(|a| a.name == "json"));
        assert!(sem.annotations.iter().any(|a| a.name == "validate"));
        assert!(sem.annotations.iter().any(|a| a.name == "yaml"));
        assert!(sem.annotations.iter().any(|a| a.name == "gorm"));
        assert!(sem.annotations.iter().any(|a| a.name == "xml"));
    }

    #[test]
    fn collects_go_directive_annotations() {
        let src = r#"
package main

//go:generate mockgen -destination=mocks/mock.go github.com/example MyInterface
//+build linux amd64
//nolint:unused

func main() {}
"#;
        let sem = parse_and_build_semantics(src);
        assert!(!sem.annotations.is_empty());
        assert!(sem.annotations.iter().any(|a| a.name == "go:generate"));
        assert!(sem.annotations.iter().any(|a| a.name == "+build"));
        assert!(sem.annotations.iter().any(|a| a.name == "nolint"));
    }
}
