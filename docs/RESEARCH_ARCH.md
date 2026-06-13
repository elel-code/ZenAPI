# Architecture Research Baseline

> Status: offline baseline, pending live review with current product versions.

## Framework Comparison

| Stack | Example Clients | Strength | Cost |
|-------|-----------------|----------|------|
| Electron + React/JS | Postman, Bruno, Insomnia | Mature UI ecosystem and web reuse | Browser runtime and app shell complexity |
| Vue/Nuxt/PWA | Hoppscotch | Easy web/self-host path, very low install friction | Browser/PWA constraints for native local workflows |
| Tauri + Rust + Web UI | Yaak | Native packaging with web UI reuse | Still depends on webview behavior and frontend bridge |
| GPUI + Rust | ZenAPI | Native Rust UI, direct state ownership, no browser runtime | Smaller ecosystem, more custom widgets required |

## ZenAPI Architecture Policy

- GPUI comes from Zed's official repository.
- Linux uses `gpui_platform` with Wayland and X11 features enabled.
- The GPUI rewrite is a breaking replacement of the Slint prototype.
- Do not keep adapter layers solely for Slint-era callback names, generated UI
  modules, file layout, or binding shapes.
- Do not bundle font assets for the GPUI shell unless a future design pass
  introduces a concrete need.
- Keep dependency versions upgrade-friendly. Prefer broad compatible
  requirements such as `1`, `0.12`, or git dependencies without pinning to an
  unnecessarily narrow revision unless reproducibility requires it.

## Current Dependency Posture

| Dependency | Policy |
|------------|--------|
| `gpui` | Git dependency from `https://github.com/zed-industries/zed.git`, package `gpui` |
| `gpui_platform` | Same Zed repo, package `gpui_platform`, features `wayland` and `x11` |
| `reqwest` | HTTP transport, rustls TLS, multipart and JSON enabled |
| `axum` | Local mock server |
| `serde`, `serde_json`, `serde_yaml` | Data model and import/export parsing |
| `tokio` | Async runtime for client and mock server tasks |

## Runtime Model

- GPUI owns application state and event flow.
- Domain modules remain plain Rust where possible: OpenAPI parsing,
  collections, variables, history, codegen, mock routing, and HTTP transport.
- UI-specific state lives in the GPUI app entity.
- Long-running request/mock work runs through Tokio and posts results back to
  GPUI.

## Storage Direction

- Use local files and local state first.
- Native collection JSON is the current default.
- Postman JSON is the interoperability format.
- Bru-style text export is a future Git-friendly storage path.
- History and environments should remain local unless explicit sync support is
  designed later.

## Risks

- GPUI APIs can move because the dependency is sourced from Zed's active repo.
- Linux packaging needs explicit Wayland/X11 dependency validation.
- More custom GPUI widgets increase maintenance cost; isolate them when they
  represent reusable behavior, as with read-only selectable response text.
