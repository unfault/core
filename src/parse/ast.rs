use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tree_sitter::{Node, Tree};

use crate::types::context::Language;

/// Engine-internal identifier for a file in a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FileId(pub u64);

/// Text range in (line, col) space; 0-based.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextRange {
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

/// Lightweight handle to "where in the AST" something lives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AstLocation {
    pub file_id: FileId,
    pub range: TextRange,
}

/// A fully parsed source file: source + tree-sitter AST + language tag.
#[derive(Debug)]
pub struct ParsedFile {
    pub file_id: FileId,
    pub path: String,
    pub language: Language,
    pub source: Arc<String>,
    pub tree: Tree,
}

impl ParsedFile {
    /// Convert a tree-sitter node range into a TextRange.
    pub fn location_for_node(&self, node: &Node) -> AstLocation {
        let range = node.range();
        let start = range.start_point;
        let end = range.end_point;

        AstLocation {
            file_id: self.file_id,
            range: TextRange {
                start_line: start.row as u32,
                start_col: start.column as u32,
                end_line: end.row as u32,
                end_col: end.column as u32,
            },
        }
    }

    /// Get the exact source text for a node.
    pub fn text_for_node(&self, node: &Node) -> String {
        let byte_range = node.byte_range();
        self.source[byte_range.start..byte_range.end].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // ==================== FileId Tests ====================

    #[test]
    fn test_file_id_equality() {
        let id1 = FileId(1);
        let id2 = FileId(1);
        let id3 = FileId(2);

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_file_id_clone() {
        let id1 = FileId(42);
        let id2 = id1;
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_file_id_hash() {
        let mut set = HashSet::new();
        set.insert(FileId(1));
        set.insert(FileId(2));
        set.insert(FileId(1)); // duplicate

        assert_eq!(set.len(), 2);
        assert!(set.contains(&FileId(1)));
        assert!(set.contains(&FileId(2)));
        assert!(!set.contains(&FileId(3)));
    }

    #[test]
    fn test_file_id_debug() {
        let id = FileId(123);
        let debug_str = format!("{:?}", id);
        assert!(debug_str.contains("FileId"));
        assert!(debug_str.contains("123"));
    }

    #[test]
    fn test_file_id_serialize_deserialize() {
        let id = FileId(999);
        let json = serde_json::to_string(&id).unwrap();
        let deserialized: FileId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }

    #[test]
    fn test_file_id_zero() {
        let id = FileId(0);
        assert_eq!(id.0, 0);
    }

    #[test]
    fn test_file_id_max() {
        let id = FileId(u64::MAX);
        assert_eq!(id.0, u64::MAX);
    }

    // ==================== TextRange Tests ====================

    #[test]
    fn test_text_range_creation() {
        let range = TextRange {
            start_line: 0,
            start_col: 0,
            end_line: 10,
            end_col: 5,
        };
        assert_eq!(range.start_line, 0);
        assert_eq!(range.start_col, 0);
        assert_eq!(range.end_line, 10);
        assert_eq!(range.end_col, 5);
    }

    #[test]
    fn test_text_range_equality() {
        let range1 = TextRange {
            start_line: 1,
            start_col: 2,
            end_line: 3,
            end_col: 4,
        };
        let range2 = TextRange {
            start_line: 1,
            start_col: 2,
            end_line: 3,
            end_col: 4,
        };
        let range3 = TextRange {
            start_line: 1,
            start_col: 2,
            end_line: 3,
            end_col: 5, // different
        };

        assert_eq!(range1, range2);
        assert_ne!(range1, range3);
    }

    #[test]
    fn test_text_range_clone() {
        let range1 = TextRange {
            start_line: 5,
            start_col: 10,
            end_line: 15,
            end_col: 20,
        };
        let range2 = range1;
        assert_eq!(range1, range2);
    }

    #[test]
    fn test_text_range_debug() {
        let range = TextRange {
            start_line: 1,
            start_col: 2,
            end_line: 3,
            end_col: 4,
        };
        let debug_str = format!("{:?}", range);
        assert!(debug_str.contains("TextRange"));
        assert!(debug_str.contains("start_line"));
    }

    #[test]
    fn test_text_range_serialize_deserialize() {
        let range = TextRange {
            start_line: 10,
            start_col: 20,
            end_line: 30,
            end_col: 40,
        };
        let json = serde_json::to_string(&range).unwrap();
        let deserialized: TextRange = serde_json::from_str(&json).unwrap();
        assert_eq!(range, deserialized);
    }

    #[test]
    fn test_text_range_single_character() {
        let range = TextRange {
            start_line: 5,
            start_col: 10,
            end_line: 5,
            end_col: 11,
        };
        assert_eq!(range.start_line, range.end_line);
        assert_eq!(range.end_col - range.start_col, 1);
    }

    #[test]
    fn test_text_range_multiline() {
        let range = TextRange {
            start_line: 0,
            start_col: 5,
            end_line: 100,
            end_col: 0,
        };
        assert!(range.end_line > range.start_line);
    }

    // ==================== AstLocation Tests ====================

    #[test]
    fn test_ast_location_creation() {
        let loc = AstLocation {
            file_id: FileId(1),
            range: TextRange {
                start_line: 0,
                start_col: 0,
                end_line: 1,
                end_col: 10,
            },
        };
        assert_eq!(loc.file_id, FileId(1));
        assert_eq!(loc.range.start_line, 0);
    }

    #[test]
    fn test_ast_location_equality() {
        let loc1 = AstLocation {
            file_id: FileId(1),
            range: TextRange {
                start_line: 0,
                start_col: 0,
                end_line: 1,
                end_col: 10,
            },
        };
        let loc2 = AstLocation {
            file_id: FileId(1),
            range: TextRange {
                start_line: 0,
                start_col: 0,
                end_line: 1,
                end_col: 10,
            },
        };
        let loc3 = AstLocation {
            file_id: FileId(2), // different file
            range: TextRange {
                start_line: 0,
                start_col: 0,
                end_line: 1,
                end_col: 10,
            },
        };

        assert_eq!(loc1, loc2);
        assert_ne!(loc1, loc3);
    }

    #[test]
    fn test_ast_location_clone() {
        let loc1 = AstLocation {
            file_id: FileId(42),
            range: TextRange {
                start_line: 1,
                start_col: 2,
                end_line: 3,
                end_col: 4,
            },
        };
        let loc2 = loc1.clone();
        assert_eq!(loc1, loc2);
    }

    #[test]
    fn test_ast_location_debug() {
        let loc = AstLocation {
            file_id: FileId(1),
            range: TextRange {
                start_line: 0,
                start_col: 0,
                end_line: 1,
                end_col: 10,
            },
        };
        let debug_str = format!("{:?}", loc);
        assert!(debug_str.contains("AstLocation"));
        assert!(debug_str.contains("file_id"));
        assert!(debug_str.contains("range"));
    }

    #[test]
    fn test_ast_location_serialize_deserialize() {
        let loc = AstLocation {
            file_id: FileId(123),
            range: TextRange {
                start_line: 10,
                start_col: 20,
                end_line: 30,
                end_col: 40,
            },
        };
        let json = serde_json::to_string(&loc).unwrap();
        let deserialized: AstLocation = serde_json::from_str(&json).unwrap();
        assert_eq!(loc, deserialized);
    }

    // ==================== ParsedFile Tests ====================

    // Helper to create a ParsedFile for testing
    fn create_test_parsed_file(code: &str) -> ParsedFile {
        use crate::types::context::SourceFile;

        let sf = SourceFile {
            path: "test.py".to_string(),
            language: Language::Python,
            content: code.to_string(),
        };

        crate::parse::python::parse_python_file(FileId(1), &sf).unwrap()
    }

    #[test]
    fn test_parsed_file_location_for_node_simple() {
        let code = "x = 1";
        let parsed = create_test_parsed_file(code);
        let root = parsed.tree.root_node();

        let loc = parsed.location_for_node(&root);
        assert_eq!(loc.file_id, FileId(1));
        assert_eq!(loc.range.start_line, 0);
        assert_eq!(loc.range.start_col, 0);
    }

    #[test]
    fn test_parsed_file_location_for_node_multiline() {
        let code = "def foo():\n    pass\n";
        let parsed = create_test_parsed_file(code);
        let root = parsed.tree.root_node();

        // Get the function definition
        let func_def = root.child(0).unwrap();
        assert_eq!(func_def.kind(), "function_definition");

        let loc = parsed.location_for_node(&func_def);
        assert_eq!(loc.range.start_line, 0);
        assert_eq!(loc.range.start_col, 0);
        assert_eq!(loc.range.end_line, 1);
    }

    #[test]
    fn test_parsed_file_location_for_nested_node() {
        let code = "class Foo:\n    def bar(self):\n        return 42\n";
        let parsed = create_test_parsed_file(code);
        let root = parsed.tree.root_node();

        // Navigate to the return statement
        let class_def = root.child(0).unwrap();
        let class_body = class_def.child_by_field_name("body").unwrap();
        let method = class_body.child(0).unwrap();
        let method_body = method.child_by_field_name("body").unwrap();
        let return_stmt = method_body.child(0).unwrap();

        assert_eq!(return_stmt.kind(), "return_statement");
        let loc = parsed.location_for_node(&return_stmt);
        assert_eq!(loc.range.start_line, 2);
        assert_eq!(loc.range.start_col, 8);
    }

    #[test]
    fn test_parsed_file_text_for_node_simple() {
        let code = "x = 42";
        let parsed = create_test_parsed_file(code);
        let root = parsed.tree.root_node();

        let text = parsed.text_for_node(&root);
        assert_eq!(text, code);
    }

    #[test]
    fn test_parsed_file_text_for_node_identifier() {
        let code = "variable_name = 123";
        let parsed = create_test_parsed_file(code);
        let root = parsed.tree.root_node();

        // Get the expression_statement, then the assignment
        let expr_stmt = root.child(0).unwrap();
        assert_eq!(expr_stmt.kind(), "expression_statement");

        // The assignment is inside the expression_statement
        let assignment = expr_stmt.child(0).unwrap();
        assert_eq!(assignment.kind(), "assignment");

        // Get the left side (identifier) - first child of assignment
        let identifier = assignment.child(0).unwrap();
        assert_eq!(identifier.kind(), "identifier");

        let text = parsed.text_for_node(&identifier);
        assert_eq!(text, "variable_name");
    }

    #[test]
    fn test_parsed_file_text_for_node_function_name() {
        let code = "def my_function():\n    pass";
        let parsed = create_test_parsed_file(code);
        let root = parsed.tree.root_node();

        let func_def = root.child(0).unwrap();
        let func_name = func_def.child_by_field_name("name").unwrap();

        let text = parsed.text_for_node(&func_name);
        assert_eq!(text, "my_function");
    }

    #[test]
    fn test_parsed_file_text_for_node_string_literal() {
        let code = r#"message = "Hello, World!""#;
        let parsed = create_test_parsed_file(code);
        let root = parsed.tree.root_node();

        // Get the expression_statement, then the assignment
        let expr_stmt = root.child(0).unwrap();
        let assignment = expr_stmt.child(0).unwrap();

        // Get the right side (string) - third child (after identifier and '=')
        let value = assignment.child(2).unwrap();
        assert_eq!(value.kind(), "string");

        let text = parsed.text_for_node(&value);
        assert_eq!(text, r#""Hello, World!""#);
    }

    #[test]
    fn test_parsed_file_text_for_node_multiline() {
        let code = "def foo():\n    x = 1\n    return x";
        let parsed = create_test_parsed_file(code);
        let root = parsed.tree.root_node();

        let func_def = root.child(0).unwrap();
        let text = parsed.text_for_node(&func_def);
        assert_eq!(text, code);
    }

    #[test]
    fn test_parsed_file_text_for_node_unicode() {
        let code = r#"greeting = "こんにちは""#;
        let parsed = create_test_parsed_file(code);
        let root = parsed.tree.root_node();

        // Get the expression_statement, then the assignment
        let expr_stmt = root.child(0).unwrap();
        let assignment = expr_stmt.child(0).unwrap();

        // Get the right side (string) - third child (after identifier and '=')
        let value = assignment.child(2).unwrap();
        assert_eq!(value.kind(), "string");

        let text = parsed.text_for_node(&value);
        assert_eq!(text, r#""こんにちは""#);
    }

    #[test]
    fn test_parsed_file_debug() {
        let code = "x = 1";
        let parsed = create_test_parsed_file(code);
        let debug_str = format!("{:?}", parsed);
        assert!(debug_str.contains("ParsedFile"));
        assert!(debug_str.contains("file_id"));
        assert!(debug_str.contains("path"));
    }

    #[test]
    fn test_parsed_file_source_is_arc() {
        let code = "x = 1";
        let parsed = create_test_parsed_file(code);

        // Verify source is shared via Arc
        let source_clone = Arc::clone(&parsed.source);
        assert_eq!(*source_clone, code);
        assert_eq!(Arc::strong_count(&parsed.source), 2);
    }

    #[test]
    fn test_parsed_file_preserves_path() {
        use crate::types::context::SourceFile;

        let sf = SourceFile {
            path: "some/nested/path/module.py".to_string(),
            language: Language::Python,
            content: "pass".to_string(),
        };

        let parsed = crate::parse::python::parse_python_file(FileId(99), &sf).unwrap();
        assert_eq!(parsed.path, "some/nested/path/module.py");
    }

    #[test]
    fn test_parsed_file_preserves_language() {
        let code = "x = 1";
        let parsed = create_test_parsed_file(code);
        assert_eq!(parsed.language, Language::Python);
    }

    #[test]
    fn test_location_for_node_with_indentation() {
        let code = "    x = 1";
        let parsed = create_test_parsed_file(code);
        let root = parsed.tree.root_node();

        let assignment = root.child(0).unwrap();
        let loc = parsed.location_for_node(&assignment);

        // Should start at column 4 due to indentation
        assert_eq!(loc.range.start_col, 4);
    }

    #[test]
    fn test_text_for_node_preserves_whitespace() {
        let code = "x   =   1";
        let parsed = create_test_parsed_file(code);
        let root = parsed.tree.root_node();

        let text = parsed.text_for_node(&root);
        assert_eq!(text, code);
    }

    #[test]
    fn test_location_for_empty_file() {
        let code = "";
        let parsed = create_test_parsed_file(code);
        let root = parsed.tree.root_node();

        let loc = parsed.location_for_node(&root);
        assert_eq!(loc.range.start_line, 0);
        assert_eq!(loc.range.start_col, 0);
        assert_eq!(loc.range.end_line, 0);
        assert_eq!(loc.range.end_col, 0);
    }

    #[test]
    fn test_text_for_node_empty_file() {
        let code = "";
        let parsed = create_test_parsed_file(code);
        let root = parsed.tree.root_node();

        let text = parsed.text_for_node(&root);
        assert_eq!(text, "");
    }
}
