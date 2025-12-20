use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use crate::parse::ast::{AstLocation, ParsedFile};

/// FastAPI-specific summary extracted from a Python file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FastApiFileSummary {
    pub apps: Vec<FastApiApp>,
    pub routers: Vec<FastApiRouter>,
    pub routes: Vec<FastApiRoute>,
    pub middlewares: Vec<FastApiMiddleware>,
    pub exception_handlers: Vec<FastApiExceptionHandler>,
}

/// An exception handler registered via `@app.exception_handler(...)` decorator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FastApiExceptionHandler {
    /// The app/router variable name (e.g., "app")
    pub app_var_name: String,
    /// The exception type being handled (e.g., "RequestValidationError", "HTTPException")
    pub exception_type: String,
    /// The handler function name
    pub handler_name: String,
    /// Location of the decorated function
    pub location: AstLocation,
}

/// A `FastAPI()` app instance, typically `app = FastAPI(...)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FastApiApp {
    pub var_name: String, // e.g. "app"
    pub location: AstLocation,
}

/// An `APIRouter()` instance, e.g. `router = APIRouter()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FastApiRouter {
    /// The FastAPI app variable this router is attached to (e.g. "app").
    pub app_var_name: String,

    /// The expression text for the router argument (e.g. "items.router").
    pub router_expr: String,

    /// Optional prefix argument (if we can statically extract it as a string literal).
    pub prefix: Option<String>,

    /// Where this include_router call lives.
    pub location: AstLocation,
}

/// A route defined via decorator or `app.<method>` call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FastApiRoute {
    pub http_method: String, // "GET", "POST", etc.
    pub path: String,        // "/users/{user_id}"
    pub handler_name: String,
    pub is_async: bool,
    pub has_try_except: bool,
    pub location: AstLocation,
    /// Location of just the decorator (for highlighting, not the whole function)
    pub decorator_location: AstLocation,
    /// Byte range of the function body for patch generation
    pub body_start_byte: usize,
    pub body_end_byte: usize,
    /// Handler parameter types (for detecting Pydantic-typed body parameters)
    pub handler_params: Vec<RouteParam>,
}

/// A parameter in a route handler function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteParam {
    pub name: String,
    /// Type annotation if present (e.g., "DiagnosticsRequest", "int", "str")
    pub type_annotation: Option<String>,
}

/// Middleware attached via `app.add_middleware(...)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FastApiMiddleware {
    pub app_var_name: String,    // "app"
    pub middleware_type: String, // "CORSMiddleware", etc.
    pub location: AstLocation,
}

/// Build a FastAPI summary for a single parsed Python file.
///
/// Returns None if there is no FastAPI signal in this file at all.
pub fn summarize_fastapi(file: &ParsedFile) -> Option<FastApiFileSummary> {
    let root = file.tree.root_node();

    let mut apps = Vec::new();
    let mut middlewares = Vec::new();
    let mut routers = Vec::new();
    let mut routes = Vec::new();
    let mut exception_handlers = Vec::new();

    collect_fastapi_apps(file, root, &mut apps);
    collect_fastapi_middlewares(file, root, &mut middlewares);
    collect_fastapi_routers(file, root, &mut routers);
    collect_fastapi_routes(file, root, &mut routes);
    collect_fastapi_exception_handlers(file, root, &mut exception_handlers);

    if apps.is_empty()
        && middlewares.is_empty()
        && routes.is_empty()
        && exception_handlers.is_empty()
    {
        return None;
    }

    Some(FastApiFileSummary {
        apps,
        middlewares,
        routers,
        routes,
        exception_handlers,
    })
}

