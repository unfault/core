//! Fastify framework detection and analysis.

use crate::parse::ast::ParsedFile;

use super::model::{FastifyFileSummary, FastifyRoute, FastifyMiddleware};

pub fn summarize_fastify(parsed: &ParsedFile) -> Option<FastifyFileSummary> {
    let mut summary = FastifyFileSummary {
        apps: Vec::new(),
        routes: Vec::new(),
        middlewares: Vec::new(),
    };

    let root = parsed.tree.root_node();
    walk_for_fastify(root, parsed, &mut summary);

    if summary.apps.is_empty() && summary.routes.is_empty() && summary.middlewares.is_empty() {
        return None;
    }

    Some(summary)
}

fn walk_for_fastify(
    node: tree_sitter::Node,
    parsed: &ParsedFile,
    summary: &mut FastifyFileSummary,
) {
    match node.kind() {
        "lexical_declaration" | "variable_declaration" => {
            check_fastify_instantiation(parsed, &node, summary);
        }
        "call_expression" => {
            check_fastify_routes_and_middleware(parsed, &node, summary);
        }
        _ => {}
    }

    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            walk_for_fastify(child, parsed, summary);
        }
    }
}

fn check_fastify_instantiation(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    summary: &mut FastifyFileSummary,
) {
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
                    if value_text.contains("fastify()")
                        || value_text.contains("Fastify()")
                        || value_text.contains("require('fastify')")
                    {
                        summary.apps.push(var_name.clone());
                    }
                }
            }
        }
    }
}

