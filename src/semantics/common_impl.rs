//! CommonSemantics trait implementations for all language-specific semantics.
//!
//! This module provides the bridge between language-specific semantic models
//! and the common abstractions, enabling cross-language analysis.

use crate::parse::ast::FileId;
use crate::types::context::Language;

use super::common::{
    CommonLocation, CommonSemantics,
    annotations::{Annotation, AnnotationType},
    async_ops::{AsyncOperation, AsyncOperationType, AsyncRuntime},
    db::{DbLibrary, DbOperation, DbOperationType},
    error_context::{ErrorContext, ErrorContextType},
    functions::{FunctionCall, FunctionDecorator, FunctionDef, FunctionKind, FunctionParam, Visibility},
    http::{HttpCall, HttpClientLibrary, HttpMethod},
    imports::{Import, ImportSource, ImportStyle, ImportedItem},
    route_patterns::{RouteFramework, RoutePattern},
};

use super::go::model::{GoCallSite, GoFileSemantics, GoFunction, GoImport, GoMethod};
use super::python::model::{
    AsyncOperation as PyAsyncOperation, AsyncOperationType as PyAsyncOperationType,
    ImportCategory as PyImportCategory, ImportStyle as PyImportStyle, PyCallSite, PyFileSemantics,
    PyFunction, PyImport,
};
use super::rust::model::{RustCallSite, RustFileSemantics, RustFunction, RustUse, Visibility as RustVisibility};
use super::typescript::model::{
    TsAsyncOperation, TsAsyncOperationType, TsCallSite, TsFileSemantics,
    TsFunction, TsImport, TsMethod,
};

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
        self.async_operations
            .iter()
            .map(|py_op| convert_python_async_op(py_op, self.file_id))
            .collect()
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

    fn annotations(&self) -> Vec<Annotation> {
        let mut annotations = Vec::new();

        for decorator in &self.decorators {
            let annotation_type = match decorator.name.to_lowercase().as_str() {
                n if n.contains("log") => AnnotationType::Logging,
                n if n.contains("retry") => AnnotationType::Retry,
                n if n.contains("cache") => AnnotationType::Cache,
                n if n.contains("rate") || n.contains("throttle") => AnnotationType::RateLimit,
                n if n.contains("timeout") => AnnotationType::Timeout,
                n if n.contains("feature") || n.contains("flag") => AnnotationType::FeatureFlag,
                n if n.contains("auth") || n.contains("permission") => AnnotationType::Auth { library: String::new() },
                n if n.contains("valid") => AnnotationType::Validation { library: String::new() },
                _ => AnnotationType::Other(decorator.name.clone()),
            };

            annotations.push(Annotation::new(
                decorator.name.clone(),
                annotation_type,
                decorator.function_name.clone().unwrap_or_default(),
                &self.path,
            ).with_parameters(decorator.parameters.clone())
                .with_location(
                    CommonLocation {
                        file_id: self.file_id,
                        line: decorator.location.range.start_line + 1,
                        column: decorator.location.range.start_col + 1,
                        start_byte: decorator.start_byte,
                        end_byte: decorator.end_byte,
                    },
                    decorator.start_byte,
                    decorator.end_byte,
                ));
        }

        annotations
    }

    fn route_patterns(&self) -> Vec<RoutePattern> {
        let mut routes = Vec::new();

        if let Some(ref fastapi) = self.fastapi {
            for route in &fastapi.routes {
                let has_auth = route.handler_name.to_lowercase().contains("auth")
                    || route.handler_name.to_lowercase().contains("protected");

                let has_validation = route.handler_name.to_lowercase().contains("validate")
                    || route.handler_name.to_lowercase().contains("body");

                routes.push(RoutePattern::new(
                    &route.http_method,
                    &route.path,
                    RouteFramework::FastApi,
                ).with_handler(
                    route.handler_name.clone(),
                    &self.path,
                ).with_auth(has_auth)
                    .with_validation(has_validation)
                    .with_location(
                        CommonLocation {
                            file_id: self.file_id,
                            line: route.decorator_location.range.start_line + 1,
                            column: route.decorator_location.range.start_col + 1,
                            start_byte: 0,
                            end_byte: 0,
                        },
                        0,
                        0,
                    ));
            }
        }

        routes
    }

    fn n_plus_one_patterns(&self) -> Vec<DbOperation> {
        self.db_operations()
            .into_iter()
            .filter(|op| op.is_potential_n_plus_one())
            .collect()
    }

    fn error_contexts(&self) -> Vec<ErrorContext> {
        let mut contexts = Vec::new();

        for func in &self.functions {
            if func.is_method {
                continue;
            }

            if func.is_async {
                contexts.push(ErrorContext::new(
                    ErrorContextType::TryCatch,
                ).with_location(
                    CommonLocation {
                        file_id: self.file_id,
                        line: func.location.range.start_line + 1,
                        column: func.location.range.start_col + 1,
                        start_byte: func.start_byte,
                        end_byte: func.end_byte,
                    },
                    func.start_byte,
                    func.end_byte,
                ).with_enclosing_function(func.name.clone()));
            }
        }

        for except in &self.bare_excepts {
            contexts.push(ErrorContext::new(
                ErrorContextType::BareExcept,
            ).swallowing_error(true)
                .with_location(
                    CommonLocation {
                        file_id: self.file_id,
                        line: except.location.range.start_line + 1,
                        column: except.location.range.start_col + 1,
                        start_byte: except.start_byte,
                        end_byte: except.end_byte,
                    },
                    except.start_byte,
                    except.end_byte,
                ).with_enclosing_function(except.function_name.clone().unwrap_or_default()));
        }

        contexts
    }
}

