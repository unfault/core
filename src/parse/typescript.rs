use std::sync::Arc;

use anyhow::Result;
use tree_sitter::{Language as TsLanguage, Parser};

use crate::parse::ast::{FileId, ParsedFile};
use crate::types::context::{Language, SourceFile};

fn typescript_language() -> TsLanguage {
    // Modern tree-sitter crate exposes LANGUAGE directly
    tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
}

fn tsx_language() -> TsLanguage {
    tree_sitter_typescript::LANGUAGE_TSX.into()
}

/// Parse a TypeScript source file into a `ParsedFile`.
pub fn parse_typescript_file(file_id: FileId, sf: &SourceFile) -> Result<ParsedFile> {
    let mut parser = Parser::new();

    // Use TSX parser for .tsx files, regular TypeScript for others
    let lang = if sf.path.ends_with(".tsx") {
        tsx_language()
    } else {
        typescript_language()
    };

    parser.set_language(&lang)?;

    let source = Arc::new(sf.content.clone());
    let tree = parser
        .parse(&*source, None)
        .ok_or_else(|| anyhow::anyhow!("failed to parse TypeScript source"))?;

    Ok(ParsedFile {
        file_id,
        path: sf.path.clone(),
        language: Language::Typescript,
        source,
        tree,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_typescript_source_file(path: &str, content: &str) -> SourceFile {
        SourceFile {
            path: path.to_string(),
            language: Language::Typescript,
            content: content.to_string(),
        }
    }

    #[test]
    fn test_typescript_language_returns_valid_language() {
        let lang = typescript_language();
        // Verify it's a valid tree-sitter language by checking ABI version
        assert!(lang.abi_version() > 0);
    }

    #[test]
    fn test_tsx_language_returns_valid_language() {
        let lang = tsx_language();
        // Verify it's a valid tree-sitter language by checking ABI version
        assert!(lang.abi_version() > 0);
    }

    #[test]
    fn test_parse_simple_assignment() {
        let sf = make_typescript_source_file("test.ts", "const x: number = 1;");
        let result = parse_typescript_file(FileId(1), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.tree.root_node().kind(), "program");
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_function_definition() {
        let code = r#"
function hello(name: string): string {
    return `Hello, ${name}!`;
}
"#;
        let sf = make_typescript_source_file("func.ts", code);
        let result = parse_typescript_file(FileId(2), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        let root = parsed.tree.root_node();
        assert_eq!(root.kind(), "program");
        // Should have a function_declaration child
        let mut cursor = root.walk();
        let children: Vec<_> = root.children(&mut cursor).collect();
        assert!(children.iter().any(|c| c.kind() == "function_declaration"));
    }

    #[test]
    fn test_parse_class_definition() {
        let code = r#"
class MyClass {
    private value: number;
    
    constructor(value: number) {
        this.value = value;
    }

    getValue(): number {
        return this.value;
    }
}
"#;
        let sf = make_typescript_source_file("class.ts", code);
        let result = parse_typescript_file(FileId(3), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        let root = parsed.tree.root_node();
        let mut cursor = root.walk();
        let children: Vec<_> = root.children(&mut cursor).collect();
        assert!(children.iter().any(|c| c.kind() == "class_declaration"));
    }

    #[test]
    fn test_parse_async_function() {
        let code = r#"
async function fetchData(url: string): Promise<Response> {
    const response = await fetch(url);
    return response;
}
"#;
        let sf = make_typescript_source_file("async.ts", code);
        let result = parse_typescript_file(FileId(4), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        let root = parsed.tree.root_node();
        let mut cursor = root.walk();
        let children: Vec<_> = root.children(&mut cursor).collect();
        assert!(children.iter().any(|c| c.kind() == "function_declaration"));
    }

    #[test]
    fn test_parse_imports() {
        let code = r#"
import express from 'express';
import { Router, Request, Response } from 'express';
import * as fs from 'fs';
import type { Config } from './types';
"#;
        let sf = make_typescript_source_file("imports.ts", code);
        let result = parse_typescript_file(FileId(5), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        let root = parsed.tree.root_node();
        let mut cursor = root.walk();
        let children: Vec<_> = root.children(&mut cursor).collect();
        let import_count = children
            .iter()
            .filter(|c| c.kind() == "import_statement")
            .count();
        assert!(import_count >= 4);
    }

    #[test]
    fn test_parse_decorators() {
        let code = r#"
@Controller('users')
class UserController {
    @Get(':id')
    @UseGuards(AuthGuard)
    getUser(@Param('id') id: string) {
        return this.userService.findOne(id);
    }
}
"#;
        let sf = make_typescript_source_file("decorators.ts", code);
        let result = parse_typescript_file(FileId(6), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        // Decorators are parsed in TypeScript
        assert!(!parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_empty_file() {
        let sf = make_typescript_source_file("empty.ts", "");
        let result = parse_typescript_file(FileId(7), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.tree.root_node().kind(), "program");
    }

    #[test]
    fn test_parse_whitespace_only() {
        let sf = make_typescript_source_file("whitespace.ts", "   \n\n   \t\n");
        let result = parse_typescript_file(FileId(8), &sf);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_comments_only() {
        let code = r#"
// This is a comment
// Another comment
/**
 * This is a JSDoc comment
 */
"#;
        let sf = make_typescript_source_file("comments.ts", code);
        let result = parse_typescript_file(FileId(9), &sf);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_syntax_error_tolerant() {
        // tree-sitter is error-tolerant
        let code = "function broken(\n";
        let sf = make_typescript_source_file("broken.ts", code);
        let result = parse_typescript_file(FileId(10), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_parse_incomplete_class() {
        let code = "class Incomplete {\n    method(param";
        let sf = make_typescript_source_file("incomplete.ts", code);
        let result = parse_typescript_file(FileId(11), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(parsed.tree.root_node().has_error());
    }

    #[test]
    fn test_source_content_preserved() {
        let code = "const x = 42;\nconst y = 'hello';\n";
        let sf = make_typescript_source_file("content.ts", code);
        let result = parse_typescript_file(FileId(12), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert_eq!(parsed.source.as_str(), code);
    }

    #[test]
    fn test_file_id_preserved() {
        let sf = make_typescript_source_file("test.ts", "const x = 1;");
        let file_id = FileId(99999);
        let result = parse_typescript_file(file_id, &sf);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().file_id, file_id);
    }

    #[test]
    fn test_path_preserved() {
        let path = "some/deep/nested/path/module.ts";
        let sf = make_typescript_source_file(path, "const x = 1;");
        let result = parse_typescript_file(FileId(13), &sf);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().path, path);
    }

    #[test]
    fn test_language_is_typescript() {
        let sf = make_typescript_source_file("test.ts", "const x = 1;");
        let result = parse_typescript_file(FileId(14), &sf);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().language, Language::Typescript);
    }

    #[test]
    fn test_parse_interface() {
        let code = r#"
interface User {
    id: number;
    name: string;
    email?: string;
}

type UserRole = 'admin' | 'user' | 'guest';
"#;
        let sf = make_typescript_source_file("types.ts", code);
        let result = parse_typescript_file(FileId(15), &sf);
        assert!(result.is_ok());
        assert!(!result.unwrap().tree.root_node().has_error());
    }

    #[test]
    fn test_parse_generics() {
        let code = r#"
function identity<T>(arg: T): T {
    return arg;
}

class Container<T> {
    private value: T;
    
    constructor(value: T) {
        this.value = value;
    }
    
    get(): T {
        return this.value;
    }
}
"#;
        let sf = make_typescript_source_file("generics.ts", code);
        let result = parse_typescript_file(FileId(16), &sf);
        assert!(result.is_ok());
        assert!(!result.unwrap().tree.root_node().has_error());
    }

    #[test]
    fn test_parse_try_catch() {
        let code = r#"
try {
    riskyOperation();
} catch (error) {
    console.error(error);
} finally {
    cleanup();
}
"#;
        let sf = make_typescript_source_file("exceptions.ts", code);
        let result = parse_typescript_file(FileId(17), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        let root = parsed.tree.root_node();
        let mut cursor = root.walk();
        let children: Vec<_> = root.children(&mut cursor).collect();
        assert!(children.iter().any(|c| c.kind() == "try_statement"));
    }

    #[test]
    fn test_parse_arrow_functions() {
        let code = r#"
const simple = (x: number): number => x * 2;
const multiLine = (x: number, y: number): number => {
    return x + y;
};
const withDefault = (x: number, y = 10) => x + y;
"#;
        let sf = make_typescript_source_file("arrow.ts", code);
        let result = parse_typescript_file(FileId(18), &sf);
        assert!(result.is_ok());
        assert!(!result.unwrap().tree.root_node().has_error());
    }

    #[test]
    fn test_parse_template_literals() {
        let code = r#"
const name = "World";
const greeting = `Hello, ${name}!`;
const complex = `Result: ${1 + 2 * 3} and ${func(arg)}`;
"#;
        let sf = make_typescript_source_file("template.ts", code);
        let result = parse_typescript_file(FileId(19), &sf);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_unicode_content() {
        let code = r#"
// 日本語コメント
const message = "こんにちは世界";
const emoji = "🎉🚀";
"#;
        let sf = make_typescript_source_file("unicode.ts", code);
        let result = parse_typescript_file(FileId(20), &sf);
        assert!(result.is_ok());
        let parsed = result.unwrap();
        assert!(parsed.source.contains("日本語"));
        assert!(parsed.source.contains("🎉"));
    }

    #[test]
    fn test_parse_express_route() {
        let code = r#"
import express, { Request, Response, NextFunction } from 'express';

const router = express.Router();

router.get('/users', async (req: Request, res: Response) => {
    const users = await User.findAll();
    res.json(users);
});

router.post('/users', async (req: Request, res: Response) => {
    const user = await User.create(req.body);
    res.status(201).json(user);
});

export default router;
"#;
        let sf = make_typescript_source_file("routes.ts", code);
        let result = parse_typescript_file(FileId(21), &sf);
        assert!(result.is_ok());
        assert!(!result.unwrap().tree.root_node().has_error());
    }

    #[test]
    fn test_parse_tsx_component() {
        let code = r#"
import React from 'react';

interface Props {
    name: string;
    count?: number;
}

export const Greeting: React.FC<Props> = ({ name, count = 0 }) => {
    return (
        <div className="greeting">
            <h1>Hello, {name}!</h1>
            <p>Count: {count}</p>
        </div>
    );
};
"#;
        let mut sf = make_typescript_source_file("component.tsx", code);
        sf.path = "component.tsx".to_string();
        let result = parse_typescript_file(FileId(22), &sf);
        assert!(result.is_ok());
        assert!(!result.unwrap().tree.root_node().has_error());
    }

    #[test]
    fn test_parse_enum() {
        let code = r#"
enum Direction {
    Up = 'UP',
    Down = 'DOWN',
    Left = 'LEFT',
    Right = 'RIGHT',
}

const enum Status {
    Active,
    Inactive,
    Pending,
}
"#;
        let sf = make_typescript_source_file("enums.ts", code);
        let result = parse_typescript_file(FileId(23), &sf);
        assert!(result.is_ok());
        assert!(!result.unwrap().tree.root_node().has_error());
    }

    #[test]
    fn test_parse_namespace() {
        let code = r#"
namespace MyNamespace {
    export interface User {
        id: number;
        name: string;
    }

    export function createUser(name: string): User {
        return { id: Date.now(), name };
    }
}
"#;
        let sf = make_typescript_source_file("namespace.ts", code);
        let result = parse_typescript_file(FileId(24), &sf);
        assert!(result.is_ok());
        assert!(!result.unwrap().tree.root_node().has_error());
    }

    #[test]
    fn test_parse_optional_chaining() {
        let code = r#"
const user = {
    profile: {
        name: 'John',
    },
};

const name = user?.profile?.name;
const age = user?.profile?.age ?? 0;
"#;
        let sf = make_typescript_source_file("optional.ts", code);
        let result = parse_typescript_file(FileId(25), &sf);
        assert!(result.is_ok());
        assert!(!result.unwrap().tree.root_node().has_error());
    }
}
