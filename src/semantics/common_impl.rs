//! CommonSemantics trait implementations for all language-specific semantics.
//!
//! This module provides the bridge between language-specific semantic models
//! and the common abstractions, enabling cross-language analysis.

use crate::parse::ast::FileId;
use crate::types::context::Language;

use super::common::{
    CommonLocation, CommonSemantics,
    async_ops::{AsyncOperation, AsyncOperationType, AsyncRuntime},
    db::{DbLibrary, DbOperation, DbOperationType},
    functions::{FunctionCall, FunctionDef, FunctionKind, FunctionParam, Visibility},
    http::{HttpCall, HttpClientLibrary, HttpMethod},
    imports::{Import, ImportSource, ImportStyle, ImportedItem},
};

use super::go::model::{GoFileSemantics, GoFunction, GoImport};
use super::python::model::{
    ImportCategory as PyImportCategory, ImportStyle as PyImportStyle, PyCallSite, PyFileSemantics,
    PyFunction, PyImport,
};
use super::rust::model::{RustFileSemantics, RustFunction, RustUse, Visibility as RustVisibility};
use super::typescript::model::{TsFileSemantics, TsFunction, TsImport};

// =============================================================================
// Python Implementation
// =============================================================================

impl CommonSemantics for PyFileSemantics {
    fn file_id(&self) -> FileId {
        self.file_id
    }

    fn file_path(&self) -> &str {
        &self.path
    }

    fn language(&self) -> Language {
        Language::Python
    }

    fn http_calls(&self) -> Vec<HttpCall> {
        use super::python::http::HttpClientKind;

        self.http_calls
            .iter()
            .map(|call| {
                let library = match &call.client_kind {
                    HttpClientKind::Requests => HttpClientLibrary::Requests,
                    HttpClientKind::Httpx => HttpClientLibrary::Httpx,
                    HttpClientKind::Aiohttp => HttpClientLibrary::Aiohttp,
                    HttpClientKind::Other(s) => HttpClientLibrary::Other(s.clone()),
                };
                let method = match call.method_name.to_uppercase().as_str() {
                    "GET" => HttpMethod::Get,
                    "POST" => HttpMethod::Post,
                    "PUT" => HttpMethod::Put,
                    "PATCH" => HttpMethod::Patch,
                    "DELETE" => HttpMethod::Delete,
                    other => HttpMethod::Other(other.to_string()),
                };
                HttpCall {
                    library,
                    method,
                    url: None, // HttpCallSite doesn't store URL directly
                    has_timeout: call.has_timeout,
                    timeout_value: None,
                    retry_mechanism: None,
                    call_text: call.call_text.clone(),
                    location: CommonLocation {
                        file_id: self.file_id,
                        line: call.location.range.start_line + 1,
                        column: call.location.range.start_col + 1,
                        start_byte: call.start_byte,
                        end_byte: call.end_byte,
                    },
                    enclosing_function: call.function_name.clone(),
                    in_async_context: call.in_async_function,
                    in_loop: false,
                    start_byte: call.start_byte,
                    end_byte: call.end_byte,
                }
            })
            .collect()
    }

    fn db_operations(&self) -> Vec<DbOperation> {
        use super::common::db::EagerLoadingStrategy;
        use super::python::orm::{OrmKind, QueryType};

        self.orm_queries
            .iter()
            .map(|query| {
                let library = match query.orm_kind {
                    OrmKind::SqlAlchemy => DbLibrary::SqlAlchemy,
                    OrmKind::Django => DbLibrary::DjangoOrm,
                    OrmKind::Tortoise => DbLibrary::TortoiseOrm,
                    OrmKind::SqlModel => DbLibrary::Other("SQLModel".to_string()),
                    OrmKind::Peewee => DbLibrary::Peewee,
                    OrmKind::Unknown => DbLibrary::Other("Unknown".to_string()),
                };
                let operation_type = match query.query_type {
                    QueryType::Select => DbOperationType::Select,
                    QueryType::Insert => DbOperationType::Insert,
                    QueryType::Update => DbOperationType::Update,
                    QueryType::Delete => DbOperationType::Delete,
                    QueryType::RelationshipAccess => DbOperationType::RelationshipAccess,
                    QueryType::Unknown => DbOperationType::Unknown,
                };
                // Convert bool to EagerLoadingStrategy
                let eager_loading = if query.has_eager_loading {
                    Some(EagerLoadingStrategy::Other("detected".to_string()))
                } else {
                    None
                };
                DbOperation {
                    library,
                    operation_type,
                    has_timeout: false,
                    timeout_value: None,
                    in_transaction: false,
                    eager_loading,
                    in_loop: query.in_loop,
                    in_iteration: query.in_comprehension,
                    model_name: query.model_name.clone(),
                    relationship_field: query.loop_variable.clone(),
                    operation_text: query.query_text.clone().unwrap_or_default(),
                    location: CommonLocation {
                        file_id: self.file_id,
                        line: query.location.range.start_line + 1,
                        column: query.location.range.start_col + 1,
                        start_byte: query.start_byte,
                        end_byte: query.end_byte,
                    },
                    enclosing_function: None, // OrmQueryCall doesn't have enclosing_function
                    start_byte: query.start_byte,
                    end_byte: query.end_byte,
                }
            })
            .collect()
    }

