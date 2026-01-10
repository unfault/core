# Contributing to unfault-core

Thank you for your interest in contributing to unfault-core! This crate is the foundation of Unfault's code analysis capabilities.

## What is unfault-core?

This crate handles all client-side parsing and semantic extraction:

- **Parsing**: Tree-sitter grammars for Python, Go, Rust, TypeScript
- **Semantics**: Extract functions, imports, classes, call sites
- **Graph**: Build code graphs with import/call relationships
- **Framework detection**: Recognize FastAPI, Express, Gin, etc.

Changes here affect both the CLI and VS Code extension.

## Getting Started

### Prerequisites

- **Rust 1.70+**: Install via [rustup](https://rustup.rs/)
- **Git**: For version control

### Setup

```bash
git clone https://github.com/unfault/core.git
cd core
cargo build
cargo test
```

### Project Structure

```
src/
├── lib.rs              # Public API exports
├── error.rs            # Error types
├── parse/              # Tree-sitter parsing
│   ├── ast.rs          # AST types, FileId
│   ├── python.rs       # Python parser
│   ├── go.rs           # Go parser
│   ├── rust.rs         # Rust parser
│   └── typescript.rs   # TypeScript parser
├── semantics/          # Semantic analysis
│   ├── mod.rs          # SourceSemantics enum
│   ├── common/         # Shared types (FunctionInfo, ImportInfo)
│   ├── python/         # Python-specific extraction
│   ├── go/             # Go-specific extraction
│   ├── rust/           # Rust-specific extraction
│   └── typescript/     # TypeScript-specific extraction
├── graph/              # Code graph construction
│   └── mod.rs          # CodeGraph, GraphNode, GraphEdgeKind
└── types/              # Common types
    ├── context.rs      # SourceFile, Language, Dimension
    └── profile.rs      # Analysis profiles
```

## Development

### Building

```bash
cargo build          # Debug build
cargo build --release # Release build
```

### Testing

```bash
cargo test                    # All tests
cargo test python             # Python-related tests
cargo test semantics::python  # Specific module
cargo test -- --nocapture     # With output
```

### Formatting and Linting

```bash
cargo fmt             # Format code
cargo fmt --check     # Check formatting
cargo clippy -- -D warnings  # Lint
```

## Making Changes

### Adding Language Support

1. Add Tree-sitter grammar to `Cargo.toml`
2. Create parser in `src/parse/{language}.rs`
3. Create semantics extractor in `src/semantics/{language}/`
4. Add to `SourceSemantics` enum
5. Add tests with sample code

### Adding Framework Detection

1. Add detection logic in `src/semantics/{language}/frameworks/`
2. Update `FrameworkGuess` signals
3. Add framework-specific nodes/edges if needed
4. Add tests with sample framework code

### Improving Semantic Extraction

1. Identify what's missing (e.g., decorators, type hints)
2. Extend the semantic model in `src/semantics/{language}/model.rs`
3. Update extraction logic in `src/semantics/{language}/mod.rs`
4. Add tests covering the new extraction

## Code Guidelines

### Error Handling

Use `thiserror` for library errors:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("Failed to parse {language} file: {message}")]
    ParseFailed { language: String, message: String },
    
    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),
}
```

### Documentation

Document public APIs:

```rust
/// Parse a Python source file and extract its AST.
///
/// # Arguments
///
/// * `file_id` - Unique identifier for this file
/// * `source` - Source file containing path, language, and content
///
/// # Returns
///
/// Parsed AST or error if parsing fails.
///
/// # Example
///
/// ```rust
/// use unfault_core::parse::python::parse_python_file;
/// 
/// let parsed = parse_python_file(FileId(1), &source)?;
/// ```
pub fn parse_python_file(file_id: FileId, source: &SourceFile) -> Result<ParsedFile> {
    // ...
}
```

### Testing

Test both positive and negative cases:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_function_with_decorators() {
        let source = r#"
@decorator
def my_function():
    pass
"#;
        let sem = parse_and_extract(source);
        assert_eq!(sem.functions.len(), 1);
        assert!(sem.functions[0].decorators.contains(&"decorator".to_string()));
    }

    #[test]
    fn handles_empty_file() {
        let sem = parse_and_extract("");
        assert!(sem.functions.is_empty());
    }

    #[test]
    fn handles_syntax_error_gracefully() {
        let source = "def broken(";
        // Should not panic, may return partial results or error
        let result = parse_and_extract_result(source);
        // ...
    }
}
```

## Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(python): extract decorator arguments
fix(graph): handle circular imports correctly
docs(readme): add usage example
test(go): add interface extraction tests
```

## Pull Request Process

1. **Fork and branch**: Create a feature branch from `main`
2. **Make changes**: Follow code guidelines
3. **Test**: Run `cargo test` and `cargo clippy`
4. **Document**: Update rustdoc for public APIs
5. **Submit PR**: Clear description of changes

### PR Checklist

- [ ] Tests pass (`cargo test`)
- [ ] Code formatted (`cargo fmt`)
- [ ] No clippy warnings (`cargo clippy -- -D warnings`)
- [ ] Public APIs documented
- [ ] Commit messages follow conventions

## Questions?

- Open a [GitHub Discussion](https://github.com/unfault/core/discussions)
- Check existing [issues](https://github.com/unfault/core/issues)

Thank you for contributing!
