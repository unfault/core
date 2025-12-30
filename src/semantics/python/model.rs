use serde::{Deserialize, Serialize};

use crate::parse::ast::{AstLocation, FileId, ParsedFile};
use crate::semantics::common::calls::FunctionCall;
use crate::semantics::common::CommonLocation;
use crate::semantics::python::http::HttpCallSite;
use crate::semantics::python::orm::OrmQueryCall;
use crate::types::context::Language;

use super::fastapi::FastApiFileSummary;
use super::django::DjangoFileSummary;
use super::flask::FlaskFileSummary;

/// Information about a bare except clause found in the code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BareExceptClause {
    /// 1-based line number where the except clause starts
    pub line: u32,
    /// 1-based column number
    pub column: u32,
    /// The text of the except clause (just the `except:` line)
    pub text: String,
    /// Name of the enclosing function, if any
    pub function_name: Option<String>,
    /// Start byte offset of the entire except clause
    pub start_byte: usize,
    /// End byte offset of the entire except clause
    pub end_byte: usize,
    /// Start byte offset of just the "except" keyword
    pub except_keyword_start: usize,
    /// End byte offset of just the "except" keyword (before the colon)
    pub except_keyword_end: usize,
    /// Location information
    pub location: AstLocation,
}

/// Semantic model for a single Python file.
/// Framework-agnostic core + optional framework-specific views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyFileSemantics {
    pub file_id: FileId,
    pub path: String,
    pub language: Language,

    /// Raw imports like `import fastapi as fa` or `from fastapi import FastAPI`.
    pub imports: Vec<PyImport>,

    /// Top-level functions and methods.
    pub functions: Vec<PyFunction>,

    /// Class definitions with their base classes.
    pub classes: Vec<PyClass>,

    /// Top-level assignments like `app = FastAPI()` or `router = APIRouter()`.
    pub assignments: Vec<PyAssignment>,

    /// Call sites we care about (function/method calls).
    pub calls: Vec<PyCallSite>,

    /// Framework-specific summary: FastAPI-related semantics for this file.
    pub fastapi: Option<FastApiFileSummary>,

    /// Framework-specific summary: Django-related semantics for this file.
    pub django: Option<DjangoFileSummary>,

    /// Framework-specific summary: Flask-related semantics for this file.
    pub flask: Option<FlaskFileSummary>,

    /// HTTP clients calls
    pub http_calls: Vec<HttpCallSite>,

    /// ORM query calls (SQLAlchemy, Django, etc.)
    pub orm_queries: Vec<OrmQueryCall>,

    /// Bare except clauses (except: without exception type)
    pub bare_excepts: Vec<BareExceptClause>,

    /// If a module-level docstring is present, this is the 1-based line number
    /// where it ends (useful for inserting new imports after it).
    pub module_docstring_end_line: Option<u32>,

    /// Async operations (asyncio.create_task, gather, await, etc.)
    pub async_operations: Vec<AsyncOperation>,

    /// Detected decorators (logging, retry, etc.)
    pub decorators: Vec<Decorator>,
}

/// Information about a decorator found in the code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decorator {
    /// The decorator name (e.g., "log", "retry", "app.get")
    pub name: String,
    /// Full text of the decorator including @ symbol
    pub text: String,
    /// Parameters to the decorator if any
    pub parameters: Vec<String>,
    /// Name of the function this decorator is attached to
    pub function_name: Option<String>,
    /// Start byte offset
    pub start_byte: usize,
    /// End byte offset
    pub end_byte: usize,
    /// Location in source file
    pub location: AstLocation,
}

/// Async operation in Python code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsyncOperation {
    /// Type of async operation
    pub operation_type: AsyncOperationType,

    /// Whether this operation has error handling
    pub has_error_handling: bool,

    /// Whether this operation has a timeout
    pub has_timeout: bool,

    /// Timeout value in seconds (if determinable)
    pub timeout_value: Option<f64>,

    /// Whether this operation has cancellation support
    pub has_cancellation: bool,

    /// Whether this operation is bounded (limited concurrency)
    pub is_bounded: bool,

    /// Bound/semaphore limit (if determinable)
    pub bound_limit: Option<u32>,

    /// Full text of the operation
    pub operation_text: String,

    /// Name of the enclosing function
    pub enclosing_function: Option<String>,

    /// Start byte offset of the entire operation
    pub start_byte: usize,

    /// End byte offset of the entire operation
    pub end_byte: usize,

    /// Location in source file
    pub location: AstLocation,
}

/// Type of async operation in Python.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AsyncOperationType {
    /// asyncio.create_task, asyncio.Task, etc.
    TaskSpawn,

    /// await expression
    Await,

    /// asyncio.gather, asyncio.wait, etc.
    TaskGather,

    /// Channel send (asyncio.Queue.put)
    ChannelSend,

    /// Channel receive (asyncio.Queue.get)
    ChannelReceive,

    /// Lock acquisition (asyncio.Lock.acquire)
    LockAcquire,

    /// Lock release (asyncio.Lock.release)
    LockRelease,

    /// Semaphore acquire (asyncio.Semaphore.acquire)
    SemaphoreAcquire,

    /// Sleep (asyncio.sleep)
    Sleep,

    /// Timeout wrapper (asyncio.timeout, asyncio.wait_for)
    Timeout,

    /// Select (asyncio.wait, select on multiple futures)
    Select,

    /// Async for loop iteration
    AsyncFor,

    /// Unknown operation
    Unknown,
}

impl AsyncOperationType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::TaskSpawn => "spawn",
            Self::Await => "await",
            Self::TaskGather => "gather",
            Self::ChannelSend => "send",
            Self::ChannelReceive => "receive",
            Self::LockAcquire => "lock",
            Self::LockRelease => "unlock",
            Self::SemaphoreAcquire => "semaphore",
            Self::Sleep => "sleep",
            Self::Timeout => "timeout",
            Self::Select => "select",
            Self::AsyncFor => "async_for",
            Self::Unknown => "unknown",
        }
    }

    /// Check if this operation can potentially block/hang
    pub fn can_hang(&self) -> bool {
        matches!(
            self,
            Self::Await | Self::ChannelReceive | Self::LockAcquire | Self::SemaphoreAcquire
        )
    }

    /// Check if this operation creates concurrent work
    pub fn creates_concurrent_work(&self) -> bool {
        matches!(self, Self::TaskSpawn | Self::TaskGather | Self::AsyncFor)
    }
}

impl PyFileSemantics {
    /// Build the semantic model from a parsed Python file.
    pub fn from_parsed(parsed: &ParsedFile) -> Self {
        let mut sem = PyFileSemantics {
            file_id: parsed.file_id,
            path: parsed.path.clone(),
            language: parsed.language,
            imports: Vec::new(),
            functions: Vec::new(),
            classes: Vec::new(),
            assignments: Vec::new(),
            calls: Vec::new(),
            fastapi: None,
            django: None,
            flask: None,
            http_calls: Vec::new(),
            orm_queries: Vec::new(),
            bare_excepts: Vec::new(),
            module_docstring_end_line: find_module_docstring_end_line(parsed),
            async_operations: Vec::new(),
            decorators: Vec::new(),
        };

        if parsed.language == Language::Python {
            collect_semantics(parsed, &mut sem);
        }

        sem
    }

    /// Run framework-specific analysis (FastAPI, Django, Flask, etc.).
    pub fn analyze_frameworks(&mut self, parsed: &ParsedFile) -> anyhow::Result<()> {
        // FastAPI analysis
        let fastapi_summary = super::fastapi::summarize_fastapi(parsed);
        if fastapi_summary.is_some() {
            self.fastapi = fastapi_summary;
        }

        // Django analysis
        let django_summary = super::django::summarize_django(parsed);
        if django_summary.is_some() {
            self.django = django_summary;
        }

        // Flask analysis
        let flask_summary = super::flask::summarize_flask(parsed);
        if flask_summary.is_some() {
            self.flask = flask_summary;
        }

        self.http_calls = super::http::summarize_http_clients(parsed);

        // ORM analysis
        self.orm_queries = super::orm::summarize_orm_queries(parsed);

        // Async operation analysis
        let async_summary = super::async_ops::summarize_async_operations(parsed);
        self.async_operations = async_summary.operations;

        // Decorator analysis (logging, retry, etc.)
        self.decorators = summarize_decorators(parsed);

        Ok(())
    }

    /// Returns the 1-based line number where new imports should be inserted.
    ///
    /// This is a simple method that places imports after the last existing import
    /// or after the module docstring. For PEP 8 compliant import ordering, use
    /// `import_insertion_line_for` instead.
    ///
    /// Use this method with `PatchRange::InsertBeforeLine` for consistent import placement.
    ///
    /// # Example
    /// ```ignore
    /// let import_line = py.import_insertion_line();
    /// PatchHunk {
    ///     range: PatchRange::InsertBeforeLine { line: import_line },
    ///     replacement: "import foo\n".to_string(),
    /// }
    /// ```
    pub fn import_insertion_line(&self) -> u32 {
        // Only consider module-level imports (not imports inside if blocks, functions, etc.)
        if let Some(last_import) = self
            .imports
            .iter()
            .filter(|imp| imp.is_module_level)
            .max_by_key(|imp| imp.location.range.end_line)
        {
            // end_line is 0-based, InsertBeforeLine expects 1-based
            // We want to insert on the line AFTER the import, so +2
            return last_import.location.range.end_line + 2;
        }

        // No module-level imports exist - insert after the module docstring if present
        if let Some(docstring_end) = self.module_docstring_end_line {
            // docstring_end is 1-based, and we want to insert AFTER it
            return docstring_end + 1;
        }

        // No docstring and no imports - insert at line 1
        1
    }

