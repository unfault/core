use serde::{Deserialize, Serialize};

use crate::types::context::Dimension;
use crate::types::context::{FrameworkGuess, Language};
use crate::types::context::{GitInfo, RepoLayout};

/// High-level description of what this review session is about.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewSessionMeta {
    /// Optional human-readable label (e.g. repo name, PR title).
    pub label: Option<String>,

    pub languages: Vec<Language>,
    pub framework_guesses: Vec<FrameworkGuess>,
    pub layout: RepoLayout,
    pub git: Option<GitInfo>,
    pub requested_dimensions: Vec<Dimension>,
}

impl Default for ReviewSessionMeta {
    fn default() -> Self {
        Self {
            label: None,
            languages: Vec::new(),
            framework_guesses: Vec::new(),
            layout: RepoLayout::default(),
            git: None,
            requested_dimensions: Vec::new(),
        }
    }
}
