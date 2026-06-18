# ZenAPI Slint Migration TODO

> Last updated: 2026-06-18
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
- [x] Implement a Slint app shell with global navigation, endpoint sidebar,
  request editor, response viewer, and compact navigation.
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
- [x] Keep the request body editor and response viewer usable as Slint text
  controls; response Pretty, Raw, Headers, and Set-Cookie-derived Cookies tabs
  are visible, and the response toolbar now wires Copy, JSON Format,
  per-tab Fold/Open, Fold All, and Open All actions. JSON response folding now
  renders top-level object/array structure while collapsing nested containers,
  with line-count summaries for non-JSON tabs, and Copy prefers the selected
  text in the current visible response view before falling back to the whole
  visible tab.

## Phase 3: Slint UI Parity Backlog

- [~] Rebuild Params and Headers editors in Slint and wire them into
  `send_request_with_options`; current Slint baseline uses real editable row
  controls for Params and Headers backed by line-based text storage, plus
  table-shaped line-based editors for API-key pairs. Parsing supports
  `key=value` / `key: value` plus common header presets and header clipboard
  copy; Params/Headers now support clipboard bulk paste and local file import.
- [~] Rebuild Body mode controls for none, form-data, URL-encoded, raw, GraphQL,
  and binary bodies; current Slint baseline has dedicated visible panels for
  no-body, raw text, form-data, URL-encoded, GraphQL query/variables, and
  binary file paths, all mapped into transport modes, with editable rows for
  form-data and URL-encoded fields. Raw JSON/Text/XML subtype selection is
  wired through transport, collection restore, and history restore. Dedicated
  form file attach controls now add validated local files as multipart `@path`
  rows, and binary file paths infer common Content-Type values from extensions.
  GraphQL introspection responses can be summarized from the response pane,
  with native file picker dialogs now wired for OpenAPI files, Params/Headers
  imports, multipart attachments, gRPC descriptors/proto files, and
  codegen/runner/mock export paths.
- [~] Rebuild Auth controls for None, Bearer, Basic, JWT, and API key modes;
  current Slint Request Builder has the dedicated Auth tab, mode buttons, and
  mode-specific config panels wired to transport mapping. API key header/query
  modes now use editable add/delete rows backed by the existing `key=value`
  config storage, and Basic auth uses split username/password fields while
  preserving the existing `username:password` config format. Manual OAuth2
  access-token mode is wired as `Authorization: Bearer <token>`; browser token
  acquisition, redirect handling, refresh, and secure token storage remain
  pending.
- [~] Rebuild Environments and Variables UI on top of `variables::VariableStore`;
  current Slint baseline uses the reference three-pane Environments page with
  a persisted dynamic environment list, editable global/env variable rows,
  add/delete variable row actions, environment add/delete actions, scope badges,
  per-environment value preservation during switches, a masked JSON preview,
  and local `.zenapi-environments.json` persistence for global variables plus
  per-environment values, still backed by the existing `key=value` text
  storage. Environment rename and persisted environment reorder controls are
  wired.
- [~] Rebuild Collections UI for native JSON/Postman import/export, saving
  current requests, nested folders, rename/delete/copy, and drag/drop; current
  Slint baseline loads native/Postman JSON, saves the current editor request,
  saves native JSON, exports Postman JSON, exposes native file pickers for
  collection import/save/Postman export, and restores saved requests from a
  sidebar list with imported folder rows visible, selects folder rows, creates
  root-level and nested folders, includes pre-request/tests text, and
  saves current requests into the selected folder or collection root,
  duplicates/deletes saved requests, with request rename and folder reparenting
  wired; drag/drop remains pending.
- [~] Rebuild History UI for local request history, filtering, restore, delete,
  and clear; current Slint baseline records recent requests and restores method,
  URL, query params, headers, auth config, body mode, body preview, pre-request
  script, and tests, with visible sidebar filtering, single-entry delete, and
  clear controls plus local JSON persistence.
- [~] Rebuild Codegen UI for cURL, Python, JavaScript, Rust, and Go snippets;
  domain wiring still generates snippets from the resolved request projection,
  and the Slint shell now has a dedicated Codegen page with generate, clipboard
  copy, save actions, and generated snippet metadata for language, request,
  size, headers, query params, and body mode.
- [~] Rebuild Collection Runner UI while keeping `zenapi run` as the stable CLI;
  domain wiring and CLI execution are in place, and the Slint shell now has a
  dedicated Test Runner page with run/cancel/report export controls plus
  text/JSON report format selection.
- [~] Rebuild Pre-request script-lite and native Tests panels; current Slint
  baseline provides a pre-request editor plus editable Tests assertion rows
  with Kind/Target/Expect fields and add/delete controls, applies pre-request
  actions during send/codegen, evaluates native tests after single sends, and
  saves/restores both fields with collection requests. Kind cycling and Status,
  Header, Body, and JSON assertion template builders are wired, plus a custom
  Kind/Target/Expect builder that validates against the native assertion parser.
  Common single-line `pm.test(...)` status, header, body, and JSON expectations
  are mapped into native assertions; full JavaScript `pm.*` compatibility
  remains pending.
- [~] Rebuild WebSocket and SSE panels using the restored client modules;
  current Slint Request Builder has a visible Realtime tab with WebSocket
  one-shot text send, persistent open/send/close text and binary sessions with
  subprotocol entry and WebSocket event history copy/clear controls, plus SSE
  bounded previews and persistent stream/stop controls with Last-Event-ID resume
  plus SSE event history copy/clear controls.