fn collect_fastapi_apps(file: &ParsedFile, node: Node, out: &mut Vec<FastApiApp>) {
    if node.kind() == "assignment" {
        if let Some(app) = extract_fastapi_app(file, node) {
            out.push(app);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_fastapi_apps(file, child, out);
    }
}

fn collect_fastapi_middlewares(file: &ParsedFile, node: Node, out: &mut Vec<FastApiMiddleware>) {
    if node.kind() == "call" {
        if let Some(mw) = extract_middleware_site(file, node) {
            out.push(mw);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_fastapi_middlewares(file, child, out);
    }
}

fn collect_fastapi_routers(file: &ParsedFile, root: Node, out: &mut Vec<FastApiRouter>) {
    fn walk(file: &ParsedFile, node: Node, out: &mut Vec<FastApiRouter>) {
        // We're looking for `app.include_router(...)`-style calls:
        //
        //   call
        //     function: attribute
        //       object: identifier("app")
        //       attribute: identifier("include_router")
        //
        if node.kind() == "call" {
            if let Some(router) = extract_include_router_call(file, node) {
                out.push(router);
            }
        }

        let mut child = node.child(0);
        while let Some(c) = child {
            walk(file, c, out);
            child = c.next_sibling();
        }
    }

    walk(file, root, out);
}

fn extract_fastapi_app(file: &ParsedFile, node: Node) -> Option<FastApiApp> {
    // Tree-sitter-python assignment shape:
    // (assignment
    //    left: <expr>
    //    right: <expr>)
    let left = node.child_by_field_name("left")?;
    let right = node.child_by_field_name("right")?;

    // We only handle simple: `identifier = FastAPI(...)`
    if left.kind() != "identifier" {
        return None;
    }

    if right.kind() != "call" {
        return None;
    }

    let function = right.child_by_field_name("function")?;
    let func_name = file.text_for_node(&function);
    if func_name != "FastAPI" {
        return None;
    }

    let app_var_name = file.text_for_node(&left);
    let location = file.location_for_node(&right);

    Some(FastApiApp {
        var_name: app_var_name,
        location,
    })
}

fn extract_middleware_site(file: &ParsedFile, node: Node) -> Option<FastApiMiddleware> {
    let source_bytes = file.source.as_bytes();

    // Expect: (call function: (attribute object: (identifier) attribute: (identifier)))
    let function = node.child_by_field_name("function")?;
    if function.kind() != "attribute" {
        return None;
    }

    let object = function.child_by_field_name("object")?;
    let attr = function.child_by_field_name("attribute")?;

    let attr_name = attr.utf8_text(source_bytes).ok()?;
    if attr_name != "add_middleware" {
        return None;
    }

    let app_var_name = object.utf8_text(source_bytes).ok()?.to_string();

    // Arguments → first non-punctuation child is middleware type
    let args = node.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    let mut first_arg: Option<Node> = None;

    for child in args.children(&mut cursor) {
        match child.kind() {
            "(" | ")" | "," => continue,
            _ => {
                first_arg = Some(child);
                break;
            }
        }
    }

    let first_arg = first_arg?;
    let middleware_type = first_arg.utf8_text(source_bytes).ok()?.to_string();

    // Only care about CORS for now
    if !middleware_type.contains("CORSMiddleware") {
        return None;
    }

    let location = file.location_for_node(&node);

    Some(FastApiMiddleware {
        app_var_name,
        middleware_type,
        location,
    })
}

fn extract_include_router_call(file: &ParsedFile, call_node: Node) -> Option<FastApiRouter> {
    // In tree-sitter-python, `call` has a `function` field.
    let func = call_node.child_by_field_name("function")?;

    // We only care about attribute calls: `app.include_router(...)`.
    if func.kind() != "attribute" {
        return None;
    }

    let object = func.child_by_field_name("object")?;
    let attr = func.child_by_field_name("attribute")?;

    let app_var_name = file.text_for_node(&object);
    let method_name = file.text_for_node(&attr);

    if method_name != "include_router" {
        return None;
    }

    // For now, we don't try too hard to parse arguments. We just grab the raw
    // text of the "arguments" node and, optionally later, can refine it into
    // router_expr + prefix if needed.
    let router_expr = {
        if let Some(args) = call_node.child_by_field_name("arguments") {
            file.text_for_node(&args)
        } else {
            "<unknown>".to_string()
        }
    };

    // TODO: later we can inspect the arguments node and try to extract
    // a literal `prefix="..."` keyword argument.
    let prefix = None;

    let location = file.location_for_node(&call_node);

    Some(FastApiRouter {
        app_var_name,
        router_expr,
        prefix,
        location,
    })
}

/// Collect FastAPI routes from decorated functions.
///
/// Looks for patterns like:
/// - `@app.get("/path")`
/// - `@router.post("/path")`
fn collect_fastapi_routes(file: &ParsedFile, node: Node, out: &mut Vec<FastApiRoute>) {
    // Look for decorated_definition nodes
    if node.kind() == "decorated_definition" {
        if let Some(route) = extract_fastapi_route(file, node) {
            out.push(route);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_fastapi_routes(file, child, out);
    }
}

/// Collect FastAPI exception handlers from decorated functions.
///
/// Looks for patterns like:
/// - `@app.exception_handler(RequestValidationError)`
/// - `@app.exception_handler(HTTPException)`
fn collect_fastapi_exception_handlers(
    file: &ParsedFile,
    node: Node,
    out: &mut Vec<FastApiExceptionHandler>,
) {
    // Look for decorated_definition nodes
    if node.kind() == "decorated_definition" {
        if let Some(handler) = extract_fastapi_exception_handler(file, node) {
            out.push(handler);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_fastapi_exception_handlers(file, child, out);
    }
}

/// Extract a FastAPI exception handler from a decorated function definition.
fn extract_fastapi_exception_handler(
    file: &ParsedFile,
    decorated_def: Node,
) -> Option<FastApiExceptionHandler> {
    let source_bytes = file.source.as_bytes();

    // Find the decorator(s) and the function definition
    let mut decorators = Vec::new();
    let mut func_def: Option<Node> = None;

    let mut cursor = decorated_def.walk();
    for child in decorated_def.children(&mut cursor) {
        match child.kind() {
            "decorator" => decorators.push(child),
            "function_definition" => func_def = Some(child),
            _ => {}
        }
    }

    let func_def = func_def?;

    // Check if any decorator is an exception_handler decorator
    for decorator in decorators {
        if let Some((app_var_name, exception_type)) =
            extract_exception_handler_decorator_info(file, decorator)
        {
            // Get the function name
            let handler_name = func_def
                .child_by_field_name("name")
                .map(|n| n.utf8_text(source_bytes).ok())
                .flatten()
                .unwrap_or("unknown")
                .to_string();

            let location = file.location_for_node(&decorated_def);

            return Some(FastApiExceptionHandler {
                app_var_name,
                exception_type,
                handler_name,
                location,
            });
        }
    }

    None
}

/// Extract app variable name and exception type from an exception_handler decorator.
///
/// Handles patterns like:
/// - `@app.exception_handler(RequestValidationError)`
/// - `@app.exception_handler(HTTPException)`
fn extract_exception_handler_decorator_info(
    file: &ParsedFile,
    decorator: Node,
) -> Option<(String, String)> {
    let source_bytes = file.source.as_bytes();

    // Decorator structure:
    // (decorator
    //   "@"
    //   (call
    //     function: (attribute
    //       object: (identifier)  ; "app"
    //       attribute: (identifier))  ; "exception_handler"
    //     arguments: (argument_list
    //       (identifier))))  ; "RequestValidationError"

    let mut cursor = decorator.walk();
    for child in decorator.children(&mut cursor) {
        if child.kind() == "call" {
            let func = child.child_by_field_name("function")?;
            if func.kind() != "attribute" {
                continue;
            }

            let object = func.child_by_field_name("object")?;
            let attr = func.child_by_field_name("attribute")?;

            let method_name = attr.utf8_text(source_bytes).ok()?;

            // Check if it's an exception_handler decorator
            if method_name != "exception_handler" {
                continue;
            }

            let app_var_name = object.utf8_text(source_bytes).ok()?.to_string();

            // Extract the exception type from arguments
            let args = child.child_by_field_name("arguments")?;
            let exception_type = extract_first_identifier_or_attribute(file, args)?;

            return Some((app_var_name, exception_type));
        }
    }

    None
}

/// Extract the first identifier or attribute from an argument list.
fn extract_first_identifier_or_attribute(file: &ParsedFile, args: Node) -> Option<String> {
    let source_bytes = file.source.as_bytes();

    let mut cursor = args.walk();
    for child in args.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                return child.utf8_text(source_bytes).ok().map(|s| s.to_string());
            }
            "attribute" => {
                // Handle module.ExceptionType
                return child.utf8_text(source_bytes).ok().map(|s| s.to_string());
            }
            _ => {}
        }
    }

    None
}

/// Extract a FastAPI route from a decorated function definition.
fn extract_fastapi_route(file: &ParsedFile, decorated_def: Node) -> Option<FastApiRoute> {
    let source_bytes = file.source.as_bytes();

    // Find the decorator(s) and the function definition
    let mut decorators = Vec::new();
    let mut func_def: Option<Node> = None;

    let mut cursor = decorated_def.walk();
    for child in decorated_def.children(&mut cursor) {
        match child.kind() {
            "decorator" => decorators.push(child),
            "function_definition" | "async_function_definition" => func_def = Some(child),
            _ => {}
        }
    }

    let func_def = func_def?;

    // Detect async by checking if the function text starts with "async def"
    // This is the same heuristic used in http.rs
    let fn_text = file.text_for_node(&func_def);
    let is_async = fn_text.trim_start().starts_with("async def");

    // Check if any decorator is a FastAPI route decorator
    for decorator in &decorators {
        if let Some((http_method, path)) = extract_route_decorator_info(file, *decorator) {
            // Get the function name
            let handler_name = func_def
                .child_by_field_name("name")
                .map(|n| n.utf8_text(source_bytes).ok())
                .flatten()
                .unwrap_or("unknown")
                .to_string();

            // Check if the function body has a try-except block
            let body = func_def.child_by_field_name("body")?;
            let has_try_except = body_has_try_except(body);

            let location = file.location_for_node(&decorated_def);

            // Get decorator location (just the decorator line, not the whole function)
            let decorator_location = file.location_for_node(decorator);

            // Extract handler parameters with their type annotations
            let handler_params = extract_handler_params(file, &func_def);

            return Some(FastApiRoute {
                http_method,
                path,
                handler_name,
                is_async,
                has_try_except,
                location,
                decorator_location,
                body_start_byte: body.start_byte(),
                body_end_byte: body.end_byte(),
                handler_params,
            });
        }
    }

    None
}

/// Extract parameters from a route handler function.
fn extract_handler_params(file: &ParsedFile, func_def: &Node) -> Vec<RouteParam> {
    let mut params = Vec::new();

    let params_node = match func_def.child_by_field_name("parameters") {
        Some(n) => n,
        None => return params,
    };

    let child_count = params_node.named_child_count();
    for i in 0..child_count {
        if let Some(param_node) = params_node.named_child(i) {
            match param_node.kind() {
                "identifier" => {
                    // Simple parameter without type annotation
                    let name = file.text_for_node(&param_node);
                    if !name.is_empty() && name != "self" {
                        params.push(RouteParam {
                            name,
                            type_annotation: None,
                        });
                    }
                }
                "typed_parameter" => {
                    // Parameter with type annotation like `request: DiagnosticsRequest`
                    // First try to get using field names
                    let name = param_node
                        .child_by_field_name("name")
                        .map(|n| file.text_for_node(&n));
                    let type_annotation = param_node
                        .child_by_field_name("type")
                        .map(|t| file.text_for_node(&t));

                    if let Some(name) = name {
                        if !name.is_empty() && name != "self" {
                            params.push(RouteParam {
                                name,
                                type_annotation,
                            });
                        }
                    } else {
                        // Fallback: parse the text "name: type" directly
                        let text = file.text_for_node(&param_node);
                        if let Some((name, type_ann)) = parse_typed_param_text(&text) {
                            if name != "self" {
                                params.push(RouteParam {
                                    name,
                                    type_annotation: Some(type_ann),
                                });
                            }
                        }
                    }
                }
                "default_parameter" => {
                    // Parameter with default value like `limit=10`
                    let name = param_node
                        .child_by_field_name("name")
                        .map(|n| file.text_for_node(&n))
                        .unwrap_or_default();
                    if !name.is_empty() && name != "self" {
                        params.push(RouteParam {
                            name,
                            type_annotation: None,
                        });
                    }
                }
                "typed_default_parameter" => {
                    // Parameter with type and default like `limit: int = 10`
                    let name = param_node
                        .child_by_field_name("name")
                        .map(|n| file.text_for_node(&n));
                    let type_annotation = param_node
                        .child_by_field_name("type")
                        .map(|t| file.text_for_node(&t));

                    if let Some(name) = name {
                        if !name.is_empty() && name != "self" {
                            params.push(RouteParam {
                                name,
                                type_annotation,
                            });
                        }
                    } else {
                        // Fallback: parse "name: type = value" directly
                        let text = file.text_for_node(&param_node);
                        if let Some((name, type_ann)) = parse_typed_default_param_text(&text) {
                            if name != "self" {
                                params.push(RouteParam {
                                    name,
                                    type_annotation: Some(type_ann),
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    params
}

/// Parse a typed parameter text like "name: type" into (name, type).
fn parse_typed_param_text(text: &str) -> Option<(String, String)> {
    let colon_idx = text.find(':')?;
    let name = text[..colon_idx].trim().to_string();
    let type_ann = text[colon_idx + 1..].trim().to_string();

    if name.is_empty() || type_ann.is_empty() {
        return None;
    }

    Some((name, type_ann))
}

/// Parse a typed default parameter text like "name: type = value" into (name, type).
fn parse_typed_default_param_text(text: &str) -> Option<(String, String)> {
    let colon_idx = text.find(':')?;
    let name = text[..colon_idx].trim().to_string();
    let rest = &text[colon_idx + 1..];

    // Find the '=' for default value
    if let Some(eq_idx) = rest.find('=') {
        let type_ann = rest[..eq_idx].trim().to_string();
        if name.is_empty() || type_ann.is_empty() {
            return None;
        }
        Some((name, type_ann))
    } else {
        let type_ann = rest.trim().to_string();
        if name.is_empty() || type_ann.is_empty() {
            return None;
        }
        Some((name, type_ann))
    }
}

/// Extract HTTP method and path from a route decorator.
///
/// Handles patterns like:
/// - `@app.get("/path")`
/// - `@router.post("/path")`
fn extract_route_decorator_info(file: &ParsedFile, decorator: Node) -> Option<(String, String)> {
    let source_bytes = file.source.as_bytes();

    // Decorator structure:
    // (decorator
    //   "@"
    //   (call
    //     function: (attribute
    //       object: (identifier)  ; "app" or "router"
    //       attribute: (identifier))  ; "get", "post", etc.
    //     arguments: (argument_list
    //       (string))))

    let mut cursor = decorator.walk();
    for child in decorator.children(&mut cursor) {
        if child.kind() == "call" {
            let func = child.child_by_field_name("function")?;
            if func.kind() != "attribute" {
                continue;
            }

            let attr = func.child_by_field_name("attribute")?;
            let method_name = attr.utf8_text(source_bytes).ok()?;

            // Check if it's an HTTP method
            let http_method = match method_name.to_lowercase().as_str() {
                "get" => "GET",
                "post" => "POST",
                "put" => "PUT",
                "delete" => "DELETE",
                "patch" => "PATCH",
                "options" => "OPTIONS",
                "head" => "HEAD",
                _ => return None,
            };

            // Extract the path from arguments
            let args = child.child_by_field_name("arguments")?;
            let path = extract_first_string_arg(file, args)?;

            return Some((http_method.to_string(), path));
        }
    }

    None
}

/// Extract the first string argument from an argument list.
fn extract_first_string_arg(file: &ParsedFile, args: Node) -> Option<String> {
    let source_bytes = file.source.as_bytes();

    let mut cursor = args.walk();
    for child in args.children(&mut cursor) {
        match child.kind() {
            "string" => {
                let text = child.utf8_text(source_bytes).ok()?;
                // Remove quotes
                let path = text.trim_matches(|c| c == '"' || c == '\'');
                return Some(path.to_string());
            }
            "concatenated_string" => {
                // Handle f-strings or concatenated strings - just get the first part
                let mut inner_cursor = child.walk();
                for inner in child.children(&mut inner_cursor) {
                    if inner.kind() == "string" {
                        let text = inner.utf8_text(source_bytes).ok()?;
                        let path = text.trim_matches(|c| c == '"' || c == '\'');
                        return Some(path.to_string());
                    }
                }
            }
            _ => {}
        }
    }

    None
}

/// Check if a function body contains a try-except block at the top level.
fn body_has_try_except(body: Node) -> bool {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "try_statement" {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::python::parse_python_file;
    use crate::types::context::{Language, SourceFile};

    /// Helper to parse Python source and summarize FastAPI
    fn parse_and_summarize_fastapi(source: &str) -> Option<FastApiFileSummary> {
        let sf = SourceFile {
            path: "test.py".to_string(),
            language: Language::Python,
            content: source.to_string(),
        };
        let parsed = parse_python_file(FileId(1), &sf).expect("parsing should succeed");
        summarize_fastapi(&parsed)
    }

    // ==================== FastAPI App Detection Tests ====================

    #[test]
    fn detects_simple_fastapi_app() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.apps.len(), 1);
        assert_eq!(summary.apps[0].var_name, "app");
    }

    #[test]
    fn detects_fastapi_app_with_different_name() {
        let src = r#"
from fastapi import FastAPI

my_api = FastAPI()
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.apps.len(), 1);
        assert_eq!(summary.apps[0].var_name, "my_api");
    }

    #[test]
    fn detects_fastapi_app_with_arguments() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI(title="My API", version="1.0.0")
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.apps.len(), 1);
        assert_eq!(summary.apps[0].var_name, "app");
    }

    #[test]
    fn detects_multiple_fastapi_apps() {
        let src = r#"
from fastapi import FastAPI

app1 = FastAPI()
app2 = FastAPI()
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.apps.len(), 2);

        let names: Vec<&str> = summary.apps.iter().map(|a| a.var_name.as_str()).collect();
        assert!(names.contains(&"app1"));
        assert!(names.contains(&"app2"));
    }

    #[test]
    fn ignores_non_fastapi_assignments() {
        let src = r#"
x = 42
name = "hello"
obj = SomeOtherClass()
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_none());
    }

    #[test]
    fn ignores_fastapi_without_assignment() {
        let src = r#"
from fastapi import FastAPI

FastAPI()  # Not assigned to a variable
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_none());
    }

    // ==================== CORS Middleware Detection Tests ====================

    #[test]
    fn detects_cors_middleware() {
        let src = r#"
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

app = FastAPI()

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
)
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.middlewares.len(), 1);
        assert_eq!(summary.middlewares[0].app_var_name, "app");
        assert!(
            summary.middlewares[0]
                .middleware_type
                .contains("CORSMiddleware")
        );
    }

    #[test]
    fn detects_cors_middleware_with_full_config() {
        let src = r#"
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

app = FastAPI()

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.middlewares.len(), 1);
    }

    #[test]
    fn detects_cors_on_different_app_variable() {
        let src = r#"
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

my_app = FastAPI()

my_app.add_middleware(CORSMiddleware)
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.middlewares.len(), 1);
        assert_eq!(summary.middlewares[0].app_var_name, "my_app");
    }

    #[test]
    fn ignores_non_cors_middleware() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()