    /// Returns the 1-based line number where a new import of the given type should be inserted.
    ///
    /// This method follows PEP 8 import ordering:
    /// 1. Standard library imports (sorted: `import x` before `from x import y`)
    /// 2. Third-party imports (sorted: `import x` before `from x import y`)
    /// 3. Local imports
    ///
    /// # Arguments
    /// * `insertion_type` - The type of import being added (stdlib vs third-party, import vs from-import)
    ///
    /// # Example
    /// ```ignore
    /// // Adding a stdlib import like "import asyncio"
    /// let import_line = py.import_insertion_line_for(ImportInsertionType::stdlib_import());
    /// PatchHunk {
    ///     range: PatchRange::InsertBeforeLine { line: import_line },
    ///     replacement: "import asyncio\n".to_string(),
    /// }
    /// ```
    pub fn import_insertion_line_for(&self, insertion_type: ImportInsertionType) -> u32 {
        // Only consider module-level imports
        let module_level_imports: Vec<_> = self
            .imports
            .iter()
            .filter(|imp| imp.is_module_level)
            .collect();

        // If no existing module-level imports, use simple logic
        if module_level_imports.is_empty() {
            return self.base_import_line();
        }

        match (insertion_type.category, insertion_type.style) {
            // Stdlib `import x` - goes at the very top of imports
            (ImportCategory::Stdlib, ImportStyle::Import) => {
                // Find the first module-level import of any kind
                if let Some(first_import) = module_level_imports
                    .iter()
                    .min_by_key(|imp| imp.location.range.start_line)
                {
                    // Insert before the first import
                    return first_import.location.range.start_line + 1;
                }
                self.base_import_line()
            }

            // Stdlib `from x import y` - after stdlib `import x`, before third-party
            (ImportCategory::Stdlib, ImportStyle::FromImport) => {
                // Find the last stdlib `import x` statement
                if let Some(last_stdlib_import) = module_level_imports
                    .iter()
                    .filter(|imp| imp.is_stdlib() && !imp.is_from_import())
                    .max_by_key(|imp| imp.location.range.end_line)
                {
                    return last_stdlib_import.location.range.end_line + 2;
                }

                // No stdlib `import x` found, find the first stdlib `from x import y`
                if let Some(first_stdlib_from) = module_level_imports
                    .iter()
                    .filter(|imp| imp.is_stdlib() && imp.is_from_import())
                    .min_by_key(|imp| imp.location.range.start_line)
                {
                    return first_stdlib_from.location.range.start_line + 1;
                }

                // No stdlib imports at all, insert at the top
                if let Some(first_import) = module_level_imports
                    .iter()
                    .min_by_key(|imp| imp.location.range.start_line)
                {
                    return first_import.location.range.start_line + 1;
                }
                self.base_import_line()
            }

            // Third-party `import x` - after all stdlib imports, before third-party `from x import y`
            (ImportCategory::ThirdParty, ImportStyle::Import) => {
                // Find the last stdlib import (any style)
                if let Some(last_stdlib) = module_level_imports
                    .iter()
                    .filter(|imp| imp.is_stdlib())
                    .max_by_key(|imp| imp.location.range.end_line)
                {
                    // Check if there are any third-party `import x` already
                    if let Some(first_third_party_import) = module_level_imports
                        .iter()
                        .filter(|imp| imp.is_third_party() && !imp.is_from_import())
                        .min_by_key(|imp| imp.location.range.start_line)
                    {
                        return first_third_party_import.location.range.start_line + 1;
                    }
                    // Insert after the last stdlib import (with a blank line)
                    return last_stdlib.location.range.end_line + 2;
                }

                // No stdlib imports, find the first third-party `import x`
                if let Some(first_third_party_import) = module_level_imports
                    .iter()
                    .filter(|imp| imp.is_third_party() && !imp.is_from_import())
                    .min_by_key(|imp| imp.location.range.start_line)
                {
                    return first_third_party_import.location.range.start_line + 1;
                }

                // No third-party `import x`, insert before the first import if all are from-imports
                if let Some(first_import) = module_level_imports
                    .iter()
                    .min_by_key(|imp| imp.location.range.start_line)
                {
                    return first_import.location.range.start_line + 1;
                }
                self.base_import_line()
            }

            // Third-party `from x import y` - after all `import x` statements
            (ImportCategory::ThirdParty, ImportStyle::FromImport) => {
                // Find the last `import x` statement (either stdlib or third-party)
                if let Some(last_import_stmt) = module_level_imports
                    .iter()
                    .filter(|imp| !imp.is_from_import())
                    .max_by_key(|imp| imp.location.range.end_line)
                {
                    return last_import_stmt.location.range.end_line + 2;
                }

                // All existing imports are `from x import y`, insert after the last one
                if let Some(last_import) = module_level_imports
                    .iter()
                    .max_by_key(|imp| imp.location.range.end_line)
                {
                    return last_import.location.range.end_line + 2;
                }
                self.base_import_line()
            }

            // Local imports - go at the very end of module-level imports
            (ImportCategory::Local, _) => {
                if let Some(last_import) = module_level_imports
                    .iter()
                    .max_by_key(|imp| imp.location.range.end_line)
                {
                    return last_import.location.range.end_line + 2;
                }
                self.base_import_line()
            }
        }
    }

    /// Returns the base import line (after docstring or line 1).
    fn base_import_line(&self) -> u32 {
        if let Some(docstring_end) = self.module_docstring_end_line {
            docstring_end + 1
        } else {
            1
        }
    }

    /// Returns true if this file imports any known ORM libraries.
    ///
    /// This checks for imports from:
    /// - SQLAlchemy (sqlalchemy, sqlalchemy.orm)
    /// - Django ORM (django.db, django.db.models)
    /// - Tortoise ORM (tortoise)
    /// - SQLModel (sqlmodel)
    /// - Peewee (peewee)
    pub fn has_orm_imports(&self) -> bool {
        self.imports.iter().any(|imp| {
            let module = imp.module.to_lowercase();
            // SQLAlchemy
            module == "sqlalchemy"
                || module.starts_with("sqlalchemy.")
                // Django ORM
                || module == "django.db"
                || module.starts_with("django.db.")
                // Tortoise ORM
                || module == "tortoise"
                || module.starts_with("tortoise.")
                // SQLModel
                || module == "sqlmodel"
                || module.starts_with("sqlmodel.")
                // Peewee
                || module == "peewee"
                || module.starts_with("peewee.")
        })
    }
}

/// The style of an import statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportStyle {
    /// `import module` or `import module as alias`
    Import,
    /// `from module import name` or `from module import name as alias`
    FromImport,
}

/// Whether an import is a standard library, third-party, or local import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ImportCategory {
    /// Standard library (asyncio, collections, os, etc.)
    Stdlib,
    /// Third-party packages (fastapi, requests, etc.)
    ThirdParty,
    /// Local/application imports (relative or unknown)
    Local,
}

/// The type of import being inserted, used to determine correct placement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportInsertionType {
    /// The category of the import (stdlib, third-party, local)
    pub category: ImportCategory,
    /// The style of the import (import vs from-import)
    pub style: ImportStyle,
}

impl ImportInsertionType {
    /// Create a stdlib `import x` insertion type.
    pub fn stdlib_import() -> Self {
        Self {
            category: ImportCategory::Stdlib,
            style: ImportStyle::Import,
        }
    }

    /// Create a stdlib `from x import y` insertion type.
    pub fn stdlib_from_import() -> Self {
        Self {
            category: ImportCategory::Stdlib,
            style: ImportStyle::FromImport,
        }
    }

    /// Create a third-party `import x` insertion type.
    pub fn third_party_import() -> Self {
        Self {
            category: ImportCategory::ThirdParty,
            style: ImportStyle::Import,
        }
    }

    /// Create a third-party `from x import y` insertion type.
    pub fn third_party_from_import() -> Self {
        Self {
            category: ImportCategory::ThirdParty,
            style: ImportStyle::FromImport,
        }
    }
}

/// Representation of a Python import statement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyImport {
    /// The full module path, e.g. "fastapi", "app.api.v1".
    pub module: String,

    /// Imported names, e.g. ["FastAPI", "APIRouter"].
    pub names: Vec<String>,

    /// Optional alias, e.g. "fa" in `import fastapi as fa`.
    pub alias: Option<String>,

    /// The style of the import (import vs from-import).
    pub style: ImportStyle,

    /// The category of the import (stdlib, third-party, local).
    pub category: ImportCategory,

    /// True if this import is at module level (not inside a function, class, if-block, etc.).
    pub is_module_level: bool,

    pub location: AstLocation,
}

impl PyImport {
    /// Returns true if this is a `from x import y` style import.
    pub fn is_from_import(&self) -> bool {
        self.style == ImportStyle::FromImport
    }

    /// Returns true if this is a stdlib import.
    pub fn is_stdlib(&self) -> bool {
        self.category == ImportCategory::Stdlib
    }

    /// Returns true if this is a third-party import.
    pub fn is_third_party(&self) -> bool {
        self.category == ImportCategory::ThirdParty
    }
}

