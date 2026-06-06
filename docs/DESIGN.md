# ZenAPI Design Notes

This document captures visual and interaction decisions that should survive
iteration. Treat it as a working guide: update it when the UI direction changes
or a repeated product decision becomes explicit.

## Direction

ZenAPI should feel like a focused local API workstation, not a marketing site
or a generic system-themed desktop demo. The first screen is the working tool:
import specification, inspect endpoints, send requests, and start mock routes.

The interface should be quiet, dense, and scannable. Avoid oversized decorative
sections, floating nested cards, and one-color themes. Favor direct controls,
stable dimensions, and clear state over explanatory in-app copy.

## Typography

- Do not rely on OS default fonts or generic families such as `monospace` for
  critical UI text.
- Import bundled font files from Slint and reference explicit families. The
  current app families are `Inter` for UI text and `Noto Sans Mono` for code.
- Technical strings such as URLs, filenames, file paths, API paths, and local
  server addresses should use `Noto Sans Mono`, not the UI text face.
- Placeholder text should use the same family as the eventual input value so
  focused or edited controls do not shift typography mid-interaction.
- Do not register fonts through runtime APIs that require the Slint platform
  before it is initialized.
- Keep font sizes fixed per component type. Do not scale text with viewport
  width.
- Prefer concise labels that fit at the minimum window size. Use elision for
  long paths, statuses, and summaries.
- Repeated list rows should not use filler copy for missing optional metadata.
  Leave absent summaries absent while preserving stable row height and alignment.
- Empty states should be one concise status line in dense panes. Avoid stacking
  helper explanations when surrounding controls already express the next action.

## Visual System

- Slint-provided components are allowed when they fit the interaction. Do not
  expose their default system font, colors, or theme chrome directly; wrap or
  configure them so typography, palette, density, and states match ZenAPI.
- Embedded `TextInput` instances must set selection colors and cursor width
  explicitly instead of inheriting toolkit style metrics.
- Slint scroll containers may be used for behavior, but default scrollbar
  chrome must be disabled or replaced with a ZenAPI-styled scrollbar before it
  is exposed in the UI.
- The visual baseline is a modern light split-pane workstation inspired by
  Postman: flat white and near-white surfaces, no floating cards, no heavy
  shadows, and 1 px dividers instead of broad gutters.
- Below the global top bar, the app is organized into three coordinated regions:
  Endpoints, Request, and Response. These regions are peers separated by 1 px
  dividers; do not stack Response under Request as a secondary afterthought.
- Region lines must have a single owner. The app shell owns the top-bar bottom
  rule and the vertical split rules between Endpoints, Request, and Response;
  child panels should not add competing outside borders on the same edges.
- Do not add ornamental dots, short divider strokes, or stray line fragments.
  Status should be expressed through button state, concise text, or color on an
  existing control; lines are only for region splits, control borders, and tab
  underlines.
- Do not use tab underlines for a pane with only one implemented content view.
  Present it as a compact pane title instead; a single short underline reads as
  decoration rather than navigation.
- Request and Response must share the same vertical rhythm. Their top utility
  bands are 52 px high, their pane title or tab rows are 36 px high, and
  editor/content panes must start on the same y-axis.
- Use the current light palette consistently: app and editor surfaces
  `#ffffff`, toolbar/sidebar surfaces `#f9fafb`, subtle control fills
  `#f3f4f6`, split dividers and borders `#e5e7eb` / `#d1d5db`, primary text
  `#111827`, and secondary text `#6b7280` / `#9ca3af`.
- Keep controls compact: most buttons should stay at 34-40 px height with a
  maximum 8 px radius.
- Fixed-height controls and list rows must explicitly center their contents
  vertically. Give text and pill contents stable heights instead of relying on
  layout defaults.
- Composite controls must preserve visible focus. If an embedded input hides
  its own border, export its focus state to the parent shell and render focus on
  the shared outer border.
- Segmented composite controls such as method + URL + Send must attach their
  children to the shared shell. Embedded command segments should use only the
  exposed outside corner radius and no competing inner border.
