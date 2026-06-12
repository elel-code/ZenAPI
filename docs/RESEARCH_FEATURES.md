# Feature Research Baseline

> Status: offline baseline, pending live review with current product versions.

## Feature Matrix

| Area | Reference Pattern | ZenAPI Current Direction |
|------|-------------------|--------------------------|
| Collections | Folder/request tree, import/export, Postman compatibility | Native JSON plus Postman v2.1 import/export; Bru-style export remains future work |
| Variables | Global/environment scopes with `{{var}}` replacement | Global plus dynamic environment variables implemented |
| Environments | Named environments, quick switching, per-env values | `dev`, `test`, `prod` seed environments plus custom create/delete |
| Auth | Bearer, Basic, API Key, OAuth2, JWT, Digest variants | Bearer, Basic, API Key, JWT implemented; OAuth2 remains future work |
| Headers/Params | Table editor, presets, bulk import | Key/value editor plus bulk header copy/paste |
| Body | none, form-data, urlencoded, raw, binary, GraphQL | none, form-data, urlencoded, raw, binary implemented |
| Response | Pretty/Raw/Headers, status/time/size, copy/select | Pretty/Raw/Headers, status/time/size, read-only selectable response text |
| History | Auto-record, search, restore, delete/clear | Implemented with sidebar filter and restore/delete/clear |
| Mock | Cloud mock or local mock servers | Local Axum mock server with permissive CORS |
| Codegen | cURL baseline plus language snippets | cURL, Python, JavaScript, Rust, Go snippets |
| Tests/Scripts | Pre-request scripts and response assertions | Native response assertions implemented and runner-integrated; script editor and `pm.*` compatibility remain future work |
| Runner | Collection runner with environment selection | Sequential runner core, GPUI Run All entry, and `zenapi run` CLI implemented; parallel scheduling remains future work |
| Protocols | REST plus GraphQL, WebSocket, gRPC, SSE | REST/OpenAPI MVP; other protocols future work |
| Plugins | Auth/template/UI extension points | Future work |
| AI | Request generation, docs, test help | Future exploration |

## Collection Direction

- Keep Postman Collection v2.1 as the broad interchange format.
- Keep native JSON as the simplest internal persistence format for now.
- Add Bru-style export before exact Bru import compatibility.
- Avoid claiming exact Bruno compatibility until round-trip tests exist.

## Variables Direction

- Keep the replacement syntax simple and explicit: `{{variableName}}`.
- Environment values should override globals.
- Unknown variables should produce user-visible errors rather than silently
  sending broken requests.

## Auth Direction

- Existing auth types cover the common MVP path.
- OAuth2 requires token acquisition flows, redirect handling, refresh state,
  secret storage, and UI affordances; keep it as a later dedicated stage.

## Scripts And Tests Direction

Reference clients expose scripting as a major power-user feature, but it has a
large trust and execution-model surface.

ZenAPI should not bolt scripts into request sending until these decisions are
explicit:

- JavaScript engine or Rust-native assertion DSL.
- Sandbox and filesystem/network permissions.
- Collection runner output model.
- Environment variable mutation model.

## Multi-Protocol Direction

REST/OpenAPI should stay the core until request builder, collections, and
history are stable. GraphQL is the nearest next protocol because it can reuse
HTTP transport. WebSocket, gRPC, SSE, MQTT, and Socket.IO need dedicated
connection/session views.

## Live Review Checklist

- Re-check current auth type coverage across all five reference clients.
- Verify which clients still support local-only history and collection storage.
- Compare collection runner result UIs.
- Compare GraphQL and WebSocket request editing flows.
