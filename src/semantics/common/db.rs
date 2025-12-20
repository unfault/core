//! Common database operation abstractions for cross-language analysis.
//!
//! This module provides language-agnostic types for database operations,
//! enabling shared rule logic for timeouts, connection pools, N+1 queries, etc.

use serde::{Deserialize, Serialize};

use super::CommonLocation;

/// Database/ORM library classification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DbLibrary {
    // Python
    SqlAlchemy,
    DjangoOrm,
    TortoiseOrm,
    Peewee,
    Psycopg2,
    Asyncpg,

    // Go
    DatabaseSql,
    Gorm,
    Sqlx,
    Sqlc,

    // Rust
    Diesel,
    SeaOrm,
    SqlxRust,
    TokioPostgres,

    // TypeScript
    Prisma,
    TypeOrm,
    Knex,
    Sequelize,
    DrizzleOrm,

    // Java
    Jpa,
    Hibernate,
    Jdbc,
    Mybatis,
    Jooq,

    // Generic
    Other(String),
}

impl DbLibrary {
    pub fn as_str(&self) -> &str {
        match self {
            Self::SqlAlchemy => "SQLAlchemy",
            Self::DjangoOrm => "Django ORM",
            Self::TortoiseOrm => "Tortoise ORM",
            Self::Peewee => "Peewee",
            Self::Psycopg2 => "psycopg2",
            Self::Asyncpg => "asyncpg",
            Self::DatabaseSql => "database/sql",
            Self::Gorm => "GORM",
            Self::Sqlx => "sqlx",
            Self::Sqlc => "sqlc",
            Self::Diesel => "Diesel",
            Self::SeaOrm => "SeaORM",
            Self::SqlxRust => "sqlx",
            Self::TokioPostgres => "tokio-postgres",
            Self::Prisma => "Prisma",
            Self::TypeOrm => "TypeORM",
            Self::Knex => "Knex",
            Self::Sequelize => "Sequelize",
            Self::DrizzleOrm => "Drizzle ORM",
            Self::Jpa => "JPA",
            Self::Hibernate => "Hibernate",
            Self::Jdbc => "JDBC",
            Self::Mybatis => "MyBatis",
            Self::Jooq => "jOOQ",
            Self::Other(s) => s,
        }
    }

    /// Check if this is an ORM (vs raw SQL)
    pub fn is_orm(&self) -> bool {
        matches!(
            self,
            Self::SqlAlchemy
                | Self::DjangoOrm
                | Self::TortoiseOrm
                | Self::Peewee
                | Self::Gorm
                | Self::Diesel
                | Self::SeaOrm
                | Self::Prisma
                | Self::TypeOrm
                | Self::Sequelize
                | Self::DrizzleOrm
                | Self::Jpa
                | Self::Hibernate
        )
    }

    /// Check if this supports async operations
    pub fn supports_async(&self) -> bool {
        matches!(
            self,
            Self::TortoiseOrm
                | Self::Asyncpg
                | Self::Sqlx
                | Self::SeaOrm
                | Self::SqlxRust
                | Self::TokioPostgres
                | Self::Prisma
        )
    }
}

/// Type of database operation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DbOperationType {
    /// SELECT/read query
    Select,
    /// INSERT/create
    Insert,
    /// UPDATE
    Update,
    /// DELETE
    Delete,
    /// Connection creation/acquisition
    Connect,
    /// Transaction begin
    TransactionBegin,
    /// Transaction commit
    TransactionCommit,
    /// Transaction rollback
    TransactionRollback,
    /// Relationship/lazy loading access
    RelationshipAccess,
    /// Raw SQL execution
    RawSql,
    /// Unknown operation
    Unknown,
}