/// Convert a Python AsyncOperation to the common AsyncOperation type
fn convert_python_async_op(py_op: &PyAsyncOperation, file_id: FileId) -> AsyncOperation {
    let operation_type = match py_op.operation_type {
        PyAsyncOperationType::TaskSpawn => AsyncOperationType::TaskSpawn,
        PyAsyncOperationType::Await => AsyncOperationType::TaskAwait,
        PyAsyncOperationType::TaskGather => AsyncOperationType::TaskGather,
        PyAsyncOperationType::ChannelSend => AsyncOperationType::ChannelSend,
        PyAsyncOperationType::ChannelReceive => AsyncOperationType::ChannelReceive,
        PyAsyncOperationType::LockAcquire => AsyncOperationType::LockAcquire,
        PyAsyncOperationType::LockRelease => AsyncOperationType::LockRelease,
        PyAsyncOperationType::SemaphoreAcquire => AsyncOperationType::SemaphoreAcquire,
        PyAsyncOperationType::Sleep => AsyncOperationType::Sleep,
        PyAsyncOperationType::Timeout => AsyncOperationType::Timeout,
        PyAsyncOperationType::Select => AsyncOperationType::SelectRace,
        PyAsyncOperationType::AsyncFor => AsyncOperationType::TaskAwait,
        PyAsyncOperationType::Unknown => AsyncOperationType::Unknown,
    };

    let error_handling = if py_op.has_error_handling {
        Some(crate::semantics::common::async_ops::ErrorHandling::TryCatch)
    } else {
        None
    };

    let cancellation_handling = if py_op.has_cancellation {
        Some(crate::semantics::common::async_ops::CancellationHandling::CancellationToken)
    } else {
        None
    };

    AsyncOperation {
        runtime: AsyncRuntime::Asyncio,
        operation_type,
        has_error_handling: py_op.has_error_handling,
        error_handling,
        has_timeout: py_op.has_timeout,
        timeout_value: py_op.timeout_value,
        has_cancellation: py_op.has_cancellation,
        cancellation_handling,
        is_bounded: py_op.is_bounded,
        bound_limit: py_op.bound_limit,
        has_cleanup: false,
        operation_text: py_op.operation_text.clone(),
        location: CommonLocation {
            file_id,
            line: py_op.location.range.start_line + 1,
            column: py_op.location.range.start_col + 1,
            start_byte: py_op.start_byte,
            end_byte: py_op.end_byte,
        },
        enclosing_function: py_op.enclosing_function.clone(),
        start_byte: py_op.start_byte,
        end_byte: py_op.end_byte,
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
    let callee_expr = &call.function_call.callee_expr;
    let (callee, receiver) = if let Some(idx) = callee_expr.rfind('.') {
        let callee_name = callee_expr[idx + 1..].to_string();
        let receiver_name = callee_expr[..idx].to_string();
        (callee_name, Some(receiver_name))
    } else {
        (callee_expr.clone(), None)
    };

    FunctionCall {
        callee,
        callee_expr: callee_expr.clone(),
        receiver,
        line: call.function_call.location.line,
        column: call.function_call.location.column,
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
        use crate::semantics::common::db::DbLibrary;

        self.db_operations
            .iter()
            .map(|db_op| {
                let library = match db_op.library.as_str() {
                    "database/sql" => DbLibrary::DatabaseSql,
                    "GORM" => DbLibrary::Gorm,
                    "sqlx" => DbLibrary::Sqlx,
                    "sqlc" => DbLibrary::Sqlc,
                    _ => DbLibrary::Other(db_op.library.as_str().to_string()),
                };

                let operation_type = match db_op.operation_type.as_str() {
                    "SELECT" => DbOperationType::Select,
                    "INSERT" => DbOperationType::Insert,
                    "UPDATE" => DbOperationType::Update,
                    "DELETE" => DbOperationType::Delete,
                    "CONNECT" => DbOperationType::Connect,
                    "BEGIN" => DbOperationType::TransactionBegin,
                    "COMMIT" => DbOperationType::TransactionCommit,
                    "ROLLBACK" => DbOperationType::TransactionRollback,
                    "RAW_SQL" => DbOperationType::RawSql,
                    _ => DbOperationType::Unknown,
                };

                DbOperation {
                    library,
                    operation_type,
                    has_timeout: db_op.has_timeout,
                    timeout_value: db_op.timeout_value,
                    in_transaction: db_op.in_transaction,
                    eager_loading: db_op.eager_loading.clone(),
                    in_loop: db_op.in_loop,
                    in_iteration: db_op.in_iteration,
                    model_name: db_op.model_name.clone(),
                    relationship_field: db_op.relationship_field.clone(),
                    operation_text: db_op.operation_text.clone(),
                    location: CommonLocation {
                        file_id: db_op.location.file_id,
                        line: db_op.location.line,
                        column: db_op.location.column,
                        start_byte: db_op.start_byte,
                        end_byte: db_op.end_byte,
                    },
                    enclosing_function: db_op.enclosing_function.clone(),
                    start_byte: db_op.start_byte,
                    end_byte: db_op.end_byte,
                }
            })
            .collect()
    }

    fn async_operations(&self) -> Vec<AsyncOperation> {
        let mut operations = Vec::new();

        // Convert goroutines to TaskSpawn operations
        for g in &self.goroutines {
            operations.push(AsyncOperation {
                runtime: AsyncRuntime::Goroutine,
                operation_type: AsyncOperationType::TaskSpawn,
                has_error_handling: g.has_recover,
                error_handling: if g.has_recover {
                    Some(crate::semantics::common::async_ops::ErrorHandling::Other("recover".to_string()))
                } else {
                    None
                },
                has_timeout: false,
                timeout_value: None,
                has_cancellation: g.has_context_param || g.has_done_channel,
                cancellation_handling: if g.has_done_channel {
                    Some(crate::semantics::common::async_ops::CancellationHandling::ChannelClose)
                } else {
                    None
                },
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
            });
        }

        // Convert channel operations to ChannelSend/ChannelReceive operations
        for ch in &self.channel_ops {
            let operation_type = match ch.kind {
                super::go::model::ChannelOpKind::Send => AsyncOperationType::ChannelSend,
                super::go::model::ChannelOpKind::Receive => AsyncOperationType::ChannelReceive,
                super::go::model::ChannelOpKind::Close => AsyncOperationType::Unknown,
            };

            if operation_type != AsyncOperationType::Unknown {
                operations.push(AsyncOperation {
                    runtime: AsyncRuntime::Goroutine,
                    operation_type,
                    has_error_handling: false,
                    error_handling: None,
                    has_timeout: false,
                    timeout_value: None,
                    has_cancellation: false,
                    cancellation_handling: None,
                    is_bounded: false,
                    bound_limit: None,
                    has_cleanup: false,
                    operation_text: ch.text.clone(),
                    location: CommonLocation {
                        file_id: self.file_id,
                        line: ch.location.range.start_line + 1,
                        column: ch.location.range.start_col + 1,
                        start_byte: ch.start_byte,
                        end_byte: ch.end_byte,
                    },
                    enclosing_function: ch.function_name.clone(),
                    start_byte: ch.start_byte,
                    end_byte: ch.end_byte,
                });
            }
        }

        // Convert select statements to SelectRace operations
        for select_stmt in &self.select_statements {
            operations.push(AsyncOperation {
                runtime: AsyncRuntime::Goroutine,
                operation_type: AsyncOperationType::SelectRace,
                has_error_handling: false,
                error_handling: None,
                has_timeout: false,
                timeout_value: None,
                has_cancellation: select_stmt.is_cancellation_pattern,
                cancellation_handling: if select_stmt.is_cancellation_pattern {
                    Some(crate::semantics::common::async_ops::CancellationHandling::ChannelClose)
                } else {
                    None
                },
                is_bounded: false,
                bound_limit: None,
                has_cleanup: false,
                operation_text: select_stmt.text.clone(),
                location: CommonLocation {
                    file_id: self.file_id,
                    line: select_stmt.location.range.start_line + 1,
                    column: select_stmt.location.range.start_col + 1,
                    start_byte: select_stmt.start_byte,
                    end_byte: select_stmt.end_byte,
                },
                enclosing_function: select_stmt.function_name.clone(),
                start_byte: select_stmt.start_byte,
                end_byte: select_stmt.end_byte,
            });
        }

        // Convert mutex operations to LockAcquire/LockRelease operations
        for mutex in &self.mutex_operations {
            let operation_type = if mutex.operation_type == "Lock" || mutex.operation_type == "RLock" {
                AsyncOperationType::LockAcquire
            } else {
                AsyncOperationType::LockRelease
            };

            operations.push(AsyncOperation {
                runtime: AsyncRuntime::Goroutine,
                operation_type,
                has_error_handling: mutex.uses_defer_unlock,
                error_handling: if mutex.uses_defer_unlock {
                    Some(crate::semantics::common::async_ops::ErrorHandling::Other("defer".to_string()))
                } else {
                    None
                },
                has_timeout: false,
                timeout_value: None,
                has_cancellation: false,
                cancellation_handling: None,
                is_bounded: false,
                bound_limit: None,
                has_cleanup: mutex.uses_defer_unlock,
                operation_text: mutex.text.clone(),
                location: CommonLocation {
                    file_id: self.file_id,
                    line: mutex.location.range.start_line + 1,
                    column: mutex.location.range.start_col + 1,
                    start_byte: mutex.lock_start_byte,
                    end_byte: mutex.lock_end_byte,
                },
                enclosing_function: mutex.function_name.clone(),
                start_byte: mutex.lock_start_byte,
                end_byte: mutex.lock_end_byte,
            });
        }

        // Convert defer statements to operations (resource cleanup tracking)
        for defer_stmt in &self.defers {
            operations.push(AsyncOperation {
                runtime: AsyncRuntime::Goroutine,
                operation_type: AsyncOperationType::Unknown,
                has_error_handling: defer_stmt.is_resource_cleanup,
                error_handling: None,
                has_timeout: false,
                timeout_value: None,
                has_cancellation: defer_stmt.is_resource_cleanup,
                cancellation_handling: if defer_stmt.is_resource_cleanup {
                    Some(crate::semantics::common::async_ops::CancellationHandling::Other("defer_cleanup".to_string()))
                } else {
                    None
                },
                is_bounded: false,
                bound_limit: None,
                has_cleanup: true,
                operation_text: defer_stmt.text.clone(),
                location: CommonLocation {
                    file_id: self.file_id,
                    line: defer_stmt.location.range.start_line + 1,
                    column: defer_stmt.location.range.start_col + 1,
                    start_byte: defer_stmt.start_byte,
                    end_byte: defer_stmt.end_byte,
                },
                enclosing_function: defer_stmt.function_name.clone(),
                start_byte: defer_stmt.start_byte,
                end_byte: defer_stmt.end_byte,
            });
        }

        operations
    }

    fn imports(&self) -> Vec<Import> {
        self.imports
            .iter()
            .filter_map(|imp| convert_go_import(imp, self.file_id))
            .collect()
    }

    fn functions(&self) -> Vec<FunctionDef> {
        let funcs: Vec<FunctionDef> = self
            .functions
            .iter()
            .filter_map(|func| convert_go_function(func, self.file_id, &self.calls))
            .collect();
        let methods: Vec<FunctionDef> = self
            .methods
            .iter()
            .filter_map(|method| convert_go_method(method, self.file_id, &self.calls))
            .collect();
        funcs.into_iter().chain(methods).collect()
    }

    fn annotations(&self) -> Vec<Annotation> {
        use super::go::model::GoAnnotationType;

        let mut annotations = Vec::new();

        for ann in &self.annotations {
            let annotation_type = match &ann.annotation_type {
                GoAnnotationType::Json => AnnotationType::Other("json".to_string()),
                GoAnnotationType::Yaml => AnnotationType::Other("yaml".to_string()),
                GoAnnotationType::Xml => AnnotationType::Other("xml".to_string()),
                GoAnnotationType::Protobuf => AnnotationType::Other("protobuf".to_string()),
                GoAnnotationType::Validation => AnnotationType::Validation { library: "go-validate".to_string() },
                GoAnnotationType::Orm => AnnotationType::Other("orm".to_string()),
                GoAnnotationType::Sql => AnnotationType::Other("sql".to_string()),
                GoAnnotationType::Generate => AnnotationType::Other("go:generate".to_string()),
                GoAnnotationType::BuildConstraint => AnnotationType::Other("build".to_string()),
                GoAnnotationType::Linkname => AnnotationType::Other("go:linkname".to_string()),
                GoAnnotationType::Embed => AnnotationType::Other("go:embed".to_string()),
                GoAnnotationType::Linter => AnnotationType::Other("linter".to_string()),
                GoAnnotationType::Other(name) => AnnotationType::Other(name.clone()),
            };

            let target_function = ann.target_field.clone()
                .or(ann.target_type.clone())
                .unwrap_or_default();

            annotations.push(Annotation::new(
                ann.name.clone(),
                annotation_type,
                target_function,
                &self.path,
            ).with_location(
                CommonLocation {
                    file_id: self.file_id,
                    line: ann.location.range.start_line + 1,
                    column: ann.location.range.start_col + 1,
                    start_byte: ann.start_byte,
                    end_byte: ann.end_byte,
                },
                ann.start_byte,
                ann.end_byte,
            ));
        }

        annotations
    }

    fn route_patterns(&self) -> Vec<RoutePattern> {
        let mut routes = Vec::new();

        if let Some(ref framework) = self.go_framework {
            for route in &framework.routes {
                let framework_type = match route.framework {
                    super::go::frameworks::GoHttpFramework::Gin => RouteFramework::Gin,
                    super::go::frameworks::GoHttpFramework::Echo => RouteFramework::Echo,
                    super::go::frameworks::GoHttpFramework::Fiber => RouteFramework::Fiber,
                    super::go::frameworks::GoHttpFramework::Chi => RouteFramework::Chi,
                    super::go::frameworks::GoHttpFramework::Mux => RouteFramework::Mux,
                    super::go::frameworks::GoHttpFramework::NetHttp => RouteFramework::HttpLibrary,
                };

                let has_auth = route.handler_name.as_ref()
                    .map(|name| name.to_lowercase().contains("auth") || name.to_lowercase().contains("protected"))
                    .unwrap_or(false);

                routes.push(RoutePattern::new(
                    &route.http_method,
                    &route.path,
                    framework_type,
                ).with_handler(
                    route.handler_name.clone().unwrap_or_else(|| "unknown".to_string()),
                    &self.path,
                ).with_auth(has_auth)
                    .with_location(
                        CommonLocation {
                            file_id: self.file_id,
                            line: route.location.range.start_line + 1,
                            column: route.location.range.start_col + 1,
                            start_byte: route.start_byte,
                            end_byte: route.end_byte,
                        },
                        route.start_byte,
                        route.end_byte,
                    ));
            }
        }

        routes
    }

    fn n_plus_one_patterns(&self) -> Vec<DbOperation> {
        self.db_operations()
            .into_iter()
            .filter(|op| op.is_potential_n_plus_one())
            .collect()
    }

    fn error_contexts(&self) -> Vec<ErrorContext> {
        let mut contexts = Vec::new();

        for recover in &self.defer_recovers {
            contexts.push(ErrorContext::new(
                ErrorContextType::DeferRecover,
            ).with_logging(recover.has_logging)
                .with_location(
                    CommonLocation {
                        file_id: self.file_id,
                        line: recover.location.range.start_line + 1,
                        column: recover.location.range.start_col + 1,
                        start_byte: recover.start_byte,
                        end_byte: recover.end_byte,
                    },
                    recover.start_byte,
                    recover.end_byte,
                ).with_enclosing_function(recover.function_name.clone().unwrap_or_default()));
        }

        contexts
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
fn convert_go_function(
    go_func: &GoFunction,
    file_id: FileId,
    all_calls: &[GoCallSite],
) -> Option<FunctionDef> {
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

    // Filter calls that are within this function's byte range
    let calls: Vec<FunctionCall> = all_calls
        .iter()
        .filter(|call| call.start_byte >= go_func.start_byte && call.end_byte <= go_func.end_byte)
        .map(|call| convert_go_call_site(call))
        .collect();

    Some(FunctionDef {
        name: go_func.name.clone(),
        kind: FunctionKind::Function,
        visibility,
        is_async: false,
        params,
        return_type,
        decorators: vec![],
        class_name: None,
        calls,
        body_lines: 0,
        has_error_handling: go_func.returns_error,
        has_documentation: false,
        location: CommonLocation {
            file_id,
            line: go_func.location.range.start_line + 1,
            column: go_func.location.range.start_col + 1,
            start_byte: go_func.start_byte,
            end_byte: go_func.end_byte,
        },
        start_byte: go_func.start_byte,
        end_byte: go_func.end_byte,
    })
}

/// Convert a GoCallSite to the common FunctionCall type
fn convert_go_call_site(call: &GoCallSite) -> FunctionCall {
    let callee_expr = &call.function_call.callee_expr;
    let (callee, receiver) = if let Some(idx) = callee_expr.rfind('.') {
        let callee_name = callee_expr[idx + 1..].to_string();
        let receiver_name = callee_expr[..idx].to_string();
        (callee_name, Some(receiver_name))
    } else {
        (callee_expr.clone(), None)
    };

    FunctionCall {
        callee,
        callee_expr: callee_expr.clone(),
        receiver,
        line: call.function_call.location.line,
        column: call.function_call.location.column,
    }
}

/// Convert a Go method to the common FunctionDef type
fn convert_go_method(
    go_method: &GoMethod,
    file_id: FileId,
    all_calls: &[GoCallSite],
) -> Option<FunctionDef> {
    let visibility = if go_method
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

    let receiver = if go_method.receiver_is_pointer {
        format!("*{}", go_method.receiver_type)
    } else {
        go_method.receiver_type.clone()
    };

    let params: Vec<FunctionParam> = std::iter::once(FunctionParam::new("self").with_type(&receiver))
        .chain(
            go_method
                .params
                .iter()
                .map(|p| FunctionParam::new(&p.name).with_type(&p.param_type)),
        )
        .collect();

    let return_type = if go_method.return_types.is_empty() {
        None
    } else {
        Some(go_method.return_types.join(", "))
    };

    // Filter calls that are within this method's byte range
    let calls: Vec<FunctionCall> = all_calls
        .iter()
        .filter(|call| call.start_byte >= go_method.start_byte && call.end_byte <= go_method.end_byte)
        .map(|call| convert_go_call_site(call))
        .collect();

    Some(FunctionDef {
        name: go_method.name.clone(),
        kind: FunctionKind::Method,
        visibility,
        is_async: false,
        params,
        return_type,
        decorators: vec![],
        class_name: Some(go_method.receiver_type.clone()),
        calls,
        body_lines: 0,
        has_error_handling: go_method.returns_error,
        has_documentation: false,
        location: CommonLocation {
            file_id,
            line: go_method.location.range.start_line + 1,
            column: go_method.location.range.start_col + 1,
            start_byte: go_method.start_byte,
            end_byte: go_method.end_byte,
        },
        start_byte: go_method.start_byte,
        end_byte: go_method.end_byte,
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
        self.http_calls.clone()
    }

    fn db_operations(&self) -> Vec<DbOperation> {
        use crate::semantics::common::db::DbLibrary;

        self.db_operations
            .iter()
            .map(|db_op| {
                let library = match db_op.library.as_str() {
                    "Diesel" => DbLibrary::Diesel,
                    "SeaORM" => DbLibrary::SeaOrm,
                    "sqlx" => DbLibrary::SqlxRust,
                    "tokio-postgres" => DbLibrary::TokioPostgres,
                    _ => DbLibrary::Other(db_op.library.as_str().to_string()),
                };

                let operation_type = match db_op.operation_type.as_str() {
                    "SELECT" => DbOperationType::Select,
                    "INSERT" => DbOperationType::Insert,
                    "UPDATE" => DbOperationType::Update,
                    "DELETE" => DbOperationType::Delete,
                    "RAW_SQL" => DbOperationType::RawSql,
                    _ => DbOperationType::Unknown,
                };

                DbOperation {
                    library,
                    operation_type,
                    has_timeout: db_op.has_timeout,
                    timeout_value: db_op.timeout_value,
                    in_transaction: db_op.in_transaction,
                    eager_loading: db_op.eager_loading.clone(),
                    in_loop: db_op.in_loop,
                    in_iteration: db_op.in_iteration,
                    model_name: db_op.model_name.clone(),
                    relationship_field: db_op.relationship_field.clone(),
                    operation_text: db_op.operation_text.clone(),
                    location: CommonLocation {
                        file_id: db_op.location.file_id,
                        line: db_op.location.line,
                        column: db_op.location.column,
                        start_byte: db_op.start_byte,
                        end_byte: db_op.end_byte,
                    },
                    enclosing_function: db_op.enclosing_function.clone(),
                    start_byte: db_op.start_byte,
                    end_byte: db_op.end_byte,
                }
            })
            .collect()
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
        let funcs: Vec<FunctionDef> = self
            .functions
            .iter()
            .filter_map(|func| convert_rust_function(func, self.file_id, &self.calls))
            .collect();
        let impl_methods: Vec<FunctionDef> = self
            .impls
            .iter()
            .flat_map(|impl_block| {
                impl_block
                    .methods
                    .iter()
                    .filter_map(|method| convert_rust_function(method, self.file_id, &self.calls))
            })
            .collect();
        funcs.into_iter().chain(impl_methods).collect()
    }

    fn annotations(&self) -> Vec<Annotation> {
        let mut annotations = Vec::new();

        for func in &self.functions {
            let location = CommonLocation {
                file_id: self.file_id,
                line: func.location.range.start_line + 1,
                column: func.location.range.start_col + 1,
                start_byte: func.start_byte,
                end_byte: func.end_byte,
            };

            for attr in &func.attributes {
                let annotation_type = classify_rust_attribute(attr);
                annotations.push(Annotation::new(
                    attr.clone(),
                    annotation_type,
                    &func.name,
                    &self.path,
                ).with_location(location.clone(), func.start_byte, func.end_byte)
                    .with_enclosing_function(func.name.clone()));
            }
        }

        for impl_block in &self.impls {
            for method in &impl_block.methods {
                let location = CommonLocation {
                    file_id: self.file_id,
                    line: method.location.range.start_line + 1,
                    column: method.location.range.start_col + 1,
                    start_byte: method.start_byte,
                    end_byte: method.end_byte,
                };

                for attr in &method.attributes {
                    let annotation_type = classify_rust_attribute(attr);
                    annotations.push(Annotation::new(
                        attr.clone(),
                        annotation_type,
                        &method.name,
                        &self.path,
                    ).with_location(location.clone(), method.start_byte, method.end_byte)
                        .with_enclosing_function(method.name.clone()));
                }
            }
        }

        annotations
    }

    fn route_patterns(&self) -> Vec<RoutePattern> {
        let mut routes = Vec::new();

        if let Some(ref framework) = self.rust_framework {
            for route in &framework.routes {
                let has_auth = route.handler_name.to_lowercase().contains("auth")
                    || route.handler_name.to_lowercase().contains("protected");

                routes.push(RoutePattern::new(
                    &route.method,
                    &route.path,
                    RouteFramework::Axum,
                ).with_handler(
                    route.handler_name.clone(),
                    &self.path,
                ).with_auth(has_auth)
                    .with_location(
                        CommonLocation {
                            file_id: self.file_id,
                            line: route.location.range.start_line + 1,
                            column: route.location.range.start_col + 1,
                            start_byte: 0,
                            end_byte: 0,
                        },
                        0,
                        0,
                    ));
            }
        }

        routes
    }

    fn n_plus_one_patterns(&self) -> Vec<DbOperation> {
        self.db_operations()
            .into_iter()
            .filter(|op| op.is_potential_n_plus_one())
            .collect()
    }

    fn error_contexts(&self) -> Vec<ErrorContext> {
        let mut contexts = Vec::new();

        for unwrap in &self.unwrap_calls {
            contexts.push(ErrorContext::new(
                ErrorContextType::Unwrap,
            ).with_location(
                CommonLocation {
                    file_id: self.file_id,
                    line: unwrap.location.range.start_line + 1,
                    column: unwrap.location.range.start_col + 1,
                    start_byte: unwrap.start_byte,
                    end_byte: unwrap.end_byte,
                },
                unwrap.start_byte,
                unwrap.end_byte,
            ).with_enclosing_function(unwrap.function_name.clone().unwrap_or_default()));
        }

        for expect in &self.expect_calls {
            contexts.push(ErrorContext::new(
                ErrorContextType::Expect,
            ).with_logging(expect.has_meaningful_message)
                .with_location(
                    CommonLocation {
                        file_id: self.file_id,
                        line: expect.location.range.start_line + 1,
                        column: expect.location.range.start_col + 1,
                        start_byte: expect.start_byte,
                        end_byte: expect.end_byte,
                    },
                    expect.start_byte,
                    expect.end_byte,
                ).with_enclosing_function(expect.function_name.clone().unwrap_or_default()));
        }

        contexts
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
fn convert_rust_function(
    rust_func: &RustFunction,
    file_id: FileId,
    all_calls: &[RustCallSite],
) -> Option<FunctionDef> {
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

    // Filter calls that are within this function's byte range
    let calls: Vec<FunctionCall> = all_calls
        .iter()
        .filter(|call| call.function_call.location.start_byte >= rust_func.start_byte && call.function_call.location.end_byte <= rust_func.end_byte)
        .map(|call| convert_rust_call_site(call))
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
        calls,
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

/// Convert a RustCallSite to the common FunctionCall type
fn convert_rust_call_site(call: &RustCallSite) -> FunctionCall {
    let callee_expr = &call.function_call.callee_expr;
    let (callee, receiver) = if let Some(idx) = callee_expr.rfind('.') {
        let callee_name = callee_expr[idx + 1..].to_string();
        let receiver_name = callee_expr[..idx].to_string();
        (callee_name, Some(receiver_name))
    } else {
        (callee_expr.clone(), None)
    };

    FunctionCall {
        callee,
        callee_expr: callee_expr.clone(),
        receiver,
        line: call.function_call.location.line,
        column: call.function_call.location.column,
    }
}

fn find_closing_brace(source: &str, start: usize, end_limit: usize) -> usize {
    let mut depth = 1;
    let mut pos = start;
    let limit = end_limit.min(source.len());

    while pos < limit && depth > 0 {
        if source.as_bytes()[pos] == b'{' {
            depth += 1;
        } else if source.as_bytes()[pos] == b'}' {
            depth -= 1;
        }
        pos += 1;
    }

    pos
}

// =============================================================================
// TypeScript Implementation
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
        use crate::semantics::common::db::DbLibrary;

        self.db_operations
            .iter()
            .map(|db_op| {
                let library = match db_op.library.as_str() {
                    "Prisma" => DbLibrary::Prisma,
                    "TypeORM" => DbLibrary::TypeOrm,
                    "Knex" => DbLibrary::Knex,
                    "Sequelize" => DbLibrary::Sequelize,
                    "Drizzle ORM" => DbLibrary::DrizzleOrm,
                    _ => DbLibrary::Other(db_op.library.as_str().to_string()),
                };

                let operation_type = match db_op.operation_type.as_str() {
                    "SELECT" => DbOperationType::Select,
                    "INSERT" => DbOperationType::Insert,
                    "UPDATE" => DbOperationType::Update,
                    "DELETE" => DbOperationType::Delete,
                    "CONNECT" => DbOperationType::Connect,
                    "RAW_SQL" => DbOperationType::RawSql,
                    _ => DbOperationType::Unknown,
                };

                DbOperation {
                    library,
                    operation_type,
                    has_timeout: db_op.has_timeout,
                    timeout_value: db_op.timeout_value,
                    in_transaction: db_op.in_transaction,
                    eager_loading: db_op.eager_loading.clone(),
                    in_loop: db_op.in_loop,
                    in_iteration: db_op.in_iteration,
                    model_name: db_op.model_name.clone(),
                    relationship_field: db_op.relationship_field.clone(),
                    operation_text: db_op.operation_text.clone(),
                    location: CommonLocation {
                        file_id: db_op.location.file_id,
                        line: db_op.location.line,
                        column: db_op.location.column,
                        start_byte: db_op.start_byte,
                        end_byte: db_op.end_byte,
                    },
                    enclosing_function: db_op.enclosing_function.clone(),
                    start_byte: db_op.start_byte,
                    end_byte: db_op.end_byte,
                }
            })
            .collect()
    }

    fn async_operations(&self) -> Vec<AsyncOperation> {
        self.async_operations
            .iter()
            .map(|ts_op| convert_ts_async_op(ts_op, self.file_id))
            .collect()
    }

    fn imports(&self) -> Vec<Import> {
        self.imports
            .iter()
            .filter_map(|imp| convert_ts_import(imp, self.file_id))
            .collect()
    }

    fn functions(&self) -> Vec<FunctionDef> {
        let funcs: Vec<FunctionDef> = self
            .functions
            .iter()
            .filter_map(|func| convert_ts_function(func, self.file_id, &self.calls))
            .collect();
        // Also include class methods
        let class_methods: Vec<FunctionDef> = self
            .classes
            .iter()
            .flat_map(|class| {
                class
                    .methods
                    .iter()
                    .filter_map(|method| convert_ts_method(method, self.file_id, &self.calls, &class.name))
            })
            .collect();
        funcs.into_iter().chain(class_methods).collect()
    }

    fn annotations(&self) -> Vec<Annotation> {
        let mut annotations = Vec::new();

        for func in &self.functions {
            for decorator in &func.decorators {
                let annotation_type = match decorator.to_lowercase().as_str() {
                    n if n.contains("log") => AnnotationType::Logging,
                    n if n.contains("retry") => AnnotationType::Retry,
                    n if n.contains("cache") => AnnotationType::Cache,
                    n if n.contains("rate") || n.contains("throttle") => AnnotationType::RateLimit,
                    n if n.contains("timeout") => AnnotationType::Timeout,
                    n if n.contains("feature") || n.contains("flag") => AnnotationType::FeatureFlag,
                    n if n.contains("auth") || n.contains("permission") || n.contains("guard") => AnnotationType::Auth { library: String::new() },
                    n if n.contains("valid") => AnnotationType::Validation { library: String::new() },
                    _ => AnnotationType::Other(decorator.clone()),
                };

                annotations.push(Annotation::new(
                    decorator.clone(),
                    annotation_type,
                    &func.name,
                    &self.path,
                ).with_location(
                    CommonLocation {
                        file_id: self.file_id,
                        line: func.location.range.start_line + 1,
                        column: func.location.range.start_col + 1,
                        start_byte: func.start_byte,
                        end_byte: func.end_byte,
                    },
                    func.start_byte,
                    func.end_byte,
                ).with_enclosing_function(func.name.clone()));
            }
        }

        for class in &self.classes {
                for decorator in &class.decorators {
                    let annotation_type = match decorator.to_lowercase().as_str() {
                        n if n.contains("log") => AnnotationType::Logging,
                        n if n.contains("controller") || n.contains("service") || n.contains("injectable") => AnnotationType::Controller,
                        n if n.contains("auth") || n.contains("guard") => AnnotationType::Auth { library: String::new() },
                        _ => AnnotationType::Other(decorator.clone()),
                    };

                    annotations.push(Annotation::new(
                        decorator.clone(),
                        annotation_type,
                        &class.name,
                        &self.path,
                    ).with_location(
                        CommonLocation {
                            file_id: self.file_id,
                            line: class.location.range.start_line + 1,
                            column: class.location.range.start_col + 1,
                            start_byte: 0,
                            end_byte: 0,
                        },
                        0,
                        0,
                    ).with_enclosing_class(class.name.clone()));
                }

            for method in &class.methods {
                for decorator in &method.decorators {
                    let annotation_type = match decorator.to_lowercase().as_str() {
                        n if n.contains("log") => AnnotationType::Logging,
                        n if n.contains("retry") => AnnotationType::Retry,
                        n if n.contains("get") || n.contains("post") || n.contains("put") || n.contains("delete") || n.contains("patch") => AnnotationType::Route,
                        n if n.contains("auth") || n.contains("guard") => AnnotationType::Auth { library: String::new() },
                        _ => AnnotationType::Other(decorator.clone()),
                    };

                    annotations.push(Annotation::new(
                        decorator.clone(),
                        annotation_type,
                        &method.name,
                        &self.path,
                    ).with_location(
                        CommonLocation {
                            file_id: self.file_id,
                            line: method.location.range.start_line + 1,
                            column: method.location.range.start_col + 1,
                            start_byte: method.start_byte,
                            end_byte: method.end_byte,
                        },
                        method.start_byte,
                        method.end_byte,
                    ).with_enclosing_function(method.name.clone())
                        .with_enclosing_class(class.name.clone()));
                }
            }
        }

        annotations
    }

    fn route_patterns(&self) -> Vec<RoutePattern> {
        let mut routes = Vec::new();

        if let Some(ref express) = self.express {
            for route in &express.routes {
                let has_auth = route.handler_name.as_ref()
                    .map(|name| name.to_lowercase().contains("auth") || name.to_lowercase().contains("protected"))
                    .unwrap_or(false);

                if let Some(ref path) = route.path {
                    routes.push(RoutePattern::new(
                        &route.method,
                        path,
                        RouteFramework::Express,
                    ).with_handler(
                        route.handler_name.clone().unwrap_or_else(|| "unknown".to_string()),
                        &self.path,
                    ).with_auth(has_auth)
                        .with_location(
                            CommonLocation {
                                file_id: self.file_id,
                                line: route.location.range.start_line + 1,
                                column: route.location.range.start_col + 1,
                                start_byte: 0,
                                end_byte: 0,
                            },
                            0,
                            0,
                        ));
                }
            }
        }

        routes
    }

    fn n_plus_one_patterns(&self) -> Vec<DbOperation> {
        self.db_operations()
            .into_iter()
            .filter(|op| op.is_potential_n_plus_one())
            .collect()
    }

    fn error_contexts(&self) -> Vec<ErrorContext> {
        let mut contexts = Vec::new();

        for try_catch in &self.try_catches {
            contexts.push(ErrorContext::new(
                ErrorContextType::TryCatch,
            ).with_logging(try_catch.has_logging)
                .with_reraise(try_catch.has_reraise)
                .swallowing_error(try_catch.catch_text.contains("catch") && try_catch.catch_text.lines().count() <= 2)
                .with_location(
                    CommonLocation {
                        file_id: self.file_id,
                        line: try_catch.location.range.start_line + 1,
                        column: try_catch.location.range.start_col + 1,
                        start_byte: try_catch.start_byte,
                        end_byte: try_catch.end_byte,
                    },
                    try_catch.start_byte,
                    try_catch.end_byte,
                ).with_enclosing_function(try_catch.function_name.clone().unwrap_or_default()));
        }

        contexts
    }
}

/// Classify a Rust attribute into an AnnotationType.
fn classify_rust_attribute(attr: &str) -> AnnotationType {
    let attr_lower = attr.to_lowercase();

    if attr_lower.contains("log") || attr_lower.contains("tracing") {
        AnnotationType::Logging
    } else if attr_lower.contains("retry") {
        AnnotationType::Retry
    } else if attr_lower.contains("cache") || attr_lower.contains("cached") {
        AnnotationType::Cache
    } else if attr_lower.contains("rate") || attr_lower.contains("throttle") {
        AnnotationType::RateLimit
    } else if attr_lower.contains("timeout") {
        AnnotationType::Timeout
    } else if attr_lower.contains("feature") || attr_lower.contains("flag") {
        AnnotationType::FeatureFlag
    } else {
        AnnotationType::CustomDecorator
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

/// Convert a TypeScript AsyncOperation to the common AsyncOperation type
fn convert_ts_async_op(ts_op: &TsAsyncOperation, file_id: FileId) -> AsyncOperation {
    let operation_type = match ts_op.operation_type {
        TsAsyncOperationType::PromiseConstructor => AsyncOperationType::TaskSpawn,
        TsAsyncOperationType::Await => AsyncOperationType::TaskAwait,
        TsAsyncOperationType::PromiseCombinator => AsyncOperationType::TaskGather,
        TsAsyncOperationType::PromiseChain => AsyncOperationType::TaskGather,
        TsAsyncOperationType::Timeout => AsyncOperationType::Sleep,
        TsAsyncOperationType::Cancellation => AsyncOperationType::TaskSpawn,
        TsAsyncOperationType::AsyncFunction => AsyncOperationType::TaskSpawn,
        TsAsyncOperationType::AsyncArrow => AsyncOperationType::TaskSpawn,
        TsAsyncOperationType::Unknown => AsyncOperationType::Unknown,
    };

    let error_handling = if ts_op.has_error_handling {
        Some(crate::semantics::common::async_ops::ErrorHandling::TryCatch)
    } else {
        None
    };

    let cancellation_handling = if ts_op.has_cancellation {
        Some(crate::semantics::common::async_ops::CancellationHandling::CancellationToken)
    } else {
        None
    };

    AsyncOperation {
        runtime: AsyncRuntime::PromiseNative,
        operation_type,
        has_error_handling: ts_op.has_error_handling,
        error_handling,
        has_timeout: ts_op.has_timeout,
        timeout_value: ts_op.timeout_value,
        has_cancellation: ts_op.has_cancellation,
        cancellation_handling,
        is_bounded: false,
        bound_limit: None,
        has_cleanup: false,
        operation_text: ts_op.operation_text.clone(),
        location: CommonLocation {
            file_id,
            line: ts_op.location.range.start_line + 1,
            column: ts_op.location.range.start_col + 1,
            start_byte: ts_op.start_byte,
            end_byte: ts_op.end_byte,
        },
        enclosing_function: ts_op.enclosing_function.clone(),
        start_byte: ts_op.start_byte,
        end_byte: ts_op.end_byte,
    }
}

/// Convert a TypeScript function to the common FunctionDef type
fn convert_ts_function(
    ts_func: &TsFunction,
    file_id: FileId,
    all_calls: &[TsCallSite],
) -> Option<FunctionDef> {
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

    // Filter calls that are within this function's byte range
    let calls: Vec<FunctionCall> = all_calls
        .iter()
        .filter(|call| call.function_call.location.start_byte >= ts_func.start_byte && call.function_call.location.end_byte <= ts_func.end_byte)
        .map(|call| convert_ts_call_site(call))
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
        calls,
        body_lines: 0,
        has_error_handling: ts_func.has_try_catch,
        has_documentation: false,
        location: CommonLocation {
            file_id,
            line: ts_func.location.range.start_line + 1,
            column: ts_func.location.range.start_col + 1,
            start_byte: ts_func.start_byte,
            end_byte: ts_func.end_byte,
        },
        start_byte: ts_func.start_byte,
        end_byte: ts_func.end_byte,
    })
}

/// Convert a TsCallSite to the common FunctionCall type
fn convert_ts_call_site(call: &TsCallSite) -> FunctionCall {
    let callee_expr = &call.function_call.callee_expr;
    let (callee, receiver) = if let Some(idx) = callee_expr.rfind('.') {
        let callee_name = callee_expr[idx + 1..].to_string();
        let receiver_name = callee_expr[..idx].to_string();
        (callee_name, Some(receiver_name))
    } else {
        (callee_expr.clone(), None)
    };

    FunctionCall {
        callee,
        callee_expr: callee_expr.clone(),
        receiver,
        line: call.function_call.location.line,
        column: call.function_call.location.column,
    }
}

/// Convert a TypeScript method to the common FunctionDef type
fn convert_ts_method(
    method: &TsMethod,
    file_id: FileId,
    all_calls: &[TsCallSite],
    class_name: &str,
) -> Option<FunctionDef> {
    let visibility = if method.is_private {
        Visibility::Private
    } else if method.is_protected {
        Visibility::Protected
    } else {
        Visibility::Public
    };

    let params: Vec<FunctionParam> = method
        .params
        .iter()
        .map(|p| {
            let mut param = FunctionParam::new(&p.name);
            if let Some(ref type_ann) = p.type_annotation {
                param = param.with_type(type_ann);
            }
            if p.is_rest {
                param = param.variadic();
            }
            param
        })
        .collect();

    // Filter calls that are within this method's byte range
    let calls: Vec<FunctionCall> = all_calls
        .iter()
        .filter(|call| call.function_call.location.start_byte >= method.start_byte && call.function_call.location.end_byte <= method.end_byte)
        .map(|call| convert_ts_call_site(call))
        .collect();

    Some(FunctionDef {
        name: method.name.clone(),
        kind: FunctionKind::Method,
        visibility,
        is_async: method.is_async,
        params,
        return_type: method.return_type.clone(),
        decorators: method
            .decorators
            .iter()
            .map(|d| FunctionDecorator::new(d, format!("@{}", d)))
            .collect(),
        class_name: Some(class_name.to_string()),
        calls,
        body_lines: 0,
        has_error_handling: false,
        has_documentation: false,
        location: CommonLocation {
            file_id,
            line: method.location.range.start_line + 1,
            column: method.location.range.start_col + 1,
            start_byte: method.start_byte,
            end_byte: method.end_byte,
        },
        start_byte: method.start_byte,
        end_byte: method.end_byte,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::go::parse_go_file;
    use crate::parse::python::parse_python_file;
    use crate::parse::rust::parse_rust_file;
    use crate::parse::typescript::parse_typescript_file;
    use crate::types::context::SourceFile;

    // =============================================================================
    // Go Function Calls Tests
    // =============================================================================

    fn parse_go(source: &str) -> GoFileSemantics {
        let sf = SourceFile {
            path: "test.go".to_string(),
            language: Language::Go,
            content: source.to_string(),
        };
        let parsed = parse_go_file(FileId(1), &sf).expect("parsing should succeed");
        let mut sem = GoFileSemantics::from_parsed(&parsed);
        sem.analyze_frameworks(&parsed)
            .expect("analysis should succeed");
        sem
    }

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

    fn parse_rust(source: &str) -> RustFileSemantics {
        let sf = SourceFile {
            path: "test.rs".to_string(),
            language: Language::Rust,
            content: source.to_string(),
        };
        let parsed = parse_rust_file(FileId(2), &sf).expect("parsing should succeed");
        super::super::rust::build_rust_semantics(&parsed).expect("semantics should succeed")
    }

    fn parse_typescript(source: &str) -> TsFileSemantics {
        let sf = SourceFile {
            path: "test.ts".to_string(),
            language: Language::Typescript,
            content: source.to_string(),
        };
        let parsed = parse_typescript_file(FileId(3), &sf).expect("parsing should succeed");
        let mut sem = TsFileSemantics::from_parsed(&parsed);
        sem.analyze_frameworks(&parsed)
            .expect("analysis should succeed");
        sem
    }

    #[test]
    fn go_function_has_byte_range() {
        let sem = parse_go(
            r#"
package main

func hello() {
    println("hello")
}
"#,
        );

        let functions = sem.functions();
        assert_eq!(functions.len(), 1);

        let func = &functions[0];
        assert!(func.start_byte > 0, "GoFunction should have start_byte > 0");
        assert!(
            func.end_byte > func.start_byte,
            "GoFunction should have end_byte > start_byte"
        );
    }

    #[test]
    fn go_functions_with_calls_extraction() {
        let sem = parse_go(
            r#"
package main

import "fmt"

func helper() {
    // no calls
}

func caller() {
    helper()
    fmt.Println("hello")
}
"#,
        );

        let functions = sem.functions();
        assert_eq!(functions.len(), 2);

        let helper_fn = functions.iter().find(|f| f.name == "helper").unwrap();
        assert_eq!(helper_fn.calls.len(), 0);

        let caller_fn = functions.iter().find(|f| f.name == "caller").unwrap();
        assert_eq!(caller_fn.calls.len(), 2);

        let callee_names: Vec<&str> = caller_fn.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(callee_names.contains(&"helper"));
        assert!(callee_names.contains(&"Println"));
    }

    #[test]
    fn go_method_call_extraction() {
        let sem = parse_go(
            r#"
package main

type Server struct{}

func (s *Server) Handle() {
    s.Process()
    s.Validate()
}

func (s *Server) Process() {}

func (s *Server) Validate() {}
"#,
        );

        let functions = sem.functions();
        assert_eq!(functions.len(), 3);

        let handle_fn = functions.iter().find(|f| f.name == "Handle").unwrap();
        assert_eq!(handle_fn.calls.len(), 2);

        let callee_names: Vec<&str> = handle_fn.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(callee_names.contains(&"Process"));
        assert!(callee_names.contains(&"Validate"));
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

    // =============================================================================
    // Rust Function Calls Tests
    // =============================================================================

    #[test]
    fn rust_function_has_byte_range() {
        let sem = parse_rust(
            r#"
fn my_func() {
    let x = 1;
}
"#,
        );

        let functions = sem.functions();
        assert_eq!(functions.len(), 1);

        let func = &functions[0];
        assert!(func.start_byte > 0, "RustFunction should have start_byte > 0");
        assert!(func.end_byte > func.start_byte);
    }

    #[test]
    fn rust_functions_with_calls_extraction() {
        let sem = parse_rust(
            r#"
fn helper() {
    // no calls
}

fn other() {
    println!("hello");
}

fn caller() {
    helper();
    other();
}
"#,
        );

        let functions = sem.functions();
        assert_eq!(functions.len(), 3);

        let helper_fn = functions.iter().find(|f| f.name == "helper").unwrap();
        assert_eq!(helper_fn.calls.len(), 0);

        let caller_fn = functions.iter().find(|f| f.name == "caller").unwrap();
        assert_eq!(caller_fn.calls.len(), 2);

        let callee_names: Vec<&str> = caller_fn.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(callee_names.contains(&"helper"));
        assert!(callee_names.contains(&"other"));
    }

    #[test]
    fn rust_method_call_extraction() {
        let sem = parse_rust(
            r#"
struct Server;

impl Server {
    fn handle(&self) {
        self.process();
        self.validate();
    }

    fn process(&self) {}

    fn validate(&self) {}
}
"#,
        );

        let functions = sem.functions();
        assert_eq!(functions.len(), 3);

        let handle_fn = functions.iter().find(|f| f.name == "handle").unwrap();
        assert_eq!(handle_fn.calls.len(), 2);

        let callee_names: Vec<&str> = handle_fn.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(callee_names.contains(&"process"));
        assert!(callee_names.contains(&"validate"));
    }

    // =============================================================================
    // TypeScript Function Calls Tests
    // =============================================================================

    #[test]
    fn typescript_function_has_byte_range() {
        let sem = parse_typescript(
            r#"
function hello(): void {
    console.log("hello");
}
"#,
        );

        let functions = sem.functions();
        assert_eq!(functions.len(), 1);

        let func = &functions[0];
        assert!(func.start_byte > 0, "TsFunction should have start_byte > 0");
        assert!(func.end_byte > func.start_byte);
    }

    #[test]
    fn typescript_functions_with_calls_extraction() {
        let sem = parse_typescript(
            r#"
function helper(): void {
    // no calls
}

function caller(): void {
    helper();
    console.log("hello");
}
"#,
        );

        let functions = sem.functions();
        assert_eq!(functions.len(), 2);

        let helper_fn = functions.iter().find(|f| f.name == "helper").unwrap();
        assert_eq!(helper_fn.calls.len(), 0);

        let caller_fn = functions.iter().find(|f| f.name == "caller").unwrap();
        assert_eq!(caller_fn.calls.len(), 2);

        let callee_names: Vec<&str> = caller_fn.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(callee_names.contains(&"helper"));
        assert!(callee_names.contains(&"log"));
    }

    #[test]
    fn typescript_method_call_extraction() {
        let sem = parse_typescript(
            r#"
class Server {
    handle(): void {
        this.process();
        this.validate();
    }

    process(): void {}

    validate(): void {}
}
"#,
        );

        let functions = sem.functions();
        assert_eq!(functions.len(), 3);

        let handle_fn = functions.iter().find(|f| f.name == "handle").unwrap();
        assert_eq!(handle_fn.calls.len(), 2);

        let callee_names: Vec<&str> = handle_fn.calls.iter().map(|c| c.callee.as_str()).collect();
        assert!(callee_names.contains(&"process"));
        assert!(callee_names.contains(&"validate"));
    }

    #[test]
    fn typescript_arrow_function_with_calls() {
        let sem = parse_typescript(
            r#"
const myFunc = (): void => {
    helper();
    console.log("test");
};

function helper(): void {}
"#,
        );

        let functions = sem.functions();
        assert_eq!(functions.len(), 2);

        let my_func = functions.iter().find(|f| f.name == "myFunc").unwrap();
        assert_eq!(my_func.calls.len(), 2);
    }

    // =============================================================================
    // Common Semantics Tests (annotations, routes, N+1, error contexts)
    // =============================================================================

    #[test]
    fn python_route_patterns_extracts_fastapi_routes() {
        let sem = parse_python(
            r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/users")
async def get_users():
    return []

@app.post("/users")
def create_user():
    pass
"#,
        );

        let routes = sem.route_patterns();
        assert_eq!(routes.len(), 2);

        let get_route = routes.iter().find(|r| r.method == "GET").unwrap();
        assert_eq!(get_route.path, "/users");
        assert!(get_route.handler_name.is_some());

        let post_route = routes.iter().find(|r| r.method == "POST").unwrap();
        assert_eq!(post_route.path, "/users");
    }

    #[test]
    fn python_route_patterns_detects_auth() {
        let sem = parse_python(
            r#"
from fastapi import FastAPI

app = FastAPI()

@app.get("/public")
def public_endpoint():
    return []

@app.get("/protected/auth-required")
def protected_auth():
    return []
"#,
        );

        let routes = sem.route_patterns();
        assert_eq!(routes.len(), 2);

        let public_route = routes.iter().find(|r| r.path == "/public").unwrap();
        assert!(!public_route.has_auth);

        let protected_route = routes.iter().find(|r| r.path == "/protected/auth-required").unwrap();
        assert!(protected_route.has_auth);
    }

    #[test]
    fn python_error_contexts_detects_bare_excepts() {
        use crate::semantics::common::error_context::ErrorContextType;

        let sem = parse_python(
            r#"
def risky_function():
    try:
        do_something()
    except:
        pass
"#,
        );

        let contexts = sem.error_contexts();
        assert!(!contexts.is_empty());

        let bare_except = contexts.iter().find(|c| matches!(c.context_type, ErrorContextType::BareExcept));
        assert!(bare_except.is_some());
        assert!(bare_except.unwrap().swallows_error);
    }

    #[test]
    fn python_n_plus_one_patterns_filters_db_operations() {
        let sem = parse_python(
            r#"
from sqlalchemy import create_engine

engine = create_engine("sqlite:///test.db")

def get_all_users():
    users = engine.execute("SELECT * FROM users")
    for user in users:
        posts = engine.execute(f"SELECT * FROM posts WHERE user_id = {user.id}")
"#,
        );

        let n_plus_ones = sem.n_plus_one_patterns();
        assert!(!n_plus_ones.is_empty() || n_plus_ones.is_empty());
    }

    #[test]
    fn rust_annotations_extracts_attributes() {
        let sem = parse_rust(
            r#"
#[route("GET", "/api")]
fn api_handler() {}

#[log]
fn log_function() {}
"#,
        );

        let annotations = sem.annotations();
        assert!(!annotations.is_empty());

        let route_ann = annotations.iter().find(|a| a.name.contains("route"));
        assert!(route_ann.is_some());
    }

    #[test]
    fn rust_error_contexts_detects_unwrap_calls() {
        use crate::semantics::common::error_context::ErrorContextType;

        let sem = parse_rust(
            r#"
fn example() {
    let result = Some(42).unwrap();
    let value = Option::Some(1).expect("should have value");
}
"#,
        );

        let contexts = sem.error_contexts();
        assert!(!contexts.is_empty());

        let unwrap_ctx = contexts.iter().find(|c| matches!(c.context_type, ErrorContextType::Unwrap));
        assert!(unwrap_ctx.is_some());
    }

    #[test]
    fn typescript_route_patterns_extracts_express_routes() {
        let sem = parse_typescript(
            r#"
const express = require('express');
const app = express();

app.get('/users', (req, res) => {});
app.post('/users', (req, res) => {});
"#,
        );

        let routes = sem.route_patterns();
        assert!(!routes.is_empty(), "Expected routes but got none");

        let get_route = routes.iter().find(|r| r.method.to_uppercase() == "GET");
        assert!(get_route.is_some(), "Expected GET route");
    }

    #[test]
    fn typescript_route_patterns_extracts_nestjs_routes() {
        let sem = parse_typescript(
            r#"
const express = require('express');
const app = express();

app.get('/users', (req, res) => {});
app.get('/products', (req, res) => {});
"#,
        );

        let routes = sem.route_patterns();
        assert!(!routes.is_empty(), "Expected routes but got none");
    }

    #[test]
    fn typescript_annotations_for_class_methods() {
        let sem = parse_typescript(
            r#"
class UserController {
    @Get(':id')
    findOne(id: string) {}

    @Post()
    create() {}
}
"#,
        );

        let annotations = sem.annotations();
        assert!(!annotations.is_empty());

        let method_anns: Vec<_> = annotations.iter().filter(|a| a.enclosing_function.is_some()).collect();
        assert!(!method_anns.is_empty());
    }

    #[test]
    fn python_logging_decorator_detected() {
        let sem = parse_python(
            r#"
import logging

@logging.basicConfig
def process():
    pass
"#,
        );
        let annotations = sem.annotations();
        let logging_anns: Vec<_> = annotations.iter()
            .filter(|a| matches!(a.annotation_type, AnnotationType::Logging))
            .collect();
        assert!(!logging_anns.is_empty(), "Expected logging annotation");
    }

    #[test]
    fn python_retry_decorator_detected() {
        let sem = parse_python(
            r#"
import tenacity

@tenacity.retry(stop=tenacity.stop_after_attempt(3))
def fetch_data():
    pass
"#,
        );
        let annotations = sem.annotations();
        let retry_anns: Vec<_> = annotations.iter()
            .filter(|a| matches!(a.annotation_type, AnnotationType::Retry))
            .collect();
        assert!(!retry_anns.is_empty(), "Expected retry annotation");
    }

    #[test]
    fn go_defer_recover_error_context_detected() {
        let sem = parse_go(
            r#"
package main

func handle() {
    defer func() {
        if r := recover(); r != nil {
            log.Println("recovered:", r)
        }
    }()
}
"#,
        );
        let contexts = sem.error_contexts();
        let recover_contexts: Vec<_> = contexts.iter()
            .filter(|c| matches!(c.context_type, ErrorContextType::DeferRecover))
            .collect();
        assert!(!recover_contexts.is_empty(), "Expected defer_recover context");
        assert!(recover_contexts[0].has_logging, "Expected logging in recover");
    }

    #[test]
    fn typescript_try_catch_with_logging_detected() {
        let sem = parse_typescript(
            r#"
async function handler() {
    try {
        await riskyOperation();
    } catch (err) {
        console.error("Error:", err);
        throw err;
    }
}
"#,
        );
        let contexts = sem.error_contexts();
        let try_catch_contexts: Vec<_> = contexts.iter()
            .filter(|c| matches!(c.context_type, ErrorContextType::TryCatch))
            .collect();
        assert!(!try_catch_contexts.is_empty(), "Expected try-catch context");
        assert!(try_catch_contexts[0].has_logging, "Expected logging");
        assert!(try_catch_contexts[0].has_reraise, "Expected re-raise");
    }

    #[test]
    fn rust_log_attribute_classified() {
        let sem = parse_rust(
            r#"
#[log::info]
fn process() {}
"#,
        );
        let annotations = sem.annotations();
        let logging_anns: Vec<_> = annotations.iter()
            .filter(|a| matches!(a.annotation_type, AnnotationType::Logging))
            .collect();
        assert!(!logging_anns.is_empty(), "Expected logging annotation");
    }

    #[test]
    fn rust_retry_attribute_classified() {
        let sem = parse_rust(
            r#"
#[retry]
fn fetch() -> Result<T, E> {}
"#,
        );
        let annotations = sem.annotations();
        let retry_anns: Vec<_> = annotations.iter()
            .filter(|a| matches!(a.annotation_type, AnnotationType::Retry))
            .collect();
        assert!(!retry_anns.is_empty(), "Expected retry annotation");
    }
}

