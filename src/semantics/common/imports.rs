//! Common import/dependency abstractions for cross-language analysis.
//!
//! This module provides language-agnostic types for import statements,
//! enabling shared rule logic for analyzing dependencies across languages.

use serde::{Deserialize, Serialize};

use super::CommonLocation;

/// Import style classification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportStyle {
    /// Full module import (import foo, import "foo")
    Module,
    /// Named imports from module (from foo import bar, import { bar } from "foo")
    Named,
    /// Star/wildcard import (from foo import *, import * as foo from "bar")
    Star,
    /// Default import (TypeScript/ES6: import foo from "bar")
    Default,
    /// Side-effect only import (import "foo")
    SideEffect,
    /// Re-export (export { foo } from "bar")
    ReExport,
}

impl Default for ImportStyle {
    fn default() -> Self {
        Self::Module
    }
}

/// Import source type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ImportSource {
    /// Standard library module
    StandardLib,
    /// Third-party/external package
    External,
    /// Local/relative import from same project
    Local,
    /// Unknown source
    Unknown,
}

impl Default for ImportSource {
    fn default() -> Self {
        Self::Unknown
    }
}

/// An imported item (for named imports)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedItem {
    /// Original name in the source module
    pub name: String,
    /// Alias if renamed (as alias)
    pub alias: Option<String>,
}

impl ImportedItem {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            alias: None,
        }
    }

    pub fn with_alias(mut self, alias: impl Into<String>) -> Self {
        self.alias = Some(alias.into());
        self
    }

    /// Get the name used in the local scope (alias if present, otherwise name)
    pub fn local_name(&self) -> &str {
        self.alias.as_deref().unwrap_or(&self.name)
    }
}

/// A language-agnostic import statement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Import {
    /// The module path being imported (e.g., "os.path", "express", "@types/node")
    pub module_path: String,

    /// Import style
    pub style: ImportStyle,

    /// Import source type
    pub source: ImportSource,

    /// Specific items imported (for Named imports)
    pub items: Vec<ImportedItem>,

    /// Module alias (import foo as bar, import * as foo)
    pub module_alias: Option<String>,

    /// Full import statement text
    pub raw_text: String,

    /// Whether this is a type-only import (TypeScript)
    pub is_type_only: bool,

    /// Whether this is a dynamic import (import())
    pub is_dynamic: bool,

    /// Location in source file
    pub location: CommonLocation,
}

impl Import {
    /// Check if this import matches a module pattern
    pub fn matches_module(&self, pattern: &str) -> bool {
        self.module_path == pattern
            || self.module_path.starts_with(&format!("{pattern}."))
            || self.module_path.starts_with(&format!("{pattern}/"))
    }

    /// Check if this import imports a specific item
    pub fn imports_item(&self, item_name: &str) -> bool {
        self.items
            .iter()
            .any(|i| i.name == item_name || i.local_name() == item_name)
    }

    /// Get the local name used for this module
    pub fn local_module_name(&self) -> Option<&str> {
        self.module_alias.as_deref().or_else(|| {
            // For simple module imports, the module name is the last segment
            self.module_path.split(&['.', '/']).last()
        })
    }

    /// Check if this is a standard library import
    pub fn is_stdlib(&self) -> bool {
        matches!(self.source, ImportSource::StandardLib)
    }

    /// Check if this is a third-party import
    pub fn is_external(&self) -> bool {
        matches!(self.source, ImportSource::External)
    }

    /// Check if this is a local/relative import
    pub fn is_local(&self) -> bool {
        matches!(self.source, ImportSource::Local)
            || self.module_path.starts_with('.')
            || self.module_path.starts_with("./")
            || self.module_path.starts_with("../")
    }

    /// Get the package name (first segment of module path)
    pub fn package_name(&self) -> &str {
        self.module_path
            .split(&['.', '/'])
            .next()
            .unwrap_or(&self.module_path)
    }
}

