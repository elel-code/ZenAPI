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
- Keep font sizes fixed per component type. Do not scale text with viewport
  width.
- Prefer concise labels that fit at the minimum window size. Use elision for
  long paths, statuses, and summaries.

## Visual System

- Keep controls compact: most buttons should stay at 34-40 px height with a
  maximum 8 px radius.
- Use a restrained neutral workbench base with functional accents. Current
  accents are green for selected/ready states, blue for primary request action,
  amber for waiting states, and red for errors.
- Avoid drifting back to an all blue/gray default theme. When adding surfaces,
  choose colors that fit the existing neutral, green, amber, and dark code-pane
  system.
- Code and response bodies should use the dark code pane with `Zen Mono`, not
  default text-editor chrome.

## Interaction

- Disable actions that cannot succeed yet when the condition is knowable in the
  UI, such as starting the mock server before routes are imported.
- Still keep server-side or binding-side validation for all disabled actions,
  because state can change asynchronously.
- Route lists should remain manageable for large specs. Filtering by method,
  path, or summary is part of the MVP workstation behavior.
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
