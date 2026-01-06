# Changelog

All notable changes to `unfault-core` will be documented in this file.

The format is based on Keep a Changelog, and this project adheres to Semantic Versioning.

## [0.1.1] - 2026-01-06

### Fixed
- Prevent method calls like `db.add()` from being misinterpreted as function calls to `add()` (both for local functions and imported functions).

## [0.1.0] - 2026-01-06

### Added
- Client-side parsing and graph construction for Python, Go, Rust, and TypeScript.
- Framework summaries and route extraction (e.g., FastAPI, net/http, Express/NestJS/Fastify).
- Common semantics extraction across languages (annotations, route patterns, error contexts).

[0.1.1]: https://github.com/unfault/core/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/unfault/core/releases/tag/v0.1.0