    fn async_operations(&self) -> Vec<AsyncOperation> {
        // For Python, we could track asyncio.create_task, etc.
        // For now, return empty - can be enhanced later
        vec![]
    }

    fn imports(&self) -> Vec<Import> {
        self.imports
            .iter()
            .filter_map(|imp| convert_python_import(imp, self.file_id))
            .collect()
    }

    fn functions(&self) -> Vec<FunctionDef> {
        self.functions
            .iter()
            .filter_map(|func| convert_python_function(func, self.file_id, &self.calls))
            .collect()
    }
}

/// Convert a Python import to the common Import type
fn convert_python_import(py_import: &PyImport, file_id: FileId) -> Option<Import> {
    let style = match py_import.style {
        PyImportStyle::Import => ImportStyle::Module,
        PyImportStyle::FromImport => ImportStyle::Named,
    };

    let source = match py_import.category {
        PyImportCategory::Stdlib => ImportSource::StandardLib,
        PyImportCategory::ThirdParty => ImportSource::External,
        PyImportCategory::Local => ImportSource::Local,
    };

    let items: Vec<ImportedItem> = py_import
        .names
        .iter()
        .map(|name| ImportedItem::new(name.clone()))
        .collect();

    Some(Import {
        module_path: py_import.module.clone(),
        style,
        source,
        items,
        module_alias: py_import.alias.clone(),
        raw_text: String::new(),
        is_type_only: false,
        is_dynamic: false,
        location: CommonLocation {
            file_id,
            line: py_import.location.range.start_line + 1,
            column: py_import.location.range.start_col + 1,
            start_byte: 0,
            end_byte: 0,
        },
    })
}

/// Convert a Python function to the common FunctionDef type
fn convert_python_function(
    py_func: &PyFunction,
    file_id: FileId,
    all_calls: &[PyCallSite],
) -> Option<FunctionDef> {
    let kind = if py_func.is_method {
        FunctionKind::Method
    } else {
        FunctionKind::Function
    };

    let visibility = if py_func.name.starts_with("__") && py_func.name.ends_with("__") {
        Visibility::Public
    } else if py_func.name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Public
    };

    let params: Vec<FunctionParam> = py_func
        .params
        .iter()
        .map(|p| {
            let mut param = FunctionParam::new(&p.name);
            if let Some(ref type_ann) = p.type_annotation {
                param = param.with_type(type_ann);
            }
            if let Some(ref default) = p.default {
                param = param.with_default(default);
            }
            param
        })
        .collect();

    // Filter calls that are within this function's byte range
    let calls: Vec<FunctionCall> = all_calls
        .iter()
        .filter(|call| call.start_byte >= py_func.start_byte && call.end_byte <= py_func.end_byte)
        .map(|call| convert_py_call_site(call))
        .collect();

    Some(FunctionDef {
        name: py_func.name.clone(),
        kind,
        visibility,
        is_async: py_func.is_async,
        params,
        return_type: py_func.return_type.clone(),
        decorators: vec![],
        class_name: py_func.class_name.clone(),
        calls,
        body_lines: 0,
        has_error_handling: false,
        has_documentation: false,
        location: CommonLocation {
            file_id,
            line: py_func.location.range.start_line + 1,
            column: py_func.location.range.start_col + 1,
            start_byte: py_func.start_byte,
            end_byte: py_func.end_byte,
        },
        start_byte: py_func.start_byte,
        end_byte: py_func.end_byte,
    })
}

