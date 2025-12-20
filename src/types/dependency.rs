//! Runtime dependency types for tracking external service connections.
//!
//! This module defines types for representing runtime dependencies detected
//! during code analysis, such as HTTP calls, database connections, Redis,
//! message queues, etc.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::parse::ast::FileId;

/// Protocol type for runtime dependencies.
///
/// Represents the communication protocol or service type.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencyProtocol {
    /// HTTP (unencrypted)
    Http,
    /// HTTPS (TLS encrypted)
    Https,
    /// PostgreSQL database
    Postgres,
    /// MySQL/MariaDB database
    Mysql,
    /// Redis cache/datastore
    Redis,
    /// MongoDB document database
    Mongodb,
    /// gRPC remote procedure calls
    Grpc,
    /// AMQP (RabbitMQ, etc.)
    Amqp,
    /// Apache Kafka
    Kafka,
    /// WebSocket connections
    Websocket,
    /// SQLite database
    Sqlite,
    /// Elasticsearch
    Elasticsearch,
    /// Memcached
    Memcached,
    /// Other/unknown protocol
    Other(String),
}

impl DependencyProtocol {
    /// Get a string representation of the protocol.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
            Self::Postgres => "postgres",
            Self::Mysql => "mysql",
            Self::Redis => "redis",
            Self::Mongodb => "mongodb",
            Self::Grpc => "grpc",
            Self::Amqp => "amqp",
            Self::Kafka => "kafka",
            Self::Websocket => "websocket",
            Self::Sqlite => "sqlite",
            Self::Elasticsearch => "elasticsearch",
            Self::Memcached => "memcached",
            Self::Other(s) => s,
        }
    }

    /// Detect protocol from a URI string.
    pub fn from_uri(uri: &str) -> Self {
        let uri_lower = uri.to_lowercase();

        // Check for explicit scheme
        if uri_lower.starts_with("https://") || uri_lower.starts_with("https:") {
            return Self::Https;
        }
        if uri_lower.starts_with("http://") || uri_lower.starts_with("http:") {
            return Self::Http;
        }
        if uri_lower.starts_with("postgres://") || uri_lower.starts_with("postgresql://") {
            return Self::Postgres;
        }
        if uri_lower.starts_with("mysql://") || uri_lower.starts_with("mariadb://") {
            return Self::Mysql;
        }
        if uri_lower.starts_with("redis://") || uri_lower.starts_with("rediss://") {
            return Self::Redis;
        }
        if uri_lower.starts_with("mongodb://") || uri_lower.starts_with("mongodb+srv://") {
            return Self::Mongodb;
        }
        if uri_lower.starts_with("grpc://") || uri_lower.starts_with("grpcs://") {
            return Self::Grpc;
        }
        if uri_lower.starts_with("amqp://") || uri_lower.starts_with("amqps://") {
            return Self::Amqp;
        }
        if uri_lower.starts_with("kafka://") {
            return Self::Kafka;
        }
        if uri_lower.starts_with("ws://") {
            return Self::Websocket;
        }
        if uri_lower.starts_with("wss://") {
            return Self::Websocket;
        }
        if uri_lower.starts_with("sqlite://") || uri_lower.ends_with(".db") {
            return Self::Sqlite;
        }
        if uri_lower.contains("elasticsearch") || uri_lower.contains(":9200") {
            return Self::Elasticsearch;
        }
        if uri_lower.contains(":11211") {
            return Self::Memcached;
        }

        // Fallback: try to detect from common patterns
        if uri_lower.contains(":5432") {
            return Self::Postgres;
        }
        if uri_lower.contains(":3306") {
            return Self::Mysql;
        }
        if uri_lower.contains(":6379") {
            return Self::Redis;
        }
        if uri_lower.contains(":27017") {
            return Self::Mongodb;
        }

        Self::Other("unknown".to_string())
    }
}

/// Type of code block containing the dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockType {
    /// Top-level function
    Function,
    /// Class/struct method
    Method,
    /// Class/struct body (not in a method)
    Class,
    /// Module/file level
    Module,
    /// Lambda/anonymous function
    Lambda,
    /// Closure
    Closure,
    /// Unknown context
    Unknown,
}

/// Source location for a dependency.
///
/// Tracks where in the codebase a dependency was detected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencySource {
    /// File path relative to workspace root.
    pub file_path: String,

    /// File ID for cross-referencing with other engine data.
    pub file_id: FileId,

    /// 1-based line number where the dependency is located.
    pub line: u32,

    /// 1-based column number where the dependency starts.
    pub column: u32,

    /// Name of the enclosing block (function, method, class name).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_name: Option<String>,

    /// Type of the enclosing block.
    pub block_type: BlockType,
}

