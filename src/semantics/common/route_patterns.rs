//! Common abstractions for HTTP route patterns across languages.
//!
//! This module provides types for representing HTTP routes from various web frameworks,
//! enabling cross-language route analysis and embedding generation.

use serde::{Deserialize, Serialize};

use super::CommonLocation;
use crate::parse::ast::FileId;

/// HTTP route pattern for embeddings and analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutePattern {
    /// The HTTP method (GET, POST, PUT, DELETE, PATCH, etc.)
    pub method: String,
    /// The route path pattern (e.g., "/users/:id", "/api/v1/items/{item_id}")
    pub path: String,
    /// The framework that registered this route
    pub framework: RouteFramework,
    /// Handler function name if detectable
    pub handler_name: Option<String>,
    /// Handler file path
    pub handler_file: String,
    /// Whether this route has authentication required
    pub has_auth: bool,
    /// Whether this route has validation
    pub has_validation: bool,
    /// Route summary/description
    pub summary: Option<String>,
    /// Route description
    pub description: Option<String>,
    /// Tags or labels attached to this route
    pub tags: Vec<String>,
    /// Location in source file
    pub location: CommonLocation,
    /// Start byte offset
    pub start_byte: usize,
    /// End byte offset
    pub end_byte: usize,
}

impl RoutePattern {
    /// Create a new route pattern
    pub fn new(
        method: impl Into<String>,
        path: impl Into<String>,
        framework: RouteFramework,
    ) -> Self {
        Self {
            method: method.into(),
            path: path.into(),
            framework,
            handler_name: None,
            handler_file: String::new(),
            has_auth: false,
            has_validation: false,
            summary: None,
            description: None,
            tags: Vec::new(),
            location: CommonLocation {
                file_id: FileId(0),
                line: 1,
                column: 1,
                start_byte: 0,
                end_byte: 0,
            },
            start_byte: 0,
            end_byte: 0,
        }
    }

    /// Set the handler name and file
    pub fn with_handler(mut self, name: impl Into<String>, file: impl Into<String>) -> Self {
        self.handler_name = Some(name.into());
        self.handler_file = file.into();
        self
    }

    /// Mark as requiring auth
    pub fn with_auth(mut self, has_auth: bool) -> Self {
        self.has_auth = has_auth;
        self
    }

    /// Mark as having validation
    pub fn with_validation(mut self, has_validation: bool) -> Self {
        self.has_validation = has_validation;
        self
    }

    /// Set summary
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Set description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Add a tag
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Set location
    pub fn with_location(
        mut self,
        location: CommonLocation,
        start_byte: usize,
        end_byte: usize,
    ) -> Self {
        self.location = location;
        self.start_byte = start_byte;
        self.end_byte = end_byte;
        self
    }

    /// Check if this is a REST-like route (has path parameters)
    pub fn has_path_parameters(&self) -> bool {
        self.path.contains(':') || self.path.contains('{') || self.path.contains('<')
    }

    /// Extract path parameter names
    pub fn path_parameters(&self) -> Vec<String> {
        let mut params = Vec::new();

        // Pattern 1: /users/:id (Gin, Echo style)
        for part in self.path.split('/') {
            if part.starts_with(':') {
                params.push(part[1..].to_string());
            }
        }

        // Pattern 2: /users/{id} (Fastify, Fiber, Spring style)
        for part in self.path.split('/') {
            if part.starts_with('{') && part.ends_with('}') {
                params.push(part[1..part.len() - 1].to_string());
            }
        }

        params
    }

    /// Get a canonical representation for embeddings
    pub fn embedding_string(&self) -> String {
        let params = self.path_parameters();
        let param_str = if params.is_empty() {
            "static".to_string()
        } else {
            format!("params[{}]", params.join(","))
        };

        format!(
            "{} {} {} {}",
            self.method.to_uppercase(),
            self.path,
            param_str,
            self.handler_name
                .clone()
                .unwrap_or_else(|| "anonymous".to_string())
        )
    }
}

