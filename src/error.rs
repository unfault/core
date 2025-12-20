use thiserror::Error;

/// Top-level error type exposed by the engine.
///
/// This is what bubbles out to API / CLI / LSP callers.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("parsing error: {0}")]
    Parse(#[from] ParseError),

    #[error("semantic model error: {0}")]
    Semantic(#[from] SemanticError),

    #[error("graph error: {0}")]
    Graph(#[from] GraphError),

    #[error("rule evaluation error: {0}")]
    Rule(#[from] RuleError),

    #[error("session error: {0}")]
    Session(#[from] SessionError),

    #[error("channel closed")]
    ChannelClosed,

    #[error("session aborted by caller")]
    AbortedByCaller,

    /// "Catch-all" for unexpected internal failures.
    #[error("internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

/// Errors that occur while parsing individual files.
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("failed to parse {file_path}: {source}")]
    File {
        file_path: String,
        #[source]
        source: anyhow::Error,
    },
}

/// Errors building semantic models (symbol tables, HTTP call model, etc.).
#[derive(Debug, Error)]
pub enum SemanticError {
    #[error("failed to build semantic model for {file_path}: {reason}")]
    File { file_path: String, reason: String },
}

/// Errors building or querying the CodeGraph.
#[derive(Debug, Error)]
pub enum GraphError {
    #[error("inconsistent graph state: {0}")]
    Inconsistent(String),

    #[error("graph build failed: {0}")]
    Build(String),
}

/// Errors executing rules.
#[derive(Debug, Error)]
pub enum RuleError {
    #[error("rule {rule_id} failed: {source}")]
    RuleFailed {
        rule_id: String,
        #[source]
        source: anyhow::Error,
    },
}

/// Errors in the session orchestration layer.
#[derive(Debug, Error)]
pub enum SessionError {
    #[error("missing context: {0}")]
    MissingContext(String),

    #[error("invalid context state: {0}")]
    InvalidState(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== EngineError Tests ====================

    #[test]
    fn test_engine_error_config_display() {
        let err = EngineError::Config("invalid setting".to_string());
        assert_eq!(err.to_string(), "configuration error: invalid setting");
    }

    #[test]
    fn test_engine_error_config_debug() {
        let err = EngineError::Config("test".to_string());
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("Config"));
    }

    #[test]
    fn test_engine_error_channel_closed_display() {
        let err = EngineError::ChannelClosed;
        assert_eq!(err.to_string(), "channel closed");
    }

    #[test]
    fn test_engine_error_aborted_by_caller_display() {
        let err = EngineError::AbortedByCaller;
        assert_eq!(err.to_string(), "session aborted by caller");
    }

    #[test]
    fn test_engine_error_from_parse_error() {
        let parse_err = ParseError::File {
            file_path: "test.py".to_string(),
            source: anyhow::anyhow!("syntax error"),
        };
        let engine_err: EngineError = parse_err.into();
        assert!(engine_err.to_string().contains("parsing error"));
        assert!(engine_err.to_string().contains("test.py"));
    }

    #[test]
    fn test_engine_error_from_semantic_error() {
        let sem_err = SemanticError::File {
            file_path: "app.py".to_string(),
            reason: "unknown symbol".to_string(),
        };
        let engine_err: EngineError = sem_err.into();
        assert!(engine_err.to_string().contains("semantic model error"));
    }

    #[test]
    fn test_engine_error_from_graph_error() {
        let graph_err = GraphError::Inconsistent("cycle detected".to_string());
        let engine_err: EngineError = graph_err.into();
        assert!(engine_err.to_string().contains("graph error"));
    }

    #[test]
    fn test_engine_error_from_rule_error() {
        let rule_err = RuleError::RuleFailed {
            rule_id: "test-rule".to_string(),
            source: anyhow::anyhow!("rule crashed"),
        };
        let engine_err: EngineError = rule_err.into();
        assert!(engine_err.to_string().contains("rule evaluation error"));
    }

    #[test]
    fn test_engine_error_from_session_error() {
        let session_err = SessionError::MissingContext("ctx-1".to_string());
        let engine_err: EngineError = session_err.into();
        assert!(engine_err.to_string().contains("session error"));
    }

    #[test]
    fn test_engine_error_from_anyhow() {
        let anyhow_err = anyhow::anyhow!("unexpected failure");
        let engine_err: EngineError = anyhow_err.into();
        assert!(engine_err.to_string().contains("internal error"));
        assert!(engine_err.to_string().contains("unexpected failure"));
    }

    // ==================== ParseError Tests ====================

    #[test]
    fn test_parse_error_file_display() {
        let err = ParseError::File {
            file_path: "main.py".to_string(),
            source: anyhow::anyhow!("invalid syntax at line 10"),
        };
        let msg = err.to_string();
        assert!(msg.contains("failed to parse main.py"));
        assert!(msg.contains("invalid syntax at line 10"));
    }

    #[test]
    fn test_parse_error_file_debug() {
        let err = ParseError::File {
            file_path: "test.rs".to_string(),
            source: anyhow::anyhow!("parse failure"),
        };
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("File"));
        assert!(debug_str.contains("test.rs"));
    }

    // ==================== SemanticError Tests ====================

    #[test]
    fn test_semantic_error_file_display() {
        let err = SemanticError::File {
            file_path: "routes.py".to_string(),
            reason: "undefined function call".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("failed to build semantic model for routes.py"));
        assert!(msg.contains("undefined function call"));
    }

    #[test]
    fn test_semantic_error_file_debug() {
        let err = SemanticError::File {
            file_path: "api.py".to_string(),
            reason: "type mismatch".to_string(),
        };
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("File"));
        assert!(debug_str.contains("api.py"));
        assert!(debug_str.contains("type mismatch"));
    }

    // ==================== GraphError Tests ====================

    #[test]
    fn test_graph_error_inconsistent_display() {
        let err = GraphError::Inconsistent("node missing".to_string());
        assert_eq!(err.to_string(), "inconsistent graph state: node missing");
    }

    #[test]
    fn test_graph_error_build_display() {
        let err = GraphError::Build("out of memory".to_string());
        assert_eq!(err.to_string(), "graph build failed: out of memory");
    }

    #[test]
    fn test_graph_error_debug() {
        let err = GraphError::Inconsistent("test".to_string());
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("Inconsistent"));
    }

    // ==================== RuleError Tests ====================

    #[test]
    fn test_rule_error_failed_display() {
        let err = RuleError::RuleFailed {
            rule_id: "fastapi-cors".to_string(),
            source: anyhow::anyhow!("missing middleware"),
        };
        let msg = err.to_string();
        assert!(msg.contains("rule fastapi-cors failed"));
        assert!(msg.contains("missing middleware"));
    }

    #[test]
    fn test_rule_error_debug() {
        let err = RuleError::RuleFailed {
            rule_id: "http-timeout".to_string(),
            source: anyhow::anyhow!("timeout not set"),
        };
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("RuleFailed"));
        assert!(debug_str.contains("http-timeout"));
    }

    // ==================== SessionError Tests ====================

    #[test]
    fn test_session_error_missing_context_display() {
        let err = SessionError::MissingContext("context-abc".to_string());
        assert_eq!(err.to_string(), "missing context: context-abc");
    }

    #[test]
    fn test_session_error_invalid_state_display() {
        let err = SessionError::InvalidState("already finalized".to_string());
        assert_eq!(err.to_string(), "invalid context state: already finalized");
    }

    #[test]
    fn test_session_error_debug() {
        let err = SessionError::MissingContext("test".to_string());
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("MissingContext"));
    }

    // ==================== Error Chain Tests ====================

    #[test]
    fn test_error_source_chain_parse() {
        use std::error::Error;

        let parse_err = ParseError::File {
            file_path: "test.py".to_string(),
            source: anyhow::anyhow!("root cause"),
        };

        // ParseError should have a source
        assert!(parse_err.source().is_some());
    }

    #[test]
    fn test_error_source_chain_rule() {
        use std::error::Error;

        let rule_err = RuleError::RuleFailed {
            rule_id: "test".to_string(),
            source: anyhow::anyhow!("inner error"),
        };

        // RuleError should have a source
        assert!(rule_err.source().is_some());
    }
}