impl DbOperationType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Select => "SELECT",
            Self::Insert => "INSERT",
            Self::Update => "UPDATE",
            Self::Delete => "DELETE",
            Self::Connect => "CONNECT",
            Self::TransactionBegin => "BEGIN",
            Self::TransactionCommit => "COMMIT",
            Self::TransactionRollback => "ROLLBACK",
            Self::RelationshipAccess => "RELATIONSHIP",
            Self::RawSql => "RAW_SQL",
            Self::Unknown => "UNKNOWN",
        }
    }

    /// Check if this is a mutating operation
    pub fn is_mutating(&self) -> bool {
        matches!(self, Self::Insert | Self::Update | Self::Delete)
    }

    /// Check if this is a transaction control operation
    pub fn is_transaction_control(&self) -> bool {
        matches!(
            self,
            Self::TransactionBegin | Self::TransactionCommit | Self::TransactionRollback
        )
    }
}

/// Eager loading strategy detected
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EagerLoadingStrategy {
    /// JOIN-based eager loading (select_related, joinedload)
    Join(String),
    /// Subquery/IN-based eager loading (prefetch_related, selectinload)
    Subquery(String),
    /// Entity graph (JPA)
    EntityGraph(String),
    /// Other strategy
    Other(String),
}

/// A language-agnostic database operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbOperation {
    /// The database library being used
    pub library: DbLibrary,

    /// The type of operation
    pub operation_type: DbOperationType,

    /// Whether this operation has an explicit timeout
    pub has_timeout: bool,

    /// Timeout value in seconds (if determinable)
    pub timeout_value: Option<f64>,

    /// Whether this operation is inside a transaction
    pub in_transaction: bool,

    /// Whether eager loading is configured
    pub eager_loading: Option<EagerLoadingStrategy>,

    /// Whether this operation is inside a loop
    pub in_loop: bool,

    /// Whether this operation is inside a comprehension/map/etc
    pub in_iteration: bool,

    /// The model/table being operated on
    pub model_name: Option<String>,

    /// The relationship field being accessed (for lazy loading)
    pub relationship_field: Option<String>,

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

impl DbOperation {
    /// Check if this might cause N+1 queries
    pub fn is_potential_n_plus_one(&self) -> bool {
        // A query/relationship access inside a loop without eager loading
        (self.in_loop || self.in_iteration)
            && (self.operation_type == DbOperationType::Select
                || self.operation_type == DbOperationType::RelationshipAccess)
            && self.eager_loading.is_none()
    }

    /// Check if this needs timeout configuration
    pub fn needs_timeout(&self) -> bool {
        !self.has_timeout
            && matches!(
                self.operation_type,
                DbOperationType::Select
                    | DbOperationType::Insert
                    | DbOperationType::Update
                    | DbOperationType::Delete
                    | DbOperationType::Connect
                    | DbOperationType::RawSql
            )
    }

    /// Check if this mutating operation should be in a transaction
    pub fn needs_transaction(&self) -> bool {
        self.operation_type.is_mutating() && !self.in_transaction
    }

    /// Get suggested timeout based on library
    pub fn suggested_timeout(&self) -> f64 {
        match self.operation_type {
            DbOperationType::Connect => 10.0,
            _ => 30.0,
        }
    }

    /// Get the timeout configuration method for this library
    pub fn timeout_config_hint(&self) -> &'static str {
        match self.library {
            DbLibrary::SqlAlchemy => "connect_args={'connect_timeout': 30}",
            DbLibrary::DjangoOrm => "CONN_MAX_AGE in settings, or execute timeout",
            DbLibrary::Psycopg2 => "connect_timeout parameter",
            DbLibrary::Asyncpg => "timeout parameter in connection",
            DbLibrary::DatabaseSql => "context.WithTimeout() or db.SetConnMaxLifetime()",
            DbLibrary::Gorm => "db.WithContext(ctx) with context timeout",
            DbLibrary::Diesel => "connection_timeout in config",
            DbLibrary::Prisma => "timeout in schema.prisma or query options",
            DbLibrary::TypeOrm => "connectTimeoutMS in options",
            DbLibrary::Jpa | DbLibrary::Hibernate => "@QueryHints with timeout",
            DbLibrary::Jdbc => "setQueryTimeout() on Statement",
            _ => "timeout parameter",
        }
    }
}