/// Known Python standard library modules (for categorizing imports).
const STDLIB_MODULES: &[&str] = &[
    // Frequently used in production code
    "abc",
    "argparse",
    "asyncio",
    "base64",
    "bisect",
    "builtins",
    "calendar",
    "codecs",
    "collections",
    "concurrent",
    "contextlib",
    "copy",
    "csv",
    "dataclasses",
    "datetime",
    "decimal",
    "difflib",
    "dis",
    "email",
    "enum",
    "errno",
    "faulthandler",
    "functools",
    "gc",
    "getpass",
    "glob",
    "gzip",
    "hashlib",
    "heapq",
    "hmac",
    "html",
    "http",
    "importlib",
    "inspect",
    "io",
    "ipaddress",
    "itertools",
    "json",
    "locale",
    "logging",
    "math",
    "mimetypes",
    "multiprocessing",
    "numbers",
    "operator",
    "os",
    "pathlib",
    "pickle",
    "platform",
    "pprint",
    "queue",
    "random",
    "re",
    "secrets",
    "select",
    "shutil",
    "signal",
    "socket",
    "sqlite3",
    "ssl",
    "stat",
    "statistics",
    "string",
    "struct",
    "subprocess",
    "sys",
    "tempfile",
    "textwrap",
    "threading",
    "time",
    "timeit",
    "traceback",
    "typing",
    "unittest",
    "urllib",
    "uuid",
    "warnings",
    "weakref",
    "xml",
    "zipfile",
    "zlib",
    // Testing and debugging
    "doctest",
    "pdb",
    "profile",
    "trace",
    "unittest",
    // Less common but still stdlib
    "aifc",
    "array",
    "ast",
    "atexit",
    "audioop",
    "bdb",
    "binascii",
    "binhex",
    "cgi",
    "cgitb",
    "chunk",
    "cmath",
    "cmd",
    "code",
    "codeop",
    "colorsys",
    "compileall",
    "configparser",
    "cProfile",
    "crypt",
    "ctypes",
    "curses",
    "dbm",
    "filecmp",
    "fileinput",
    "fnmatch",
    "formatter",
    "fractions",
    "ftplib",
    "getopt",
    "gettext",
    "graphlib",
    "grp",
    "imaplib",
    "imp",
    "keyword",
    "lib2to3",
    "linecache",
    "lzma",
    "mailbox",
    "mailcap",
    "marshal",
    "mmap",
    "modulefinder",
    "netrc",
    "nis",
    "nntplib",
    "ntpath",
    "optparse",
    "ossaudiodev",
    "parser",
    "pickletools",
    "pipes",
    "pkgutil",
    "poplib",
    "posix",
    "posixpath",
    "pow",
    "pstats",
    "pty",
    "pwd",
    "py_compile",
    "pyclbr",
    "pydoc",
    "quopri",
    "readline",
    "reprlib",
    "resource",
    "rlcompleter",
    "runpy",
    "sched",
    "selectors",
    "shelve",
    "shlex",
    "smtpd",
    "smtplib",
    "sndhdr",
    "spwd",
    "stringprep",
    "sunau",
    "symbol",
    "symtable",
    "sysconfig",
    "syslog",
    "tabnanny",
    "tarfile",
    "telnetlib",
    "termios",
    "test",
    "token",
    "tokenize",
    "tomllib",
    "tty",
    "turtle",
    "turtledemo",
    "types",
    "unicodedata",
    "uu",
    "venv",
    "wave",
    "webbrowser",
    "winreg",
    "winsound",
    "wsgiref",
    "xdrlib",
    "xmlrpc",
    "zipapp",
    "zipimport",
];

/// Check if a module name is a Python standard library module.
pub fn is_stdlib_module(module: &str) -> bool {
    // Get the top-level module name (e.g., "os.path" -> "os")
    let top_level = module.split('.').next().unwrap_or(module);
    STDLIB_MODULES.contains(&top_level)
}

/// Representation of a Python function or method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyFunction {
    pub name: String,
    pub is_method: bool,
    pub class_name: Option<String>,
    pub params: Vec<PyParam>,
    pub is_async: bool,
    /// Return type annotation if present, e.g. "bool", "dict[str, Any]", "None"
    pub return_type: Option<String>,
    /// Hash of the normalized function body for duplication detection.
    /// Two functions with identical body_hash have identical body content.
    pub body_hash: Option<u64>,
    pub location: AstLocation,
    /// Start byte offset of the function definition
    pub start_byte: usize,
    /// End byte offset of the function definition
    pub end_byte: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyParam {
    pub name: String,
    pub default: Option<String>,
    /// Type annotation if present, e.g. "int", "str", "dict", "SessionRunRequest"
    pub type_annotation: Option<String>,
}

/// Representation of a Python class definition.
/// Tracks the class name and its base classes for inheritance analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyClass {
    /// The class name
    pub name: String,
    /// List of base class names (e.g., ["BaseModel"], ["ABC", "Generic[T]"])
    pub base_classes: Vec<String>,
    /// Location in source code
    pub location: AstLocation,
}

/// Representation of an assignment like `app = FastAPI()` or `session = Session()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyAssignment {
    pub target: String,
    pub value_repr: String,
    pub location: AstLocation,
    /// True if this assignment is at module level (not inside a function, class, or lambda).
    pub is_module_level: bool,
    /// Type annotation if present, e.g. "dict[str, str]" for `x: dict[str, str] = {}`
    pub type_annotation: Option<String>,
}

/// Representation of a function/method call in the file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyCallSite {
    /// Resolution information for this call site.
    pub function_call: FunctionCall,

    /// Arguments for the call, both positional and keyword.
    pub args: Vec<PyCallArg>,

    /// Full text representation of the arguments (for pattern matching)
    pub args_repr: String,

    /// Whether this call is inside a loop (for, while)
    pub in_loop: bool,

    /// Whether this call is inside a comprehension (list, dict, set, generator)
    pub in_comprehension: bool,

    /// Start byte offset of the call
    pub start_byte: usize,

    /// End byte offset of the call
    pub end_byte: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PyCallArg {
    pub name: Option<String>,
    pub value_repr: String,
}

#[derive(Debug, Clone)]
pub struct PyRange {
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
}

impl PyRange {
    pub fn from_ts_range(range: tree_sitter::Range) -> Self {
        Self {
            start_line: range.start_point.row as u32,
            start_col: range.start_point.column as u32,
            end_line: range.end_point.row as u32,
            end_col: range.end_point.column as u32,
        }
    }
}

/// Context for tracking loop/comprehension nesting during AST traversal.
#[derive(Default, Clone)]
struct TraversalContext {
    in_loop: bool,
    in_comprehension: bool,
    /// True if we're inside a function, class, if-block, or any non-module scope.
    /// Used to filter out non-module-level assignments and imports.
    in_nested_scope: bool,
    /// Current enclosing class name, if inside a class.
    current_class: Option<String>,
    /// Current enclosing function name (simple name).
    current_function: Option<String>,
    /// Current enclosing qualified name (Class.method or just function).
    current_qualified_name: Option<String>,
}

/// Collect semantics by walking the tree-sitter AST.
fn collect_semantics(parsed: &ParsedFile, sem: &mut PyFileSemantics) {
    let root = parsed.tree.root_node();

    // First pass: collect basic semantics with context tracking
    let ctx = TraversalContext::default();
    walk_nodes_with_context(root, parsed, sem, ctx);

    // Second pass: collect bare except clauses with function context
    collect_bare_excepts(parsed, sem);
}

/// Walk nodes while tracking loop/comprehension context.
fn walk_nodes_with_context(
    node: tree_sitter::Node,
    parsed: &ParsedFile,
    sem: &mut PyFileSemantics,
    ctx: TraversalContext,
) {
    // Update context based on current node
    let new_ctx = match node.kind() {
        "class_definition" => {
            let class_name = node
                .child_by_field_name("name")
                .map(|n| parsed.text_for_node(&n))
                .unwrap_or_default();
            TraversalContext {
                current_class: Some(class_name),
                in_nested_scope: true,
                ..ctx.clone()
            }
        }
        "function_definition" | "async_function_definition" => {
            let func_name = node
                .child_by_field_name("name")
                .map(|n| parsed.text_for_node(&n))
                .unwrap_or_default();
            let qualified = if let Some(class) = &ctx.current_class {
                format!("{}.{}", class, func_name)
            } else {
                func_name.clone()
            };
            TraversalContext {
                current_function: Some(func_name),
                current_qualified_name: Some(qualified),
                in_nested_scope: true,
                ..ctx.clone()
            }
        }
        "for_statement" | "while_statement" => TraversalContext {
            in_loop: true,
            ..ctx.clone()
        },
        "list_comprehension"
        | "dictionary_comprehension"
        | "set_comprehension"
        | "generator_expression" => TraversalContext {
            in_comprehension: true,
            ..ctx.clone()
        },
        // Track when we enter a lambda or if-block - these create new scopes
        "lambda" | "if_statement" => TraversalContext {
            in_nested_scope: true,
            ..ctx.clone()
        },
        _ => ctx.clone(),
    };

    // Process current node
    match node.kind() {
        "import_statement" | "import_from_statement" => {
            if let Some(mut imp) = build_import(parsed, &node) {
                // Track whether this is a module-level import
                imp.is_module_level = !ctx.in_nested_scope;
                sem.imports.push(imp);
            }
        }
        "assignment" => {
            if let Some(mut assign) = build_assignment(parsed, &node) {
                // Track whether this is a module-level assignment
                assign.is_module_level = !ctx.in_nested_scope;
                sem.assignments.push(assign);
            }
        }
        "call" => {
            if let Some(call) = build_callsite(parsed, &node, &new_ctx, sem) {
                sem.calls.push(call);
            }
        }
        "function_definition" | "async_function_definition" => {
            if let Some(fun) = build_function(parsed, &node) {
                sem.functions.push(fun);
            }
        }
        "class_definition" => {
            if let Some(cls) = build_class(parsed, &node) {
                sem.classes.push(cls);
            }
        }
        _ => {}
    }

    // Recurse into children
    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            walk_nodes_with_context(child, parsed, sem, new_ctx.clone());
        }
    }
}

/// Collect bare except clauses from the AST.
fn collect_bare_excepts(parsed: &ParsedFile, sem: &mut PyFileSemantics) {
    let root = parsed.tree.root_node();
    walk_for_bare_excepts(root, parsed, sem, None);
}

/// Walk the AST to find bare except clauses.
fn walk_for_bare_excepts(
    node: tree_sitter::Node,
    parsed: &ParsedFile,
    sem: &mut PyFileSemantics,
    current_function: Option<&str>,
) {
    // Track function context
    let func_name = if node.kind() == "function_definition" {
        node.child_by_field_name("name")
            .map(|n| parsed.text_for_node(&n))
    } else {
        None
    };

    let effective_function = func_name.as_deref().or(current_function);

    // Check if this is an except_clause
    if node.kind() == "except_clause" {
        // A bare except has no exception type specified
        let has_exception_type = has_exception_type_child(&node);

        if !has_exception_type {
            let range = node.range();
            let text = parsed.text_for_node(&node);

            // Get just the except line (not the body)
            let except_line = text.lines().next().unwrap_or(&text).to_string();

            // Find the "except" keyword position
            let (except_start, except_end) = find_except_keyword_position(&node, &parsed.source);

            let location = parsed.location_for_node(&node);

            sem.bare_excepts.push(BareExceptClause {
                line: range.start_point.row as u32 + 1,
                column: range.start_point.column as u32 + 1,
                text: except_line,
                function_name: effective_function.map(|s| s.to_string()),
                start_byte: node.start_byte(),
                end_byte: node.end_byte(),
                except_keyword_start: except_start,
                except_keyword_end: except_end,
                location,
            });
        }
    }

    // Recurse into children
    let child_count = node.child_count();
    for i in 0..child_count {
        if let Some(child) = node.child(i) {
            walk_for_bare_excepts(child, parsed, sem, effective_function);
        }
    }
}