/// Common import patterns for well-known libraries
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KnownLibrary {
    // === HTTP Clients ===
    /// Python: requests
    Requests,
    /// Python: httpx
    Httpx,
    /// Python: aiohttp
    Aiohttp,
    /// Python: urllib3
    Urllib3,
    /// Go: net/http
    GoNetHttp,
    /// Rust: reqwest
    Reqwest,
    /// Rust: hyper
    Hyper,
    /// TypeScript/Node: axios
    Axios,
    /// TypeScript/Node: fetch / node-fetch
    Fetch,
    /// TypeScript/Node: got
    Got,
    /// Java: OkHttp
    OkHttp,
    /// Java: HttpClient
    JavaHttpClient,
    /// Java: RestTemplate (Spring)
    RestTemplate,
    /// Java: WebClient (Spring WebFlux)
    WebClient,

    // === Async Runtimes ===
    /// Python: asyncio
    Asyncio,
    /// Rust: tokio
    Tokio,
    /// Rust: async-std
    AsyncStd,
    /// Go: (goroutines built-in)
    Goroutine,
    /// Java: CompletableFuture
    CompletableFuture,
    /// Java: Project Reactor
    ProjectReactor,

    // === Database ===
    /// Python: sqlalchemy
    Sqlalchemy,
    /// Python: django.db
    DjangoORM,
    /// Go: database/sql
    GoDatabaseSql,
    /// Go: gorm
    Gorm,
    /// Go: sqlx
    GoSqlx,
    /// Rust: diesel
    Diesel,
    /// Rust: sqlx
    RustSqlx,
    /// Rust: sea-orm
    SeaOrm,
    /// TypeScript: prisma
    Prisma,
    /// TypeScript: typeorm
    TypeORM,
    /// TypeScript: knex
    Knex,
    /// TypeScript: sequelize
    Sequelize,
    /// Java: JPA/Hibernate
    JpaHibernate,
    /// Java: Spring Data
    SpringData,
    /// Java: JOOQ
    Jooq,

    // === Web Frameworks ===
    /// Python: fastapi
    FastAPI,
    /// Python: flask
    Flask,
    /// Python: django
    Django,
    /// Go: gin
    Gin,
    /// Go: echo
    Echo,
    /// Go: chi
    Chi,
    /// Rust: axum
    Axum,
    /// Rust: actix-web
    ActixWeb,
    /// Rust: rocket
    Rocket,
    /// TypeScript: express
    Express,
    /// TypeScript: nestjs
    NestJS,
    /// TypeScript: fastify
    Fastify,
    /// TypeScript: koa
    Koa,
    /// Java: Spring MVC
    SpringMVC,
    /// Java: Spring WebFlux
    SpringWebFlux,

    // === Redis ===
    /// Python: redis
    PythonRedis,
    /// Go: go-redis
    GoRedis,
    /// Rust: redis-rs
    RustRedis,
    /// TypeScript: ioredis
    IORedis,
    /// Java: Jedis, Lettuce
    JavaRedis,

    // === Logging ===
    /// Python: logging
    PythonLogging,
    /// Python: structlog
    Structlog,
    /// Go: log
    GoLog,
    /// Go: zap
    Zap,
    /// Go: zerolog
    Zerolog,
    /// Rust: tracing
    Tracing,
    /// Rust: log
    RustLog,
    /// TypeScript: winston
    Winston,
    /// TypeScript: pino
    Pino,
    /// Java: SLF4J, Logback
    Slf4j,

    // === Retry/Resilience ===
    /// Python: tenacity
    Tenacity,
    /// Python: stamina
    Stamina,
    /// Go: github.com/avast/retry-go
    GoRetry,
    /// Java: resilience4j
    Resilience4j,

    /// Unknown/other library
    Unknown,
}

impl KnownLibrary {
    /// Detect library from import module path and language
    pub fn from_import(module_path: &str, language: &str) -> Self {
        match language {
            "python" => Self::from_python_import(module_path),
            "go" => Self::from_go_import(module_path),
            "rust" => Self::from_rust_import(module_path),
            "typescript" | "javascript" => Self::from_ts_import(module_path),
            "java" => Self::from_java_import(module_path),
            _ => Self::Unknown,
        }
    }

