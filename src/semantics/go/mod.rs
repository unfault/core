//! Go semantic model and framework-specific analysis (net/http, Gin, Echo, etc.)

pub mod frameworks;
pub mod http;
pub mod model;

pub use frameworks::{extract_go_routes, GoFrameworkSummary, GoHttpFramework, GoRoute};
pub use model::GoFileSemantics;

use anyhow::Result;

use crate::parse::ast::ParsedFile;

/// Build the semantic model for a single Go file.
///
/// This is the entry point the engine will call after parsing.
pub fn build_go_semantics(parsed: &ParsedFile) -> Result<GoFileSemantics> {
    let mut sem = GoFileSemantics::from_parsed(parsed);
    sem.analyze_frameworks(parsed)?;
    Ok(sem)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::go::parse_go_file;
    use crate::types::context::{Language, SourceFile};

    /// Helper to parse Go source and build full semantics.
    fn parse_and_build_full_semantics(source: &str) -> GoFileSemantics {
        let sf = SourceFile {
            path: "test.go".to_string(),
            language: Language::Go,
            content: source.to_string(),
        };
        let parsed = parse_go_file(FileId(1), &sf).expect("parsing should succeed");
        build_go_semantics(&parsed).expect("semantics building should succeed")
    }

    #[test]
    fn build_go_semantics_returns_ok_for_valid_go() {
        let sf = SourceFile {
            path: "test.go".to_string(),
            language: Language::Go,
            content: "package main".to_string(),
        };
        let parsed = parse_go_file(FileId(1), &sf).expect("parsing should succeed");
        let result = build_go_semantics(&parsed);
        assert!(result.is_ok());
    }

    #[test]
    fn build_go_semantics_populates_basic_structure() {
        let src = r#"
package main

import "fmt"

func hello() {
    fmt.Println("Hello")
}

var x = 42
"#;
        let sem = parse_and_build_full_semantics(src);

        assert!(!sem.imports.is_empty());
        assert!(!sem.functions.is_empty());
        assert!(!sem.declarations.is_empty());
    }

    #[test]
    fn build_go_semantics_populates_http_calls() {
        let src = r#"
package main

import "net/http"

func fetch() {
    http.Get("https://example.com")
}
"#;
        let sem = parse_and_build_full_semantics(src);

        assert_eq!(sem.http_calls.len(), 1);
    }

    #[test]
    fn full_semantics_for_empty_file() {
        let sem = parse_and_build_full_semantics("");

        assert!(sem.imports.is_empty());
        assert!(sem.functions.is_empty());
        assert!(sem.declarations.is_empty());
        assert!(sem.calls.is_empty());
        assert!(sem.http_calls.is_empty());
    }

    #[test]
    fn full_semantics_for_complete_http_server() {
        let src = r#"
package main

import (
    "fmt"
    "net/http"
)

func handler(w http.ResponseWriter, r *http.Request) {
    fmt.Fprintf(w, "Hello, World!")
}

func main() {
    http.HandleFunc("/", handler)
    http.ListenAndServe(":8080", nil)
}
"#;
        let sem = parse_and_build_full_semantics(src);

        // Imports
        assert_eq!(sem.imports.len(), 2);

        // Functions
        assert_eq!(sem.functions.len(), 2);
        assert!(sem.functions.iter().any(|f| f.name == "handler"));
        assert!(sem.functions.iter().any(|f| f.name == "main"));
    }

    #[test]
    fn full_semantics_for_goroutines() {
        let src = r#"
package main

func main() {
    go func() {
        fmt.Println("goroutine")
    }()
    
    go handleRequest()
}
"#;
        let sem = parse_and_build_full_semantics(src);

        assert_eq!(sem.goroutines.len(), 2);
    }

    #[test]
    fn full_semantics_for_error_handling() {
        let src = r#"
package main

import "os"

func readFile() {
    os.ReadFile("test.txt")
}
"#;
        let sem = parse_and_build_full_semantics(src);

        // Should detect unchecked error
        assert!(!sem.unchecked_errors.is_empty());
    }

    #[test]
    fn full_semantics_for_context_usage() {
        let src = r#"
package main

import "context"

func doWork() {
    ctx := context.Background()
    ctx, cancel := context.WithTimeout(ctx, time.Second)
    defer cancel()
}
"#;
        let sem = parse_and_build_full_semantics(src);

        // Should detect context usage
        assert!(!sem.context_usages.is_empty());
    }

    #[test]
    fn semantics_preserves_file_metadata() {
        let sf = SourceFile {
            path: "my/custom/path.go".to_string(),
            language: Language::Go,
            content: "package main".to_string(),
        };
        let parsed = parse_go_file(FileId(42), &sf).expect("parsing should succeed");
        let sem = build_go_semantics(&parsed).expect("semantics building should succeed");

        assert_eq!(sem.file_id, FileId(42));
        assert_eq!(sem.path, "my/custom/path.go");
        assert_eq!(sem.language, Language::Go);
    }

    #[test]
    fn full_semantics_for_api_handler() {
        let src = r#"
package main

import (
    "encoding/json"
    "net/http"
)

type User struct {
    ID   int    `json:"id"`
    Name string `json:"name"`
}

func getUser(w http.ResponseWriter, r *http.Request) {
    user := User{ID: 1, Name: "John"}
    json.NewEncoder(w).Encode(user)
}

func main() {
    http.HandleFunc("/user", getUser)
    http.ListenAndServe(":8080", nil)
}
"#;
        let sem = parse_and_build_full_semantics(src);

        // Types
        assert_eq!(sem.types.len(), 1);
        assert_eq!(sem.types[0].name, "User");
        assert!(!sem.types[0].fields.is_empty());

        // Functions
        let function_names: Vec<&str> = sem.functions.iter().map(|f| f.name.as_str()).collect();
        assert!(function_names.contains(&"getUser"));
        assert!(function_names.contains(&"main"));
    }
}
