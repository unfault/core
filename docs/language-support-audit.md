# unfault-core: Multi-language Support Audit (Python, Go, Rust, TypeScript)

This document audits language support in `unfault-core` across:
- parsing (tree-sitter),
- semantic extraction (per-file),
- framework/server detection (routes, middleware),
- HTTP client detection (timeouts/retries/url extraction),
- concurrency/async detection,
- error-handling patterns,
- DB/ORM detection,

and then highlights quality risks (false positives/negatives) + concrete improvement suggestions.

Repo scope: `/home/sylvain/dev/unfault/core` (crate: `unfault-core`).

---

## 0) Executive Snapshot

- Supported end-to-end (parse + semantics build): Python, Go, Rust, TypeScript (TSX supported for `.tsx`).
- Framework/server detection exists in all four, with varying precision:
  - Python: strongest and most explicit (FastAPI/Flask/Django).
  - Go: route extraction across multiple frameworks; more heuristic classification.
  - Rust: framework detection is import/text-based + route extraction for several frameworks.
  - TypeScript: Express/Fastify/NestJS support; decorators/routes are heuristic but broad.
- HTTP client detection exists in all four; URL extraction is strongest in Python/TypeScript, weakest in Rust/Go.
- CI tests are Rust-only (this is a Rust crate); there are extensive unit tests for parsing and many semantics features.

---

## 1) Architecture + Entry Points

### Parsing (tree-sitter)

- Generic parse entrypoint: `src/parse/mod.rs`
  - Supports `Language::Python | Go | Rust | Typescript`.
  - Unsupported languages return `parsing not yet implemented`.
- Per-language parsers:
  - Python: `src/parse/python.rs`
  - Go: `src/parse/go.rs`
  - Rust: `src/parse/rust.rs`
  - TypeScript/TSX: `src/parse/typescript.rs` (chooses TSX grammar for `.tsx`)

Dependencies (tree-sitter grammars): `Cargo.toml` includes python/rust/go/typescript, plus javascript/java/yaml/hcl/json (not yet wired end-to-end).

### Semantics build wiring

- Generic semantics wrapper + dispatcher: `src/semantics/mod.rs`
  - `SourceSemantics::{Python, Go, Rust, Typescript}`
  - `build_source_semantics(parsed)` routes by `Language`.
- Each language has a semantics module:
  - Python: `src/semantics/python/mod.rs`
  - Go: `src/semantics/go/mod.rs`
  - Rust: `src/semantics/rust/mod.rs`
  - TypeScript: `src/semantics/typescript/mod.rs`

---

## 2) Coverage Audit (Feature-by-feature, with recognized frameworks/clients)

### 2.1 Python Coverage

#### Parsing

- `src/parse/python.rs`: tree-sitter-python parsing.

#### Core semantics model (language-agnostic-ish primitives)

- `src/semantics/python/model.rs`
  - Imports:
    - Captures `import ...` and `from ... import ...`.
    - Categorizes: stdlib vs third-party vs local (via allowlist of stdlib module names).
    - Tracks module-level vs nested-scope imports.
    - Provides import insertion helpers (basic and PEP8-ish placement).
  - Functions:
    - Captures `def` / `async def`, params, optional return type annotation.
    - Computes a normalized body hash for duplication detection.
  - Classes:
    - Captures class name + base classes.
  - Assignments:
    - Captures module-level and nested assignments; supports type annotations (`x: T = ...`) in a heuristic way.
  - Call sites:
    - Captures callee expr, args repr, loop/comprehension context, “self call” / “import call” heuristics.
  - Error patterns:
    - Bare `except:` detection with byte ranges and enclosing function context.
  - Decorators:
    - Captures decorator text/name and attachment to function.

#### Framework/server detection

- FastAPI: `src/semantics/python/fastapi.rs`
  - Detects:
    - `app = FastAPI(...)` (only simple `identifier = FastAPI(...)`).
    - `app.add_middleware(CORSMiddleware, ...)` (currently only cares about CORS).
    - `app.include_router(...)` (captures args text; prefix parsing TODO in code).
    - Decorator routes:
      - `@app.get("/path")`, `@router.post("/path")`, etc.
      - Captures method/path/handler name/async/try-except presence.
    - Exception handlers:
      - `@app.exception_handler(ExceptionType)`.
    - Dependency injection:
      - Detects `Depends(...)` inside handler parameter defaults or `Annotated[..., Depends(...)]`.
      - Extracts dependency targets (identifier/attribute only).
