//! Workspace-related types for session/profile-based analysis.
//!
//! A Workspace represents a project/repo snapshot as seen by the client (CLI, LSP).
//! It carries metadata about the project structure and advertised profiles.

use serde::{Deserialize, Serialize};

/// What the client knows about the project.
///
/// This is sent by the client when creating a new session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceDescriptor {
    /// Human-readable label (e.g. directory name, repo name).
    pub label: String,

    /// Profiles the client thinks apply to this project.
    /// Ordered by confidence (highest first).
    pub profiles: Vec<AdvertisedProfile>,

    /// Meta files from the project (e.g. pyproject.toml, package.json).
    #[serde(default)]
    pub meta_files: Vec<MetaFile>,
}

impl WorkspaceDescriptor {
    /// Create a new workspace descriptor with just a label.
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            profiles: Vec::new(),
            meta_files: Vec::new(),
        }
    }

    /// Add an advertised profile.
    pub fn with_profile(mut self, profile: AdvertisedProfile) -> Self {
        self.profiles.push(profile);
        self
    }

    /// Add a meta file.
    pub fn with_meta_file(mut self, meta_file: MetaFile) -> Self {
        self.meta_files.push(meta_file);
        self
    }
}

/// A profile advertised by the client.
///
/// The client detects what kind of project this is (e.g. "python_fastapi_backend")
/// and sends this information to the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvertisedProfile {
    /// Profile identifier (e.g. "python_fastapi_backend", "go_gin_service").
    pub id: String,

    /// Confidence score [0.0, 1.0] indicating how sure the client is.
    pub confidence: f32,
}

impl AdvertisedProfile {
    /// Create a new advertised profile.
    pub fn new(id: impl Into<String>, confidence: f32) -> Self {
        Self {
            id: id.into(),
            confidence: confidence.clamp(0.0, 1.0),
        }
    }
}

/// A meta file from the project.
///
/// These are files like pyproject.toml, package.json, go.mod that help
/// identify the project type and dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaFile {
    /// Path relative to workspace root.
    pub path: String,

    /// Language/format of the file (e.g. "toml", "json", "yaml").
    pub language: String,

    /// Kind of meta file (e.g. "pyproject", "package_json", "go_mod").
    pub kind: MetaFileKind,

    /// File contents.
    pub contents: String,
}

impl MetaFile {
    /// Create a new meta file.
    pub fn new(
        path: impl Into<String>,
        language: impl Into<String>,
        kind: MetaFileKind,
        contents: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            language: language.into(),
            kind,
            contents: contents.into(),
        }
    }
}

