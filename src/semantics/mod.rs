pub mod common;
mod common_impl; // CommonSemantics trait implementations
pub mod go;
pub mod python;
pub mod rust;
pub mod typescript;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::parse::ast::{FileId, ParsedFile};
use crate::types::context::Language;
use go::model::GoFileSemantics;
use python::model::PyFileSemantics;
use rust::model::RustFileSemantics;
use typescript::model::TsFileSemantics;

// Re-export common types for convenience
pub use common::{
    CommonSemantics, async_ops::AsyncOperation, db::DbOperation, functions::FunctionDef,
    http::HttpCall, imports::Import,
};

/// Language-agnostic wrapper for per-file semantics.
///
/// Each variant contains the rich, language-specific model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SourceSemantics {
    Python(PyFileSemantics),
    Go(GoFileSemantics),
    Rust(RustFileSemantics),
    Typescript(TsFileSemantics),
    // Java(JavaFileSemantics),
}

impl SourceSemantics {
    /// Get the language of this file
    pub fn language(&self) -> Language {
        match self {
            SourceSemantics::Python(_) => Language::Python,
            SourceSemantics::Go(_) => Language::Go,
            SourceSemantics::Rust(_) => Language::Rust,
            SourceSemantics::Typescript(_) => Language::Typescript,
        }
    }

    /// Get the file ID
    pub fn file_id(&self) -> FileId {
        match self {
            SourceSemantics::Python(sem) => sem.file_id,
            SourceSemantics::Go(sem) => sem.file_id,
            SourceSemantics::Rust(sem) => sem.file_id,
            SourceSemantics::Typescript(sem) => sem.file_id,
        }
    }

    /// Get the file path
    pub fn file_path(&self) -> &str {
        match self {
            SourceSemantics::Python(sem) => &sem.path,
            SourceSemantics::Go(sem) => &sem.path,
            SourceSemantics::Rust(sem) => &sem.path,
            SourceSemantics::Typescript(sem) => &sem.path,
        }
    }

    /// Get the inner Python semantics if this is Python
    pub fn as_python(&self) -> Option<&PyFileSemantics> {
        match self {
            SourceSemantics::Python(sem) => Some(sem),
            _ => None,
        }
    }

    /// Get the inner Go semantics if this is Go
    pub fn as_go(&self) -> Option<&GoFileSemantics> {
        match self {
            SourceSemantics::Go(sem) => Some(sem),
            _ => None,
        }
    }

    /// Get the inner Rust semantics if this is Rust
    pub fn as_rust(&self) -> Option<&RustFileSemantics> {
        match self {
            SourceSemantics::Rust(sem) => Some(sem),
            _ => None,
        }
    }

    /// Get the inner TypeScript semantics if this is TypeScript
    pub fn as_typescript(&self) -> Option<&TsFileSemantics> {
        match self {
            SourceSemantics::Typescript(sem) => Some(sem),
            _ => None,
        }
    }
}

// NOTE: CommonSemantics trait implementation is on hold until language-specific
// semantics (PyFileSemantics, etc.) are refactored to produce common types.
// For now, rules access language-specific semantics directly via as_python() etc.
//
// Future: Each language will implement CommonSemantics, allowing cross-language rules:
// ```
// impl CommonSemantics for PyFileSemantics { ... }
// impl CommonSemantics for GoFileSemantics { ... }
// ```

