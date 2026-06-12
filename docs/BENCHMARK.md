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

## Priority Gaps

| Gap | Reason |
|-----|--------|
| OAuth2 | Requires a dedicated token flow and secure state handling |
| Parallel collection runner | Sequential execution is implemented; parallel scheduling and richer reports remain future work |
| Scripts/tests | Major power-user feature, but requires sandbox decisions |
| GraphQL | Closest non-REST protocol extension because it can reuse HTTP |
| WebSocket/gRPC/SSE | Requires session-oriented views and transport-specific UI |
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

Measured on Linux x86_64 in this workspace with `rustc 1.96.0`.

| Metric | Current Result | Notes |
|--------|----------------|-------|
| Release build | Passed | `cargo build --release` completed successfully |
| Release binary | 37M / 37,903,920 bytes | `target/release/zenapi`, not stripped |
| Stripped estimate | 28M / 29,245,120 bytes | Measured from `/tmp/zenapi-stripped`; release artifact was not modified |

The current binary is above the rough 10 MB target. That target should remain
an optimization goal, but GPUI/wgpu/platform dependencies make the first native
baseline larger than the early estimate.

## Next Validation Work

- Run live UI review for Postman, Hoppscotch, Bruno, Insomnia, and Yaak.
- Capture screenshots and version numbers for each reference.
- Measure ZenAPI release binary size and startup time.
- Run Linux smoke tests under Wayland and X11.
- Validate packaging dependencies for common Linux distributions.
- Compare response viewer selection/copy behavior against reference clients.

## Decision Log

- Use Zed official `gpui` and `gpui_platform`.
- Use `gpui_platform` Wayland/X11 features on Linux.
- Keep dependency requirements upgrade-friendly rather than overly pinned.
- Do not bundle font assets for GPUI.
- Treat GPUI migration as a breaking rewrite with no Slint compatibility layer.
