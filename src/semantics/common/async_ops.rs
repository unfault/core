//! Common async/concurrent operation abstractions for cross-language analysis.
//!
//! This module provides language-agnostic types for async operations,
//! enabling shared rule logic for error handling, cancellation, timeouts, etc.

use serde::{Deserialize, Serialize};

use super::CommonLocation;

/// Async runtime/framework classification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AsyncRuntime {
    // Python
    Asyncio,
    Trio,
    AnyIO,
    Curio,

    // Go (goroutines are the only model)
    Goroutine,

    // Rust
    Tokio,
    AsyncStd,
    Smol,

    // TypeScript/JavaScript
    PromiseNative,
    BluesbirdPromise,

    // Java
    CompletableFuture,
    ProjectReactor,
    RxJava,
    VirtualThreads,

    // Generic
    Other(String),
}

impl AsyncRuntime {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Asyncio => "asyncio",
            Self::Trio => "trio",
            Self::AnyIO => "anyio",
            Self::Curio => "curio",
            Self::Goroutine => "goroutine",
            Self::Tokio => "tokio",
            Self::AsyncStd => "async-std",
            Self::Smol => "smol",
            Self::PromiseNative => "Promise",
            Self::BluesbirdPromise => "Bluebird",
            Self::CompletableFuture => "CompletableFuture",
            Self::ProjectReactor => "Reactor",
            Self::RxJava => "RxJava",
            Self::VirtualThreads => "Virtual Threads",
            Self::Other(s) => s,
        }
    }

    /// Get the typical timeout wrapper/function for this runtime
    pub fn timeout_function(&self) -> &'static str {
        match self {
            Self::Asyncio => "asyncio.timeout() or asyncio.wait_for()",
            Self::Trio => "trio.fail_after() or trio.move_on_after()",
            Self::AnyIO => "anyio.fail_after() or anyio.move_on_after()",
            Self::Goroutine => "context.WithTimeout()",
            Self::Tokio => "tokio::time::timeout()",
            Self::AsyncStd => "async_std::future::timeout()",
            Self::PromiseNative => "AbortController with signal",
            Self::CompletableFuture => ".orTimeout() or .completeOnTimeout()",
            Self::ProjectReactor => ".timeout()",
            _ => "timeout wrapper",
        }
    }
}

/// Type of async operation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AsyncOperationType {
    /// Task/future spawn (create_task, tokio::spawn, go func)
    TaskSpawn,
    /// Task/future await
    TaskAwait,
    /// Task gather/join (asyncio.gather, tokio::join!, Promise.all)
    TaskGather,
    /// Channel send
    ChannelSend,
    /// Channel receive
    ChannelReceive,
    /// Lock acquisition
    LockAcquire,
    /// Lock release
    LockRelease,
    /// Semaphore acquire
    SemaphoreAcquire,
    /// Sleep/delay
    Sleep,
    /// Timeout wrapper
    Timeout,
    /// Select/race (select!, Promise.race)
    SelectRace,
    /// Unknown operation
    Unknown,
}

impl AsyncOperationType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::TaskSpawn => "spawn",
            Self::TaskAwait => "await",
            Self::TaskGather => "gather",
            Self::ChannelSend => "send",
            Self::ChannelReceive => "receive",
            Self::LockAcquire => "lock",
            Self::LockRelease => "unlock",
            Self::SemaphoreAcquire => "semaphore",
            Self::Sleep => "sleep",
            Self::Timeout => "timeout",
            Self::SelectRace => "select",
            Self::Unknown => "unknown",
        }
    }

    /// Check if this operation can potentially block/hang
    pub fn can_hang(&self) -> bool {
        matches!(
            self,
            Self::TaskAwait | Self::ChannelReceive | Self::LockAcquire | Self::SemaphoreAcquire
        )
    }

    /// Check if this operation creates concurrent work
    pub fn creates_concurrent_work(&self) -> bool {
        matches!(self, Self::TaskSpawn | Self::TaskGather)
    }
}

