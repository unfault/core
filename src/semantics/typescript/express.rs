//! Express.js framework detection and analysis.

use crate::parse::ast::ParsedFile;

use super::model::{
    ExpressApp, ExpressFileSummary, ExpressMiddleware, ExpressRoute, ExpressRouter,
};

/// Summarize Express.js-related semantics in a TypeScript file.
pub fn summarize_express(parsed: &ParsedFile) -> Option<ExpressFileSummary> {
    let mut summary = ExpressFileSummary {
        apps: Vec::new(),
        routers: Vec::new(),
        routes: Vec::new(),
        middlewares: Vec::new(),
    };

    let root = parsed.tree.root_node();
    walk_for_express(root, parsed, &mut summary);

    // Only return Some if we found any Express-related constructs
    if summary.apps.is_empty()
        && summary.routers.is_empty()
        && summary.routes.is_empty()
        && summary.middlewares.is_empty()
    {
        return None;
    }

    Some(summary)
}

fn walk_for_express(
    node: tree_sitter::Node,
    parsed: &ParsedFile,
    summary: &mut ExpressFileSummary,
) {
    match node.kind() {
        "lexical_declaration" | "variable_declaration" => {
            // Check for express() or express.Router() calls
            check_express_instantiation(parsed, &node, summary);
        }
        "call_expression" => {
            // Check for route definitions and middleware
            check_route_or_middleware(parsed, &node, summary);
        }
        _ => {}
    }

    // Recurse
    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            walk_for_express(child, parsed, summary);
        }
    }
}

fn check_express_instantiation(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    summary: &mut ExpressFileSummary,
) {
    let _text = parsed.text_for_node(node);

    // Look for patterns like:
    // const app = express()
    // const router = express.Router()
    // const router = Router()

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "variable_declarator" {
                let name = child
                    .child_by_field_name("name")
                    .map(|n| parsed.text_for_node(&n));

                let value = child
                    .child_by_field_name("value")
                    .map(|n| parsed.text_for_node(&n));

                if let (Some(var_name), Some(value_text)) = (name, value) {
                    let location = parsed.location_for_node(&child);

                    // Check for express() call
                    if value_text.contains("express()") {
                        summary.apps.push(ExpressApp {
                            variable_name: var_name.clone(),
                            location: location.clone(),
                        });
                    }

                    // Check for Router() call
                    if value_text.contains("Router()") || value_text.contains("express.Router()") {
                        summary.routers.push(ExpressRouter {
                            variable_name: var_name,
                            location,
                        });
                    }
                }
            }
        }
    }
}

fn check_route_or_middleware(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    summary: &mut ExpressFileSummary,
) {
    let func_node = match node.child_by_field_name("function") {
        Some(n) => n,
        None => return,
    };

    let callee = parsed.text_for_node(&func_node);
    let location = parsed.location_for_node(node);

    // HTTP method routes: app.get(), router.post(), etc.
    let http_methods = [
        "get", "post", "put", "patch", "delete", "head", "options", "all",
    ];

    for method in http_methods {
        let patterns = [format!("app.{}", method), format!("router.{}", method)];

        for pattern in &patterns {
            if callee.ends_with(pattern.as_str())
                || callee == pattern.as_str()
                || callee.ends_with(&format!(".{}", method))
            {
                let route = extract_route_info(parsed, node, method, &location);
                summary.routes.push(route);
                return;
            }
        }
    }

    // Middleware: app.use() or router.use()
    if callee.ends_with(".use") || callee == "app.use" || callee == "router.use" {
        let middleware = extract_middleware_info(parsed, node, &location);
        summary.middlewares.push(middleware);
    }
}

fn extract_route_info(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    method: &str,
    location: &crate::parse::ast::AstLocation,
) -> ExpressRoute {
    let mut path = None;
    let mut handler_name = None;
    let mut is_async = false;
    let mut has_error_handler = false;

    if let Some(args_node) = node.child_by_field_name("arguments") {
        // First argument is usually the path
        if let Some(first_arg) = args_node.named_child(0) {
            let text = parsed.text_for_node(&first_arg);
            if text.starts_with('\'') || text.starts_with('"') || text.starts_with('`') {
                path = Some(
                    text.trim_matches(|c| c == '\'' || c == '"' || c == '`')
                        .to_string(),
                );
            }
        }

        // Check for handler (usually second argument)
        if let Some(handler) = args_node.named_child(1) {
            let handler_text = parsed.text_for_node(&handler);
            is_async = handler_text.starts_with("async");

            // Check if handler has error handling parameter (4 params = error handler)
            has_error_handler = has_four_params(&handler);

            // Try to get handler name if it's a reference
            if handler.kind() == "identifier" {
                handler_name = Some(handler_text);
            }
        }

        // Check for error handler middleware (if there are 3+ arguments)
        if args_node.named_child_count() >= 3 {
            if let Some(last_arg) = args_node.named_child(args_node.named_child_count() - 1) {
                if has_four_params(&last_arg) {
                    has_error_handler = true;
                }
            }
        }
    }

    ExpressRoute {
        method: method.to_string(),
        path,
        handler_name,
        is_async,
        has_error_handler,
        location: location.clone(),
    }
}

