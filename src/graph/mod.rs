//! CodeGraph built from per-file semantics.
//!
//! This module provides a comprehensive code graph that captures:
//! - File nodes with path information
//! - Function/method nodes
//! - Class/type nodes
//! - External module/library nodes
//! - Framework-specific nodes (FastAPI apps, routes, middlewares)
//!
//! Edges capture relationships:
//! - Contains: File contains functions/classes
//! - Imports: File imports another file
//! - ImportsFrom: File imports specific items from module
//! - Calls: Function calls another function
//! - UsesLibrary: File/function uses external library
//! - Framework-specific edges (FastAPI routes, middlewares)

use std::collections::HashMap;
use std::sync::Arc;

use petgraph::Direction;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize};

use crate::parse::ast::FileId;
use crate::semantics::{Import, SourceSemantics};
use crate::semantics::common::CommonSemantics;
use crate::semantics::go::frameworks::GoFrameworkSummary;
use crate::semantics::go::model::GoFileSemantics;
use crate::semantics::python::fastapi::FastApiFileSummary;
use crate::semantics::python::model::PyFileSemantics;
use crate::semantics::rust::frameworks::RustFrameworkSummary;
use crate::semantics::rust::model::RustFileSemantics;
use crate::semantics::typescript::model::{ExpressFileSummary, TsFileSemantics};
use crate::types::context::Language;

/// Category of external modules for better organization
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModuleCategory {
    /// HTTP client libraries (requests, httpx, axios, etc.)
    HttpClient,
    /// Database/ORM libraries (sqlalchemy, prisma, etc.)
    Database,
    /// Web frameworks (fastapi, express, gin, etc.)
    WebFramework,
    /// Async runtimes (asyncio, tokio, etc.)
    AsyncRuntime,
    /// Logging libraries
    Logging,
    /// Retry/resilience libraries
    Resilience,
    /// Standard library
    StandardLib,
    /// Other external library
    Other,
}

impl Default for ModuleCategory {
    fn default() -> Self {
        Self::Other
    }
}

/// Nodes in the code graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GraphNode {
    /// A source file
    File {
        file_id: FileId,
        path: String,
        language: Language,
    },

    /// A function or method definition
    Function {
        file_id: FileId,
        name: String,
        /// Qualified name including class (e.g., "MyClass.my_method")
        qualified_name: String,
        is_async: bool,
        /// Whether this is an HTTP handler, event handler, etc.
        is_handler: bool,
        /// HTTP method if this is an HTTP route handler (e.g., "GET", "POST")
        http_method: Option<String>,
        /// HTTP path if this is an HTTP route handler (e.g., "/users/{user_id}")
        http_path: Option<String>,
    },

    /// A class or type definition
    Class {
        file_id: FileId,
        name: String,
    },

    /// An external module/library dependency
    ExternalModule {
        /// Module name (e.g., "requests", "fastapi", "gin")
        name: String,
        /// Category for grouping
        category: ModuleCategory,
    },

    // === FastAPI-specific nodes (for backward compatibility) ===
    FastApiApp {
        file_id: FileId,
        var_name: String,
    },

    FastApiRoute {
        file_id: FileId,
        http_method: String,
        path: String,
    },

    FastApiMiddleware {
        file_id: FileId,
        app_var_name: String,
        middleware_type: String,
    },
}

impl GraphNode {
    /// Get the file_id if this node is associated with a file
    pub fn file_id(&self) -> Option<FileId> {
        match self {
            GraphNode::File { file_id, .. } => Some(*file_id),
            GraphNode::Function { file_id, .. } => Some(*file_id),
            GraphNode::Class { file_id, .. } => Some(*file_id),
            GraphNode::FastApiApp { file_id, .. } => Some(*file_id),
            GraphNode::FastApiRoute { file_id, .. } => Some(*file_id),
            GraphNode::FastApiMiddleware { file_id, .. } => Some(*file_id),
            GraphNode::ExternalModule { .. } => None,
        }
    }

    /// Get the display name for this node
    pub fn display_name(&self) -> String {
        match self {
            GraphNode::File { path, .. } => path.clone(),
            GraphNode::Function { qualified_name, .. } => qualified_name.clone(),
            GraphNode::Class { name, .. } => name.clone(),
            GraphNode::ExternalModule { name, .. } => name.clone(),
            GraphNode::FastApiApp { var_name, .. } => format!("FastAPI({})", var_name),
            GraphNode::FastApiRoute {
                http_method, path, ..
            } => format!("{} {}", http_method, path),
            GraphNode::FastApiMiddleware {
                middleware_type, ..
            } => middleware_type.clone(),
        }
    }

    /// Get the HTTP method for this node if it's an HTTP handler
    pub fn http_method(&self) -> Option<&str> {
        match self {
            GraphNode::Function { http_method, .. } => http_method.as_deref(),
            GraphNode::FastApiRoute { http_method, .. } => Some(http_method),
            _ => None,
        }
    }

    /// Get the HTTP path for this node if it's an HTTP handler
    pub fn http_path(&self) -> Option<&str> {
        match self {
            GraphNode::Function { http_path, .. } => http_path.as_deref(),
            GraphNode::FastApiRoute { path, .. } => Some(path),
            _ => None,
        }
    }

    /// Check if this is a file node
    pub fn is_file(&self) -> bool {
        matches!(self, GraphNode::File { .. })
    }
}

/// Edge kinds between nodes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphEdgeKind {
    /// A file "contains" a construct (function, class, app, route, middleware).
    Contains,

    /// File A imports File B (entire module)
    /// Direction: importing file -> imported file
    Imports,

    /// File A imports specific items from File B
    /// Contains the item names in the edge data
    ImportsFrom {
        /// Items imported (e.g., ["FastAPI", "HTTPException"])
        items: Vec<String>,
    },

    /// Function A calls Function B
    Calls,

    /// Class A inherits from Class B
    Inherits,

    /// File or Function uses an external library
    UsesLibrary,

    // === FastAPI-specific edges (for backward compatibility) ===
    /// A FastAPI app "owns" a route.
    FastApiAppOwnsRoute,

    /// A FastAPI app "has" a middleware attached.
    FastApiAppHasMiddleware,
}

/// The main code graph structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeGraph {
    pub graph: DiGraph<GraphNode, GraphEdgeKind>,
    /// Quick lookup: file_id -> node index for the file node.
    #[serde(skip)]
    pub file_nodes: HashMap<FileId, NodeIndex>,
    /// Quick lookup: file path -> node index for the file node.
    #[serde(skip)]
    pub path_to_file: HashMap<String, NodeIndex>,
    /// Quick lookup: path suffix -> node index (for fast import resolution)
    /// Maps "module.py", "pkg/module.py", etc. to their file nodes
    #[serde(skip)]
    pub suffix_to_file: HashMap<String, NodeIndex>,
    /// Quick lookup: module path (dot-separated) -> node index
    /// Maps "pkg.module" to its file node
    #[serde(skip)]
    pub module_to_file: HashMap<String, NodeIndex>,
    /// Quick lookup: external module name -> node index
    #[serde(skip)]
    pub external_modules: HashMap<String, NodeIndex>,
    /// Quick lookup: (file_id, function_name) -> node index
    #[serde(skip)]
    pub function_nodes: HashMap<(FileId, String), NodeIndex>,
    /// Quick lookup: (file_id, class_name) -> node index
    #[serde(skip)]
    pub class_nodes: HashMap<(FileId, String), NodeIndex>,
}

