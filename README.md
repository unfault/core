# unfault-core

Core parsing, semantics extraction, and graph building for unfault.

## Overview

This crate provides language-agnostic code analysis capabilities:

- **Parsing**: Tree-sitter based parsing for Python, Go, Rust, TypeScript, etc.
- **Semantics**: Extract semantic information (functions, imports, classes, call sites, etc.)
- **Graph**: Build code dependency graphs with import/call relationships
- **Types**: Common types for language identification, source files, and profiles

## Features

- **Multi-language support**: Python, Go, Rust, TypeScript
- **Framework detection**: FastAPI, Express, Gin, and more
- **Graph construction**: Build import graphs, call graphs, and dependency graphs

## Usage

```rust
use unfault_core::parse::python::parse_python_file;
use unfault_core::semantics::python::model::PyFileSemantics;
use unfault_core::graph::build_code_graph;
use unfault_core::types::context::{SourceFile, Language};
use unfault_core::parse::ast::FileId;

// Parse a Python file
let source = SourceFile {
    path: "example.py".to_string(),
    language: Language::Python,
    content: r#"
import os
from typing import List

def hello(name: str) -> str:
    return f"Hello, {name}!"
"#.to_string(),
};

let parsed = parse_python_file(FileId(1), &source).unwrap();
let semantics = PyFileSemantics::from_parsed(&parsed);

println!("Imports: {:?}", semantics.imports);
println!("Functions: {:?}", semantics.functions);
```

## Architecture

```
unfault-core/
├── src/
│   ├── lib.rs          # Public API exports
│   ├── error.rs        # Error types
│   ├── parse/          # Tree-sitter parsing
│   │   ├── ast.rs      # AST types and FileId
│   │   ├── python.rs   # Python parser
│   │   ├── go.rs       # Go parser
│   │   └── ...
│   ├── semantics/      # Semantic analysis
│   │   ├── common/     # Language-agnostic types
│   │   ├── python/     # Python-specific semantics
│   │   ├── go/         # Go-specific semantics
│   │   └── ...
│   ├── graph/          # Code graph construction
│   │   └── mod.rs      # CodeGraph, GraphNode, GraphEdgeKind
│   └── types/          # Common types
│       ├── context.rs  # SourceFile, Language
│       ├── profile.rs  # Analysis profiles
│       └── ...
```

## License

MIT
