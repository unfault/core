//! Common semantic abstractions for cross-language analysis.
//!
//! This module provides language-agnostic traits and types that can be
//! implemented by each language's semantic model, enabling shared rule logic.

pub mod async_ops;
pub mod calls;
pub mod db;
pub mod frameworks;
pub mod functions;
pub mod http;
pub mod imports;

use crate::parse::ast::{AstLocation, FileId};
use crate::types::context::Language;

/// Language-agnostic semantic information for a source file.
///
/// This trait provides a common interface for accessing semantic information
/// across different programming languages, enabling cross-language rule implementations.
///
/// **Design Note**: This trait returns owned `Vec<T>` to allow language-specific
/// implementations to convert their internal representations to common types.
/// For performance-critical paths, implementations may cache these conversions.
pub trait CommonSemantics: Send + Sync {
    /// Get the file ID
    fn file_id(&self) -> FileId;

    /// Get the file path
    fn file_path(&self) -> &str;

    /// Get the language of this file
    fn language(&self) -> Language;

    /// Get HTTP client calls in this file
    fn http_calls(&self) -> Vec<http::HttpCall>;

    /// Get database operations in this file
    fn db_operations(&self) -> Vec<db::DbOperation>;

    /// Get async/concurrent operations in this file
    fn async_operations(&self) -> Vec<async_ops::AsyncOperation>;

    /// Get imports/dependencies in this file
    fn imports(&self) -> Vec<imports::Import>;

    /// Get function/method definitions in this file
    fn functions(&self) -> Vec<functions::FunctionDef>;

    /// Check if a specific import exists by module path
    fn has_import(&self, module: &str) -> bool {
        self.imports().iter().any(|i| i.matches_module(module))
    }

    /// Check if any import matches a pattern
    fn has_import_matching(&self, pattern: &str) -> bool {
        self.imports().iter().any(|i| {
            i.module_path.contains(pattern)
                || i.items.iter().any(|item| item.name.contains(pattern))
        })
    }

    /// Find a function by name
    fn find_function(&self, name: &str) -> Option<functions::FunctionDef> {
        self.functions().into_iter().find(|f| f.name == name)
    }

    /// Get HTTP calls without timeout
    fn http_calls_without_timeout(&self) -> Vec<http::HttpCall> {
        self.http_calls()
            .into_iter()
            .filter(|c| !c.has_timeout)
            .collect()
    }

    /// Get HTTP calls without retry logic
    fn http_calls_without_retry(&self) -> Vec<http::HttpCall> {
        self.http_calls()
            .into_iter()
            .filter(|c| c.retry_mechanism.is_none())
            .collect()
    }

    /// Get database operations without timeout
    fn db_operations_without_timeout(&self) -> Vec<db::DbOperation> {
        self.db_operations()
            .into_iter()
            .filter(|op| !op.has_timeout)
            .collect()
    }

    /// Get async operations without error handling
    fn async_operations_without_error_handling(&self) -> Vec<async_ops::AsyncOperation> {
        self.async_operations()
            .into_iter()
            .filter(|op| !op.has_error_handling)
            .collect()
    }
}

/// Location information that can be converted from language-specific locations
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CommonLocation {
    pub file_id: FileId,
    pub line: u32,
    pub column: u32,
    pub start_byte: usize,
    pub end_byte: usize,
}

impl From<&AstLocation> for CommonLocation {
    fn from(loc: &AstLocation) -> Self {
        Self {
            file_id: loc.file_id,
            line: loc.range.start_line + 1,
            column: loc.range.start_col + 1,
            start_byte: 0, // Would need byte info from node
            end_byte: 0,
        }
    }
}

/// A common call site structure that can represent calls across languages
#[derive(Debug, Clone)]
pub struct CommonCallSite {
    /// The full callee expression (e.g., "requests.get", "http.Get")
    pub callee: String,
    /// The method/function name being called
    pub method_name: String,
    /// Full text of the call expression
    pub call_text: String,
    /// Location in source
    pub location: CommonLocation,
    /// Name of enclosing function
    pub enclosing_function: Option<String>,
    /// Whether inside an async context
    pub in_async_context: bool,
    /// Whether inside a loop
    pub in_loop: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_location_from_ast_location() {
        use crate::parse::ast::TextRange;
        let ast_loc = AstLocation {
            file_id: FileId(1),
            range: TextRange {
                start_line: 10,
                start_col: 5,
                end_line: 10,
                end_col: 20,
            },
        };
        let common_loc = CommonLocation::from(&ast_loc);
        assert_eq!(common_loc.file_id, FileId(1));
        assert_eq!(common_loc.line, 11); // 1-based
        assert_eq!(common_loc.column, 6); // 1-based
    }
}
