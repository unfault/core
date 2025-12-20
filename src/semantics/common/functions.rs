//! Common function/method definition abstractions for cross-language analysis.
//!
//! This module provides language-agnostic types for function definitions,
//! enabling shared rule logic for analyzing function signatures, async status, etc.

use serde::{Deserialize, Serialize};

use super::CommonLocation;

/// A function call site within a function body.
///
/// This captures calls made by a function to other functions,
/// enabling call graph construction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// The name of the called function.
    ///
    /// For simple calls like `foo()`, this is `"foo"`.
    /// For method calls like `obj.method()`, this is `"method"`.
    /// For attribute calls like `self.service.process()`, this is `"process"`.
    pub callee: String,

    /// The full callee expression as it appears in code.
    ///
    /// For `self.service.process()`, this is `"self.service.process"`.
    /// For `foo()`, this is `"foo"`.
    pub callee_expr: String,

    /// The receiver/object if this is a method call.
    ///
    /// For `obj.method()`, this is `Some("obj")`.
    /// For `foo()`, this is `None`.
    pub receiver: Option<String>,

    /// Line number (1-based) where the call occurs.
    pub line: u32,

    /// Column number (1-based) where the call starts.
    pub column: u32,
}

impl FunctionCall {
    /// Create a new function call.
    pub fn new(callee: impl Into<String>, callee_expr: impl Into<String>) -> Self {
        Self {
            callee: callee.into(),
            callee_expr: callee_expr.into(),
            receiver: None,
            line: 0,
            column: 0,
        }
    }

    /// Set the receiver for method calls.
    pub fn with_receiver(mut self, receiver: impl Into<String>) -> Self {
        self.receiver = Some(receiver.into());
        self
    }

    /// Set the location.
    pub fn with_location(mut self, line: u32, column: u32) -> Self {
        self.line = line;
        self.column = column;
        self
    }

    /// Check if this is a method call (has a receiver).
    pub fn is_method_call(&self) -> bool {
        self.receiver.is_some()
    }

    /// Get the simple function name (last component of the call chain).
    pub fn function_name(&self) -> &str {
        &self.callee
    }
}

/// Function/method visibility
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    /// Public (exported, pub)
    Public,
    /// Private (unexported, not pub)
    Private,
    /// Protected (Java/C++, Python _prefix convention)
    Protected,
    /// Package-private (Go unexported, Java default)
    Package,
    /// Unknown/not applicable
    Unknown,
}

impl Default for Visibility {
    fn default() -> Self {
        Self::Unknown
    }
}

/// Function kind classification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FunctionKind {
    /// Regular function
    Function,
    /// Instance method
    Method,
    /// Static method
    StaticMethod,
    /// Class method (Python)
    ClassMethod,
    /// Constructor
    Constructor,
    /// Destructor/finalizer
    Destructor,
    /// Lambda/closure
    Lambda,
    /// Generator function
    Generator,
    /// Coroutine/async generator
    AsyncGenerator,
}

impl Default for FunctionKind {
    fn default() -> Self {
        Self::Function
    }
}

/// A language-agnostic function parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionParam {
    /// Parameter name
    pub name: String,
    /// Type annotation (if present)
    pub type_annotation: Option<String>,
    /// Default value (if present)
    pub default_value: Option<String>,
    /// Whether this is a rest/variadic parameter (*args, ...args)
    pub is_variadic: bool,
    /// Whether this is a keyword-only parameter
    pub is_keyword_only: bool,
}

impl FunctionParam {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            type_annotation: None,
            default_value: None,
            is_variadic: false,
            is_keyword_only: false,
        }
    }

    pub fn with_type(mut self, type_ann: impl Into<String>) -> Self {
        self.type_annotation = Some(type_ann.into());
        self
    }

    pub fn with_default(mut self, default: impl Into<String>) -> Self {
        self.default_value = Some(default.into());
        self
    }

    pub fn variadic(mut self) -> Self {
        self.is_variadic = true;
        self
    }

    /// Check if this parameter has a default value
    pub fn has_default(&self) -> bool {
        self.default_value.is_some()
    }

    /// Check if this parameter is typed
    pub fn is_typed(&self) -> bool {
        self.type_annotation.is_some()
    }
}

/// Decorator/annotation on a function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDecorator {
    /// Decorator name (e.g., "staticmethod", "app.get", "Override")
    pub name: String,
    /// Full decorator text including arguments
    pub full_text: String,
    /// Arguments to the decorator
    pub arguments: Vec<String>,
    /// Location of the decorator
    pub location: CommonLocation,
}