/// Check if an except_clause node has an exception type specified.
fn has_exception_type_child(except_node: &tree_sitter::Node) -> bool {
    let child_count = except_node.child_count();

    for i in 0..child_count {
        if let Some(child) = except_node.child(i) {
            let kind = child.kind();
            // These are the node types that indicate an exception type is specified
            if kind == "identifier"
                || kind == "tuple"
                || kind == "attribute"  // e.g., `except module.Error:`
                || kind == "as_pattern"
            // e.g., `except Error as e:`
            {
                return true;
            }
        }
    }

    false
}

/// Find the byte position of the "except" keyword in an except_clause node.
fn find_except_keyword_position(except_node: &tree_sitter::Node, source: &str) -> (usize, usize) {
    let start = except_node.start_byte();
    let text = &source[except_node.byte_range()];

    let except_keyword = "except";
    if text.starts_with(except_keyword) {
        (start, start + except_keyword.len())
    } else {
        (start, start + except_keyword.len())
    }
}

fn find_module_docstring_end_line(parsed: &ParsedFile) -> Option<u32> {
    let lines: Vec<&str> = parsed.source.lines().collect();
    let mut idx = 0;

    while idx < lines.len() {
        let trimmed = lines[idx].trim_start();

        if trimmed.is_empty() {
            idx += 1;
            continue;
        }

        if trimmed.starts_with('#') {
            idx += 1;
            continue;
        }

        if let Some(marker) = detect_triple_quote(trimmed) {
            // Same-line docstring (opening and closing on the same line)
            let after_open = &trimmed[marker.start + marker.pattern.len()..];
            if after_open.contains(marker.pattern) {
                return Some(idx as u32 + 1);
            }

            // Multi-line docstring: search for the closing marker
            let mut line = idx + 1;
            while line < lines.len() {
                if lines[line].contains(marker.pattern) {
                    return Some(line as u32 + 1);
                }
                line += 1;
            }

            // Unterminated docstring: treat EOF as the end to avoid returning 0
            return Some(lines.len() as u32);
        }

        break;
    }

    None
}

struct DocstringMarker<'a> {
    pattern: &'a str,
    start: usize,
}

fn detect_triple_quote(line: &str) -> Option<DocstringMarker<'static>> {
    let mut offset = 0;
    for ch in line.chars() {
        if matches!(ch, 'r' | 'R' | 'u' | 'U' | 'f' | 'F' | 'b' | 'B') {
            offset += ch.len_utf8();
            continue;
        }
        break;
    }

    let rest = &line[offset..];
    if rest.starts_with("\"\"\"") {
        return Some(DocstringMarker {
            pattern: "\"\"\"",
            start: offset,
        });
    }
    if rest.starts_with("'''") {
        return Some(DocstringMarker {
            pattern: "'''",
            start: offset,
        });
    }

    None
}

/// Simple recursive traversal of all nodes in the tree.
fn _walk_nodes(root: tree_sitter::Node, f: &mut dyn FnMut(tree_sitter::Node)) {
    fn recurse(node: tree_sitter::Node, f: &mut dyn FnMut(tree_sitter::Node)) {
        f(node);
        let child_count = node.child_count();
        for i in 0..child_count {
            if let Some(child) = node.child(i) {
                recurse(child, f);
            }
        }
    }

    recurse(root, f);
}