/// Build semantics for a parsed file, if supported.
///
/// Returns:
/// - Ok(Some(SourceSemantics)) if we know how to analyze this language.
/// - Ok(None) if the language is not yet supported.
/// - Err(_) if something went wrong building semantics.
pub fn build_source_semantics(parsed: &ParsedFile) -> Result<Option<SourceSemantics>> {
    match parsed.language {
        Language::Python => {
            let mut sem = PyFileSemantics::from_parsed(parsed);
            sem.analyze_frameworks(parsed)?;
            Ok(Some(SourceSemantics::Python(sem)))
        }
        Language::Go => {
            let sem = go::build_go_semantics(parsed)?;
            Ok(Some(SourceSemantics::Go(sem)))
        }
        Language::Rust => {
            let sem = rust::build_rust_semantics(parsed)?;
            Ok(Some(SourceSemantics::Rust(sem)))
        }
        Language::Typescript => {
            let sem = typescript::build_typescript_semantics(parsed)?;
            Ok(Some(SourceSemantics::Typescript(sem)))
        }
        // Later:
        // Language::Java => { ... }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::go::parse_go_file;
    use crate::parse::python::parse_python_file;
    use crate::types::context::SourceFile;

    fn make_source_file(path: &str, language: Language, content: &str) -> SourceFile {
        SourceFile {
            path: path.to_string(),
            language,
            content: content.to_string(),
        }
    }

    // ==================== SourceSemantics Tests ====================

    #[test]
    fn source_semantics_language_returns_python() {
        let sf = make_source_file("test.py", Language::Python, "x = 1");
        let parsed = parse_python_file(FileId(1), &sf).expect("parsing should succeed");
        let mut sem = PyFileSemantics::from_parsed(&parsed);
        sem.analyze_frameworks(&parsed)
            .expect("framework analysis should succeed");

        let source_sem = SourceSemantics::Python(sem);
        assert_eq!(source_sem.language(), Language::Python);
    }

    #[test]
    fn source_semantics_language_returns_go() {
        let sf = make_source_file("test.go", Language::Go, "package main");
        let parsed = parse_go_file(FileId(1), &sf).expect("parsing should succeed");
        let sem = go::build_go_semantics(&parsed).expect("semantics building should succeed");

        let source_sem = SourceSemantics::Go(sem);
        assert_eq!(source_sem.language(), Language::Go);
    }

    #[test]
    fn source_semantics_debug_impl() {
        let sf = make_source_file("test.py", Language::Python, "x = 1");
        let parsed = parse_python_file(FileId(1), &sf).expect("parsing should succeed");
        let sem = PyFileSemantics::from_parsed(&parsed);

        let source_sem = SourceSemantics::Python(sem);
        let debug_str = format!("{:?}", source_sem);
        assert!(debug_str.contains("Python"));
    }

    #[test]
    fn source_semantics_clone() {
        let sf = make_source_file("test.py", Language::Python, "x = 1");
        let parsed = parse_python_file(FileId(1), &sf).expect("parsing should succeed");
        let sem = PyFileSemantics::from_parsed(&parsed);

        let source_sem = SourceSemantics::Python(sem);
        let cloned = source_sem.clone();
        assert_eq!(source_sem.language(), cloned.language());
    }

    // ==================== build_source_semantics Tests ====================

    #[test]
    fn build_source_semantics_python_returns_some() {
        let sf = make_source_file("test.py", Language::Python, "x = 1");
        let parsed = parse_python_file(FileId(1), &sf).expect("parsing should succeed");

        let result = build_source_semantics(&parsed);
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn build_source_semantics_python_returns_python_variant() {
        let sf = make_source_file("test.py", Language::Python, "x = 1");
        let parsed = parse_python_file(FileId(1), &sf).expect("parsing should succeed");

        let result = build_source_semantics(&parsed).unwrap().unwrap();
        assert!(matches!(result, SourceSemantics::Python(_)));
    }

    #[test]
    fn build_source_semantics_unsupported_language_returns_none() {
        // Create a parsed file but manually set language to something unsupported
        let sf = make_source_file("test.py", Language::Python, "x = 1");
        let mut parsed = parse_python_file(FileId(1), &sf).expect("parsing should succeed");
        parsed.language = Language::Java; // Override to unsupported

        let result = build_source_semantics(&parsed);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn build_source_semantics_go_returns_some() {
        let sf = make_source_file("test.go", Language::Go, "package main");
        let parsed = parse_go_file(FileId(1), &sf).expect("parsing should succeed");

        let result = build_source_semantics(&parsed);
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn build_source_semantics_go_returns_go_variant() {
        let sf = make_source_file("test.go", Language::Go, "package main\n\nfunc main() {}");
        let parsed = parse_go_file(FileId(1), &sf).expect("parsing should succeed");

        let result = build_source_semantics(&parsed).unwrap().unwrap();
        assert!(matches!(result, SourceSemantics::Go(_)));
    }

    #[test]
    fn source_semantics_as_go_returns_some_for_go() {
        let sf = make_source_file("test.go", Language::Go, "package main");
        let parsed = parse_go_file(FileId(1), &sf).expect("parsing should succeed");
        let sem = go::build_go_semantics(&parsed).expect("semantics building should succeed");

        let source_sem = SourceSemantics::Go(sem);
        assert!(source_sem.as_go().is_some());
        assert!(source_sem.as_python().is_none());
    }

    #[test]
    fn source_semantics_as_python_returns_none_for_go() {
        let sf = make_source_file("test.go", Language::Go, "package main");
        let parsed = parse_go_file(FileId(1), &sf).expect("parsing should succeed");
        let sem = go::build_go_semantics(&parsed).expect("semantics building should succeed");

        let source_sem = SourceSemantics::Go(sem);
        assert!(source_sem.as_python().is_none());
    }

    #[test]
    fn build_source_semantics_java_returns_none() {
        let sf = make_source_file("test.py", Language::Python, "x = 1");
        let mut parsed = parse_python_file(FileId(1), &sf).expect("parsing should succeed");
        parsed.language = Language::Java;

        let result = build_source_semantics(&parsed);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn build_source_semantics_typescript_returns_some() {
        use crate::parse::typescript::parse_typescript_file;

        let sf = make_source_file("test.ts", Language::Typescript, "const x = 1;");
        let parsed = parse_typescript_file(FileId(1), &sf).expect("parsing should succeed");

        let result = build_source_semantics(&parsed);
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn build_source_semantics_typescript_returns_typescript_variant() {
        use crate::parse::typescript::parse_typescript_file;

        let sf = make_source_file("test.ts", Language::Typescript, "const x = 1;");
        let parsed = parse_typescript_file(FileId(1), &sf).expect("parsing should succeed");

        let result = build_source_semantics(&parsed).unwrap().unwrap();
        assert!(matches!(result, SourceSemantics::Typescript(_)));
    }

    #[test]
    fn source_semantics_as_typescript_returns_some_for_typescript() {
        use crate::parse::typescript::parse_typescript_file;

        let sf = make_source_file("test.ts", Language::Typescript, "const x = 1;");
        let parsed = parse_typescript_file(FileId(1), &sf).expect("parsing should succeed");
        let sem = typescript::build_typescript_semantics(&parsed)
            .expect("semantics building should succeed");

        let source_sem = SourceSemantics::Typescript(sem);
        assert!(source_sem.as_typescript().is_some());
        assert!(source_sem.as_python().is_none());
        assert!(source_sem.as_go().is_none());
        assert!(source_sem.as_rust().is_none());
    }

    #[test]
    fn build_source_semantics_javascript_returns_none() {
        let sf = make_source_file("test.py", Language::Python, "x = 1");
        let mut parsed = parse_python_file(FileId(1), &sf).expect("parsing should succeed");
        parsed.language = Language::Javascript;

        let result = build_source_semantics(&parsed);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn build_source_semantics_with_fastapi_code() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()
"#;
        let sf = make_source_file("main.py", Language::Python, src);
        let parsed = parse_python_file(FileId(1), &sf).expect("parsing should succeed");

        let result = build_source_semantics(&parsed).unwrap().unwrap();
        if let SourceSemantics::Python(py_sem) = result {
            assert!(py_sem.fastapi.is_some());
        } else {
            panic!("Expected Python semantics");
        }
    }

    #[test]
    fn build_source_semantics_with_http_calls() {
        let src = "requests.get('https://example.com')";
        let sf = make_source_file("client.py", Language::Python, src);
        let parsed = parse_python_file(FileId(1), &sf).expect("parsing should succeed");

        let result = build_source_semantics(&parsed).unwrap().unwrap();
        if let SourceSemantics::Python(py_sem) = result {
            assert!(!py_sem.http_calls.is_empty());
        } else {
            panic!("Expected Python semantics");
        }
    }
}