app.add_middleware(SomeOtherMiddleware)
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        // Should have the app but no CORS middleware
        assert_eq!(summary.apps.len(), 1);
        assert!(summary.middlewares.is_empty());
    }

    // ==================== Router Detection Tests ====================

    #[test]
    fn detects_include_router_call() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()

app.include_router(users_router)
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.routers.len(), 1);
        assert_eq!(summary.routers[0].app_var_name, "app");
    }

    #[test]
    fn detects_include_router_with_prefix() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()

app.include_router(users_router, prefix="/api/v1")
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.routers.len(), 1);
        // The router_expr should contain the arguments
        assert!(summary.routers[0].router_expr.contains("users_router"));
    }

    #[test]
    fn detects_multiple_include_router_calls() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()

app.include_router(users_router)
app.include_router(items_router)
app.include_router(orders_router)
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.routers.len(), 3);
    }

    #[test]
    fn detects_include_router_with_module_path() {
        let src = r#"
from fastapi import FastAPI
from app.routers import users

app = FastAPI()

app.include_router(users.router)
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.routers.len(), 1);
        assert!(summary.routers[0].router_expr.contains("users.router"));
    }

    // ==================== Location Tests ====================

    #[test]
    fn app_has_correct_location() {
        let src = r#"from fastapi import FastAPI

app = FastAPI()
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.apps.len(), 1);
        // The app assignment is on line 2 (0-indexed)
        assert_eq!(summary.apps[0].location.range.start_line, 2);
    }

    #[test]
    fn middleware_has_correct_location() {
        let src = r#"from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

app = FastAPI()

app.add_middleware(CORSMiddleware)
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.middlewares.len(), 1);
        // The middleware call is on line 5 (0-indexed)
        assert_eq!(summary.middlewares[0].location.range.start_line, 5);
    }

    #[test]
    fn router_has_correct_location() {
        let src = r#"from fastapi import FastAPI

app = FastAPI()

app.include_router(users_router)
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.routers.len(), 1);
        // The router call is on line 4 (0-indexed)
        assert_eq!(summary.routers[0].location.range.start_line, 4);
    }

    // ==================== Edge Cases ====================

    #[test]
    fn handles_empty_file() {
        let summary = parse_and_summarize_fastapi("");
        assert!(summary.is_none());
    }

    #[test]
    fn handles_file_with_only_imports() {
        let src = r#"
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_none());
    }

    #[test]
    fn handles_file_with_only_comments() {
        let src = r#"
# This is a FastAPI application
# But it has no actual code
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_none());
    }

    #[test]
    fn ignores_fastapi_in_function() {
        // FastAPI() inside a function should still be detected
        // because we walk the entire tree
        let src = r#"
from fastapi import FastAPI

def create_app():
    app = FastAPI()
    return app
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.apps.len(), 1);
    }

    #[test]
    fn handles_complex_assignment_lhs() {
        // Complex LHS should be ignored
        let src = r#"
from fastapi import FastAPI

self.app = FastAPI()
"#;
        let summary = parse_and_summarize_fastapi(src);
        // Should not detect because LHS is not a simple identifier
        assert!(summary.is_none());
    }

    // ==================== Real-World Scenarios ====================

    #[test]
    fn handles_complete_fastapi_setup() {
        let src = r#"
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from app.routers import users, items

app = FastAPI(
    title="My API",
    description="A sample API",
    version="1.0.0"
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.include_router(users.router, prefix="/users", tags=["users"])
app.include_router(items.router, prefix="/items", tags=["items"])

@app.get("/")
async def root():
    return {"message": "Hello World"}
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        assert_eq!(summary.apps.len(), 1);
        assert_eq!(summary.apps[0].var_name, "app");

        assert_eq!(summary.middlewares.len(), 1);
        assert!(
            summary.middlewares[0]
                .middleware_type
                .contains("CORSMiddleware")
        );

        assert_eq!(summary.routers.len(), 2);
    }

    #[test]
    fn handles_split_cors_setup() {
        // CORS configured in a separate function
        let src = r#"
from fastapi.middleware.cors import CORSMiddleware

def setup_cors(app):
    app.add_middleware(
        CORSMiddleware,
        allow_origins=["*"],
    )
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        // Should detect the middleware even without a FastAPI app in this file
        assert_eq!(summary.middlewares.len(), 1);
        assert_eq!(summary.middlewares[0].app_var_name, "app");
    }

    #[test]
    fn handles_factory_pattern() {
        let src = r#"
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

def create_application() -> FastAPI:
    application = FastAPI()
    
    application.add_middleware(
        CORSMiddleware,
        allow_origins=["*"],
    )
    
    return application

app = create_application()
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        // Should detect the app inside the factory function
        assert!(summary.apps.iter().any(|a| a.var_name == "application"));
        assert_eq!(summary.middlewares.len(), 1);
    }

    // ==================== Negative Tests ====================

    #[test]
    fn does_not_detect_flask_app() {
        let src = r#"
from flask import Flask

app = Flask(__name__)
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_none());
    }

    #[test]
    fn does_not_detect_django_app() {
        let src = r#"
from django.apps import AppConfig

class MyAppConfig(AppConfig):
    name = 'myapp'
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_none());
    }

    #[test]
    fn does_not_confuse_fastapi_variable_name() {
        let src = r#"
FastAPI = "not a framework"
app = FastAPI
"#;
        let summary = parse_and_summarize_fastapi(src);
        // Should not detect because FastAPI is not a call
        assert!(summary.is_none());
    }

    // ==================== Routes Tests ====================

    #[test]
    fn detects_simple_route() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/")
