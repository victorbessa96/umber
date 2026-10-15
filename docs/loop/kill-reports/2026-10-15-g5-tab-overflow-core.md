# Kill report (UNRUN, host-gated): g5-tab-overflow core in umber-gui

Date: 2026-10-15. Author: dragon. Lock: `g5-tab-overflow-core` over
`umber-gui/src/widgets/{mod,tab_overflow}.rs` and this report. It is the
pure half of g5-tab-overflow: the dock wiring needs `umber-app` main.rs,
which is under `g5-workspace-persist`'s lock. Design:
`docs/specs/g5-tab-overflow-design.md`.

## Written

`umber_gui::widgets::tab_overflow`:
- `elided_title(ctx, title, font, max_width) -> Arc<Galley>`: one row,
  `TextWrapping::truncate_at_width`, "…" overflow, placeholder colour (so
  the dock still tints active and inactive tabs). Pre-laid, because
  egui_dock re-wraps any `LayoutJob` at infinite width.
- `TAB_TITLE_MAX = 140.0`.
- `scroll_into_view(widths, index, bar, scroll) -> f32`: the
  `LeafNode::scroll` that shows tab `index`, moving minimally, clamped to
  egui_dock's own `[-overflow, 0]`.
- `overflows(widths, bar)`.

Five tests: elide/no-elide on a real font context, the shipped titles vs
the cap (measured at the Button style), pinned scroll cases, a sweep
property (every target lands inside the bar from every start), and the
overflow edge.

## Pre-host verification (no cargo)

- egui 0.36.2 / epaint API checked in the registry source:
  - `LayoutJob::single_section`, `TextFormat::simple`;
  - `TextWrapping::truncate_at_width`, `fonts_mut(|f| f.layout_job)`;
  - `Galley.elided` / `.rows`, and `PlacedRow` derefs to `Row::text()`;
  - `Context::run_ui`, `global_style`.
  All re-exported through `egui::text`.
- Caught in review: `Galley::text()` returns the job's FULL input, never
  the elided text. The test now reads `rows[0].text()` for what is drawn
  and asserts `text()` is the whole title (the tooltip's source).
- `scroll_into_view` ported line for line to Python; all pins and the
  sweep pass. Mutants:

  | Mutant | Pins | Sweep |
  |---|---|---|
  | no clamp | killed | survives |
  | the right branch aligns left | killed | survives |
  | forgets the too-wide case | killed | killed |

  So the PINNED cases are the load-bearing test. The sweep guards the
  invariant, not the exact landing.

## Host

`cargo test -p umber-gui tab_overflow`, then clippy. On the host,
mutation-check the right branch (`bar - right` → `-left`) against the
pins.