impl FunctionDecorator {
    pub fn new(name: impl Into<String>, full_text: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            full_text: full_text.into(),
            arguments: Vec::new(),
            location: CommonLocation {
                file_id: crate::parse::ast::FileId(0),
                line: 0,
                column: 0,
                start_byte: 0,
                end_byte: 0,
            },
        }
    }

    /// Check if this decorator matches a pattern (case-insensitive contains)
    pub fn matches(&self, pattern: &str) -> bool {
        self.name.to_lowercase().contains(&pattern.to_lowercase())
            || self
                .full_text
                .to_lowercase()
                .contains(&pattern.to_lowercase())
    }

    /// Check if this is a route decorator (FastAPI, Flask, Express-like)
    pub fn is_route_decorator(&self) -> bool {
        let route_patterns = ["get", "post", "put", "patch", "delete", "route", "api_view"];
        route_patterns.iter().any(|p| self.matches(p))
    }

    /// Check if this is a retry decorator
    pub fn is_retry_decorator(&self) -> bool {
        let retry_patterns = [
            "retry",
            "backoff",
            "stamina",
            "tenacity",
            "resilience",
            "circuitbreaker",
        ];
        retry_patterns.iter().any(|p| self.matches(p))
    }
}

/// A language-agnostic function definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDef {
    /// Function name
    pub name: String,

    /// Function kind (method, static, constructor, etc.)
    pub kind: FunctionKind,

    /// Visibility modifier
    pub visibility: Visibility,

    /// Whether this is an async function
    pub is_async: bool,

    /// Parameters
    pub params: Vec<FunctionParam>,

    /// Return type annotation (if present)
    pub return_type: Option<String>,

    /// Decorators/annotations
    pub decorators: Vec<FunctionDecorator>,

    /// Class name (if this is a method)
    pub class_name: Option<String>,

    /// Function calls made within this function's body.
    ///
    /// Used for call graph construction. Contains the names/expressions
    /// of functions called from within this function.
    pub calls: Vec<FunctionCall>,

    /// Body length in lines (for complexity heuristics)
    pub body_lines: u32,

    /// Whether this function has error handling (try/catch/except)
    pub has_error_handling: bool,

    /// Whether this function has a docstring/documentation
    pub has_documentation: bool,

    /// Location in source file
    pub location: CommonLocation,

    /// Start byte offset
    pub start_byte: usize,

    /// End byte offset
    pub end_byte: usize,
}

impl FunctionDef {
    /// Check if this is a constructor
    pub fn is_constructor(&self) -> bool {
        self.kind == FunctionKind::Constructor
            || self.name == "__init__"
            || self.name == "new"
            || self.name == "constructor"
    }

    /// Check if this is a public/exported function
    pub fn is_public(&self) -> bool {
        matches!(self.visibility, Visibility::Public | Visibility::Unknown)
            && !self.name.starts_with('_')
    }

    /// Check if this is a test function
    pub fn is_test(&self) -> bool {
        self.name.starts_with("test_")
            || self.name.starts_with("Test")
            || self.decorators.iter().any(|d| d.matches("test"))
    }

    /// Check if this function has a specific decorator
    pub fn has_decorator(&self, pattern: &str) -> bool {
        self.decorators.iter().any(|d| d.matches(pattern))
    }

    /// Check if this is a route handler (HTTP endpoint)
    pub fn is_route_handler(&self) -> bool {
        self.decorators.iter().any(|d| d.is_route_decorator())
    }

    /// Check if this function has retry configured
    pub fn has_retry(&self) -> bool {
        self.decorators.iter().any(|d| d.is_retry_decorator())
    }

    /// Get the number of required parameters (without defaults)
    pub fn required_param_count(&self) -> usize {
        self.params.iter().filter(|p| !p.has_default()).count()
    }

    /// Get the number of typed parameters
    pub fn typed_param_count(&self) -> usize {
        self.params.iter().filter(|p| p.is_typed()).count()
    }

    /// Check if all parameters are typed
    pub fn is_fully_typed(&self) -> bool {
        !self.params.is_empty()
            && self.params.iter().all(|p| p.is_typed())
            && self.return_type.is_some()
    }
}

/// Builder for creating FunctionDef instances
#[derive(Debug, Default)]
pub struct FunctionDefBuilder {
    name: Option<String>,
    kind: FunctionKind,
    visibility: Visibility,
    is_async: bool,
    params: Vec<FunctionParam>,
    return_type: Option<String>,
    decorators: Vec<FunctionDecorator>,
    class_name: Option<String>,
    calls: Vec<FunctionCall>,
    body_lines: u32,
    has_error_handling: bool,
    has_documentation: bool,
    location: Option<CommonLocation>,
    start_byte: usize,
    end_byte: usize,
}