fn check_fastify_routes_and_middleware(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    summary: &mut FastifyFileSummary,
) {
    let func_node = match node.child_by_field_name("function") {
        Some(n) => n,
        None => return,
    };

    let callee = parsed.text_for_node(&func_node);
    let location = parsed.location_for_node(node);

    let http_methods = ["get", "post", "put", "patch", "delete", "head", "options"];

    for method in http_methods {
        let patterns = [
            format!("fastify.{}", method),
            format!("app.{}", method),
            format!("instance.{}", method),
        ];

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

    if callee.ends_with(".use") || callee == "fastify.use" || callee == "app.use" {
        let middleware = extract_middleware_name(parsed, node, &location);
        summary.middlewares.push(middleware);
    }

    if callee.ends_with(".register") || callee.contains(".register(") {
        if let Some(name) = extract_register_name(parsed, node) {
            summary.middlewares.push(name);
        }
    }
}

fn extract_route_info(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    method: &str,
    location: &crate::parse::ast::AstLocation,
) -> FastifyRoute {
    let mut path = None;
    let mut handler_name = None;
    let mut is_async = false;

    if let Some(args_node) = node.child_by_field_name("arguments") {
        if let Some(first_arg) = args_node.named_child(0) {
            let text = parsed.text_for_node(&first_arg);
            if text.starts_with('\'') || text.starts_with('"') || text.starts_with('`') {
                path = Some(
                    text.trim_matches(|c| c == '\'' || c == '"' || c == '`')
                        .to_string(),
                );
            }
        }

        if let Some(handler) = args_node.named_child(1) {
            let handler_text = parsed.text_for_node(&handler);
            is_async = handler_text.starts_with("async");

            if handler.kind() == "identifier" {
                handler_name = Some(handler_text);
            }
        }
    }

    FastifyRoute {
        method: method.to_string(),
        path,
        handler_name,
        is_async,
        location: location.clone(),
    }
}

fn extract_middleware_name(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    location: &crate::parse::ast::AstLocation,
) -> FastifyMiddleware {
    let mut middleware_name = "unknown".to_string();

    if let Some(args_node) = node.child_by_field_name("arguments") {
        if let Some(first_arg) = args_node.named_child(0) {
            let text = parsed.text_for_node(&first_arg);

            if text.contains("cors") {
                middleware_name = "cors".to_string();
            } else if text.contains("helmet") {
                middleware_name = "helmet".to_string();
            } else if text.contains("fastify.json") || text.contains("bodyParser.json") {
                middleware_name = "json".to_string();
            } else if text.contains("fastify.urlencoded") || text.contains("bodyParser.urlencoded") {
                middleware_name = "urlencoded".to_string();
            } else if text.contains("fastify.static") {
                middleware_name = "static".to_string();
            } else if text.contains("morgan") {
                middleware_name = "morgan".to_string();
            } else if text.contains("compression") {
                middleware_name = "compression".to_string();
            } else if first_arg.kind() == "identifier" {
                middleware_name = text;
            } else if first_arg.kind() == "call_expression" {
                if let Some(func) = first_arg.child_by_field_name("function") {
                    middleware_name = parsed.text_for_node(&func);
                }
            }
        }
    }

    FastifyMiddleware {
        middleware_name,
        location: location.clone(),
    }
}

fn extract_register_name(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<FastifyMiddleware> {
    let location = parsed.location_for_node(node);

    if let Some(args_node) = node.child_by_field_name("arguments") {
        if let Some(first_arg) = args_node.named_child(0) {
            let text = parsed.text_for_node(&first_arg);
            let text_lower = text.to_lowercase();

            let middleware_name = if text_lower.contains("cors") {
                "cors".to_string()
            } else if text_lower.contains("helmet") {
                "helmet".to_string()
            } else if text_lower.contains("fastify-jwt") || text_lower.contains("@fastify/jwt") || text_lower.contains("jwt") {
                "fastify-jwt".to_string()
            } else if text_lower.contains("fastify-cookie") || text_lower.contains("@fastify/cookie") || text_lower.contains("cookie") {
                "fastify-cookie".to_string()
            } else if text_lower.contains("fastify-rate-limit") || text_lower.contains("@fastify/rate-limit") || text_lower.contains("rate-limit") || text_lower.contains("ratelimit") {
                "fastify-rate-limit".to_string()
            } else if first_arg.kind() == "identifier" {
                text
            } else {
                return None;
            };

            return Some(FastifyMiddleware {
                middleware_name,
                location,
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::typescript::parse_typescript_file;
    use crate::types::context::{Language, SourceFile};

    fn parse_and_summarize(source: &str) -> Option<FastifyFileSummary> {
        let sf = SourceFile {
            path: "test.ts".to_string(),
            language: Language::Typescript,
            content: source.to_string(),
        };
        let parsed = parse_typescript_file(FileId(1), &sf).expect("parsing should succeed");
        summarize_fastify(&parsed)
    }

    #[test]
    fn detects_fastify_app() {
        let src = r#"
import fastify from 'fastify';

const app = fastify();
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert!(summary.apps.contains(&"app".to_string()));
    }

    #[test]
    fn detects_fastify_with_require() {
        let src = r#"
const fastify = require('fastify');
const app = fastify();
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert!(!summary.apps.is_empty());
    }

    #[test]
    fn detects_route_definition() {
        let src = r#"
const app = fastify();

app.get('/users', (req, reply) => {
    reply.send([]);
});
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert!(!summary.routes.is_empty());
        assert_eq!(summary.routes[0].method, "get");
        assert_eq!(summary.routes[0].path, Some("/users".to_string()));
    }

    #[test]
    fn detects_all_http_methods() {
        let src = r#"
const app = fastify();

app.get('/', () => {});
app.post('/', () => {});
app.put('/', () => {});
app.patch('/', () => {});
app.delete('/', () => {});
app.head('/', () => {});
app.options('/', () => {});
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert_eq!(summary.routes.len(), 7);

        let methods: Vec<&str> = summary.routes.iter().map(|r| r.method.as_str()).collect();
        assert!(methods.contains(&"get"));
        assert!(methods.contains(&"post"));
        assert!(methods.contains(&"put"));
        assert!(methods.contains(&"patch"));
        assert!(methods.contains(&"delete"));
        assert!(methods.contains(&"head"));
        assert!(methods.contains(&"options"));
    }

    #[test]
    fn detects_middleware() {
        let src = r#"
import fastify from 'fastify';
import cors from 'cors';

const app = fastify();

app.use(cors());
app.use(fastify.json());
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert!(summary.middlewares.len() >= 2);
    }

    #[test]
    fn detects_async_route_handler() {
        let src = r#"
const app = fastify();

app.get('/users', async (req, reply) => {
    return await db.findAll();
});
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert!(!summary.routes.is_empty());
        assert!(summary.routes[0].is_async);
    }

    #[test]
    fn detects_handler_name_for_named_function() {
        let src = r#"
const app = fastify();

async function getUsers(req, reply) {
    return [];
}

app.get('/users', getUsers);
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert_eq!(summary.routes.len(), 1);
        assert_eq!(summary.routes[0].handler_name, Some("getUsers".to_string()));
    }

    #[test]
    fn returns_none_for_non_fastify_code() {
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
const app = fastify();

app.post('/api/users', (req, res) => {});
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert_eq!(summary.routes[0].path, Some("/api/users".to_string()));
    }

    #[test]
    fn detects_fastify_register() {
        let src = r#"
import fastify from 'fastify';
import fastifyJwt from 'fastify-jwt';

const app = fastify();

app.register(fastifyJwt, { secret: 'supersecret' });
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert!(!summary.middlewares.is_empty());
    }

    #[test]
    fn detects_multiple_routes() {
        let src = r#"
const app = fastify();

app.get('/users', () => {});
app.post('/users', () => {});
app.delete('/users/:id', () => {});
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert_eq!(summary.routes.len(), 3);
    }

    #[test]
    fn detects_cors_middleware() {
        let src = r#"
import fastify from 'fastify';
import cors from 'cors';

const app = fastify();

app.register(cors, { origin: true });
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert!(summary.middlewares.len() >= 1);
    }

    #[test]
    fn detects_helmet_middleware() {
        let src = r#"
import fastify from 'fastify';
import helmet from 'helmet';

const app = fastify();

app.use(helmet());
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert!(summary.middlewares.iter().any(|m| m.middleware_name == "helmet"));
    }

    #[test]
    fn detects_fastify_cookie_plugin() {
        let src = r#"
import fastify from 'fastify';
import fastifyCookie from '@fastify/cookie';

const app = fastify();

app.register(fastifyCookie, { secret: 'my-secret' });
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert!(summary.middlewares.iter().any(|m| m.middleware_name.contains("cookie")));
    }

    #[test]
    fn detects_fastify_rate_limit_plugin() {
        let src = r#"
import fastify from 'fastify';
import fastifyRateLimit from '@fastify/rate-limit';

const app = fastify();

app.register(fastifyRateLimit, { max: 100 });
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert!(summary
            .middlewares
            .iter()
            .any(|m| m.middleware_name.contains("rate-limit")));
    }
}
