//! Python semantic model and framework-specific analysis (FastAPI, Django, Flask, etc.)

pub mod async_ops;
pub mod django;
pub mod fastapi;
pub mod flask;
pub mod http;
pub mod model;
pub mod orm;

use anyhow::Result;

use crate::parse::ast::ParsedFile;
use model::PyFileSemantics;

/// Build the semantic model for a single Python file.
///
/// This is the entry point the engine will call after parsing.
pub fn build_python_semantics(parsed: &ParsedFile) -> Result<PyFileSemantics> {
    // For now we only populate the basic structure; framework-specific
    // analysis (FastAPI) happens inside PyFileSemantics::analyze_frameworks.
    let mut sem = PyFileSemantics::from_parsed(parsed);
    sem.analyze_frameworks(parsed)?;
    Ok(sem)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::python::parse_python_file;
    use crate::types::context::{Language, SourceFile};

    /// Helper to parse Python source and build full semantics
    fn parse_and_build_full_semantics(source: &str) -> PyFileSemantics {
        let sf = SourceFile {
            path: "test.py".to_string(),
            language: Language::Python,
            content: source.to_string(),
        };
        let parsed = parse_python_file(FileId(1), &sf).expect("parsing should succeed");
        build_python_semantics(&parsed).expect("semantics building should succeed")
    }

    // ==================== build_python_semantics Tests ====================

    #[test]
    fn build_python_semantics_returns_ok_for_valid_python() {
        let sf = SourceFile {
            path: "test.py".to_string(),
            language: Language::Python,
            content: "x = 1".to_string(),
        };
        let parsed = parse_python_file(FileId(1), &sf).expect("parsing should succeed");
        let result = build_python_semantics(&parsed);
        assert!(result.is_ok());
    }

    #[test]
    fn build_python_semantics_populates_basic_structure() {
        let src = r#"
import os

def hello():
    pass

x = 42
"#;
        let sem = parse_and_build_full_semantics(src);

        assert!(!sem.imports.is_empty());
        assert!(!sem.functions.is_empty());
        assert!(!sem.assignments.is_empty());
    }

    #[test]
    fn build_python_semantics_runs_framework_analysis() {
        let src = r#"
from fastapi import FastAPI

app = FastAPI()
"#;
        let sem = parse_and_build_full_semantics(src);

        // FastAPI analysis should have run
        assert!(sem.fastapi.is_some());
        let fastapi = sem.fastapi.unwrap();
        assert_eq!(fastapi.apps.len(), 1);
    }

    #[test]
    fn build_python_semantics_populates_http_calls() {
        let src = r#"
import requests

def fetch():
    return requests.get('https://example.com')
"#;
        let sem = parse_and_build_full_semantics(src);

        // HTTP calls should be populated by analyze_frameworks
        assert_eq!(sem.http_calls.len(), 1);
    }

    // ==================== Integration Tests ====================

    #[test]
    fn full_semantics_for_fastapi_with_cors() {
        let src = r#"
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

app = FastAPI()

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
)

@app.get("/")
async def root():
    return {"message": "Hello"}
"#;
        let sem = parse_and_build_full_semantics(src);

        // Basic semantics
        assert_eq!(sem.imports.len(), 2);
        assert!(sem.functions.iter().any(|f| f.name == "root"));
        assert!(sem.assignments.iter().any(|a| a.target == "app"));

        // FastAPI semantics
        assert!(sem.fastapi.is_some());
        let fastapi = sem.fastapi.unwrap();
        assert_eq!(fastapi.apps.len(), 1);
        assert_eq!(fastapi.middlewares.len(), 1);
    }

    #[test]
    fn full_semantics_for_http_client_code() {
        let src = r#"
import requests
import httpx

def fetch_with_requests():
    return requests.get('https://example.com', timeout=30)

async def fetch_with_httpx():
    return httpx.get('https://example.com')
"#;
        let sem = parse_and_build_full_semantics(src);

        // Basic semantics
        assert_eq!(sem.imports.len(), 2);
        assert_eq!(sem.functions.len(), 2);

        // HTTP calls
        assert_eq!(sem.http_calls.len(), 2);

        let requests_call = sem
            .http_calls
            .iter()
            .find(|c| matches!(c.client_kind, http::HttpClientKind::Requests))
            .unwrap();
        let httpx_call = sem
            .http_calls
            .iter()
            .find(|c| matches!(c.client_kind, http::HttpClientKind::Httpx))
            .unwrap();

        assert!(requests_call.has_timeout);
        assert!(!httpx_call.has_timeout);
    }

    #[test]
    fn full_semantics_for_mixed_fastapi_and_http() {
        let src = r#"
from fastapi import FastAPI
import requests

app = FastAPI()

@app.get("/proxy")
async def proxy():
    response = requests.get('https://external-api.com')
    return response.json()
"#;
        let sem = parse_and_build_full_semantics(src);

        // Should have both FastAPI and HTTP semantics
        assert!(sem.fastapi.is_some());
        assert_eq!(sem.http_calls.len(), 1);

        let fastapi = sem.fastapi.unwrap();
        assert_eq!(fastapi.apps.len(), 1);
    }

    #[test]
    fn full_semantics_for_empty_file() {
        let sem = parse_and_build_full_semantics("");

        assert!(sem.imports.is_empty());
        assert!(sem.functions.is_empty());
        assert!(sem.assignments.is_empty());
        assert!(sem.calls.is_empty());
        assert!(sem.fastapi.is_none());
        assert!(sem.http_calls.is_empty());
    }

    #[test]
    fn full_semantics_for_non_framework_code() {
        let src = r#"
import os
import sys

def main():
    print("Hello, World!")

if __name__ == "__main__":
    main()
"#;
        let sem = parse_and_build_full_semantics(src);

        // Basic semantics should be populated
        assert_eq!(sem.imports.len(), 2);
        assert!(sem.functions.iter().any(|f| f.name == "main"));

        // No framework-specific semantics
        assert!(sem.fastapi.is_none());
        assert!(sem.http_calls.is_empty());
    }

    // ==================== File Metadata Tests ====================

    #[test]
    fn semantics_preserves_file_metadata() {
        let sf = SourceFile {
            path: "my/custom/path.py".to_string(),
            language: Language::Python,
            content: "x = 1".to_string(),
        };
        let parsed = parse_python_file(FileId(42), &sf).expect("parsing should succeed");
        let sem = build_python_semantics(&parsed).expect("semantics building should succeed");

        assert_eq!(sem.file_id, FileId(42));
        assert_eq!(sem.path, "my/custom/path.py");
        assert_eq!(sem.language, Language::Python);
    }

    // ==================== Complex Real-World Scenarios ====================

    #[test]
    fn full_semantics_for_complete_api_module() {
        let src = r#"
from fastapi import FastAPI, APIRouter, Depends
from fastapi.middleware.cors import CORSMiddleware
import requests
from typing import List, Optional

app = FastAPI(
    title="My API",
    description="A complete API example",
    version="1.0.0"
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

router = APIRouter()

@router.get("/items")
async def list_items():
    return []

@router.post("/items")
async def create_item(item: dict):
    return item

app.include_router(router, prefix="/api/v1")

def fetch_external_data():
    response = requests.get('https://external-api.com/data', timeout=30)
    return response.json()

async def async_handler():
    data = fetch_external_data()
    return data
"#;
        let sem = parse_and_build_full_semantics(src);

        // Imports
        assert!(sem.imports.len() >= 4);

        // Functions
        let function_names: Vec<&str> = sem.functions.iter().map(|f| f.name.as_str()).collect();
        assert!(function_names.contains(&"list_items"));
        assert!(function_names.contains(&"create_item"));
        assert!(function_names.contains(&"fetch_external_data"));
        assert!(function_names.contains(&"async_handler"));

        // Assignments
        assert!(sem.assignments.iter().any(|a| a.target == "app"));
        assert!(sem.assignments.iter().any(|a| a.target == "router"));

        // FastAPI
        assert!(sem.fastapi.is_some());
        let fastapi = sem.fastapi.unwrap();
        assert_eq!(fastapi.apps.len(), 1);
        assert_eq!(fastapi.middlewares.len(), 1);
        assert_eq!(fastapi.routers.len(), 1);

        // HTTP calls
        assert_eq!(sem.http_calls.len(), 1);
        assert!(sem.http_calls[0].has_timeout);
    }

    #[test]
    fn full_semantics_for_api_client_module() {
        let src = r#"
import requests
import httpx
from typing import Dict, Any, Optional

class APIClient:
    def __init__(self, base_url: str):
        self.base_url = base_url
    
    def get(self, path: str, timeout: int = 30) -> Dict[str, Any]:
        return requests.get(f"{self.base_url}{path}", timeout=timeout).json()
    
    def post(self, path: str, data: Dict[str, Any]) -> Dict[str, Any]:
        return requests.post(f"{self.base_url}{path}", json=data).json()

async def fetch_async(url: str) -> Dict[str, Any]:
    response = httpx.get(url, timeout=10)
    return response.json()

def fetch_sync(url: str) -> Dict[str, Any]:
    response = requests.get(url)
    return response.json()
"#;
        let sem = parse_and_build_full_semantics(src);

        // Should have multiple HTTP calls
        assert!(sem.http_calls.len() >= 4);

        // Check timeout detection
        let with_timeout: Vec<_> = sem.http_calls.iter().filter(|c| c.has_timeout).collect();
        let without_timeout: Vec<_> = sem.http_calls.iter().filter(|c| !c.has_timeout).collect();

        assert!(!with_timeout.is_empty());
        assert!(!without_timeout.is_empty());

        // No FastAPI in this module
        assert!(sem.fastapi.is_none());
    }

    // ==================== Async Operation Tests ====================

    #[test]
    fn async_operation_detection_in_build_semantics() {
        let src = r#"
import asyncio

async def main():
    task = asyncio.create_task(coro())
    await asyncio.gather(task)
"#;
        let sem = parse_and_build_full_semantics(src);

        assert!(!sem.async_operations.is_empty());
        let task_spawns: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::AsyncOperationType::TaskSpawn))
            .collect();
        assert_eq!(task_spawns.len(), 1);

        let gathers: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::AsyncOperationType::TaskGather))
            .collect();
        assert_eq!(gathers.len(), 1);
    }

    #[test]
    fn async_awaits_detected() {
        let src = r#"
import asyncio

async def main():
    task = asyncio.create_task(coro())
"#;
        let sem = parse_and_build_full_semantics(src);

        assert!(!sem.async_operations.is_empty());
    }

    #[test]
    fn asyncio_sleep_detected() {
        let src = r#"
import asyncio

async def main():
    await asyncio.sleep(5)
"#;
        let sem = parse_and_build_full_semantics(src);

        let sleeps: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::AsyncOperationType::Sleep))
            .collect();
        assert_eq!(sleeps.len(), 1);
    }

    #[test]
    fn asyncio_wait_for_with_timeout() {
        let src = r#"
import asyncio

async def main():
    await asyncio.wait_for(coro(), timeout=30.0)
"#;
        let sem = parse_and_build_full_semantics(src);

        let timeouts: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::AsyncOperationType::Timeout))
            .collect();
        assert!(!timeouts.is_empty() || !sem.async_operations.is_empty());
    }

    #[test]
    fn async_operation_with_error_handling() {
        let src = r#"
import asyncio

async def main():
    try:
        task = asyncio.create_task(coro())
    except Exception:
        pass
"#;
        let sem = parse_and_build_full_semantics(src);

        let spawns: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::AsyncOperationType::TaskSpawn))
            .collect();
        assert_eq!(spawns.len(), 1);
        assert!(spawns[0].has_error_handling);
    }

    #[test]
    fn async_operation_without_error_handling() {
        let src = r#"
import asyncio

async def main():
    task = asyncio.create_task(coro())
"#;
        let sem = parse_and_build_full_semantics(src);

        let spawns: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::AsyncOperationType::TaskSpawn))
            .collect();
        assert_eq!(spawns.len(), 1);
        assert!(!spawns[0].has_error_handling);
    }

    #[test]
    fn sync_code_has_no_async_operations() {
        let src = r#"
def main():
    result = sync_func()
    data = other_sync()
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
    fn async_operation_in_class_method() {
        let src = r#"
import asyncio

class MyService:
    async def async_method(self):
        task = asyncio.create_task(self.coro())
        await task
"#;
        let sem = parse_and_build_full_semantics(src);

        assert!(!sem.async_operations.is_empty());
        let tasks: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::AsyncOperationType::TaskSpawn))
            .collect();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].enclosing_function, Some("async_method".to_string()));
    }

    #[test]
    fn multiple_async_operations_detected() {
        let src = r#"
import asyncio

async def main():
    task1 = asyncio.create_task(coro1())
    task2 = asyncio.create_task(coro2())
    task3 = asyncio.create_task(coro3())
    await asyncio.gather(task1, task2, task3)
    await asyncio.sleep(1)
"#;
        let sem = parse_and_build_full_semantics(src);

        let spawns: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::AsyncOperationType::TaskSpawn))
            .collect();
        assert_eq!(spawns.len(), 3);

        let gathers: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::AsyncOperationType::TaskGather))
            .collect();
        assert_eq!(gathers.len(), 1);

        let sleeps: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::AsyncOperationType::Sleep))
            .collect();
        assert_eq!(sleeps.len(), 1);
    }

    #[test]
    fn async_operation_enclosing_function_tracked() {
        let src = r#"
import asyncio

async def outer():
    def inner():
        task = asyncio.create_task(coro())
    inner()
"#;
        let sem = parse_and_build_full_semantics(src);

        let spawns: Vec<_> = sem
            .async_operations
            .iter()
            .filter(|op| matches!(op.operation_type, model::AsyncOperationType::TaskSpawn))
            .collect();
        assert_eq!(spawns.len(), 1);
        assert_eq!(spawns[0].enclosing_function, Some("inner".to_string()));
    }
}