/// Error handling mechanism detected
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorHandling {
    /// Try/catch/except block
    TryCatch,
    /// Error callback
    ErrorCallback,
    /// Result type (Rust Result, Go error return)
    ResultType,
    /// Promise .catch() handler
    PromiseCatch,
    /// Task exception handler
    TaskExceptionHandler,
    /// Other mechanism
    Other(String),
}

/// Cancellation handling mechanism detected
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CancellationHandling {
    /// CancellationToken/Context
    CancellationToken,
    /// AbortController (JS)
    AbortController,
    /// Task cancel method
    TaskCancel,
    /// Channel close
    ChannelClose,
    /// Other mechanism
    Other(String),
}

/// A language-agnostic async operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsyncOperation {
    /// The async runtime being used
    pub runtime: AsyncRuntime,

    /// The type of operation
    pub operation_type: AsyncOperationType,

    /// Whether this operation has error handling
    pub has_error_handling: bool,

    /// Error handling mechanism detected
    pub error_handling: Option<ErrorHandling>,

    /// Whether this operation has a timeout
    pub has_timeout: bool,

    /// Timeout value in seconds (if determinable)
    pub timeout_value: Option<f64>,

    /// Whether this operation has cancellation support
    pub has_cancellation: bool,

    /// Cancellation mechanism detected
    pub cancellation_handling: Option<CancellationHandling>,

    /// Whether this operation is bounded (limited concurrency)
    pub is_bounded: bool,

    /// Bound/semaphore limit (if determinable)
    pub bound_limit: Option<u32>,

    /// Whether this operation is properly cleaned up on shutdown
    pub has_cleanup: bool,

    /// Full text of the operation
    pub operation_text: String,

    /// Location in source file
    pub location: CommonLocation,

    /// Name of enclosing function
    pub enclosing_function: Option<String>,

    /// Start byte offset
    pub start_byte: usize,

    /// End byte offset
    pub end_byte: usize,
}

impl AsyncOperation {
    /// Check if this spawned task needs error handling
    pub fn needs_error_handling(&self) -> bool {
        self.operation_type == AsyncOperationType::TaskSpawn && !self.has_error_handling
    }

    /// Check if this operation needs a timeout
    pub fn needs_timeout(&self) -> bool {
        self.operation_type.can_hang() && !self.has_timeout
    }

    /// Check if this operation could cause unbounded concurrency
    pub fn is_unbounded_concurrency(&self) -> bool {
        self.operation_type.creates_concurrent_work() && !self.is_bounded
    }

    /// Check if this operation needs proper cleanup
    pub fn needs_cleanup(&self) -> bool {
        self.operation_type == AsyncOperationType::TaskSpawn && !self.has_cleanup
    }

    /// Get suggested timeout based on runtime
    pub fn suggested_timeout(&self) -> f64 {
        30.0 // Default to 30 seconds
    }

    /// Get the error handling recommendation for this runtime
    pub fn error_handling_hint(&self) -> &'static str {
        match self.runtime {
            AsyncRuntime::Asyncio => "wrap with try/except or add task exception handler",
            AsyncRuntime::Tokio => "spawn with JoinHandle and await, or use spawn_blocking",
            AsyncRuntime::Goroutine => "use error channel or recover() in deferred function",
            AsyncRuntime::PromiseNative | AsyncRuntime::BluesbirdPromise => {
                "add .catch() handler or use async/await with try/catch"
            }
            AsyncRuntime::CompletableFuture => "use .exceptionally() or .handle()",
            _ => "add error handling",
        }
    }

    /// Get the timeout wrapper recommendation for this runtime
    pub fn timeout_hint(&self) -> &'static str {
        self.runtime.timeout_function()
    }
}

/// Concurrency pattern detected
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConcurrencyPattern {
    /// Number of concurrent operations spawned (if determinable)
    pub concurrency_count: Option<u32>,

    /// Whether concurrency is bounded by semaphore/pool
    pub is_bounded: bool,

    /// Bound limit value
    pub bound_limit: Option<u32>,

    /// Whether there's backpressure handling
    pub has_backpressure: bool,

    /// The pattern detected (e.g., "asyncio.gather", "go func in loop")
    pub pattern_name: String,

    /// Location in source
    pub location: CommonLocation,
}

