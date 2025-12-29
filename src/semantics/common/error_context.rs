//! Common abstractions for error handling and propagation analysis.
//!
//! This module provides types for analyzing try/catch/finally patterns,
//! error propagation paths, and error context across different languages.

use serde::{Deserialize, Serialize};

use super::CommonLocation;
use crate::parse::ast::FileId;

/// Type of error handling construct
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorContextType {
    /// Try-catch-finally block
    TryCatch,
    /// Try-except block (Python)
    TryExcept,
    /// Generic except block
    GenericExcept,
    /// Specific exception types
    SpecificExcept(Vec<String>),
    /// Bare except clause
    BareExcept,
    /// Generic catch clause
    GenericCatch,
    /// Specific catch types
    SpecificCatch(Vec<String>),
    /// Bare catch clause
    BareCatch,
    /// Finally block
    Finally,
    /// Do-catch (Swift)
    DoCatch,
    /// Unwrap call (Rust)
    Unwrap,
    /// Expect call (Rust)
    Expect,
    /// Panic/throw handling
    Panic,
    /// Defer-recover pattern (Go)
    DeferRecover,
    /// Error boundary (React, etc.)
    ErrorBoundary,
    /// Other error handling construct
    Other(String),
}

/// Error context information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorContext {
    /// The type of error handling
    pub context_type: ErrorContextType,
    /// Whether the error handler has logging
    pub has_logging: bool,
    /// Whether the error is being re-raised
    pub has_reraise: bool,
    /// Whether the error is being propagated
    pub has_propagation: bool,
    /// Whether errors are being swallowed (empty catch, except: pass)
    pub swallows_error: bool,
    /// Whether this adds error context/logging
    pub adds_context: bool,
    /// The error variable name if any
    pub error_variable: Option<String>,
    /// Location in source file
    pub location: CommonLocation,
    /// Name of enclosing function
    pub enclosing_function: Option<String>,
    /// Start byte offset
    pub start_byte: usize,
    /// End byte offset
    pub end_byte: usize,
}

impl ErrorContext {
    /// Create a new error context
    pub fn new(context_type: ErrorContextType) -> Self {
        Self {
            context_type,
            has_logging: false,
            has_reraise: false,
            has_propagation: false,
            swallows_error: false,
            adds_context: false,
            error_variable: None,
            location: CommonLocation {
                file_id: FileId(0),
                line: 1,
                column: 1,
                start_byte: 0,
                end_byte: 0,
            },
            enclosing_function: None,
            start_byte: 0,
            end_byte: 0,
        }
    }

    /// Mark as swallowing error
    pub fn swallowing_error(mut self, swallows: bool) -> Self {
        self.swallows_error = swallows;
        self
    }

    /// Mark as adding context
    pub fn adding_context(mut self, adds: bool) -> Self {
        self.adds_context = adds;
        self
    }

    /// Mark as having logging
    pub fn with_logging(mut self, has_logging: bool) -> Self {
        self.has_logging = has_logging;
        self
    }

    /// Mark as re-raised
    pub fn with_reraise(mut self, has_reraise: bool) -> Self {
        self.has_reraise = has_reraise;
        self
    }

    /// Mark as propagated
    pub fn with_propagation(mut self, has_propagation: bool) -> Self {
        self.has_propagation = has_propagation;
        self
    }

    /// Set error variable
    pub fn with_error_variable(mut self, var: impl Into<String>) -> Self {
        self.error_variable = Some(var.into());
        self
    }

    /// Set enclosing function
    pub fn with_enclosing_function(mut self, func: impl Into<String>) -> Self {
        self.enclosing_function = Some(func.into());
        self
    }

    /// Set location
    pub fn with_location(mut self, location: CommonLocation, start_byte: usize, end_byte: usize) -> Self {
        self.location = location;
        self.start_byte = start_byte;
        self.end_byte = end_byte;
        self
    }
}

/// Error handling summary for a file
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ErrorSummary {
    /// All error contexts found
    pub contexts: Vec<ErrorContext>,
    /// Functions with error handling
    pub functions_with_handling: Vec<String>,
    /// Functions that swallow errors
    pub functions_swallowing: Vec<String>,
    /// Functions adding context
    pub functions_adding_context: Vec<String>,
    /// Nested error handlers
    pub nested_count: usize,
    /// Silent error paths
    pub silent_paths: usize,
}

impl ErrorSummary {
    /// Create empty summary
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an error context
    pub fn add_context(&mut self, context: ErrorContext) {
        if let Some(ref func) = context.enclosing_function {
            if context.swallows_error {
                self.functions_swallowing.push(func.clone());
            }
            if context.adds_context {
                self.functions_adding_context.push(func.clone());
            }
            if !self.functions_with_handling.contains(func) {
                self.functions_with_handling.push(func.clone());
            }
        }
        if context.swallows_error {
            self.silent_paths += 1;
        }
        self.contexts.push(context);
    }

    /// Get contexts that swallow errors
    pub fn swallowing_errors(&self) -> Vec<&ErrorContext> {
        self.contexts.iter().filter(|c| c.swallows_error).collect()
    }

    /// Get contexts that add context
    pub fn adding_context(&self) -> Vec<&ErrorContext> {
        self.contexts.iter().filter(|c| c.adds_context).collect()
    }

    /// Count total error handling constructs
    pub fn count(&self) -> usize {
        self.contexts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_context_new() {
        let ctx = ErrorContext::new(ErrorContextType::TryCatch);

        assert!(matches!(ctx.context_type, ErrorContextType::TryCatch));
        assert!(!ctx.swallows_error);
        assert!(!ctx.adds_context);
    }

    #[test]
    fn error_context_swallowing() {
        let ctx = ErrorContext::new(ErrorContextType::BareCatch)
            .swallowing_error(true);

        assert!(ctx.swallows_error);
    }

    #[test]
    fn error_context_adding_context() {
        let ctx = ErrorContext::new(ErrorContextType::TryCatch)
            .adding_context(true)
            .with_logging(true);

        assert!(ctx.adds_context);
        assert!(ctx.has_logging);
    }

    #[test]
    fn error_context_with_function() {
        let ctx = ErrorContext::new(ErrorContextType::SpecificCatch(vec!["IOException".to_string()]))
            .with_error_variable("e")
            .with_enclosing_function("readFile");

        assert_eq!(ctx.error_variable, Some("e".to_string()));
        assert_eq!(ctx.enclosing_function, Some("readFile".to_string()));
    }
}
