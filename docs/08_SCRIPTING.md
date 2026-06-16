# Scripting And Tests Evaluation

> Status: stage 10.1 evaluation plus native pre-request action plumbing and
> native response assertion plumbing.
> No scripting dependency has been added yet.

## Goal

ZenAPI should eventually support pre-request scripts and response tests for
collection workflows. The scripting model must fit a local-first native app:
predictable, sandboxed, testable, and maintainable.

## Requirements

| Requirement | Notes |
|-------------|-------|
| Pre-request mutation | Scripts should be able to set variables and prepare request data before sending |
| Response assertions | Tests should inspect status, headers, body text, and JSON |
| Collection runner integration | Test results should attach to each runner result |
| Determinism | Scripts should not silently depend on cloud or hidden global state |
| Sandboxing | Filesystem, network, process, and environment access must be denied by default |
| Upgrade-friendly dependencies | Avoid tight version pins unless the engine API requires it |

## Engine Options

| Engine | Strengths | Costs | Fit |
|--------|-----------|-------|-----|
| Rhai | Rust-native, small, easy to embed, controllable scope | Not JavaScript; Postman-like `pm.*` compatibility would be an adaptation layer | Best first implementation candidate if ZenAPI accepts a native script dialect |
| mlua | Mature embeddable Lua, small runtime, good sandbox control | Lua syntax diverges from common API-client scripts; `pm.test` compatibility is unnatural | Reasonable technically, weaker product fit |
| deno_core | JavaScript/TypeScript-compatible path, closer to Postman/Hoppscotch mental model | More complex permissions and embedding surface | Best compatibility candidate, but higher integration complexity |

## Recommendation

Use a staged strategy:

1. Define the script host API and test result model before adding an engine.
2. Implement response assertion plumbing in Rust first so runner/reporting does
   not depend on an engine choice.
3. Prefer Rhai for the first embedded engine if the priority is predictable
   sandboxing and a Rust-native host API.
4. Revisit `deno_core` only when JavaScript compatibility becomes a hard
   requirement.
5. Avoid `mlua` unless Lua syntax becomes a deliberate product choice.

## Proposed Host API

The host should expose a narrow context:

| Name | Capability |
|------|------------|
| `request.method` | Read/write request method before send |
| `request.url` | Read/write request URL before send |
| `request.headers` | Read/write headers |
| `request.query` | Read/write query params |
| `request.body` | Read/write body text for supported modes |
| `variables.get(name)` | Read variable |
| `variables.set(name, value)` | Set current variable |
| `response.status` | Read response status in tests |
| `response.headers` | Read response headers in tests |
| `response.body` | Read response raw body in tests |
| `response.json()` | Parse response body as JSON |
| `test(name, fn)` | Record a named assertion |
| `expect(value)` | Assertion helper |

For JavaScript compatibility later, these can be mapped to a `pm` facade:

```js
pm.test("status is 200", () => {
  pm.expect(pm.response.status).to.equal(200)
})
```

The first implementation does not need to promise exact Postman compatibility.
Exact compatibility should only be claimed after dedicated compatibility tests.

## Result Model

Each executed request should be able to produce:

| Field | Meaning |
|-------|---------|
| Request path | Collection/folder/request path |
| Script phase | Pre-request or test |
| Test name | Named assertion |
| Passed | Boolean result |
| Error | Assertion or runtime error |
| Duration | Script/test execution time |

Collection runner summaries should count request transport failures separately
from assertion failures.

## Implemented Pre-request Script-Lite

ZenAPI has a Rust-native pre-request action line that runs before variable
replacement and request transport. It is deliberately not a general-purpose
script engine and does not claim Postman `pm.*` compatibility.

Supported actions:

| Action | Effect |
|--------|--------|
| `set_method VALUE` | Replace request method |
| `set_url VALUE` | Replace request URL |
| `set_header NAME=VALUE` | Upsert a request header, case-insensitively by name |
| `set_query NAME=VALUE` | Upsert a query parameter |
| `unset_header NAME` | Remove matching request headers, case-insensitively by name |
| `unset_query NAME` | Remove matching query parameters by exact name |
| `set_body VALUE` | Replace the raw request body, promoting empty/non-raw bodies to text |
| `set_var NAME=VALUE` | Set active-environment variable when one is active, otherwise global |
| `set_global NAME=VALUE` | Set a request-local global variable override |
| `set_env NAME=VALUE` | Set a request-local active-environment variable override |
| `unset_var NAME` | Remove the active-environment variable when one is active, otherwise global |
| `unset_global NAME` | Remove a request-local global variable |
| `unset_env NAME` | Remove a request-local active-environment variable |

`remove_header` / `delete_header` and `remove_query` / `delete_query` are
accepted aliases for unset actions. `remove_var` / `delete_var`,
`remove_global` / `delete_global`, and `remove_env` / `delete_env` are also
accepted.

Actions can be separated by semicolons or new lines in collection JSON. The Slint
editor currently uses a compact single-line input. Saved collection requests use
`pre_request_script`; the field is omitted when empty and defaults to an empty
string for older collection files. Saving preserves the raw editor request and
the action line, so actions run when the request is built or executed rather
than being baked into the collection at save time.

Execution behavior:

- Single request sending, code generation preview, the Slint collection runner,
  and the CLI runner all execute the same Rust action evaluator.
- Action logs record the action name and target field, not the configured
  value, so bearer tokens and other secrets are not echoed into summaries.
- Slint runner summaries and CLI output include pre-request action counts or
  action target lines for saved collection requests.
- Variable mutations are request-local for build/runner execution and do not
  persist back into stored environment/global variable rows.
- Invalid actions fail request construction before transport.

## Implemented Native Assertions

ZenAPI now has a Rust-native response assertion model that can be stored on
collection requests and evaluated by the collection runner before any scripting
engine is embedded.

Supported assertion kinds:

| Assertion | Purpose |
|-----------|---------|
| `status_equals` | Exact status match |
| `status_in_range` | Inclusive status range |
| `header_exists` | Case-insensitive response header presence |
| `header_equals` | Case-insensitive response header value comparison |
| `body_contains` | Raw body substring check |
| `json_path_equals` | Dot-path JSON value comparison, including numeric array indexes |

Runner behavior:

- If a request has no assertions, HTTP 2xx/3xx is treated as success.
- If a request has assertions, all assertions must pass for the request to
  pass. This allows expected error responses such as status 500 to be modeled
  explicitly.
- Assertion failures appear in runner summaries and response summary text.
- The Slint request editor includes a native Tests panel that configures these
  assertions without a script engine.
- Saving a request to a collection preserves configured assertions, and
  restoring a collection request brings them back into the Tests panel.

Example native collection JSON fragment:

```json
{
  "name": "Health",
  "method": "GET",
  "url": "http://localhost:8080/health",
  "headers": [],
  "query_params": [],
  "body": { "mode": "none" },
  "tests": [
    {
      "name": "status is 200",
      "kind": "status_equals",
      "status": 200
    },
    {
      "name": "ok flag",
      "kind": "json_path_equals",
      "path": "ok",
      "value": true
    }
  ]
}
```

## Security Defaults

- No filesystem access by default.
- No process spawning.
- No arbitrary network access from scripts.
- No environment-variable access except values explicitly exposed by ZenAPI.
- Execution timeout per script.
- Output/log buffer limit.
- Explicit future permission model if plugins/scripts need broader powers.

## Open Questions

- Whether script source belongs inside collection files, alongside requests, or
  in separate files.
- Whether collection-level scripts should inherit into folders/requests.
- Whether failed pre-request scripts should count as skipped request transport
  or failed request execution.
- Whether variables mutated during a collection run should persist after the
  run or remain run-local by default.
