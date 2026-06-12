# ZenAPI

ZenAPI is a fast, lightweight, local-first API workstation that combines an API
testing client with a local mock server.

For the current product direction and MVP scope, see [docs/PRD.md](docs/PRD.md).
For visual and interaction iteration guidelines, see
[docs/DESIGN.md](docs/DESIGN.md).

## Framework Direction

ZenAPI's desktop UI is built with GPUI from Zed's official repository. Linux
builds use `gpui_platform` with Wayland and X11 features enabled. The previous
Slint prototype was removed as a breaking replacement, so there is no
compatibility surface for Slint files, generated UI modules, callback names,
binding-layer shapes, or build scripts.

## Current MVP

- Import OpenAPI / Swagger documents from local JSON or YAML files.
- Parse paths and HTTP methods into the route list.
- Filter imported routes by method, path, or summary.
- Select a route to prefill the request method and local mock URL.
- Send HTTP and HTTPS requests through `reqwest`.
- Display status code, elapsed time, and formatted JSON response bodies.
- Start and stop a local Axum mock server with permissive CORS enabled.
- Return schema-derived JSON mock responses when response schemas are available.

## Run

```bash
cargo run
```

Use `Import` to enter a local OpenAPI file path, then select a route. The mock
server starts on `http://127.0.0.1:8080` by default.

## Project Layout

- `src/app.rs` and `src/app/`: GPUI application shell, input widgets, runtime
  state, and workflow wiring.
- `src/openapi.rs` and `src/openapi/`: OpenAPI parsing, route extraction, and
  schema-based mock data generation.
- `src/client.rs` and `src/client/`: API client transport and response
  formatting.
- `src/mock_server.rs` and `src/mock_server/`: local mock server and CORS route
  handling.
