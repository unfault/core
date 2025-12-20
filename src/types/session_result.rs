use serde::{Deserialize, Serialize};

use crate::types::dependency::RuntimeDependency;
use crate::types::finding::Finding;
use crate::types::meta::ReviewSessionMeta;

/// Result of a full review session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewSessionResult {
    pub meta: ReviewSessionMeta,
    pub contexts: Vec<ContextResult>,
    /// Runtime dependencies extracted from all analyzed files.
    /// These represent external service connections detected in the code.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_dependencies: Vec<RuntimeDependency>,
}

/// Result of analyzing a single context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextResult {
    pub context_id: String,
    pub label: String,
    pub findings: Vec<Finding>,
}

impl Default for ReviewSessionResult {
    fn default() -> Self {
        Self {
            meta: ReviewSessionMeta::default(),
            contexts: Vec::new(),
            runtime_dependencies: Vec::new(),
        }
    }
}