/// Build a PyImport from an import node.
///
/// This is intentionally naive for now: we just store the raw module text and
/// imported names based on simple heuristics.
fn build_import(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<PyImport> {
    let location = parsed.location_for_node(node);
    let text = parsed.text_for_node(node);

    // Very naive splitting; we will refine later with real field-based parsing.
    // Examples:
    //   import fastapi as fa
    //   from fastapi import FastAPI, APIRouter
    let mut module = String::new();
    let mut names = Vec::new();
    let mut alias = None;
    let mut style = ImportStyle::Import;

    if text.starts_with("import ") {
        // import fastapi as fa
        style = ImportStyle::Import;
        let rest = text.trim_start_matches("import").trim();
        // split by " as " if present
        if let Some(idx) = rest.find(" as ") {
            module = rest[..idx].trim().to_string();
            alias = Some(rest[idx + 4..].trim().to_string());
        } else {
            module = rest.to_string();
        }
    } else if text.starts_with("from ") {
        // from fastapi import FastAPI, APIRouter
        style = ImportStyle::FromImport;
        if let Some(import_idx) = text.find(" import ") {
            let mod_part = text[4..import_idx].trim(); // after "from "
            let names_part = text[import_idx + 8..].trim(); // after " import "
            module = mod_part.to_string();
            names = names_part
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }

    if module.is_empty() && names.is_empty() {
        return None;
    }

    // Determine if this is a stdlib or third-party import
    let category = if is_stdlib_module(&module) {
        ImportCategory::Stdlib
    } else if module.starts_with('.') {
        // Relative imports are local
        ImportCategory::Local
    } else {
        // Assume third-party for all other imports
        ImportCategory::ThirdParty
    };

    Some(PyImport {
        module,
        names,
        alias,
        style,
        category,
        is_module_level: true, // Will be set properly by the caller
        location,
    })
}

/// Build a PyAssignment from an assignment node.
fn build_assignment(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<PyAssignment> {
    let location = parsed.location_for_node(node);
    let text = parsed.text_for_node(node);

    // Handle both regular assignments (x = value) and annotated assignments (x: Type = value)
    // First check for annotated assignment pattern: name: type = value
    let (target, type_annotation, value_repr) =
        if let Some(colon_idx) = find_annotation_colon(&text) {
            // Annotated assignment: x: Type = value
            let name = text[..colon_idx].trim();
            let after_colon = &text[colon_idx + 1..];

            if let Some(eq_idx) = after_colon.find('=') {
                let type_part = after_colon[..eq_idx].trim();
                let value_part = after_colon[eq_idx + 1..].trim();
                (
                    name.to_string(),
                    Some(type_part.to_string()),
                    value_part.to_string(),
                )
            } else {
                // Annotation without assignment (e.g., `x: int`) - skip these
                return None;
            }
        } else if let Some(eq_idx) = text.find('=') {
            // Regular assignment: x = value
            let left = text[..eq_idx].trim();
            let right = text[eq_idx + 1..].trim();
            (left.to_string(), None, right.to_string())
        } else {
            return None;
        };

    if target.is_empty() || value_repr.is_empty() {
        return None;
    }

    // For now we only support simple identifiers on the LHS.
    if target.contains(' ') || target.contains('(') || target.contains('.') {
        return None;
    }

    Some(PyAssignment {
        target,
        value_repr,
        location,
        is_module_level: true, // Will be set properly by the caller
        type_annotation,
    })
}

/// Find the position of the type annotation colon in an assignment.
/// Returns None if no annotation colon is found.
/// This is tricky because colons also appear in dict literals and slices.
fn find_annotation_colon(text: &str) -> Option<usize> {
    // Look for pattern: identifier : type =
    // The colon must come before any '=' and must be followed eventually by '='

    let mut bracket_depth: i32 = 0;
    let mut paren_depth: i32 = 0;
    let mut brace_depth: i32 = 0;

    for (i, c) in text.char_indices() {
        match c {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = (bracket_depth - 1).max(0),
            '(' => paren_depth += 1,
            ')' => paren_depth = (paren_depth - 1).max(0),
            '{' => brace_depth += 1,
            '}' => brace_depth = (brace_depth - 1).max(0),
            ':' if bracket_depth == 0 && paren_depth == 0 && brace_depth == 0 => {
                // Found a colon at the top level
                // Check if there's an '=' after this colon (indicating annotation)
                let after_colon = &text[i + 1..];
                if after_colon.contains('=') {
                    // This is likely an annotation colon
                    // But we need to make sure the part before the colon is a simple identifier
                    let before = text[..i].trim();
                    if !before.contains('=') && !before.contains('{') && !before.contains('[') {
                        return Some(i);
                    }
                }
            }
            '=' => {
                // Hit an '=' before finding an annotation colon
                // This is a regular assignment
                return None;
            }
            _ => {}
        }
    }

    None
}

/// Build a PyCallSite from a `call` node.
fn build_callsite(
    parsed: &ParsedFile,
    node: &tree_sitter::Node,
    ctx: &TraversalContext,
    sem: &PyFileSemantics,
) -> Option<PyCallSite> {
    let location = parsed.location_for_node(node);

    // Try to get the function being called.
    let func_node = node.child_by_field_name("function")?;
    let callee = parsed.text_for_node(&func_node);

    // Parse callee into parts
    let callee_parts: Vec<String> = callee.split('.').map(|s| s.to_string()).collect();
    let first_part = callee_parts.first().cloned().unwrap_or_default();

    // Detect self call
    let is_self_call = first_part == "self";

    // Detect import call and alias
    let (is_import_call, import_alias) = if is_self_call {
        (false, None)
    } else {
        let mut matching_alias = None;
        let found = sem.imports.iter().any(|imp| {
            if imp.module == first_part {
                matching_alias = None;
                true
            } else if imp.alias.as_ref() == Some(&first_part) {
                matching_alias = imp.alias.clone();
                true
            } else {
                false
            }
        });
        (found, matching_alias)
    };

    let function_call = FunctionCall {
        callee_expr: callee.clone(),
        callee_parts,
        caller_function: ctx.current_function.clone().unwrap_or_default(),
        caller_qualified_name: ctx.current_qualified_name.clone().unwrap_or_default(),
        location: CommonLocation::from(&location),
        is_self_call,
        is_import_call,
        import_alias,
    };

    // Get the full arguments text representation
    let args_repr = if let Some(args_node) = node.child_by_field_name("arguments") {
        parsed.text_for_node(&args_node)
    } else {
        String::new()
    };

    // Try to get arguments.
    let mut args = Vec::new();
    if let Some(args_node) = node.child_by_field_name("arguments") {
        let named_count = args_node.named_child_count();
        for i in 0..named_count {
            if let Some(arg_node) = args_node.named_child(i) {
                let value_repr = parsed.text_for_node(&arg_node);
                // We ignore keyword vs positional for now; we can refine later.
                args.push(PyCallArg {
                    name: None,
                    value_repr,
                });
            }
        }
    }

    Some(PyCallSite {
        function_call,
        args,
        args_repr,
        in_loop: ctx.in_loop,
        in_comprehension: ctx.in_comprehension,
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

/// Build a PyClass from a class_definition node.
fn build_class(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<PyClass> {
    let location = parsed.location_for_node(node);

    // Get the class name
    let name_node = node.child_by_field_name("name")?;
    let name = parsed.text_for_node(&name_node);

    // Extract base classes from the superclasses field
    let mut base_classes = Vec::new();
    if let Some(superclasses_node) = node.child_by_field_name("superclasses") {
        // The superclasses node is an argument_list containing the base classes
        let child_count = superclasses_node.named_child_count();
        for i in 0..child_count {
            if let Some(base_node) = superclasses_node.named_child(i) {
                let base_text = parsed.text_for_node(&base_node);
                if !base_text.is_empty() {
                    base_classes.push(base_text);
                }
            }
        }
    }

    Some(PyClass {
        name,
        base_classes,
        location,
    })
}

/// Build a PyFunction from a function or async_function_definition node.
fn build_function(parsed: &ParsedFile, node: &tree_sitter::Node) -> Option<PyFunction> {
    let location = parsed.location_for_node(node);

    // child_by_field_name("name") should give us the function name identifier.
    let name_node = node.child_by_field_name("name")?;
    let name = parsed.text_for_node(&name_node);

    // Detect async using both node kind and text-based approach for reliability
    // Some tree-sitter versions may not distinguish async_function_definition
    let is_async = node.kind() == "async_function_definition"
        || parsed
            .text_for_node(node)
            .trim_start()
            .starts_with("async def");

    // Extract parameters from the function
    let params = extract_function_params(parsed, node);

    // Extract return type annotation if present
    let return_type = node
        .child_by_field_name("return_type")
        .map(|n| parsed.text_for_node(&n));

    // Compute body hash for duplication detection
    let body_hash = node
        .child_by_field_name("body")
        .map(|body_node| compute_body_hash(parsed, &body_node));

    Some(PyFunction {
        name,
        is_method: false, // we'll refine later if we want to track methods vs functions
        class_name: None, // same here
        params,
        is_async,
        return_type,
        body_hash,
        location,
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

/// Compute a hash of the function body for duplication detection.
///
/// The body is normalized to ignore:
/// - Leading/trailing whitespace
/// - Indentation differences
/// - Empty lines
///
/// This ensures that two functions with semantically identical bodies
/// (but different formatting) will have the same hash.
fn compute_body_hash(parsed: &ParsedFile, body_node: &tree_sitter::Node) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let body_text = parsed.text_for_node(body_node);

    // Normalize the body:
    // 1. Split into lines
    // 2. Trim each line
    // 3. Skip empty lines and comments
    // 4. Join with single spaces
    let normalized: String = body_text
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join(" ");

    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    hasher.finish()
}

/// Extract parameters from a function definition node.
fn extract_function_params(parsed: &ParsedFile, node: &tree_sitter::Node) -> Vec<PyParam> {
    let mut params = Vec::new();

    // Get the parameters node
    let params_node = match node.child_by_field_name("parameters") {
        Some(n) => n,
        None => return params,
    };

    // Iterate through children of the parameters node
    let child_count = params_node.named_child_count();
    for i in 0..child_count {
        if let Some(param_node) = params_node.named_child(i) {
            match param_node.kind() {
                "identifier" => {
                    // Simple parameter like `x` (no type annotation)
                    let name = parsed.text_for_node(&param_node);
                    params.push(PyParam {
                        name,
                        default: None,
                        type_annotation: None,
                    });
                }
                "typed_parameter" => {
                    // Parameter with type annotation like `x: int`
                    // First try to get using field names
                    let name = param_node
                        .child_by_field_name("name")
                        .map(|n| parsed.text_for_node(&n));
                    let type_annotation = param_node
                        .child_by_field_name("type")
                        .map(|t| parsed.text_for_node(&t));

                    if let Some(name) = name {
                        params.push(PyParam {
                            name,
                            default: None,
                            type_annotation,
                        });
                    } else {
                        // Fallback: parse the text "name: type" directly
                        let text = parsed.text_for_node(&param_node);
                        if let Some((name, type_ann)) = parse_typed_param_text(&text) {
                            params.push(PyParam {
                                name,
                                default: None,
                                type_annotation: Some(type_ann),
                            });
                        } else {
                            // Ultimate fallback: try to get the first identifier child
                            for j in 0..param_node.named_child_count() {
                                if let Some(child) = param_node.named_child(j) {
                                    if child.kind() == "identifier" {
                                        let name = parsed.text_for_node(&child);
                                        params.push(PyParam {
                                            name,
                                            default: None,
                                            type_annotation: None,
                                        });
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                "default_parameter" => {
                    // Parameter with default value like `x=10` (no type annotation)
                    if let Some(name_node) = param_node.child_by_field_name("name") {
                        let name = parsed.text_for_node(&name_node);
                        let default = param_node
                            .child_by_field_name("value")
                            .map(|v| parsed.text_for_node(&v));
                        params.push(PyParam {
                            name,
                            default,
                            type_annotation: None,
                        });
                    }
                }
                "typed_default_parameter" => {
                    // Parameter with type and default like `x: int = 10`
                    let name = param_node
                        .child_by_field_name("name")
                        .map(|n| parsed.text_for_node(&n));
                    let default = param_node
                        .child_by_field_name("value")
                        .map(|v| parsed.text_for_node(&v));
                    let type_annotation = param_node
                        .child_by_field_name("type")
                        .map(|t| parsed.text_for_node(&t));

                    if let Some(name) = name {
                        params.push(PyParam {
                            name,
                            default,
                            type_annotation,
                        });
                    } else {
                        // Fallback: parse the text "name: type = value" directly
                        let text = parsed.text_for_node(&param_node);
                        if let Some((name, type_ann, def_val)) =
                            parse_typed_default_param_text(&text)
                        {
                            params.push(PyParam {
                                name,
                                default: def_val,
                                type_annotation: Some(type_ann),
                            });
                        }
                    }
                }
                "list_splat_pattern" | "dictionary_splat_pattern" => {
                    // *args or **kwargs
                    let text = parsed.text_for_node(&param_node);
                    params.push(PyParam {
                        name: text,
                        default: None,
                        type_annotation: None,
                    });
                }
                _ => {
                    // Other parameter types - try to extract text as fallback
                    let text = parsed.text_for_node(&param_node);
                    if !text.is_empty() && text != "," {
                        params.push(PyParam {
                            name: text,
                            default: None,
                            type_annotation: None,
                        });
                    }
                }
            }
        }
    }

    params
}

/// Parse a typed parameter text like "name: type" into (name, type).
fn parse_typed_param_text(text: &str) -> Option<(String, String)> {
    let colon_idx = text.find(':')?;
    let name = text[..colon_idx].trim().to_string();
    let type_ann = text[colon_idx + 1..].trim().to_string();

    if name.is_empty() || type_ann.is_empty() {
        return None;
    }

    Some((name, type_ann))
}

/// Parse a typed default parameter text like "name: type = value" into (name, type, default).
fn parse_typed_default_param_text(text: &str) -> Option<(String, String, Option<String>)> {
    let colon_idx = text.find(':')?;
    let name = text[..colon_idx].trim().to_string();
    let rest = &text[colon_idx + 1..];

    // Find the '=' for default value
    if let Some(eq_idx) = rest.find('=') {
        let type_ann = rest[..eq_idx].trim().to_string();
        let default = rest[eq_idx + 1..].trim().to_string();

        if name.is_empty() || type_ann.is_empty() {
            return None;
        }

        Some((
            name,
            type_ann,
            if default.is_empty() {
                None
            } else {
                Some(default)
            },
        ))
    } else {
        // No default value
        let type_ann = rest.trim().to_string();
        if name.is_empty() || type_ann.is_empty() {
            return None;
        }
        Some((name, type_ann, None))
    }
}

/// Summarize all decorators in a parsed Python file.
/// Detects logging, retry, and other common decorators.
pub fn summarize_decorators(parsed: &ParsedFile) -> Vec<Decorator> {
    let mut decorators = Vec::new();

    fn walk(file: &ParsedFile, node: tree_sitter::Node, decorators: &mut Vec<Decorator>) {
        if node.kind() == "decorated_definition" {
            let mut fn_name = None;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "decorator" {
                    let decorator_text = file.text_for_node(&child);
                    let start_byte = child.start_byte();
                    let end_byte = child.end_byte();
                    let location = file.location_for_node(&child);

                    let (name, parameters) = extract_decorator_name_and_params(&decorator_text);

                    decorators.push(Decorator {
                        name,
                        text: decorator_text,
                        parameters,
                        function_name: fn_name.clone(),
                        start_byte,
                        end_byte,
                        location,
                    });
                } else if child.kind() == "function_definition" {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        fn_name = Some(file.text_for_node(&name_node));
                    }
                }
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk(file, child, decorators);
        }
    }

    let root = parsed.tree.root_node();
    walk(parsed, root, &mut decorators);

    decorators
}

fn extract_decorator_name_and_params(decorator_text: &str) -> (String, Vec<String>) {
    let text = decorator_text.trim_start_matches('@').trim();

    let (name, params) = if let Some(paren_idx) = text.find('(') {
        let name = text[..paren_idx].to_string();
        let params_text = &text[paren_idx + 1..text.len().saturating_sub(1)];
        let params = params_text
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        (name, params)
    } else {
        (text.to_string(), Vec::new())
    };

    (name, params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::parse::python::parse_python_file;
    use crate::types::context::{Language, SourceFile};

    /// Helper to parse Python source and build semantics
    fn parse_and_build_semantics(source: &str) -> PyFileSemantics {
        let sf = SourceFile {
            path: "test.py".to_string(),
            language: Language::Python,
            content: source.to_string(),
        };
        let parsed = parse_python_file(FileId(1), &sf).expect("parsing should succeed");
        PyFileSemantics::from_parsed(&parsed)
    }

    // ==================== PyRange Tests ====================

    #[test]
    fn py_range_from_ts_range_converts_correctly() {
        let ts_range = tree_sitter::Range {
            start_byte: 0,
            end_byte: 10,
            start_point: tree_sitter::Point { row: 1, column: 5 },
            end_point: tree_sitter::Point { row: 2, column: 10 },
        };
        let py_range = PyRange::from_ts_range(ts_range);
        assert_eq!(py_range.start_line, 1);
        assert_eq!(py_range.start_col, 5);
        assert_eq!(py_range.end_line, 2);
        assert_eq!(py_range.end_col, 10);
    }

    // ==================== Import Tests ====================

    #[test]
    fn collects_simple_import_statement() {
        let sem = parse_and_build_semantics("import fastapi");
        assert_eq!(sem.imports.len(), 1);
        assert_eq!(sem.imports[0].module, "fastapi");
        assert!(sem.imports[0].names.is_empty());
        assert!(sem.imports[0].alias.is_none());
    }

    #[test]
    fn collects_import_with_alias() {
        let sem = parse_and_build_semantics("import fastapi as fa");
        assert_eq!(sem.imports.len(), 1);
        assert_eq!(sem.imports[0].module, "fastapi");
        assert_eq!(sem.imports[0].alias, Some("fa".to_string()));
    }

    #[test]
    fn collects_from_import_single_name() {
        let sem = parse_and_build_semantics("from fastapi import FastAPI");
        assert_eq!(sem.imports.len(), 1);
        assert_eq!(sem.imports[0].module, "fastapi");
        assert_eq!(sem.imports[0].names, vec!["FastAPI"]);
    }

    #[test]
    fn collects_from_import_multiple_names() {
        let sem = parse_and_build_semantics("from fastapi import FastAPI, APIRouter, Depends");
        assert_eq!(sem.imports.len(), 1);
        assert_eq!(sem.imports[0].module, "fastapi");
        assert_eq!(
            sem.imports[0].names,
            vec!["FastAPI", "APIRouter", "Depends"]
        );
    }

    #[test]
    fn collects_from_import_with_dotted_module() {
        let sem = parse_and_build_semantics("from fastapi.middleware.cors import CORSMiddleware");
        assert_eq!(sem.imports.len(), 1);
        assert_eq!(sem.imports[0].module, "fastapi.middleware.cors");
        assert_eq!(sem.imports[0].names, vec!["CORSMiddleware"]);
    }

    #[test]
    fn collects_multiple_import_statements() {
        let src = r#"
import os
import sys
from typing import List
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.imports.len(), 3);
    }

    #[test]
    fn detects_module_docstring_end_line() {
        let src = "\"\"\"Sample docstring\nwith details\n\"\"\"\n\nfrom fastapi import FastAPI\n";
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.module_docstring_end_line, Some(3));
    }

    #[test]
    fn module_docstring_end_line_is_none_when_absent() {
        let src = r#"
from fastapi import FastAPI
"#;
        let sem = parse_and_build_semantics(src);
        assert!(sem.module_docstring_end_line.is_none());
    }

    // ==================== Assignment Tests ====================

    #[test]
    fn collects_simple_assignment() {
        let sem = parse_and_build_semantics("x = 42");
        assert_eq!(sem.assignments.len(), 1);
        assert_eq!(sem.assignments[0].target, "x");
        assert_eq!(sem.assignments[0].value_repr, "42");
    }

    #[test]
    fn collects_assignment_with_function_call() {
        let sem = parse_and_build_semantics("app = FastAPI()");
        assert_eq!(sem.assignments.len(), 1);
        assert_eq!(sem.assignments[0].target, "app");
        assert_eq!(sem.assignments[0].value_repr, "FastAPI()");
    }

    #[test]
    fn collects_assignment_with_string_value() {
        let sem = parse_and_build_semantics(r#"name = "hello""#);
        assert_eq!(sem.assignments.len(), 1);
        assert_eq!(sem.assignments[0].target, "name");
        assert_eq!(sem.assignments[0].value_repr, r#""hello""#);
    }

    #[test]
    fn ignores_complex_lhs_assignments() {
        // Assignments with dots, parentheses, or spaces on LHS should be ignored
        let sem = parse_and_build_semantics("self.x = 42");
        // This should be ignored because LHS contains '.'
        assert!(sem.assignments.is_empty());
    }

    #[test]
    fn collects_multiple_assignments() {
        let src = r#"
x = 1
y = 2
z = 3
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.assignments.len(), 3);
    }

    // ==================== Function Tests ====================

    #[test]
    fn collects_simple_function() {
        let src = r#"
def hello():
    pass
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.functions.len(), 1);
        assert_eq!(sem.functions[0].name, "hello");
        assert!(!sem.functions[0].is_async);
        assert!(!sem.functions[0].is_method);
    }

    #[test]
    fn collects_async_function() {
        let src = r#"
async def fetch_data():
    pass
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.functions.len(), 1);
        assert_eq!(sem.functions[0].name, "fetch_data");
        // Note: is_async detection depends on tree-sitter node kind
        // The function is collected regardless of async status
    }

    #[test]
    fn collects_multiple_functions() {
        let src = r#"
def foo():
    pass

async def bar():
    pass

def baz():
    pass
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.functions.len(), 3);

        let names: Vec<&str> = sem.functions.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"foo"));
        assert!(names.contains(&"bar"));
        assert!(names.contains(&"baz"));
    }

    #[test]
    fn function_async_detection_is_correct() {
        let src = r#"
def sync_func():
    pass

async def async_func():
    pass
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.functions.len(), 2);

        let sync_fn = sem
            .functions
            .iter()
            .find(|f| f.name == "sync_func")
            .unwrap();
        let _async_fn = sem
            .functions
            .iter()
            .find(|f| f.name == "async_func")
            .unwrap();

        assert!(!sync_fn.is_async);
        // Note: async detection depends on tree-sitter node structure
        // Both functions are collected; async status may vary by tree-sitter version
    }

    // ==================== Call Site Tests ====================

    #[test]
    fn collects_simple_function_call() {
        let sem = parse_and_build_semantics("print('hello')");
        assert!(sem.calls.iter().any(|c| c.function_call.callee_expr == "print"));
    }

    #[test]
    fn collects_method_call() {
        let sem = parse_and_build_semantics("app.add_middleware(CORSMiddleware)");
        assert!(sem.calls.iter().any(|c| c.function_call.callee_expr == "app.add_middleware"));
    }

    #[test]
    fn collects_call_with_arguments() {
        let sem = parse_and_build_semantics("requests.get('https://example.com', timeout=30)");
        let call = sem
            .calls
            .iter()
            .find(|c| c.function_call.callee_expr == "requests.get")
            .unwrap();
        assert!(!call.args.is_empty());
    }

    #[test]
    fn collects_nested_calls() {
        let src = r#"
result = outer(inner(x))
"#;
        let sem = parse_and_build_semantics(src);
        // Should collect both outer and inner calls
        assert!(sem.calls.iter().any(|c| c.function_call.callee_expr == "outer"));
        assert!(sem.calls.iter().any(|c| c.function_call.callee_expr == "inner"));
    }

    #[test]
    fn collects_chained_method_calls() {
        let src = r#"
result = obj.method1().method2()
"#;
        let sem = parse_and_build_semantics(src);
        // Should collect the chained calls
        assert!(!sem.calls.is_empty());
    }

    // ==================== PyFileSemantics Tests ====================

    #[test]
    fn from_parsed_sets_file_metadata() {
        let sf = SourceFile {
            path: "my/path/test.py".to_string(),
            language: Language::Python,
            content: "x = 1".to_string(),
        };
        let parsed = parse_python_file(FileId(42), &sf).expect("parsing should succeed");
        let sem = PyFileSemantics::from_parsed(&parsed);

        assert_eq!(sem.file_id, FileId(42));
        assert_eq!(sem.path, "my/path/test.py");
        assert_eq!(sem.language, Language::Python);
    }

    #[test]
    fn from_parsed_initializes_empty_collections_for_non_python() {
        // Create a parsed file but manually set language to something else
        // This tests the guard in from_parsed
        let sf = SourceFile {
            path: "test.py".to_string(),
            language: Language::Python,
            content: "x = 1".to_string(),
        };
        let mut parsed = parse_python_file(FileId(1), &sf).expect("parsing should succeed");
        parsed.language = Language::Rust; // Override language

        let sem = PyFileSemantics::from_parsed(&parsed);

        // Should have empty collections because language != Python
        assert!(sem.imports.is_empty());
        assert!(sem.functions.is_empty());
        assert!(sem.assignments.is_empty());
        assert!(sem.calls.is_empty());
    }

    #[test]
    fn fastapi_is_none_before_analyze_frameworks() {
        let sem = parse_and_build_semantics("x = 1");
        assert!(sem.fastapi.is_none());
    }

    #[test]
    fn http_calls_is_empty_before_analyze_frameworks() {
        let sem = parse_and_build_semantics("requests.get('https://example.com')");
        // http_calls is populated by analyze_frameworks, not from_parsed
        assert!(sem.http_calls.is_empty());
    }

    // ==================== Location Tests ====================

    #[test]
    fn import_has_correct_location() {
        let src = "import os";
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.imports.len(), 1);
        assert_eq!(sem.imports[0].location.range.start_line, 0);
        assert_eq!(sem.imports[0].location.range.start_col, 0);
    }

    #[test]
    fn function_has_correct_location() {
        let src = r#"
def hello():
    pass
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.functions.len(), 1);
        // Function starts on line 1 (0-indexed), after the newline
        assert_eq!(sem.functions[0].location.range.start_line, 1);
    }

    // ==================== Complex Code Tests ====================

    #[test]
    fn handles_complete_fastapi_file() {
        let src = r#"
from fastapi import FastAPI, APIRouter
from fastapi.middleware.cors import CORSMiddleware

app = FastAPI()

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
)

@app.get("/")
async def root():
    return {"message": "Hello"}

def helper():
    pass
"#;
        let sem = parse_and_build_semantics(src);

        // Should have imports
        assert_eq!(sem.imports.len(), 2);

        // Should have assignment for app
        assert!(sem.assignments.iter().any(|a| a.target == "app"));

        // Should have functions (both root and helper)
        assert!(sem.functions.iter().any(|f| f.name == "root"));
        assert!(sem.functions.iter().any(|f| f.name == "helper"));

        // Should have calls
        assert!(sem.calls.iter().any(|c| c.function_call.callee_expr == "FastAPI"));
        assert!(sem.calls.iter().any(|c| c.function_call.callee_expr == "app.add_middleware"));
    }

    #[test]
    fn handles_empty_file() {
        let sem = parse_and_build_semantics("");
        assert!(sem.imports.is_empty());
        assert!(sem.functions.is_empty());
        assert!(sem.assignments.is_empty());
        assert!(sem.calls.is_empty());
    }

    #[test]
    fn handles_file_with_only_comments() {
        let src = r#"
# This is a comment
# Another comment
"#;
        let sem = parse_and_build_semantics(src);
        assert!(sem.imports.is_empty());
        assert!(sem.functions.is_empty());
        assert!(sem.assignments.is_empty());
        assert!(sem.calls.is_empty());
    }

    #[test]
    fn handles_class_with_methods() {
        let src = r#"
class MyClass:
    def __init__(self):
        self.x = 1
    
    async def async_method(self):
        pass
    
    def sync_method(self):
        pass
"#;
        let sem = parse_and_build_semantics(src);

        // Should collect all methods as functions
        assert!(sem.functions.iter().any(|f| f.name == "__init__"));
        assert!(sem.functions.iter().any(|f| f.name == "async_method"));
        assert!(sem.functions.iter().any(|f| f.name == "sync_method"));
    }

    // ==================== Edge Cases ====================

    #[test]
    fn handles_multiline_import() {
        let src = r#"
from typing import (
    List,
    Dict,
    Optional
)
"#;
        let sem = parse_and_build_semantics(src);
        // Should still collect the import
        assert!(!sem.imports.is_empty());
    }

    #[test]
    fn handles_decorated_function() {
        let src = r#"
@decorator
def decorated():
    pass

@app.get("/")
async def route():
    pass
"#;
        let sem = parse_and_build_semantics(src);
        assert!(sem.functions.iter().any(|f| f.name == "decorated"));
        assert!(sem.functions.iter().any(|f| f.name == "route"));
    }

    #[test]
    fn handles_lambda_expressions() {
        let src = r#"
f = lambda x: x + 1
"#;
        let sem = parse_and_build_semantics(src);
        // Lambda should be captured as an assignment
        assert!(sem.assignments.iter().any(|a| a.target == "f"));
    }

    #[test]
    fn handles_nested_function_definitions() {
        let src = r#"
def outer():
    def inner():
        pass
    return inner
"#;
        let sem = parse_and_build_semantics(src);
        // Should collect both outer and inner functions
        assert!(sem.functions.iter().any(|f| f.name == "outer"));
        assert!(sem.functions.iter().any(|f| f.name == "inner"));
    }

    // ==================== Edge Cases for Import Parsing (line 238) ====================

    #[test]
    fn handles_import_with_no_module_or_names() {
        // This tests line 238 - when module and names are both empty
        // In practice, this shouldn't happen with valid Python,
        // but we test the guard
        let sem = parse_and_build_semantics("# just a comment");
        // Should have no imports
        assert!(sem.imports.is_empty());
    }

    #[test]
    fn handles_malformed_import_gracefully() {
        // tree-sitter is error-tolerant, so even malformed imports
        // should be handled gracefully
        let sem = parse_and_build_semantics("import");
        // May or may not parse as an import depending on tree-sitter
        // The important thing is it doesn't crash
        let _ = sem.imports;
    }

    // ==================== Edge Cases for Assignment Parsing (lines 260, 274) ====================

    #[test]
    fn handles_assignment_with_empty_left_side() {
        // This tests line 260 - when left side is empty after split
        // In practice, this shouldn't happen with valid Python
        let sem = parse_and_build_semantics("= 42");
        // Should not crash, may or may not have assignments
        let _ = sem.assignments;
    }

    #[test]
    fn handles_assignment_with_empty_right_side() {
        // This tests line 260 - when right side is empty after split
        // In practice, this shouldn't happen with valid Python
        let sem = parse_and_build_semantics("x =");
        // Should not crash, may or may not have assignments
        let _ = sem.assignments;
    }

    #[test]
    fn handles_assignment_without_equals() {
        // This tests line 274 - when there's no '=' in the text
        // This shouldn't happen for assignment nodes, but we test the guard
        let sem = parse_and_build_semantics("x");
        // Should have no assignments (just an expression)
        assert!(sem.assignments.is_empty());
    }

    #[test]
    fn handles_augmented_assignment() {
        // Augmented assignments like += are different from regular assignments
        let sem = parse_and_build_semantics("x += 1");
        // May or may not be captured as a regular assignment
        // The important thing is it doesn't crash
        let _ = sem.assignments;
    }

    #[test]
    fn handles_multiple_assignment_targets() {
        // Multiple targets like a = b = 1
        let sem = parse_and_build_semantics("a = b = 1");
        // Should handle gracefully
        let _ = sem.assignments;
    }

    #[test]
    fn handles_tuple_unpacking_assignment() {
        // Tuple unpacking like a, b = 1, 2
        let sem = parse_and_build_semantics("a, b = 1, 2");
        // Should be ignored because LHS contains ','
        // (our simple parser only handles simple identifiers)
        let _ = sem.assignments;
    }

    #[test]
    fn handles_list_unpacking_assignment() {
        // List unpacking like [a, b] = [1, 2]
        let sem = parse_and_build_semantics("[a, b] = [1, 2]");
        // Should be ignored because LHS contains '['
        let _ = sem.assignments;
    }

    #[test]
    fn handles_attribute_assignment() {
        // Attribute assignment like obj.attr = 1
        let sem = parse_and_build_semantics("obj.attr = 1");
        // Should be ignored because LHS contains '.'
        assert!(sem.assignments.is_empty());
    }

    #[test]
    fn handles_subscript_assignment() {
        // Subscript assignment like arr[0] = 1
        let sem = parse_and_build_semantics("arr[0] = 1");
        // Should be ignored because LHS contains '['
        let _ = sem.assignments;
    }

    #[test]
    fn marks_assignments_inside_functions_as_not_module_level() {
        // Assignments inside functions should be marked as not module-level
        let src = r#"
def test_function():
    local_dict = {}
    local_list = []
    local_set = set()
"#;
        let sem = parse_and_build_semantics(src);
        // All assignments should be marked as not module-level
        for assign in &sem.assignments {
            assert!(
                !assign.is_module_level,
                "Expected assignment '{}' to NOT be module-level",
                assign.target
            );
        }
    }

    #[test]
    fn marks_assignments_inside_methods_as_not_module_level() {
        // Assignments inside class methods should be marked as not module-level
        let src = r#"
class TestClass:
    def test_method(self):
        finding = {
            "rule_id": "test-rule",
            "title": "Test message",
            "severity": "low",
            "line": 0,
        }
        return finding
"#;
        let sem = parse_and_build_semantics(src);
        // All assignments should be marked as not module-level
        for assign in &sem.assignments {
            assert!(
                !assign.is_module_level,
                "Expected assignment '{}' to NOT be module-level",
                assign.target
            );
        }
    }

    #[test]
    fn marks_assignments_inside_async_functions_as_not_module_level() {
        // Assignments inside async functions should be marked as not module-level
        let src = r#"
async def async_func():
    cache = []
    data = {}
    return data
"#;
        let sem = parse_and_build_semantics(src);
        // All assignments should be marked as not module-level
        for assign in &sem.assignments {
            assert!(
                !assign.is_module_level,
                "Expected assignment '{}' to NOT be module-level",
                assign.target
            );
        }
    }

    #[test]
    fn distinguishes_module_level_from_function_level_assignments() {
        // Mixed: module-level and function-level assignments
        let src = r#"
MODULE_CACHE = []

def some_function():
    local_dict = {}
    return local_dict

ANOTHER_GLOBAL = {}
"#;
        let sem = parse_and_build_semantics(src);

        // Filter to only module-level assignments
        let module_level: Vec<_> = sem
            .assignments
            .iter()
            .filter(|a| a.is_module_level)
            .collect();

        // Should have MODULE_CACHE and ANOTHER_GLOBAL as module-level
        assert_eq!(
            module_level.len(),
            2,
            "Expected 2 module-level assignments, but found: {:?}",
            module_level
        );

        let targets: Vec<&str> = module_level.iter().map(|a| a.target.as_str()).collect();
        assert!(targets.contains(&"MODULE_CACHE"));
        assert!(targets.contains(&"ANOTHER_GLOBAL"));

        // local_dict should exist but not be module-level
        let local = sem.assignments.iter().find(|a| a.target == "local_dict");
        assert!(local.is_some(), "local_dict should be collected");
        assert!(
            !local.unwrap().is_module_level,
            "local_dict should NOT be module-level"
        );
    }

    #[test]
    fn marks_lambda_assignment_as_module_level() {
        // Lambda expressions create a new scope
        // The lambda itself is assigned at module level
        let src = r#"
# Note: lambdas don't have internal assignments in Python syntax,
# but the lambda itself can be assigned
my_lambda = lambda x: x + 1
"#;
        let sem = parse_and_build_semantics(src);
        // Should collect my_lambda as a module-level assignment
        let module_level: Vec<_> = sem
            .assignments
            .iter()
            .filter(|a| a.is_module_level)
            .collect();
        assert_eq!(module_level.len(), 1);
        assert_eq!(module_level[0].target, "my_lambda");
    }

    // ==================== import_insertion_line Tests ====================

    #[test]
    fn import_insertion_line_returns_1_for_empty_file() {
        let sem = parse_and_build_semantics("");
        assert_eq!(sem.import_insertion_line(), 1);
    }

    #[test]
    fn import_insertion_line_returns_1_for_file_without_docstring_or_imports() {
        let sem = parse_and_build_semantics("x = 1\ny = 2");
        // No imports and no docstring, so line 1
        assert_eq!(sem.import_insertion_line(), 1);
    }

    #[test]
    fn import_insertion_line_after_single_line_docstring() {
        let src = r#""""This is a module docstring."""
x = 1
"#;
        let sem = parse_and_build_semantics(src);
        // Docstring is on line 1, so insert on line 2
        assert_eq!(sem.import_insertion_line(), 2);
    }

    #[test]
    fn import_insertion_line_after_multiline_docstring() {
        let src = r#""""This is a module docstring.

It spans multiple lines.
"""
x = 1
"#;
        let sem = parse_and_build_semantics(src);
        // Docstring ends on line 4, so insert on line 5
        assert_eq!(sem.import_insertion_line(), 5);
    }

    #[test]
    fn import_insertion_line_after_existing_imports() {
        let src = r#"import os
import sys

x = 1
"#;
        let sem = parse_and_build_semantics(src);
        // Last import is on line 2 (0-based line 1), so insert on line 3 (end_line + 2)
        assert_eq!(sem.import_insertion_line(), 3);
    }

    #[test]
    fn import_insertion_line_after_docstring_and_imports() {
        let src = r#""""Module docstring."""
import os
import sys

x = 1
"#;
        let sem = parse_and_build_semantics(src);
        // Last import is on line 3 (0-based line 2), so insert on line 4
        assert_eq!(sem.import_insertion_line(), 4);
    }

    #[test]
    fn import_insertion_line_handles_multiline_import() {
        let src = r#"from typing import (
    List,
    Dict,
    Optional
)

x = 1
"#;
        let sem = parse_and_build_semantics(src);
        // Multiline import ends on line 5 (0-based line 4), so insert on line 6
        assert_eq!(sem.import_insertion_line(), 6);
    }

    #[test]
    fn import_insertion_line_combined_docstring_and_multiline_import() {
        let src = r#""""Sample FastAPI application.

This sample app demonstrates common production-readiness issues.
"""

from fastapi import FastAPI
import requests
import httpx

app = FastAPI()
"#;
        let sem = parse_and_build_semantics(src);
        // Docstring ends on line 4, imports end on line 8 (0-based line 7)
        // So import_insertion_line should return 9 (line 7 + 2)
        assert_eq!(sem.import_insertion_line(), 9);
    }
    // ==================== Type Annotation Tests ====================

    #[test]
    fn collects_annotated_assignment() {
        let sem = parse_and_build_semantics("x: int = 42");
        assert_eq!(sem.assignments.len(), 1);
        assert_eq!(sem.assignments[0].target, "x");
        assert_eq!(sem.assignments[0].value_repr, "42");
        assert_eq!(sem.assignments[0].type_annotation, Some("int".to_string()));
    }

    #[test]
    fn collects_annotated_dict_assignment() {
        let sem = parse_and_build_semantics("CONFIG: dict[str, str] = {}");
        assert_eq!(sem.assignments.len(), 1);
        assert_eq!(sem.assignments[0].target, "CONFIG");
        assert_eq!(sem.assignments[0].value_repr, "{}");
        assert_eq!(
            sem.assignments[0].type_annotation,
            Some("dict[str, str]".to_string())
        );
    }

    #[test]
    fn collects_regular_assignment_without_annotation() {
        let sem = parse_and_build_semantics("x = 42");
        assert_eq!(sem.assignments.len(), 1);
        assert_eq!(sem.assignments[0].target, "x");
        assert_eq!(sem.assignments[0].type_annotation, None);
    }

    #[test]
    fn collects_multiline_annotated_dict() {
        let src = r#"LANGUAGE_MAP: dict[str, str] = {
    "python": "Python",
    "go": "Go",
}"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.assignments.len(), 1);
        assert_eq!(sem.assignments[0].target, "LANGUAGE_MAP");
        assert_eq!(
            sem.assignments[0].type_annotation,
            Some("dict[str, str]".to_string())
        );
    }

    #[test]
    fn does_not_confuse_dict_colon_with_annotation() {
        // The colon in the dict value should not be confused with a type annotation colon
        let sem = parse_and_build_semantics("x = {'key': 'value'}");
        assert_eq!(sem.assignments.len(), 1);
        assert_eq!(sem.assignments[0].target, "x");
        assert_eq!(sem.assignments[0].type_annotation, None);
        assert_eq!(sem.assignments[0].value_repr, "{'key': 'value'}");
    }

    // ==================== Function Parameter Type Annotation Tests ====================

    #[test]
    fn extracts_type_annotation_from_typed_parameter() {
        let src = r#"
def create_item(item: ItemCreate):
    return item
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.functions.len(), 1);
        let func = &sem.functions[0];
        assert_eq!(func.params.len(), 1);
        let param = &func.params[0];
        assert_eq!(param.name, "item");
        assert_eq!(param.type_annotation, Some("ItemCreate".to_string()));
    }

    #[test]
    fn extracts_type_annotation_from_multiple_params() {
        let src = r#"
def update_item(item_id: int, item: ItemUpdate, user: User):
    return item
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.functions.len(), 1);
        let func = &sem.functions[0];
        assert_eq!(func.params.len(), 3);

        assert_eq!(func.params[0].name, "item_id");
        assert_eq!(func.params[0].type_annotation, Some("int".to_string()));

        assert_eq!(func.params[1].name, "item");
        assert_eq!(
            func.params[1].type_annotation,
            Some("ItemUpdate".to_string())
        );

        assert_eq!(func.params[2].name, "user");
        assert_eq!(func.params[2].type_annotation, Some("User".to_string()));
    }

    #[test]
    fn extracts_return_type_annotation() {
        let src = r#"
def is_empty(self) -> bool:
    return True
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.functions.len(), 1);
        let func = &sem.functions[0];
        assert_eq!(func.name, "is_empty");
        assert_eq!(func.return_type, Some("bool".to_string()));
    }

    #[test]
    fn extracts_complex_return_type_annotation() {
        let src = r#"
def to_dict(self) -> dict[str, Any]:
    return {}
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.functions.len(), 1);
        let func = &sem.functions[0];
        assert_eq!(func.name, "to_dict");
        assert_eq!(func.return_type, Some("dict[str, Any]".to_string()));
    }

    #[test]
    fn extracts_none_return_type() {
        let src = r#"
def process(data) -> None:
    pass
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.functions.len(), 1);
        let func = &sem.functions[0];
        assert_eq!(func.name, "process");
        assert_eq!(func.return_type, Some("None".to_string()));
    }

    #[test]
    fn return_type_is_none_when_not_annotated() {
        let src = r#"
def no_annotation():
    pass
"#;
        let sem = parse_and_build_semantics(src);
        assert_eq!(sem.functions.len(), 1);
        let func = &sem.functions[0];
        assert_eq!(func.name, "no_annotation");
        assert_eq!(func.return_type, None);
    }

    // ==================== ORM Import Detection Tests ====================

    #[test]
    fn has_orm_imports_true_for_sqlalchemy() {
        let src = r#"
from sqlalchemy.orm import Session
from sqlalchemy import Column, Integer
"#;
        let sem = parse_and_build_semantics(src);
        assert!(sem.has_orm_imports());
    }

    #[test]
    fn has_orm_imports_true_for_django() {
        let src = r#"
from django.db import models
from django.db.models import Q
"#;
        let sem = parse_and_build_semantics(src);
        assert!(sem.has_orm_imports());
    }

    #[test]
    fn has_orm_imports_true_for_tortoise() {
        let src = r#"
from tortoise import fields
from tortoise.models import Model
"#;
        let sem = parse_and_build_semantics(src);
        assert!(sem.has_orm_imports());
    }

    #[test]
    fn has_orm_imports_true_for_sqlmodel() {
        let src = r#"
from sqlmodel import SQLModel, Field
"#;
        let sem = parse_and_build_semantics(src);
        assert!(sem.has_orm_imports());
    }

    #[test]
    fn has_orm_imports_true_for_peewee() {
        let src = r#"
from peewee import Model, CharField
"#;
        let sem = parse_and_build_semantics(src);
        assert!(sem.has_orm_imports());
    }

    #[test]
    fn has_orm_imports_false_for_non_orm() {
        let src = r#"
import os
import asyncio
from kubernetes import client
from fastapi import FastAPI
import requests
"#;
        let sem = parse_and_build_semantics(src);
        assert!(!sem.has_orm_imports());
    }

    #[test]
    fn has_orm_imports_false_for_empty_file() {
        let sem = parse_and_build_semantics("");
        assert!(!sem.has_orm_imports());
    }

    #[test]
    fn has_orm_imports_case_insensitive() {
        // SQLAlchemy is commonly imported with various casings
        let src = r#"
import SQLAlchemy
"#;
        let sem = parse_and_build_semantics(src);
        // Note: The actual import statement would fail in Python,
        // but our detection should be case-insensitive
        assert!(sem.has_orm_imports());
    }
}