/// A runtime dependency detected in code.
///
/// Represents a connection to an external service such as an HTTP API,
/// database, cache, or message queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeDependency {
    /// Unique identifier for this dependency instance.
    pub id: String,

    /// The protocol/type of dependency.
    pub protocol: DependencyProtocol,

    /// Raw URI as found in code (may contain ${VAR} placeholders).
    /// Examples: "${DATABASE_URL}", "os.getenv('REDIS_URL')", "https://api.example.com"
    pub raw_uri: String,

    /// Resolved URI after environment variable substitution (sanitized).
    /// Passwords and tokens are masked with "***".
    /// Example: "postgres://user:***@host:5432/db"
    /// None if resolution was not performed or failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_uri: Option<String>,

    /// Host:port extracted from resolved URI for grouping.
    /// Example: "api.example.com", "db.internal:5432"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_host: Option<String>,

    /// Whether the raw URI contains variables/environment references.
    pub uri_is_dynamic: bool,

    /// Source location in code where this dependency was detected.
    pub source: DependencySource,

    /// Additional metadata about the dependency.
    /// Common keys: "library", "method", "http_method", "table"
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, String>,
}

impl RuntimeDependency {
    /// Create a new RuntimeDependency with a generated ID.
    pub fn new(protocol: DependencyProtocol, raw_uri: String, source: DependencySource) -> Self {
        let id = generate_dependency_id();
        let uri_is_dynamic = is_dynamic_uri(&raw_uri);

        Self {
            id,
            protocol,
            raw_uri,
            resolved_uri: None,
            resolved_host: None,
            uri_is_dynamic,
            source,
            metadata: HashMap::new(),
        }
    }

    /// Add metadata to this dependency.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Set the resolved URI (should be sanitized).
    pub fn with_resolved(mut self, resolved_uri: String, resolved_host: Option<String>) -> Self {
        self.resolved_uri = Some(resolved_uri);
        self.resolved_host = resolved_host;
        self
    }
}

/// Check if a URI string contains dynamic references (environment variables).
pub fn is_dynamic_uri(uri: &str) -> bool {
    // Check for common patterns:
    // - ${VAR} or $VAR
    // - os.environ["VAR"] or os.getenv("VAR") (Python)
    // - process.env.VAR (JavaScript/TypeScript)
    // - os.Getenv("VAR") (Go)
    // - env::var("VAR") (Rust)
    uri.contains("${")
        || uri.contains("$ENV")
        || uri.contains("os.environ")
        || uri.contains("os.getenv")
        || uri.contains("os.Getenv")
        || uri.contains("process.env")
        || uri.contains("env::var")
        || uri.contains("env!")
        || uri.contains("std::env")
        || (uri.contains('$') && !uri.contains("://"))
}