    fn from_python_import(module_path: &str) -> Self {
        if module_path == "requests" || module_path.starts_with("requests.") {
            Self::Requests
        } else if module_path == "httpx" || module_path.starts_with("httpx.") {
            Self::Httpx
        } else if module_path == "aiohttp" || module_path.starts_with("aiohttp.") {
            Self::Aiohttp
        } else if module_path == "urllib3" || module_path.starts_with("urllib3.") {
            Self::Urllib3
        } else if module_path == "asyncio" || module_path.starts_with("asyncio.") {
            Self::Asyncio
        } else if module_path == "sqlalchemy" || module_path.starts_with("sqlalchemy.") {
            Self::Sqlalchemy
        } else if module_path == "django" || module_path.starts_with("django.") {
            Self::Django
        } else if module_path == "fastapi" || module_path.starts_with("fastapi.") {
            Self::FastAPI
        } else if module_path == "flask" || module_path.starts_with("flask.") {
            Self::Flask
        } else if module_path == "redis" || module_path.starts_with("redis.") {
            Self::PythonRedis
        } else if module_path == "logging" || module_path.starts_with("logging.") {
            Self::PythonLogging
        } else if module_path == "structlog" || module_path.starts_with("structlog.") {
            Self::Structlog
        } else if module_path == "tenacity" || module_path.starts_with("tenacity.") {
            Self::Tenacity
        } else if module_path == "stamina" || module_path.starts_with("stamina.") {
            Self::Stamina
        } else {
            Self::Unknown
        }
    }

    fn from_go_import(module_path: &str) -> Self {
        match module_path {
            "net/http" => Self::GoNetHttp,
            "database/sql" => Self::GoDatabaseSql,
            "log" => Self::GoLog,
            s if s.contains("gorm.io/gorm") => Self::Gorm,
            s if s.contains("go.uber.org/zap") => Self::Zap,
            s if s.contains("rs/zerolog") => Self::Zerolog,
            s if s.contains("go-redis") => Self::GoRedis,
            s if s.contains("gin-gonic") => Self::Gin,
            s if s.contains("labstack/echo") => Self::Echo,
            s if s.contains("go-chi") => Self::Chi,
            s if s.contains("jmoiron/sqlx") => Self::GoSqlx,
            s if s.contains("avast/retry-go") => Self::GoRetry,
            _ => Self::Unknown,
        }
    }

    fn from_rust_import(module_path: &str) -> Self {
        if module_path == "reqwest" || module_path.starts_with("reqwest::") {
            Self::Reqwest
        } else if module_path == "hyper" || module_path.starts_with("hyper::") {
            Self::Hyper
        } else if module_path == "tokio" || module_path.starts_with("tokio::") {
            Self::Tokio
        } else if module_path == "async_std" || module_path.starts_with("async_std::") {
            Self::AsyncStd
        } else if module_path == "diesel" || module_path.starts_with("diesel::") {
            Self::Diesel
        } else if module_path == "sqlx" || module_path.starts_with("sqlx::") {
            Self::RustSqlx
        } else if module_path == "sea_orm" || module_path.starts_with("sea_orm::") {
            Self::SeaOrm
        } else if module_path == "axum" || module_path.starts_with("axum::") {
            Self::Axum
        } else if module_path == "actix_web" || module_path.starts_with("actix_web::") {
            Self::ActixWeb
        } else if module_path == "rocket" || module_path.starts_with("rocket::") {
            Self::Rocket
        } else if module_path == "redis" || module_path.starts_with("redis::") {
            Self::RustRedis
        } else if module_path == "tracing" || module_path.starts_with("tracing::") {
            Self::Tracing
        } else if module_path == "log" || module_path.starts_with("log::") {
            Self::RustLog
        } else {
            Self::Unknown
        }
    }

    fn from_ts_import(module_path: &str) -> Self {
        if module_path == "axios" {
            Self::Axios
        } else if module_path == "got" {
            Self::Got
        } else if module_path == "node-fetch"
            || module_path == "isomorphic-fetch"
            || module_path == "cross-fetch"
        {
            Self::Fetch
        } else if module_path == "prisma" || module_path.starts_with("@prisma/") {
            Self::Prisma
        } else if module_path == "typeorm" {
            Self::TypeORM
        } else if module_path == "knex" {
            Self::Knex
        } else if module_path == "sequelize" {
            Self::Sequelize
        } else if module_path == "express" {
            Self::Express
        } else if module_path == "fastify" {
            Self::Fastify
        } else if module_path == "koa" || module_path.starts_with("@koa/") {
            Self::Koa
        } else if module_path == "ioredis" {
            Self::IORedis
        } else if module_path == "winston" {
            Self::Winston
        } else if module_path == "pino" {
            Self::Pino
        } else if module_path.starts_with("@nestjs/") {
            Self::NestJS
        } else {
            Self::Unknown
        }
    }