impl CodeGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            file_nodes: HashMap::new(),
            path_to_file: HashMap::new(),
            suffix_to_file: HashMap::new(),
            module_to_file: HashMap::new(),
            external_modules: HashMap::new(),
            function_nodes: HashMap::new(),
            class_nodes: HashMap::new(),
        }
    }

    /// Get or create an external module node
    pub fn get_or_create_external_module(
        &mut self,
        name: &str,
        category: ModuleCategory,
    ) -> NodeIndex {
        if let Some(&idx) = self.external_modules.get(name) {
            return idx;
        }
        let idx = self.graph.add_node(GraphNode::ExternalModule {
            name: name.to_string(),
            category,
        });
        self.external_modules.insert(name.to_string(), idx);
        idx
    }

    /// Find a file node by path (supports partial matching for relative imports)
    ///
    /// This uses pre-built indexes for O(1) lookups instead of O(n) iteration:
    /// 1. Exact path match via path_to_file
    /// 2. Module path (dot-separated) via module_to_file
    /// 3. Path suffix match via suffix_to_file
    pub fn find_file_by_path(&self, path: &str) -> Option<NodeIndex> {
        // Try exact match first (fastest)
        if let Some(&idx) = self.path_to_file.get(path) {
            return Some(idx);
        }

        // Try module path lookup (e.g., "auth.middleware" -> "auth/middleware.py")
        if let Some(&idx) = self.module_to_file.get(path) {
            return Some(idx);
        }

        // Try suffix match via pre-built index
        if let Some(&idx) = self.suffix_to_file.get(path) {
            return Some(idx);
        }

        // Try converting module path to file path and lookup
        if path.contains('.') {
            let file_path = path.replace('.', "/");
            // Try with common extensions
            for ext in &[".py", ".ts", ".tsx", ".js", ".go", ".rs"] {
                let full_path = format!("{}{}", file_path, ext);
                if let Some(&idx) = self.suffix_to_file.get(&full_path) {
                    return Some(idx);
                }
            }
            // Try __init__.py for package imports
            let init_path = format!("{}/__init__.py", file_path);
            if let Some(&idx) = self.suffix_to_file.get(&init_path) {
                return Some(idx);
            }
        }

        None
    }

    /// Get all files that directly import a given file
    pub fn get_importers(&self, file_id: FileId) -> Vec<FileId> {
        let Some(&target_idx) = self.file_nodes.get(&file_id) else {
            return vec![];
        };

        self.graph
            .edges_directed(target_idx, Direction::Incoming)
            .filter(|e| {
                matches!(
                    e.weight(),
                    GraphEdgeKind::Imports | GraphEdgeKind::ImportsFrom { .. }
                )
            })
            .filter_map(|e| {
                let source_idx = e.source();
                if let GraphNode::File { file_id, .. } = &self.graph[source_idx] {
                    Some(*file_id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get all files that a given file directly imports
    pub fn get_imports(&self, file_id: FileId) -> Vec<FileId> {
        let Some(&source_idx) = self.file_nodes.get(&file_id) else {
            return vec![];
        };

        self.graph
            .edges_directed(source_idx, Direction::Outgoing)
            .filter(|e| {
                matches!(
                    e.weight(),
                    GraphEdgeKind::Imports | GraphEdgeKind::ImportsFrom { .. }
                )
            })
            .filter_map(|e| {
                let target_idx = e.target();
                if let GraphNode::File { file_id, .. } = &self.graph[target_idx] {
                    Some(*file_id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get all files that transitively import a given file (up to max_depth hops)
    pub fn get_transitive_importers(
        &self,
        file_id: FileId,
        max_depth: usize,
    ) -> Vec<(FileId, usize)> {
        let Some(&start_idx) = self.file_nodes.get(&file_id) else {
            return vec![];
        };

        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();

        visited.insert(start_idx);
        queue.push_back((start_idx, 0usize));

        while let Some((current_idx, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            for edge in self.graph.edges_directed(current_idx, Direction::Incoming) {
                if !matches!(
                    edge.weight(),
                    GraphEdgeKind::Imports | GraphEdgeKind::ImportsFrom { .. }
                ) {
                    continue;
                }

                let importer_idx = edge.source();
                if visited.contains(&importer_idx) {
                    continue;
                }

                visited.insert(importer_idx);

                if let GraphNode::File { file_id, .. } = &self.graph[importer_idx] {
                    result.push((*file_id, depth + 1));
                    queue.push_back((importer_idx, depth + 1));
                }
            }
        }

        result
    }

    /// Get external libraries used by a file
    pub fn get_external_dependencies(&self, file_id: FileId) -> Vec<String> {
        let Some(&file_idx) = self.file_nodes.get(&file_id) else {
            return vec![];
        };

        self.graph
            .edges_directed(file_idx, Direction::Outgoing)
            .filter(|e| matches!(e.weight(), GraphEdgeKind::UsesLibrary))
            .filter_map(|e| {
                let target_idx = e.target();
                if let GraphNode::ExternalModule { name, .. } = &self.graph[target_idx] {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get all files that use a specific external library
    pub fn get_files_using_library(&self, library_name: &str) -> Vec<FileId> {
        let Some(&lib_idx) = self.external_modules.get(library_name) else {
            return vec![];
        };

        self.graph
            .edges_directed(lib_idx, Direction::Incoming)
            .filter(|e| matches!(e.weight(), GraphEdgeKind::UsesLibrary))
            .filter_map(|e| {
                let source_idx = e.source();
                if let GraphNode::File { file_id, .. } = &self.graph[source_idx] {
                    Some(*file_id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get statistics about the graph
    pub fn stats(&self) -> GraphStats {
        let mut file_count = 0;
        let mut function_count = 0;
        let mut class_count = 0;
        let mut external_module_count = 0;

        for node in self.graph.node_weights() {
            match node {
                GraphNode::File { .. } => file_count += 1,
                GraphNode::Function { .. } => function_count += 1,
                GraphNode::Class { .. } => class_count += 1,
                GraphNode::ExternalModule { .. } => external_module_count += 1,
                _ => {}
            }
        }

        let mut import_edge_count = 0;
        let mut contains_edge_count = 0;
        let mut uses_library_edge_count = 0;
        let mut calls_edge_count = 0;

        for edge in self.graph.edge_weights() {
            match edge {
                GraphEdgeKind::Imports | GraphEdgeKind::ImportsFrom { .. } => {
                    import_edge_count += 1
                }
                GraphEdgeKind::Contains => contains_edge_count += 1,
                GraphEdgeKind::UsesLibrary => uses_library_edge_count += 1,
                GraphEdgeKind::Calls => calls_edge_count += 1,
                _ => {}
            }
        }

        GraphStats {
            file_count,
            function_count,
            class_count,
            external_module_count,
            import_edge_count,
            contains_edge_count,
            uses_library_edge_count,
            calls_edge_count,
            total_nodes: self.graph.node_count(),
            total_edges: self.graph.edge_count(),
        }
    }

    /// Rebuild all lookup indexes from the graph.
    ///
    /// This must be called after deserializing a CodeGraph to restore
    /// the quick-lookup HashMaps that are skipped during serialization.
    pub fn rebuild_indexes(&mut self) {
        self.file_nodes.clear();
        self.path_to_file.clear();
        self.suffix_to_file.clear();
        self.module_to_file.clear();
        self.external_modules.clear();
        self.function_nodes.clear();
        self.class_nodes.clear();

        for node_idx in self.graph.node_indices() {
            match &self.graph[node_idx] {
                GraphNode::File { file_id, path, .. } => {
                    self.file_nodes.insert(*file_id, node_idx);
                    self.path_to_file.insert(path.clone(), node_idx);
                    // Build suffix indexes for fast import resolution
                    Self::add_path_to_indexes(
                        path,
                        node_idx,
                        &mut self.suffix_to_file,
                        &mut self.module_to_file,
                    );
                }
                GraphNode::Function { file_id, name, .. } => {
                    self.function_nodes.insert((*file_id, name.clone()), node_idx);
                }
                GraphNode::Class { file_id, name, .. } => {
                    self.class_nodes.insert((*file_id, name.clone()), node_idx);
                }
                GraphNode::ExternalModule { name, .. } => {
                    self.external_modules.insert(name.clone(), node_idx);
                }
                _ => {}
            }
        }
    }

    /// Add a path to suffix and module indexes for fast lookup
    fn add_path_to_indexes(
        path: &str,
        node_idx: NodeIndex,
        suffix_to_file: &mut HashMap<String, NodeIndex>,
        module_to_file: &mut HashMap<String, NodeIndex>,
    ) {
        // Add various suffixes for import resolution
        // e.g., "src/auth/middleware.py" -> ["middleware.py", "auth/middleware.py", "src/auth/middleware.py"]
        let parts: Vec<&str> = path.split('/').collect();
        for i in 0..parts.len() {
            let suffix: String = parts[i..].join("/");
            // Only insert if not already present (first path wins)
            suffix_to_file.entry(suffix).or_insert(node_idx);
        }

        // Add module-style path (dots instead of slashes, no extension)
        // e.g., "src/auth/middleware.py" -> "src.auth.middleware"
        if let Some(without_ext) = path.strip_suffix(".py")
            .or_else(|| path.strip_suffix(".ts"))
            .or_else(|| path.strip_suffix(".tsx"))
            .or_else(|| path.strip_suffix(".js"))
            .or_else(|| path.strip_suffix(".go"))
            .or_else(|| path.strip_suffix(".rs"))
        {
            let module_path = without_ext.replace('/', ".");
            module_to_file.entry(module_path.clone()).or_insert(node_idx);

            // Also add partial module paths
            let mod_parts: Vec<&str> = module_path.split('.').collect();
            for i in 0..mod_parts.len() {
                let partial: String = mod_parts[i..].join(".");
                module_to_file.entry(partial).or_insert(node_idx);
            }
        }

        // Special case for __init__.py - map directory to the init file
        if path.ends_with("__init__.py") {
            if let Some(dir_path) = path.strip_suffix("/__init__.py") {
                let module_path = dir_path.replace('/', ".");
                module_to_file.entry(module_path).or_insert(node_idx);
            }
        }
    }
}

impl Default for CodeGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the code graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphStats {
    pub file_count: usize,
    pub function_count: usize,
    pub class_count: usize,
    pub external_module_count: usize,
    pub import_edge_count: usize,
    pub contains_edge_count: usize,
    pub uses_library_edge_count: usize,
    /// Number of function-to-function call edges
    pub calls_edge_count: usize,
    pub total_nodes: usize,
    pub total_edges: usize,
}

/// Build a CodeGraph from all file semantics.
///
/// `sem_entries` is typically a snapshot of the session's semantics map:
/// (FileId, Arc<SourceSemantics>).
pub fn build_code_graph(sem_entries: &[(FileId, Arc<SourceSemantics>)]) -> CodeGraph {
    let mut cg = CodeGraph::new();

    // First pass: create file nodes and collect path mappings with suffix indexes
    for (file_id, sem) in sem_entries {
        let (path, language) = match sem.as_ref() {
            SourceSemantics::Python(py) => (py.path.clone(), Language::Python),
            SourceSemantics::Go(go) => (go.path.clone(), Language::Go),
            SourceSemantics::Rust(rs) => (rs.path.clone(), Language::Rust),
            SourceSemantics::Typescript(ts) => (ts.path.clone(), Language::Typescript),
        };

        let node_index = cg.graph.add_node(GraphNode::File {
            file_id: *file_id,
            path: path.clone(),
            language,
        });

        cg.file_nodes.insert(*file_id, node_index);
        cg.path_to_file.insert(path.clone(), node_index);
        
        // Build suffix and module indexes for fast import resolution
        CodeGraph::add_path_to_indexes(
            &path,
            node_index,
            &mut cg.suffix_to_file,
            &mut cg.module_to_file,
        );
    }

    // Second pass: add functions, classes, imports, and framework-specific nodes
    for (file_id, sem) in sem_entries {
        let file_node = match cg.file_nodes.get(file_id) {
            Some(idx) => *idx,
            None => continue,
        };

        // Add imports (works for all languages via CommonSemantics)
        add_import_edges(&mut cg, file_node, *file_id, sem);

        // Add functions (works for all languages via CommonSemantics)
        add_function_nodes(&mut cg, file_node, *file_id, sem);

        // Language-specific additions
        match sem.as_ref() {
            SourceSemantics::Python(py) => {
                if let Some(fastapi) = &py.fastapi {
                    add_fastapi_nodes(&mut cg, file_node, *file_id, py, fastapi);
                }
            }
            SourceSemantics::Go(go) => {
                // Add Go framework-specific nodes (Gin, Echo, Chi, Fiber, etc.)
                if let Some(framework) = &go.go_framework {
                    add_go_framework_nodes(&mut cg, file_node, *file_id, go, framework);
                }
            }
            SourceSemantics::Rust(rs) => {
                // Add Rust framework-specific nodes (Axum, Actix-web, Rocket, Warp, etc.)
                if let Some(framework) = &rs.rust_framework {
                    add_rust_framework_nodes(&mut cg, file_node, *file_id, rs, framework);
                }
            }
            SourceSemantics::Typescript(ts) => {
                if let Some(express) = &ts.express {
                    add_express_nodes(&mut cg, file_node, *file_id, ts, express);
                }
            }
        }
    }

    // Third pass: add Calls edges between functions
    // First resolve intra-file calls (callee within the same file)
    for (file_id, sem) in sem_entries {
        let functions = match sem.as_ref() {
            SourceSemantics::Python(py) => py.functions(),
            SourceSemantics::Go(go) => go.functions(),
            SourceSemantics::Rust(rs) => rs.functions(),
            SourceSemantics::Typescript(ts) => ts.functions(),
        };

        for func in functions {
            // Get the caller node
            let caller_key = (*file_id, func.name.clone());
            let Some(&caller_node) = cg.function_nodes.get(&caller_key) else {
                continue;
            };

            // Process each call site
            for call in &func.calls {
                // Try to find callee in the same file (intra-file call resolution)
                let callee_key = (*file_id, call.callee.clone());
                if let Some(&callee_node) = cg.function_nodes.get(&callee_key) {
                    // Add Calls edge: caller -> callee
                    cg.graph
                        .add_edge(caller_node, callee_node, GraphEdgeKind::Calls);
                }
            }
        }
    }

    // Fourth pass: add cross-file Calls edges using import analysis
    add_cross_file_call_edges(&mut cg, sem_entries);

    cg
}

/// Add cross-file Calls edges by resolving function calls through imports.
///
/// For each call that wasn't resolved intra-file:
/// 1. Check if the callee name matches an imported item
/// 2. Find the source file of that import
/// 3. Look for the function in that file
/// 4. Add a Calls edge if found
fn add_cross_file_call_edges(
    cg: &mut CodeGraph,
    sem_entries: &[(FileId, Arc<SourceSemantics>)],
) {
    // Build a lookup: file_id -> (file_path, Vec<Import>)
    let imports_by_file: HashMap<FileId, (String, Vec<Import>)> = sem_entries
        .iter()
        .map(|(file_id, sem)| {
            let (path, imports) = match sem.as_ref() {
                SourceSemantics::Python(py) => (py.path.clone(), py.imports()),
                SourceSemantics::Go(go) => (go.path.clone(), go.imports()),
                SourceSemantics::Rust(rs) => (rs.path.clone(), rs.imports()),
                SourceSemantics::Typescript(ts) => (ts.path.clone(), ts.imports()),
            };
            (*file_id, (path, imports))
        })
        .collect();

    // For each file and its functions
    for (file_id, sem) in sem_entries {
        let functions = match sem.as_ref() {
            SourceSemantics::Python(py) => py.functions(),
            SourceSemantics::Go(go) => go.functions(),
            SourceSemantics::Rust(rs) => rs.functions(),
            SourceSemantics::Typescript(ts) => ts.functions(),
        };

        let empty_path = String::new();
        let empty_imports = Vec::new();
        let (file_path, imports) = imports_by_file
            .get(file_id)
            .map(|(p, i)| (p.as_str(), i.as_slice()))
            .unwrap_or((empty_path.as_str(), empty_imports.as_slice()));

        for func in functions {
            // Get the caller node
            let caller_key = (*file_id, func.name.clone());
            let Some(&caller_node) = cg.function_nodes.get(&caller_key) else {
                continue;
            };

            // Process each call site
            for call in &func.calls {
                // Skip if already resolved intra-file
                let callee_key = (*file_id, call.callee.clone());
                if cg.function_nodes.contains_key(&callee_key) {
                    continue;
                }

                // Try to resolve through imports (with file path context for relative imports)
                if let Some(callee_node) =
                    resolve_call_through_imports(cg, &call.callee, &call.callee_expr, imports, file_path)
                {
                    cg.graph
                        .add_edge(caller_node, callee_node, GraphEdgeKind::Calls);
                }
            }
        }
    }
}

/// Try to resolve a function call through imports.
///
/// Returns the NodeIndex of the callee function if found.
///
/// # Arguments
///
/// * `cg` - The code graph containing file and function nodes
/// * `callee` - The simple function name being called (e.g., "add")
/// * `callee_expr` - The full call expression (e.g., "utils.add" or just "add")
/// * `imports` - The list of imports in the calling file
/// * `importing_file_path` - The path of the file making the call (for relative import resolution)
fn resolve_call_through_imports(
    cg: &CodeGraph,
    callee: &str,
    callee_expr: &str,
    imports: &[Import],
    importing_file_path: &str,
) -> Option<NodeIndex> {
    // Strategy 1: Direct import match
    // e.g., `from utils import process` then call `process()`
    // or `from .utils import add` then call `add()`
    for import in imports {
        // Check if the callee is a directly imported item
        if import.imports_item(callee) {
            // Find the source file for this import, with context for relative imports
            if let Some(source_file_idx) = find_import_source_file_with_context(cg, &import.module_path, importing_file_path) {
                // Get the file_id from the source file node
                if let GraphNode::File { file_id, .. } = &cg.graph[source_file_idx] {
                    // Look for the function in that file
                    let callee_key = (*file_id, callee.to_string());
                    if let Some(&func_node) = cg.function_nodes.get(&callee_key) {
                        return Some(func_node);
                    }
                }
            }
        }
    }

    // Strategy 2: Module attribute access
    // e.g., `import utils` then call `utils.process()`
    // The callee_expr would be "utils.process" and callee would be "process"
    if callee_expr.contains('.') {
        let parts: Vec<&str> = callee_expr.split('.').collect();
        if parts.len() >= 2 {
            let module_alias = parts[0];
            let func_name = parts[parts.len() - 1];

            for import in imports {
                // Check if module was imported with this alias
                let matches_alias = import.module_alias.as_deref() == Some(module_alias)
                    || import.local_module_name() == Some(module_alias);

                if matches_alias {
                    if let Some(source_file_idx) = find_import_source_file_with_context(cg, &import.module_path, importing_file_path) {
                        if let GraphNode::File { file_id, .. } = &cg.graph[source_file_idx] {
                            let callee_key = (*file_id, func_name.to_string());
                            if let Some(&func_node) = cg.function_nodes.get(&callee_key) {
                                return Some(func_node);
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

/// Find the file node for an import, with context for relative import resolution.
///
/// This function can resolve both absolute and relative imports.
///
/// # Arguments
///
/// * `cg` - The code graph containing file nodes
/// * `module_path` - The module path from the import (e.g., ".utils", "pkg.models")
/// * `importing_file_path` - The path of the file doing the import (for relative resolution)
fn find_import_source_file_with_context(cg: &CodeGraph, module_path: &str, importing_file_path: &str) -> Option<NodeIndex> {
    // Handle relative imports
    if module_path.starts_with('.') {
        // Use the same resolution logic as add_import_edges
        let possible_paths = resolve_relative_import(importing_file_path, module_path);
        for path in &possible_paths {
            if let Some(idx) = cg.find_file_by_path(path) {
                return Some(idx);
            }
        }
        return None;
    }
    
    // For absolute imports, delegate to the existing function
    find_import_source_file(cg, module_path)
}

/// Find the file node that corresponds to an import module path.
///
/// For relative imports, this function needs the importing file path to resolve
/// the relative path. When called without that context (from cross-file call resolution),
/// it only handles absolute imports.
fn find_import_source_file(cg: &CodeGraph, module_path: &str) -> Option<NodeIndex> {
    // Relative imports start with '.' - these can't be resolved without the importing file path
    // The add_import_edges function handles relative imports directly
    if module_path.starts_with('.') {
        return None;
    }
    
    // Try various path patterns for absolute imports
    let module_as_file = module_path.replace('.', "/");
    let possible_paths = [
        format!("{}.py", module_as_file),
        format!("{}/__init__.py", module_as_file),
        format!("{}.ts", module_as_file),
        format!("{}.tsx", module_as_file),
        format!("{}.js", module_as_file),
        format!("{}.go", module_as_file),
        format!("{}.rs", module_as_file),
        module_as_file.clone(),
    ];

    for path in &possible_paths {
        if let Some(idx) = cg.find_file_by_path(path) {
            return Some(idx);
        }
    }

    None
}

/// Add import edges from a file to other files or external modules
fn add_import_edges(
    cg: &mut CodeGraph,
    file_node: NodeIndex,
    _file_id: FileId,
    sem: &Arc<SourceSemantics>,
) {
    // Get the file path to resolve relative imports
    let file_path = match sem.as_ref() {
        SourceSemantics::Python(py) => py.path.clone(),
        SourceSemantics::Go(go) => go.path.clone(),
        SourceSemantics::Rust(rs) => rs.path.clone(),
        SourceSemantics::Typescript(ts) => ts.path.clone(),
    };
    
    // Get imports via CommonSemantics trait
    let imports = match sem.as_ref() {
        SourceSemantics::Python(py) => py.imports(),
        SourceSemantics::Go(go) => go.imports(),
        SourceSemantics::Rust(rs) => rs.imports(),
        SourceSemantics::Typescript(ts) => ts.imports(),
    };

    for import in imports {
        // First, try to find the imported module as a file in our graph
        // This handles both:
        // - Explicit local/relative imports (import.is_local() == true)
        // - Absolute imports to local packages (e.g., "from myapp.task import Task")
        //
        // We try multiple path patterns to find the file:
        // 1. Exact module path (e.g., "reliably_app.task" -> "reliably_app/task.py")
        // 2. Module path as directory init (e.g., "reliably_app" -> "reliably_app/__init__.py")
        // 3. Relative imports (e.g., ".utils" resolved relative to the importing file's directory)
        
        let possible_paths = if import.module_path.starts_with('.') {
            // Handle relative imports (Python-style: from .utils import foo)
            // The module path starts with one or more dots indicating relative level
            resolve_relative_import(&file_path, &import.module_path)
        } else {
            // Absolute imports
            let module_as_file = import.module_path.replace('.', "/");
            vec![
                format!("{}.py", module_as_file),
                format!("{}/__init__.py", module_as_file),
                format!("{}.ts", module_as_file),
                format!("{}.tsx", module_as_file),
                format!("{}.js", module_as_file),
                module_as_file.clone(),
            ]
        };

        let mut found_local_file = false;
        for path in &possible_paths {
            if let Some(target_idx) = cg.find_file_by_path(path) {
                // Found as a local file - create import edge
                if import.items.is_empty() {
                    cg.graph
                        .add_edge(file_node, target_idx, GraphEdgeKind::Imports);
                } else {
                    let items: Vec<String> = import.items.iter().map(|i| i.name.clone()).collect();
                    cg.graph
                        .add_edge(file_node, target_idx, GraphEdgeKind::ImportsFrom { items });
                }
                found_local_file = true;
                break;
            }
        }

        // If not found as a local file, treat as external library
        if !found_local_file {
            let category = categorize_module(&import.module_path);
            let package_name = import.package_name().to_string();
            let module_idx = cg.get_or_create_external_module(&package_name, category);
            cg.graph
                .add_edge(file_node, module_idx, GraphEdgeKind::UsesLibrary);
        }
    }
}

/// Resolve a relative import path to possible file paths.
///
/// Python relative imports use dots to indicate the relative level:
/// - `.utils` means `utils` in the same package
/// - `..utils` means `utils` in the parent package
/// - etc.
///
/// # Arguments
///
/// * `importing_file` - The path of the file doing the import (e.g., "app.py")
/// * `module_path` - The relative module path (e.g., ".utils", "..models.user")
///
/// # Returns
///
/// A vector of possible file paths to try for resolution.
fn resolve_relative_import(importing_file: &str, module_path: &str) -> Vec<String> {
    // Count leading dots to determine relative level
    let dots = module_path.chars().take_while(|&c| c == '.').count();
    let remaining = &module_path[dots..];
    
    // Get the directory of the importing file
    let importing_dir = if let Some(last_slash) = importing_file.rfind('/') {
        &importing_file[..last_slash]
    } else {
        // File is in root directory
        ""
    };
    
    // Go up `dots - 1` directories (one dot = same directory, two dots = parent, etc.)
    let mut base_dir = importing_dir.to_string();
    for _ in 0..(dots.saturating_sub(1)) {
        if let Some(last_slash) = base_dir.rfind('/') {
            base_dir = base_dir[..last_slash].to_string();
        } else {
            base_dir = String::new();
            break;
        }
    }
    
    // Convert remaining module path to file path
    let module_as_path = remaining.replace('.', "/");
    
    // Build the resolved path
    let resolved_base = if base_dir.is_empty() {
        module_as_path
    } else if module_as_path.is_empty() {
        // Just dots, no module name - refers to __init__.py
        base_dir
    } else {
        format!("{}/{}", base_dir, module_as_path)
    };
    
    // Return possible file paths
    vec![
        format!("{}.py", resolved_base),
        format!("{}/__init__.py", resolved_base),
        format!("{}.ts", resolved_base),
        format!("{}.tsx", resolved_base),
        format!("{}.js", resolved_base),
        resolved_base,
    ]
}

/// Add function nodes from a file
///
/// For Python files with FastAPI and TypeScript files with Express.js, route handlers
/// are skipped here since they will be added by framework-specific functions with
/// HTTP method/path metadata.
fn add_function_nodes(
    cg: &mut CodeGraph,
    file_node: NodeIndex,
    file_id: FileId,
    sem: &Arc<SourceSemantics>,
) {
    // Get functions via CommonSemantics trait
    let functions = match sem.as_ref() {
        SourceSemantics::Python(py) => py.functions(),
        SourceSemantics::Go(go) => go.functions(),
        SourceSemantics::Rust(rs) => rs.functions(),
        SourceSemantics::Typescript(ts) => ts.functions(),
    };

    // Collect framework route handler names to skip
    // (they'll be added by framework-specific functions with HTTP metadata)
    let handler_names_to_skip: std::collections::HashSet<&str> = match sem.as_ref() {
        SourceSemantics::Python(py) => {
            // Skip FastAPI route handlers
            if let Some(fastapi) = &py.fastapi {
                fastapi.routes.iter().map(|r| r.handler_name.as_str()).collect()
            } else {
                std::collections::HashSet::new()
            }
        }
        SourceSemantics::Typescript(ts) => {
            // Skip Express route handlers
            if let Some(express) = &ts.express {
                express.routes.iter()
                    .filter_map(|r| r.handler_name.as_deref())
                    .collect()
            } else {
                std::collections::HashSet::new()
            }
        }
        SourceSemantics::Go(go) => {
            // Skip Go framework route handlers (Gin, Echo, Fiber, Chi)
            if let Some(framework) = &go.go_framework {
                framework.routes.iter()
                    .filter_map(|r| r.handler_name.as_deref())
                    .collect()
            } else {
                std::collections::HashSet::new()
            }
        }
        SourceSemantics::Rust(_rs) => {
            // Rust doesn't have framework route handlers yet
            std::collections::HashSet::new()
        }
    };

    for func in functions {
        // Skip framework route handlers - they're added by framework-specific functions with HTTP metadata
        if handler_names_to_skip.contains(func.name.as_str()) {
            continue;
        }

        let qualified_name = match &func.class_name {
            Some(class) => format!("{}.{}", class, func.name),
            None => func.name.clone(),
        };

        let func_node = cg.graph.add_node(GraphNode::Function {
            file_id,
            name: func.name.clone(),
            qualified_name: qualified_name.clone(),
            is_async: func.is_async,
            is_handler: func.is_route_handler(),
            http_method: None,
            http_path: None,
        });

        // File contains function
        cg.graph
            .add_edge(file_node, func_node, GraphEdgeKind::Contains);

        // Store for lookup
        cg.function_nodes
            .insert((file_id, func.name.clone()), func_node);
    }
}

/// Categorize a module based on its name
fn categorize_module(module_path: &str) -> ModuleCategory {
    let path_lower = module_path.to_lowercase();

    // Logging - check first to catch "logging", "structlog" etc before other patterns
    if path_lower == "logging"
        || path_lower.starts_with("logging.")
        || path_lower.contains("structlog")
        || path_lower == "tracing"
        || path_lower.starts_with("tracing::")
        || path_lower.contains("uber.org/zap")
        || path_lower.contains("zerolog")
        || path_lower == "winston"
        || path_lower == "pino"
    {
        return ModuleCategory::Logging;
    }

    // HTTP clients
    if path_lower.contains("requests")
        || path_lower.contains("httpx")
        || path_lower.contains("aiohttp")
        || path_lower.contains("urllib")
        || path_lower.contains("axios")
        || path_lower.contains("fetch")
        || path_lower.contains("got")
        || path_lower.contains("reqwest")
        || path_lower.contains("hyper")
    {
        return ModuleCategory::HttpClient;
    }

    // Databases
    if path_lower.contains("sqlalchemy")
        || path_lower.contains("prisma")
        || path_lower.contains("typeorm")
        || path_lower.contains("sequelize")
        || path_lower.contains("diesel")
        || path_lower.contains("sqlx")
        || path_lower.contains("gorm")
        || path_lower.contains("database/sql")
    {
        return ModuleCategory::Database;
    }

    // Web frameworks
    if path_lower.contains("fastapi")
        || path_lower.contains("flask")
        || path_lower.contains("django")
        || path_lower.contains("express")
        || path_lower.contains("nestjs")
        || path_lower.contains("gin")
        || path_lower.contains("echo")
        || path_lower.contains("chi")
        || path_lower.contains("axum")
        || path_lower.contains("actix")
    {
        return ModuleCategory::WebFramework;
    }

    // Async runtimes
    if path_lower.contains("asyncio")
        || path_lower.contains("tokio")
        || path_lower.contains("async_std")
    {
        return ModuleCategory::AsyncRuntime;
    }

    // Resilience
    if path_lower.contains("tenacity")
        || path_lower.contains("stamina")
        || path_lower.contains("retry")
        || path_lower.contains("backoff")
        || path_lower.contains("resilience")
    {
        return ModuleCategory::Resilience;
    }

    // Standard library (approximate)
    if module_path.starts_with("os")
        || module_path.starts_with("sys")
        || module_path.starts_with("io")
        || module_path.starts_with("time")
        || module_path.starts_with("json")
        || module_path.starts_with("re")
        || module_path.starts_with("collections")
        || module_path.starts_with("typing")
        || module_path.starts_with("pathlib")
        || module_path.starts_with("fmt")
        || module_path.starts_with("net/")
        || module_path.starts_with("std::")
    {
        return ModuleCategory::StandardLib;
    }

    ModuleCategory::Other
}

fn add_fastapi_nodes(
    cg: &mut CodeGraph,
    file_node: NodeIndex,
    file_id: FileId,
    py: &PyFileSemantics,
    fastapi: &FastApiFileSummary,
) {
    // Map app var_name -> node index so we can wire routes/middlewares.
    let mut app_nodes: HashMap<String, NodeIndex> = HashMap::new();

    // Apps
    for app in &fastapi.apps {
        let app_node = cg.graph.add_node(GraphNode::FastApiApp {
            file_id,
            var_name: app.var_name.clone(),
        });

        // File contains app
        cg.graph
            .add_edge(file_node, app_node, GraphEdgeKind::Contains);

        app_nodes.insert(app.var_name.clone(), app_node);
    }

    // Routes
    for route in &fastapi.routes {
        let route_node = cg.graph.add_node(GraphNode::FastApiRoute {
            file_id,
            http_method: route.http_method.clone(),
            path: route.path.clone(),
        });

        // File contains route
        cg.graph
            .add_edge(file_node, route_node, GraphEdgeKind::Contains);

        // Try to find an app owner by heuristic.
        // For now, we don't know which app exactly, so we might associate all apps.
        // Later, we can refine using app.var_name, routers, and decorators.
        for app_node in app_nodes.values() {
            cg.graph
                .add_edge(*app_node, route_node, GraphEdgeKind::FastApiAppOwnsRoute);
        }

        // Also create a function node for the route handler with HTTP metadata
        let qualified_name = route.handler_name.clone();
        let func_node = cg.graph.add_node(GraphNode::Function {
            file_id,
            name: route.handler_name.clone(),
            qualified_name,
            is_async: route.is_async,
            is_handler: true,
            http_method: Some(route.http_method.clone()),
            http_path: Some(route.path.clone()),
        });

        // File contains function
        cg.graph
            .add_edge(file_node, func_node, GraphEdgeKind::Contains);

        // Function is the route handler
        cg.graph
            .add_edge(func_node, route_node, GraphEdgeKind::Contains);

        // Store for lookup (needed for call resolution)
        cg.function_nodes
            .insert((file_id, route.handler_name.clone()), func_node);
    }

    // Middlewares
    for mw in &fastapi.middlewares {
        let mw_node = cg.graph.add_node(GraphNode::FastApiMiddleware {
            file_id,
            app_var_name: mw.app_var_name.clone(),
            middleware_type: mw.middleware_type.clone(),
        });

        // File contains middleware
        cg.graph
            .add_edge(file_node, mw_node, GraphEdgeKind::Contains);

        // Attach middleware to its app if we know it.
        if let Some(app_node) = app_nodes.get(&mw.app_var_name) {
            cg.graph
                .add_edge(*app_node, mw_node, GraphEdgeKind::FastApiAppHasMiddleware);
        }
    }

    let _ = py; // unused for now, but we'll likely need it later.
}

/// Add Express.js route handlers as function nodes with HTTP metadata.
///
/// This creates function nodes for Express route handlers with `http_method`
/// and `http_path` fields populated, similar to how FastAPI routes are handled.
fn add_express_nodes(
    cg: &mut CodeGraph,
    file_node: NodeIndex,
    file_id: FileId,
    _ts: &TsFileSemantics,
    express: &ExpressFileSummary,
) {
    // Routes - create function nodes with HTTP metadata
    for route in &express.routes {
        // Only add routes that have a handler name (named function handlers)
        let handler_name = match &route.handler_name {
            Some(name) => name.clone(),
            None => continue, // Skip inline anonymous handlers for now
        };

        // Skip if this function was already added by add_function_nodes
        if cg.function_nodes.contains_key(&(file_id, handler_name.clone())) {
            // Update the existing node with HTTP metadata instead of creating a new one
            // For now, we skip - but ideally we'd update the existing node
            // This is a limitation we can fix later by refactoring
            continue;
        }

        let http_method = route.method.to_uppercase();
        let http_path = route.path.clone();

        let func_node = cg.graph.add_node(GraphNode::Function {
            file_id,
            name: handler_name.clone(),
            qualified_name: handler_name.clone(),
            is_async: route.is_async,
            is_handler: true,
            http_method: Some(http_method),
            http_path,
        });

        // File contains function
        cg.graph
            .add_edge(file_node, func_node, GraphEdgeKind::Contains);

        // Store for lookup (needed for call resolution)
        cg.function_nodes.insert((file_id, handler_name), func_node);
    }
}

/// Add Go HTTP framework route handlers as function nodes with HTTP metadata.
///
/// This creates function nodes for Gin, Echo, Fiber, and Chi route handlers with
/// `http_method` and `http_path` fields populated, similar to how FastAPI routes
/// are handled.
fn add_go_framework_nodes(
    cg: &mut CodeGraph,
    file_node: NodeIndex,
    file_id: FileId,
    _go: &GoFileSemantics,
    framework: &GoFrameworkSummary,
) {
    // Routes - create function nodes with HTTP metadata
    for route in &framework.routes {
        // Only add routes that have a handler name (named function handlers)
        let handler_name = match &route.handler_name {
            Some(name) => name.clone(),
            None => continue, // Skip anonymous handlers
        };

        // Skip if this function was already added by add_function_nodes
        if cg.function_nodes.contains_key(&(file_id, handler_name.clone())) {
            // We could update the existing node, but for simplicity we skip
            continue;
        }

        let func_node = cg.graph.add_node(GraphNode::Function {
            file_id,
            name: handler_name.clone(),
            qualified_name: handler_name.clone(),
            is_async: false, // Go doesn't have async keyword
            is_handler: true,
            http_method: Some(route.http_method.clone()),
            http_path: Some(route.path.clone()),
        });

        // File contains function
        cg.graph
            .add_edge(file_node, func_node, GraphEdgeKind::Contains);

        // Store for lookup (needed for call resolution)
        cg.function_nodes.insert((file_id, handler_name), func_node);
    }
}

/// Add Rust HTTP framework route handlers as function nodes with HTTP metadata.
///
/// This creates function nodes for Axum, Actix-web, Rocket, Warp, and Poem route handlers
/// with `http_method` and `http_path` fields populated, similar to how FastAPI routes
/// are handled.
fn add_rust_framework_nodes(
    cg: &mut CodeGraph,
    file_node: NodeIndex,
    file_id: FileId,
    _rs: &RustFileSemantics,
    framework: &RustFrameworkSummary,
) {
    // Routes - create function nodes with HTTP metadata
    for route in &framework.routes {
        let handler_name = route.handler_name.clone();

        // Skip if this function was already added by add_function_nodes
        if cg.function_nodes.contains_key(&(file_id, handler_name.clone())) {
            // We could update the existing node, but for simplicity we skip
            continue;
        }

        let func_node = cg.graph.add_node(GraphNode::Function {
            file_id,
            name: handler_name.clone(),
            qualified_name: handler_name.clone(),
            is_async: route.is_async,
            is_handler: true,
            http_method: Some(route.method.clone()),
            http_path: Some(route.path.clone()),
        });

        // File contains function
        cg.graph
            .add_edge(file_node, func_node, GraphEdgeKind::Contains);

        // Store for lookup (needed for call resolution)
        cg.function_nodes.insert((file_id, handler_name), func_node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::python::parse_python_file;
    use crate::semantics::SourceSemantics;
    use crate::semantics::python::model::PyFileSemantics;
    use crate::types::context::{Language, SourceFile};

    /// Helper to parse Python source and build semantics with framework analysis
    fn parse_and_build_semantics(path: &str, source: &str) -> (FileId, Arc<SourceSemantics>) {
        let sf = SourceFile {
            path: path.to_string(),
            language: Language::Python,
            content: source.to_string(),
        };
        let file_id = FileId(1);
        let parsed = parse_python_file(file_id, &sf).expect("parsing should succeed");
        let mut sem = PyFileSemantics::from_parsed(&parsed);
        sem.analyze_frameworks(&parsed)
            .expect("framework analysis should succeed");
        (file_id, Arc::new(SourceSemantics::Python(sem)))
    }

    fn parse_python_with_id(path: &str, source: &str, id: u64) -> (FileId, Arc<SourceSemantics>) {
        let sf = SourceFile {
            path: path.to_string(),
            language: Language::Python,
            content: source.to_string(),
        };
        let file_id = FileId(id);
        let parsed = parse_python_file(file_id, &sf).expect("parsing should succeed");
        let mut sem = PyFileSemantics::from_parsed(&parsed);
        sem.analyze_frameworks(&parsed)
            .expect("framework analysis should succeed");
        (file_id, Arc::new(SourceSemantics::Python(sem)))
    }

    // ==================== CodeGraph Tests ====================

    #[test]
    fn code_graph_new_creates_empty_graph() {
        let cg = CodeGraph::new();
        assert_eq!(cg.graph.node_count(), 0);
        assert_eq!(cg.graph.edge_count(), 0);
        assert!(cg.file_nodes.is_empty());
    }

    #[test]
    fn code_graph_debug_impl() {
        let cg = CodeGraph::new();
        let debug_str = format!("{:?}", cg);
        assert!(debug_str.contains("CodeGraph"));
    }

    #[test]
    fn code_graph_default_impl() {
        let cg = CodeGraph::default();
        assert_eq!(cg.graph.node_count(), 0);
    }

    // ==================== GraphNode Tests ====================

    #[test]
    fn graph_node_file_debug() {
        let node = GraphNode::File {
            file_id: FileId(1),
            path: "test.py".to_string(),
            language: Language::Python,
        };
        let debug_str = format!("{:?}", node);
        assert!(debug_str.contains("File"));
        assert!(debug_str.contains("test.py"));
    }

    #[test]
    fn graph_node_function_debug() {
        let node = GraphNode::Function {
            file_id: FileId(1),
            name: "my_func".to_string(),
            qualified_name: "MyClass.my_func".to_string(),
            is_async: true,
            is_handler: false,
            http_method: None,
            http_path: None,
        };
        let debug_str = format!("{:?}", node);
        assert!(debug_str.contains("Function"));
        assert!(debug_str.contains("my_func"));
    }

    #[test]
    fn graph_node_class_debug() {
        let node = GraphNode::Class {
            file_id: FileId(1),
            name: "MyClass".to_string(),
        };
        let debug_str = format!("{:?}", node);
        assert!(debug_str.contains("Class"));
        assert!(debug_str.contains("MyClass"));
    }

    #[test]
    fn graph_node_external_module_debug() {
        let node = GraphNode::ExternalModule {
            name: "requests".to_string(),
            category: ModuleCategory::HttpClient,
        };
        let debug_str = format!("{:?}", node);
        assert!(debug_str.contains("ExternalModule"));
        assert!(debug_str.contains("requests"));
    }

    #[test]
    fn graph_node_fastapi_app_debug() {
        let node = GraphNode::FastApiApp {
            file_id: FileId(1),
            var_name: "app".to_string(),
        };
        let debug_str = format!("{:?}", node);
        assert!(debug_str.contains("FastApiApp"));
        assert!(debug_str.contains("app"));
    }

    #[test]
    fn graph_node_fastapi_route_debug() {
        let node = GraphNode::FastApiRoute {
            file_id: FileId(1),
            http_method: "GET".to_string(),
            path: "/users".to_string(),
        };
        let debug_str = format!("{:?}", node);
        assert!(debug_str.contains("FastApiRoute"));
        assert!(debug_str.contains("GET"));
    }

    #[test]
    fn graph_node_fastapi_middleware_debug() {
        let node = GraphNode::FastApiMiddleware {
            file_id: FileId(1),
            app_var_name: "app".to_string(),
            middleware_type: "CORSMiddleware".to_string(),
        };
        let debug_str = format!("{:?}", node);
        assert!(debug_str.contains("FastApiMiddleware"));
        assert!(debug_str.contains("CORSMiddleware"));
    }

    #[test]
    fn graph_node_display_name() {
        let file = GraphNode::File {
            file_id: FileId(1),
            path: "src/main.py".to_string(),
            language: Language::Python,
        };
        assert_eq!(file.display_name(), "src/main.py");

        let func = GraphNode::Function {
            file_id: FileId(1),
            name: "process".to_string(),
            qualified_name: "Handler.process".to_string(),
            is_async: false,
            is_handler: true,
            http_method: None,
            http_path: None,
        };
        assert_eq!(func.display_name(), "Handler.process");

        let module = GraphNode::ExternalModule {
            name: "fastapi".to_string(),
            category: ModuleCategory::WebFramework,
        };
        assert_eq!(module.display_name(), "fastapi");
    }

    #[test]
    fn graph_node_file_id() {
        let file = GraphNode::File {
            file_id: FileId(1),
            path: "test.py".to_string(),
            language: Language::Python,
        };
        assert_eq!(file.file_id(), Some(FileId(1)));

        let module = GraphNode::ExternalModule {
            name: "requests".to_string(),
            category: ModuleCategory::HttpClient,
        };
        assert_eq!(module.file_id(), None);
    }

    #[test]
    fn graph_node_is_file() {
        let file = GraphNode::File {
            file_id: FileId(1),
            path: "test.py".to_string(),
            language: Language::Python,
        };
        assert!(file.is_file());

        let func = GraphNode::Function {
            file_id: FileId(1),
            name: "test".to_string(),
            qualified_name: "test".to_string(),
            is_async: false,
            is_handler: false,
            http_method: None,
            http_path: None,
        };
        assert!(!func.is_file());
    }

    // ==================== GraphEdgeKind Tests ====================

    #[test]
    fn graph_edge_kind_debug() {
        let edge = GraphEdgeKind::Contains;
        let debug_str = format!("{:?}", edge);
        assert!(debug_str.contains("Contains"));

        let edge = GraphEdgeKind::Imports;
        let debug_str = format!("{:?}", edge);
        assert!(debug_str.contains("Imports"));

        let edge = GraphEdgeKind::ImportsFrom {
            items: vec!["FastAPI".to_string()],
        };
        let debug_str = format!("{:?}", edge);
        assert!(debug_str.contains("ImportsFrom"));
        assert!(debug_str.contains("FastAPI"));

        let edge = GraphEdgeKind::UsesLibrary;
        let debug_str = format!("{:?}", edge);
        assert!(debug_str.contains("UsesLibrary"));
    }

    #[test]
    fn graph_edge_kind_eq() {
        assert_eq!(GraphEdgeKind::Contains, GraphEdgeKind::Contains);
        assert_eq!(GraphEdgeKind::Imports, GraphEdgeKind::Imports);
        assert_ne!(GraphEdgeKind::Contains, GraphEdgeKind::Imports);

        let edge1 = GraphEdgeKind::ImportsFrom {
            items: vec!["A".to_string()],
        };
        let edge2 = GraphEdgeKind::ImportsFrom {
            items: vec!["A".to_string()],
        };
        let edge3 = GraphEdgeKind::ImportsFrom {
            items: vec!["B".to_string()],
        };
        assert_eq!(edge1, edge2);
        assert_ne!(edge1, edge3);
    }

    // ==================== build_code_graph Tests ====================

    #[test]
    fn build_code_graph_empty_semantics() {
        let sem_entries: Vec<(FileId, Arc<SourceSemantics>)> = vec![];
        let cg = build_code_graph(&sem_entries);
        assert_eq!(cg.graph.node_count(), 0);
        assert_eq!(cg.graph.edge_count(), 0);
    }

    #[test]
    fn build_code_graph_single_file_no_fastapi() {
        let (file_id, sem) = parse_and_build_semantics("test.py", "x = 1\ny = 2");
        let sem_entries = vec![(file_id, sem)];

        let cg = build_code_graph(&sem_entries);

        // Should have one file node
        assert!(cg.graph.node_count() >= 1);
        assert!(cg.file_nodes.contains_key(&file_id));
    }

    #[test]
    fn build_code_graph_with_function() {
        let src = r#"
def process_data(data):
    return data * 2

async def fetch_user(user_id):
    return {"id": user_id}
"#;
        let (file_id, sem) = parse_and_build_semantics("handlers.py", src);
        let sem_entries = vec![(file_id, sem)];

        let cg = build_code_graph(&sem_entries);

        // Should have file node + 2 function nodes
        let stats = cg.stats();
        assert_eq!(stats.file_count, 1);
        assert!(stats.function_count >= 2);

        // Functions should be in lookup
        assert!(
            cg.function_nodes
                .contains_key(&(file_id, "process_data".to_string()))
        );
        assert!(
            cg.function_nodes
                .contains_key(&(file_id, "fetch_user".to_string()))
        );
    }

    #[test]
    fn build_code_graph_with_external_imports() {
        let src = r#"
import requests
from fastapi import FastAPI
import sqlalchemy
"#;
        let (file_id, sem) = parse_and_build_semantics("main.py", src);
        let sem_entries = vec![(file_id, sem)];

        let cg = build_code_graph(&sem_entries);

        // Should have external module nodes
        assert!(cg.external_modules.contains_key("requests"));
        assert!(cg.external_modules.contains_key("fastapi"));
        assert!(cg.external_modules.contains_key("sqlalchemy"));

        // Should have UsesLibrary edges
        let stats = cg.stats();
        assert!(stats.uses_library_edge_count >= 3);

        // Check categories
        if let Some(&idx) = cg.external_modules.get("requests") {
            if let GraphNode::ExternalModule { category, .. } = &cg.graph[idx] {
                assert_eq!(*category, ModuleCategory::HttpClient);
            }
        }
    }

    #[test]
    fn build_code_graph_with_fastapi_app() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()
"#;
        let (file_id, sem) = parse_and_build_semantics("main.py", src);
        let sem_entries = vec![(file_id, sem)];

        let cg = build_code_graph(&sem_entries);

        // Should have file node + app node + external module
        assert!(cg.graph.node_count() >= 2);

        // Should have at least one edge (file contains app)
        assert!(cg.graph.edge_count() >= 1);
    }

    #[test]
    fn build_code_graph_with_fastapi_middleware() {
        let src = r#"
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

app = FastAPI()

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
)
"#;
        let (file_id, sem) = parse_and_build_semantics("main.py", src);
        let sem_entries = vec![(file_id, sem)];

        let cg = build_code_graph(&sem_entries);

        // Should have file node + app node + middleware node
        assert!(cg.graph.node_count() >= 3);

        // Should have edges: file->app, file->middleware, app->middleware
        assert!(cg.graph.edge_count() >= 3);
    }

    #[test]
    fn build_code_graph_multiple_files() {
        let (file_id1, sem1) = parse_python_with_id("file1.py", "x = 1", 1);
        let (file_id2, sem2) = parse_python_with_id("file2.py", "y = 2", 2);

        let sem_entries = vec![(file_id1, sem1), (file_id2, sem2)];

        let cg = build_code_graph(&sem_entries);

        // Should have two file nodes
        assert_eq!(cg.stats().file_count, 2);
        assert!(cg.file_nodes.contains_key(&file_id1));
        assert!(cg.file_nodes.contains_key(&file_id2));
    }

    #[test]
    fn build_code_graph_with_fastapi_routes() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/users")
async def get_users():
    return []

@app.post("/users")
async def create_user():
    return {}
"#;
        let (file_id, sem) = parse_and_build_semantics("routes.py", src);
        let sem_entries = vec![(file_id, sem)];

        let cg = build_code_graph(&sem_entries);

        // Should have: file node + app node + 2 route nodes + 2 function nodes
        assert!(cg.graph.node_count() >= 4);

        // Verify route nodes exist
        let mut route_count = 0;
        for node in cg.graph.node_weights() {
            if matches!(node, GraphNode::FastApiRoute { .. }) {
                route_count += 1;
            }
        }
        assert_eq!(route_count, 2);
    }

    #[test]
    fn build_code_graph_middleware_attached_to_correct_app() {
        let src = r#"
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

app = FastAPI()

app.add_middleware(CORSMiddleware)
"#;
        let (file_id, sem) = parse_and_build_semantics("main.py", src);
        let sem_entries = vec![(file_id, sem)];

        let cg = build_code_graph(&sem_entries);

        // Find the middleware node and verify it's connected to the app
        let mut found_middleware_edge = false;
        for edge in cg.graph.edge_weights() {
            if matches!(edge, GraphEdgeKind::FastApiAppHasMiddleware) {
                found_middleware_edge = true;
                break;
            }
        }
        assert!(found_middleware_edge);
    }

    #[test]
    fn build_code_graph_middleware_without_matching_app() {
        let src = r#"
from fastapi.middleware.cors import CORSMiddleware

def setup_cors(app):
    app.add_middleware(CORSMiddleware)
"#;
        let (file_id, sem) = parse_and_build_semantics("cors_setup.py", src);
        let sem_entries = vec![(file_id, sem)];

        let cg = build_code_graph(&sem_entries);

        // Should still build without errors
        assert!(cg.graph.node_count() >= 1);
    }

    // ==================== Graph Query Tests ====================

    #[test]
    fn code_graph_get_external_dependencies() {
        let src = r#"
import requests
from fastapi import FastAPI
"#;
        let (file_id, sem) = parse_and_build_semantics("main.py", src);
        let sem_entries = vec![(file_id, sem)];

        let cg = build_code_graph(&sem_entries);

        let deps = cg.get_external_dependencies(file_id);
        assert!(deps.contains(&"requests".to_string()));
        assert!(deps.contains(&"fastapi".to_string()));
    }

    #[test]
    fn code_graph_get_files_using_library() {
        let src = r#"import requests"#;
        let (file_id, sem) = parse_and_build_semantics("main.py", src);
        let sem_entries = vec![(file_id, sem)];

        let cg = build_code_graph(&sem_entries);

        let files = cg.get_files_using_library("requests");
        assert!(files.contains(&file_id));
    }

    #[test]
    fn code_graph_stats() {
        let src = r#"
import requests
from fastapi import FastAPI

app = FastAPI()

def process():
    pass
"#;
        let (file_id, sem) = parse_and_build_semantics("main.py", src);
        let sem_entries = vec![(file_id, sem)];

        let cg = build_code_graph(&sem_entries);
        let stats = cg.stats();

        assert_eq!(stats.file_count, 1);
        assert!(stats.function_count >= 1);
        assert!(stats.external_module_count >= 2);
        assert!(stats.uses_library_edge_count >= 2);
        assert!(stats.contains_edge_count >= 1);
    }

    // ==================== Module Category Tests ====================

    #[test]
    fn categorize_module_http_client() {
        assert_eq!(categorize_module("requests"), ModuleCategory::HttpClient);
        assert_eq!(categorize_module("httpx"), ModuleCategory::HttpClient);
        assert_eq!(categorize_module("axios"), ModuleCategory::HttpClient);
        assert_eq!(categorize_module("reqwest"), ModuleCategory::HttpClient);
    }

    #[test]
    fn categorize_module_database() {
        assert_eq!(categorize_module("sqlalchemy"), ModuleCategory::Database);
        assert_eq!(categorize_module("prisma"), ModuleCategory::Database);
        assert_eq!(categorize_module("diesel"), ModuleCategory::Database);
    }

    #[test]
    fn categorize_module_web_framework() {
        assert_eq!(categorize_module("fastapi"), ModuleCategory::WebFramework);
        assert_eq!(categorize_module("express"), ModuleCategory::WebFramework);
        assert_eq!(categorize_module("gin"), ModuleCategory::WebFramework);
    }

    #[test]
    fn categorize_module_async_runtime() {
        assert_eq!(categorize_module("asyncio"), ModuleCategory::AsyncRuntime);
        assert_eq!(categorize_module("tokio"), ModuleCategory::AsyncRuntime);
    }

    #[test]
    fn categorize_module_logging() {
        assert_eq!(categorize_module("logging"), ModuleCategory::Logging);
        assert_eq!(categorize_module("structlog"), ModuleCategory::Logging);
        assert_eq!(categorize_module("tracing"), ModuleCategory::Logging);
    }

    #[test]
    fn categorize_module_resilience() {
        assert_eq!(categorize_module("tenacity"), ModuleCategory::Resilience);
        assert_eq!(categorize_module("stamina"), ModuleCategory::Resilience);
    }

    #[test]
    fn categorize_module_stdlib() {
        assert_eq!(categorize_module("os"), ModuleCategory::StandardLib);
        assert_eq!(categorize_module("json"), ModuleCategory::StandardLib);
        assert_eq!(categorize_module("typing"), ModuleCategory::StandardLib);
    }

    #[test]
    fn categorize_module_other() {
        assert_eq!(categorize_module("some_random_lib"), ModuleCategory::Other);
    }

    // ==================== Find File Tests ====================

    #[test]
    fn find_file_by_path_exact() {
        let (file_id, sem) = parse_and_build_semantics("src/main.py", "x = 1");
        let sem_entries = vec![(file_id, sem)];

        let cg = build_code_graph(&sem_entries);

        assert!(cg.find_file_by_path("src/main.py").is_some());
        assert!(cg.find_file_by_path("nonexistent.py").is_none());
    }

    #[test]
    fn find_file_by_path_suffix() {
        let (file_id, sem) = parse_and_build_semantics("src/auth/middleware.py", "x = 1");
        let sem_entries = vec![(file_id, sem)];

        let cg = build_code_graph(&sem_entries);

        // Should find by suffix
        assert!(cg.find_file_by_path("auth/middleware.py").is_some());
        assert!(cg.find_file_by_path("middleware.py").is_some());
    }

    // ==================== Call Edge Tests ====================

    #[test]
    fn calls_edge_count_starts_at_zero() {
        let src = r#"
def foo():
    pass

def bar():
    pass
"#;
        let (file_id, sem) = parse_and_build_semantics("test.py", src);
        let sem_entries = vec![(file_id, sem)];

        let cg = build_code_graph(&sem_entries);
        let stats = cg.stats();

        // Currently we don't extract calls from source yet, so count is 0
        assert_eq!(stats.calls_edge_count, 0);
        // But we should have function nodes
        assert!(stats.function_count >= 2);
    }

    #[test]
    fn calls_edge_manual_creation() {
        // Test that we can manually add Calls edges and count them
        let mut cg = CodeGraph::new();

        let file_id = FileId(1);
        let file_node = cg.graph.add_node(GraphNode::File {
            file_id,
            path: "test.py".to_string(),
            language: Language::Python,
        });

        let func_a = cg.graph.add_node(GraphNode::Function {
            file_id,
            name: "func_a".to_string(),
            qualified_name: "func_a".to_string(),
            is_async: false,
            is_handler: false,
            http_method: None,
            http_path: None,
        });

        let func_b = cg.graph.add_node(GraphNode::Function {
            file_id,
            name: "func_b".to_string(),
            qualified_name: "func_b".to_string(),
            is_async: false,
            is_handler: false,
            http_method: None,
            http_path: None,
        });

        // File contains both functions
        cg.graph
            .add_edge(file_node, func_a, GraphEdgeKind::Contains);
        cg.graph
            .add_edge(file_node, func_b, GraphEdgeKind::Contains);

        // func_a calls func_b
        cg.graph.add_edge(func_a, func_b, GraphEdgeKind::Calls);

        let stats = cg.stats();
        assert_eq!(stats.calls_edge_count, 1);
        assert_eq!(stats.function_count, 2);
        assert_eq!(stats.contains_edge_count, 2);
    }

    #[test]
    fn graph_edge_kind_calls_debug() {
        let edge = GraphEdgeKind::Calls;
        let debug_str = format!("{:?}", edge);
        assert!(debug_str.contains("Calls"));
    }

    #[test]
    fn graph_edge_kind_calls_eq() {
        assert_eq!(GraphEdgeKind::Calls, GraphEdgeKind::Calls);
        assert_ne!(GraphEdgeKind::Calls, GraphEdgeKind::Contains);
    }

    // ==================== Serialization / Rebuild Tests ====================

    #[test]
    fn rebuild_indexes_restores_lookups() {
        // Build a graph manually
        let mut cg = CodeGraph::new();

        let file_id = FileId(1);
        let file_node = cg.graph.add_node(GraphNode::File {
            file_id,
            path: "test.py".to_string(),
            language: Language::Python,
        });
        cg.file_nodes.insert(file_id, file_node);
        cg.path_to_file.insert("test.py".to_string(), file_node);

        let func_node = cg.graph.add_node(GraphNode::Function {
            file_id,
            name: "my_func".to_string(),
            qualified_name: "my_func".to_string(),
            is_async: false,
            is_handler: false,
            http_method: None,
            http_path: None,
        });
        cg.function_nodes
            .insert((file_id, "my_func".to_string()), func_node);

        let class_node = cg.graph.add_node(GraphNode::Class {
            file_id,
            name: "MyClass".to_string(),
        });
        cg.class_nodes
            .insert((file_id, "MyClass".to_string()), class_node);

        let ext_node = cg.graph.add_node(GraphNode::ExternalModule {
            name: "requests".to_string(),
            category: ModuleCategory::HttpClient,
        });
        cg.external_modules.insert("requests".to_string(), ext_node);

        // Simulate serialization by clearing all the lookup maps
        cg.file_nodes.clear();
        cg.path_to_file.clear();
        cg.function_nodes.clear();
        cg.class_nodes.clear();
        cg.external_modules.clear();

        // Verify lookups are empty
        assert!(cg.file_nodes.is_empty());
        assert!(cg.path_to_file.is_empty());
        assert!(cg.function_nodes.is_empty());
        assert!(cg.class_nodes.is_empty());
        assert!(cg.external_modules.is_empty());

        // Rebuild indexes
        cg.rebuild_indexes();

        // Verify lookups are restored
        assert!(cg.file_nodes.contains_key(&file_id));
        assert!(cg.path_to_file.contains_key("test.py"));
        assert!(cg.function_nodes.contains_key(&(file_id, "my_func".to_string())));
        assert!(cg.class_nodes.contains_key(&(file_id, "MyClass".to_string())));
        assert!(cg.external_modules.contains_key("requests"));
    }

    #[test]
    fn rebuild_indexes_clears_stale_data() {
        let mut cg = CodeGraph::new();

        // Add some stale data to lookups (not matching graph)
        cg.file_nodes.insert(FileId(999), NodeIndex::new(0));
        cg.external_modules
            .insert("stale_module".to_string(), NodeIndex::new(0));

        // Add a real node
        let file_id = FileId(1);
        let _file_node = cg.graph.add_node(GraphNode::File {
            file_id,
            path: "real.py".to_string(),
            language: Language::Python,
        });

        // Rebuild should clear stale data and add real data
        cg.rebuild_indexes();

        // Stale data should be gone
        assert!(!cg.file_nodes.contains_key(&FileId(999)));
        assert!(!cg.external_modules.contains_key("stale_module"));

        // Real data should be present
        assert!(cg.file_nodes.contains_key(&file_id));
        assert!(cg.path_to_file.contains_key("real.py"));
    }

    #[test]
    fn code_graph_serde_roundtrip() {
        // Build a simple graph
        let src = r#"
import requests

def process():
    pass
"#;
        let (file_id, sem) = parse_and_build_semantics("main.py", src);
        let sem_entries = vec![(file_id, sem)];
        let cg = build_code_graph(&sem_entries);

        // Record stats before serialization
        let stats_before = cg.stats();

        // Serialize to JSON
        let json = serde_json::to_string(&cg).expect("serialization should succeed");

        // Deserialize
        let mut cg_restored: CodeGraph =
            serde_json::from_str(&json).expect("deserialization should succeed");

        // Lookups should be empty after deserialization
        assert!(cg_restored.file_nodes.is_empty());
        assert!(cg_restored.external_modules.is_empty());

        // Rebuild indexes
        cg_restored.rebuild_indexes();

        // Stats should match
        let stats_after = cg_restored.stats();
        assert_eq!(stats_before.file_count, stats_after.file_count);
        assert_eq!(stats_before.function_count, stats_after.function_count);
        assert_eq!(
            stats_before.external_module_count,
            stats_after.external_module_count
        );
        assert_eq!(stats_before.total_nodes, stats_after.total_nodes);
        assert_eq!(stats_before.total_edges, stats_after.total_edges);

        // Lookups should work
        assert!(cg_restored.file_nodes.contains_key(&file_id));
        assert!(cg_restored.external_modules.contains_key("requests"));
    }

    // ==================== Cross-File Call Edge Tests ====================

    #[test]
    fn cross_file_call_edge_direct_import() {
        // File 1 defines a helper function
        let helper_src = r#"
def helper_func():
    return 42
"#;
        let (helper_id, helper_sem) = parse_python_with_id("helpers.py", helper_src, 1);

        // File 2 imports and calls the helper
        let main_src = r#"
from helpers import helper_func

def main():
    result = helper_func()
    return result
"#;
        let (main_id, main_sem) = parse_python_with_id("main.py", main_src, 2);

        let sem_entries = vec![(helper_id, helper_sem), (main_id, main_sem)];
        let cg = build_code_graph(&sem_entries);

        // Both functions should exist
        assert!(cg.function_nodes.contains_key(&(helper_id, "helper_func".to_string())));
        assert!(cg.function_nodes.contains_key(&(main_id, "main".to_string())));

        // Check that there's an import edge from main.py to helpers.py
        let stats = cg.stats();
        assert!(stats.import_edge_count >= 1);
    }

    #[test]
    fn find_import_source_file_returns_none_for_external() {
        let src = "x = 1";
        let (file_id, sem) = parse_and_build_semantics("test.py", src);
        let sem_entries = vec![(file_id, sem)];
        let cg = build_code_graph(&sem_entries);

        // Should return None for an external module
        assert!(find_import_source_file(&cg, "requests").is_none());
        assert!(find_import_source_file(&cg, "fastapi.FastAPI").is_none());
    }

    #[test]
    fn find_import_source_file_finds_local_file() {
        let (file_id, sem) = parse_and_build_semantics("utils.py", "x = 1");
        let sem_entries = vec![(file_id, sem)];
        let cg = build_code_graph(&sem_entries);

        // Should find the local file
        assert!(find_import_source_file(&cg, "utils").is_some());
    }

    // ==================== Express.js Graph Tests ====================

    use crate::parse::typescript::parse_typescript_file;
    use crate::semantics::typescript::model::TsFileSemantics;

    fn parse_typescript_and_build_semantics(path: &str, source: &str) -> (FileId, Arc<SourceSemantics>) {
        let sf = SourceFile {
            path: path.to_string(),
            language: Language::Typescript,
            content: source.to_string(),
        };
        let file_id = FileId(1);
        let parsed = parse_typescript_file(file_id, &sf).expect("parsing should succeed");
        let mut sem = TsFileSemantics::from_parsed(&parsed);
        sem.analyze_frameworks(&parsed)
            .expect("framework analysis should succeed");
        (file_id, Arc::new(SourceSemantics::Typescript(sem)))
    }

    #[test]
    fn build_code_graph_with_express_routes_with_http_metadata() {
        let src = r#"
import express from 'express';

const app = express();

async function getUsers(req, res) {
    res.json([]);
}

function createUser(req, res) {
    res.json({});
}

app.get('/users', getUsers);
app.post('/users', createUser);
"#;
        let (file_id, sem) = parse_typescript_and_build_semantics("app.ts", src);
        let sem_entries = vec![(file_id, sem)];

        let cg = build_code_graph(&sem_entries);

        // Should have: file node + 2 function nodes with HTTP metadata
        let stats = cg.stats();
        assert_eq!(stats.file_count, 1);
        assert_eq!(stats.function_count, 2);

        // Check that getUsers has HTTP metadata
        let get_users_key = (file_id, "getUsers".to_string());
        assert!(cg.function_nodes.contains_key(&get_users_key));
        let get_users_idx = cg.function_nodes[&get_users_key];
        if let GraphNode::Function { http_method, http_path, is_handler, .. } = &cg.graph[get_users_idx] {
            assert_eq!(*http_method, Some("GET".to_string()));
            assert_eq!(*http_path, Some("/users".to_string()));
            assert!(*is_handler);
        } else {
            panic!("Expected Function node for getUsers");
        }

        // Check that createUser has HTTP metadata
        let create_user_key = (file_id, "createUser".to_string());
        assert!(cg.function_nodes.contains_key(&create_user_key));
        let create_user_idx = cg.function_nodes[&create_user_key];
        if let GraphNode::Function { http_method, http_path, is_handler, .. } = &cg.graph[create_user_idx] {
            assert_eq!(*http_method, Some("POST".to_string()));
            assert_eq!(*http_path, Some("/users".to_string()));
            assert!(*is_handler);
        } else {
            panic!("Expected Function node for createUser");
        }
    }

    // ==================== Relative Import Resolution Tests ====================

    #[test]
    fn resolve_relative_import_single_dot_same_dir() {
        // from .utils import foo -> should resolve to utils.py in same directory
        let paths = resolve_relative_import("app.py", ".utils");
        assert!(paths.contains(&"utils.py".to_string()));
        
        let paths = resolve_relative_import("pkg/app.py", ".utils");
        assert!(paths.contains(&"pkg/utils.py".to_string()));
    }

    #[test]
    fn resolve_relative_import_double_dot_parent_dir() {
        // from ..utils import foo -> should resolve to utils.py in parent directory
        let paths = resolve_relative_import("pkg/sub/app.py", "..utils");
        assert!(paths.contains(&"pkg/utils.py".to_string()));
    }

    #[test]
    fn resolve_relative_import_triple_dot() {
        // from ...utils import foo -> should resolve to utils.py two directories up
        let paths = resolve_relative_import("a/b/c/app.py", "...utils");
        assert!(paths.contains(&"a/utils.py".to_string()));
    }

    #[test]
    fn resolve_relative_import_nested_module() {
        // from .models.user import User -> should resolve to models/user.py
        let paths = resolve_relative_import("pkg/app.py", ".models.user");
        assert!(paths.contains(&"pkg/models/user.py".to_string()));
    }

    #[test]
    fn resolve_relative_import_package_init() {
        // from . import models -> should try __init__.py
        let paths = resolve_relative_import("pkg/app.py", ".");
        assert!(paths.contains(&"pkg/__init__.py".to_string()));
    }

    #[test]
    fn resolve_relative_import_root_file() {
        // File in root directory
        let paths = resolve_relative_import("app.py", ".utils");
        assert!(paths.contains(&"utils.py".to_string()));
    }

    #[test]
    fn build_code_graph_with_relative_import() {
        // utils.py defines a function
        let utils_src = r#"
def add(a, b):
    return a + b
"#;
        let (utils_id, utils_sem) = parse_python_with_id("utils.py", utils_src, 1);

        // app.py imports from .utils
        let app_src = r#"
from .utils import add

def main():
    return add(1, 2)
"#;
        let (app_id, app_sem) = parse_python_with_id("app.py", app_src, 2);

        let sem_entries = vec![(utils_id, utils_sem), (app_id, app_sem)];
        let cg = build_code_graph(&sem_entries);

        // Should have import edge from app.py to utils.py
        let stats = cg.stats();
        assert!(stats.import_edge_count >= 1, "Expected at least 1 import edge, got {}", stats.import_edge_count);
        
        // Verify the edge is ImportsFrom with correct items
        let app_file_idx = cg.file_nodes.get(&app_id).expect("app file should exist");
        let mut found_import_edge = false;
        for edge in cg.graph.edges(*app_file_idx) {
            if let GraphEdgeKind::ImportsFrom { items } = edge.weight() {
                if items.contains(&"add".to_string()) {
                    found_import_edge = true;
                }
            }
        }
        assert!(found_import_edge, "Expected ImportsFrom edge with 'add' item");
    }

    #[test]
    fn build_code_graph_with_relative_import_nested() {
        // pkg/utils.py defines a function
        let utils_src = r#"
def helper():
    pass
"#;
        let (utils_id, utils_sem) = parse_python_with_id("pkg/utils.py", utils_src, 1);

        // pkg/sub/app.py imports from ..utils
        let app_src = r#"
from ..utils import helper
"#;
        let (app_id, app_sem) = parse_python_with_id("pkg/sub/app.py", app_src, 2);

        let sem_entries = vec![(utils_id, utils_sem), (app_id, app_sem)];
        let cg = build_code_graph(&sem_entries);

        // Should have import edge from app.py to utils.py
        let stats = cg.stats();
        assert!(stats.import_edge_count >= 1, "Expected at least 1 import edge for ..utils");
    }

    #[test]
    fn find_import_source_file_returns_none_for_relative() {
        let src = "x = 1";
        let (file_id, sem) = parse_and_build_semantics("test.py", src);
        let sem_entries = vec![(file_id, sem)];
        let cg = build_code_graph(&sem_entries);

        // Relative imports can't be resolved without the importing file context
        assert!(find_import_source_file(&cg, ".utils").is_none());
        assert!(find_import_source_file(&cg, "..models").is_none());
    }

    #[test]
    fn cross_file_call_edge_with_relative_import() {
        // This is the exact scenario from the bug report:
        // utils.py defines add(), app.py imports via `from .utils import add` and calls it
        
        // utils.py defines the add function
        let utils_src = r#"
def add(a, b):
    return a + b
"#;
        let (utils_id, utils_sem) = parse_python_with_id("utils.py", utils_src, 1);

        // app.py imports add via relative import and calls it
        let app_src = r#"
from .utils import add

def main():
    result = add(1, 2)
    return result
"#;
        let (app_id, app_sem) = parse_python_with_id("app.py", app_src, 2);

        let sem_entries = vec![(utils_id, utils_sem), (app_id, app_sem)];
        let cg = build_code_graph(&sem_entries);

        // Verify both functions exist
        assert!(cg.function_nodes.contains_key(&(utils_id, "add".to_string())));
        assert!(cg.function_nodes.contains_key(&(app_id, "main".to_string())));

        // Key assertion: There should be a Calls edge from main() to add()
        let stats = cg.stats();
        assert!(stats.calls_edge_count >= 1,
            "Expected at least 1 Calls edge for cross-file call via relative import, got {}",
            stats.calls_edge_count);

        // Verify the specific edge exists: main -> add
        let main_func_idx = cg.function_nodes.get(&(app_id, "main".to_string())).expect("main function should exist");
        let add_func_idx = cg.function_nodes.get(&(utils_id, "add".to_string())).expect("add function should exist");
        
        let mut found_calls_edge = false;
        for edge in cg.graph.edges(*main_func_idx) {
            if matches!(edge.weight(), GraphEdgeKind::Calls) {
                if edge.target() == *add_func_idx {
                    found_calls_edge = true;
                    break;
                }
            }
        }
        assert!(found_calls_edge, "Expected Calls edge from main() to add()");
    }

    #[test]
    fn find_import_source_file_with_context_resolves_relative() {
        // Set up a graph with a utils.py file
        let (utils_id, utils_sem) = parse_python_with_id("utils.py", "x = 1", 1);
        let sem_entries = vec![(utils_id, utils_sem)];
        let cg = build_code_graph(&sem_entries);

        // Without context, relative import should fail
        assert!(find_import_source_file(&cg, ".utils").is_none());

        // With context, relative import should succeed
        let result = find_import_source_file_with_context(&cg, ".utils", "app.py");
        assert!(result.is_some(), "Expected to find utils.py via relative import from app.py");
    }
}
