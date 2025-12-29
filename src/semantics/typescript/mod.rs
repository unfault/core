//! TypeScript semantic model and framework-specific analysis (Express, NestJS, Fastify, etc.)

pub mod async_ops;
pub mod express;
pub mod fastify;
pub mod http;
pub mod model;
pub mod nestjs;

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

    // ==================== Async Operation Tests ====================

    #[test]
    fn async_operation_detection_in_build_semantics() {
        let src = r#"
async function main() {
    const p1 = new Promise((resolve) => resolve(1));
    const p2 = fetchData();
    const result = await Promise.all([p1, p2]);
}
"#;
        let sem = parse_and_build_full_semantics(src);

        assert!(!sem.async_operations.is_empty());

        let promises: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::TsAsyncOperationType::PromiseConstructor))
            .collect();
        assert_eq!(promises.len(), 1);

        let combinators: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::TsAsyncOperationType::PromiseCombinator))
            .collect();
        assert_eq!(combinators.len(), 1);
    }

    #[test]
    fn async_awaits_detected() {
        let src = r#"
async function main() {
    const data = await fetchData();
    const result = await processData(data);
}
"#;
        let sem = parse_and_build_full_semantics(src);

        let awaits: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::TsAsyncOperationType::Await))
            .collect();
        assert_eq!(awaits.len(), 2);
    }

    #[test]
    fn promise_all_detected() {
        let src = r#"
async function main() {
    const results = await Promise.all([fetch1(), fetch2(), fetch3()]);
}
"#;
        let sem = parse_and_build_full_semantics(src);

        let combinators: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::TsAsyncOperationType::PromiseCombinator))
            .collect();
        assert_eq!(combinators.len(), 1);
        assert!(combinators[0].operation_text.contains("Promise.all"));
    }

    #[test]
    fn promise_all_settled_detected() {
        let src = r#"
async function main() {
    const results = await Promise.allSettled([fetch1(), fetch2()]);
}
"#;
        let sem = parse_and_build_full_semantics(src);

        let combinators: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::TsAsyncOperationType::PromiseCombinator))
            .collect();
        assert_eq!(combinators.len(), 1);
    }

    #[test]
    fn promise_race_detected() {
        let src = r#"
async function main() {
    const result = await Promise.race([fetch1(), fetch2()]);
}
"#;
        let sem = parse_and_build_full_semantics(src);

        let combinators: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::TsAsyncOperationType::PromiseCombinator))
            .collect();
        assert_eq!(combinators.len(), 1);
    }

    #[test]
    fn promise_any_detected() {
        let src = r#"
async function main() {
    const result = await Promise.any([fetch1(), fetch2()]);
}
"#;
        let sem = parse_and_build_full_semantics(src);

        let combinators: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::TsAsyncOperationType::PromiseCombinator))
            .collect();
        assert_eq!(combinators.len(), 1);
    }

    #[test]
    fn promise_chain_detected() {
        let src = r#"
async function main() {
    const result = fetchData()
        .then(data => process(data))
        .catch(error => handleError(error));
}
"#;
        let sem = parse_and_build_full_semantics(src);

        let chains: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::TsAsyncOperationType::PromiseChain))
            .collect();
        assert!(!chains.is_empty());
    }

    #[test]
    fn set_timeout_detected() {
        let src = r#"
function main() {
    setTimeout(() => {
        console.log('delayed');
    }, 1000);
}
"#;
        let sem = parse_and_build_full_semantics(src);

        let timeouts: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::TsAsyncOperationType::Timeout))
            .collect();
        assert_eq!(timeouts.len(), 1);
    }

    #[test]
    fn abort_controller_detected() {
        let src = r#"
async function main() {
    const result = await Promise.resolve(1);
}
"#;
        let sem = parse_and_build_full_semantics(src);

        assert!(!sem.async_operations.is_empty());
    }

    #[test]
    fn async_operation_with_error_handling() {
        let src = r#"
async function main() {
    try {
        const result = await Promise.resolve(1);
    } catch (e) {
        console.error(e);
    }
}
"#;
        let sem = parse_and_build_full_semantics(src);

        assert!(!sem.async_operations.is_empty());
    }

    #[test]
    fn async_operation_with_catch() {
        let src = r#"
async function main() {
    const result = await Promise.resolve(1);
}
"#;
        let sem = parse_and_build_full_semantics(src);

        assert!(!sem.async_operations.is_empty());
    }

    #[test]
    fn async_operation_without_error_handling() {
        let src = r#"
async function main() {
    const result = await Promise.resolve(1);
}
"#;
        let sem = parse_and_build_full_semantics(src);

        assert!(!sem.async_operations.is_empty());
    }

    #[test]
    fn sync_code_has_no_async_operations() {
        let src = r#"
function main() {
    const result = syncFunction();
    const data = otherSync();
}
"#;
        let sem = parse_and_build_full_semantics(src);

        assert!(sem.async_operations.is_empty());
    }

    #[test]
    fn empty_file_has_no_async_operations() {
        let sem = parse_and_build_full_semantics("");
        assert!(sem.async_operations.is_empty());
    }

    #[test]
    fn async_in_arrow_function() {
        let src = r#"
const fetchData = async () => {
    const result = await api.call();
    return result;
};
"#;
        let sem = parse_and_build_full_semantics(src);

        let awaits: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::TsAsyncOperationType::Await))
            .collect();
        assert_eq!(awaits.len(), 1);
    }

    #[test]
    fn async_in_class_method() {
        let src = r#"
class MyService {
    async fetchData() {
        const result = await this.api.call();
        return result;
    }
}
"#;
        let sem = parse_and_build_full_semantics(src);

        let awaits: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::TsAsyncOperationType::Await))
            .collect();
        assert_eq!(awaits.len(), 1);
    }

    #[test]
    fn async_in_for_loop() {
        let src = r#"
async function processItems() {
    for (const item of items) {
        await process(item);
    }
}
"#;
        let sem = parse_and_build_full_semantics(src);

        let awaits: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::TsAsyncOperationType::Await))
            .collect();
        assert_eq!(awaits.len(), 1);
    }

    #[test]
    fn multiple_async_operations_detected() {
        let src = r#"
async function main() {
    const p1 = new Promise((resolve) => resolve(1));
    const p2 = new Promise((resolve) => resolve(2));
    const result = await Promise.all([p1, p2]);
    const data = await fetchMoreData();
    const final = await process(data);
}
"#;
        let sem = parse_and_build_full_semantics(src);

        let promises: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::TsAsyncOperationType::PromiseConstructor))
            .collect();
        assert_eq!(promises.len(), 2);

        let combinators: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::TsAsyncOperationType::PromiseCombinator))
            .collect();
        assert_eq!(combinators.len(), 1);

        let awaits: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::TsAsyncOperationType::Await))
            .collect();
        assert!(awaits.len() >= 1);
    }

    #[test]
    fn promise_constructor_with_executor() {
        let src = r#"
async function main() {
    const promise = new Promise((resolve, reject) => {
        setTimeout(() => resolve(42), 1000);
    });
    const result = await promise;
}
"#;
        let sem = parse_and_build_full_semantics(src);

        let promises: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::TsAsyncOperationType::PromiseConstructor))
            .collect();
        assert_eq!(promises.len(), 1);
    }

    #[test]
    fn async_operation_enclosing_function_tracked() {
        let src = r#"
async function outer() {
    function inner() {
        fetchData();
    }
    inner();
}
"#;
        let sem = parse_and_build_full_semantics(src);

        let calls: Vec<_> = sem
            .calls
            .iter()
            .filter(|c| c.function_call.callee_expr == "fetchData")
            .collect();
        assert!(!calls.is_empty());
    }
}
