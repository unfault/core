//! Profile types for rule-aware analysis.
//!
//! A Profile is a reusable description of a project "shape" (e.g. python_fastapi_backend).
//! It defines which rules apply and provides file selection hints for clients.

use serde::{Deserialize, Serialize};

use crate::types::context::{Dimension, Framework, Language};

/// Internal profile definition.
///
/// This is the server-side representation of a profile. It defines:
/// - Which rules should be active when this profile is selected
/// - File selection hints that clients should use to pick files
#[derive(Debug, Clone)]
pub struct Profile {
    /// Unique identifier (e.g. "python_fastapi_backend").
    pub id: String,

    /// Human-readable label (e.g. "Python FastAPI backend").
    pub label: String,

    /// Languages this profile applies to.
    pub languages: Vec<Language>,

    /// Frameworks this profile is associated with.
    pub frameworks: Vec<Framework>,

    /// Dimensions this profile focuses on.
    pub dimensions: Vec<Dimension>,

    /// IDs of rules that should be active when this profile is selected.
    pub rule_ids: Vec<String>,

    /// Static file selection hints that clients should use to pick files.
    pub file_hints: Vec<FileQueryHint>,
}

impl Profile {
    /// Create a new profile with the given id and label.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            languages: Vec::new(),
            frameworks: Vec::new(),
            dimensions: Vec::new(),
            rule_ids: Vec::new(),
            file_hints: Vec::new(),
        }
    }

    /// Add a language to this profile.
    pub fn with_language(mut self, language: Language) -> Self {
        self.languages.push(language);
        self
    }

    /// Add a framework to this profile.
    pub fn with_framework(mut self, framework: Framework) -> Self {
        self.frameworks.push(framework);
        self
    }

    /// Add a dimension to this profile.
    pub fn with_dimension(mut self, dimension: Dimension) -> Self {
        self.dimensions.push(dimension);
        self
    }

    /// Add a rule ID to this profile.
    pub fn with_rule(mut self, rule_id: impl Into<String>) -> Self {
        self.rule_ids.push(rule_id.into());
        self
    }

    /// Add multiple rule IDs to this profile.
    pub fn with_rules(mut self, rule_ids: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.rule_ids.extend(rule_ids.into_iter().map(|s| s.into()));
        self
    }

    /// Add a file hint to this profile.
    pub fn with_file_hint(mut self, hint: FileQueryHint) -> Self {
        self.file_hints.push(hint);
        self
    }
}

/// Hint describing which files the client should select for analysis.
///
/// These are static patterns used by clients to select files before upload.
/// The server returns these hints after resolving profiles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileQueryHint {
    /// Unique identifier for this hint (e.g. "fastapi_entrypoints").
    pub id: String,

    /// Human-readable label (e.g. "FastAPI entrypoints").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,

    /// Maximum number of files to select for this hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_files: Option<u32>,

    /// Maximum total bytes across all files for this hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_total_bytes: Option<u64>,

    /// Predicates that files must match (all must be satisfied).
    pub include: Vec<FilePredicate>,

    /// Predicates that exclude files (any match excludes the file).
    #[serde(default)]
    pub exclude: Vec<FilePredicate>,
}

impl FileQueryHint {
    /// Create a new file query hint with the given id.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: None,
            max_files: None,
            max_total_bytes: None,
            include: Vec::new(),
            exclude: Vec::new(),
        }
    }

    /// Set the label for this hint.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the maximum number of files.
    pub fn with_max_files(mut self, max: u32) -> Self {
        self.max_files = Some(max);
        self
    }

    /// Set the maximum total bytes.
    pub fn with_max_total_bytes(mut self, max: u64) -> Self {
        self.max_total_bytes = Some(max);
        self
    }

    /// Add an include predicate.
    pub fn include(mut self, predicate: FilePredicate) -> Self {
        self.include.push(predicate);
        self
    }

    /// Add an exclude predicate.
    pub fn exclude(mut self, predicate: FilePredicate) -> Self {
        self.exclude.push(predicate);
        self
    }
}

/// Predicate for filtering files.
///
/// Used in FileQueryHint to specify which files should be selected.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FilePredicate {
    /// Match files by language (e.g. "python", "go").
    Language { value: String },

    /// Match files by glob pattern (e.g. "**/main.py", "src/**/*.go").
    PathGlob { pattern: String },

    /// Match files under a specific directory (e.g. "src", "app").
    UnderDirectory { path: String },

    /// Match files containing any of the given strings.
    TextContainsAny { values: Vec<String> },

    /// Match files containing all of the given strings.
    TextContainsAll { values: Vec<String> },

    /// Match files where content matches a regex pattern.
    TextMatchesRegex { pattern: String },
}

