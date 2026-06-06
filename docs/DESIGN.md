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
- Register bundled fonts at startup and reference explicit app families in
  Slint. The current app families are `Zen Sans` and `Zen Mono`.
- Register bundled fonts before creating any Slint components so first paint
  does not fall back to system fonts.
- Keep font sizes fixed per component type. Do not scale text with viewport
  width.
- Prefer concise labels that fit at the minimum window size. Use elision for
  long paths, statuses, and summaries.

## Visual System

- The visual baseline is an integrated dark charcoal split-pane workstation.
  Avoid beige or light page surfaces, oversized gutters, and floating card
  stacks. Panels should meet through 1 px dividers, not broad background gaps.
- Use the current dark palette consistently: app background `#0e1015`,
  toolbar/sidebar/panel surfaces `#151720`, editor and input surfaces
  `#1a1d26`, split dividers and borders `#252936`, primary text `#e2e8f0`,
  and secondary text `#64748b`.
- Keep controls compact: most buttons should stay at 34-40 px height with a
  maximum 8 px radius.
- Fixed-height controls and list rows must explicitly center their contents
  vertically. Give text and pill contents stable heights instead of relying on
  layout defaults.
- Use functional accents sparingly: green for selected/ready states, blue for
  primary request actions, amber for waiting or mock-stop actions, and red for
  errors.
- Response status color must come from explicit state/tone, not from broad text
  assumptions. Use neutral gray for idle, filtering, and route-selection states;
  amber for in-progress work; green for successful import or 2xx/3xx responses;
  red for validation, transport, mock, and 4xx/5xx response failures.
- Avoid drifting back to an all blue/gray default theme. When adding surfaces,
  choose colors that fit the existing neutral, green, amber, and dark code-pane
  system.
- Code and response bodies should use the dark code pane with `Zen Mono`, not
  default text-editor chrome.
- The top bar is a global console, not a form. Keep it fixed height, align the
  brand area exactly with the sidebar width, use a single bottom divider, and
  avoid explanatory status sentences. Show specification state as a compact
  read-only label plus an Import action; keep path entry inside an import affordance
  instead of making it the persistent visual center.
- Mock controls should communicate state through enabled/disabled state, button
  text, concise short labels such as a port number, and accent color. Do not add
  long helper text to explain why a disabled action is unavailable.

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

## Iteration And Commits

- Document persistent design decisions in this file or the PRD as they emerge.
- Commit coherent, verified increments: a visual system pass, a focused feature,
  or a documentation update can each be a commit when tests pass.
- Before committing, run `cargo fmt`, `cargo check`, and `cargo test` unless the
  change is documentation-only.
- Keep unrelated worktree changes intact. Do not revert user changes while
  iterating.
