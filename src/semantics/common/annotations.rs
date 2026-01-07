//! Common abstractions for annotations, decorators, and metadata across languages.
//!
//! This module provides types for representing annotations like logging, retry,
//! feature flags, and other metadata that can be attached to functions/methods.

use serde::{Deserialize, Serialize};

use super::CommonLocation;
use crate::parse::ast::FileId;

/// Type of annotation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnnotationType {
    /// Logging annotation
    Logging,
    /// Retry annotation
    Retry,
    /// Feature flag annotation
    FeatureFlag,
    /// Rate limiting annotation
    RateLimit,
    /// Timeout annotation
    Timeout,
    /// Cache annotation
    Cache,
    /// Validation annotation
    Validation { library: String },
    /// Authentication/Authorization annotation
    Auth { library: String },
    /// Route annotation
    Route,
    /// Controller annotation
    Controller,
    /// Injectable/Dependency injection annotation
    Injectable,
    /// Custom decorator
    CustomDecorator,
    /// Interceptor annotation
    Interceptor,
    /// Other/custom annotation
    Other(String),
}

/// A language-agnostic annotation/decorator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    /// Name of the annotation (e.g., "log", "retry", "Get")
    pub name: String,
    /// The type of annotation
    pub annotation_type: AnnotationType,
    /// Parameters/arguments to the annotation
    pub parameters: Vec<String>,
    /// The function this annotation is attached to
    pub target_function: String,
    /// The file containing this annotation
    pub target_file: String,
    /// Location in source file
    pub location: CommonLocation,
    /// Name of enclosing function if different from target
    pub enclosing_function: Option<String>,
    /// Name of enclosing class if any
    pub enclosing_class: Option<String>,
    /// Start byte offset
    pub start_byte: usize,
    /// End byte offset
    pub end_byte: usize,
}

impl Annotation {
    /// Create a new annotation
    pub fn new(
        name: impl Into<String>,
        annotation_type: AnnotationType,
        target_function: impl Into<String>,
        target_file: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            annotation_type,
            parameters: Vec::new(),
            target_function: target_function.into(),
            target_file: target_file.into(),
            location: CommonLocation {
                file_id: FileId(0),
                line: 1,
                column: 1,
                start_byte: 0,
                end_byte: 0,
            },
            enclosing_function: None,
            enclosing_class: None,
            start_byte: 0,
            end_byte: 0,
        }
    }

    /// Add parameters
    pub fn with_parameters(mut self, parameters: Vec<String>) -> Self {
        self.parameters = parameters;
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

    /// Set enclosing function
    pub fn with_enclosing_function(mut self, func: impl Into<String>) -> Self {
        self.enclosing_function = Some(func.into());
        self
    }

    /// Set enclosing class
    pub fn with_enclosing_class(mut self, class: impl Into<String>) -> Self {
        self.enclosing_class = Some(class.into());
        self
    }
}

/// Detected annotations on a function/method
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FunctionAnnotations {
    /// All annotations detected on this item
    pub annotations: Vec<Annotation>,
    /// Whether this item has any logging annotation
    pub has_logging: bool,
    /// Whether this item has any retry annotation
    pub has_retry: bool,
    /// Whether this item has any feature flag annotation
    pub has_feature_flag: bool,
    /// Whether this item has any rate limit annotation
    pub has_rate_limit: bool,
    /// Whether this item has any cache annotation
    pub has_cache: bool,
    /// Whether this item has any auth annotation
    pub has_auth: bool,
}

impl FunctionAnnotations {
    /// Create empty annotations
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an annotation and update flags
    pub fn add(&mut self, annotation: Annotation) {
        self.annotations.push(annotation.clone());
        match annotation.annotation_type {
            AnnotationType::Logging => self.has_logging = true,
            AnnotationType::Retry => self.has_retry = true,
            AnnotationType::FeatureFlag => self.has_feature_flag = true,
            AnnotationType::RateLimit => self.has_rate_limit = true,
            AnnotationType::Cache => self.has_cache = true,
            AnnotationType::Auth { .. } => self.has_auth = true,
            _ => {}
        }
    }

    /// Check if any annotation matches a pattern
    pub fn has_annotation_matching(&self, pattern: &str) -> bool {
        self.annotations.iter().any(|a| a.name.contains(pattern))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annotation_new() {
        let ann = Annotation::new("retry", AnnotationType::Retry, "fetch_data", "utils.py");

        assert_eq!(ann.name, "retry");
        assert!(matches!(ann.annotation_type, AnnotationType::Retry));
        assert_eq!(ann.target_function, "fetch_data");
    }

    #[test]
    fn annotation_with_parameters() {
        let ann = Annotation::new("route", AnnotationType::Route, "get_users", "api.py")
            .with_parameters(vec!["path=/users".to_string(), "method=GET".to_string()]);

        assert_eq!(ann.parameters.len(), 2);
    }

    #[test]
    fn annotation_with_class() {
        let ann = Annotation::new(
            "Get",
            AnnotationType::Route,
            "getUser",
            "user_controller.ts",
        )
        .with_enclosing_class("UserController");

        assert_eq!(ann.enclosing_class, Some("UserController".to_string()));
    }

    #[test]
    fn function_annotations_flags() {
        let mut annotations = FunctionAnnotations::new();
        assert!(!annotations.has_logging);
        assert!(!annotations.has_retry);

        annotations.add(Annotation::new(
            "log",
            AnnotationType::Logging,
            "process",
            "handler.py",
        ));

        annotations.add(Annotation::new(
            "retry",
            AnnotationType::Retry,
            "fetch",
            "handler.py",
        ));

        assert!(annotations.has_logging);
        assert!(annotations.has_retry);
        assert_eq!(annotations.annotations.len(), 2);
    }
}