/// Connection pool configuration status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionPoolConfig {
    /// Whether pool size is configured
    pub has_pool_size: bool,
    /// Pool size value
    pub pool_size: Option<u32>,
    /// Whether max overflow is configured
    pub has_max_overflow: bool,
    /// Max overflow value
    pub max_overflow: Option<u32>,
    /// Whether pool timeout is configured
    pub has_pool_timeout: bool,
    /// Pool timeout value
    pub pool_timeout: Option<f64>,
    /// Whether connection max lifetime is configured
    pub has_max_lifetime: bool,
    /// Connection max lifetime value
    pub max_lifetime: Option<f64>,
    /// Location of pool configuration
    pub location: Option<CommonLocation>,
}

impl ConnectionPoolConfig {
    /// Check if pool configuration is complete
    pub fn is_complete(&self) -> bool {
        self.has_pool_size && self.has_pool_timeout && self.has_max_lifetime
    }

    /// Get missing configuration items
    pub fn missing_configs(&self) -> Vec<&'static str> {
        let mut missing = Vec::new();
        if !self.has_pool_size {
            missing.push("pool_size");
        }
        if !self.has_max_overflow {
            missing.push("max_overflow");
        }
        if !self.has_pool_timeout {
            missing.push("pool_timeout");
        }
        if !self.has_max_lifetime {
            missing.push("pool_recycle/max_lifetime");
        }
        missing
    }
}

impl Default for ConnectionPoolConfig {
    fn default() -> Self {
        Self {
            has_pool_size: false,
            pool_size: None,
            has_max_overflow: false,
            max_overflow: None,
            has_pool_timeout: false,
            pool_timeout: None,
            has_max_lifetime: false,
            max_lifetime: None,
            location: None,
        }
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
    fn db_library_is_orm() {
        assert!(DbLibrary::SqlAlchemy.is_orm());
        assert!(DbLibrary::DjangoOrm.is_orm());
        assert!(DbLibrary::Gorm.is_orm());
        assert!(DbLibrary::Prisma.is_orm());
        assert!(!DbLibrary::Psycopg2.is_orm());
        assert!(!DbLibrary::Jdbc.is_orm());
    }

    #[test]
    fn db_operation_is_potential_n_plus_one() {
        let op = DbOperation {
            library: DbLibrary::SqlAlchemy,
            operation_type: DbOperationType::Select,
            has_timeout: false,
            timeout_value: None,
            in_transaction: false,
            eager_loading: None,
            in_loop: true,
            in_iteration: false,
            model_name: Some("User".into()),
            relationship_field: None,
            operation_text: "session.query(Post).filter_by(user_id=user.id)".into(),
            location: make_location(),
            enclosing_function: Some("get_posts".into()),
            start_byte: 100,
            end_byte: 150,
        };

        assert!(op.is_potential_n_plus_one());
    }

    #[test]
    fn db_operation_with_eager_loading_not_n_plus_one() {
        let op = DbOperation {
            library: DbLibrary::SqlAlchemy,
            operation_type: DbOperationType::Select,
            has_timeout: false,
            timeout_value: None,
            in_transaction: false,
            eager_loading: Some(EagerLoadingStrategy::Join("joinedload".into())),
            in_loop: true,
            in_iteration: false,
            model_name: Some("User".into()),
            relationship_field: None,
            operation_text: "session.query(User).options(joinedload(User.posts))".into(),
            location: make_location(),
            enclosing_function: Some("get_users".into()),
            start_byte: 100,
            end_byte: 150,
        };

        assert!(!op.is_potential_n_plus_one());
    }

    #[test]
    fn connection_pool_config_missing() {
        let config = ConnectionPoolConfig::default();
        let missing = config.missing_configs();
        assert!(missing.contains(&"pool_size"));
        assert!(missing.contains(&"pool_timeout"));
        assert!(!config.is_complete());
    }

    #[test]
    fn connection_pool_config_complete() {
        let config = ConnectionPoolConfig {
            has_pool_size: true,
            pool_size: Some(10),
            has_max_overflow: true,
            max_overflow: Some(5),
            has_pool_timeout: true,
            pool_timeout: Some(30.0),
            has_max_lifetime: true,
            max_lifetime: Some(3600.0),
            location: None,
        };
        assert!(config.is_complete());
        assert!(config.missing_configs().is_empty());
    }
}
