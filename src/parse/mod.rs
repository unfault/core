pub mod ast;
pub mod go;
pub mod python;
pub mod rust;
pub mod typescript;

use crate::error::ParseError;
use crate::types::context::{Language, SourceFile};
use ast::{FileId, ParsedFile};

/// Generic entry point: parse a `SourceFile` into a `ParsedFile`.

pub fn parse_source_file(file_id: FileId, sf: &SourceFile) -> Result<ParsedFile, ParseError> {
    match sf.language {
        Language::Python => {
            python::parse_python_file(file_id, sf).map_err(|source| ParseError::File {
                file_path: sf.path.clone(),
                source,
            })
        }
        Language::Go => go::parse_go_file(file_id, sf).map_err(|source| ParseError::File {
            file_path: sf.path.clone(),
            source,
        }),
        Language::Rust => rust::parse_rust_file(file_id, sf).map_err(|source| ParseError::File {
            file_path: sf.path.clone(),
            source,
        }),
        Language::Typescript => {
            typescript::parse_typescript_file(file_id, sf).map_err(|source| ParseError::File {
                file_path: sf.path.clone(),
                source,
            })
        }
        _ => Err(ParseError::File {
            file_path: sf.path.clone(),
            source: anyhow::anyhow!("parsing not yet implemented for this language"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_source_file(path: &str, language: Language, content: &str) -> SourceFile {
        SourceFile {
            path: path.to_string(),
            language,
            content: content.to_string(),
        }
    }

    #[test]
    fn test_parse_python_file_success() {
        let sf = make_source_file("test.py", Language::Python, "x = 1\n");
        let result = parse_source_file(FileId(1), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.file_id, FileId(1));
        assert_eq!(parsed.path, "test.py");
        assert_eq!(parsed.language, Language::Python);
    }

    #[test]
    fn test_parse_python_empty_file() {
        let sf = make_source_file("empty.py", Language::Python, "");
        let result = parse_source_file(FileId(2), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.file_id, FileId(2));
        assert_eq!(parsed.source.as_str(), "");
    }

    #[test]
    fn test_parse_python_complex_code() {
        let code = r#"
import os
from typing import List

class MyClass:
    def __init__(self, value: int):
        self.value = value

    async def async_method(self) -> List[str]:
        return ["hello", "world"]

def main():
    obj = MyClass(42)
    print(obj.value)

if __name__ == "__main__":
    main()
"#;
        let sf = make_source_file("complex.py", Language::Python, code);
        let result = parse_source_file(FileId(3), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.language, Language::Python);
        // Verify tree has root node
        assert_eq!(parsed.tree.root_node().kind(), "module");
    }

    #[test]
    fn test_parse_rust_file_success() {
        let sf = make_source_file("test.rs", Language::Rust, "fn main() {}");
        let result = parse_source_file(FileId(4), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.file_id, FileId(4));
        assert_eq!(parsed.path, "test.rs");
        assert_eq!(parsed.language, Language::Rust);
    }

    #[test]
    fn test_parse_rust_async_function() {
        let sf = make_source_file("async.rs", Language::Rust, "async fn fetch() {}");
        let result = parse_source_file(FileId(5), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.language, Language::Rust);
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_go_file_success() {
        let sf = make_source_file("test.go", Language::Go, "package main\n\nfunc main() {}\n");
        let result = parse_source_file(FileId(5), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.file_id, FileId(5));
        assert_eq!(parsed.path, "test.go");
        assert_eq!(parsed.language, Language::Go);
    }

    #[test]
    fn test_parse_unsupported_language_java() {
        let sf = make_source_file("Test.java", Language::Java, "public class Test {}");
        let result = parse_source_file(FileId(6), &sf);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_typescript_file_success() {
        let sf = make_source_file("test.ts", Language::Typescript, "const x: number = 1;");
        let result = parse_source_file(FileId(7), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.file_id, FileId(7));
        assert_eq!(parsed.path, "test.ts");
        assert_eq!(parsed.language, Language::Typescript);
    }

    #[test]
    fn test_parse_unsupported_language_javascript() {
        let sf = make_source_file("test.js", Language::Javascript, "const x = 1;");
        let result = parse_source_file(FileId(8), &sf);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_python_with_syntax_error() {
        // tree-sitter is error-tolerant, so this should still parse
        let sf = make_source_file("bad.py", Language::Python, "def foo(\n");
        let result = parse_source_file(FileId(9), &sf);
        // tree-sitter parses even invalid syntax, producing error nodes
        assert!(result.is_ok());
        let parsed = result.unwrap();
        // The tree should have error nodes but still be valid
        assert!(parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_python_unicode_content() {
        let code = r#"
# 日本語コメント
message = "こんにちは世界"
emoji = "🎉🚀"
"#;
        let sf = make_source_file("unicode.py", Language::Python, code);
        let result = parse_source_file(FileId(10), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(parsed.source.contains("日本語"));
        assert!(parsed.source.contains("🎉"));
    }

    #[test]
    fn test_file_id_preserved() {
        let sf = make_source_file("test.py", Language::Python, "pass");
        let file_id = FileId(12345);
        let result = parse_source_file(file_id, &sf);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().file_id, file_id);
    }

    #[test]
    fn test_path_preserved() {
        let path = "some/nested/path/to/file.py";
        let sf = make_source_file(path, Language::Python, "pass");
        let result = parse_source_file(FileId(11), &sf);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().path, path);
    }
}