/// Convert a PyCallSite to the common FunctionCall type
fn convert_py_call_site(call: &PyCallSite) -> FunctionCall {
    // Extract callee name (the function being called)
    // For method calls like "obj.method()", callee is "method"
    // For simple calls like "func()", callee is "func"
    let (callee, receiver) = if let Some(idx) = call.callee.rfind('.') {
        let callee_name = call.callee[idx + 1..].to_string();
        let receiver_name = call.callee[..idx].to_string();
        (callee_name, Some(receiver_name))
    } else {
        (call.callee.clone(), None)
    };

    FunctionCall {
        callee,
        callee_expr: call.callee.clone(),
        receiver,
        line: call.location.range.start_line + 1,
        column: call.location.range.start_col + 1,
    }
}

// =============================================================================
// Go Implementation
// =============================================================================

impl CommonSemantics for GoFileSemantics {
    fn file_id(&self) -> FileId {
        self.file_id
    }

    fn file_path(&self) -> &str {
        &self.path
    }

    fn language(&self) -> Language {
        Language::Go
    }

    fn http_calls(&self) -> Vec<HttpCall> {
        use super::go::http::HttpClientKind;

        self.http_calls
            .iter()
            .map(|call| {
                let library = match &call.client_kind {
                    HttpClientKind::NetHttp => HttpClientLibrary::NetHttp,
                    HttpClientKind::Resty => HttpClientLibrary::Other("Resty".to_string()),
                    HttpClientKind::Fasthttp => HttpClientLibrary::Other("Fasthttp".to_string()),
                    HttpClientKind::Fiber => HttpClientLibrary::Other("Fiber".to_string()),
                    HttpClientKind::Other(s) => HttpClientLibrary::Other(s.clone()),
                };
                let method = match call.method_name.to_uppercase().as_str() {
                    "GET" => HttpMethod::Get,
                    "POST" => HttpMethod::Post,
                    "PUT" => HttpMethod::Put,
                    "PATCH" => HttpMethod::Patch,
                    "DELETE" => HttpMethod::Delete,
                    "DO" => HttpMethod::Other("DO".to_string()),
                    other => HttpMethod::Other(other.to_string()),
                };
                HttpCall {
                    library,
                    method,
                    url: None, // Go HttpCallSite doesn't store URL
                    has_timeout: call.has_timeout,
                    timeout_value: None,
                    retry_mechanism: None,
                    call_text: call.call_text.clone(),
                    location: CommonLocation {
                        file_id: self.file_id,
                        line: call.location.range.start_line + 1,
                        column: call.location.range.start_col + 1,
                        start_byte: call.start_byte,
                        end_byte: call.end_byte,
                    },
                    enclosing_function: call.function_name.clone(),
                    in_async_context: false, // Go doesn't have async/await
                    in_loop: false,
                    start_byte: call.start_byte,
                    end_byte: call.end_byte,
                }
            })
            .collect()
    }

    fn db_operations(&self) -> Vec<DbOperation> {
        // TODO: Track database operations in Go semantics
        vec![]
    }

    fn async_operations(&self) -> Vec<AsyncOperation> {
        self.goroutines
            .iter()
            .map(|g| AsyncOperation {
                runtime: AsyncRuntime::Goroutine,
                operation_type: AsyncOperationType::TaskSpawn,
                has_error_handling: g.has_recover,
                error_handling: None,
                has_timeout: false,
                timeout_value: None,
                has_cancellation: g.has_context_param || g.has_done_channel,
                cancellation_handling: None,
                is_bounded: false,
                bound_limit: None,
                has_cleanup: false,
                operation_text: g.text.clone(),
                location: CommonLocation {
                    file_id: self.file_id,
                    line: g.location.range.start_line + 1,
                    column: g.location.range.start_col + 1,
                    start_byte: g.start_byte,
                    end_byte: g.end_byte,
                },
                enclosing_function: g.function_name.clone(),
                start_byte: g.start_byte,
                end_byte: g.end_byte,
            })
            .collect()
    }

    fn imports(&self) -> Vec<Import> {
        self.imports
            .iter()
            .filter_map(|imp| convert_go_import(imp, self.file_id))
            .collect()
    }