- [~] Add GraphQL and gRPC UI surfaces after REST parity is stable; current
  Slint baseline has GraphQL query/variables editing, query/mutation/
  introspection templates, plus a visible Realtime gRPC draft surface backed by
  the domain model with optional manual method catalog validation,
  FileDescriptorSet/protoset catalog extraction and Slint protoset load action,
  direct proto source descriptor extraction and Slint proto source load action,
  reflection descriptor extraction and Slint reflection load action, descriptor
  display plus grpcurl command previews, unary gRPC transport with dynamic
  protobuf JSON mapping, local tonic service coverage, and a Slint Invoke action
  wired to real unary calls; server streaming transport has local tonic service
  coverage, incremental stream events, and Slint Stream/Stop actions. GraphQL
  schema response summaries are wired.
- [~] Add mock request logs and richer mock manager controls; current Slint
  baseline has a dedicated Mock Manager page with endpoint selection,
  start/stop, editable selected route response JSON, real per-route header/query
  conditional response rules, traffic filtering, clear, and export placement.
- [~] Split reusable controls from `ui/app.slint` into dedicated Slint modules;
  current Slint baseline keeps only shared `UiText` typography in
  `ui/widgets.slint`, while method/status indicators live in
  `ui/status_components.slint`, action buttons in `ui/action_button.slint`,
  text fields in `ui/text_field.slint`, mode buttons in `ui/mode_button.slint`,
  request method selection in `ui/method_selector.slint`, and request/response
  tab controls in `ui/tab_controls.slint`,
  moved metric cards, data panels, and mock log rows into `ui/cards.slint`,
  moved code/editor panes into `ui/editors.slint`, and the Realtime editor
  business panel, shared Key/Value table panel, Body editor panel, Header
  editor panel, Auth editor panel, Scripts editor panel, Tests assertion panel,
  Request panel, Response panel, Sidebar, App navigation, Address bar,
  Dashboard page, Codegen page, Settings page, Team page, API Keys page,
  Analytics page, Documentation page, Environment page, Runner page, Mock
  Manager page, Mock Manager row components, request editor pane, request
  sidebar pane, sidebar rows, sidebar OpenAPI import panel, sidebar Collections
  panel, and sidebar History panel now live in dedicated Slint files;
  additional large panels still need extraction.

## Phase 4: Verification And Release

- [x] `cargo check`
- [x] `cargo test`
- [x] `cargo build --release`
- [x] GUI smoke test on Linux Wayland.
- [ ] GUI smoke test on Linux X11.
- [ ] Windows build validation.
- [ ] macOS build validation.
- [ ] Release packaging.

## Current Status

| Area | Status | Notes |
|------|--------|-------|
| App shell | Multi-page Slint shell restored | Responsive page stack with page-level ScrollViews kept mounted and switched by visibility, compact single-row request address bar, lightweight read-only code blocks, global navigation, compact navigation, request-sidebar OpenAPI import, and overflow scrollbars |
| OpenAPI import | Implemented | JSON/YAML parser and Slint route list wired |
| HTTP client | Core implemented | Slint wires method, URL, params, headers, auth, variables, and body modes |
| Mock server | Core implemented | Slint start/stop toggle wired |
| Variables | Domain implemented | Slint baseline has a three-pane environment page with dynamic persisted environment rows, environment add/delete/rename/reorder, editable variable rows, scope badges, masked preview, and local persistence for global/per-environment values/order |
| Collections | Domain implemented | Slint baseline supports load/save/export, folder row display/selection, root and nested folder creation, and request restore/rename/duplicate/delete/move |
| History | Domain implemented | Slint baseline records, filters, deletes, restores, and persists recent requests |
| Codegen | Domain + Slint page implemented | Dedicated page generates, copies, and saves snippets |
| Runner | Domain + Slint page + CLI implemented | Dedicated page runs/cancels collections and saves reports; CLI remains stable |
| Assertions/scripts | Domain implemented | Slint baseline edits, builds, saves, restores, and evaluates native scripts/tests |
| WebSocket/SSE | Client modules restored | Realtime tab supports one-shot WS text, persistent WS text/binary sends with subprotocols, WebSocket history copy/clear, plus SSE previews, streams, history copy, and clear |
| Mock logs | Core implemented | Slint baseline shows, filters, clears, and exports recent mock requests |
| GraphQL | Payload builder implemented | Slint baseline edits query/variables, applies query/mutation/introspection templates, and summarizes introspection responses from the response pane |
| gRPC | Domain draft + descriptor catalog + unary invoke + server-stream action implemented | Realtime tab validates endpoint, method, metadata, message JSON, optional method catalog entries, FileDescriptorSet/protoset extraction, Slint protoset/proto source/reflection loading into the method catalog, grpcurl command previews, real unary calls, incremental server-stream events, and real Stream/Stop actions from the Slint gRPC pane |
| Reference pages | Slint baseline implemented | Dashboard, Request Builder, Mock Manager, Environments, Test Runner, API Docs, API Keys, Team, Project Settings, and Traffic Analytics now have routed reference-aligned page layouts; unwired reference-page action buttons were removed, API Docs snippets use the active request URL/auth state, API Keys mirrors active request auth, Team/Settings show local-only workspace state, and Traffic Analytics reads local session/mock-log data instead of static samples |