fn has_four_params(node: &tree_sitter::Node) -> bool {
    // Error handlers in Express have 4 parameters: (err, req, res, next)
    if let Some(params) = node.child_by_field_name("parameters") {
        return params.named_child_count() == 4;
    }

    // For arrow functions, check params
    if node.kind() == "arrow_function" {
        if let Some(params) = node.child_by_field_name("parameters") {
            return params.named_child_count() == 4;
        }
        // Single param without parentheses
        if node.child_by_field_name("parameter").is_some() {
            return false;
        }
    }

    false
}

fn extract_middleware_info(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    location: &crate::parse::ast::AstLocation,
) -> ExpressMiddleware {
    let mut middleware_name = "unknown".to_string();

    if let Some(args_node) = node.child_by_field_name("arguments") {
        if let Some(first_arg) = args_node.named_child(0) {
            let text = parsed.text_for_node(&first_arg);

            // Extract middleware name from common patterns
            if text.contains("cors") {
                middleware_name = "cors".to_string();
            } else if text.contains("helmet") {
                middleware_name = "helmet".to_string();
            } else if text.contains("express.json") || text.contains("bodyParser.json") {
                middleware_name = "json".to_string();
            } else if text.contains("express.urlencoded") || text.contains("bodyParser.urlencoded")
            {
                middleware_name = "urlencoded".to_string();
            } else if text.contains("express.static") {
                middleware_name = "static".to_string();
            } else if text.contains("morgan") {
                middleware_name = "morgan".to_string();
            } else if text.contains("compression") {
                middleware_name = "compression".to_string();
            } else if text.contains("cookieParser") || text.contains("cookie-parser") {
                middleware_name = "cookieParser".to_string();
            } else if text.contains("session") {
                middleware_name = "session".to_string();
            } else if text.contains("passport") {
                middleware_name = "passport".to_string();
            } else if text.contains("rateLimit") {
                middleware_name = "rateLimit".to_string();
            } else if first_arg.kind() == "identifier" {
                middleware_name = text;
            } else if first_arg.kind() == "call_expression" {
                // Get function name from call
                if let Some(func) = first_arg.child_by_field_name("function") {
                    middleware_name = parsed.text_for_node(&func);
                }
            }
        }
    }

    ExpressMiddleware {
        middleware_name,
        location: location.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::typescript::parse_typescript_file;
    use crate::types::context::{Language, SourceFile};

    fn parse_and_summarize(source: &str) -> Option<ExpressFileSummary> {
        let sf = SourceFile {
            path: "test.ts".to_string(),
            language: Language::Typescript,
            content: source.to_string(),
        };
        let parsed = parse_typescript_file(FileId(1), &sf).expect("parsing should succeed");
        summarize_express(&parsed)
    }

    #[test]
    fn detects_express_app() {
        let src = r#"
import express from 'express';
const app = express();
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert_eq!(summary.apps.len(), 1);
        assert_eq!(summary.apps[0].variable_name, "app");
    }

    #[test]
    fn detects_express_router() {
        let src = r#"
import { Router } from 'express';
const router = Router();
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert_eq!(summary.routers.len(), 1);
        assert_eq!(summary.routers[0].variable_name, "router");
    }

    #[test]
    fn detects_route_definition() {
        let src = r#"
const app = express();
app.get('/users', (req, res) => {
    res.json([]);
});
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert!(!summary.routes.is_empty());
        assert_eq!(summary.routes[0].method, "get");
    }

    #[test]
    fn detects_middleware() {
        let src = r#"
const app = express();
app.use(express.json());
app.use(cors());
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert!(summary.middlewares.len() >= 2);
    }

    #[test]
    fn detects_async_route_handler() {
        let src = r#"
const app = express();
app.get('/users', async (req, res) => {
    const users = await User.findAll();
    res.json(users);
});
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert!(!summary.routes.is_empty());
        assert!(summary.routes[0].is_async);
    }

    #[test]
    fn returns_none_for_non_express_code() {
        let src = r#"
const x = 1;
console.log(x);
"#;
        let summary = parse_and_summarize(src);
        assert!(summary.is_none());
    }

    #[test]
    fn detects_route_path() {
        let src = r#"
const app = express();
app.post('/api/users', (req, res) => {});
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert_eq!(summary.routes[0].path, Some("/api/users".to_string()));
    }

    #[test]
    fn detects_multiple_routes() {
        let src = r#"
const app = express();
app.get('/users', (req, res) => {});
app.post('/users', (req, res) => {});
app.delete('/users/:id', (req, res) => {});
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert_eq!(summary.routes.len(), 3);
    }

    #[test]
    fn detects_handler_name_for_named_function_reference() {
        let src = r#"
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
        let summary = parse_and_summarize(src).unwrap();
        assert_eq!(summary.routes.len(), 2);

        // First route should have handler_name = getUsers
        assert_eq!(summary.routes[0].handler_name, Some("getUsers".to_string()));
        assert_eq!(summary.routes[0].path, Some("/users".to_string()));
        assert_eq!(summary.routes[0].method, "get");

        // Second route should have handler_name = createUser
        assert_eq!(
            summary.routes[1].handler_name,
            Some("createUser".to_string())
        );
        assert_eq!(summary.routes[1].path, Some("/users".to_string()));
        assert_eq!(summary.routes[1].method, "post");
    }
}