    fn functions(&self) -> Vec<FunctionDef> {
        self.functions
            .iter()
            .filter_map(|func| convert_go_function(func, self.file_id))
            .collect()
    }
}

/// Convert a Go import to the common Import type
fn convert_go_import(go_import: &GoImport, file_id: FileId) -> Option<Import> {
    let source = if go_import.path.starts_with("github.com")
        || go_import.path.starts_with("golang.org")
        || go_import.path.contains('.')
    {
        ImportSource::External
    } else if go_import.path.starts_with('.') || go_import.path.starts_with('/') {
        ImportSource::Local
    } else {
        ImportSource::StandardLib
    };

    Some(Import {
        module_path: go_import.path.clone(),
        style: ImportStyle::Module,
        source,
        items: vec![],
        module_alias: go_import.alias.clone(),
        raw_text: String::new(),
        is_type_only: false,
        is_dynamic: false,
        location: CommonLocation {
            file_id,
            line: go_import.location.range.start_line + 1,
            column: go_import.location.range.start_col + 1,
            start_byte: 0,
            end_byte: 0,
        },
    })
}

/// Convert a Go function to the common FunctionDef type
fn convert_go_function(go_func: &GoFunction, file_id: FileId) -> Option<FunctionDef> {
    let visibility = if go_func
        .name
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
    {
        Visibility::Public
    } else {
        Visibility::Package
    };

    let params: Vec<FunctionParam> = go_func
        .params
        .iter()
        .map(|p| FunctionParam::new(&p.name).with_type(&p.param_type))
        .collect();

    let return_type = if go_func.return_types.is_empty() {
        None
    } else {
        Some(go_func.return_types.join(", "))
    };

    Some(FunctionDef {
        name: go_func.name.clone(),
        kind: FunctionKind::Function,
        visibility,
        is_async: false,
        params,
        return_type,
        decorators: vec![],
        class_name: None,
        calls: vec![], // TODO: Extract calls from Go function body
        body_lines: 0,
        has_error_handling: go_func.returns_error,
        has_documentation: false,
        location: CommonLocation {
            file_id,
            line: go_func.location.range.start_line + 1,
            column: go_func.location.range.start_col + 1,
            start_byte: 0,
            end_byte: 0,
        },
        start_byte: 0,
        end_byte: 0,
    })
}

// =============================================================================
// Rust Implementation
// =============================================================================

impl CommonSemantics for RustFileSemantics {
    fn file_id(&self) -> FileId {
        self.file_id
    }

    fn file_path(&self) -> &str {
        &self.path
    }

    fn language(&self) -> Language {
        Language::Rust
    }

    fn http_calls(&self) -> Vec<HttpCall> {
        // Rust doesn't have http_calls field yet - return empty
        vec![]
    }

    fn db_operations(&self) -> Vec<DbOperation> {
        vec![]
    }

    fn async_operations(&self) -> Vec<AsyncOperation> {
        self.async_info
            .spawn_calls
            .iter()
            .map(|spawn| {
                let runtime = if self.async_info.uses_tokio {
                    AsyncRuntime::Tokio
                } else if self.async_info.uses_async_std {
                    AsyncRuntime::AsyncStd
                } else {
                    AsyncRuntime::Other("unknown".to_string())
                };
                AsyncOperation {
                    runtime,
                    operation_type: AsyncOperationType::TaskSpawn,
                    has_error_handling: spawn.has_error_handling,
                    error_handling: None,
                    has_timeout: false,
                    timeout_value: None,
                    has_cancellation: spawn.handle_captured,
                    cancellation_handling: None,
                    is_bounded: false,
                    bound_limit: None,
                    has_cleanup: false,
                    operation_text: spawn.spawned_expr.clone(),
                    location: CommonLocation {
                        file_id: self.file_id,
                        line: spawn.location.range.start_line + 1,
                        column: spawn.location.range.start_col + 1,
                        start_byte: spawn.start_byte,
                        end_byte: spawn.end_byte,
                    },
                    enclosing_function: spawn.function_name.clone(),
                    start_byte: spawn.start_byte,
                    end_byte: spawn.end_byte,
                }
            })
            .collect()
    }

    fn imports(&self) -> Vec<Import> {
        self.uses
            .iter()
            .filter_map(|imp| convert_rust_use(imp, self.file_id))
            .collect()
    }