/// Supported web frameworks for route extraction
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RouteFramework {
    // Python
    Django,
    Flask,
    FastApi,
    Starlette,

    // Go
    Gin,
    Echo,
    Fiber,
    Chi,
    Mux,
    GorillaMux,
    HttpRouter,
    ChiRouter,

    // Rust
    Axum,
    Rocket,
    Warp,
    ActixWeb,
    Tide,
    Salvo,

    // TypeScript/JavaScript
    Express,
    Fastify,
    NestJS,
    Koa,
    Hono,

    // Java
    SpringBoot,
    Quarkus,
    Micronaut,

    // .NET
    AspNetCore,

    // Ruby
    Rails,
    Sinatra,

    // PHP
    Laravel,
    Symfony,
    Slim,

    // Generic
    HttpLibrary,   // net/http, http module, etc.
    Other(String), // Other/custom frameworks
}

impl RouteFramework {
    /// Get the framework name as a string
    pub fn name(&self) -> &str {
        match self {
            Self::Django => "Django",
            Self::Flask => "Flask",
            Self::FastApi => "FastAPI",
            Self::Starlette => "Starlette",
            Self::Gin => "Gin",
            Self::Echo => "Echo",
            Self::Fiber => "Fiber",
            Self::Chi => "Chi",
            Self::Mux => "net/http",
            Self::GorillaMux => "Gorilla Mux",
            Self::HttpRouter => "HttpRouter",
            Self::ChiRouter => "chi",
            Self::Axum => "Axum",
            Self::Rocket => "Rocket",
            Self::Warp => "Warp",
            Self::ActixWeb => "Actix Web",
            Self::Tide => "Tide",
            Self::Salvo => "Salvo",
            Self::Express => "Express",
            Self::Fastify => "Fastify",
            Self::NestJS => "NestJS",
            Self::Koa => "Koa",
            Self::Hono => "Hono",
            Self::SpringBoot => "Spring Boot",
            Self::Quarkus => "Quarkus",
            Self::Micronaut => "Micronaut",
            Self::AspNetCore => "ASP.NET Core",
            Self::Rails => "Rails",
            Self::Sinatra => "Sinatra",
            Self::Laravel => "Laravel",
            Self::Symfony => "Symfony",
            Self::Slim => "Slim",
            Self::HttpLibrary => "HTTP Library",
            Self::Other(s) => s,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_pattern_new() {
        let route = RoutePattern::new("GET", "/users/:id", RouteFramework::Gin);

        assert_eq!(route.method, "GET");
        assert_eq!(route.path, "/users/:id");
        assert_eq!(route.framework, RouteFramework::Gin);
    }

    #[test]
    fn route_with_handler() {
        let route = RoutePattern::new("POST", "/users", RouteFramework::Express)
            .with_handler("createUser", "users_controller.ts");

        assert_eq!(route.handler_name, Some("createUser".to_string()));
        assert_eq!(route.handler_file, "users_controller.ts");
    }

    #[test]
    fn route_has_path_parameters() {
        let gin_route = RoutePattern::new("GET", "/users/:id", RouteFramework::Gin);
        let fastify_route = RoutePattern::new("GET", "/items/{item_id}", RouteFramework::Fastify);
        let static_route = RoutePattern::new("GET", "/health", RouteFramework::Express);

        assert!(gin_route.has_path_parameters());
        assert!(fastify_route.has_path_parameters());
        assert!(!static_route.has_path_parameters());
    }

    #[test]
    fn route_embedding_string() {
        let route = RoutePattern::new("GET", "/users/:id", RouteFramework::Gin)
            .with_handler("getUser", "user_controller.rs");

        let embedding = route.embedding_string();
        assert!(embedding.contains("GET"));
        assert!(embedding.contains("/users/:id"));
        assert!(embedding.contains("params"));
    }
}
