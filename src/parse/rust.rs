//! Rust parser using tree-sitter-rust grammar.
//!
//! This module provides parsing capabilities for Rust source files,
//! producing a tree-sitter AST that can be used for semantic analysis.

use std::sync::Arc;

use anyhow::Result;
use tree_sitter::{Language as TsLanguage, Parser};

use crate::parse::ast::{FileId, ParsedFile};
use crate::types::context::{Language, SourceFile};

fn rust_language() -> TsLanguage {
    tree_sitter_rust::LANGUAGE.into()
}

/// Parse a Rust source file into a `ParsedFile`.
pub fn parse_rust_file(file_id: FileId, sf: &SourceFile) -> Result<ParsedFile> {
    let mut parser = Parser::new();
    parser.set_language(&rust_language())?;

    let source = Arc::new(sf.content.clone());
    let tree = parser
        .parse(&*source, None)
        .ok_or_else(|| anyhow::anyhow!("failed to parse Rust source"))?;

    Ok(ParsedFile {
        file_id,
        path: sf.path.clone(),
        language: Language::Rust,
        source,
        tree,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rust_source_file(path: &str, content: &str) -> SourceFile {
        SourceFile {
            path: path.to_string(),
            language: Language::Rust,
            content: content.to_string(),
        }
    }

    #[test]
    fn test_rust_language_returns_valid_language() {
        let lang = rust_language();
        assert!(lang.abi_version() > 0);
    }

    #[test]
    fn test_parse_simple_function() {
        let sf = make_rust_source_file("test.rs", "fn main() {}");
        let result = parse_rust_file(FileId(1), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.tree.root_node().kind(), "source_file");
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_hello_world() {
        let code = r#"
fn main() {
    println!("Hello, World!");
}
"#;
        let sf = make_rust_source_file("main.rs", code);
        let result = parse_rust_file(FileId(2), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        let root = parsed.tree.root_node();
        assert_eq!(root.kind(), "source_file");
        assert!(!root.has_error());
    }

    #[test]
    fn test_parse_async_function() {
        let code = r#"
async fn fetch_data() -> Result<String, Error> {
    let response = client.get(url).await?;
    Ok(response.text().await?)
}
"#;
        let sf = make_rust_source_file("async.rs", code);
        let result = parse_rust_file(FileId(3), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_struct_definition() {
        let code = r#"
#[derive(Debug, Clone)]
pub struct User {
    pub id: u64,
    pub name: String,
    pub email: Option<String>,
}
"#;
        let sf = make_rust_source_file("types.rs", code);
        let result = parse_rust_file(FileId(4), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_impl_block() {
        let code = r#"
struct Counter {
    value: i32,
}

impl Counter {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn increment(&mut self) {
        self.value += 1;
    }

    pub fn value(&self) -> i32 {
        self.value
    }
}
"#;
        let sf = make_rust_source_file("impl.rs", code);
        let result = parse_rust_file(FileId(5), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_trait_definition() {
        let code = r#"
pub trait Repository {
    type Error;
    
    fn find_by_id(&self, id: u64) -> Result<Option<Self>, Self::Error>
    where
        Self: Sized;
    
    fn save(&mut self, entity: &Self) -> Result<(), Self::Error>;
}
"#;
        let sf = make_rust_source_file("trait.rs", code);
        let result = parse_rust_file(FileId(6), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_use_statements() {
        let code = r#"
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use crate::models::User;
use super::config::Config;
"#;
        let sf = make_rust_source_file("uses.rs", code);
        let result = parse_rust_file(FileId(7), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_enum_definition() {
        let code = r#"
#[derive(Debug)]
pub enum Status {
    Pending,
    Active { since: DateTime },
    Completed(Result<(), Error>),
    Failed {
        error: String,
        retries: u32,
    },
}
"#;
        let sf = make_rust_source_file("enum.rs", code);
        let result = parse_rust_file(FileId(8), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_macro_invocations() {
        let code = r#"
fn main() {
    println!("Debug: {:?}", value);
    eprintln!("Error occurred");
    vec![1, 2, 3];
    format!("Hello, {}", name);
    panic!("This should not happen");
}
"#;
        let sf = make_rust_source_file("macros.rs", code);
        let result = parse_rust_file(FileId(9), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_error_handling() {
        let code = r#"
fn process() -> Result<String, Error> {
    let data = read_file()?.parse()?;
    let result = data.unwrap();
    let safe = data.unwrap_or_default();
    let explicit = data.expect("should have data");
    Ok(result)
}
"#;
        let sf = make_rust_source_file("errors.rs", code);
        let result = parse_rust_file(FileId(10), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_closures() {
        let code = r#"
fn main() {
    let add = |a, b| a + b;
    let complex = |x: i32| -> i32 {
        x * 2
    };
    let moved = move || {
        println!("{}", captured);
    };
    items.iter().map(|x| x * 2).collect::<Vec<_>>();
}
"#;
        let sf = make_rust_source_file("closures.rs", code);
        let result = parse_rust_file(FileId(11), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_unsafe_block() {
        let code = r#"
fn dangerous() {
    unsafe {
        let ptr = &value as *const i32;
        let dereferenced = *ptr;
    }
}

unsafe fn unsafe_fn() {}
"#;
        let sf = make_rust_source_file("unsafe.rs", code);
        let result = parse_rust_file(FileId(12), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_tokio_async() {
        let code = r#"
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::channel(100);
    
    tokio::spawn(async move {
        tx.send("hello").await.unwrap();
    });
    
    while let Some(msg) = rx.recv().await {
        println!("{}", msg);
    }
}
"#;
        let sf = make_rust_source_file("tokio.rs", code);
        let result = parse_rust_file(FileId(13), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_lifetimes() {
        let code = r#"
struct Borrowed<'a> {
    data: &'a str,
}

impl<'a> Borrowed<'a> {
    fn new(data: &'a str) -> Self {
        Self { data }
    }
}

fn longest<'a>(x: &'a str, y: &'a str) -> &'a str {
    if x.len() > y.len() { x } else { y }
}
"#;
        let sf = make_rust_source_file("lifetimes.rs", code);
        let result = parse_rust_file(FileId(14), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_generics() {
        let code = r#"
struct Container<T> {
    value: T,
}

impl<T: Clone> Container<T> {
    fn get(&self) -> T {
        self.value.clone()
    }
}

fn process<T, E>(input: Result<T, E>) -> Option<T>
where
    T: Default,
    E: std::fmt::Debug,
{
    input.ok()
}
"#;
        let sf = make_rust_source_file("generics.rs", code);
        let result = parse_rust_file(FileId(15), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_empty_file() {
        let sf = make_rust_source_file("empty.rs", "");
        let result = parse_rust_file(FileId(16), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.tree.root_node().kind(), "source_file");
    }

    #[test]
    fn test_parse_syntax_error_tolerant() {
        let code = "fn broken(";
        let sf = make_rust_source_file("broken.rs", code);
        let result = parse_rust_file(FileId(17), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        // tree-sitter is error-tolerant
        assert!(parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_source_content_preserved() {
        let code = "fn main() { let x = 42; }\n";
        let sf = make_rust_source_file("content.rs", code);
        let result = parse_rust_file(FileId(18), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.source.as_str(), code);
    }

    #[test]
    fn test_file_id_preserved() {
        let sf = make_rust_source_file("test.rs", "fn main() {}");
        let file_id = FileId(99999);
        let result = parse_rust_file(file_id, &sf);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().file_id, file_id);
    }

    #[test]
    fn test_path_preserved() {
        let path = "some/deep/nested/path/module.rs";
        let sf = make_rust_source_file(path, "fn main() {}");
        let result = parse_rust_file(FileId(19), &sf);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().path, path);
    }

    #[test]
    fn test_language_is_rust() {
        let sf = make_rust_source_file("test.rs", "fn main() {}");
        let result = parse_rust_file(FileId(20), &sf);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().language, Language::Rust);
    }

    #[test]
    fn test_parse_attributes() {
        let code = r#"
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default)]
    pub timeout: u64,
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_something() {}
}
"#;
        let sf = make_rust_source_file("attrs.rs", code);
        let result = parse_rust_file(FileId(21), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_axum_handler() {
        let code = r#"
use axum::{
    extract::{Path, State},
    response::Json,
    routing::get,
    Router,
};

async fn get_user(
    State(pool): State<PgPool>,
    Path(id): Path<u64>,
) -> Result<Json<User>, AppError> {
    let user = sqlx::query_as!(User, "SELECT * FROM users WHERE id = $1", id)
        .fetch_one(&pool)
        .await?;
    Ok(Json(user))
}
"#;
        let sf = make_rust_source_file("handler.rs", code);
        let result = parse_rust_file(FileId(22), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_pattern_matching() {
        let code = r#"
fn process(value: Option<Result<i32, Error>>) {
    match value {
        Some(Ok(n)) if n > 0 => println!("positive: {}", n),
        Some(Ok(n)) => println!("non-positive: {}", n),
        Some(Err(e)) => eprintln!("error: {:?}", e),
        None => println!("nothing"),
    }
    
    if let Some(Ok(n)) = value {
        println!("{}", n);
    }
    
    let Some(inner) = value else {
        return;
    };
}
"#;
        let sf = make_rust_source_file("match.rs", code);
        let result = parse_rust_file(FileId(23), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }
}