- Flask: `src/semantics/python/flask.rs`
  - Detects:
    - `app = Flask(...)`.
    - `bp = Blueprint('x', __name__)`.
    - Route decorators `@app.get(...)`, `@app.route(...)`, etc.
    - Error handlers `@app.errorhandler(404)`.
- Django: `src/semantics/python/django.rs`
  - Detects:
    - Models: `class X(models.Model): ...` and variants with base classes containing `Model`.
    - Views: function definitions are treated as views (HTTP method detection currently stubbed to `"GET"`).
    - URL patterns: `path(...)`, `re_path(...)`, `include(...)`.
    - Middleware: assignment of middleware lists or calls containing `Middleware`.

#### HTTP client detection

- `src/semantics/python/http.rs`
  - Recognized clients:
    - `requests.*` request methods.
    - `httpx.*` request methods.
    - `aiohttp.ClientSession` + calls on bound client instances.
    - Client instances:
      - `session = requests.Session()`.
      - `client = httpx.Client()/AsyncClient()` and `with/async with ... as client`.
  - Extracts:
    - Method name (get/post/put/patch/delete/head/options/request).
    - URL literal vs expression:
      - literals from quoted strings.
      - templates from f-strings.
      - env-var extraction: `os.getenv("X")`, `os.environ["X"]`, `os.environ.get("X")`.
      - can resolve f-string `{VAR}` when `VAR = os.getenv("ENV")` at module level.
    - Timeout presence: checks `timeout=` kwarg (text-based).
    - Retry signals:
      - decorators: tenacity/backoff/stamina.
      - session-level retry config (HTTPAdapter/Retry, httpx transport retries).
    - Async correctness signal:
      - marks blocking calls as “thread-offloaded” if wrapped in `asyncio.to_thread`, `run_in_executor`, `sync_to_async`, etc.

#### Async/concurrency detection

- `src/semantics/python/async_ops.rs`
  - Detects:
    - asyncio task spawns (`create_task`, `ensure_future`, etc.).
    - gather/wait/wait_for patterns.
    - await expressions.
    - sleep/timeouts.
  - Flags:
    - operations without error handling (try/except ancestor).
    - “can hang” ops without a timeout flag (heuristic).

#### DB/ORM detection

- `src/semantics/python/orm.rs`
  - Recognizes (heuristically):
    - SQLAlchemy query patterns (`session.query`, `.filter`, `.all`, etc.).
    - Django ORM `.objects.*`.
    - select_related / prefetch_related eager loading signals.
    - Relationship access heuristics in loops/comprehensions for N+1 suspicion.
  - Provides N+1 pattern helper: `detect_n_plus_one_patterns(...)`.

---

### 2.2 Go Coverage

#### Parsing

- `src/parse/go.rs`: tree-sitter-go parsing.

#### Core semantics model

- `src/semantics/go/model.rs` (built via `src/semantics/go/mod.rs`)
  - Package name, imports.
  - Functions, methods, types, declarations, call sites.
  - Reliability + concurrency oriented signals (see file for full structures):
    - unchecked errors.
    - goroutine spawns.
    - channel operations + select statements.
    - defer statements + defer/recover.
    - context usage (`context.Background/TODO/WithTimeout/WithDeadline`), including whether used in handlers.

#### HTTP server/framework route extraction

- `src/semantics/go/frameworks.rs`
  - Framework detection via import text:
    - Gin: `github.com/gin-gonic/gin`.
    - Echo: `github.com/labstack/echo`.
    - Fiber: `github.com/gofiber/fiber`.
    - Chi: `github.com/go-chi/chi`.
    - Gorilla Mux: `github.com/gorilla/mux`.
    - net/http: `net/http`.
  - Route extraction patterns:
    - Gin/Echo style: `r.GET("/path", handler)`.
    - Fiber/Chi style: `app.Get("/path", handler)`.
    - Mux: `r.HandleFunc("/path", handler).Methods("GET")`.
    - net/http: attempts `http.HandleFunc("/path", handler)` and `http.Handle(...)`.
  - Emits: framework, http method, path, handler name (optional), location/byte ranges.

#### HTTP client detection

