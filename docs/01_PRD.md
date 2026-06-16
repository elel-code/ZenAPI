# ZenAPI PRD Summary

## Product Positioning

ZenAPI is a local-first developer tool that combines an API testing client with
a local mock server. It is built around Rust and Slint, with the goal of becoming
a focused post-Postman API workstation: native, private, offline-friendly, and
simple.

Visual and interaction decisions are tracked in [DESIGN.md](DESIGN.md) so the
product can keep improving without drifting back to unstyled toolkit defaults.

The UI design is aligned with the Nexus API design system (`stitch_nextgen_api_studio/`),
a dark-themed "Geek Modernity" aesthetic optimized for developer tooling. All
visual tokens — colors, typography, spacing, elevation — follow this system.

## Framework And Compatibility Policy

The desktop UI is Slint, using the official `slint` crate (v1) and `slint-build`
for compile-time `.slint` file processing. There is no GPUI compatibility layer.

- UI is defined declaratively in `.slint` files.
- Rust modules own business logic, state management, and domain models.
- Slint `global` singletons and `callback` mechanisms bridge Rust ↔ UI.
- Do not introduce GPUI adapters, compatibility shims, or hybrid rendering paths.
- Keep reusable product logic in Rust modules such as OpenAPI parsing, request
  transport, and the mock server.
- Documentation, examples, and implementation notes should describe the Slint
  architecture as the current application architecture.

## Core Problems

1. Mandatory login and cloud sync create privacy, reliability, and workflow
   friction for local development.
2. Frontend and backend API workflows are split: frontend developers often need
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
- Display response status code and formatted response body.
- Render JSON responses in a readable form.

### 3. One-Click Local Mock Server

- Provide a global UI switch to start or stop the local mock server.
- Enable permissive CORS by default for local frontend development.
- Generate mock routes from imported OpenAPI paths.
- Return JSON responses based on OpenAPI response schemas where possible.

### 4. Local Workstation Features

- Manage global and environment variables with `{{variable}}` replacement.
- Store requests in local collections.
- Import/export native ZenAPI collection JSON and Postman Collection v2.1 JSON.
- Record request history locally and restore prior requests.
- Generate request snippets for cURL, Python, JavaScript, Rust, and Go.
- Run all requests in the current collection sequentially from the UI or
  `zenapi run`.

## Non-Functional Requirements

- Support Windows, macOS, and Linux through Rust and Slint.
- Keep the UI minimal, direct, and local-first: no ads, no forced accounts, no
  cloud dependency, and no unnecessary configuration surface.
- Follow the Nexus API dark theme design system: deep charcoal background,
  Indigo primary, Mint secondary, Inter + JetBrains Mono typography.

## Initial Acceptance Criteria

- A user can import a valid OpenAPI or Swagger file from disk.
- The app lists parsed routes by path and HTTP method.
- Selecting a route fills the request method and URL fields.
- A user can filter imported routes by method, path, or summary.
- The user can send a request and see status, timing, and response body.
- The user can start the mock server and call generated local mock endpoints
  from a browser or frontend dev server without CORS errors.
- The user can save requests to a local collection and import/export Postman
  Collection v2.1 JSON.
- The user can run a collection sequentially and inspect pass/fail summaries.

## Backlog

- OAuth2 authorization flow.
- Pre-request scripts and response tests.
- Parallel collection runner scheduling and richer runner reports.
- GraphQL, WebSocket, SSE, and gRPC protocol workspaces.
- Plugin system for auth, template tags, and future UI extension points.
- Release packaging and cross-platform validation.

## Suggested Build Order

1. Scaffold the Rust + Slint application shell with `slint` and `slint-build`.
2. Establish the global Slint theme: color tokens, typography, spacing from
   the Nexus API design system.
3. Implement OpenAPI and Swagger file import.
4. Parse paths and methods into an internal route model.
5. Render the route tree in Slint.
6. Wire route selection to the request editor.
7. Add request sending and response rendering.
8. Add the local mock server with permissive CORS.
9. Add schema-based mock response generation.
10. Maintain local collections, variables, history, code generation, and runner
    workflows.
11. Add scripts/tests and additional protocols only after core REST workflows
    stay stable.