    fn functions(&self) -> Vec<FunctionDef> {
        self.functions
            .iter()
            .filter_map(|func| convert_rust_function(func, self.file_id))
            .collect()
    }
}

/// Convert a Rust use statement to the common Import type
fn convert_rust_use(rust_use: &RustUse, file_id: FileId) -> Option<Import> {
    let source = if rust_use.path.starts_with("std::") || rust_use.path.starts_with("core::") {
        ImportSource::StandardLib
    } else if rust_use.path.starts_with("crate::")
        || rust_use.path.starts_with("super::")
        || rust_use.path.starts_with("self::")
    {
        ImportSource::Local
    } else {
        ImportSource::External
    };

    let style = if rust_use.is_glob {
        ImportStyle::Star
    } else if rust_use.items.is_empty() {
        ImportStyle::Module
    } else {
        ImportStyle::Named
    };

    let items: Vec<ImportedItem> = rust_use
        .items
        .iter()
        .map(|name| ImportedItem::new(name.clone()))
        .collect();

    Some(Import {
        module_path: rust_use.path.clone(),
        style,
        source,
        items,
        module_alias: rust_use.alias.clone(),
        raw_text: String::new(),
        is_type_only: false,
        is_dynamic: false,
        location: CommonLocation {
            file_id,
            line: rust_use.location.range.start_line + 1,
            column: rust_use.location.range.start_col + 1,
            start_byte: 0,
            end_byte: 0,
        },
    })
}

/// Convert a Rust function to the common FunctionDef type
fn convert_rust_function(rust_func: &RustFunction, file_id: FileId) -> Option<FunctionDef> {
    let visibility = match rust_func.visibility {
        RustVisibility::Pub => Visibility::Public,
        RustVisibility::PubCrate => Visibility::Package,
        RustVisibility::PubSuper => Visibility::Protected,
        RustVisibility::PubIn(_) => Visibility::Package,
        RustVisibility::Private => Visibility::Private,
    };

    let params: Vec<FunctionParam> = rust_func
        .params
        .iter()
        .map(|p| FunctionParam::new(&p.name).with_type(&p.param_type))
        .collect();

    Some(FunctionDef {
        name: rust_func.name.clone(),
        kind: FunctionKind::Function,
        visibility,
        is_async: rust_func.is_async,
        params,
        return_type: rust_func.return_type.clone(),
        decorators: vec![],
        class_name: None,
        calls: vec![], // TODO: Extract calls from Rust function body
        body_lines: 0,
        has_error_handling: false,
        has_documentation: false,
        location: CommonLocation {
            file_id,
            line: rust_func.location.range.start_line + 1,
            column: rust_func.location.range.start_col + 1,
            start_byte: rust_func.start_byte,
            end_byte: rust_func.end_byte,
        },
        start_byte: rust_func.start_byte,
        end_byte: rust_func.end_byte,
    })
}

// =============================================================================
// TypeScript Implementation
// =============================================================================

impl CommonSemantics for TsFileSemantics {
    fn file_id(&self) -> FileId {
        self.file_id
    }

    fn file_path(&self) -> &str {
        &self.path
    }

    fn language(&self) -> Language {
        Language::Typescript
    }

    fn http_calls(&self) -> Vec<HttpCall> {
        use super::typescript::http::HttpClientKind;

        self.http_calls
            .iter()
            .map(|call| {
                let library = match call.client_kind {
                    HttpClientKind::Fetch => HttpClientLibrary::Fetch,
                    HttpClientKind::Axios => HttpClientLibrary::Axios,
                    HttpClientKind::Got => HttpClientLibrary::Got,
                    HttpClientKind::NodeHttp => HttpClientLibrary::Other("node-http".to_string()),
                    HttpClientKind::NodeFetch => HttpClientLibrary::Other("node-fetch".to_string()),
                    HttpClientKind::Undici => HttpClientLibrary::Other("undici".to_string()),
                    HttpClientKind::Ky => HttpClientLibrary::Other("ky".to_string()),
                    HttpClientKind::Superagent => {
                        HttpClientLibrary::Other("superagent".to_string())
                    }
                    HttpClientKind::Unknown => HttpClientLibrary::Other("unknown".to_string()),
                };
                let method = match call.method.to_uppercase().as_str() {
                    "GET" => HttpMethod::Get,
                    "POST" => HttpMethod::Post,
                    "PUT" => HttpMethod::Put,
                    "PATCH" => HttpMethod::Patch,
                    "DELETE" => HttpMethod::Delete,
                    "FETCH" => HttpMethod::Get, // fetch defaults to GET
                    "REQUEST" => HttpMethod::Other("REQUEST".to_string()),
                    other => HttpMethod::Other(other.to_string()),
                };
                HttpCall {
                    library,
                    method,
                    url: call.url.clone(),
                    has_timeout: call.has_timeout,
                    timeout_value: None,
                    retry_mechanism: None,
                    call_text: String::new(),
                    location: CommonLocation {
                        file_id: self.file_id,
                        line: call.location.range.start_line + 1,
                        column: call.location.range.start_col + 1,
                        start_byte: call.start_byte,
                        end_byte: call.end_byte,
                    },
                    enclosing_function: call.function_name.clone(),
                    in_async_context: call.in_async_context,
                    in_loop: false,
                    start_byte: call.start_byte,
                    end_byte: call.end_byte,
                }
            })
            .collect()
    }