- `src/semantics/go/http.rs`
  - Recognized client patterns:
    - `http.Get`, `http.Post`, `http.PostForm`, `http.Head`.
    - heuristic for `*Client.Do/Get/Post/Head`, plus resty/fasthttp based on receiver text.
  - Extracts:
    - method name, call text.
    - timeout hints (string search for `WithTimeout`/`WithDeadline`/`Timeout:`).
    - “error handled” heuristic: checks whether call expression is in assignment/short var decl/if statement.

---

### 2.3 Rust Coverage

#### Parsing

- `src/parse/rust.rs`: tree-sitter-rust parsing.

#### Core semantics model

- `src/semantics/rust/mod.rs`, `src/semantics/rust/model.rs`
  - Use statements, functions (with visibility/async/unsafe/const/extern), structs/enums/traits/impls.
  - Call sites, macro invocations, field accesses, variable bindings.
  - Test subtree skipping:
    - inline tests (`#[cfg(test)]`, `#[test]`, `#[tokio::test]`) are intentionally ignored in many traversals.

#### HTTP server/framework route extraction

- `src/semantics/rust/frameworks.rs`
  - Framework detection (import/text): Axum, Actix-web, Rocket, Warp, Poem, Tide.
  - Route extraction patterns:
    - Axum: `.route("/path", get(handler))` and chained `.post(...)` etc.
    - Actix-web: `.route("/path", web::get().to(handler))` and `web::resource("/path")...`.
    - Rocket: `#[get("/path")] fn handler...`.
    - Warp: `warp::path("x").and(warp::get()).and_then(handler)` and `warp::path!(...)`.
    - Poem/Tide: `.at("/path", get(handler))` / `.at("/path").get(handler)`.
  - Middleware extraction:
    - Axum `.layer(...)` best-effort layer name extraction.

#### HTTP client detection

- `src/semantics/rust/http.rs`
  - Recognized clients:
    - Reqwest (async) and reqwest blocking (`reqwest::blocking::*`).
    - ureq (`ureq::get/post/...`).
    - hyper / surf / awc / isahc (mostly classification).
  - Extracts:
    - method name (field name heuristics).
    - has_timeout + timeout_value from chains like `.timeout(Duration::from_secs(...))`.
    - async context + whether call appears under an await expression (`has_await`).
  - Converted into common `HttpCall` representation in `src/semantics/rust/mod.rs`.

#### DB detection

- `src/semantics/rust/mod.rs` has heuristic detection for Diesel / SeaORM / sqlx / tokio-postgres patterns, emitting common `DbOperation`.

#### Async/concurrency + safety + error handling

- Implemented within `src/semantics/rust/mod.rs` (and supporting model):
  - async runtime usage signals (tokio/async-std).
  - spawn calls, await points, select macro usage.
  - unwrap/expect/result ignore patterns.
  - unsafe blocks.
  - synchronization patterns and channels (see model).

---

### 2.4 TypeScript Coverage

#### Parsing

- `src/parse/typescript.rs`: TypeScript and TSX grammars (TSX for `.tsx`).

#### Core semantics model

- `src/semantics/typescript/mod.rs`, `src/semantics/typescript/model.rs`
  - Imports, functions, classes, variables, call sites.
  - Try/catch analysis:
    - empty catches.
    - bare `catch {}` clauses (no error parameter).
    - try/catch blocks with “has logging” / “has reraise” style signals.
  - Global mutable state detection (module-level let/var).
  - Async operations:
    - Promise constructor/combinators/chains.
    - await expressions.
    - timeouts.
    - cancellation hints.
  - DB operations container exists (`db_operations: Vec<DbOperation>`) and is populated by TS analysis code in this module.

#### HTTP server/framework detection

- Express: `src/semantics/typescript/express.rs`
  - Detects `const app = express()` and `const router = Router()` / `express.Router()`.
  - Routes: `app.get('/path', handler)` / `router.post(...)` etc.
  - Middleware: `app.use(...)` / `router.use(...)` with common middleware-name heuristics.
  - Extracts method, path (if first arg string-ish), handler name (if identifier), `is_async`, “error handler” heuristics (4 params).
- Fastify: `src/semantics/typescript/fastify.rs`
  - Detects `const app = fastify()` (and some require patterns).
  - Routes: `app.get('/path', handler)` etc.
  - Middleware/plugins: `.use(...)` and `.register(...)` with plugin-name heuristics (jwt/cookie/rate-limit, cors, helmet).
