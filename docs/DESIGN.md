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

## GPUI Rewrite Policy

The UI target is GPUI from Zed's official repository. Linux builds use
`gpui_platform` with Wayland and X11 features. Existing Slint files and
bindings were prototype implementation details and should not constrain the
architecture.

- Treat the GPUI migration as a breaking rewrite, not an incremental compatible
  skin over the old UI.
- Keep Slint-specific UI files, generated modules, build steps, callback names,
  and binding helper shapes out of the app shell.
- Do not introduce adapter layers whose only purpose is to keep old Slint-era
  APIs compiling.
- Preserve product behavior and tested domain logic where useful, but allow
  view models, state ownership, event flow, and file layout to change to match
  GPUI.
- New UI documentation should describe GPUI components and app state directly;
  legacy Slint references are only useful as historical migration notes.

## Typography

- Do not bundle application font assets for the GPUI shell unless a future
  design pass introduces a concrete need.
- Ordinary UI copy should go through local text helpers that set font size,
  font weight, text color, and `letter-spacing: 0px` explicitly. Window-level
  defaults are only a fallback, not the primary styling mechanism for component
  text.
- Technical strings such as URLs, filenames, file paths, API paths, and local
  server addresses should use the platform monospace family, not the UI text
  face.
- Placeholder text should use the same family as the eventual input value so
  focused or edited controls do not shift typography mid-interaction.
- Do not rely on platform font registration side effects for sizing,
  alignment, or interaction behavior.
- Keep font sizes fixed per component type. Do not scale text with viewport
  width.
- Prefer concise labels that fit at the minimum window size. Use elision for
  long paths, statuses, and summaries.
- Repeated list rows should not use filler copy for missing optional metadata.
  Leave absent summaries absent while preserving stable row height and alignment.
- Empty states should be one concise status line in dense panes. Avoid stacking
  helper explanations when surrounding controls already express the next action.
- Default Response and tool preview empty states should be status labels such as
  `No response`, `No URL`, or `No headers`, not instructions that tell the user
  what to click next or repeated region names already present in the UI chrome.
- Validation failures shown in the Response pane should stay short and factual;
  examples and procedural usage notes belong in docs, not in dense app chrome.
- Operation success messages should not repeat the same fact in title, metadata,
  and body. For example, OpenAPI import success uses the imported spec name in
  the title, route count in metadata, and the source filename in the body. Dense
  collection/save success titles should be short actions such as `Imported`,
  `Exported`, or `Saved`, with object names or paths moved into metadata/body.
- Pending Response body placeholders should follow the same pattern: use a
  short state label such as `Pending`, followed by the method and URL when that
  context is useful.
- Log panes should use generic compact empty states such as `No messages`,
  `No events`, or `No logs`; avoid repeating protocol or feature names already
  present in the panel title.
- Sidebar empty states should preserve list density. Use a muted fixed row
  placeholder instead of a taller card-like block, with explicit text
  coordinates matching route-row rhythm.

## Visual System

- GPUI primitives and shared components are allowed when they fit the
  interaction. Do not expose default system font, colors, or theme chrome
  directly; wrap or configure them so typography, palette, density, and states
  match ZenAPI.
- Embedded `TextInput` instances must set selection colors and cursor width
  explicitly instead of inheriting toolkit style metrics.
- GPUI scroll containers provide behavior only. ZenAPI panes use app-styled
  vertical scrollbars with a reserved gutter, explicit content-side padding,
  draggable thumb, active drag state, and track click jump behavior. Scrollbar
  pointer events must stop propagation so dragging the thumb never selects
  response text or activates rows below it.
- The visual baseline is a modern light split-pane workstation inspired by
  Postman: flat white and near-white surfaces, no floating cards, no prominent
  shadows, and 1 px dividers instead of broad gutters.
- Below the global top bar, the app uses three same-level panes: Sidebar,
  Request, and Response. The request method, URL, and Send action belong only
  to the Request pane. The top bar stays reserved for app-level chrome.
- Region lines must have a single owner. The app shell owns the top-bar bottom
  rule, and the two workspace resize handles own the Sidebar/Request and
  Request/Response splits. Pane headers and toolbars own their bottom rules;
  child panels should not add competing outside borders on the same edges.
- Do not add ornamental dots, short divider strokes, or stray line fragments.
  Status should be expressed through button state, concise text, or color on an
  existing control; lines are only for region splits, control borders, and tab
  underlines.
