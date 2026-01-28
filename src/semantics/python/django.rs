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
    #[serde(default)]
    pub http_methods: Vec<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
    if node.kind() == "decorated_definition" {
        if let Some(view) = extract_django_view_from_decorated_definition(file, node) {
            out.push(view);
        }
    } else if node.kind() == "function_definition" || node.kind() == "async_function_definition" {
        if !is_inside_class_definition(node) && !is_under_decorated_definition(node) {
            if let Some(view) = extract_django_view(file, node) {
                out.push(view);
            }
        }
    } else if node.kind() == "class_definition" {
        if let Some(view) = extract_django_class_view(file, node) {
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

    let http_methods = finalize_http_methods(detect_http_methods_from_decorators(file, &[]));
    let http_method = summarize_http_method(&http_methods);

    Some(DjangoView {
        name,
        http_method,
        http_methods,
        path: None,
        is_async,
        has_try_except,
        location,
        body_start_byte: body.start_byte(),
        body_end_byte: body.end_byte(),
    })
}

fn extract_django_view_from_decorated_definition(
    file: &ParsedFile,
    node: Node,
) -> Option<DjangoView> {
    if is_inside_class_definition(node) {
        return None;
    }

    let mut decorators = Vec::new();
    let mut func_def: Option<Node> = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "decorator" => decorators.push(child),
            "function_definition" | "async_function_definition" => func_def = Some(child),
            _ => {}
        }
    }

    let func_def = func_def?;
    let name_node = func_def.child_by_field_name("name")?;
    let name = file.text_for_node(&name_node);
    let body = func_def.child_by_field_name("body")?;
    let fn_text = file.text_for_node(&func_def);
    let is_async = fn_text.trim_start().starts_with("async def");
    let has_try_except = body_has_try_except(body);
    let location = file.location_for_node(&func_def);

    let http_methods =
        finalize_http_methods(detect_http_methods_from_decorators(file, &decorators));
    let http_method = summarize_http_method(&http_methods);

    Some(DjangoView {
        name,
        http_method,
        http_methods,
        path: None,
        is_async,
        has_try_except,
        location,
        body_start_byte: body.start_byte(),
        body_end_byte: body.end_byte(),
    })
}

