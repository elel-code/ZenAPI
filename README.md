# ZenAPI

A fast, lightweight, local-first API workstation built with Rust and GPUI —
combining an API testing client with a local mock server in a single native
executable.

[Documentation](docs/) · [Design Notes](docs/DESIGN.md) · [Roadmap](docs/TODO.md)

## Features

- **OpenAPI / Swagger import** — load local JSON or YAML specs, parse routes,
  and build an interactive API tree.
- **HTTP client** — send requests with full method, header, query param,
  body, and authorization support through `reqwest`.
- **Response viewer** — formatted JSON, raw text, response headers, status
  code, elapsed time, and response size.
- **Local mock server** — one-click Axum server with permissive CORS and
  schema-derived JSON responses, ideal for frontend development.
- **Environments & variables** — global and per-environment variables with
  `{{name}}` syntax replacement across URL, headers, and body.
- **Collections** — organize requests into folders, import/export Postman
  Collection v2.1 JSON, save current requests from the sidebar, manage items
  with a right-click context menu, and move items with drag/drop.
- **Request history** — automatic local history with search and one-click
  restore.
- **Code generation** — generate cURL, Python, JavaScript, Rust, and Go
  snippets from any request.
- **Rust + GPUI desktop** — native performance, sub-second startup, small
  binary (~10 MB target), and no Chromium dependency.

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (stable, 1.80+)
- Linux: `cmake`, `pkg-config`, `libfontconfig-dev`, `libxkbcommon-dev`,
  `libwayland-dev` (Wayland), `libx11-dev` (X11)

### Build & Run

```bash
git clone https://github.com/your-org/ZenAPI.git
cd ZenAPI
cargo run
```

The application window opens. Click **Import** to load an OpenAPI file,
select a route from the sidebar, and send your first request. The mock
server starts on `http://127.0.0.1:8080`.

## Project Layout

```
ZenAPI/
├── src/
│   ├── main.rs                  # GPUI application entry point
│   ├── lib.rs                   # Library root
│   ├── app.rs                   # App state, actions, and workflow wiring
│   ├── app/input.rs             # Input widgets (text, multiline, key-value)
│   ├── openapi.rs               # OpenAPI module umbrella
│   ├── openapi/model.rs         # Parsed route and schema models
│   ├── openapi/parser.rs        # OpenAPI 3.0 / Swagger 2.0 file parser
│   ├── openapi/json.rs          # JSON format handler
│   ├── openapi/yaml.rs          # YAML format handler
│   ├── openapi/schema.rs        # Schema-to-mock-data generation
│   ├── client.rs                # HTTP client module umbrella
│   ├── client/transport.rs      # reqwest request transport
│   ├── client/response.rs       # Response formatting
│   ├── mock_server.rs           # Mock server module umbrella
│   ├── mock_server/server.rs    # Axum server lifecycle
│   ├── mock_server/routing.rs   # Dynamic mock route generation
│   ├── collections.rs           # Collection tree and Postman import/export
│   ├── variables.rs             # Variable storage and interpolation
│   ├── history.rs               # Request history model and filtering
│   └── codegen.rs               # Multi-language snippet generation
├── docs/
│   ├── PRD.md                   # Product requirements and MVP scope
│   ├── DESIGN.md                # Visual and interaction design decisions
│   ├── TODO.md                  # Development roadmap and task tracking
│   └── USER_GUIDE.md            # User guide (planned)
├── Cargo.toml
├── Cargo.lock
├── README.md                    # This file
└── README.zh-CN.md              # Simplified Chinese version
```

### Key Dependencies

| Crate | Purpose |
|-------|---------|
| `gpui` / `gpui_platform` | GPU-accelerated desktop UI (Zed's official repo) |
| `reqwest` | HTTP/HTTPS client with TLS |
| `axum` / `tokio` | Local mock server (async, permissive CORS) |
| `serde_json` / `serde_yaml` | OpenAPI document parsing |
| `syntect` (planned) | JSON syntax highlighting |

## Documentation

- [PRD](docs/PRD.md) — product requirements and MVP scope
- [DESIGN](docs/DESIGN.md) — visual and interaction guidelines
- [TODO](docs/TODO.md) — development roadmap
- [User Guide](docs/USER_GUIDE.md) — planned

## Platform Support

| Platform | Status |
|----------|--------|
| Linux (Wayland) | ✅ Primary development target |
| Linux (X11) | ✅ Supported via `gpui_platform` |
| macOS | Planned |
| Windows | Planned |

## License

Unless otherwise noted, ZenAPI source code is available under the terms of
the MIT License or Apache License 2.0, at your option.