- Single-view panes may use a Postman-style active tab row when it anchors the
  editor surface or carries nearby status metadata. Do not add inactive tabs
  unless their content and behavior exist.
- Request and Response should preserve a compact Postman-like rhythm. The
  Request pane owns its 54 px method/URL/Send row, while the Response pane keeps
  status metadata fixed in its header and view/copy actions in the local tab
  row instead of using a separate global status band.
- Use the current light palette consistently without flattening the workspace
  into one gray field: app and editor surfaces `#ffffff`, app chrome
  `#f7f7f8`, workspace gutter `#f0f1f3`, sidebar/request/response panes
  `#ffffff`, request/response tab bars `#ffffff`, muted control fills
  `#ffffff` with hover `#f4f4f5`, disabled controls `#f2f2f3` with border
  `#d9d9df` and text `#8a8f98`, split dividers and borders `#e2e4e8` /
  `#aeb7c2`, primary text `#111827`, body/detail text `#1f2937`,
  secondary text `#4b5563` / `#64748b`, and placeholder text `#b8c0cc`.
- Keep pane backgrounds neutral. Inputs, code blocks, popovers, and dense
  content surfaces stay white or near-white; use structure, borders, tabs, and
  small text or rule accents for orientation instead of broad colored
  backgrounds. Selected rows should prefer a white fill plus a thin accent
  marker over a colored row fill.
- Primary and warning actions should use white control fills with semantic
  borders and text. Reserve solid color for narrow rules, status text, and
  similarly compact accents.
- Implement recurring palette values and stable layout tokens through
  shared UI tokens/metrics in the GPUI app shell rather than inline numeric
  literals in each panel.
- Fixed heights and line heights for menu items, sidebar section buttons,
  status bars, generated-code previews, Response text, result rows, history
  rows, log rows, composite-control dividers, and drag previews should also use
  named layout tokens.
- Sidebar secondary lines should align from the method column width plus the
  row gap, not from a hard-coded left margin. Compact toggle widths should use
  the shared short/long width rule so labels do not resize neighboring controls
  unpredictably.
- Keep controls compact: most buttons should stay at 34-40 px height with a
  maximum 8 px radius. Repeated corner radii should come from the shared radius
  tokens: tight list rows use 4 px, regular controls use 5 px, and input shells
  use 6 px.
- Fixed-height controls and list rows must explicitly center their contents
  vertically. Give text and pill contents stable heights instead of relying on
  layout defaults.
- Text input shells should use explicit coordinates for optional labels and
  value text. When a labeled variant is deliberately used, reserve a 30 px
  label slot, a 10 px gap, and stable horizontal insets so label and value
  baselines do not drift.
- Request-pane input shells, fixed preview boxes, and mode toggles should use
  white or near-white fills with primary/body text. Section titles, table
  headers, and idle control states should not look disabled; input placeholders
  use the placeholder token so they remain visible without competing with
  entered values.
- Preserve a readable typography ladder without globally scaling the app: pane
  header titles use 18 px; panel titles use 17 px; primary editable text,
  Request editor tabs, Request method/Send controls, Response body text, panel
  preview bodies, generated snippets, test results, and realtime/runner/mock log
  rows use 16 px. General action buttons and sidebar navigation/primary rows use
  15 px, top-bar actions, sidebar action buttons, method labels, compact
  controls, panel meta, and table headers use 14 px; row metadata can stay at
  13 px, and compact method/status cells can stay at 12 px.
  GPUI app-shell text sizes should be expressed through named typography
  tokens rather than inline numeric literals.
- Generic Request-pane key/value editors should use a narrower key column than
  credential input groups, with denser columns for body field tables, so the
  value column remains useful in narrow panes. Their column headers should use
  domain labels such as Header, Param, Var, Field, or Name instead of a
  generic Key label when the context is known. Table-like editors, including
  response assertions, should use compact fixed-width symbol buttons for row
  add/remove actions so editing stays discoverable without turning the table
  itself into a large control cluster.
- Compact address/search inputs should not show large inline labels. The
  request URL field, sidebar route filter, and import popover path field use
  only concise placeholders so controls read like address/search bars instead
  of small labeled form fragments.
- Composite controls must preserve visible focus. If an embedded input hides
  its own border, export its focus state to the parent shell and render focus on
  the shared outer border.
