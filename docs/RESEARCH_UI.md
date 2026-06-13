# UI Research Baseline

> Status: offline baseline, pending live review with current product versions
> and screenshots.

## Target

ZenAPI should be a dense native API workstation: local-first and built around
the actual request/response workflow on the first screen.
The visual direction is closer to a workbench than a landing page.

## Product References

| Client | Layout Signal | Visual Signal | Useful For ZenAPI |
|--------|---------------|---------------|-------------------|
| Postman | Global header, left workspace sidebar, tabbed workbench, response area | Dense light UI, strong command hierarchy, method/status colors | Functional ceiling and familiar request workflow |
| Hoppscotch | Minimal single-page request surface, collapsible navigation | Minimal surfaces, web-first responsive behavior | Low-friction request editing and simple protocol switching |
| Bruno | Collection tree, file-backed request editor, response panel | Git-oriented, quieter chrome, readable text artifacts | Local collection mental model and Bru-style storage |
| Insomnia | Left collection/sidebar, central request editor, response pane, OpenAPI design tools | Dark-theme heritage, strong editor feel | OpenAPI design/debug bridge and environment UX |
| Yaak | Native desktop shell, local data, tabbed requests | Privacy-first posture, native desktop conventions | Rust/native expectations and local-first product tone |

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
| App chrome | `#f7f7f8` |
| Workspace gutter | `#f0f1f3` |
| Sidebar pane | `#ffffff` |
| Request pane | `#ffffff` |
| Response pane | `#ffffff` |
| Muted control fill | `#ffffff`, hover `#f4f4f5` |
| Disabled control | `#f2f2f3`, border `#d9d9df`, text `#8a8f98` |
| Border/divider | `#e2e4e8`, `#c7ccd3` |
| Primary text | `#111827` |
| Secondary text | `#4b5563`, `#64748b` |
| Placeholder text | `#e3e7ee` |
| Primary action | White fill, blue border/text |
| Success/ready | Green text/accent |
| Busy/warning | Amber text/accent |
| Error/failure | Red text/accent |

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
- Opening a request method picker should not recolor the whole address bar;
  keep feedback on the method text, chevron, and menu surface.
- Tabs should be real content switches with stable height and an underline for
  active state.
- Text editors and response viewers need explicit ZenAPI chrome instead of
  exposing default toolkit editor styling.
- Pane backgrounds should remain neutral; use borders, tabs, thin active
  markers, and compact accents for orientation instead of broad colored fields.

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