    fn from_java_import(module_path: &str) -> Self {
        match module_path {
            s if s.starts_with("okhttp") || s.contains("okhttp3") => Self::OkHttp,
            s if s.contains("java.net.http") => Self::JavaHttpClient,
            s if s.contains("RestTemplate") => Self::RestTemplate,
            s if s.contains("WebClient") => Self::WebClient,
            s if s.contains("CompletableFuture") => Self::CompletableFuture,
            s if s.contains("reactor.core") => Self::ProjectReactor,
            s if s.contains("javax.persistence") || s.contains("hibernate") => Self::JpaHibernate,
            s if s.contains("springframework.data") => Self::SpringData,
            s if s.contains("jooq") => Self::Jooq,
            s if s.contains("springframework.web") && s.contains("servlet") => Self::SpringMVC,
            s if s.contains("springframework.webflux") => Self::SpringWebFlux,
            s if s.contains("redis") || s.contains("jedis") || s.contains("lettuce") => {
                Self::JavaRedis
            }
            s if s.contains("slf4j") || s.contains("logback") => Self::Slf4j,
            s if s.contains("resilience4j") => Self::Resilience4j,
            _ => Self::Unknown,
        }
    }

    /// Check if this library is an HTTP client
    pub fn is_http_client(&self) -> bool {
        matches!(
            self,
            Self::Requests
                | Self::Httpx
                | Self::Aiohttp
                | Self::Urllib3
                | Self::GoNetHttp
                | Self::Reqwest
                | Self::Hyper
                | Self::Axios
                | Self::Fetch
                | Self::Got
                | Self::OkHttp
                | Self::JavaHttpClient
                | Self::RestTemplate
                | Self::WebClient
        )
    }

    /// Check if this library is a database/ORM library
    pub fn is_database(&self) -> bool {
        matches!(
            self,
            Self::Sqlalchemy
                | Self::DjangoORM
                | Self::GoDatabaseSql
                | Self::Gorm
                | Self::GoSqlx
                | Self::Diesel
                | Self::RustSqlx
                | Self::SeaOrm
                | Self::Prisma
                | Self::TypeORM
                | Self::Knex
                | Self::Sequelize
                | Self::JpaHibernate
                | Self::SpringData
                | Self::Jooq
        )
    }

    /// Check if this library is an async runtime
    pub fn is_async_runtime(&self) -> bool {
        matches!(
            self,
            Self::Asyncio
                | Self::Tokio
                | Self::AsyncStd
                | Self::Goroutine
                | Self::CompletableFuture
                | Self::ProjectReactor
        )
    }

    /// Check if this library is a web framework
    pub fn is_web_framework(&self) -> bool {
        matches!(
            self,
            Self::FastAPI
                | Self::Flask
                | Self::Django
                | Self::Gin
                | Self::Echo
                | Self::Chi
                | Self::Axum
                | Self::ActixWeb
                | Self::Rocket
                | Self::Express
                | Self::NestJS
                | Self::Fastify
                | Self::Koa
                | Self::SpringMVC
                | Self::SpringWebFlux
        )
    }
}

/// Builder for creating Import instances
#[derive(Debug, Default)]
pub struct ImportBuilder {
    module_path: Option<String>,
    style: ImportStyle,
    source: ImportSource,
    items: Vec<ImportedItem>,
    module_alias: Option<String>,
    raw_text: String,
    is_type_only: bool,
    is_dynamic: bool,
    location: Option<CommonLocation>,
}

impl ImportBuilder {
    pub fn new(module_path: impl Into<String>) -> Self {
        Self {
            module_path: Some(module_path.into()),
            ..Default::default()
        }
    }

    pub fn style(mut self, style: ImportStyle) -> Self {
        self.style = style;
        self
    }

    pub fn source(mut self, source: ImportSource) -> Self {
        self.source = source;
        self
    }

    pub fn item(mut self, item: ImportedItem) -> Self {
        self.items.push(item);
        self
    }

    pub fn module_alias(mut self, alias: impl Into<String>) -> Self {
        self.module_alias = Some(alias.into());
        self
    }

    pub fn raw_text(mut self, text: impl Into<String>) -> Self {
        self.raw_text = text.into();
        self
    }

    pub fn type_only(mut self, is_type_only: bool) -> Self {
        self.is_type_only = is_type_only;
        self
    }

    pub fn dynamic(mut self, is_dynamic: bool) -> Self {
        self.is_dynamic = is_dynamic;
        self
    }