- Header buttons must center the actual button rectangle within the toolbar
  slot, not only center the label text inside a drifting button.
- Split panes and primary content regions must declare explicit stretch rules.
  The sidebar can be fixed width, but the main work area, route list, panels,
  and editor panes should not depend on implicit layout expansion.
- Use functional accents sparingly: green for selected/ready states, blue for
  primary request actions, amber for waiting or mock-stop actions, and red for
  errors.
- Response status color must come from explicit state/tone, not from broad text
  assumptions. Use neutral gray for idle, filtering, and route-selection states;
  amber for in-progress work; green for successful import or 2xx/3xx responses;
  red for validation, transport, mock, and 4xx/5xx response failures.
- Avoid drifting into unstyled system theme defaults. When adding surfaces,
  choose colors that fit the light neutral workbench plus green, blue, amber,
  and red functional accents.
- Component APIs should not keep unused theme switches such as stale `dark`
  flags. If a component needs variants, each variant must map to visible tokens
  and be used intentionally.
- Fixed control icons should be drawn with stable icon components or bundled
  assets, not improvised from text glyphs whose shape depends on the font.
- Code and response bodies should use explicit ZenAPI editor chrome with
  `Noto Sans Mono`, not default text-editor chrome.
- Editor panes must show focus through their ZenAPI border color. Do not rely
  on the embedded text editor's native focus chrome.
- The top bar is a global console, not a form. Keep it fixed height, align the
  brand area exactly with the sidebar width, use a single bottom divider, avoid
  internal structural split lines, and avoid explanatory status sentences. Show
  specification state as a compact read-only label plus an Import action; keep
  path entry inside an import affordance instead of making it the persistent
  visual center.
- Mock controls should communicate state through enabled/disabled state, button
  text, concise short labels such as a port number, and accent color. Do not add
  long helper text to explain why a disabled action is unavailable.
- Primary command labels should stay stable during transient work when a nearby
  status region already communicates progress. Prefer disabled/busy styling
  plus response status over changing button text length.
- Runtime labels in the top bar must be bound to actual application state. Do
  not hard-code server ports, filenames, counts, or statuses when the binding
  layer already knows the real value.

## Interaction

- Disable actions that cannot succeed yet when the condition is knowable in the
  UI, such as starting the mock server before routes are imported.
- Still keep server-side or binding-side validation for all disabled actions,
  because state can change asynchronously.
- UI disabled states are not enough for long-running operations. Import,
  request sending, and mock start/stop callbacks must also guard against
  re-entry in Rust using the current busy state.
- Importing a new specification while the mock server is running must stop the
  existing mock server and return the UI to a ready-but-stopped state. The route
  list, status text, and actual running service must describe the same spec.
- Route lists should remain manageable for large specs. Filtering by method,
  path, or summary is part of the MVP workstation behavior.
- Filtering is a view state only. Global actions such as starting the mock
  server must use the imported route count, not the current visible match count.
- Filtering must not overwrite the response panel status or body. The response
  panel represents the latest import, route selection, request, or mock error,
  while filter feedback belongs in the sidebar count and empty state.
- Avoid inactive tabs or controls that imply functionality not yet implemented.
  Add tabs only when their content and behavior exist.
- Slint-provided and custom controls must preserve expected native keyboard
  behavior. Single-line inputs that drive a primary action should expose
  Enter/accepted behavior, such as importing a specification path or sending the
  current request URL.
- Clickable custom controls must declare their cursor intentionally. Buttons,
  tabs, dropdowns, and route rows should all expose pointer feedback instead of
  relying on toolkit defaults.
- Disabled controls must not keep pointer feedback. Their cursor and visual
  state should both communicate that the action is unavailable.

## Iteration And Commits

- Document persistent design decisions in this file or the PRD as they emerge.
- Commit coherent, verified increments: a visual system pass, a focused feature,
  or a documentation update can each be a commit when tests pass.
- Before committing, run `cargo fmt`, `cargo check`, and `cargo test` unless the
  change is documentation-only.
- Keep unrelated worktree changes intact. Do not revert user changes while
  iterating.