impl ConcurrencyPattern {
    /// Check if this pattern could overwhelm resources
    pub fn is_potentially_dangerous(&self) -> bool {
        !self.is_bounded && self.concurrency_count.map(|c| c > 100).unwrap_or(true)
    }

    /// Get the bounded version recommendation
    pub fn bounded_recommendation(&self) -> &'static str {
        // Generic recommendation - can be specialized by runtime
        "Use a semaphore or worker pool to limit concurrency"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;

    fn make_location() -> CommonLocation {
        CommonLocation {
            file_id: FileId(1),
            line: 10,
            column: 5,
            start_byte: 100,
            end_byte: 150,
        }
    }

    #[test]
    fn async_operation_type_can_hang() {
        assert!(AsyncOperationType::TaskAwait.can_hang());
        assert!(AsyncOperationType::ChannelReceive.can_hang());
        assert!(AsyncOperationType::LockAcquire.can_hang());
        assert!(!AsyncOperationType::TaskSpawn.can_hang());
        assert!(!AsyncOperationType::Sleep.can_hang());
    }

    #[test]
    fn async_operation_type_creates_concurrent_work() {
        assert!(AsyncOperationType::TaskSpawn.creates_concurrent_work());
        assert!(AsyncOperationType::TaskGather.creates_concurrent_work());
        assert!(!AsyncOperationType::TaskAwait.creates_concurrent_work());
    }

    #[test]
    fn async_operation_needs_error_handling() {
        let op = AsyncOperation {
            runtime: AsyncRuntime::Asyncio,
            operation_type: AsyncOperationType::TaskSpawn,
            has_error_handling: false,
            error_handling: None,
            has_timeout: false,
            timeout_value: None,
            has_cancellation: false,
            cancellation_handling: None,
            is_bounded: false,
            bound_limit: None,
            has_cleanup: false,
            operation_text: "asyncio.create_task(coro())".into(),
            location: make_location(),
            enclosing_function: Some("main".into()),
            start_byte: 100,
            end_byte: 130,
        };

        assert!(op.needs_error_handling());
        assert!(op.is_unbounded_concurrency());
    }

    #[test]
    fn async_operation_with_error_handling() {
        let op = AsyncOperation {
            runtime: AsyncRuntime::Tokio,
            operation_type: AsyncOperationType::TaskSpawn,
            has_error_handling: true,
            error_handling: Some(ErrorHandling::TryCatch),
            has_timeout: false,
            timeout_value: None,
            has_cancellation: false,
            cancellation_handling: None,
            is_bounded: true,
            bound_limit: Some(10),
            has_cleanup: true,
            operation_text: "tokio::spawn(async { ... })".into(),
            location: make_location(),
            enclosing_function: Some("process".into()),
            start_byte: 100,
            end_byte: 130,
        };

        assert!(!op.needs_error_handling());
        assert!(!op.is_unbounded_concurrency());
    }

    #[test]
    fn concurrency_pattern_potentially_dangerous() {
        let unbounded = ConcurrencyPattern {
            concurrency_count: None,
            is_bounded: false,
            bound_limit: None,
            has_backpressure: false,
            pattern_name: "asyncio.gather(*tasks)".into(),
            location: make_location(),
        };
        assert!(unbounded.is_potentially_dangerous());

        let bounded = ConcurrencyPattern {
            concurrency_count: Some(50),
            is_bounded: true,
            bound_limit: Some(50),
            has_backpressure: true,
            pattern_name: "semaphore-limited gather".into(),
            location: make_location(),
        };
        assert!(!bounded.is_potentially_dangerous());
    }

    #[test]
    fn async_runtime_timeout_function() {
        assert!(
            AsyncRuntime::Asyncio
                .timeout_function()
                .contains("asyncio.timeout")
        );
        assert!(
            AsyncRuntime::Tokio
                .timeout_function()
                .contains("tokio::time::timeout")
        );
        assert!(
            AsyncRuntime::Goroutine
                .timeout_function()
                .contains("context.WithTimeout")
        );
    }
}