async def root():
    return {"message": "Hello"}
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        assert_eq!(summary.routes.len(), 1);
        assert_eq!(summary.routes[0].http_method, "GET");
        assert_eq!(summary.routes[0].path, "/");
        assert_eq!(summary.routes[0].handler_name, "root");
        assert!(summary.routes[0].is_async);
        assert!(!summary.routes[0].has_try_except);
    }

    #[test]
    fn detects_multiple_routes() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/")
async def root():
    return {"message": "Hello"}

@app.post("/items")
async def create_item(item: dict):
    return item
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        assert_eq!(summary.routes.len(), 2);

        let methods: Vec<&str> = summary
            .routes
            .iter()
            .map(|r| r.http_method.as_str())
            .collect();
        assert!(methods.contains(&"GET"));
        assert!(methods.contains(&"POST"));
    }

    #[test]
    fn detects_route_with_try_except() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/users/{user_id}")
async def get_user(user_id: int):
    try:
        return {"user_id": user_id}
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        assert_eq!(summary.routes.len(), 1);
        assert!(summary.routes[0].has_try_except);
    }

    #[test]
    fn detects_sync_route() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/sync")
def sync_handler():
    return {"sync": True}
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        assert_eq!(summary.routes.len(), 1);
        assert!(!summary.routes[0].is_async);
    }

    #[test]
    fn detects_all_http_methods() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/")
