# ZenAPI Slint Migration TODO

> Last updated: 2026-06-17
> This roadmap tracks the current Rust + Slint rewrite. Completed items use
> `[x]`, in-progress items use `[~]`, and remaining items use `[ ]`.

## Migration Policy

- The desktop UI framework is Slint.
- UI source lives in `ui/*.slint` and is compiled by `slint-build`.
- Rust modules keep reusable product logic: OpenAPI parsing, request transport,
  mock server, collections, variables, history, codegen, runners, SSE, and
  WebSocket.
- Do not add GPUI adapters, compatibility shims, generated GPUI modules, or old
  toolkit build steps.

## Phase 1: Slint Baseline

- [x] Restore Slint build path through `build.rs` and `ui/app.slint`.
- [x] Add `ui/theme.slint` with Nexus dark theme surface, text, outline, primary,
  secondary, tertiary, and error tokens.
- [x] Route `src/main.rs` through `src/app.rs` for the desktop app and keep
  `src/cli.rs` for CLI commands.
- [x] Implement a Slint app shell with top bar, endpoint sidebar, request editor,
  response viewer, and bottom status bar.
- [x] Wire Rust callbacks for OpenAPI import, route filtering, route selection,
  request sending, and mock server start/stop.
- [x] Remove remaining GPUI source files from `src/app/`.
- [x] Restore missing SSE/WebSocket client modules required by current exports.
- [x] Verify baseline with `cargo check` and `cargo test`.

## Phase 2: Core Slint Workbench

- [x] Import local OpenAPI/Swagger JSON or YAML files.
- [x] Render parsed routes in the Slint sidebar.
- [x] Filter routes by method, path, or summary.
- [x] Selecting a route fills method, URL, and default request body.
- [x] Send HTTP requests through the existing reqwest transport.
- [x] Show response status, timing, size, and formatted body.
- [x] Start/stop the local Axum mock server from the Slint shell.
- [~] Keep the request body editor and response viewer usable as Slint text
  controls; response headers are visible, while richer Pretty/Raw/Header tabs
  still need parity work.

## Phase 3: Slint UI Parity Backlog

- [~] Rebuild Params and Headers editors in Slint and wire them into
  `send_request_with_options`; current Slint baseline supports line-based
  `key=value` / `key: value` editors, with table controls still pending.
- [~] Rebuild Body mode controls for none, form-data, URL-encoded, raw, GraphQL,
  and binary bodies; current Slint baseline maps editors into transport modes,
  including GraphQL query/variables payload building, with dedicated form file
  controls and GraphQL introspection/schema helpers still pending.
- [~] Rebuild Auth controls for None, Bearer, Basic, JWT, and API key modes;
  current Slint baseline supports mode buttons plus one config field with
  mode-specific format hints, with dedicated per-mode forms still pending.
- [~] Rebuild Environments and Variables UI on top of `variables::VariableStore`;
  current Slint baseline supports one active environment with dev/test/prod
  quick selectors plus line-based global and environment variable editors, with
  full environment list management pending.
- [~] Rebuild Collections UI for native JSON/Postman import/export, saving
  current requests, nested folders, rename/delete/copy, and drag/drop; current
  Slint baseline loads native/Postman JSON, saves the current editor request,
  saves native JSON, exports Postman JSON, and restores saved requests from a
  flat sidebar list, including pre-request/tests text, and duplicates/deletes
  flat saved requests, with tree editing plus rename/drag/drop still pending.
- [~] Rebuild History UI for local request history, filtering, restore, delete,
  and clear; current Slint baseline records recent requests and restores method,
  URL, query params, headers, auth config, body mode, body preview, pre-request
  script, and tests, with filtering, single-entry delete, and clear controls
  wired plus local JSON persistence.
- [~] Rebuild Codegen UI for cURL, Python, JavaScript, Rust, and Go snippets;
  current Slint baseline generates snippets from the resolved request projection,
  exports snippets to a local path, with clipboard copy still pending.
- [~] Rebuild Collection Runner UI while keeping `zenapi run` as the stable CLI;
  current Slint baseline runs the active collection with delay and stop-on-fail
  controls, cancellation, and summary/result rows, with richer reports still
  pending.
- [~] Rebuild Pre-request script-lite and native Tests panels; current Slint
  baseline provides line-based editors, applies pre-request actions during
  send/codegen, evaluates native tests after single sends, and saves/restores
  both fields with collection requests, with row-based test controls pending.
- [~] Rebuild WebSocket and SSE panels using the restored client modules;
  current Slint baseline provides WebSocket one-shot send, persistent
  open/send/close text sessions, plus SSE bounded previews and persistent
  stream/stop controls with Last-Event-ID resume, with richer WebSocket
  binary/protocol controls and SSE copy/clear/history actions still pending.
- [~] Add GraphQL and gRPC UI surfaces after REST parity is stable; current
  Slint baseline has GraphQL query/variables editing plus a gRPC draft surface
  backed by the domain model, with gRPC descriptor loading/transport and GraphQL
  schema helpers still pending.
- [~] Add mock request logs and richer mock manager controls; current Slint
  baseline shows, filters, and exports recent mock requests in the sidebar,
  with richer route controls pending.
- [~] Split reusable controls from `ui/app.slint` into `ui/widgets/`; current
  Slint baseline extracts shared styles, buttons, text fields, method controls,
  tab headers, and editor panes into `ui/widgets.slint`, with larger business
  panels still pending.

## Phase 4: Verification And Release

- [x] `cargo check`
- [x] `cargo test`
- [ ] GUI smoke test on Linux Wayland.
- [ ] GUI smoke test on Linux X11.
- [ ] Windows build validation.
- [ ] macOS build validation.
- [ ] Release packaging.

## Current Status

| Area | Status | Notes |
|------|--------|-------|
| App shell | Slint MVP restored | `ui/app.slint`, `ui/theme.slint`, `src/app.rs` |
| OpenAPI import | Implemented | JSON/YAML parser and Slint route list wired |
| HTTP client | Core implemented | Slint wires method, URL, params, headers, auth, variables, and body modes |
| Mock server | Core implemented | Slint start/stop toggle wired |
| Variables | Domain implemented | Slint baseline supports global and one active environment editor |
| Collections | Domain implemented | Slint baseline supports load/save/export and request restore |
| History | Domain implemented | Slint baseline records, filters, deletes, restores, and persists recent requests |
| Codegen | Domain implemented | Slint baseline generates and exports resolved request snippets |
| Runner | Domain + CLI implemented | Slint baseline runs and cancels active collections with result rows |
| Assertions/scripts | Domain implemented | Slint baseline edits, saves, restores, and evaluates native scripts/tests |
| WebSocket/SSE | Client modules restored | Slint baseline supports one-shot and persistent WS text sends plus SSE previews and persistent SSE streams |
| Mock logs | Core implemented | Slint baseline shows, filters, and exports recent mock requests |
| GraphQL | Payload builder implemented | Slint baseline edits query and variables |
| gRPC | Domain draft model implemented | Slint draft surface wired; descriptor loading and unary transport pending |
