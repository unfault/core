use serde::{Deserialize, Serialize};

use crate::types::context::Dimension;

/// Core finding type produced by the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub rule_id: String,
    pub kind: FindingKind,
    pub title: String,
    pub description: String,
    pub severity: Severity,
    pub confidence: f32,
    pub dimension: Dimension,

    /// File path where this finding was detected
    pub file_path: String,

    /// Line number (1-based) where the finding starts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,

    /// Column number (1-based) where the finding starts
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,

    /// Line number (1-based) where the finding ends
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,

    /// Column number (1-based) where the finding ends
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_column: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FindingKind {
    BehaviorThreat,
    PerformanceSmell,
    StabilityRisk,
    AntiPattern,
    ResourceLeak,
    ReliabilityRisk,
    SecurityVulnerability,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Severity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}