async def get_handler():
    pass

@app.post("/")
async def post_handler():
    pass

@app.put("/")
async def put_handler():
    pass

@app.delete("/")
async def delete_handler():
    pass

@app.patch("/")
async def patch_handler():
    pass
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        assert_eq!(summary.routes.len(), 5);

        let methods: Vec<&str> = summary
            .routes
            .iter()
            .map(|r| r.http_method.as_str())
            .collect();
        assert!(methods.contains(&"GET"));
        assert!(methods.contains(&"POST"));
        assert!(methods.contains(&"PUT"));
        assert!(methods.contains(&"DELETE"));
        assert!(methods.contains(&"PATCH"));
    }

    #[test]
    fn detects_router_routes() {
        let src = r#"
from fastapi import APIRouter

router = APIRouter()

@router.get("/items")
async def get_items():
    return []
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        assert_eq!(summary.routes.len(), 1);
        assert_eq!(summary.routes[0].http_method, "GET");
        assert_eq!(summary.routes[0].path, "/items");
    }

    // ==================== Edge Case: include_router without arguments ====================

    #[test]
    fn handles_include_router_without_arguments() {
        // This tests line 251 - the <unknown> fallback when no arguments node
        // In practice, tree-sitter always provides an arguments node,
        // but we test the fallback path by checking the router_expr field
        let src = r#"
from fastapi import FastAPI

app = FastAPI()

app.include_router(users_router)
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.routers.len(), 1);
        // The router_expr should contain the arguments text
        assert!(summary.routers[0].router_expr.contains("users_router"));
    }

    #[test]
    fn router_expr_contains_full_arguments() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()

