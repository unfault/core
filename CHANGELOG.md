# Changelog

All notable changes to `unfault-core` will be documented in this file.

The format is based on Keep a Changelog, and this project adheres to Semantic Versioning.

## [0.1.7] - 2026-01-15

- Add missing license file

## [0.1.6] - 2026-01-15

- Fix publishing of the crate

## [0.1.4] - 2026-01-14

### Added
- Added `start_line`/`end_line` attributes to Function nodes for precise code location.
- Added HTTP call detection on client instances in Python analysis.

### Fixed
- Populated `start_line`/`end_line` for route handler function nodes.
- Fixed `body_lines` calculation from location range for accurate function end line detection.
- Filtered non-HTTP methods from HTTP call detection to reduce false positives.

### Docs
- Added CONTRIBUTING.md guide.

## [0.1.3] - 2026-01-09

### Added
- Added SLO nodes and route lookup helpers to support observability-aware workflows.
- Extended FastAPI analysis with lifespan detection and improved dependency injection edge resolution.

## [0.1.1] - 2026-01-06

### Fixed
- Prevent method calls like `db.add()` from being misinterpreted as function calls to `add()` (both for local functions and imported functions).

## [0.1.0] - 2026-01-06

### Added
- Client-side parsing and graph construction for Python, Go, Rust, and TypeScript.
- Framework summaries and route extraction (e.g., FastAPI, net/http, Express/NestJS/Fastify).
- Common semantics extraction across languages (annotations, route patterns, error contexts).

[0.1.7]: https://github.com/unfault/core/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/unfault/core/compare/v0.1.5...v0.1.6
[0.1.4]: https://github.com/unfault/core/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/unfault/core/compare/v0.1.1...v0.1.3
[0.1.1]: https://github.com/unfault/core/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/unfault/core/releases/tag/v0.1.0
