# ZenAPI Developer Guide

> Status: current guide for the Rust + GPUI rewrite.

## Project Shape

ZenAPI is a Rust application with a GPUI desktop shell and reusable domain
modules. The GPUI rewrite is a breaking replacement of the old Slint prototype.
Do not add Slint compatibility shims, generated UI modules, callback adapters,
or old toolkit build steps.

Primary entry points:

| Path | Purpose |
|------|---------|
| `src/main.rs` | Binary entry point; dispatches desktop app or CLI commands |
| `src/app.rs` | GPUI app shell, state, rendering, and event flow |
| `src/app/input.rs` | Custom single-line GPUI text input |
| `src/app/read_only_text.rs` | Read-only selectable response text view |
| `src/cli.rs` | CLI command parsing and collection runner command |
| `src/lib.rs` | Public library module exports |

Core modules:

| Module | Responsibility |
|--------|----------------|
| `openapi` | JSON/YAML OpenAPI and Swagger parsing |
| `mock_server` | Local Axum mock server and route generation |
| `client` | reqwest request transport and response formatting |
| `collections` | Native and Postman collection models/import/export |
| `collection_runner` | Sequential collection execution and summaries |
| `assertions` | Native response assertion model and evaluator |
| `pre_request` | Native pre-request script-lite action parser/executor |
| `variables` | `{{variable}}` replacement with global/environment scopes |
| `history` | Request history model and filtering |
| `codegen` | cURL/Python/JavaScript/Rust/Go snippet generation |

## Dependency Policy

- `gpui` and `gpui_platform` come from Zed's official repository.
- Linux builds use `gpui_platform` with `wayland` and `x11` features.
- Keep versions upgrade-friendly; avoid narrow pins unless reproducibility or
  a concrete incompatibility requires one.
- Do not bundle font assets for the GPUI shell.
- Prefer Rust/domain modules for reusable behavior and keep GPUI rendering code
  responsible for view state and event wiring.

## Build And Test

Format:

```sh
cargo fmt
```

Compile check:

```sh
cargo check
```

Run tests:

```sh
cargo test
```

Build debug binary:

```sh
cargo build
```

Build release binary:

```sh
cargo build --release
```

CLI help can be checked without starting GPUI:

```sh
target/debug/zenapi --help
target/debug/zenapi run --help
```

Starting the desktop app requires a GUI session with a usable Wayland or X11
environment.

## App State Flow

`ZenApiApp` owns GPUI entities and view state:

- Text inputs are GPUI entities.
- Long-running network/mock work runs on Tokio.
- Results return to GPUI through channels and `cx.spawn`.
- Domain modules stay plain Rust where possible.

The request flow is:

1. UI state builds a `CodegenRequest`.
2. Pre-request script-lite actions mutate request fields and request-local
   variable overrides.
3. Variables are resolved through `VariableStore`.
4. Auth/query/header/body state is normalized.
5. `client::send_request_with_body` sends through reqwest.
6. The response updates history and the response viewer.

The collection runner flow is:

1. `collection_runner::collect_collection_requests` flattens nested collection
   requests in depth-first order.
2. Each `CollectionRequest` is converted into the client request shape.
3. Requests run sequentially with optional delay.
4. Results are summarized for GPUI and CLI output.

## UI Guidelines

Persistent visual decisions live in `docs/DESIGN.md`.

Key constraints:

- Keep the UI dense and workbench-focused.
- Avoid landing-page composition and decorative cards.
- Keep cards for repeated items, modals, and framed tools only.
- Use shared helpers for button tones, response tones, and HTTP method colors.
- Keep stable layout metrics, such as table column widths, method label widths,
  and collection tree indentation, in shared constants instead of inline values.
- Use platform UI font for normal copy and `monospace` for technical text.
- Response body text must stay selectable and read-only.
- Native Tests panel rows should convert to `assertions::ResponseAssertion`
  before request execution or collection save.
- Actions that cannot succeed should be visibly disabled and guarded in code.

## Collections And Formats

Native collection JSON is the current internal format. Postman Collection v2.1
import/export is the interoperability format. Bru-style text storage is still
exploratory; see `docs/BRU_FORMAT.md`.

When changing collection models:

- Preserve native JSON round-trip tests.
- Preserve Postman import/export tests when possible.
- Update the runner conversion path if request fields change.
- Update user docs and TODO status.

## Scripts And Tests

The scripting engine decision is documented in `docs/SCRIPTING.md`. Do not add
Rhai, mlua, `deno_core`, or another engine without updating that evaluation and
measuring the effect on startup and binary size. Native response assertions are
implemented in Rust and should remain the runner/test result foundation when a
script engine is added. Native pre-request script-lite actions are implemented
in Rust and should remain the compatibility layer used by GPUI, runner, and CLI
until a full engine is deliberately added.

## Adding Features

For feature work:

1. Start from `docs/TODO.md`.
2. Keep edits scoped to the module boundary implied by the TODO item.
3. Add focused tests for domain behavior.
4. Add UI tests or pure helper tests when a GPUI surface has meaningful state
   projection logic.
5. Update docs when behavior changes.
6. Run `cargo fmt`, `cargo check`, and `cargo test`.

## Current Verification Baseline

The repository currently has unit coverage for:

- OpenAPI parsing.
- Mock server routing/CORS/logs.
- HTTP transport body modes.
- Variable replacement.
- Collection import/export and tree projection.
- History filtering and visible row projection.
- Code generation and executable cURL against a local server.
- Collection runner execution and failure strategy.
- Pre-request script-lite action parsing and runner request mutation.
- Native response assertion evaluation and runner assertion summaries.
- CLI run option parsing and summary formatting.

## Release Notes

Current release-size measurement is documented in `docs/BENCHMARK.md`.
Release packaging, startup timing, idle memory, Wayland/X11 smoke tests,
Windows validation, and macOS validation remain open TODO items.