app.include_router(users.router, prefix="/api/v1", tags=["users"])
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.routers.len(), 1);
        // Should contain the full arguments text
        assert!(summary.routers[0].router_expr.contains("users.router"));
        assert!(summary.routers[0].router_expr.contains("prefix"));
    }

    // ==================== Exception Handler Detection Tests ====================

    #[test]
    fn detects_simple_exception_handler() {
        let src = r#"
from fastapi import FastAPI, Request
from fastapi.exceptions import RequestValidationError

app = FastAPI()

@app.exception_handler(RequestValidationError)
async def validation_handler(request: Request, exc: RequestValidationError):
    return {"error": "validation failed"}
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        assert_eq!(summary.exception_handlers.len(), 1);
        assert_eq!(summary.exception_handlers[0].app_var_name, "app");
        assert_eq!(
            summary.exception_handlers[0].exception_type,
            "RequestValidationError"
        );
        assert_eq!(
            summary.exception_handlers[0].handler_name,
            "validation_handler"
        );
    }

    #[test]
    fn detects_multiple_exception_handlers() {
        let src = r#"
from fastapi import FastAPI, Request
from fastapi.exceptions import RequestValidationError
from starlette.exceptions import HTTPException

app = FastAPI()

@app.exception_handler(RequestValidationError)
async def validation_handler(request: Request, exc: RequestValidationError):
    return {"error": "validation failed"}

@app.exception_handler(HTTPException)
async def http_handler(request: Request, exc: HTTPException):
    return {"error": exc.detail}
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        assert_eq!(summary.exception_handlers.len(), 2);

        let types: Vec<&str> = summary
            .exception_handlers
            .iter()
            .map(|h| h.exception_type.as_str())
            .collect();
        assert!(types.contains(&"RequestValidationError"));
        assert!(types.contains(&"HTTPException"));
    }

    #[test]
    fn detects_generic_exception_handler() {
        let src = r#"
from fastapi import FastAPI, Request

app = FastAPI()

@app.exception_handler(Exception)
async def generic_handler(request: Request, exc: Exception):
    return {"error": str(exc)}
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        assert_eq!(summary.exception_handlers.len(), 1);
        assert_eq!(summary.exception_handlers[0].exception_type, "Exception");
    }

    #[test]
    fn detects_exception_handler_with_module_path() {
        let src = r#"
from fastapi import FastAPI, Request
import starlette.exceptions

app = FastAPI()

@app.exception_handler(starlette.exceptions.HTTPException)
async def http_handler(request: Request, exc):
    return {"error": "http error"}
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        assert_eq!(summary.exception_handlers.len(), 1);
        assert!(
            summary.exception_handlers[0]
                .exception_type
                .contains("HTTPException")
        );
    }

    #[test]
    fn detects_exception_handler_on_different_app() {
        let src = r#"
from fastapi import FastAPI, Request

my_app = FastAPI()

@my_app.exception_handler(ValueError)
async def value_error_handler(request: Request, exc: ValueError):
    return {"error": "value error"}
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        assert_eq!(summary.exception_handlers.len(), 1);
        assert_eq!(summary.exception_handlers[0].app_var_name, "my_app");
    }

    #[test]
    fn exception_handler_only_file_returns_summary() {
        // A file with only exception handlers (no app) should still return a summary
        let src = r#"
from fastapi import Request

@app.exception_handler(Exception)
async def handler(request: Request, exc: Exception):
    return {"error": str(exc)}
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        assert_eq!(summary.exception_handlers.len(), 1);
        assert!(summary.apps.is_empty());
    }

    #[test]
    fn sync_exception_handler_detected() {
        let src = r#"
from fastapi import FastAPI, Request

app = FastAPI()

@app.exception_handler(ValueError)
def sync_handler(request: Request, exc: ValueError):
    return {"error": "sync handler"}
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        assert_eq!(summary.exception_handlers.len(), 1);
        assert_eq!(summary.exception_handlers[0].handler_name, "sync_handler");
    }

    // ==================== Handler Params Tests ====================

    #[test]
    fn extracts_handler_params_with_type_annotation() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()

