# ZenAPI User Guide

> Status: current local guide for the Slint rewrite. Some advanced roadmap
> features such as OAuth2 token acquisition/refresh, full `pm.*` script
> compatibility, plugins, and multi-protocol sessions are still future work.

## Overview

ZenAPI is a local-first API workstation. It combines an OpenAPI/Swagger route
browser, an HTTP client, local mock server, collections, variables, request
history, code generation, and a sequential collection runner.

The desktop app is built with Slint, a declarative Rust UI framework. The UI
follows the Nexus API dark theme design system — deep charcoal background,
Indigo primary, Mint secondary, Inter + JetBrains Mono typography.

## Workbench Layout

The main window is a three-pane workbench:

- Sidebar tabs for Routes, Saved, and History.
- Request for method, URL, parameters, headers, auth, body, scripts, realtime,
  and tools.
- Response for status, Pretty/Raw/Header views, and response body text.

The two pane dividers can be dragged to resize the workbench. Dragging shows a
neutral divider preview, and the new pane widths are applied when the pointer is
released. Sidebar, Request, and Response body content each scroll independently
with ZenAPI-styled vertical scrollbars. The scrollbar gutter is reserved outside
the content area; drag the thumb or click the track to jump without activating
rows or selecting response text beneath it.

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| Enter in URL | Send request |
| Ctrl/Cmd+Enter | Send request from the current request editor context |
| Ctrl/Cmd+F | Focus the active sidebar input |
| Ctrl/Cmd+L | Focus the request URL input |
| Ctrl/Cmd+S | Save current request to the collection |
| Ctrl/Cmd+A in response | Select all response text |
| Ctrl/Cmd+C in response | Copy selected response text |

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

The Slint shell uses Inter for UI text and JetBrains Mono for code.

## Import An API Specification

1. Click `Import` in the top bar.
2. Enter a local OpenAPI or Swagger file path in the import popover.
3. Press Enter or click `Open`.
4. Parsed routes appear in the left Routes tab.
5. Use the Routes filter to narrow by method, path, or summary.

Supported inputs include JSON and YAML OpenAPI/Swagger files. Importing a new
spec stops any currently running mock server so the visible route list and
mock routes stay aligned.

## Send Requests

1. Select a route from the Routes tab or enter a URL manually.
2. Choose an HTTP method: GET, POST, PUT, PATCH, DELETE, OPTIONS, or HEAD.
3. Add query params, headers, auth, and body data as needed.
4. Click `Send` or press Enter in the URL field.

While a request is in flight, the response pane shows the pending method and URL
instead of leaving the previous response body visible.

The response pane shows:

- Status code.
- Pretty body.
- Raw body.
- Response headers.

Pretty JSON responses can be folded to a structural summary with `Fold` and
restored with `Open`. Response text is read-only but selectable and copyable.
The Response tab row also includes `Copy`, which copies the currently visible
`Pretty`, `Raw`, or `Hdrs` view. New responses and response view switches start
at the top of the response body.

## Pre-request

The request editor includes a Pre-request action line. It is a native
script-lite layer, not a JavaScript engine. Actions are separated by semicolons
or new lines in collection JSON.

Supported actions:

- `set_method VALUE`
- `set_url VALUE`
- `set_header NAME=VALUE`
- `set_query NAME=VALUE`
- `unset_header NAME`
- `unset_query NAME`
- `set_body VALUE`
- `set_var NAME=VALUE`
- `set_global NAME=VALUE`
- `set_env NAME=VALUE`
- `unset_var NAME`
- `unset_global NAME`
- `unset_env NAME`

`remove_header`/`delete_header` and `remove_query`/`delete_query` are accepted
as aliases for unset actions. `remove_var`/`delete_var`,
`remove_global`/`delete_global`, and `remove_env`/`delete_env` are also
accepted.

Pre-request actions run before `{{variable}}` replacement. They apply when a
single request is sent, when generated code is previewed, and when the
collection runner or CLI runs a saved collection request. Saved collection
requests preserve the original editor fields plus the action line in
`pre_request_script`; actions are applied at send/run time rather than being
expanded during save.

The Pre-request panel status shows the most recent action count or request
build error. The panel, collection runner summaries, and CLI output include
pre-request action names and target fields, but not action values.

## Tests

The request editor includes a native Tests panel for response assertions. The
kind selector uses compact labels for common checks:

- `Status =`: status equals.
- `Range`: status is within a range.
- `Header ?`: header exists.
- `Header =`: header equals.
- `Body ?`: body contains text.
- `JSON =`: JSON path equals a value.

Use `+` in the Tests header to add a row. Click the kind selector in a test row
to cycle through assertion types. Use the `Target` and `Expect` fields according
to the selected kind. For `JSON =` assertions, use dot paths such as
`data.items.0.id`; expected values can be JSON literals such as `true`, `42`,
or `"name"`. Use `x` at the row edge to remove a test row.

Tests run when a request is sent and when a collection is run. Tests are saved
with collection requests and restored with them.

## Headers

Headers can be edited as key/value rows. Use `+` in the table header to add
another row, and `x` at the row edge to remove one. The bulk header tools
accept common formats:

```text
Accept: application/json
Authorization=Bearer token
-H 'X-Trace-Id: abc'
--header "X-Mode: test"
```

Use `Copy` to copy the current headers as one header per line. Header presets
add or update common values: `Accept`, `Content`, and `Bearer`.

## Request Body

Supported body modes:

- `none`
- `form-data`
- `x-www-form-urlencoded`
- `raw`
- `graphql`
- `binary`

