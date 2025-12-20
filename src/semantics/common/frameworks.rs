//! Framework detection registry for cross-language analysis.
//!
//! This module provides a centralized framework detection system that identifies
//! which web frameworks, database libraries, and other technologies are in use
//! based on imports and code patterns.

use serde::{Deserialize, Serialize};

use super::imports::{Import, KnownLibrary};

/// Categories of frameworks/libraries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FrameworkCategory {
    /// Web framework (FastAPI, Express, Spring MVC, etc.)
    Web,
    /// ORM/Database library (SQLAlchemy, Prisma, JPA, etc.)
    Database,
    /// HTTP client library (requests, axios, etc.)
    HttpClient,
    /// Async runtime (asyncio, tokio, etc.)
    AsyncRuntime,
    /// Caching (Redis, Memcached, etc.)
    Cache,
    /// Message queue (RabbitMQ, Kafka, etc.)
    MessageQueue,
    /// Logging framework
    Logging,
    /// Validation library (Pydantic, Joi, etc.)
    Validation,
    /// Testing framework
    Testing,
    /// Other
    Other,
}

/// Detected framework information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedFramework {
    /// Framework name
    pub name: String,
    /// Framework category
    pub category: FrameworkCategory,
    /// Specific version (if detectable)
    pub version: Option<String>,
    /// Import that triggered detection
    pub detection_source: String,
    /// Language
    pub language: String,
    /// Whether this is the primary framework in its category
    pub is_primary: bool,
}

impl DetectedFramework {
    pub fn new(
        name: impl Into<String>,
        category: FrameworkCategory,
        detection_source: impl Into<String>,
        language: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            category,
            version: None,
            detection_source: detection_source.into(),
            language: language.into(),
            is_primary: false,
        }
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    pub fn primary(mut self) -> Self {
        self.is_primary = true;
        self
    }
}

/// Framework detection result for a file or project
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FrameworkProfile {
    /// All detected frameworks
    pub frameworks: Vec<DetectedFramework>,
}

impl FrameworkProfile {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a detected framework
    pub fn add(&mut self, framework: DetectedFramework) {
        self.frameworks.push(framework);
    }

    /// Get the primary web framework
    pub fn primary_web_framework(&self) -> Option<&DetectedFramework> {
        self.frameworks
            .iter()
            .filter(|f| f.category == FrameworkCategory::Web)
            .find(|f| f.is_primary)
            .or_else(|| {
                self.frameworks
                    .iter()
                    .find(|f| f.category == FrameworkCategory::Web)
            })
    }

    /// Get the primary database framework
    pub fn primary_database(&self) -> Option<&DetectedFramework> {
        self.frameworks
            .iter()
            .filter(|f| f.category == FrameworkCategory::Database)
            .find(|f| f.is_primary)
            .or_else(|| {
                self.frameworks
                    .iter()
                    .find(|f| f.category == FrameworkCategory::Database)
            })
    }

    /// Get all frameworks in a category
    pub fn by_category(&self, category: FrameworkCategory) -> Vec<&DetectedFramework> {
        self.frameworks
            .iter()
            .filter(|f| f.category == category)
            .collect()
    }

    /// Check if a specific framework is detected by name
    pub fn has_framework(&self, name: &str) -> bool {
        self.frameworks
            .iter()
            .any(|f| f.name.eq_ignore_ascii_case(name))
    }

    /// Check if any framework in a category is detected
    pub fn has_category(&self, category: FrameworkCategory) -> bool {
        self.frameworks.iter().any(|f| f.category == category)
    }

    /// Get framework names as a list (for display)
    pub fn framework_names(&self) -> Vec<&str> {
        self.frameworks.iter().map(|f| f.name.as_str()).collect()
    }
}

/// Framework detector that analyzes imports to detect frameworks
pub struct FrameworkDetector {
    /// Language being analyzed
    language: String,
}

impl FrameworkDetector {
    pub fn new(language: impl Into<String>) -> Self {
        Self {
            language: language.into(),
        }
    }

