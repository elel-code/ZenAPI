# Research Summary And Benchmark Baseline

> Status: offline baseline. This document summarizes current direction and
> lists the live research and measurement work still required.

## Position

ZenAPI should compete as a native local API workstation:

- Faster and lighter than Electron-heavy clients.
- More private and offline-friendly than cloud-first tools.
- More integrated with local OpenAPI mock workflows than generic request
  clients.
- Directly built in Rust + GPUI, without preserving Slint prototype
  compatibility.

## Current Strengths

- GPUI app shell is in place with `gpui_platform` startup.
- OpenAPI/Swagger import drives the endpoint list and local mock server.
- HTTP request sending uses `reqwest`.
- Response viewing supports Pretty, Raw, Headers, status, elapsed time, body
  size, JSON formatting/collapse, and read-only selectable text.
- Collections support native JSON and Postman Collection v2.1 import/export.
- Variables and dynamic environments are implemented.
- History auto-record, filter, restore, delete, and clear are implemented.
- cURL/Python/JavaScript/Rust/Go code generation is implemented.
- Sequential collection runner is available from GPUI and `zenapi run`.
- GraphQL request mode, schema introspection summary/browser, WebSocket
  sessions, and SSE fetch/subscribe flows are implemented.
- The GPUI workbench now uses three same-level panes with draggable split
  handles and independent styled scroll regions.

## Priority Gaps

| Gap | Reason |
|-----|--------|
| OAuth2 | Requires a dedicated token flow and secure state handling |
| Parallel collection runner | Sequential execution is implemented; parallel scheduling and richer reports remain future work |
| Full scripting | Script-lite pre-request actions and assertions exist; a sandboxed script runtime is still a separate design decision |
| GraphQL field builder | HTTP GraphQL requests and introspection helpers exist; full visual query building remains future work |
| gRPC | Requires descriptor loading, reflection, and streaming-specific UI |
| Plugin system | Should wait until core extension points are stable |
| Visual audit | Needed after live screenshots and measurements are collected |
| Release packaging | Needed to validate Linux Wayland/X11 and binary-size goals |

## Benchmark Targets

| Metric | Target |
|--------|--------|
| Cold start | Under 1 second on a typical developer laptop |
| Idle memory | Far below Chromium-based clients |
| Release binary | Around 10 MB where practical |
| Local mock startup | Immediate enough for frontend iteration |
| Request send overhead | Close to direct `reqwest`/`curl` expectations |

## Current Measurements

Measured on Linux x86_64 in this workspace with `rustc 1.96.0
(ac68faa20 2026-05-25)`.

| Metric | Current Result | Notes |
|--------|----------------|-------|
| Release build | Passed | `cargo build --release` completed successfully on 2026-06-13 |
| Release binary | 38M / 39,590,792 bytes | `target/release/zenapi`, not stripped |
| Stripped estimate | 30M / 30,575,048 bytes | Measured from `/tmp/zenapi-stripped`; release artifact was not modified |
| Release desktop smoke | Passed outside sandbox | `timeout 4s target/release/zenapi` reached the GPUI event loop and was terminated by timeout with exit 124 and no stderr |
| 2s RSS sample | 229,132 KiB | One local `ps` sample after 2 seconds of idle release runtime; no spec imported and no request sent |
| First paint startup | Not instrumented | The current smoke proves startup does not panic in the real desktop session, but it does not measure first window paint |

The current binary is above the rough 10 MB target. That target should remain
an optimization goal, but GPUI/wgpu/platform dependencies make the first native
baseline larger than the early estimate. Idle RSS is also now tracked as a
baseline, but it needs repeated samples and comparison against reference clients
before it can drive optimization decisions.

Sandbox note: GUI startup must be measured outside the file-system sandbox.
Inside the sandbox, the same release command can fail to connect to Wayland/X11
desktop sockets and surface GPUI backend errors such as `NoCompositor` or
`Failed to initialize X11 client`; those failures are measurement environment
artifacts, not the release smoke result.

## Next Validation Work

- Run live UI review for Postman, Hoppscotch, Bruno, Insomnia, and Yaak.
- Capture screenshots and version numbers for each reference.
- Add an instrumented startup/first-paint measurement instead of relying on
  timeout-based smoke.
- Repeat idle RSS sampling across several runs and after importing a large
  OpenAPI document.
- Run Linux smoke tests under Wayland and X11.
- Validate packaging dependencies for common Linux distributions.
- Compare response viewer selection/copy behavior against reference clients.

## Decision Log

- Use Zed official `gpui` and `gpui_platform`.
- Use `gpui_platform` Wayland/X11 features on Linux.
- Keep dependency requirements upgrade-friendly rather than overly pinned.
- Do not bundle font assets for GPUI.
- Treat GPUI migration as a breaking rewrite with no Slint compatibility layer.
