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

The main window is a single Slint shell with a fixed top bar, global navigation,
page content, and a bottom status bar. Wide windows use the left navigation for
Dashboard, Requests, Mocks, Runner, Environments, Analytics, API Docs, API Keys,
Team, Settings, and Codegen. Compact widths hide the global navigation and show
bottom navigation for the primary pages.

The Requests page keeps the three-pane request builder: collection/history
sidebar, request editor, and response viewer. Long lists, code panes, and
narrow-window page content use Slint scrollbars instead of clipping overflow.
Reference pages only expose controls that perform a visible action; API Docs
snippet language buttons switch the displayed sample locally.

## Keyboard Shortcuts

Press Enter in the request URL field to send the current request. Native text
editing shortcuts still work inside editable fields.

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

1. Open the top-left menu in the app bar.
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
- Cookies derived from `Set-Cookie` response headers.

Pretty JSON responses use the formatted body from the transport layer. Raw
responses preserve the original response text. Response text is read-only but
selectable and copyable. The response toolbar can copy the active tab, format
valid JSON in Pretty or Raw, and fold/open the response viewer. Selection-aware
copy and richer structural folding are still future work.

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
Slint panel uses editable rows with `Kind`, `Target`, and `Expect` fields plus
`Add` and `Del` controls. Supported kinds include:

- `status_equals`: status equals.
- `status_in_range`: status is within a range.
- `header_exists`: header exists.
- `header_equals`: header equals.
- `body_contains`: body contains text.
- `json_path_equals`: JSON path equals a value.

For `json_path_equals` assertions, use dot paths such as `data.items.0.id`;
expected values can be JSON literals such as `true`, `42`, or `"name"`. Kind
cycling and richer assertion builders are still future work.

Tests run when a request is sent and when a collection is run. Tests are saved
with collection requests and restored with them.

## Params And Headers

Params and headers use editable rows with add/delete controls. Values are
stored as line-based text so they stay compatible with the transport parser.

Params accept one `key=value` pair per line. Headers accept common formats:

```text
Accept: application/json
Authorization=Bearer token
X-Trace-Id=abc
X-Mode: test
```

Header presets add or update common values: `Accept`, `Content`, and `Bearer`.
Use `Copy` in the Headers toolbar to copy the current header lines.

## Request Body

Supported body modes:

- `none`
- `form-data`
- `x-www-form-urlencoded`
- `raw`
- `graphql`
- `binary`

The Body toolbar exposes `none`, `form-data`, `urlenc`, `raw`, `graphql`, and
`binary`. Form-data and URL-encoded modes use editable rows with add/delete
controls and store one `key=value` pair per line. Form-data file fields use an
`@path` prefix.

Raw mode uses the code editor and exposes `JSON`, `Text`, and `XML` subtype
buttons. They send raw bodies as `application/json`, `text/plain`, and
`application/xml`, and saved collection/history entries restore the same
subtype. GraphQL mode builds a payload with `query` and `variables`; use
`Query`, `Mutation`, and `Intro` to fill starter GraphQL documents,
introspection query text, and matching variables. Binary mode accepts a local
file path. Dedicated form file picker controls and GraphQL schema response
helpers are still future work.

## WebSocket

Open the Requests page, select the Realtime tab, and choose `WebSocket`. Use
`Open` to establish a persistent `ws://` or `wss://` session, `Send` to send
messages repeatedly, and `End` to close the connection. Request headers are
sent during the handshake, and comma- or newline-separated subprotocols are
sent when opening a persistent session. `Text` sends normal text messages.
`Binary` sends hex bytes such as `00 01 ff` or `0x00,0x01,0xff` over an open
session. Sent and received messages are mirrored in the response viewer and in
the WebSocket history panel. `Copy` places the current WebSocket history on the
clipboard, and `Clear` removes it from the panel.

## SSE

Open the Requests page, select the Realtime tab, and choose `SSE`. The SSE
panel works with `http://` or `https://` `text/event-stream` endpoints. Use
`Once` for a bounded preview, `Stream` for a background stream, and `Stop` to
cancel the active subscription. SSE headers are sent on both preview fetches
and subscriptions. Event names, ids, reconnect attempts, close reasons, and
errors are mirrored in the response viewer. The `Last-Event-ID` field is sent
when starting a stream, and it is updated when incoming events include an id so
the next subscription can resume from that cursor. Subscriptions reconnect
automatically with backoff until stopped. The SSE panel keeps the latest event
history, `Copy` places it on the clipboard, and `Clear` removes it when no
stream is active.

