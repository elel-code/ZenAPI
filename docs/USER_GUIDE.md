# ZenAPI User Guide

> Status: current local guide for the GPUI rewrite. Some advanced roadmap
> features such as OAuth2, full `pm.*` script compatibility, plugins, and
> multi-protocol sessions are still future work.

## Overview

ZenAPI is a local-first API workstation. It combines an OpenAPI/Swagger route
browser, an HTTP client, local mock server, collections, variables, request
history, code generation, and a sequential collection runner.

The desktop app is built with GPUI from Zed's official repository. Linux uses
`gpui_platform` with Wayland and X11 support. The old Slint prototype is not a
compatibility target.

## Install And Run

From the repository:

```sh
cargo run
```

For a release build:

```sh
cargo build --release
target/release/zenapi
```

The GPUI shell uses platform fonts. No bundled font assets are required.

## Import An API Specification

1. Enter a local OpenAPI or Swagger file path in the top import field.
2. Press Enter or click `Import`.
3. Parsed routes appear in the left Endpoints list.
4. Use the Endpoints filter to narrow by method, path, or summary.

Supported inputs include JSON and YAML OpenAPI/Swagger files. Importing a new
spec stops any currently running mock server so the visible route list and
mock routes stay aligned.

## Send Requests

1. Select a route from the Endpoints list or enter a URL manually.
2. Choose an HTTP method: GET, POST, PUT, PATCH, DELETE, OPTIONS, or HEAD.
3. Add query params, headers, auth, and body data as needed.
4. Click `Send` or press Enter in the URL field.

The response pane shows:

- Status code.
- Elapsed time.
- Response size.
- Pretty body.
- Raw body.
- Response headers.

Pretty JSON responses can be collapsed to a structural summary. Response text is
read-only but selectable and copyable.

## Pre-request

The request editor includes a Pre-request action line. It is a native
script-lite layer, not a JavaScript engine. Actions are separated by semicolons
or new lines in collection JSON.

Supported actions:

- `set_method VALUE`
- `set_url VALUE`
- `set_header NAME=VALUE`
- `set_query NAME=VALUE`
- `set_body VALUE`
- `set_var NAME=VALUE`
- `set_global NAME=VALUE`
- `set_env NAME=VALUE`

Pre-request actions run before `{{variable}}` replacement. They apply when a
single request is sent, when generated code is previewed, and when the
collection runner or CLI runs a saved collection request. Saved collection
requests preserve the original editor fields plus the action line in
`pre_request_script`; actions are applied at send/run time rather than being
expanded during save.

The Pre-request panel status shows the most recent action count or request
build error.

## Tests

The request editor includes a native Tests panel for response assertions. These
tests do not use a script engine yet, but they cover common checks:

- Status equals.
- Status in range.
- Header exists.
- Header equals.
- Body contains text.
- JSON path equals a value.
- Body size is below a threshold.
- Elapsed time is below a threshold.

Click the kind selector in a test row to cycle through assertion types. Use the
Target and Expected fields according to the selected kind. For JSON path
assertions, use dot paths such as `data.items.0.id`; expected values can be
JSON literals such as `true`, `42`, or `"name"`.

Tests run when a request is sent and when a collection is run. Tests are saved
with collection requests and restored with them.

## Headers

Headers can be edited as key/value rows. The bulk header tools accept common
formats:

```text
Accept: application/json
Authorization=Bearer token
-H 'X-Trace-Id: abc'
--header "X-Mode: test"
```

Use `Copy Bulk` to copy the current headers as one header per line.

## Request Body

Supported body modes:

- `none`
- `form-data`
- `x-www-form-urlencoded`
- `raw`
- `binary`

Raw mode supports JSON, XML, Text, and HTML content types, with a lightweight
syntax preview for structured text. Form-data file fields use an `@path`
prefix.

## Authorization

Supported auth modes:

- None
- Bearer Token
- Basic Auth
- JWT
- API Key in header or query string

OAuth2 remains a future feature because it needs token acquisition, redirect,
refresh, and secure state handling.

## Variables And Environments

ZenAPI supports `{{variableName}}` replacement in URLs, query params, headers,
auth values, and body fields.

Variable scopes:

- Global variables.
- Active environment variables.

Seed environments are `dev`, `test`, and `prod`. You can create custom
environments, switch the active environment, and delete the active environment.
Environment variables override globals with the same name.

## Collections

Collections organize requests into folders and requests.

Supported actions:

- Import native ZenAPI JSON or Postman Collection v2.1 JSON.
- Export native ZenAPI JSON.
- Export Postman Collection v2.1 JSON.
- Save the current request to the collection.
- Add folders and requests.
- Rename, duplicate, and delete collection items.
- Drag and drop collection items.
- Restore a collection request into the request builder.

Native JSON is the current default storage format. Bru-style text export is
planned as a future Git-friendly option.

## History

Every sent request is recorded automatically with request details and response
summary. The History sidebar supports:

- Search/filter.
- Restore a request.
- Delete one entry.
- Clear all entries.

## Local Mock Server

After importing routes, click the mock control in the top bar to start or stop
the local mock server.

Behavior:

- Runs on the configured local mock port shown in the top bar.
- Enables permissive CORS for local frontend development.
- Serves generated JSON responses from OpenAPI schemas and examples.
- Records recent mock requests in the Mock Log panel.

## Code Generation

The Code panel generates snippets for:

- cURL
- Python requests
- JavaScript fetch
- Rust reqwest
- Go net/http

Use the language selector and `Copy` to copy the current snippet.

## Collection Runner

The Runner panel executes every request in the current collection sequentially.

Controls:

- `Run All`: run the current collection.
- `Stop on fail`: stop after the first failed request.

Results appear in the Runner panel and the response pane summary. HTTP 2xx and
3xx responses are treated as passing when no tests are defined. Native ZenAPI
collection JSON can include response assertions; when assertions exist, all of
them must pass for the request to pass. Assertion results are shown in runner
summaries. `pm.test` compatibility is still future work.

The same runner is available from the command line:

```sh
zenapi run collection.json
zenapi run collection.json --stop-on-failure
zenapi run collection.json --delay-ms 100
```

## Current Limits

- OAuth2 is not implemented yet.
- Pre-request script-lite and native response assertions are available in
  collection JSON, but a full script engine and `pm.*` compatibility are not
  implemented yet.
- GraphQL, WebSocket, SSE, and gRPC are future protocol work.
- Plugin APIs are future work.
- Live benchmark and visual comparison against reference clients still need
  current-version review.