- NestJS: `src/semantics/typescript/nestjs.rs`
  - Detects `@Controller(...)` classes, `@Injectable`, `@Module`.
  - Method decorators: `@Get/@Post/...` routes.
  - Guards/interceptors: `@UseGuards`, `@UseInterceptors`.
  - Extracts method, decorator path argument (string), handler name, `is_async`.

#### HTTP client detection

- `src/semantics/typescript/http.rs`
  - Recognized clients:
    - `fetch`.
    - `axios` and `axios.<method>`.
    - `got` and `got.<method>`.
    - `ky` and `ky.<method>`.
    - Node `http.get/request` and `https.get/request`.
    - `undici.fetch/request`.
    - `superagent.<method>`.
    - “instance method calls” like `httpClient.get(...)` when receiver looks like an HTTP client.
  - Extracts:
    - method and client kind.
    - URL literal vs expression:
      - strings.
      - template literals (distinguishes templates with `${...}`).
      - `process.env.FOO` and `process.env["FOO"]` env-var extraction.
    - timeout: args contain `timeout` or fetch `signal` usage.
    - error handling: try/catch ancestor or promise `.catch()` chaining heuristic.
    - retry: args contain `retry`/`retries` heuristic.

---

## 3) Quality Audit (Top Risks + Recommendations)

This section is organized by feature area; each item includes likely failure modes and suggested fixes.

### 3.1 Framework detection & route extraction

#### Python

- Risk: FastAPI app detection only handles `identifier = FastAPI(...)`; misses `self.app = FastAPI()` or complex LHS.
  - Improve: support attribute assignment targets optionally guarded by context (module-level only, or within factory functions).
- Risk: `include_router` prefix parsing is TODO; currently `router_expr` is raw arguments text.
  - Improve: parse keyword arguments in `arguments` node; extract literal `prefix=` and maybe `tags=`.
- Risk: Django view HTTP method inference is stubbed to `GET`.
  - Improve: infer based on DRF decorators (`@api_view`), class-based views (`APIView`/`View` and `get/post/...` methods), and `require_http_methods`.

#### Go

- Risk: framework inference can misclassify between Gin/Echo or Fiber/Chi when imports aren’t explicit; defaults are applied.
  - Improve: bind router variables from constructor calls (`r := gin.Default()`, `e := echo.New()`, etc.) and use that mapping for route calls.
- Risk: net/http route extraction may be brittle; it should be based on selector_expression fields rather than text prefix checks.
  - Improve: detect `selector_expression` with operand `http` and field `HandleFunc`/`Handle`.

#### Rust

- Risk: framework detection is import/text-based and may miss re-exports or fully-qualified usage without `use`.
  - Improve: inspect AST for `path_expression`/`scoped_identifier` uses like `axum::Router`, `warp::path`.
- Risk: route extraction uses text pattern searches (e.g., `.route(`) which can be fooled by formatting or unrelated APIs.
  - Improve: parse the call_expression structure and confirm receiver field names (`route`, `service`, `at`, etc.).
- Risk: assumes Axum handlers are always async.
  - Improve: mark handler async as unknown unless handler fn definition is found, or inspect handler function signatures if present.

#### TypeScript

- Risk: Express/Fastify route extraction can treat any `.get/.post` as a route if patterns match loosely.
  - Improve: only accept `.get/.post` calls on identifiers previously proven to be apps/routers.
- Risk: NestJS method decorator extraction is simplistic and may miss multiple decorators.
  - Improve: walk decorators attached to method_definition nodes directly and collect all HTTP decorators.

### 3.2 HTTP client detection (libraries, URLs, timeouts, retries)

#### Python

- Strength: URL literal vs expression + env-var detection + f-string binding resolution + client instance tracking + thread-offload detection.
- Risk: timeout detection is text-based (`timeout=`) and misses positional/client-level timeout configuration.
  - Improve: parse keyword_argument nodes; detect `httpx.Timeout(...)` usage and client defaults.
- Risk: retry detection is primarily decorator/session-config signals.
  - Improve: detect loop+sleep backoff patterns around calls (some scaffolding exists via `RetrySource::LoopWithSleep`).

#### Go

- Risk: timeout detection is weak and local to call text.
  - Improve: associate calls with nearby `context.WithTimeout/WithDeadline` usage and request construction patterns.
- Risk: error handling heuristic may miss common `err` check structures.
  - Improve: explicitly detect `err` bindings + subsequent `if err != nil`.

#### Rust