    fn db_operations(&self) -> Vec<DbOperation> {
        vec![]
    }

    fn async_operations(&self) -> Vec<AsyncOperation> {
        vec![]
    }

    fn imports(&self) -> Vec<Import> {
        self.imports
            .iter()
            .filter_map(|imp| convert_ts_import(imp, self.file_id))
            .collect()
    }

    fn functions(&self) -> Vec<FunctionDef> {
        self.functions
            .iter()
            .filter_map(|func| convert_ts_function(func, self.file_id))
            .collect()
    }
}

/// Convert a TypeScript import to the common Import type
fn convert_ts_import(ts_import: &TsImport, file_id: FileId) -> Option<Import> {
    let source = if ts_import.module.starts_with('.') {
        ImportSource::Local
    } else {
        ImportSource::External
    };

    let style = if ts_import.default_import.is_some() && ts_import.named_imports.is_empty() {
        ImportStyle::Default
    } else if ts_import.namespace_import.is_some() {
        ImportStyle::Star
    } else if !ts_import.named_imports.is_empty() {
        ImportStyle::Named
    } else {
        ImportStyle::SideEffect
    };

    let items: Vec<ImportedItem> = ts_import
        .named_imports
        .iter()
        .map(|name| ImportedItem::new(name.clone()))
        .collect();

    Some(Import {
        module_path: ts_import.module.clone(),
        style,
        source,
        items,
        module_alias: ts_import
            .default_import
            .clone()
            .or(ts_import.namespace_import.clone()),
        raw_text: String::new(),
        is_type_only: ts_import.is_type_only,
        is_dynamic: false,
        location: CommonLocation {
            file_id,
            line: ts_import.location.range.start_line + 1,
            column: ts_import.location.range.start_col + 1,
            start_byte: 0,
            end_byte: 0,
        },
    })
}

