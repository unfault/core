//! NestJS framework detection and analysis.

use crate::parse::ast::ParsedFile;

use super::model::{
    NestJSController, NestJSFileSummary, NestJSGuard, NestJSInterceptor, NestJSModule, NestJSRoute,
    NestJSService,
};

pub fn summarize_nestjs(parsed: &ParsedFile) -> Option<NestJSFileSummary> {
    let mut summary = NestJSFileSummary {
        controllers: Vec::new(),
        services: Vec::new(),
        modules: Vec::new(),
        routes: Vec::new(),
        guards: Vec::new(),
        interceptors: Vec::new(),
    };

    let root = parsed.tree.root_node();
    walk_for_nestjs(root, parsed, &mut summary);

    if summary.controllers.is_empty()
        && summary.services.is_empty()
        && summary.modules.is_empty()
        && summary.routes.is_empty()
        && summary.guards.is_empty()
        && summary.interceptors.is_empty()
    {
        return None;
    }

    Some(summary)
}

fn walk_for_nestjs(node: tree_sitter::Node, parsed: &ParsedFile, summary: &mut NestJSFileSummary) {
    match node.kind() {
        "class_declaration" => {
            analyze_nestjs_class(parsed, &node, summary);
        }
        "method_definition" => {
            analyze_nestjs_method_decorators(parsed, &node, summary);
        }
        _ => {}
    }

    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            walk_for_nestjs(child, parsed, summary);
        }
    }
}

fn analyze_nestjs_class(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    summary: &mut NestJSFileSummary,
) {
    let _class_text = parsed.text_for_node(node);

    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };
    let class_name = parsed.text_for_node(&name_node);
    let location = parsed.location_for_node(node);

    let decorators = extract_decorators(parsed, node);

    let has_controller = decorators
        .iter()
        .any(|d| d.contains("@Controller") || d.contains("@RestController"));
    let has_injectable = decorators.iter().any(|d| d.contains("@Injectable"));
    let has_module = decorators.iter().any(|d| d.contains("@Module"));
    let has_guards = decorators.iter().any(|d| d.contains("@UseGuards"));
    let has_interceptors = decorators.iter().any(|d| d.contains("@UseInterceptors"));

    if has_controller {
        let mut controller = NestJSController {
            class_name: class_name.clone(),
            routes: Vec::new(),
            location: location.clone(),
        };

        for decorator in &decorators {
            if let Some(route_path) = extract_controller_route(decorator) {
                controller.routes.push(route_path);
            }
        }

        summary.controllers.push(controller);
        summary.modules.push(NestJSModule {
            class_name: class_name.clone(),
            location: location.clone(),
        });
    } else if has_injectable {
        summary.services.push(NestJSService {
            class_name: class_name.clone(),
            location: location.clone(),
        });
    } else if has_module {
        summary.modules.push(NestJSModule {
            class_name: class_name.clone(),
            location: location.clone(),
        });
    }

    if has_guards {
        summary.guards.push(NestJSGuard {
            class_name: class_name.clone(),
            location: location.clone(),
        });
    }

    if has_interceptors {
        summary.interceptors.push(NestJSInterceptor {
            class_name: class_name.clone(),
            location: location.clone(),
        });
    }
}

fn analyze_nestjs_method_decorators(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    summary: &mut NestJSFileSummary,
) {
    let _method_text = parsed.text_for_node(node);
    let location = parsed.location_for_node(node);

    let name_node = node.child_by_field_name("name");
    let method_name = name_node
        .as_ref()
        .map(|n| parsed.text_for_node(n))
        .unwrap_or_default();

    let decorators = extract_method_decorators(parsed, node);

    for decorator in &decorators {
        if let Some((http_method, path)) = extract_http_method_and_path(decorator) {
            let is_async = _method_text.contains("async");

            summary.routes.push(NestJSRoute {
                method: http_method,
                path,
                handler_name: method_name.clone(),
                is_async,
                location: location.clone(),
            });
        }
    }
}

