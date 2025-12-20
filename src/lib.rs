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
