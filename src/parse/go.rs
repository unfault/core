use std::sync::Arc;

use anyhow::Result;
use tree_sitter::{Language as TsLanguage, Parser};

use crate::parse::ast::{FileId, ParsedFile};
use crate::types::context::{Language, SourceFile};

fn go_language() -> TsLanguage {
    tree_sitter_go::LANGUAGE.into()
}

/// Parse a Go source file into a `ParsedFile`.
pub fn parse_go_file(file_id: FileId, sf: &SourceFile) -> Result<ParsedFile> {
    let mut parser = Parser::new();
    parser.set_language(&go_language())?;

    let source = Arc::new(sf.content.clone());
    let tree = parser
        .parse(&*source, None)
        .ok_or_else(|| anyhow::anyhow!("failed to parse Go source"))?;

    Ok(ParsedFile {
        file_id,
        path: sf.path.clone(),
        language: Language::Go,
        source,
        tree,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_go_source_file(path: &str, content: &str) -> SourceFile {
        SourceFile {
            path: path.to_string(),
            language: Language::Go,
            content: content.to_string(),
        }
    }

    #[test]
    fn test_go_language_returns_valid_language() {
        let lang = go_language();
        assert!(lang.abi_version() > 0);
    }

    #[test]
    fn test_parse_simple_package() {
        let sf = make_go_source_file("test.go", "package main");
        let result = parse_go_file(FileId(1), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.tree.root_node().kind(), "source_file");
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_hello_world() {
        let code = r#"
package main

import "fmt"

func main() {
    fmt.Println("Hello, World!")
}
"#;
        let sf = make_go_source_file("main.go", code);
        let result = parse_go_file(FileId(2), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        let root = parsed.tree.root_node();
        assert_eq!(root.kind(), "source_file");
        assert!(!root.has_error());
    }

    #[test]
    fn test_parse_function_definition() {
        let code = r#"
package main

func add(a, b int) int {
    return a + b
}

func multiply(a, b int) (result int) {
    result = a * b
    return
}
"#;
        let sf = make_go_source_file("func.go", code);
        let result = parse_go_file(FileId(3), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        let root = parsed.tree.root_node();
        let mut cursor = root.walk();
        let children: Vec<_> = root.children(&mut cursor).collect();
        let func_count = children
            .iter()
            .filter(|c| c.kind() == "function_declaration")
            .count();
        assert_eq!(func_count, 2);
    }

    #[test]
    fn test_parse_struct_definition() {
        let code = r#"
package main

type User struct {
    ID        int
    Name      string
    Email     string
    CreatedAt time.Time
}

type Config struct {
    Host string
    Port int
}
"#;
        let sf = make_go_source_file("types.go", code);
        let result = parse_go_file(FileId(4), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        let root = parsed.tree.root_node();
        let mut cursor = root.walk();
        let children: Vec<_> = root.children(&mut cursor).collect();
        let type_count = children
            .iter()
            .filter(|c| c.kind() == "type_declaration")
            .count();
        assert_eq!(type_count, 2);
    }

    #[test]
    fn test_parse_interface_definition() {
        let code = r#"
package main

type Reader interface {
    Read(p []byte) (n int, err error)
}

type Writer interface {
    Write(p []byte) (n int, err error)
}

type ReadWriter interface {
    Reader
    Writer
}
"#;
        let sf = make_go_source_file("interface.go", code);
        let result = parse_go_file(FileId(5), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_imports() {
        let code = r#"
package main

import (
    "fmt"
    "net/http"
    "encoding/json"
    
    "github.com/gin-gonic/gin"
    "github.com/gorilla/mux"
)
"#;
        let sf = make_go_source_file("imports.go", code);
        let result = parse_go_file(FileId(6), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        let root = parsed.tree.root_node();
        let mut cursor = root.walk();
        let children: Vec<_> = root.children(&mut cursor).collect();
        assert!(children.iter().any(|c| c.kind() == "import_declaration"));
    }

    #[test]
    fn test_parse_goroutines() {
        let code = r#"
package main

func main() {
    go func() {
        fmt.Println("goroutine")
    }()
    
    go handleRequest(req)
}
"#;
        let sf = make_go_source_file("goroutine.go", code);
        let result = parse_go_file(FileId(7), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_channels() {
        let code = r#"
package main

func main() {
    ch := make(chan int, 10)
    ch <- 42
    value := <-ch
    close(ch)
    
    select {
    case v := <-ch:
        fmt.Println(v)
    case ch <- 1:
        fmt.Println("sent")
    default:
        fmt.Println("no communication")
    }
}
"#;
        let sf = make_go_source_file("channels.go", code);
        let result = parse_go_file(FileId(8), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_defer() {
        let code = r#"
package main

func main() {
    defer cleanup()
    defer func() {
        if r := recover(); r != nil {
            log.Println("recovered:", r)
        }
    }()
}
"#;
        let sf = make_go_source_file("defer.go", code);
        let result = parse_go_file(FileId(9), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_error_handling() {
        let code = r#"
package main

func readFile(path string) ([]byte, error) {
    data, err := os.ReadFile(path)
    if err != nil {
        return nil, fmt.Errorf("failed to read file: %w", err)
    }
    return data, nil
}
"#;
        let sf = make_go_source_file("error.go", code);
        let result = parse_go_file(FileId(10), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_method_declaration() {
        let code = r#"
package main

type Counter struct {
    value int
}

func (c *Counter) Increment() {
    c.value++
}

func (c Counter) Value() int {
    return c.value
}
"#;
        let sf = make_go_source_file("method.go", code);
        let result = parse_go_file(FileId(11), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        let root = parsed.tree.root_node();
        let mut cursor = root.walk();
        let children: Vec<_> = root.children(&mut cursor).collect();
        let method_count = children
            .iter()
            .filter(|c| c.kind() == "method_declaration")
            .count();
        assert_eq!(method_count, 2);
    }

    #[test]
    fn test_parse_empty_file() {
        let sf = make_go_source_file("empty.go", "");
        let result = parse_go_file(FileId(12), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.tree.root_node().kind(), "source_file");
    }

    #[test]
    fn test_parse_syntax_error_tolerant() {
        let code = "package main\nfunc broken(\n";
        let sf = make_go_source_file("broken.go", code);
        let result = parse_go_file(FileId(13), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_source_content_preserved() {
        let code = "package main\n\nvar x = 42\n";
        let sf = make_go_source_file("content.go", code);
        let result = parse_go_file(FileId(14), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.source.as_str(), code);
    }

    #[test]
    fn test_file_id_preserved() {
        let sf = make_go_source_file("test.go", "package main");
        let file_id = FileId(99999);
        let result = parse_go_file(file_id, &sf);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().file_id, file_id);
    }

    #[test]
    fn test_path_preserved() {
        let path = "some/deep/nested/path/module.go";
        let sf = make_go_source_file(path, "package main");
        let result = parse_go_file(FileId(15), &sf);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().path, path);
    }

    #[test]
    fn test_language_is_go() {
        let sf = make_go_source_file("test.go", "package main");
        let result = parse_go_file(FileId(16), &sf);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().language, Language::Go);
    }

    #[test]
    fn test_parse_generics() {
        let code = r#"
package main

func Map[T, U any](items []T, f func(T) U) []U {
    result := make([]U, len(items))
    for i, item := range items {
        result[i] = f(item)
    }
    return result
}

type Stack[T any] struct {
    items []T
}

func (s *Stack[T]) Push(item T) {
    s.items = append(s.items, item)
}
"#;
        let sf = make_go_source_file("generics.go", code);
        let result = parse_go_file(FileId(17), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_http_handler() {
        let code = r#"
package main

import (
    "net/http"
    "encoding/json"
)

func handleUsers(w http.ResponseWriter, r *http.Request) {
    switch r.Method {
    case http.MethodGet:
        json.NewEncoder(w).Encode(users)
    case http.MethodPost:
        var user User
        json.NewDecoder(r.Body).Decode(&user)
        users = append(users, user)
    default:
        http.Error(w, "Method not allowed", http.StatusMethodNotAllowed)
    }
}
"#;
        let sf = make_go_source_file("handler.go", code);
        let result = parse_go_file(FileId(18), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_context_usage() {
        let code = r#"
package main

import (
    "context"
    "time"
)

func doWork(ctx context.Context) error {
    ctx, cancel := context.WithTimeout(ctx, 5*time.Second)
    defer cancel()
    
    select {
    case <-ctx.Done():
        return ctx.Err()
    case result := <-work():
        return process(result)
    }
}
"#;
        let sf = make_go_source_file("context.go", code);
        let result = parse_go_file(FileId(19), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_constants_and_iota() {
        let code = r#"
package main

const (
    StatusPending = iota
    StatusActive
    StatusCompleted
    StatusFailed
)

const (
    KB = 1 << (10 * iota)
    MB
    GB
    TB
)
"#;
        let sf = make_go_source_file("const.go", code);
        let result = parse_go_file(FileId(20), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(!parsed.tree.root_node().has_error());
    }
}
