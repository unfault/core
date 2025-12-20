//! TypeScript semantic model and framework-specific analysis (Express, NestJS, etc.)

pub mod express;
pub mod http;
pub mod model;

use anyhow::Result;

use crate::parse::ast::ParsedFile;
use model::TsFileSemantics;

/// Build the semantic model for a single TypeScript file.
///
/// This is the entry point the engine will call after parsing.
pub fn build_typescript_semantics(parsed: &ParsedFile) -> Result<TsFileSemantics> {
    let mut sem = TsFileSemantics::from_parsed(parsed);
    sem.analyze_frameworks(parsed)?;
    Ok(sem)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::typescript::parse_typescript_file;
    use crate::types::context::{Language, SourceFile};

    /// Helper to parse TypeScript source and build full semantics
    fn parse_and_build_full_semantics(source: &str) -> TsFileSemantics {
        let sf = SourceFile {
            path: "test.ts".to_string(),
            language: Language::Typescript,
            content: source.to_string(),
        };
        let parsed = parse_typescript_file(FileId(1), &sf).expect("parsing should succeed");
        build_typescript_semantics(&parsed).expect("semantics building should succeed")
    }

    // ==================== build_typescript_semantics Tests ====================

    #[test]
    fn build_typescript_semantics_returns_ok_for_valid_typescript() {
        let sf = SourceFile {
            path: "test.ts".to_string(),
            language: Language::Typescript,
            content: "const x = 1;".to_string(),
        };
        let parsed = parse_typescript_file(FileId(1), &sf).expect("parsing should succeed");
        let result = build_typescript_semantics(&parsed);
        assert!(result.is_ok());
    }

    #[test]
    fn build_typescript_semantics_populates_basic_structure() {
        let src = r#"
import express from 'express';

function hello() {
    console.log('Hello');
}

const x = 42;
"#;
        let sem = parse_and_build_full_semantics(src);

        assert!(!sem.imports.is_empty());
        assert!(!sem.functions.is_empty());
        assert!(!sem.variables.is_empty());
    }

    #[test]
    fn build_typescript_semantics_runs_framework_analysis() {
        let src = r#"
import express from 'express';

const app = express();
"#;
        let sem = parse_and_build_full_semantics(src);

        // Express analysis should have run
        assert!(sem.express.is_some());
        let express = sem.express.unwrap();
        assert_eq!(express.apps.len(), 1);
    }

    #[test]
    fn build_typescript_semantics_populates_http_calls() {
        let src = r#"
async function fetchData() {
    return fetch('https://example.com');
}
"#;
        let sem = parse_and_build_full_semantics(src);

        // HTTP calls should be populated by analyze_frameworks
        assert_eq!(sem.http_calls.len(), 1);
    }

    // ==================== Integration Tests ====================

    #[test]
    fn full_semantics_for_express_with_middleware() {
        let src = r#"
import express from 'express';
import cors from 'cors';

const app = express();

app.use(cors());
app.use(express.json());

app.get('/', async (req, res) => {
    res.json({ message: 'Hello' });
});
"#;
        let sem = parse_and_build_full_semantics(src);

        // Basic semantics
        assert!(sem.imports.len() >= 2);
        assert!(!sem.variables.is_empty());

        // Express semantics
        assert!(sem.express.is_some());
        let express = sem.express.unwrap();
        assert_eq!(express.apps.len(), 1);
        assert!(express.middlewares.len() >= 2);
    }

    #[test]
    fn full_semantics_for_http_client_code() {
        let src = r#"
import axios from 'axios';

async function fetchWithAxios() {
    return axios.get('https://example.com', { timeout: 30000 });
}

async function fetchWithFetch() {
    return fetch('https://example.com');
}
"#;
        let sem = parse_and_build_full_semantics(src);

        // Functions
        assert_eq!(sem.functions.len(), 2);

        // HTTP calls
        assert_eq!(sem.http_calls.len(), 2);

        let axios_call = sem
            .http_calls
            .iter()
            .find(|c| matches!(c.client_kind, http::HttpClientKind::Axios))
            .unwrap();
        let fetch_call = sem
            .http_calls
            .iter()
            .find(|c| matches!(c.client_kind, http::HttpClientKind::Fetch))
            .unwrap();

        assert!(axios_call.has_timeout);
        assert!(!fetch_call.has_timeout);
    }

    #[test]
    fn full_semantics_for_mixed_express_and_http() {
        let src = r#"
import express from 'express';

const app = express();

app.get('/proxy', async (req, res) => {
    const response = await fetch('https://external-api.com');
    const data = await response.json();
    res.json(data);
});
"#;
        let sem = parse_and_build_full_semantics(src);

        // Should have both Express and HTTP semantics
        assert!(sem.express.is_some());
        assert_eq!(sem.http_calls.len(), 1);

        let express = sem.express.unwrap();
        assert_eq!(express.apps.len(), 1);
    }

    #[test]
    fn full_semantics_for_empty_file() {
        let sem = parse_and_build_full_semantics("");

        assert!(sem.imports.is_empty());
        assert!(sem.functions.is_empty());
        assert!(sem.classes.is_empty());
        assert!(sem.variables.is_empty());
        assert!(sem.express.is_none());
        assert!(sem.http_calls.is_empty());
    }

    #[test]
    fn full_semantics_for_non_framework_code() {
        let src = r#"
import * as fs from 'fs';

function main() {
    console.log('Hello, World!');
}

main();
"#;
        let sem = parse_and_build_full_semantics(src);

        // Basic semantics should be populated
        assert!(!sem.imports.is_empty());
        assert!(sem.functions.iter().any(|f| f.name == "main"));

        // No framework-specific semantics
        assert!(sem.express.is_none());
        assert!(sem.http_calls.is_empty());
    }

    // ==================== File Metadata Tests ====================

    #[test]
    fn semantics_preserves_file_metadata() {
        let sf = SourceFile {
            path: "my/custom/path.ts".to_string(),
            language: Language::Typescript,
            content: "const x = 1;".to_string(),
        };
        let parsed = parse_typescript_file(FileId(42), &sf).expect("parsing should succeed");
        let sem = build_typescript_semantics(&parsed).expect("semantics building should succeed");

        assert_eq!(sem.file_id, FileId(42));
        assert_eq!(sem.path, "my/custom/path.ts");
        assert_eq!(sem.language, Language::Typescript);
    }

    // ==================== Complex Real-World Scenarios ====================

    #[test]
    fn full_semantics_for_complete_api_module() {
        let src = r#"
import express, { Router, Request, Response } from 'express';
import cors from 'cors';
import helmet from 'helmet';

const app = express();

app.use(cors());
app.use(helmet());
app.use(express.json());

const router = Router();

router.get('/items', async (req: Request, res: Response) => {
    res.json([]);
});

router.post('/items', async (req: Request, res: Response) => {
    res.json(req.body);
});

app.use('/api/v1', router);

async function fetchExternalData(): Promise<any> {
    const response = await fetch('https://external-api.com/data');
    return response.json();
}

export default app;
"#;
        let sem = parse_and_build_full_semantics(src);

        // Imports
        assert!(sem.imports.len() >= 3);

        // Functions
        let function_names: Vec<&str> = sem.functions.iter().map(|f| f.name.as_str()).collect();
        assert!(function_names.contains(&"fetchExternalData"));

        // Variables
        assert!(sem.variables.iter().any(|v| v.name == "app"));
        assert!(sem.variables.iter().any(|v| v.name == "router"));

        // Express
        assert!(sem.express.is_some());
        let express = sem.express.unwrap();
        assert_eq!(express.apps.len(), 1);
        assert!(express.middlewares.len() >= 3);
        assert!(!express.routers.is_empty());

        // HTTP calls
        assert_eq!(sem.http_calls.len(), 1);
    }

    #[test]
    fn full_semantics_for_nestjs_style_controller() {
        let src = r#"
import { Controller, Get, Post, Body } from '@nestjs/common';

@Controller('users')
class UserController {
    @Get()
    async findAll() {
        return [];
    }

    @Post()
    async create(@Body() body: any) {
        return body;
    }
}

export { UserController };
"#;
        let sem = parse_and_build_full_semantics(src);

        // Should parse the class with decorators
        assert_eq!(sem.classes.len(), 1);
        assert_eq!(sem.classes[0].name, "UserController");
        assert!(!sem.classes[0].methods.is_empty());
    }
}