    pub fn location(mut self, location: CommonLocation) -> Self {
        self.location = Some(location);
        self
    }

    pub fn build(self) -> Option<Import> {
        Some(Import {
            module_path: self.module_path?,
            style: self.style,
            source: self.source,
            items: self.items,
            module_alias: self.module_alias,
            raw_text: self.raw_text,
            is_type_only: self.is_type_only,
            is_dynamic: self.is_dynamic,
            location: self.location?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;

    fn make_location() -> CommonLocation {
        CommonLocation {
            file_id: FileId(1),
            line: 1,
            column: 0,
            start_byte: 0,
            end_byte: 20,
        }
    }

    #[test]
    fn imported_item_local_name() {
        let item = ImportedItem::new("datetime").with_alias("dt");
        assert_eq!(item.local_name(), "dt");

        let item = ImportedItem::new("datetime");
        assert_eq!(item.local_name(), "datetime");
    }

    #[test]
    fn import_matches_module() {
        let import = ImportBuilder::new("requests")
            .style(ImportStyle::Module)
            .location(make_location())
            .build()
            .unwrap();

        assert!(import.matches_module("requests"));
        assert!(!import.matches_module("httpx"));

        let import = ImportBuilder::new("os.path")
            .style(ImportStyle::Module)
            .location(make_location())
            .build()
            .unwrap();

        assert!(import.matches_module("os"));
        assert!(import.matches_module("os.path"));
    }

    #[test]
    fn import_imports_item() {
        let import = ImportBuilder::new("typing")
            .style(ImportStyle::Named)
            .item(ImportedItem::new("Optional"))
            .item(ImportedItem::new("List").with_alias("ListType"))
            .location(make_location())
            .build()
            .unwrap();

        assert!(import.imports_item("Optional"));
        assert!(import.imports_item("List"));
        assert!(import.imports_item("ListType"));
        assert!(!import.imports_item("Dict"));
    }

    #[test]
    fn known_library_detection_python() {
        assert_eq!(
            KnownLibrary::from_import("requests", "python"),
            KnownLibrary::Requests
        );
        assert_eq!(
            KnownLibrary::from_import("sqlalchemy.orm", "python"),
            KnownLibrary::Sqlalchemy
        );
        assert_eq!(
            KnownLibrary::from_import("fastapi", "python"),
            KnownLibrary::FastAPI
        );
    }

    #[test]
    fn known_library_detection_go() {
        assert_eq!(
            KnownLibrary::from_import("net/http", "go"),
            KnownLibrary::GoNetHttp
        );
        assert_eq!(
            KnownLibrary::from_import("github.com/gin-gonic/gin", "go"),
            KnownLibrary::Gin
        );
        assert_eq!(
            KnownLibrary::from_import("go.uber.org/zap", "go"),
            KnownLibrary::Zap
        );
    }

    #[test]
    fn known_library_detection_rust() {
        assert_eq!(
            KnownLibrary::from_import("reqwest", "rust"),
            KnownLibrary::Reqwest
        );
        assert_eq!(
            KnownLibrary::from_import("tokio::spawn", "rust"),
            KnownLibrary::Tokio
        );
        assert_eq!(
            KnownLibrary::from_import("axum::Router", "rust"),
            KnownLibrary::Axum
        );
    }

    #[test]
    fn known_library_detection_typescript() {
        assert_eq!(
            KnownLibrary::from_import("axios", "typescript"),
            KnownLibrary::Axios
        );
        assert_eq!(
            KnownLibrary::from_import("@nestjs/common", "typescript"),
            KnownLibrary::NestJS
        );
        assert_eq!(
            KnownLibrary::from_import("prisma", "typescript"),
            KnownLibrary::Prisma
        );
    }

    #[test]
    fn library_category_checks() {
        assert!(KnownLibrary::Requests.is_http_client());
        assert!(KnownLibrary::Reqwest.is_http_client());

        assert!(KnownLibrary::Sqlalchemy.is_database());
        assert!(KnownLibrary::Prisma.is_database());

        assert!(KnownLibrary::Asyncio.is_async_runtime());
        assert!(KnownLibrary::Tokio.is_async_runtime());

        assert!(KnownLibrary::FastAPI.is_web_framework());
        assert!(KnownLibrary::Express.is_web_framework());
    }
}
