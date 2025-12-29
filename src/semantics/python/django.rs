use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use crate::parse::ast::{AstLocation, ParsedFile};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DjangoFileSummary {
    pub apps: Vec<DjangoApp>,
    pub views: Vec<DjangoView>,
    pub urls: Vec<DjangoUrlPattern>,
    pub middleware: Vec<DjangoMiddleware>,
    pub models: Vec<DjangoModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DjangoApp {
    pub var_name: String,
    pub location: AstLocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DjangoView {
    pub name: String,
    pub http_method: String,
    pub path: Option<String>,
    pub is_async: bool,
    pub has_try_except: bool,
    pub location: AstLocation,
    pub body_start_byte: usize,
    pub body_end_byte: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DjangoUrlPattern {
    pub path_expr: String,
    pub view_name: String,
    pub view_type: ViewType,
    pub name: Option<String>,
    pub location: AstLocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ViewType {
    Function,
    Class,
    Include,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DjangoMiddleware {
    pub var_name: String,
    pub middleware_type: String,
    pub location: AstLocation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DjangoModel {
    pub name: String,
    pub base_classes: Vec<String>,
    pub location: AstLocation,
}

pub fn summarize_django(file: &ParsedFile) -> Option<DjangoFileSummary> {
    let root = file.tree.root_node();

    let mut apps = Vec::new();
    let mut views = Vec::new();
    let mut urls = Vec::new();
    let mut middleware = Vec::new();
    let mut models = Vec::new();

    collect_django_apps(file, root, &mut apps);
    collect_django_views(file, root, &mut views);
    collect_django_urls(file, root, &mut urls);
    collect_django_middleware(file, root, &mut middleware);
    collect_django_models(file, root, &mut models);

    if apps.is_empty()
        && views.is_empty()
        && urls.is_empty()
        && middleware.is_empty()
        && models.is_empty()
    {
        return None;
    }

    Some(DjangoFileSummary {
        apps,
        views,
        urls,
        middleware,
        models,
    })
}

fn collect_django_apps(file: &ParsedFile, node: Node, out: &mut Vec<DjangoApp>) {
    if node.kind() == "assignment" {
        if let Some(app) = extract_django_app(file, node) {
            out.push(app);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_django_apps(file, child, out);
    }
}

fn extract_django_app(file: &ParsedFile, node: Node) -> Option<DjangoApp> {
    let left = node.child_by_field_name("left")?;
    let right = node.child_by_field_name("right")?;

    if left.kind() != "identifier" {
        return None;
    }

    if right.kind() != "call" {
        return None;
    }

    let function = right.child_by_field_name("function")?;
    let func_name = file.text_for_node(&function);
    if func_name != "Django" && func_name != "get_wsgi_application" {
        return None;
    }

    let app_var_name = file.text_for_node(&left);
    let location = file.location_for_node(&right);

    Some(DjangoApp {
        var_name: app_var_name,
        location,
    })
}

fn collect_django_views(file: &ParsedFile, node: Node, out: &mut Vec<DjangoView>) {
    if node.kind() == "function_definition" || node.kind() == "async_function_definition" {
        if let Some(view) = extract_django_view(file, node) {
            out.push(view);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_django_views(file, child, out);
    }
}

fn extract_django_view(file: &ParsedFile, node: Node) -> Option<DjangoView> {
    let _source_bytes = file.source.as_bytes();

    let name_node = node.child_by_field_name("name")?;
    let name = file.text_for_node(&name_node);

    let body = node.child_by_field_name("body")?;

    let fn_text = file.text_for_node(&node);
    let is_async = fn_text.trim_start().starts_with("async def");

    let has_try_except = body_has_try_except(body);

    let location = file.location_for_node(&node);

    let http_method = detect_http_method(&name, &fn_text);

    Some(DjangoView {
        name,
        http_method,
        path: None,
        is_async,
        has_try_except,
        location,
        body_start_byte: body.start_byte(),
        body_end_byte: body.end_byte(),
    })
}

fn detect_http_method(_func_name: &str, _fn_text: &str) -> String {
    "GET".to_string()
}

fn body_has_try_except(body: Node) -> bool {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() == "try_statement" {
            return true;
        }
    }
    false
}

fn collect_django_urls(file: &ParsedFile, root: Node, out: &mut Vec<DjangoUrlPattern>) {
    fn walk(file: &ParsedFile, node: Node, out: &mut Vec<DjangoUrlPattern>) {
        if node.kind() == "call" {
            if let Some(url) = extract_django_url(file, node) {
                out.push(url);
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

fn extract_django_url(file: &ParsedFile, node: Node) -> Option<DjangoUrlPattern> {
    let source_bytes = file.source.as_bytes();

    let func = node.child_by_field_name("function")?;

    let method_name = if func.kind() == "attribute" {
        let attr = func.child_by_field_name("attribute")?;
        attr.utf8_text(source_bytes).ok()?.to_string()
    } else if func.kind() == "identifier" {
        file.text_for_node(&func)
    } else {
        return None;
    };

    if method_name != "path" && method_name != "re_path" && method_name != "include" {
        return None;
    }

    let view_type = match method_name.as_str() {
        "include" => ViewType::Include,
        _ => ViewType::Function,
    };

    let args = node.child_by_field_name("arguments")?;
    let mut args_cursor = args.walk();
    let mut path_expr = String::new();
    let mut view_name = String::new();
    let name = None;
    let mut arg_count = 0;

    for child in args.children(&mut args_cursor) {
        match child.kind() {
            "(" | ")" | "," => continue,
            _ => {
                arg_count += 1;
                let text = child.utf8_text(source_bytes).ok()?.to_string();
                match arg_count {
                    1 => path_expr = text,
                    2 => view_name = text,
                    _ => {}
                }
            }
        }
    }

    if path_expr.is_empty() {
        return None;
    }

    let location = file.location_for_node(&node);

    Some(DjangoUrlPattern {
        path_expr,
        view_name,
        view_type,
        name,
        location,
    })
}

fn collect_django_middleware(file: &ParsedFile, node: Node, out: &mut Vec<DjangoMiddleware>) {
    if node.kind() == "assignment" {
        if let Some(mw) = extract_django_middleware(file, node) {
            out.push(mw);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_django_middleware(file, child, out);
    }
}

fn extract_django_middleware(file: &ParsedFile, node: Node) -> Option<DjangoMiddleware> {
    let left = node.child_by_field_name("left")?;
    let right = node.child_by_field_name("right")?;

    if left.kind() != "identifier" {
        return None;
    }

    let var_name = file.text_for_node(&left);

    // Check for Middleware instantiation: MIDDLEWARE = SomeMiddleware(...)
    if right.kind() == "call" {
        let function = right.child_by_field_name("function")?;
        let func_name = file.text_for_node(&function);
        if func_name.contains("Middleware") {
            let location = file.location_for_node(&right);
            return Some(DjangoMiddleware {
                var_name,
                middleware_type: func_name,
                location,
            });
        }
    }

    // Check for list of middleware paths: MIDDLEWARE = ['path.to.Middleware', ...]
    if right.kind() == "list" {
        let source_bytes = file.source.as_bytes();
        let mut cursor = right.walk();
        for child in right.children(&mut cursor) {
            if child.kind() == "string" {
                let text = child.utf8_text(source_bytes).ok()?.to_string();
                if text.contains("Middleware") {
                    let location = file.location_for_node(&right);
                    return Some(DjangoMiddleware {
                        var_name,
                        middleware_type: text,
                        location,
                    });
                }
            }
        }
    }

    None
}

fn collect_django_models(file: &ParsedFile, node: Node, out: &mut Vec<DjangoModel>) {
    if node.kind() == "class_definition" {
        if let Some(model) = extract_django_model(file, node) {
            out.push(model);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_django_models(file, child, out);
    }
}

fn extract_django_model(file: &ParsedFile, node: Node) -> Option<DjangoModel> {
    let name_node = node.child_by_field_name("name")?;
    let name = file.text_for_node(&name_node);

    let mut base_classes = Vec::new();
    if let Some(superclasses) = node.child_by_field_name("superclasses") {
        let child_count = superclasses.named_child_count();
        for i in 0..child_count {
            if let Some(base) = superclasses.named_child(i) {
                let base_text = file.text_for_node(&base);
                if !base_text.is_empty() {
                    base_classes.push(base_text);
                }
            }
        }
    }

    let has_model_base = base_classes.iter().any(|bc| {
        bc.contains("Model") || bc.contains("models.Model") || bc.contains("models.AbstractUser")
    });

    if !has_model_base {
        return None;
    }

    let location = file.location_for_node(&node);

    Some(DjangoModel {
        name,
        base_classes,
        location,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::python::parse_python_file;
    use crate::types::context::{Language, SourceFile};

    fn parse_and_summarize_django(source: &str) -> Option<DjangoFileSummary> {
        let sf = SourceFile {
            path: "test.py".to_string(),
            language: Language::Python,
            content: source.to_string(),
        };
        let parsed = parse_python_file(FileId(1), &sf).expect("parsing should succeed");
        summarize_django(&parsed)
    }

    #[test]
    fn detects_django_model() {
        let src = r#"
from django.db import models

class User(models.Model):
    name = models.CharField(max_length=100)
"#;
        let summary = parse_and_summarize_django(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.models.len(), 1);
        assert_eq!(summary.models[0].name, "User");
    }

    #[test]
    fn detects_django_view_function() {
        let src = r#"
from django.http import HttpResponse

def home(request):
    return HttpResponse("Hello")
"#;
        let summary = parse_and_summarize_django(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.views.len(), 1);
        assert_eq!(summary.views[0].name, "home");
    }

    #[test]
    fn detects_django_url_pattern() {
        let src = r#"
from django.urls import path
from . import views

urlpatterns = [
    path('', views.home, name='home'),
]
"#;
        let summary = parse_and_summarize_django(src);
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.urls.len(), 1);
        assert!(summary.urls[0].view_name.contains("views.home"));
    }

    #[test]
    fn detects_django_middleware() {
        let src = r#"
from django.middleware.security import SecurityMiddleware

SECURITY_MIDDLEWARE = [
    'django.middleware.security.SecurityMiddleware',
]
"#;
        let summary = parse_and_summarize_django(src);
        assert!(summary.is_some());
    }

    #[test]
    fn does_not_detect_flask_app() {
        let src = r#"
from flask import Flask

app = Flask(__name__)
"#;
        let summary = parse_and_summarize_django(src);
        assert!(summary.is_none());
    }

    #[test]
    fn does_not_detect_fastapi_app() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()
"#;
        let summary = parse_and_summarize_django(src);
        assert!(summary.is_none());
    }
}