fn extract_method_decorators(parsed: &ParsedFile, node: &tree_sitter::Node) -> Vec<String> {
    let mut decorators = Vec::new();

    if let Some(parent) = node.parent() {
        let mut last_decorator: Option<String> = None;
        let mut found_method = false;

        for i in 0..parent.child_count() {
            if let Some(child) = parent.child(i) {
                if child.id() == node.id() {
                    found_method = true;
                }

                if !found_method && child.kind() == "decorator" {
                    last_decorator = Some(parsed.text_for_node(&child));
                }

                if found_method && last_decorator.is_some() {
                    decorators.push(last_decorator.take().unwrap());
                    break;
                }
            }
        }
    }

    decorators
}

fn extract_decorators(parsed: &ParsedFile, node: &tree_sitter::Node) -> Vec<String> {
    let mut decorators = Vec::new();

    // Check direct children of the node
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "decorator" {
                decorators.push(parsed.text_for_node(&child));
            }
        }
    }

    // Also check parent siblings (decorators that come before the class in export_statement)
    if let Some(parent) = node.parent() {
        if parent.kind() == "export_statement" {
            if let Some(grandparent) = parent.parent() {
                for i in 0..grandparent.child_count() {
                    if let Some(child) = grandparent.child(i) {
                        if child.id() == parent.id() {
                            break;
                        }
                        if child.kind() == "decorator" {
                            decorators.push(parsed.text_for_node(&child));
                        }
                    }
                }
            }
        }
    }

    decorators
}

fn extract_controller_route(decorator: &str) -> Option<String> {
    let start = decorator.find('(')?;
    let end = decorator.rfind(')')?;
    let args = &decorator[start + 1..end];

    let content = args.trim();

    if content.is_empty() {
        return Some("/".to_string());
    }

    let content = content.trim_matches(|c| c == '"' || c == '\'');
    Some(content.to_string())
}