/// Known kinds of meta files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetaFileKind {
    /// Python pyproject.toml
    Pyproject,
    /// Python requirements.txt
    RequirementsTxt,
    /// Python setup.py
    SetupPy,
    /// Node.js package.json
    PackageJson,
    /// Go go.mod
    GoMod,
    /// Rust Cargo.toml
    CargoToml,
    /// Java pom.xml
    PomXml,
    /// Java build.gradle
    BuildGradle,
    /// Generic/unknown
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== WorkspaceDescriptor Tests ====================

    #[test]
    fn workspace_descriptor_new() {
        let ws = WorkspaceDescriptor::new("my-project");
        assert_eq!(ws.label, "my-project");
        assert!(ws.profiles.is_empty());
        assert!(ws.meta_files.is_empty());
    }

    #[test]
    fn workspace_descriptor_with_profile() {
        let ws = WorkspaceDescriptor::new("my-project")
            .with_profile(AdvertisedProfile::new("python_fastapi_backend", 0.9));

        assert_eq!(ws.profiles.len(), 1);
        assert_eq!(ws.profiles[0].id, "python_fastapi_backend");
    }

    #[test]
    fn workspace_descriptor_with_meta_file() {
        let ws = WorkspaceDescriptor::new("my-project").with_meta_file(MetaFile::new(
            "pyproject.toml",
            "toml",
            MetaFileKind::Pyproject,
            "[project]\nname = \"test\"",
        ));

        assert_eq!(ws.meta_files.len(), 1);
        assert_eq!(ws.meta_files[0].path, "pyproject.toml");
    }

    #[test]
    fn workspace_descriptor_chained_builders() {
        let ws = WorkspaceDescriptor::new("my-project")
            .with_profile(AdvertisedProfile::new("python_fastapi_backend", 0.9))
            .with_profile(AdvertisedProfile::new("python_generic_backend", 0.6))
            .with_meta_file(MetaFile::new(
                "pyproject.toml",
                "toml",
                MetaFileKind::Pyproject,
                "",
            ));

        assert_eq!(ws.profiles.len(), 2);
        assert_eq!(ws.meta_files.len(), 1);
    }

    #[test]
    fn workspace_descriptor_serialization() {
        let ws = WorkspaceDescriptor::new("test")
            .with_profile(AdvertisedProfile::new("python_fastapi_backend", 0.9));

        let json = serde_json::to_string(&ws).unwrap();
        assert!(json.contains("test"));
        assert!(json.contains("python_fastapi_backend"));
    }

    #[test]
    fn workspace_descriptor_deserialization() {
        let json = r#"{
            "label": "my-project",
            "profiles": [
                {"id": "python_fastapi_backend", "confidence": 0.9}
            ],
            "meta_files": []
        }"#;

        let ws: WorkspaceDescriptor = serde_json::from_str(json).unwrap();
        assert_eq!(ws.label, "my-project");
        assert_eq!(ws.profiles.len(), 1);
    }

    // ==================== AdvertisedProfile Tests ====================

    #[test]
    fn advertised_profile_new() {
        let profile = AdvertisedProfile::new("python_fastapi_backend", 0.85);
        assert_eq!(profile.id, "python_fastapi_backend");
        assert!((profile.confidence - 0.85).abs() < f32::EPSILON);
    }

    #[test]
    fn advertised_profile_clamps_confidence_high() {
        let profile = AdvertisedProfile::new("test", 1.5);
        assert!((profile.confidence - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn advertised_profile_clamps_confidence_low() {
        let profile = AdvertisedProfile::new("test", -0.5);
        assert!((profile.confidence - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn advertised_profile_serialization() {
        let profile = AdvertisedProfile::new("python_fastapi_backend", 0.9);
        let json = serde_json::to_string(&profile).unwrap();
        assert!(json.contains("python_fastapi_backend"));
        assert!(json.contains("0.9"));
    }

    // ==================== MetaFile Tests ====================

    #[test]
    fn meta_file_new() {
        let mf = MetaFile::new(
            "pyproject.toml",
            "toml",
            MetaFileKind::Pyproject,
            "[project]\nname = \"test\"",
        );

        assert_eq!(mf.path, "pyproject.toml");
        assert_eq!(mf.language, "toml");
        assert_eq!(mf.kind, MetaFileKind::Pyproject);
        assert!(mf.contents.contains("[project]"));
    }

    #[test]
    fn meta_file_serialization() {
        let mf = MetaFile::new("package.json", "json", MetaFileKind::PackageJson, "{}");

        let json = serde_json::to_string(&mf).unwrap();
        assert!(json.contains("package.json"));
        assert!(json.contains("package_json"));
    }

    // ==================== MetaFileKind Tests ====================

    #[test]
    fn meta_file_kind_serialization() {
        assert_eq!(
            serde_json::to_string(&MetaFileKind::Pyproject).unwrap(),
            "\"pyproject\""
        );
        assert_eq!(
            serde_json::to_string(&MetaFileKind::PackageJson).unwrap(),
            "\"package_json\""
        );
        assert_eq!(
            serde_json::to_string(&MetaFileKind::GoMod).unwrap(),
            "\"go_mod\""
        );
    }

    #[test]
    fn meta_file_kind_deserialization() {
        assert_eq!(
            serde_json::from_str::<MetaFileKind>("\"pyproject\"").unwrap(),
            MetaFileKind::Pyproject
        );
        assert_eq!(
            serde_json::from_str::<MetaFileKind>("\"cargo_toml\"").unwrap(),
            MetaFileKind::CargoToml
        );
    }
}
