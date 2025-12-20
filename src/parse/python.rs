use std::sync::Arc;

use anyhow::Result;
use tree_sitter::{Language as TsLanguage, Parser};

use crate::parse::ast::{FileId, ParsedFile};
use crate::types::context::{Language, SourceFile};

fn python_language() -> TsLanguage {
    // Modern tree-sitter crate exposes LANGUAGE directly
    tree_sitter_python::LANGUAGE.into()
}

/// Parse a Python source file into a `ParsedFile`.
pub fn parse_python_file(file_id: FileId, sf: &SourceFile) -> Result<ParsedFile> {
    let mut parser = Parser::new();
    parser.set_language(&python_language())?;

    let source = Arc::new(sf.content.clone());
    let tree = parser
        .parse(&*source, None)
        .ok_or_else(|| anyhow::anyhow!("failed to parse python source"))?;

    Ok(ParsedFile {
        file_id,
        path: sf.path.clone(),
        language: Language::Python,
        source,
        tree,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_python_source_file(path: &str, content: &str) -> SourceFile {
        SourceFile {
            path: path.to_string(),
            language: Language::Python,
            content: content.to_string(),
        }
    }

    #[test]
    fn test_python_language_returns_valid_language() {
        let lang = python_language();
        // Verify it's a valid tree-sitter language by checking ABI version
        assert!(lang.abi_version() > 0);
    }

    #[test]
    fn test_parse_simple_assignment() {
        let sf = make_python_source_file("test.py", "x = 1");
        let result = parse_python_file(FileId(1), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.tree.root_node().kind(), "module");
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_function_definition() {
        let code = r#"
def hello(name: str) -> str:
    return f"Hello, {name}!"
"#;
        let sf = make_python_source_file("func.py", code);
        let result = parse_python_file(FileId(2), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        let root = parsed.tree.root_node();
        assert_eq!(root.kind(), "module");
        // Should have a function_definition child
        let mut cursor = root.walk();
        let children: Vec<_> = root.children(&mut cursor).collect();
        assert!(children.iter().any(|c| c.kind() == "function_definition"));
    }

    #[test]
    fn test_parse_class_definition() {
        let code = r#"
class MyClass:
    def __init__(self, value):
        self.value = value

    def get_value(self):
        return self.value
"#;
        let sf = make_python_source_file("class.py", code);
        let result = parse_python_file(FileId(3), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        let root = parsed.tree.root_node();
        let mut cursor = root.walk();
        let children: Vec<_> = root.children(&mut cursor).collect();
        assert!(children.iter().any(|c| c.kind() == "class_definition"));
    }

    #[test]
    fn test_parse_async_function() {
        let code = r#"
async def fetch_data(url: str):
    async with aiohttp.ClientSession() as session:
        async for chunk in response.content.iter_chunked(1024):
            yield chunk
"#;
        let sf = make_python_source_file("async.py", code);
        let result = parse_python_file(FileId(4), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        let root = parsed.tree.root_node();
        let mut cursor = root.walk();
        let children: Vec<_> = root.children(&mut cursor).collect();
        assert!(children.iter().any(|c| c.kind() == "function_definition"));
    }

    #[test]
    fn test_parse_imports() {
        let code = r#"
import os
import sys
from typing import List, Dict, Optional
from pathlib import Path
from collections.abc import Callable
"#;
        let sf = make_python_source_file("imports.py", code);
        let result = parse_python_file(FileId(5), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        let root = parsed.tree.root_node();
        let mut cursor = root.walk();
        let children: Vec<_> = root.children(&mut cursor).collect();
        let import_count = children
            .iter()
            .filter(|c| c.kind() == "import_statement" || c.kind() == "import_from_statement")
            .count();
        assert!(import_count >= 5);
    }

    #[test]
    fn test_parse_decorators() {
        let code = r#"
@decorator
@another_decorator(arg=1)
def decorated_function():
    pass

@classmethod
def class_method(cls):
    pass
"#;
        let sf = make_python_source_file("decorators.py", code);
        let result = parse_python_file(FileId(6), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        let root = parsed.tree.root_node();
        let mut cursor = root.walk();
        let children: Vec<_> = root.children(&mut cursor).collect();
        let decorated_count = children
            .iter()
            .filter(|c| c.kind() == "decorated_definition")
            .count();
        assert!(decorated_count >= 2);
    }

    #[test]
    fn test_parse_empty_file() {
        let sf = make_python_source_file("empty.py", "");
        let result = parse_python_file(FileId(7), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.tree.root_node().kind(), "module");
        assert_eq!(parsed.tree.root_node().child_count(), 0);
    }

    #[test]
    fn test_parse_whitespace_only() {
        let sf = make_python_source_file("whitespace.py", "   \n\n   \t\n");
        let result = parse_python_file(FileId(8), &sf);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_comments_only() {
        let code = r#"
# This is a comment
# Another comment
"""
This is a docstring
"""
"#;
        let sf = make_python_source_file("comments.py", code);
        let result = parse_python_file(FileId(9), &sf);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_syntax_error_tolerant() {
        // tree-sitter is error-tolerant
        let code = "def broken(\n";
        let sf = make_python_source_file("broken.py", code);
        let result = parse_python_file(FileId(10), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_incomplete_class() {
        let code = "class Incomplete:\n    def method(self";
        let sf = make_python_source_file("incomplete.py", code);
        let result = parse_python_file(FileId(11), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_source_content_preserved() {
        let code = "x = 42\ny = 'hello'\n";
        let sf = make_python_source_file("content.py", code);
        let result = parse_python_file(FileId(12), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.source.as_str(), code);
    }

    #[test]
    fn test_file_id_preserved() {
        let sf = make_python_source_file("test.py", "pass");
        let file_id = FileId(99999);
        let result = parse_python_file(file_id, &sf);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().file_id, file_id);
    }

    #[test]
    fn test_path_preserved() {
        let path = "some/deep/nested/path/module.py";
        let sf = make_python_source_file(path, "pass");
        let result = parse_python_file(FileId(13), &sf);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().path, path);
    }

    #[test]
    fn test_language_is_python() {
        let sf = make_python_source_file("test.py", "pass");
        let result = parse_python_file(FileId(14), &sf);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().language, Language::Python);
    }

    #[test]
    fn test_parse_comprehensions() {
        let code = r#"
list_comp = [x * 2 for x in range(10) if x % 2 == 0]
dict_comp = {k: v for k, v in items.items()}
set_comp = {x for x in range(5)}
gen_exp = (x ** 2 for x in range(100))
"#;
        let sf = make_python_source_file("comprehensions.py", code);
        let result = parse_python_file(FileId(15), &sf);
        assert!(result.is_ok());
        assert!(!result.unwrap().tree.root_node().has_error());
    }

    #[test]
    fn test_parse_try_except() {
        let code = r#"
try:
    risky_operation()
except ValueError as e:
    handle_value_error(e)
except (TypeError, KeyError):
    handle_other()
else:
    success()
finally:
    cleanup()
"#;
        let sf = make_python_source_file("exceptions.py", code);
        let result = parse_python_file(FileId(16), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        let root = parsed.tree.root_node();
        let mut cursor = root.walk();
        let children: Vec<_> = root.children(&mut cursor).collect();
        assert!(children.iter().any(|c| c.kind() == "try_statement"));
    }

    #[test]
    fn test_parse_with_statement() {
        let code = r#"
with open('file.txt') as f:
    content = f.read()

with open('a') as a, open('b') as b:
    pass
"#;
        let sf = make_python_source_file("with.py", code);
        let result = parse_python_file(FileId(17), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        let root = parsed.tree.root_node();
        let mut cursor = root.walk();
        let children: Vec<_> = root.children(&mut cursor).collect();
        let with_count = children
            .iter()
            .filter(|c| c.kind() == "with_statement")
            .count();
        assert_eq!(with_count, 2);
    }

    #[test]
    fn test_parse_match_statement() {
        let code = r#"
match command:
    case "quit":
        exit()
    case "hello" | "hi":
        greet()
    case ["go", direction]:
        move(direction)
    case _:
        unknown()
"#;
        let sf = make_python_source_file("match.py", code);
        let result = parse_python_file(FileId(18), &sf);
        assert!(result.is_ok());
        // Match statement is Python 3.10+, tree-sitter should handle it
        let parsed = result.unwrap();
        let root = parsed.tree.root_node();
        let mut cursor = root.walk();
        let children: Vec<_> = root.children(&mut cursor).collect();
        assert!(children.iter().any(|c| c.kind() == "match_statement"));
    }

    #[test]
    fn test_parse_type_hints() {
        let code = r#"
from typing import List, Dict, Optional, Union

def process(
    items: List[str],
    mapping: Dict[str, int],
    optional: Optional[float] = None,
) -> Union[int, str]:
    return 42

x: int = 10
y: list[str] = []
"#;
        let sf = make_python_source_file("types.py", code);
        let result = parse_python_file(FileId(19), &sf);
        assert!(result.is_ok());
        assert!(!result.unwrap().tree.root_node().has_error());
    }

    #[test]
    fn test_parse_walrus_operator() {
        let code = r#"
if (n := len(items)) > 10:
    print(f"List is too long ({n} elements)")

while (line := file.readline()):
    process(line)
"#;
        let sf = make_python_source_file("walrus.py", code);
        let result = parse_python_file(FileId(20), &sf);
        assert!(result.is_ok());
        assert!(!result.unwrap().tree.root_node().has_error());
    }

    #[test]
    fn test_parse_f_strings() {
        let code = r#"
name = "World"
greeting = f"Hello, {name}!"
complex_fstring = f"Result: {1 + 2 * 3} and {func(arg)}"
nested = f"Outer {f'inner {value}'}"
"#;
        let sf = make_python_source_file("fstrings.py", code);
        let result = parse_python_file(FileId(21), &sf);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_lambda() {
        let code = r#"
simple = lambda x: x * 2
multi_arg = lambda x, y, z: x + y + z
with_default = lambda x, y=10: x + y
"#;
        let sf = make_python_source_file("lambda.py", code);
        let result = parse_python_file(FileId(22), &sf);
        assert!(result.is_ok());
        assert!(!result.unwrap().tree.root_node().has_error());
    }

    #[test]
    fn test_parse_unicode_identifiers() {
        let code = r#"
变量 = 42
日本語 = "hello"
émoji = "🎉"
"#;
        let sf = make_python_source_file("unicode.py", code);
        let result = parse_python_file(FileId(23), &sf);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_multiline_strings() {
        let code = r#"
multiline = """
This is a
multiline string
"""

raw_multiline = r'''
Raw \n string
'''
"#;
        let sf = make_python_source_file("multiline.py", code);
        let result = parse_python_file(FileId(24), &sf);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_global_nonlocal() {
        let code = r#"
global_var = 0

def outer():
    outer_var = 1
    
    def inner():
        global global_var
        nonlocal outer_var
        global_var += 1
        outer_var += 1
"#;
        let sf = make_python_source_file("scope.py", code);
        let result = parse_python_file(FileId(25), &sf);
        assert!(result.is_ok());
        assert!(!result.unwrap().tree.root_node().has_error());
    }

    #[test]
    fn test_parse_yield_expressions() {
        let code = r#"
def generator():
    yield 1
    yield 2
    yield from other_generator()
"#;
        let sf = make_python_source_file("yield.py", code);
        let result = parse_python_file(FileId(26), &sf);
        assert!(result.is_ok());
        assert!(!result.unwrap().tree.root_node().has_error());
    }

    #[test]
    fn test_parse_assert_statement() {
        let code = r#"
assert condition
assert x > 0, "x must be positive"
"#;
        let sf = make_python_source_file("assert.py", code);
        let result = parse_python_file(FileId(27), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        let root = parsed.tree.root_node();
        let mut cursor = root.walk();
        let children: Vec<_> = root.children(&mut cursor).collect();
        let assert_count = children
            .iter()
            .filter(|c| c.kind() == "assert_statement")
            .count();
        assert_eq!(assert_count, 2);
    }

    #[test]
    fn test_parse_raise_statement() {
        let code = r#"
raise ValueError("error message")
raise
raise Exception from original_error
"#;
        let sf = make_python_source_file("raise.py", code);
        let result = parse_python_file(FileId(28), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        let root = parsed.tree.root_node();
        let mut cursor = root.walk();
        let children: Vec<_> = root.children(&mut cursor).collect();
        let raise_count = children
            .iter()
            .filter(|c| c.kind() == "raise_statement")
            .count();
        assert_eq!(raise_count, 3);
    }
}