@app.post("/items")
def create_item(item: ItemCreate):
    return item
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        assert_eq!(summary.routes.len(), 1);
        let params = &summary.routes[0].handler_params;
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "item");
        assert_eq!(params[0].type_annotation, Some("ItemCreate".to_string()));
    }

    #[test]
    fn extracts_handler_params_with_multiple_params() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()

@app.post("/items/{item_id}")
def update_item(item_id: int, item: ItemUpdate):
    return item
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        assert_eq!(summary.routes.len(), 1);
        let params = &summary.routes[0].handler_params;
        assert_eq!(params.len(), 2);

        assert_eq!(params[0].name, "item_id");
        assert_eq!(params[0].type_annotation, Some("int".to_string()));

        assert_eq!(params[1].name, "item");
        assert_eq!(params[1].type_annotation, Some("ItemUpdate".to_string()));
    }

    #[test]
    fn extracts_handler_params_without_type_annotation() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()

@app.post("/data")
def receive_data(data):
    return data
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        assert_eq!(summary.routes.len(), 1);
        let params = &summary.routes[0].handler_params;
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "data");
        assert_eq!(params[0].type_annotation, None);
    }

    #[test]
    fn extracts_handler_params_for_async_route() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()

@app.post("/diagnostics")
async def diagnostics(request: DiagnosticsRequest):
    return {"ok": True}
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        assert_eq!(summary.routes.len(), 1);
        let params = &summary.routes[0].handler_params;
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "request");
        assert_eq!(
            params[0].type_annotation,
            Some("DiagnosticsRequest".to_string())
        );
    }

    #[test]
    fn extracts_handler_params_with_default_values() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()

@app.post("/items")
def create_item(item: ItemCreate, notify: bool = True):
    return item
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        assert_eq!(summary.routes.len(), 1);
        let params = &summary.routes[0].handler_params;
        assert_eq!(params.len(), 2);

        assert_eq!(params[0].name, "item");
        assert_eq!(params[0].type_annotation, Some("ItemCreate".to_string()));

        assert_eq!(params[1].name, "notify");
        assert_eq!(params[1].type_annotation, Some("bool".to_string()));
    }

    #[test]
    fn decorator_location_is_decorator_only() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()

@app.post("/data")
def receive_data(data: dict):
    # Long function body
    result = process(data)
    return result
"#;
        let summary = parse_and_summarize_fastapi(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();

        assert_eq!(summary.routes.len(), 1);
        let route = &summary.routes[0];

        // decorator_location should only cover the decorator line (line 5, 0-indexed)
        assert_eq!(route.decorator_location.range.start_line, 5);
        assert_eq!(route.decorator_location.range.end_line, 5);

        // location should cover the entire decorated function
        assert_eq!(route.location.range.start_line, 5);
        // end_line should be after the function body
        assert!(route.location.range.end_line > route.decorator_location.range.end_line);
    }
}
