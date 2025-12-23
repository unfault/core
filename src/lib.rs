//! # unfault-core
//!
//! Core parsing, semantics extraction, and graph building for unfault.
//!
//! This crate provides language-agnostic code analysis capabilities:
//!
//! - **Parsing**: Tree-sitter based parsing for Python, Go, Rust, TypeScript, etc.
//! - **Semantics**: Extract semantic information (functions, imports, classes, etc.)
//! - **Graph**: Build code dependency graphs with import/call relationships
//! - **Types**: Common types for language identification and source files
//!
//! ## Example
//!
//! ```rust,ignore
//! use unfault_core::parse::python::parse_python_file;
//! use unfault_core::semantics::python::model::PyFileSemantics;
//! use unfault_core::graph::build_code_graph;
//! use unfault_core::types::context::{SourceFile, Language};
//! use unfault_core::parse::ast::FileId;
//!
//! let source = SourceFile {
//!     path: "example.py".to_string(),
//!     language: Language::Python,
//!     content: "import os\ndef hello(): pass".to_string(),
//! };
//!
//! let parsed = parse_python_file(FileId(1), &source).unwrap();
//! let semantics = PyFileSemantics::from_parsed(&parsed);
//! ```

use serde::{Deserialize, Serialize};

pub mod error;
pub mod graph;
pub mod parse;
pub mod semantics;
pub mod types;

// Re-export commonly used types for convenience
pub use graph::{CodeGraph, GraphEdgeKind, GraphNode, GraphStats, build_code_graph};
pub use parse::ast::{FileId, ParsedFile};
pub use semantics::SourceSemantics;
pub use types::context::{Language, SourceFile};

/// Intermediate Representation for client-side parsing.
///
/// This struct contains all the data needed for the server to run rules
/// without needing access to the original source code:
/// - Semantics for each file (imports, functions, classes, calls, etc.)
/// - A code graph with file, function, and import relationships
///
/// The IR can be serialized to JSON and sent to the server API for analysis.
///
/// # Example
///
/// ```rust,ignore
/// use unfault_core::{IntermediateRepresentation, SourceSemantics, CodeGraph};
///
/// // Build IR from parsed files
/// let ir = IntermediateRepresentation {
///     semantics: vec![source_semantics1, source_semantics2],
///     graph: code_graph,
/// };
///
/// // Serialize to JSON for transmission
/// let json = serde_json::to_string(&ir)?;
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntermediateRepresentation {
    /// Per-file semantics containing parsed information about each source file.
    ///
    /// Each entry contains language-specific semantic data including:
    /// - Imports and dependencies
    /// - Function and method definitions
    /// - Class/type definitions
    /// - Call sites and their locations
    /// - Framework-specific information (FastAPI, Express, etc.)
    pub semantics: Vec<SourceSemantics>,

    /// The code graph representing relationships between code elements.
    ///
    /// Contains nodes for files, functions, classes, and external modules,
    /// with edges representing imports, calls, and containment relationships.
    ///
    /// Note: After deserializing, call `graph.rebuild_indexes()` to restore
    /// the quick-lookup HashMaps.
    pub graph: CodeGraph,
}

impl IntermediateRepresentation {
    /// Create a new IntermediateRepresentation from semantics and graph.
    pub fn new(semantics: Vec<SourceSemantics>, graph: CodeGraph) -> Self {
        Self { semantics, graph }
    }

    /// Get the number of files in this IR.
    pub fn file_count(&self) -> usize {
        self.semantics.len()
    }

    /// Rebuild the graph indexes after deserialization.
    ///
    /// This must be called after deserializing an IR to restore
    /// the quick-lookup HashMaps in the CodeGraph.
    pub fn rebuild_graph_indexes(&mut self) {
        self.graph.rebuild_indexes();
    }
}
