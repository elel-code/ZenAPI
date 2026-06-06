# ZenAPI

ZenAPI is a fast, lightweight, local-first API workstation that combines an API
testing client with a local mock server.

For the current product direction and MVP scope, see [docs/PRD.md](docs/PRD.md).
For visual and interaction iteration guidelines, see
[docs/DESIGN.md](docs/DESIGN.md).

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

Enter a local OpenAPI file path in the top bar, import it, then select a route.
The mock server starts on `http://127.0.0.1:8080` by default.

## Project Layout

- `ui/`: Slint UI files.
- `ui/fonts/`: bundled UI fonts registered as `Zen Sans` and `Zen Mono`.
- `src/app.rs` and `src/app/`: desktop application wiring and runtime state.
- `src/openapi.rs` and `src/openapi/`: OpenAPI parsing, route extraction, and
  schema-based mock data generation.
- `src/client.rs` and `src/client/`: API client transport and response
  formatting.
- `src/mock_server.rs` and `src/mock_server/`: local mock server and CORS route
  handling.