- Risk: URL extraction is effectively absent.
  - Improve: extract string literal first arguments for common call shapes (`.get("...")`, `ureq::get("...")`).
- Risk: reqwest detection may false-positive when receiver contains “client”.
  - Improve: track bindings from `reqwest::Client::new()` to variable names and require those.

#### TypeScript

- Strength: broad library coverage + URL literal/expr + env-var extraction + route-handler suppression.
- Risk: instance method HTTP detection relies on receiver-name heuristics.
  - Improve: also track instantiations (axios instances via `axios.create()`, got/ky instances) to widen detection without introducing `map.get()` false positives.
  - Improve: consider an opt-in “aggressive mode” based on first-arg URL shape (`http(s)://`).

### 3.3 Async/concurrency detection

#### Python

- Risk: focuses on `asyncio.*`; misses trio/anyio patterns.
  - Improve: add anyio/trio primitives (task groups, cancel scopes, timeouts).

#### Go

- Strength: richer concurrency model (goroutines/channels/select/context/defer/recover).
- Risk: handler-context classification depends on route detection quality.
  - Improve: connect extracted routes to context usage analysis.

#### Rust

- Strength: tokio/async-std signals, await points, spawn calls, select macro usage, unsafe/error patterns.
- Risk: skipping test subtrees can hide patterns when projects keep important logic in test-only modules.
  - Improve: add a configuration switch to include/exclude tests.

#### TypeScript

- Risk: async ops can be noisy and not always actionable.
  - Improve: prioritize risky patterns (un-awaited promises, `Promise.all` in handlers, missing cancellation/timeouts for IO).

### 3.4 Error-handling detection

#### Python

- Strong: bare `except:` detection with byte ranges.
- Gap: does not explicitly detect “swallowed exceptions” like `except Exception: pass`.
  - Improve: detect empty except bodies and `pass`/`return` only except blocks.

#### Go

- Mixed: unchecked error detection exists but depends on AST shapes/patterns.
  - Improve: enhance for ignored results, `_` assignments, and multi-return call sites.

#### Rust

- Strong: unwrap/expect/result ignore; reduces noise by skipping tests.
- Risk: macros/generated code can produce false positives.
  - Improve: add suppression mechanisms (comments/attributes) if needed.

#### TypeScript

- Strong: empty catch + bare catch + try/catch metadata.
- Risk: `.catch()` detection is permissive.
  - Improve: only mark error-handled if `.catch(...)` is directly chained to the promise-returning expression.

---

## 4) Suggested Next Steps (Practical Roadmap)

1. Fix Go `net/http` route extraction to use selector_expression fields (high-value correctness fix).
2. Add URL extraction for Rust HTTP calls (at least literal URL strings).
3. Make framework route extraction in TS and Go depend on tracked app/router bindings (reduce false positives).
4. Improve Python Django method inference (DRF + class-based views + decorators).
5. Add config knobs:
   - include/exclude tests (Rust),
   - strict vs aggressive HTTP client detection (TS),
   - strict vs best-effort framework classification.

---

## 5) Quick Reference (Key Files)

- Parsing:
  - `src/parse/mod.rs`
  - `src/parse/python.rs`
  - `src/parse/go.rs`
  - `src/parse/rust.rs`
  - `src/parse/typescript.rs`
- Semantics wiring:
  - `src/semantics/mod.rs`
- Python:
  - `src/semantics/python/model.rs`
  - `src/semantics/python/fastapi.rs`
  - `src/semantics/python/flask.rs`
  - `src/semantics/python/django.rs`
  - `src/semantics/python/http.rs`
  - `src/semantics/python/async_ops.rs`
  - `src/semantics/python/orm.rs`
- Go:
  - `src/semantics/go/model.rs`
  - `src/semantics/go/frameworks.rs`
  - `src/semantics/go/http.rs`
- Rust:
  - `src/semantics/rust/mod.rs`
  - `src/semantics/rust/model.rs`
  - `src/semantics/rust/frameworks.rs`
  - `src/semantics/rust/http.rs`
- TypeScript:
  - `src/semantics/typescript/model.rs`
  - `src/semantics/typescript/express.rs`
  - `src/semantics/typescript/fastify.rs`
  - `src/semantics/typescript/nestjs.rs`
  - `src/semantics/typescript/http.rs`
- CI:
  - `.github/workflows/ci.yml`
  - `.github/workflows/release.yml`