- Segmented composite controls such as method + URL + Send must attach their
  children to the shared shell. Embedded command segments should use only the
  exposed outside corner radius and no competing inner border.
- The request method selector is the left segment of the address bar, not a
  separate control. Give it a stable 100 px width, no outside border while
  embedded, fixed text and chevron coordinates, and a single divider on the
  segment boundary.
- Opening the method selector should not paint the whole method segment as a
  selected block or recolor the full address bar. Use method text color, the
  chevron state, cursor affordance, and menu chrome for feedback; the shared
  address-bar focus border belongs to the URL input.
- Method selector text, chevron, and popup option labels should use explicit
  fixed coordinates inside the control so long methods and dropdown glyphs stay
  visually centered and do not create stray line fragments.
- Composite controls should change availability as one unit. During request
  sending, the method selector, URL input, and Send segment should all present a
  shared disabled state instead of leaving editable segments visually active.
- Disabled input shells must suppress editing affordances, including blue focus
  borders and Enter/accepted submission. Any neighboring action in the same
  control group should share the same disabled condition and callback guard.
- Application-level popovers should also respect busy state in both render and
  callback paths. For example, Import popover toggling should be disabled while
  a request or runner operation is active.
- The Request address bar owns this rule explicitly: when the app is busy, the
  inline URL `TextInput` is disabled at the input handler level, not merely
  dimmed by the outer shell.
- Request configuration editors follow the same rule. Query/header/body/auth,
  variable, realtime setup, and test assertion inputs are disabled at the input
  handler level while busy; actions that mutate request configuration, such as
  header presets, bulk paste, environment add/delete, JSON formatting,
  GraphQL introspection/template loading, and test-row creation, must share
  the same enabled condition and Rust-side guard. Test result clearing should
  also be guarded while busy so assertion output cannot be reset mid-run.
  Read-only request-toolbar actions that are visually disabled during busy,
  such as Headers Copy, should use the same explicit busy-aware predicate in
  their callback.
  Format should likewise require idle state, JSON raw-body mode, and a
  non-empty body before attempting to parse or rewrite the editor content.
- Realtime actions must not rely on generic button disabling alone. WS
  Open/Send/End, SSE Once/Stream/Stop, and realtime log Copy/Clear
  should all use explicit busy-aware preconditions at render time and in their
  callbacks.
- Sidebar actions that restore or replace the active request, including
  endpoint selection, history restore, and collection request restore, are also
  disabled while busy. Collection mutations should use the same busy guard so
  saved request state cannot be reordered or removed mid-operation. This guard
  applies to collection context menu create/copy/delete/rename actions and to
  drag preview/drop feedback, not only to the final mutation callback.
  Collection context menus should not open while busy because every action they
  expose mutates collection state.
- History filters may remain editable while busy, but History Clear and per-row
  Delete mutate local records and must share an idle-state enabled condition
  plus a callback guard using the current app state.
- Busy-sensitive path inputs, including Import path, Saved JSON path, and
  Collection rename, are disabled at the input handler level while busy.
  Route and History filters may stay editable because they only change visible
  sidebar rows. Ctrl/Cmd+F should not focus a disabled sidebar path input.
- Header buttons must center the actual button rectangle within the toolbar
  slot, not only center the label text inside a drifting button.
- Top-bar inline controls must use fixed visible heights and slot centering.
  The centered Import bar is a 32 px control inside the 48 px toolbar slot; mock
  actions remain 34 px controls in the same toolbar row.
- Top-bar brand text and right-side mock controls should use fixed toolbar
  slots. The mock status/control group is a 112 px status label, an 8 px gap,
  and a 110 px button, all centered inside the 48 px toolbar row.
- Sidebar headers and status/tab bands should also use fixed-height slots with
  explicit left/right text coordinates. Avoid using stretch spacers to push
  counters or response metadata into place, because content changes can shift
  perceived baselines.
- Split panes and primary content regions must declare explicit stretch rules.
  Sidebar/Request and Request/Response separators are draggable resize handles.
  Split dragging uses a neutral divider preview while the current pane widths
  stay stable; the preview target is quantized before it is stored, and the new
  ratio is applied when the pointer is released.
  Route lists, editor panes, response bodies, and logs use fixed headers plus
  independent scroll regions instead of letting content overflow downward.
- Use functional accents sparingly: green for selected/ready states, blue for
  primary request actions, amber for waiting or mock-stop actions, and red for
  errors.
