use serde::{Deserialize, Serialize};

/// Supported programming languages.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Python,
    Rust,
    Go,
    Java,
    Typescript,
    Javascript,
}

/// Frameworks we can detect and reason about.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameworkGuess {
    pub name: Framework,
    pub strength: f32,
    pub signals: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Framework {
    Django,
    FastAPI,
    Flask,
    Express,
    VueJs,
    ReactQuery,
    ApolloClient,
    NextJs,
    Gin,
    Echo,
    Beego,
    Fiber,
    SpringBoot,
    Axum,
    ActixWeb,
    Rocket,
    Warp,
}

/// Dimensions of analysis (stability, performance, etc.).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum Dimension {
    Stability,
    Performance,
    Correctness,
    Scalability,
    Observability,
    Reliability,
    Security,
    /// Maintainability dimension for code complexity metrics.
    /// This dimension is opt-in and not included in default profiles.
    Maintainability,
}

/// Lightweight representation of the directory structure.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RepoLayout {
    pub src_dirs: Vec<String>,
    pub test_dirs: Vec<String>,
    pub other_dirs: Vec<String>,
    pub directories: Vec<DirectorySummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectorySummary {
    pub path: String,
    pub depth: u32,
    pub file_count: u32,
    pub dir_count: u32,
    pub languages: Vec<Language>,
}

/// Git-related information, optional.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitInfo {
    pub head: Option<String>,
    pub base: Option<String>,
    pub changed_files: Vec<String>,
}

/// A source file included in a context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: String,
    pub language: Language,
    pub content: String,
}

/// Input for one context in a review session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionContextInput {
    pub id: String,
    pub label: String,
    pub dimension: Dimension,
    pub files: Vec<SourceFile>,
}
