# UI Research Baseline

> Status: offline baseline, pending live review with current product versions,
> screenshots, and exact measurements.

## Target

ZenAPI should be a dense native API workstation: local-first, fast to start,
and built around the actual request/response workflow on the first screen.
The visual direction is closer to a workbench than a landing page.

## Product References

| Client | Layout Signal | Visual Signal | Useful For ZenAPI |
|--------|---------------|---------------|-------------------|
| Postman | Global header, left workspace sidebar, tabbed workbench, response area | Dense light UI, strong command hierarchy, method/status colors | Functional ceiling and familiar request workflow |
| Hoppscotch | Lightweight single-page request surface, collapsible navigation | Minimal surfaces, web-first responsive behavior | Low-friction request editing and simple protocol switching |
| Bruno | Collection tree, file-backed request editor, response panel | Git-oriented, quieter chrome, readable text artifacts | Local collection mental model and Bru-style storage |
| Insomnia | Left collection/sidebar, central request editor, response pane, OpenAPI design tools | Dark-theme heritage, strong editor feel | OpenAPI design/debug bridge and environment UX |
| Yaak | Native desktop shell, local data, tabbed requests | Small native-app footprint, privacy-first posture | Rust/native expectations and local-first product tone |

## Layout Decisions

- Use a fixed left sidebar for endpoints, collections, and history.
- Use a main workbench with one request utility row, a request editor region,
  and a peer response viewer region.
- Keep response status metadata inside the response tab/header row rather than
  creating another large status banner.
- Prefer compact rows and stable dimensions over decorative cards.
- Avoid nested cards. Use 1 px dividers for region boundaries.

## Visual Tokens

| Role | Baseline |
|------|----------|
| App surface | `#ffffff` |
| Toolbar/sidebar surface | `#f9fafb` |
| Muted control fill | `#f3f4f6` |
| Border/divider | `#e5e7eb`, `#d1d5db` |
| Primary text | `#111827` |
| Secondary text | `#6b7280`, `#9ca3af` |
| Primary action | Blue |
| Success/ready | Green |
| Busy/warning | Amber |
| Error/failure | Red |

## Typography

- Use GPUI/platform fonts; do not bundle font assets for the shell.
- Use `.SystemUIFont` for ordinary UI text.
- Use the platform generic `monospace` family for URLs, paths, snippets, and
  response bodies.
- Keep font sizes fixed per component type and avoid viewport-scaled type.
- Keep letter spacing at zero.

## Control Style

- Buttons should stay compact, usually 34-40 px high.
- Cards should be rare; repeated list items, modal surfaces, and framed tools
  are acceptable, but full page regions should remain unframed.
- HTTP method labels should be fixed-width text markers, not filled badges.
- Tabs should be real content switches with stable height and an underline for
  active state.
- Text editors and response viewers need explicit ZenAPI chrome instead of
  exposing default toolkit editor styling.

## Response Viewer

- Keep Pretty, Raw, and Headers as first-class tabs.
- Pretty JSON should support formatting and collapse/expand.
- Raw must preserve the original body.
- Headers should use a readable line-oriented monospace view.
- Response body text must be selectable, but read-only and without an editing
  insertion cursor.

## Live Review Checklist

- Capture current screenshots of Postman, Hoppscotch, Bruno, Insomnia, and
  Yaak on the same desktop scale.
- Measure sidebar width, toolbar height, tab height, and response metadata
  placement.
- Compare HTTP method palettes and status color choices.
- Verify current terminology for collection, environment, history, runner, and
  mock features.