fn extract_http_method_and_path(decorator: &str) -> Option<(String, String)> {
    let start = decorator.find('(')?;
    let end = decorator.rfind(')')?;
    let args = &decorator[start + 1..end];

    let args = args.trim();

    let content = args.trim_matches(|c| c == '"' || c == '\'');

    let http_method = if decorator.contains("@Get") {
        "GET"
    } else if decorator.contains("@Post") {
        "POST"
    } else if decorator.contains("@Put") {
        "PUT"
    } else if decorator.contains("@Patch") {
        "PATCH"
    } else if decorator.contains("@Delete") {
        "DELETE"
    } else if decorator.contains("@Options") {
        "OPTIONS"
    } else if decorator.contains("@Head") {
        "HEAD"
    } else {
        return None;
    };

    Some((http_method.to_string(), content.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::typescript::parse_typescript_file;
    use crate::types::context::{Language, SourceFile};

    fn parse_and_summarize(source: &str) -> Option<NestJSFileSummary> {
        let sf = SourceFile {
            path: "test.ts".to_string(),
            language: Language::Typescript,
            content: source.to_string(),
        };
        let parsed = parse_typescript_file(FileId(1), &sf).expect("parsing should succeed");
        summarize_nestjs(&parsed)
    }

    #[test]
    fn detects_nestjs_controller() {
        let src = r#"
import { Controller, Get } from '@nestjs/common';

@Controller('users')
class UserController {
    @Get()
    findAll() {
        return [];
    }
}
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert_eq!(summary.controllers.len(), 1);
        assert_eq!(summary.controllers[0].class_name, "UserController");
    }

    #[test]
    fn detects_nestjs_service() {
        let src = r#"
import { Injectable } from '@nestjs/common';

@Injectable()
class UserService {
    findAll() {
        return [];
    }
}
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert_eq!(summary.services.len(), 1);
        assert_eq!(summary.services[0].class_name, "UserService");
    }

    #[test]
    fn detects_nestjs_module() {
        let src = r#"
import { Module } from '@nestjs/common';

@Module({
    controllers: [UserController],
})
class AppModule {}
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert_eq!(summary.modules.len(), 1);
        assert_eq!(summary.modules[0].class_name, "AppModule");
    }

    #[test]
    fn detects_nestjs_routes() {
        let src = r#"
import { Controller, Get, Post, Body } from '@nestjs/common';

@Controller('users')
class UserController {
    @Get()
    findAll() {
        return [];
    }

    @Post()
    create(@Body() body: any) {
        return body;
    }
}
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert_eq!(summary.routes.len(), 2);
        assert_eq!(summary.routes[0].method, "GET");
        assert_eq!(summary.routes[0].path, "");
        assert_eq!(summary.routes[1].method, "POST");
    }

    #[test]
    fn detects_nestjs_route_with_path() {
        let src = r#"
import { Controller, Get, Post } from '@nestjs/common';

@Controller('users')
class UserController {
    @Get('profile')
    getProfile() {
        return {};
    }

    @Post('create')
    create() {
        return {};
    }
}
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert_eq!(summary.routes.len(), 2);
        assert_eq!(summary.routes[0].path, "profile");
        assert_eq!(summary.routes[1].path, "create");
    }

    #[test]
    fn detects_async_route_handler() {
        let src = r#"
import { Controller, Get } from '@nestjs/common';

@Controller('users')
class UserController {
    @Get()
    async findAll() {
        return await this.service.findAll();
    }
}
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert!(!summary.routes.is_empty());
        assert!(summary.routes[0].is_async);
    }

    #[test]
    fn detects_nestjs_guard() {
        let src = r#"
import { Controller, Get } from '@nestjs/common';
import { AuthGuard } from './auth.guard';

@Controller('users')
@UseGuards(AuthGuard)
class UserController {
    @Get()
    findAll() {
        return [];
    }
}
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert!(summary.guards.len() >= 1);
    }

    #[test]
    fn detects_nestjs_interceptor() {
        let src = r#"
import { Controller, Get, UseInterceptors } from '@nestjs/common';
import { LoggingInterceptor } from './logging.interceptor';

@Controller('users')
@UseInterceptors(LoggingInterceptor)
class UserController {
    @Get()
    findAll() {
        return [];
    }
}
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert!(summary.interceptors.len() >= 1);
    }

    #[test]
    fn returns_none_for_non_nestjs_code() {
        let src = r#"
const x = 1;
console.log(x);
"#;
        let summary = parse_and_summarize(src);
        assert!(summary.is_none());
    }

    #[test]
    fn detects_multiple_controllers() {
        let src = r#"
import { Controller, Get } from '@nestjs/common';

@Controller('users')
class UserController {
    @Get()
    findAll() {}
}

@Controller('orders')
class OrderController {
    @Get()
    findAll() {}
}
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert_eq!(summary.controllers.len(), 2);
    }

    #[test]
    fn detects_controller_with_prefix() {
        let src = r#"
import { Controller, Get } from '@nestjs/common';

@Controller('api/v1/users')
class UserController {
    @Get('profile')
    getProfile() {}
}
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert_eq!(summary.controllers.len(), 1);
        assert_eq!(summary.controllers[0].routes[0], "api/v1/users");
        assert_eq!(summary.routes[0].path, "profile");
    }

    #[test]
    fn detects_all_http_methods() {
        let src = r#"
import { Controller, Get, Post, Put, Patch, Delete, Options, Head } from '@nestjs/common';

@Controller('items')
class ItemController {
    @Get() get() {}
    @Post() post() {}
    @Put() put() {}
    @Patch() patch() {}
    @Delete() delete() {}
    @Options() options() {}
    @Head() head() {}
}
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert_eq!(summary.routes.len(), 7);

        let methods: Vec<&str> = summary.routes.iter().map(|r| r.method.as_str()).collect();
        assert!(methods.contains(&"GET"));
        assert!(methods.contains(&"POST"));
        assert!(methods.contains(&"PUT"));
        assert!(methods.contains(&"PATCH"));
        assert!(methods.contains(&"DELETE"));
        assert!(methods.contains(&"OPTIONS"));
        assert!(methods.contains(&"HEAD"));
    }

    #[test]
    fn extracts_handler_name() {
        let src = r#"
import { Controller, Get } from '@nestjs/common';

@Controller('users')
class UserController {
    @Get()
    findAllUsers() {
        return [];
    }
}
"#;
        let summary = parse_and_summarize(src).unwrap();
        assert_eq!(summary.routes[0].handler_name, "findAllUsers");
    }
}