/// Generate a unique ID for a dependency.
fn generate_dependency_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;

    let count = COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("dep_{:x}_{:x}", timestamp, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== DependencyProtocol Tests ====================

    #[test]
    fn test_protocol_as_str() {
        assert_eq!(DependencyProtocol::Http.as_str(), "http");
        assert_eq!(DependencyProtocol::Https.as_str(), "https");
        assert_eq!(DependencyProtocol::Postgres.as_str(), "postgres");
        assert_eq!(DependencyProtocol::Redis.as_str(), "redis");
        assert_eq!(
            DependencyProtocol::Other("custom".to_string()).as_str(),
            "custom"
        );
    }

    #[test]
    fn test_protocol_from_uri_http() {
        assert_eq!(
            DependencyProtocol::from_uri("http://example.com"),
            DependencyProtocol::Http
        );
        assert_eq!(
            DependencyProtocol::from_uri("HTTP://EXAMPLE.COM"),
            DependencyProtocol::Http
        );
    }

    #[test]
    fn test_protocol_from_uri_https() {
        assert_eq!(
            DependencyProtocol::from_uri("https://api.example.com"),
            DependencyProtocol::Https
        );
    }

    #[test]
    fn test_protocol_from_uri_postgres() {
        assert_eq!(
            DependencyProtocol::from_uri("postgres://user:pass@host:5432/db"),
            DependencyProtocol::Postgres
        );
        assert_eq!(
            DependencyProtocol::from_uri("postgresql://user@host/db"),
            DependencyProtocol::Postgres
        );
        // Port-based detection
        assert_eq!(
            DependencyProtocol::from_uri("host.internal:5432"),
            DependencyProtocol::Postgres
        );
    }

    #[test]
    fn test_protocol_from_uri_mysql() {
        assert_eq!(
            DependencyProtocol::from_uri("mysql://user:pass@host:3306/db"),
            DependencyProtocol::Mysql
        );
        assert_eq!(
            DependencyProtocol::from_uri("mariadb://user@host/db"),
            DependencyProtocol::Mysql
        );
    }

    #[test]
    fn test_protocol_from_uri_redis() {
        assert_eq!(
            DependencyProtocol::from_uri("redis://localhost:6379"),
            DependencyProtocol::Redis
        );
        assert_eq!(
            DependencyProtocol::from_uri("rediss://secure-host:6379"),
            DependencyProtocol::Redis
        );
    }

    #[test]
    fn test_protocol_from_uri_mongodb() {
        assert_eq!(
            DependencyProtocol::from_uri("mongodb://user:pass@host:27017/db"),
            DependencyProtocol::Mongodb
        );
        assert_eq!(
            DependencyProtocol::from_uri("mongodb+srv://cluster.example.com"),
            DependencyProtocol::Mongodb
        );
    }

    #[test]
    fn test_protocol_from_uri_grpc() {
        assert_eq!(
            DependencyProtocol::from_uri("grpc://service:50051"),
            DependencyProtocol::Grpc
        );
    }

    #[test]
    fn test_protocol_from_uri_amqp() {
        assert_eq!(
            DependencyProtocol::from_uri("amqp://rabbitmq:5672"),
            DependencyProtocol::Amqp
        );
    }

    #[test]
    fn test_protocol_from_uri_websocket() {
        assert_eq!(
            DependencyProtocol::from_uri("ws://localhost:8080"),
            DependencyProtocol::Websocket
        );
        assert_eq!(
            DependencyProtocol::from_uri("wss://secure.example.com"),
            DependencyProtocol::Websocket
        );
    }

    #[test]
    fn test_protocol_from_uri_sqlite() {
        assert_eq!(
            DependencyProtocol::from_uri("sqlite:///path/to/db.sqlite"),
            DependencyProtocol::Sqlite
        );
        assert_eq!(
            DependencyProtocol::from_uri("./data/app.db"),
            DependencyProtocol::Sqlite
        );
    }

    #[test]
    fn test_protocol_from_uri_unknown() {
        let result = DependencyProtocol::from_uri("custom://service");
        assert!(matches!(result, DependencyProtocol::Other(_)));
    }

    // ==================== is_dynamic_uri Tests ====================

    #[test]
    fn test_is_dynamic_uri_env_var_syntax() {
        assert!(is_dynamic_uri("${DATABASE_URL}"));
        assert!(is_dynamic_uri("postgres://${DB_HOST}:5432/mydb"));
        assert!(is_dynamic_uri("$DATABASE_URL"));
    }

    #[test]
    fn test_is_dynamic_uri_python() {
        assert!(is_dynamic_uri("os.environ['DATABASE_URL']"));
        assert!(is_dynamic_uri("os.getenv('REDIS_URL')"));
    }

    #[test]
    fn test_is_dynamic_uri_go() {
        assert!(is_dynamic_uri("os.Getenv(\"DATABASE_URL\")"));
    }

    #[test]
    fn test_is_dynamic_uri_javascript() {
        assert!(is_dynamic_uri("process.env.DATABASE_URL"));
    }

    #[test]
    fn test_is_dynamic_uri_rust() {
        assert!(is_dynamic_uri("env::var(\"DATABASE_URL\")"));
        assert!(is_dynamic_uri("std::env::var(\"REDIS_URL\")"));
    }

    #[test]
    fn test_is_dynamic_uri_literal() {
        assert!(!is_dynamic_uri("https://api.example.com"));
        assert!(!is_dynamic_uri("postgres://user:pass@host:5432/db"));
        assert!(!is_dynamic_uri("redis://localhost:6379"));
    }

    // ==================== RuntimeDependency Tests ====================

    #[test]
    fn test_runtime_dependency_new() {
        let source = DependencySource {
            file_path: "test.py".to_string(),
            file_id: FileId(1),
            line: 10,
            column: 5,
            block_name: Some("fetch_data".to_string()),
            block_type: BlockType::Function,
        };

        let dep = RuntimeDependency::new(
            DependencyProtocol::Https,
            "https://api.example.com".to_string(),
            source,
        );

        assert!(!dep.id.is_empty());
        assert_eq!(dep.protocol, DependencyProtocol::Https);
        assert_eq!(dep.raw_uri, "https://api.example.com");
        assert!(!dep.uri_is_dynamic);
        assert!(dep.resolved_uri.is_none());
    }

    #[test]
    fn test_runtime_dependency_dynamic_uri() {
        let source = DependencySource {
            file_path: "config.py".to_string(),
            file_id: FileId(1),
            line: 5,
            column: 1,
            block_name: None,
            block_type: BlockType::Module,
        };

        let dep = RuntimeDependency::new(
            DependencyProtocol::Postgres,
            "${DATABASE_URL}".to_string(),
            source,
        );

        assert!(dep.uri_is_dynamic);
    }

    #[test]
    fn test_runtime_dependency_with_metadata() {
        let source = DependencySource {
            file_path: "client.py".to_string(),
            file_id: FileId(1),
            line: 15,
            column: 8,
            block_name: Some("make_request".to_string()),
            block_type: BlockType::Function,
        };

        let dep = RuntimeDependency::new(
            DependencyProtocol::Https,
            "https://api.stripe.com/v1".to_string(),
            source,
        )
        .with_metadata("library", "requests")
        .with_metadata("http_method", "POST");

        assert_eq!(dep.metadata.get("library"), Some(&"requests".to_string()));
        assert_eq!(dep.metadata.get("http_method"), Some(&"POST".to_string()));
    }

    #[test]
    fn test_runtime_dependency_with_resolved() {
        let source = DependencySource {
            file_path: "db.py".to_string(),
            file_id: FileId(1),
            line: 3,
            column: 1,
            block_name: None,
            block_type: BlockType::Module,
        };

        let dep = RuntimeDependency::new(
            DependencyProtocol::Postgres,
            "${DATABASE_URL}".to_string(),
            source,
        )
        .with_resolved(
            "postgres://user:***@db.example.com:5432/mydb".to_string(),
            Some("db.example.com:5432".to_string()),
        );

        assert!(dep.resolved_uri.is_some());
        assert_eq!(
            dep.resolved_uri.unwrap(),
            "postgres://user:***@db.example.com:5432/mydb"
        );
        assert_eq!(dep.resolved_host, Some("db.example.com:5432".to_string()));
    }

    #[test]
    fn test_dependency_id_uniqueness() {
        let source = DependencySource {
            file_path: "test.py".to_string(),
            file_id: FileId(1),
            line: 1,
            column: 1,
            block_name: None,
            block_type: BlockType::Module,
        };

        let dep1 = RuntimeDependency::new(
            DependencyProtocol::Http,
            "http://example.com".to_string(),
            source.clone(),
        );

        let dep2 = RuntimeDependency::new(
            DependencyProtocol::Http,
            "http://example.com".to_string(),
            source,
        );

        // IDs should be different even for identical dependencies
        assert_ne!(dep1.id, dep2.id);
    }

    // ==================== Serialization Tests ====================

    #[test]
    fn test_dependency_serialization() {
        let source = DependencySource {
            file_path: "api.py".to_string(),
            file_id: FileId(42),
            line: 25,
            column: 12,
            block_name: Some("call_api".to_string()),
            block_type: BlockType::Function,
        };

        let dep = RuntimeDependency::new(
            DependencyProtocol::Https,
            "https://api.example.com/v1".to_string(),
            source,
        )
        .with_metadata("library", "httpx");

        let json = serde_json::to_string(&dep).expect("serialization should succeed");
        assert!(json.contains("https"));
        assert!(json.contains("api.example.com"));
        assert!(json.contains("httpx"));

        // Deserialize and verify
        let deserialized: RuntimeDependency =
            serde_json::from_str(&json).expect("deserialization should succeed");
        assert_eq!(deserialized.protocol, DependencyProtocol::Https);
        assert_eq!(deserialized.raw_uri, "https://api.example.com/v1");
    }

    #[test]
    fn test_protocol_serialization() {
        let protocol = DependencyProtocol::Postgres;
        let json = serde_json::to_string(&protocol).expect("serialization should succeed");
        assert_eq!(json, "\"postgres\"");

        let deserialized: DependencyProtocol =
            serde_json::from_str(&json).expect("deserialization should succeed");
        assert_eq!(deserialized, DependencyProtocol::Postgres);
    }
}
