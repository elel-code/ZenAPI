# ZenAPI

A local-first API workstation built with Rust and Slint, combining an API
testing client with a local mock server in a single native executable.

## Features

- **OpenAPI / Swagger import** — load local JSON or YAML specs, parse routes,
  and build an interactive API tree.
- **HTTP client** — send requests with full method, header, query param,
  body, and authorization support through `reqwest`.
- **Response viewer** — formatted JSON, raw text, response headers, and status
  code.
- **Local mock server** — one-click Axum server with permissive CORS and
  schema-derived JSON responses, ideal for frontend development.
- **Environments & variables** — global and per-environment variables with
  `{{name}}` syntax replacement across URL, headers, and body.
- **Collections** — organize requests into folders, import/export native
  ZenAPI and Postman Collection v2.1 JSON, save current requests from the
  sidebar, and rename, duplicate, delete, or move saved requests.
- **Request history** — automatic local history with search and one-click
  restore.
- **Code generation** — generate cURL, Python, JavaScript, Rust, and Go
  snippets from any request.
- **Rust + Slint desktop** — native desktop shell using the Slint UI
  framework with a dark-themed "Geek Modernity" design system.

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
├── ui/                         # Slint .slint UI files
│   ├── app.slint               # Application shell and main layout
│   ├── request_builder_page.slint
│   ├── app_auxiliary_pages.slint
│   └── theme.slint             # Global color/spacing/typography tokens
├── src/
│   ├── main.rs                 # Slint application entry point
│   ├── lib.rs                  # Library root
│   ├── app.rs                  # Slint app state, actions, and workflow wiring
│   ├── openapi.rs              # OpenAPI module umbrella
│   ├── openapi/model.rs        # Parsed route and schema models
│   ├── openapi/parser.rs       # OpenAPI 3.0 / Swagger 2.0 file parser
│   ├── openapi/json.rs         # JSON format handler
│   ├── openapi/yaml.rs         # YAML format handler
│   ├── openapi/schema.rs       # Schema-to-mock-data generation
│   ├── client.rs               # HTTP client module umbrella
│   ├── client/transport.rs     # reqwest request transport
│   ├── client/response.rs      # Response formatting
│   ├── mock_server.rs          # Mock server module umbrella
│   ├── mock_server/server.rs   # Axum server lifecycle
│   ├── mock_server/routing.rs  # Dynamic mock route generation
│   ├── collections.rs          # Collection tree and Postman import/export
│   ├── variables.rs            # Variable storage and interpolation
│   ├── history.rs              # Request history model and filtering
│   └── codegen.rs              # Multi-language snippet generation
├── Cargo.toml
├── Cargo.lock
└── build.rs                    # slint-build compilation
```

### Key Dependencies

| Crate | Purpose |
|-------|---------|
| `slint` / `slint-build` | Declarative desktop UI with compile-time `.slint` processing |
| `reqwest` | HTTP/HTTPS client with TLS |
| `axum` / `tokio` | Local mock server (async, permissive CORS) |
| `serde_json` / `serde_yaml` | OpenAPI document parsing |

## Design System

- **Background**: deep charcoal `#13131b`
- **Primary**: Vibrant Indigo `#c0c1ff`
- **Secondary**: Cyber Mint `#4edea3` (success states, active endpoints)
- **Typography**: Inter (UI) + JetBrains Mono (code)
- **Icons**: Material Symbols Outlined
- **Layout**: 240px sidebar with dense request, response, and auxiliary panels

The active UI tokens live in `ui/theme.slint`.

## Platform Support

| Platform | Status |
|----------|--------|
| Linux (Wayland) | ✅ Primary development target |
| Linux (X11) | ✅ Supported |
| macOS | Planned |
| Windows | Planned |

## License

Unless otherwise noted, ZenAPI source code is available under the terms of
the MIT License or Apache License 2.0, at your option.