- Button state colors must come from one shared UI helper. Disabled, neutral,
  primary, warning, hover, and pressed states should not be duplicated inside
  individual button instances.
- Disabled controls must use dedicated disabled tokens instead of borrowing the
  hover color. Hover is an interaction state; disabled controls should remain
  readable without implying clickability.
- HTTP method text colors must come from one shared token/helper. The sidebar
  list, method picker, and request method selector should never duplicate
  separate GET/POST/etc. color maps.
- HTTP method labels need stable widths and explicit overflow handling. Common
  methods such as DELETE and OPTIONS must not compress adjacent route text or
  change row height.
- Sidebar route methods should be text-only fixed-width markers, not filled
  badges. The row selection and hover state already provide enough surface
  feedback.
- Selected sidebar routes should use a restrained 3 px primary-color left
  marker plus row background, not another pill or decorative badge.
- Route list rows must keep the API path on a stable baseline whether a summary
  exists or not. The optional summary belongs in a fixed detail line and must
  not cause the path text to jump vertically between rows. Sidebar detail lines
  that carry real content, such as route summaries, history status, and saved
  request URLs, use body contrast; counters, markers, and chrome status can stay
  secondary or muted.
- Response status color must come from explicit state/tone, not from broad text
  assumptions. Use neutral gray for idle, filtering, and route-selection states;
  amber for in-progress work; green for successful import or 2xx/3xx responses;
  red for validation, transport, mock, and 4xx/5xx response failures.
- Response tone color mapping should also live in one shared UI helper. Domain
  code may emit tone names, but the GPUI view layer owns the visual token
  mapping for those names.
- Avoid drifting into unstyled system theme defaults. When adding surfaces,
  choose colors that fit the light neutral workbench plus green, blue, amber,
  and red functional accents.
- Component APIs should not keep unused theme switches such as stale `dark`
  flags. If a component needs variants, each variant must map to visible tokens
  and be used intentionally.
- Fixed control icons should be drawn with stable icon components or bundled
  assets, not improvised from text glyphs whose shape depends on the font.
- Code and response bodies should use explicit ZenAPI editor chrome with a
  monospace text style, not default text-editor chrome.
- Editable editor panes must show focus through their ZenAPI border color. Do
  not rely on the embedded text editor's native focus chrome. Read-only response
  viewers should keep text selection but must not show editing affordances such
  as an insertion cursor or blue editing focus border.
- The top bar is a global console, not a form. Keep it fixed height, align the
  brand slot and right-side actions with stable widths, use a single bottom
  divider, avoid internal structural split lines, and avoid explanatory status
  sentences. Keep file path entry inside the import popover instead of making it
  persistent chrome.
- The request address bar belongs inside the Request pane as one 38 px method /
  URL / Send shell in a 54 px request row. Do not let it span Sidebar or
  Response.
- Response panel header title should stay `Response`. Non-empty Response status
  metadata is right-aligned inside the 40 px header with a 180 px max width, 14
  px right inset, and explicit truncation. Empty or whitespace-only metadata
  should not reserve a right-side slot, so Request headers and idle Response
  headers keep their title space. Hide default `Idle`; combine meaningful status
  and tests metadata as compact right-side text.
- Pane tab rows use fixed-height slots with a 1 px bottom divider. Title,
  action, and status text should use explicit insets, fixed action widths, and
  elision instead of letting content resize the pane.
- Top-bar status labels must use fixed-height, non-stretching slots and explicit
  text height so the label rectangle and its contents are both vertically
  centered against neighboring buttons.
- The bottom status bar should suppress idle filler labels. Do not render
  default `Ready`, stopped/ready/no-route mock text, or idle Response text; only
  show busy, running mock, error, or operation-specific status when it carries
  current information. The right-side status slot should only be reserved while
  it has content, and it should be content-sized with a compact max width so
  narrow windows leave more room for route and mock context.
- Dense panel headers should follow the same rule: hide default `idle`,
  `Runner idle`, `No requests`, `No results`, and zero-count test summaries
  instead of reserving header space for filler text.
- Sidebar section headers should not carry long file or collection status
  sentences. Hide default saved-collection status and compress known collection
  outcomes to short labels such as `Imported`, `Exported`, `+ Req`, and `Busy`.
- Mock controls should communicate state through enabled/disabled state, button
  text, concise short labels such as a port number, and accent color. Running
  mock addresses in the status bar should be compressed to the port, and
  transient start/stop/fail status should use short labels. Do not add long
  helper text to explain why a disabled action is unavailable.
