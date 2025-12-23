//! Common abstractions for function calls across languages.

use serde::{Deserialize, Serialize};

use super::CommonLocation;

/// A function call that can be resolved to a target function in the graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// The callee expression as written in code (e.g., "foo", "self.bar", "module.func").
    pub callee_expr: String,

    /// Parsed components of the callee (e.g., ["module", "func"] or ["self", "bar"]).
    pub callee_parts: Vec<String>,

    /// Simple name of the enclosing caller function (e.g., "my_method").
    pub caller_function: String,

    /// Fully qualified name of the caller (e.g., "MyClass.my_method").
    pub caller_qualified_name: String,

    /// Source location of the call.
    pub location: CommonLocation,

    /// Whether this is a method call on 'self' or 'this'.
    pub is_self_call: bool,

    /// Whether this call is on an imported name or module.
    pub is_import_call: bool,

    /// If is_import_call, the alias used (e.g., 'u' in 'import utils as u; u.helper()').
    pub import_alias: Option<String>,
}