    /// Detect frameworks from a list of imports
    pub fn detect_from_imports(&self, imports: &[Import]) -> FrameworkProfile {
        let mut profile = FrameworkProfile::new();

        for import in imports {
            if let Some(framework) = self.detect_single_import(import) {
                profile.add(framework);
            }
        }

        // Mark primary frameworks
        self.mark_primary_frameworks(&mut profile);

        profile
    }

    /// Detect framework from a single import
    fn detect_single_import(&self, import: &Import) -> Option<DetectedFramework> {
        let known = KnownLibrary::from_import(&import.module_path, &self.language);

        match known {
            // Web frameworks
            KnownLibrary::FastAPI => Some(DetectedFramework::new(
                "FastAPI",
                FrameworkCategory::Web,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Flask => Some(DetectedFramework::new(
                "Flask",
                FrameworkCategory::Web,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Django => Some(DetectedFramework::new(
                "Django",
                FrameworkCategory::Web,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Gin => Some(DetectedFramework::new(
                "Gin",
                FrameworkCategory::Web,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Echo => Some(DetectedFramework::new(
                "Echo",
                FrameworkCategory::Web,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Chi => Some(DetectedFramework::new(
                "Chi",
                FrameworkCategory::Web,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Axum => Some(DetectedFramework::new(
                "Axum",
                FrameworkCategory::Web,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::ActixWeb => Some(DetectedFramework::new(
                "Actix-web",
                FrameworkCategory::Web,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Rocket => Some(DetectedFramework::new(
                "Rocket",
                FrameworkCategory::Web,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Express => Some(DetectedFramework::new(
                "Express",
                FrameworkCategory::Web,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::NestJS => Some(DetectedFramework::new(
                "NestJS",
                FrameworkCategory::Web,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Fastify => Some(DetectedFramework::new(
                "Fastify",
                FrameworkCategory::Web,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Koa => Some(DetectedFramework::new(
                "Koa",
                FrameworkCategory::Web,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::SpringMVC | KnownLibrary::SpringWebFlux => Some(DetectedFramework::new(
                "Spring",
                FrameworkCategory::Web,
                &import.module_path,
                &self.language,
            )),

            // Database/ORM
            KnownLibrary::Sqlalchemy => Some(DetectedFramework::new(
                "SQLAlchemy",
                FrameworkCategory::Database,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::DjangoORM => Some(DetectedFramework::new(
                "Django ORM",
                FrameworkCategory::Database,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Gorm => Some(DetectedFramework::new(
                "GORM",
                FrameworkCategory::Database,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::GoSqlx => Some(DetectedFramework::new(
                "sqlx",
                FrameworkCategory::Database,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Diesel => Some(DetectedFramework::new(
                "Diesel",
                FrameworkCategory::Database,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::RustSqlx => Some(DetectedFramework::new(
                "sqlx",
                FrameworkCategory::Database,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::SeaOrm => Some(DetectedFramework::new(
                "SeaORM",
                FrameworkCategory::Database,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Prisma => Some(DetectedFramework::new(
                "Prisma",
                FrameworkCategory::Database,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::TypeORM => Some(DetectedFramework::new(
                "TypeORM",
                FrameworkCategory::Database,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Knex => Some(DetectedFramework::new(
                "Knex",
                FrameworkCategory::Database,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Sequelize => Some(DetectedFramework::new(
                "Sequelize",
                FrameworkCategory::Database,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::JpaHibernate => Some(DetectedFramework::new(
                "JPA/Hibernate",
                FrameworkCategory::Database,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::SpringData => Some(DetectedFramework::new(
                "Spring Data",
                FrameworkCategory::Database,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Jooq => Some(DetectedFramework::new(
                "jOOQ",
                FrameworkCategory::Database,
                &import.module_path,
                &self.language,
            )),

            // HTTP clients
            KnownLibrary::Requests => Some(DetectedFramework::new(
                "requests",
                FrameworkCategory::HttpClient,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Httpx => Some(DetectedFramework::new(
                "httpx",
                FrameworkCategory::HttpClient,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Aiohttp => Some(DetectedFramework::new(
                "aiohttp",
                FrameworkCategory::HttpClient,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Reqwest => Some(DetectedFramework::new(
                "reqwest",
                FrameworkCategory::HttpClient,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Axios => Some(DetectedFramework::new(
                "axios",
                FrameworkCategory::HttpClient,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Fetch => Some(DetectedFramework::new(
                "fetch",
                FrameworkCategory::HttpClient,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Got => Some(DetectedFramework::new(
                "got",
                FrameworkCategory::HttpClient,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::OkHttp => Some(DetectedFramework::new(
                "OkHttp",
                FrameworkCategory::HttpClient,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::RestTemplate => Some(DetectedFramework::new(
                "RestTemplate",
                FrameworkCategory::HttpClient,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::WebClient => Some(DetectedFramework::new(
                "WebClient",
                FrameworkCategory::HttpClient,
                &import.module_path,
                &self.language,
            )),

            // Async runtimes
            KnownLibrary::Asyncio => Some(DetectedFramework::new(
                "asyncio",
                FrameworkCategory::AsyncRuntime,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Tokio => Some(DetectedFramework::new(
                "Tokio",
                FrameworkCategory::AsyncRuntime,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::AsyncStd => Some(DetectedFramework::new(
                "async-std",
                FrameworkCategory::AsyncRuntime,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::ProjectReactor => Some(DetectedFramework::new(
                "Project Reactor",
                FrameworkCategory::AsyncRuntime,
                &import.module_path,
                &self.language,
            )),

            // Cache/Redis
            KnownLibrary::PythonRedis
            | KnownLibrary::GoRedis
            | KnownLibrary::RustRedis
            | KnownLibrary::IORedis
            | KnownLibrary::JavaRedis => Some(DetectedFramework::new(
                "Redis",
                FrameworkCategory::Cache,
                &import.module_path,
                &self.language,
            )),

            // Logging
            KnownLibrary::PythonLogging => Some(DetectedFramework::new(
                "logging",
                FrameworkCategory::Logging,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Structlog => Some(DetectedFramework::new(
                "structlog",
                FrameworkCategory::Logging,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Zap => Some(DetectedFramework::new(
                "zap",
                FrameworkCategory::Logging,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Zerolog => Some(DetectedFramework::new(
                "zerolog",
                FrameworkCategory::Logging,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Tracing => Some(DetectedFramework::new(
                "tracing",
                FrameworkCategory::Logging,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Winston => Some(DetectedFramework::new(
                "winston",
                FrameworkCategory::Logging,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Pino => Some(DetectedFramework::new(
                "pino",
                FrameworkCategory::Logging,
                &import.module_path,
                &self.language,
            )),
            KnownLibrary::Slf4j => Some(DetectedFramework::new(
                "SLF4J",
                FrameworkCategory::Logging,
                &import.module_path,
                &self.language,
            )),

            // Resilience
            KnownLibrary::Tenacity
            | KnownLibrary::Stamina
            | KnownLibrary::GoRetry
            | KnownLibrary::Resilience4j => Some(DetectedFramework::new(
                "Resilience",
                FrameworkCategory::Other,
                &import.module_path,
                &self.language,
            )),

            _ => None,
        }
    }

    /// Mark the primary framework in each category
    fn mark_primary_frameworks(&self, profile: &mut FrameworkProfile) {
        // For web frameworks, prefer full frameworks over micro-frameworks
        let web_priority = [
            "Django",
            "NestJS",
            "Spring",
            "FastAPI",
            "Express",
            "Axum",
            "Actix-web",
        ];
        self.mark_priority_framework(profile, FrameworkCategory::Web, &web_priority);

        // For databases, prefer ORMs over raw drivers
        let db_priority = [
            "SQLAlchemy",
            "Django ORM",
            "Prisma",
            "TypeORM",
            "JPA/Hibernate",
            "GORM",
            "Diesel",
            "SeaORM",
        ];
        self.mark_priority_framework(profile, FrameworkCategory::Database, &db_priority);

        // For HTTP clients, prefer async over sync
        let http_priority = ["httpx", "aiohttp", "reqwest", "axios", "WebClient"];
        self.mark_priority_framework(profile, FrameworkCategory::HttpClient, &http_priority);
    }

    fn mark_priority_framework(
        &self,
        profile: &mut FrameworkProfile,
        category: FrameworkCategory,
        priority: &[&str],
    ) {
        for name in priority {
            if let Some(fw) = profile
                .frameworks
                .iter_mut()
                .find(|f| f.category == category && f.name == *name)
            {
                fw.is_primary = true;
                return;
            }
        }

        // If no priority match, mark the first in category as primary
        if let Some(fw) = profile
            .frameworks
            .iter_mut()
            .find(|f| f.category == category)
        {
            fw.is_primary = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::ast::FileId;
    use crate::semantics::common::CommonLocation;
    use crate::semantics::common::imports::{ImportBuilder, ImportStyle};

    fn make_location() -> CommonLocation {
        CommonLocation {
            file_id: FileId(1),
            line: 1,
            column: 0,
            start_byte: 0,
            end_byte: 10,
        }
    }

    fn make_import(module: &str) -> Import {
        ImportBuilder::new(module)
            .style(ImportStyle::Module)
            .location(make_location())
            .build()
            .unwrap()
    }

    #[test]
    fn detect_fastapi() {
        let detector = FrameworkDetector::new("python");
        let imports = vec![make_import("fastapi")];
        let profile = detector.detect_from_imports(&imports);

        assert!(profile.has_framework("FastAPI"));
        assert!(profile.has_category(FrameworkCategory::Web));
        assert_eq!(profile.primary_web_framework().unwrap().name, "FastAPI");
    }

    #[test]
    fn detect_multiple_frameworks() {
        let detector = FrameworkDetector::new("python");
        let imports = vec![
            make_import("fastapi"),
            make_import("sqlalchemy"),
            make_import("httpx"),
        ];
        let profile = detector.detect_from_imports(&imports);

        assert!(profile.has_framework("FastAPI"));
        assert!(profile.has_framework("SQLAlchemy"));
        assert!(profile.has_framework("httpx"));
        assert_eq!(profile.frameworks.len(), 3);
    }

    #[test]
    fn detect_primary_framework() {
        let detector = FrameworkDetector::new("python");
        let imports = vec![make_import("flask"), make_import("fastapi")];
        let profile = detector.detect_from_imports(&imports);

        // FastAPI should be marked as primary over Flask
        let primary = profile.primary_web_framework().unwrap();
        assert_eq!(primary.name, "FastAPI");
        assert!(primary.is_primary);
    }

    #[test]
    fn detect_rust_frameworks() {
        let detector = FrameworkDetector::new("rust");
        let imports = vec![
            make_import("axum"),
            make_import("sqlx"),
            make_import("tokio"),
        ];
        let profile = detector.detect_from_imports(&imports);

        assert!(profile.has_framework("Axum"));
        assert!(profile.has_framework("sqlx"));
        assert!(profile.has_framework("Tokio"));
    }

    #[test]
    fn detect_typescript_frameworks() {
        let detector = FrameworkDetector::new("typescript");
        let imports = vec![
            make_import("express"),
            make_import("prisma"),
            make_import("axios"),
        ];
        let profile = detector.detect_from_imports(&imports);

        assert!(profile.has_framework("Express"));
        assert!(profile.has_framework("Prisma"));
        assert!(profile.has_framework("axios"));
    }

    #[test]
    fn framework_category_filtering() {
        let detector = FrameworkDetector::new("python");
        let imports = vec![
            make_import("fastapi"),
            make_import("sqlalchemy"),
            make_import("redis"),
        ];
        let profile = detector.detect_from_imports(&imports);

        let web = profile.by_category(FrameworkCategory::Web);
        assert_eq!(web.len(), 1);
        assert_eq!(web[0].name, "FastAPI");

        let db = profile.by_category(FrameworkCategory::Database);
        assert_eq!(db.len(), 1);
        assert_eq!(db[0].name, "SQLAlchemy");

        let cache = profile.by_category(FrameworkCategory::Cache);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache[0].name, "Redis");
    }
}