/// Convert a TypeScript function to the common FunctionDef type
fn convert_ts_function(ts_func: &TsFunction, file_id: FileId) -> Option<FunctionDef> {
    let visibility = if ts_func.is_exported {
        Visibility::Public
    } else {
        Visibility::Private
    };

    let kind = if ts_func.is_generator {
        FunctionKind::Generator
    } else {
        FunctionKind::Function
    };

    let params: Vec<FunctionParam> = ts_func
        .params
        .iter()
        .map(|p| {
            let mut param = FunctionParam::new(&p.name);
            if let Some(ref type_ann) = p.type_annotation {
                param = param.with_type(type_ann);
            }
            if let Some(ref default) = p.default_value {
                param = param.with_default(default);
            }
            if p.is_rest {
                param = param.variadic();
            }
            param
        })
        .collect();

    Some(FunctionDef {
        name: ts_func.name.clone(),
        kind,
        visibility,
        is_async: ts_func.is_async,
        params,
        return_type: ts_func.return_type.clone(),
        decorators: vec![],
        class_name: None,
        calls: vec![], // TODO: Extract calls from TypeScript function body
        body_lines: 0,
        has_error_handling: ts_func.has_try_catch,
        has_documentation: false,
        location: CommonLocation {
            file_id,
            line: ts_func.location.range.start_line + 1,
            column: ts_func.location.range.start_col + 1,
            start_byte: 0,
            end_byte: 0,
        },
        start_byte: 0,
        end_byte: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::python::parse_python_file;
    use crate::types::context::SourceFile;

    fn parse_python(source: &str) -> PyFileSemantics {
        let sf = SourceFile {
            path: "test.py".to_string(),
            language: Language::Python,
            content: source.to_string(),
        };
        let parsed = parse_python_file(FileId(1), &sf).expect("parsing should succeed");
        let mut sem = PyFileSemantics::from_parsed(&parsed);
        sem.analyze_frameworks(&parsed)
            .expect("analysis should succeed");
        sem
    }

    #[test]
    fn python_imports_via_common_trait() {
        let sem = parse_python(
            r#"
import os
from typing import List, Optional
import requests
"#,
        );

        let imports = sem.imports();
        assert_eq!(imports.len(), 3);

        // Check that os is identified as stdlib
        let os_import = imports.iter().find(|i| i.module_path == "os").unwrap();
        assert!(os_import.is_stdlib());

        // Check that requests is identified as external
        let requests_import = imports
            .iter()
            .find(|i| i.module_path == "requests")
            .unwrap();
        assert!(requests_import.is_external());
    }

    #[test]
    fn python_functions_via_common_trait() {
        let sem = parse_python(
            r#"
def sync_function():
    pass

async def async_function():
    pass

def _private_function():
    pass
"#,
        );

        let functions = sem.functions();
        assert_eq!(functions.len(), 3);

        let sync_fn = functions
            .iter()
            .find(|f| f.name == "sync_function")
            .unwrap();
        assert!(!sync_fn.is_async);
        assert!(sync_fn.is_public());

        let async_fn = functions
            .iter()
            .find(|f| f.name == "async_function")
            .unwrap();
        assert!(async_fn.is_async);

        let private_fn = functions
            .iter()
            .find(|f| f.name == "_private_function")
            .unwrap();
        assert!(!private_fn.is_public());
    }

    #[test]
    fn python_file_metadata_via_common_trait() {
        let sem = parse_python("x = 1");

        assert_eq!(sem.file_id(), FileId(1));
        assert_eq!(sem.file_path(), "test.py");
        assert_eq!(sem.language(), Language::Python);
    }

    #[test]
    fn python_functions_with_calls_extraction() {
        let sem = parse_python(
            r#"
def helper():
    pass

def caller():
    helper()
    other_func()
"#,
        );

        let functions = sem.functions();
        assert_eq!(functions.len(), 2);

        // Helper should have no calls
        let helper_fn = functions.iter().find(|f| f.name == "helper").unwrap();
        assert_eq!(helper_fn.calls.len(), 0);

        // Caller should have 2 calls (helper and other_func)
        let caller_fn = functions.iter().find(|f| f.name == "caller").unwrap();
        assert_eq!(caller_fn.calls.len(), 2);

        // Check the callee names
        let callee_names: Vec<&str> = caller_fn.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(callee_names.contains(&"helper"));
        assert!(callee_names.contains(&"other_func"));
    }

    #[test]
    fn python_method_call_extraction() {
        let sem = parse_python(
            r#"
def process():
    obj.method()
    obj.nested.method()
"#,
        );

        let functions = sem.functions();
        assert_eq!(functions.len(), 1);

        let process_fn = &functions[0];
        assert_eq!(process_fn.calls.len(), 2);

        // First call: obj.method() -> callee="method", receiver=Some("obj")
        let first_call = &process_fn.calls[0];
        assert_eq!(first_call.callee, "method");
        assert_eq!(first_call.receiver.as_deref(), Some("obj"));

        // Second call: obj.nested.method() -> callee="method", receiver=Some("obj.nested")
        let second_call = &process_fn.calls[1];
        assert_eq!(second_call.callee, "method");
        assert_eq!(second_call.receiver.as_deref(), Some("obj.nested"));
    }

    #[test]
    fn python_function_has_byte_range() {
        let sem = parse_python(
            r#"
def my_func():
    pass
"#,
        );

        let functions = sem.functions();
        assert_eq!(functions.len(), 1);

        let func = &functions[0];
        // Function should have valid byte range
        assert!(func.start_byte > 0, "start_byte should be > 0");
        assert!(
            func.end_byte > func.start_byte,
            "end_byte should be > start_byte"
        );
    }
}
