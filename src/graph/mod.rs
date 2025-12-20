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

use crate::parse::ast::FileId;
use crate::semantics::SourceSemantics;
use crate::semantics::common::CommonSemantics;
use crate::semantics::python::fastapi::FastApiFileSummary;
use crate::semantics::python::model::PyFileSemantics;
use crate::types::context::Language;

/// Category of external modules for better organization
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
#[derive(Debug, Clone)]
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

    /// Check if this is a file node
    pub fn is_file(&self) -> bool {
        matches!(self, GraphNode::File { .. })
    }
}

/// Edge kinds between nodes.
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug)]
pub struct CodeGraph {
    pub graph: DiGraph<GraphNode, GraphEdgeKind>,
    /// Quick lookup: file_id -> node index for the file node.
    pub file_nodes: HashMap<FileId, NodeIndex>,
    /// Quick lookup: file path -> node index for the file node.
    pub path_to_file: HashMap<String, NodeIndex>,
    /// Quick lookup: external module name -> node index
    pub external_modules: HashMap<String, NodeIndex>,
    /// Quick lookup: (file_id, function_name) -> node index
    pub function_nodes: HashMap<(FileId, String), NodeIndex>,
    /// Quick lookup: (file_id, class_name) -> node index
    pub class_nodes: HashMap<(FileId, String), NodeIndex>,
}

impl CodeGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            file_nodes: HashMap::new(),
            path_to_file: HashMap::new(),
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
    pub fn find_file_by_path(&self, path: &str) -> Option<NodeIndex> {
        // Try exact match first
        if let Some(&idx) = self.path_to_file.get(path) {
            return Some(idx);
        }

        // Try suffix matching for relative imports
        for (file_path, &idx) in &self.path_to_file {
            if file_path.ends_with(path) {
                return Some(idx);
            }
            // Handle module-style paths (e.g., "auth.middleware" -> "auth/middleware.py")
            let module_path = path.replace('.', "/");
            if file_path.ends_with(&format!("{}.py", module_path))
                || file_path.ends_with(&format!("{}/", module_path))
                || file_path.ends_with(&format!("{}.ts", module_path))
                || file_path.ends_with(&format!("{}.go", module_path))
                || file_path.ends_with(&format!("{}.rs", module_path))
            {
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
}

impl Default for CodeGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the code graph
#[derive(Debug, Clone)]
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

    // First pass: create file nodes and collect path mappings
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
        cg.path_to_file.insert(path, node_index);
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
            SourceSemantics::Go(_go) => {
                // TODO: Add Go framework-specific nodes (Gin, Echo, Chi, etc.)
            }
            SourceSemantics::Rust(_rs) => {
                // TODO: Add Rust framework-specific nodes (Actix, Axum, Rocket, etc.)
            }
            SourceSemantics::Typescript(_ts) => {
                // TODO: Add TypeScript framework-specific nodes (Express, NestJS, etc.)
            }
        }
    }

    // Third pass: add Calls edges between functions
    // For now we only resolve intra-file calls (callee within the same file)
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
                // TODO: Cross-file call resolution would require import analysis
            }
        }
    }

    cg
}

/// Add import edges from a file to other files or external modules
fn add_import_edges(
    cg: &mut CodeGraph,
    file_node: NodeIndex,
    _file_id: FileId,
    sem: &Arc<SourceSemantics>,
) {
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
        let module_as_file = import.module_path.replace('.', "/");
        let possible_paths = [
            format!("{}.py", module_as_file),
            format!("{}/__init__.py", module_as_file),
            module_as_file.clone(),
        ];

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

/// Add function nodes from a file
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

    for func in functions {
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
        });

        let func_b = cg.graph.add_node(GraphNode::Function {
            file_id,
            name: "func_b".to_string(),
            qualified_name: "func_b".to_string(),
            is_async: false,
            is_handler: false,
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
}