impl FunctionDefBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: Some(name.into()),
            ..Default::default()
        }
    }

    pub fn kind(mut self, kind: FunctionKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn visibility(mut self, visibility: Visibility) -> Self {
        self.visibility = visibility;
        self
    }

    pub fn is_async(mut self, is_async: bool) -> Self {
        self.is_async = is_async;
        self
    }

    pub fn param(mut self, param: FunctionParam) -> Self {
        self.params.push(param);
        self
    }

    pub fn return_type(mut self, return_type: impl Into<String>) -> Self {
        self.return_type = Some(return_type.into());
        self
    }

    pub fn decorator(mut self, decorator: FunctionDecorator) -> Self {
        self.decorators.push(decorator);
        self
    }

    pub fn class_name(mut self, class_name: impl Into<String>) -> Self {
        self.class_name = Some(class_name.into());
        self
    }

    /// Add a function call made within this function.
    pub fn call(mut self, call: FunctionCall) -> Self {
        self.calls.push(call);
        self
    }

    /// Set all function calls made within this function.
    pub fn calls(mut self, calls: Vec<FunctionCall>) -> Self {
        self.calls = calls;
        self
    }

    pub fn body_lines(mut self, lines: u32) -> Self {
        self.body_lines = lines;
        self
    }

    pub fn has_error_handling(mut self, has: bool) -> Self {
        self.has_error_handling = has;
        self
    }

    pub fn has_documentation(mut self, has: bool) -> Self {
        self.has_documentation = has;
        self
    }

    pub fn location(mut self, location: CommonLocation) -> Self {
        self.location = Some(location);
        self
    }

    pub fn byte_range(mut self, start: usize, end: usize) -> Self {
        self.start_byte = start;
        self.end_byte = end;
        self
    }

    pub fn build(self) -> Option<FunctionDef> {
        Some(FunctionDef {
            name: self.name?,
            kind: self.kind,
            visibility: self.visibility,
            is_async: self.is_async,
            params: self.params,
            return_type: self.return_type,
            decorators: self.decorators,
            class_name: self.class_name,
            calls: self.calls,
            body_lines: self.body_lines,
            has_error_handling: self.has_error_handling,
            has_documentation: self.has_documentation,
            location: self.location?,
            start_byte: self.start_byte,
            end_byte: self.end_byte,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;

    fn make_location() -> CommonLocation {
        CommonLocation {
            file_id: FileId(1),
            line: 10,
            column: 5,
            start_byte: 100,
            end_byte: 200,
        }
    }

    #[test]
    fn function_param_builder() {
        let param = FunctionParam::new("user_id")
            .with_type("int")
            .with_default("None");

        assert_eq!(param.name, "user_id");
        assert!(param.has_default());
        assert!(param.is_typed());
    }

    #[test]
    fn function_is_constructor() {
        let init_fn = FunctionDefBuilder::new("__init__")
            .kind(FunctionKind::Method)
            .location(make_location())
            .build()
            .unwrap();
        assert!(init_fn.is_constructor());

        let new_fn = FunctionDefBuilder::new("new")
            .kind(FunctionKind::Constructor)
            .location(make_location())
            .build()
            .unwrap();
        assert!(new_fn.is_constructor());

        let regular_fn = FunctionDefBuilder::new("process")
            .location(make_location())
            .build()
            .unwrap();
        assert!(!regular_fn.is_constructor());
    }

    #[test]
    fn function_is_test() {
        let test_fn = FunctionDefBuilder::new("test_user_creation")
            .location(make_location())
            .build()
            .unwrap();
        assert!(test_fn.is_test());

        let regular_fn = FunctionDefBuilder::new("create_user")
            .location(make_location())
            .build()
            .unwrap();
        assert!(!regular_fn.is_test());
    }

    #[test]
    fn function_decorator_matching() {
        let route_decorator = FunctionDecorator::new("app.get", "@app.get('/users')");
        assert!(route_decorator.is_route_decorator());
        assert!(route_decorator.matches("app.get"));
        assert!(route_decorator.matches("GET"));

        let retry_decorator = FunctionDecorator::new("retry", "@retry(max_attempts=3)");
        assert!(retry_decorator.is_retry_decorator());
    }

    #[test]
    fn function_required_param_count() {
        let func = FunctionDefBuilder::new("process")
            .param(FunctionParam::new("required"))
            .param(FunctionParam::new("optional").with_default("None"))
            .param(FunctionParam::new("also_required"))
            .location(make_location())
            .build()
            .unwrap();

        assert_eq!(func.required_param_count(), 2);
    }

    #[test]
    fn function_is_fully_typed() {
        let typed_fn = FunctionDefBuilder::new("add")
            .param(FunctionParam::new("a").with_type("int"))
            .param(FunctionParam::new("b").with_type("int"))
            .return_type("int")
            .location(make_location())
            .build()
            .unwrap();
        assert!(typed_fn.is_fully_typed());

        let untyped_fn = FunctionDefBuilder::new("add")
            .param(FunctionParam::new("a"))
            .param(FunctionParam::new("b"))
            .location(make_location())
            .build()
            .unwrap();
        assert!(!untyped_fn.is_fully_typed());
    }
}
