# Bru-Inspired Collection Format Notes

> Status: exploration note. ZenAPI does not implement a `.bru` parser yet.

## Context

ZenAPI currently stores collections as native JSON and can import/export Postman
Collection v2.1 JSON. A Bru-style text format is attractive because it is
folder-oriented, readable in diffs, and easy to review in Git.

This should be treated as a future storage option, not as a compatibility layer
for any legacy ZenAPI UI. The GPUI application state and the native collection
model remain the source of truth.

## Proposed Mapping

| ZenAPI model | Bru-inspired storage |
|--------------|----------------------|
| Collection | Directory with `collection.json` metadata |
| Folder | Nested directory |
| Request | One request file per endpoint |
| Method + URL | Top-level request block |
| Headers | `headers` block |
| Query params | `params` block |
| Auth | `auth` block, normalized from request builder state |
| Body | `body` block with mode-specific sections |

## Recommended Direction

- Keep native JSON as the default until collection editing is more complete.
- Add Bru-style export before import, because export is deterministic and lower
  risk.
- Avoid exact Bruno compatibility claims until live compatibility tests exist.
- Preserve Postman JSON import/export as the broad interchange format.
- Keep one request per file when this lands; a monolithic text file would lose
  most Git-friendly benefits.

## Open Questions

- Exact syntax compatibility target: Bruno-compatible `.bru` vs ZenAPI-specific
  Bru-inspired text.
- How to represent binary body paths and multipart file fields without losing
  platform portability.
- Whether variables should live beside the collection or remain in separate
  environment files.
- How to round-trip comments and ordering if users edit text files manually.
