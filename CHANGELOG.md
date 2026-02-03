# Changelog

All notable changes to `unfault-core` will be documented in this file.

The format is based on Keep a Changelog, and this project adheres to Semantic Versioning.

## [Unreleased]

## [0.1.19] - 2026-02-03

### Fixed

- Python HTTP extraction: fold simple base URL f-string bindings built from `os.getenv(..., default)` so remote hosts/ports can be derived for cross-workspace resolution.

## [0.1.18] - 2026-02-03

### Added

- Cross-workspace HTTP tracing support:
  - Add `workspace_id` and `listening_ports` fields to `IntermediateRepresentation`.
  - Add `IntermediateRepresentation::with_workspace()` constructor.
  - Add `detected_ports()` method to `SourceSemantics` for port extraction.
- Listening port detection from env var defaults across all languages:
  - Python: `os.getenv("PORT", "8080")`, `os.environ.get("PORT", "8080")`
  - Rust: `env::var("PORT").unwrap_or("8081")`
  - Go: `os.Getenv` with PORT-like variable names
  - TypeScript: `process.env.PORT` patterns

## [0.1.17] - 2026-02-02

### Added

- CodeGraph: emit remote server nodes and `HttpCall` edges for outbound HTTP callsites.
- Rust HTTP extraction: propagate env var names through simple `let` bindings (e.g., `let base = env::var("API_URL")...; format!("{}/...", base)`).

## [0.1.16] - 2026-02-02

### Changed

- Formatting and clippy hygiene (no functional changes).

## [0.1.15] - 2026-01-29

### Fixed

- Rust HTTP detection: detect chained `reqwest::Client::new().post(url)` patterns for outbound fault injection cards.

## [0.1.14] - 2026-01-29

## [0.1.13] - 2026-01-29

### Fixed

- Rust route extraction: correctly handle chained builder calls (Axum `.route(...)`, Actix `.route(...)`, Rocket `.mount(...)`, Tide `.at(...).get(...).post(...)`, Poem `.at(..., get(...).post(...))`).
- Rust route handlers: populate HTTP metadata on existing function nodes and add a best-effort synthetic `Calls` edge from the registrar (typically `main`) to the handler for impact analysis.

## [0.1.12] - 2026-01-28

### Added
- Cross-language URL env-var extraction for HTTP calls (Go: `os.Getenv`, Rust: `std::env::var`/`env!`, TypeScript: `process.env`/`import.meta.env`/`Bun.env`/`Deno.env.get`).
- Go/Rust HTTP URL literal + expression metadata for client callsites.
- Richer HTTP call extraction parity:
  - Go: timeout values (seconds), request/context association for `client.Do(req)`, and retryablehttp classification.
  - TypeScript: timeout values (seconds), instance tracking (`axios.create`, `got.extend`, `ky.create/extend`), and best-effort base URL joining.
  - Rust: tighter reqwest binding detection, improved retry recognition, and loop context propagation.
  - Python: http detection upgraded toward httpx-level richness (client defaults, timeout values, retry mapping).
- Prefix-aware route extraction across frameworks:
  - Go: Gin groups, Chi `Route`/`Mount`, and Gorilla/mux `PathPrefix(...).Subrouter()`.
  - Rust: Axum nest/service routes, Actix-web scope nesting, Rocket mount prefixes.
  - TypeScript: Express router mounts, Fastify register prefixes, NestJS controller prefixes.
- FastAPI: `include_router(..., prefix=...)` parsing and path composition with `APIRouter(prefix=...)`.
- Django: method inference for views (decorators and class-based views) and richer URL pattern metadata.
- CodeGraph best-effort cross-file route composition:
  - FastAPI: apply `include_router(..., prefix=...)` prefixes to imported router modules.
  - Django: follow `include(...)` across urlconf modules and compose path prefixes into handler HTTP metadata.
- Django URL patterns emitted as common `RoutePattern` entries (best-effort; includes skipped in per-file emission).
- Language support audit document: `docs/language-support-audit.md`.

### Fixed
- Stop tracking `.unfault/` cache artifacts.

## [0.1.11] - 2026-01-28

- Extend support on env var detection

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
[0.1.11]: https://github.com/unfault/core/compare/v0.1.7...v0.1.11
[0.1.12]: https://github.com/unfault/core/compare/v0.1.11...v0.1.12
[0.1.13]: https://github.com/unfault/core/compare/v0.1.12...v0.1.13
[0.1.14]: https://github.com/unfault/core/compare/v0.1.13...v0.1.14
[0.1.16]: https://github.com/unfault/core/compare/v0.1.15...v0.1.16
[0.1.15]: https://github.com/unfault/core/compare/v0.1.14...v0.1.15
[0.1.17]: https://github.com/unfault/core/compare/v0.1.16...v0.1.17
[0.1.18]: https://github.com/unfault/core/compare/v0.1.17...v0.1.18
[0.1.19]: https://github.com/unfault/core/compare/v0.1.18...v0.1.19
[Unreleased]: https://github.com/unfault/core/compare/v0.1.19...main