- Primary command labels should stay stable during transient work when a nearby
  status region already communicates progress. Prefer disabled/busy styling
  plus response status over changing button text length.
- Runtime labels in the top bar must be bound to actual application state. Do
  not hard-code server ports, filenames, counts, or statuses when the binding
  layer already knows the real value.
- Transient popovers should use the same light surface and 1 px border without
  drop shadows. Align popover origins to the triggering control's grid position
  so they do not appear to drift by a pixel.
- `Escape` should close all transient UI layers, including Import, Method,
  Codegen, and Collection menus. Opening one transient layer should close the
  other transient layers so menus do not stack or compete for attention.
- The import popover is a compact 520 x 58 px surface: 10 px horizontal inset
  and one vertically centered 34 px path/action row. Do not add a title row or
  PATH label; the popup belongs to the Import bar and should not become a
  miniature form with extra alignment targets.

## Interaction

- Disable actions that cannot succeed yet when the condition is knowable in the
  UI, such as starting the mock server before routes are imported.
- Still keep server-side or binding-side validation for all disabled actions,
  because state can change asynchronously.
- Viewer toolbar actions should use the same pure precondition at render time
  and at callback time. For example, Response Copy depends on the active view,
  non-empty copy text, and idle state in both places.
- Request Send should use the same busy-aware URL/pre-request URL predicate for
  the button state, URL Enter handler, shortcut handler, and click callback.
  The click callback must re-read current app state rather than trusting a
  render-time `enabled` value.
- Request method selection is a request-configuration mutation. Both the method
  selector and each method menu item must use the same current idle-state
  predicate before opening the menu or changing the method.
- Runner options are local execution configuration. Toggles such as Stop on
  fail must re-read current `runner_running` and busy state in the callback, not
  rely on the render-time enabled value.
- Generated-code copy should not copy a snippet captured during an earlier
  render. The callback should rebuild the current request snippet and recheck
  busy/request validity before writing to the clipboard.
- Generated-code language selector and Response Fold/Open should also
  recheck current app state in their callbacks. Fold/Open specifically
  requires idle state and currently collapsible JSON raw response content.
- UI disabled states are not enough for long-running operations. Import,
  path-driven collection actions, request sending, request configuration
  mutations, and mock start/stop callbacks must also guard against re-entry in
  Rust using the current busy state.
- Shared action-button helpers must also recheck current busy state before
  invoking their callback. A control that was rendered enabled should not fire
  after an asynchronous operation has moved the app into busy state.
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
- GPUI-provided and custom controls must preserve expected native keyboard
  behavior. Single-line inputs that drive a primary action should expose
  Enter/accepted behavior, such as importing a specification path or sending the
  current request URL.
- Clickable custom controls must declare their cursor intentionally. Buttons,
  tabs, dropdowns, and route rows should all expose pointer feedback instead of
  relying on toolkit defaults.
- Disabled controls must not keep pointer feedback. Their cursor and visual
  state should both communicate that the action is unavailable.
- Collection runner controls belong near collection/request workflow context,
  not in a global marketing-style area. The MVP runner should expose one clear
  `Run` action, a visible stop-on-failure toggle, current status, and dense
  result rows.
- Runner result rows should use the same fixed-width method/status rhythm as
  mock logs and history rows. Passing/failing state should be communicated via
  response tone colors, not separate decorative badges.
- CLI commands must not initialize GPUI unless the user is starting the desktop
  app. `zenapi --help` and `zenapi run --help` should remain usable in headless
  environments.

## Documentation

- User-facing docs live in `docs/USER_GUIDE.md` and should describe current
  behavior without promising roadmap items as available.
- Developer docs live in `docs/DEV_GUIDE.md` and should track module
  boundaries, build/test commands, dependency policy, and verification
  expectations.
- `docs/PRD.md` owns product scope and backlog.
- `docs/TODO.md` remains the execution checklist and should be updated with
  every completed implementation or verification pass.

## Iteration And Commits

- Document persistent design decisions in this file or the PRD as they emerge.
- Commit coherent, verified increments: a visual system pass, a focused feature,
  or a documentation update can each be a commit when tests pass.
- Before committing, run `cargo fmt`, `cargo check`, and `cargo test` unless the
  change is documentation-only.
- Keep unrelated worktree changes intact. Do not revert user changes while
  iterating.