impl FilePredicate {
    /// Create a language predicate.
    pub fn language(value: impl Into<String>) -> Self {
        Self::Language {
            value: value.into(),
        }
    }

    /// Create a path glob predicate.
    pub fn path_glob(pattern: impl Into<String>) -> Self {
        Self::PathGlob {
            pattern: pattern.into(),
        }
    }

    /// Create an under-directory predicate.
    pub fn under_directory(path: impl Into<String>) -> Self {
        Self::UnderDirectory { path: path.into() }
    }

    /// Create a text-contains-any predicate.
    pub fn text_contains_any(values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self::TextContainsAny {
            values: values.into_iter().map(|s| s.into()).collect(),
        }
    }

    /// Create a text-contains-all predicate.
    pub fn text_contains_all(values: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self::TextContainsAll {
            values: values.into_iter().map(|s| s.into()).collect(),
        }
    }

    /// Create a text-matches-regex predicate.
    pub fn text_matches_regex(pattern: impl Into<String>) -> Self {
        Self::TextMatchesRegex {
            pattern: pattern.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== Profile Tests ====================

    #[test]
    fn profile_new() {
        let profile = Profile::new("python_fastapi_backend", "Python FastAPI backend");
        assert_eq!(profile.id, "python_fastapi_backend");
        assert_eq!(profile.label, "Python FastAPI backend");
        assert!(profile.languages.is_empty());
        assert!(profile.frameworks.is_empty());
        assert!(profile.dimensions.is_empty());
        assert!(profile.rule_ids.is_empty());
        assert!(profile.file_hints.is_empty());
    }

    #[test]
    fn profile_with_language() {
        let profile = Profile::new("test", "Test").with_language(Language::Python);

        assert_eq!(profile.languages.len(), 1);
        assert!(matches!(profile.languages[0], Language::Python));
    }

    #[test]
    fn profile_with_framework() {
        let profile = Profile::new("test", "Test").with_framework(Framework::FastAPI);

        assert_eq!(profile.frameworks.len(), 1);
        assert!(matches!(profile.frameworks[0], Framework::FastAPI));
    }

    #[test]
    fn profile_with_dimension() {
        let profile = Profile::new("test", "Test").with_dimension(Dimension::Stability);

        assert_eq!(profile.dimensions.len(), 1);
        assert_eq!(profile.dimensions[0], Dimension::Stability);
    }

    #[test]
    fn profile_with_rule() {
        let profile = Profile::new("test", "Test").with_rule("fastapi.missing_cors");

        assert_eq!(profile.rule_ids.len(), 1);
        assert_eq!(profile.rule_ids[0], "fastapi.missing_cors");
    }

    #[test]
    fn profile_with_rules() {
        let profile = Profile::new("test", "Test").with_rules(["rule1", "rule2", "rule3"]);

        assert_eq!(profile.rule_ids.len(), 3);
    }

    #[test]
    fn profile_with_file_hint() {
        let hint = FileQueryHint::new("test_hint");
        let profile = Profile::new("test", "Test").with_file_hint(hint);

        assert_eq!(profile.file_hints.len(), 1);
        assert_eq!(profile.file_hints[0].id, "test_hint");
    }

    #[test]
    fn profile_chained_builders() {
        let profile = Profile::new("python_fastapi_backend", "Python FastAPI backend")
            .with_language(Language::Python)
            .with_framework(Framework::FastAPI)
            .with_dimension(Dimension::Stability)
            .with_dimension(Dimension::Correctness)
            .with_rules(["fastapi.missing_cors", "python.http.missing_timeout"])
            .with_file_hint(FileQueryHint::new("entrypoints"));

        assert_eq!(profile.languages.len(), 1);
        assert_eq!(profile.frameworks.len(), 1);
        assert_eq!(profile.dimensions.len(), 2);
        assert_eq!(profile.rule_ids.len(), 2);
        assert_eq!(profile.file_hints.len(), 1);
    }

    // ==================== FileQueryHint Tests ====================

    #[test]
    fn file_query_hint_new() {
        let hint = FileQueryHint::new("test_hint");
        assert_eq!(hint.id, "test_hint");
        assert!(hint.label.is_none());
        assert!(hint.max_files.is_none());
        assert!(hint.max_total_bytes.is_none());
        assert!(hint.include.is_empty());
        assert!(hint.exclude.is_empty());
    }

    #[test]
    fn file_query_hint_with_label() {
        let hint = FileQueryHint::new("test").with_label("Test Hint");

        assert_eq!(hint.label, Some("Test Hint".to_string()));
    }

    #[test]
    fn file_query_hint_with_max_files() {
        let hint = FileQueryHint::new("test").with_max_files(10);

        assert_eq!(hint.max_files, Some(10));
    }

    #[test]
    fn file_query_hint_with_max_total_bytes() {
        let hint = FileQueryHint::new("test").with_max_total_bytes(1024 * 1024);

        assert_eq!(hint.max_total_bytes, Some(1024 * 1024));
    }

    #[test]
    fn file_query_hint_include() {
        let hint = FileQueryHint::new("test").include(FilePredicate::language("python"));

        assert_eq!(hint.include.len(), 1);
    }

    #[test]
    fn file_query_hint_exclude() {
        let hint = FileQueryHint::new("test").exclude(FilePredicate::path_glob("**/test_*.py"));

        assert_eq!(hint.exclude.len(), 1);
    }

    #[test]
    fn file_query_hint_serialization() {
        let hint = FileQueryHint::new("fastapi_entrypoints")
            .with_label("FastAPI entrypoints")
            .with_max_files(8)
            .include(FilePredicate::path_glob("**/main.py"))
            .include(FilePredicate::path_glob("**/app.py"));

        let json = serde_json::to_string(&hint).unwrap();
        assert!(json.contains("fastapi_entrypoints"));
        assert!(json.contains("FastAPI entrypoints"));
        assert!(json.contains("path_glob"));
    }

    #[test]
    fn file_query_hint_deserialization() {
        let json = r#"{
            "id": "test",
            "label": "Test",
            "max_files": 10,
            "include": [
                {"kind": "language", "value": "python"}
            ],
            "exclude": []
        }"#;

        let hint: FileQueryHint = serde_json::from_str(json).unwrap();
        assert_eq!(hint.id, "test");
        assert_eq!(hint.max_files, Some(10));
        assert_eq!(hint.include.len(), 1);
    }

    // ==================== FilePredicate Tests ====================

    #[test]
    fn file_predicate_language() {
        let pred = FilePredicate::language("python");
        match pred {
            FilePredicate::Language { value } => assert_eq!(value, "python"),
            _ => panic!("Expected Language predicate"),
        }
    }

    #[test]
    fn file_predicate_path_glob() {
        let pred = FilePredicate::path_glob("**/main.py");
        match pred {
            FilePredicate::PathGlob { pattern } => assert_eq!(pattern, "**/main.py"),
            _ => panic!("Expected PathGlob predicate"),
        }
    }

    #[test]
    fn file_predicate_under_directory() {
        let pred = FilePredicate::under_directory("src");
        match pred {
            FilePredicate::UnderDirectory { path } => assert_eq!(path, "src"),
            _ => panic!("Expected UnderDirectory predicate"),
        }
    }

    #[test]
    fn file_predicate_text_contains_any() {
        let pred = FilePredicate::text_contains_any(["requests.", "httpx."]);
        match pred {
            FilePredicate::TextContainsAny { values } => {
                assert_eq!(values.len(), 2);
                assert!(values.contains(&"requests.".to_string()));
                assert!(values.contains(&"httpx.".to_string()));
            }
            _ => panic!("Expected TextContainsAny predicate"),
        }
    }

    #[test]
    fn file_predicate_text_contains_all() {
        let pred = FilePredicate::text_contains_all(["import", "fastapi"]);
        match pred {
            FilePredicate::TextContainsAll { values } => {
                assert_eq!(values.len(), 2);
            }
            _ => panic!("Expected TextContainsAll predicate"),
        }
    }

    #[test]
    fn file_predicate_text_matches_regex() {
        let pred = FilePredicate::text_matches_regex(r"def\s+\w+\s*\(");
        match pred {
            FilePredicate::TextMatchesRegex { pattern } => {
                assert!(pattern.contains("def"));
            }
            _ => panic!("Expected TextMatchesRegex predicate"),
        }
    }

    #[test]
    fn file_predicate_serialization() {
        let pred = FilePredicate::language("python");
        let json = serde_json::to_string(&pred).unwrap();
        assert!(json.contains("language"));
        assert!(json.contains("python"));
    }

    #[test]
    fn file_predicate_deserialization() {
        let json = r#"{"kind": "path_glob", "pattern": "**/main.py"}"#;
        let pred: FilePredicate = serde_json::from_str(json).unwrap();
        match pred {
            FilePredicate::PathGlob { pattern } => assert_eq!(pattern, "**/main.py"),
            _ => panic!("Expected PathGlob predicate"),
        }
    }

    #[test]
    fn file_predicate_all_variants_serialize() {
        let predicates = vec![
            FilePredicate::language("python"),
            FilePredicate::path_glob("*.py"),
            FilePredicate::under_directory("src"),
            FilePredicate::text_contains_any(["test"]),
            FilePredicate::text_contains_all(["test"]),
            FilePredicate::text_matches_regex(".*"),
        ];

        for pred in predicates {
            let json = serde_json::to_string(&pred).unwrap();
            let _: FilePredicate = serde_json::from_str(&json).unwrap();
        }
    }
}