## Authorization

Supported auth modes:

- None
- Bearer
- Basic
- JWT
- API key header
- API key query

Bearer and JWT modes send `Authorization: Bearer <token>`. Basic auth exposes
separate username and password fields, then saves them as the existing
`username:password` config string. API key modes use editable add/delete rows
for header or query pairs, while still storing the saved config as line-based
`key=value` text. The Slint auth panel changes its label, placeholder, and
helper text for the selected mode while preserving the same saved config
format. OAuth token acquisition, redirect handling, refresh, and secure state
storage remain future work.

## Vars And Envs

ZenAPI supports `{{variableName}}` replacement in URLs, query params, headers,
auth values, and body fields.

Var scopes:

- `Global`.
- `Env` for the active env.

Use the `dev`, `test`, `prod`, and `local` rows to switch the active
environment quickly, or edit the env name field directly. The Environments page
shows editable variable rows with `Global` and `Env` scopes, row add/delete
actions, and a masked JSON preview. The backing storage is still line-based
`key=value` text, so env variables override globals with the same name. Full
multi-environment list management is still future work.

## Collections

Collections organize requests into folders and requests.

Supported actions:

- Import native ZenAPI JSON or Postman Collection v2.1 JSON.
- Export native ZenAPI JSON.
- Export Postman Collection v2.1 JSON with `PM`.
- Save the current request to the collection.
- Restore a collection request into the request builder.
- Rename the selected flat-list request with the request name field and `Ren`.
- Duplicate a saved request from the flat sidebar list with `Dup`.
- Delete a saved request from the flat sidebar list with `Del`.

Native JSON is the current default storage format. Bru-style text export is
planned as a future Git-friendly option. Folder tree editing and drag/drop
controls are still future work.

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

The mock server transport and log storage are available in the Rust domain
layer. Use the Mocks page for endpoint selection, server start/stop, generated
response preview, routing overview, traffic filtering, log clearing, and log
export.

Behavior:

- Runs on the configured local mock port shown in the top bar.
- Enables permissive CORS for local frontend development.
- Serves generated JSON responses from OpenAPI schemas and examples.
- Shows the selected endpoint method/path and generated mock response body in
  the Mocks page.
- Shows default and fallback routing cards; editing conditional rules is still
  future work.
- Records recent mock requests in the Mock Log panel.
- Filters mock logs by method, path, or status.
- Clears the current in-memory mock log list with `Clear`.
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

Code generation is implemented in the domain layer and exposed through the
Codegen page. The supported snippet targets are:

- `cURL`
- `Py` for Python requests
- `JS` for JavaScript fetch
- `Rust` for Rust reqwest
- `Go` for Go net/http

Use `Generate` to refresh the snippet from the current request, `Copy` to place
the generated snippet on the clipboard, and `Save` to write it to the configured
path. The Codegen page also shows generated snippet metadata: target language,
request method and URL, line/byte counts, header count, query parameter count,
and body mode.

## Collection Runner

The command-line runner executes every request in the current collection
sequentially. The Runner page exposes Slint controls for stop-on-failure mode,
delay, run, cancel, result review, and report export. Choose Text or JSON in
the report panel, enter a target path, and save the latest completed run.

HTTP 2xx and 3xx responses are treated as passing when no tests are defined.
Native ZenAPI collection JSON can include response assertions; when assertions
exist, all of them must pass for the request to pass. Assertion results are
shown in runner summaries. `pm.test` compatibility is still future work.

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
- GraphQL query and variables payload editing plus query, mutation, and
  introspection templates are available; introspection response schema/field
  panels are future work.
- WebSocket one-shot sends, persistent WS text sessions, WebSocket history
  copy/clear, SSE `Once` previews, and persistent SSE stream/resume controls are
  available.
- gRPC Realtime draft validation is available for endpoint, method, metadata,
  JSON message, and optional manual method catalog entries such as
  `unary demo.Users/GetUser demo.GetUserRequest demo.GetUserResponse`.
  Reflection/proto descriptor loading and unary transport are future work.
- Plugin APIs are future work.