fn extract_django_class_view(file: &ParsedFile, node: Node) -> Option<DjangoView> {
    // Only consider classes that look like Django/DRF views.
    let superclasses = node.child_by_field_name("superclasses")?;
    let mut bases = Vec::new();
    let child_count = superclasses.named_child_count();
    for i in 0..child_count {
        if let Some(base) = superclasses.named_child(i) {
            let t = file.text_for_node(&base);
            if !t.is_empty() {
                bases.push(t);
            }
        }
    }
    let looks_like_view = bases.iter().any(|b| {
        b.ends_with("View")
            || b.contains("APIView")
            || b.contains("GenericAPIView")
            || b.contains("ViewSet")
    });
    if !looks_like_view {
        return None;
    }

    let name_node = node.child_by_field_name("name")?;
    let name = file.text_for_node(&name_node);
    let body = node.child_by_field_name("body")?;

    let mut http_methods: Vec<String> = Vec::new();
    let mut is_async = false;
    let mut has_try_except = false;

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        match child.kind() {
            "function_definition" | "async_function_definition" => {
                let m_name = child
                    .child_by_field_name("name")
                    .map(|n| file.text_for_node(&n))
                    .unwrap_or_default();
                if let Some(method) = http_method_from_view_method_name(&m_name) {
                    http_methods.push(method);
                    let txt = file.text_for_node(&child);
                    if txt.trim_start().starts_with("async def") {
                        is_async = true;
                    }
                    if let Some(m_body) = child.child_by_field_name("body") {
                        if body_has_try_except(m_body) {
                            has_try_except = true;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Dedupe in encounter order.
    let mut seen = std::collections::HashSet::new();
    http_methods.retain(|m| seen.insert(m.clone()));
    if http_methods.is_empty() {
        return None;
    }

    let http_method = summarize_http_method(&http_methods);
    let location = file.location_for_node(&node);

    Some(DjangoView {
        name,
        http_method,
        http_methods,
        path: None,
        is_async,
        has_try_except,
        location,
        body_start_byte: body.start_byte(),
        body_end_byte: body.end_byte(),
    })
}

fn http_method_from_view_method_name(name: &str) -> Option<String> {
    match name {
        "get" => Some("GET".to_string()),
        "post" => Some("POST".to_string()),
        "put" => Some("PUT".to_string()),
        "patch" => Some("PATCH".to_string()),
        "delete" => Some("DELETE".to_string()),
        "head" => Some("HEAD".to_string()),
        "options" => Some("OPTIONS".to_string()),
        _ => None,
    }
}

fn summarize_http_method(methods: &[String]) -> String {
    if methods.len() == 1 {
        return methods[0].clone();
    }
    if methods.is_empty() {
        return "GET".to_string();
    }
    "ANY".to_string()
}

fn finalize_http_methods(mut methods: Vec<String>) -> Vec<String> {
    if methods.is_empty() {
        methods.push("GET".to_string());
    }
    methods
}

fn detect_http_methods_from_decorators(file: &ParsedFile, decorators: &[Node]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();

    for dec in decorators {
        if let Some(mut ms) = extract_http_methods_from_decorator(file, *dec) {
            out.append(&mut ms);
        }
    }

    // Dedupe in encounter order.
    let mut seen = std::collections::HashSet::new();
    out.retain(|m| seen.insert(m.clone()));
    out
}

fn extract_http_methods_from_decorator(file: &ParsedFile, decorator: Node) -> Option<Vec<String>> {
    // Decorator structure:
    // (decorator "@" <expression>)
    let expr = first_named_child(decorator)?;

    match expr.kind() {
        "identifier" | "attribute" => {
            let callee_expr = file.text_for_node(&expr);
            let callee = last_path_segment(&callee_expr).to_string();
            methods_for_simple_decorator(&callee)
        }
        "call" => {
            let func = expr.child_by_field_name("function")?;
            let callee_expr = file.text_for_node(&func);
            let callee = last_path_segment(&callee_expr).to_string();
            let args = expr.child_by_field_name("arguments");

            match callee.as_str() {
                "require_http_methods" => {
                    let list = args.and_then(first_arg_list_or_tuple);
                    list.and_then(|n| extract_string_list(file, n))
                }
                "api_view" => {
                    let list = args.and_then(first_arg_list_or_tuple);
                    list.and_then(|n| extract_string_list(file, n))
                }
                "action" => {
                    // DRF viewsets: @action(methods=["get", "post"], ...)
                    let args = args?;
                    let list = extract_keyword_list_or_tuple(file, args, "methods")
                        .or_else(|| extract_keyword_list_or_tuple(file, args, "method"));
                    list.and_then(|n| extract_string_list(file, n))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn methods_for_simple_decorator(callee: &str) -> Option<Vec<String>> {
    match callee {
        "require_GET" => Some(vec!["GET".to_string()]),
        "require_POST" => Some(vec!["POST".to_string()]),
        "require_safe" => Some(vec!["GET".to_string(), "HEAD".to_string()]),
        _ => None,
    }
}

fn first_named_child(node: Node) -> Option<Node> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).next()
}

fn last_path_segment(expr: &str) -> &str {
    expr.rsplit('.').next().unwrap_or(expr)
}

fn first_arg_list_or_tuple(args: Node) -> Option<Node> {
    let mut cursor = args.walk();
    for child in args.named_children(&mut cursor) {
        match child.kind() {
            "list" | "tuple" => return Some(child),
            // allow passing a name, but we won't resolve it
            _ => continue,
        }
    }
    None
}

fn extract_keyword_list_or_tuple<'a>(
    file: &ParsedFile,
    args: Node<'a>,
    keyword: &str,
) -> Option<Node<'a>> {
    let source_bytes = file.source.as_bytes();
    let mut cursor = args.walk();
    for child in args.named_children(&mut cursor) {
        if child.kind() != "keyword_argument" {
            continue;
        }
        let name_node = child.child_by_field_name("name")?;
        let name = name_node.utf8_text(source_bytes).ok()?;
        if name != keyword {
            continue;
        }
        let value = child.child_by_field_name("value")?;
        if matches!(value.kind(), "list" | "tuple") {
            return Some(value);
        }
    }
    None
}

fn extract_string_list(file: &ParsedFile, list: Node) -> Option<Vec<String>> {
    let source_bytes = file.source.as_bytes();
    let mut out: Vec<String> = Vec::new();
    let mut cursor = list.walk();
    for child in list.named_children(&mut cursor) {
        if child.kind() != "string" {
            continue;
        }
        let text = child.utf8_text(source_bytes).ok()?;
        let s = text.trim_matches(|c| c == '"' || c == '\'');
        if s.is_empty() {
            continue;
        }
        out.push(s.to_uppercase());
    }
    if out.is_empty() { None } else { Some(out) }
}

fn is_inside_class_definition(node: Node) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "class_definition" {
            return true;
        }
        if n.kind() == "module" {
            break;
        }
        current = n.parent();
    }
    false
}

fn is_under_decorated_definition(node: Node) -> bool {
    node.parent()
        .is_some_and(|p| p.kind() == "decorated_definition")
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

    let mut view_type = match method_name.as_str() {
        "include" => ViewType::Include,
        _ => ViewType::Function,
    };

    let args = node.child_by_field_name("arguments")?;
    let mut args_cursor = args.walk();
    let mut path_expr = String::new();
    let mut view_name = String::new();
    let mut name: Option<String> = None;
    let mut arg_count = 0;

    for child in args.children(&mut args_cursor) {
        match child.kind() {
            "(" | ")" | "," => continue,
            "keyword_argument" => {
                // name='route_name'
                let kw_name = child.child_by_field_name("name");
                let kw_value = child.child_by_field_name("value");
                if let (Some(n), Some(v)) = (kw_name, kw_value) {
                    if n.utf8_text(source_bytes).ok()? == "name" {
                        if v.kind() == "string" {
                            let t = v.utf8_text(source_bytes).ok()?;
                            name = Some(t.trim_matches(|c| c == '\'' || c == '"').to_string());
                        }
                    }
                }
                continue;
            }
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

    // Refine view type based on common call shapes in urlpatterns.
    let view_trim = view_name.replace(' ', "");
    if view_trim.contains(".as_view(") || view_trim.ends_with(".as_view()") {
        view_type = ViewType::Class;
    }
    if view_trim.starts_with("include(") {
        view_type = ViewType::Include;
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
        assert_eq!(summary.views[0].http_method, "GET");
        assert_eq!(summary.views[0].http_methods, vec!["GET".to_string()]);
    }

    #[test]
    fn infers_methods_from_require_http_methods_decorator() {
        let src = r#"
from django.views.decorators.http import require_http_methods

@require_http_methods(["GET", "POST"])
def home(request):
    return None
"#;
        let summary = parse_and_summarize_django(src).unwrap();
        assert_eq!(summary.views.len(), 1);
        assert_eq!(summary.views[0].http_method, "ANY");
        assert_eq!(
            summary.views[0].http_methods,
            vec!["GET".to_string(), "POST".to_string()]
        );
    }

    #[test]
    fn infers_methods_from_require_post_decorator() {
        let src = r#"
from django.views.decorators.http import require_POST

@require_POST
def submit(request):
    return None
"#;
        let summary = parse_and_summarize_django(src).unwrap();
        assert_eq!(summary.views.len(), 1);
        assert_eq!(summary.views[0].http_method, "POST");
        assert_eq!(summary.views[0].http_methods, vec!["POST".to_string()]);
    }

    #[test]
    fn infers_methods_from_drf_api_view_decorator() {
        let src = r#"
from rest_framework.decorators import api_view

@api_view(["get", "post"])
def home(request):
    return None
"#;
        let summary = parse_and_summarize_django(src).unwrap();
        assert_eq!(summary.views.len(), 1);
        assert_eq!(summary.views[0].http_method, "ANY");
        assert_eq!(
            summary.views[0].http_methods,
            vec!["GET".to_string(), "POST".to_string()]
        );
    }

    #[test]
    fn detects_class_based_view_http_methods() {
        let src = r#"
from django.views import View

class MyView(View):
    def get(self, request):
        return None

    def post(self, request):
        return None
"#;
        let summary = parse_and_summarize_django(src).unwrap();
        assert_eq!(summary.views.len(), 1);
        assert_eq!(summary.views[0].name, "MyView");
        assert_eq!(summary.views[0].http_method, "ANY");
        assert_eq!(
            summary.views[0].http_methods,
            vec!["GET".to_string(), "POST".to_string()]
        );
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
        assert_eq!(summary.urls[0].name, Some("home".to_string()));
    }

    #[test]
    fn detects_class_based_url_pattern() {
        let src = r#"
from django.urls import path
from .views import MyView

urlpatterns = [
    path('x/', MyView.as_view(), name='x'),
]
"#;
        let summary = parse_and_summarize_django(src).unwrap();
        assert_eq!(summary.urls.len(), 1);
        assert_eq!(summary.urls[0].view_type, ViewType::Class);
        assert!(summary.urls[0].view_name.contains("MyView.as_view"));
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