The Body toolbar uses compact labels: `Form` maps to `form-data`, and
`URL Enc` maps to `x-www-form-urlencoded`. The field editors use matching
compact titles, and their table headers include `+` for more fields.

Raw mode supports JSON, XML, Text, and HTML content types, with a compact
`Preview` for structured text. In JSON raw mode, `Format` rewrites the body as
pretty JSON when the content parses successfully. GraphQL mode builds a
`Payload` with `query` and `variables`, can fill a standard introspection
query, shows `Schema` and `Fields` panels when an introspection response is
returned, and offers `Templates` rows that can be applied back into the editor.
Form-data file fields use an `@path` prefix.

## WebSocket

The WS panel opens a persistent `ws://` or `wss://` session. Use `Open` to
establish the session, `Send` to send messages repeatedly, and `End` to close
the connection. WS headers and comma-separated protocols are sent during the
handshake. The message editor supports `Text` and `Hex` modes; `Hex` accepts
byte input such as `00 ff 7a`. Sent and received messages are recorded in the
panel, and the latest event is mirrored in the response viewer. `Open` is
enabled only for `ws://` or `wss://` URLs.
`Copy` copies the current message history as text, and `Clear` clears the panel
history.

## SSE

The SSE panel works with `http://` or `https://` `text/event-stream` endpoints.
Use `Once` for a bounded preview, `Stream` for a background stream, and `Stop`
to cancel the active subscription. `Once` and `Stream` are enabled only for
`http://` or `https://` URLs. SSE headers are sent on both preview fetches and
subscriptions. Event names, ids, and data are recorded in the panel and mirrored
in the response viewer. `Copy` copies the current event history
as text, and `Clear` clears the panel history and resume cursor. When an event
id is seen, reconnect attempts resume with `Last-Event-ID`. Subscriptions
reconnect automatically with backoff until
stopped.

## Authorization

Supported auth modes:

- None
- Bearer
- Basic
- JWT
- API key header
- API key query

Bearer and JWT modes send `Authorization: Bearer <token>`. Basic auth expects
`username:password`. API key modes expect line-based `key=value` input and place
the values in headers or query params. OAuth token acquisition, redirect
handling, refresh, and secure state storage remain future work.

## Vars And Envs

ZenAPI supports `{{variableName}}` replacement in URLs, query params, headers,
auth values, and body fields.

Var scopes:

- `Global`.
- `Env` for the active env.

Use the `dev`, `test`, and `prod` chips to switch the active environment quickly,
or edit the env name field directly. Global and env vars are edited as
line-based `key=value` text. Env variables override globals with the same name.
Full environment list management is still future work.

## Collections

Collections organize requests into folders and requests.

Supported actions:

- Import native ZenAPI JSON or Postman Collection v2.1 JSON.
- Export native ZenAPI JSON.
- Export Postman Collection v2.1 JSON with `PM`.
- Save the current request to the collection.
- Restore a collection request into the request builder.
- Duplicate a saved request from the flat sidebar list with `Dup`.
- Delete a saved request from the flat sidebar list with `Del`.

Native JSON is the current default storage format. Bru-style text export is
planned as a future Git-friendly option. Folder tree editing, rename,
duplicate, and drag/drop controls are still future work.

## History

Every sent request is recorded automatically with request details and response
summary. History is loaded from and saved to `.zenapi-history.json` in the
current working directory. The History sidebar supports:

- Search/filter.
- Restore a request.
- `Del` one entry.
- Clear all entries.

Restoring a history entry fills method, URL, query params, headers, auth config,
body mode, body preview, pre-request script, and tests from the request
snapshot.

## Local Mock Server

After importing routes, click the mock control in the top bar to start or stop
the local mock server.

Behavior:

- Runs on the configured local mock port shown in the top bar.
- Enables permissive CORS for local frontend development.
- Serves generated JSON responses from OpenAPI schemas and examples.
- Records recent mock requests in the Mock Log panel.
- Filters mock logs by method, path, or status.
- Saves the currently filtered mock log view to a local JSON file.

## Error Feedback

Import, collection import/export, request build, request send, test
configuration, WebSocket, SSE, collection runner, and mock-server failures
are shown in the Response pane with the operation context, target path
or URL when relevant, the underlying error, and a next-step hint. Collection
import/export failures also update the Collection status line, mock failures
update the Mock status line, and realtime failures update their local
panel status.

## Code Generation

The Code panel generates snippets for:

- `cURL`
- `Py` for Python requests
- `JS` for JavaScript fetch
- `Rust` for Rust reqwest
- `Go` for Go net/http

Use the language selector and `Generate` to preview the current snippet. Enter
a local path and click `Save` to export the snippet to disk.

## Collection Runner

The Runner panel executes every request in the current collection sequentially.

Controls:

- `Run`: run the current collection.
- `Stop Fail`: stop after the first failed request.

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

- OAuth2 token acquisition, redirect handling, refresh, and secure state storage
  are not implemented yet. Manual access token auth is available.
- Pre-request script-lite and native response assertions are available in
  collection JSON, but a full script engine and `pm.*` compatibility are not
  implemented yet.
- GraphQL query and variables payload editing is available; introspection query
  fill, `Schema`, `Fields`, and `Templates` are future work. WebSocket one-shot
  sends and SSE `Once` previews are available; persistent WS sessions, SSE
  stream controls, and reconnect/resume UI are future work. gRPC has a draft
  domain model and an implementation plan in `docs/09_GRPC.md`, but transport/UI
  support is future work.
- Plugin APIs are future work.
