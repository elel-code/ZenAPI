# Interaction Research Baseline

> Status: offline baseline, pending live review with current product versions.

## Sidebar

Reference clients converge on a left navigation model for repeat workflows:
collections, environments, history, and imported API structure should remain
available without leaving the request editor.

ZenAPI direction:

- Keep endpoints, collections, and history in the left sidebar.
- Use expand/collapse for trees.
- Support right-click actions on collection items.
- Support drag/drop moves in collections.
- Keep filtering local to each sidebar section.
- Use concise empty states that preserve list density.

## Request Builder

Common interaction shape:

- Method selector and URL input are the primary request command row.
- Params, headers, auth, and body are grouped as editor sections or tabs.
- Enter in the URL field should send the request.
- Bulk header paste is expected by users migrating from cURL, Postman, and
  browser devtools.

ZenAPI direction:

- Treat method + URL + send as one command surface.
- Keep body mode switching explicit: none, form-data, urlencoded, raw, binary.
- Keep raw format switching explicit: JSON, XML, Text, HTML.
- Use variables with `{{name}}` replacement in URL, headers, params, auth, and
  body fields.

## Response Interaction

Expected behavior:

- Pretty, Raw, and Headers switch without losing the request context.
- JSON pretty view should not destroy the original raw body.
- Collapsing should summarize structure rather than hiding all useful detail.
- Response body text must be copyable/selectable, but never editable.

ZenAPI direction:

- Use a dedicated read-only selectable GPUI text viewer for response content.
- Do not use editable input widgets for response bodies.
- Keep response metadata fixed and scannable.

## Tabs And Panes

The main tradeoff across reference clients is multi-tab request management
versus a single focused request surface.

ZenAPI direction:

- The current MVP can stay single-request/workbench-first.
- Future request tabs should preserve local request state independently.
- Split-pane behavior should keep request and response visible together on
  desktop widths.

## Keyboard

Baseline shortcuts to consider:

| Shortcut | Expected Action |
|----------|-----------------|
| Enter in URL | Send request |
| Ctrl/Cmd+C in read-only response | Copy selection |
| Ctrl/Cmd+A in read-only response | Select all response text |
| Ctrl/Cmd+F | Focus active list/filter or future global search |
| Ctrl/Cmd+S | Save current request to collection |

## Offline And Local-First

Hoppscotch, Bruno, and Yaak all reinforce that API clients should keep common
workflows available without cloud dependency.

ZenAPI direction:

- Do not require accounts or cloud sync for MVP workflows.
- Keep collections, variables, history, and mock routing local.
- Prefer file import/export formats that work in Git.
- Treat network calls as user request execution, not app dependency.

## Live Review Checklist

- Verify current context-menu behavior in Postman, Bruno, Insomnia, and Yaak.
- Compare collection drag/drop affordances and insertion feedback.
- Record response viewer copy/select behavior in each client.
- Confirm common shortcut defaults before binding global shortcuts.
