# ZenAPI PRD Summary

## Product Positioning

ZenAPI is a fast, lightweight, local-first developer tool that combines an API
testing client with a local mock server. It is built around Rust and GPUI, with
the goal of becoming a focused post-Postman API workstation: native, private,
offline-friendly, and simple.

Visual and interaction decisions are tracked in [DESIGN.md](DESIGN.md) so the
product can keep improving without drifting back to unstyled toolkit defaults.

## Framework And Compatibility Policy

The desktop UI is GPUI, using Zed's official repository. Linux support goes
through `gpui_platform` with Wayland and X11 features. The former Slint
implementation was prototype code and is not a compatibility contract.

- The GPUI rewrite is a breaking replacement of the old application shell.
- Do not reintroduce Slint UI files, generated UI modules, callback names,
  binding-layer shapes, or build scripts for backwards compatibility.
- Prefer deleting obsolete toolkit-specific code over adding adapters or
  compatibility shims.
- Keep reusable product logic in Rust modules such as OpenAPI parsing, request
  transport, and the mock server only when it fits the GPUI architecture
  cleanly.
- Documentation, examples, and future implementation notes should describe the
  GPUI architecture as the current application architecture.

## Core Problems

1. Heavy API clients are slow to start and costly in memory because they often
   depend on Electron or browser runtimes.
2. Mandatory login and cloud sync create privacy, reliability, and workflow
   friction for local development.
3. Frontend and backend API workflows are split: frontend developers often need
   a separate Node.js mock stack, while backend developers still need a simple
   client to test freshly written endpoints.

## MVP Scope

### 1. OpenAPI / Swagger Driven Engine

- Import local `openapi.json`, `openapi.yaml`, or `swagger.yaml` files.
- Support OpenAPI 3.0 and Swagger 2.0 as the initial compatibility target.
- Parse paths and HTTP methods.
- Generate a visible API tree in the left sidebar.

### 2. Minimal API Client

- Select an imported endpoint or manually enter a URL.
- Support common HTTP methods: `GET`, `POST`, `PUT`, `PATCH`, and `DELETE`.
- Send requests through `reqwest`.
- Display response status code, elapsed time, and formatted response body.
- Render JSON responses in a readable form.

### 3. One-Click Local Mock Server

- Provide a global UI switch to start or stop the local mock server.
- Enable permissive CORS by default for local frontend development.
- Generate mock routes from imported OpenAPI paths.
- Return JSON responses based on OpenAPI response schemas where possible.

## Non-Functional Requirements

- Produce a small native executable, targeting roughly 10 MB where practical.
- Keep runtime memory usage far below Chromium-based tools.
- Support Windows, macOS, and Linux through Rust and GPUI.
- Keep the UI minimal, direct, and local-first: no ads, no forced accounts, no
  cloud dependency, and no unnecessary configuration surface.

## Initial Acceptance Criteria

- A user can import a valid OpenAPI or Swagger file from disk.
- The app lists parsed routes by path and HTTP method.
- Selecting a route fills the request method and URL fields.
- A user can filter imported routes by method, path, or summary.
- The user can send a request and see status, timing, and response body.
- The user can start the mock server and call generated local mock endpoints
  from a browser or frontend dev server without CORS errors.

## Backlog

- Environment profiles such as `dev`, `test`, and `prod`.
- Variables such as `baseURL`.
- JSON syntax highlighting and folding, potentially through `syntect`.
- Schema-aware fake data generation with libraries such as `fake-rs`.
- Smarter field-name heuristics for generated values such as `email`, `name`,
  `phone`, and `avatar`.

## Suggested Build Order

1. Maintain the Rust + GPUI application shell backed by Zed's official
   `gpui` and `gpui_platform` crates.
2. Implement OpenAPI and Swagger file import.
3. Parse paths and methods into an internal route model.
4. Render the route tree in GPUI.
5. Wire route selection to the request editor.
6. Add request sending and response rendering.
7. Add the local mock server with permissive CORS.
8. Add schema-based mock response generation.
