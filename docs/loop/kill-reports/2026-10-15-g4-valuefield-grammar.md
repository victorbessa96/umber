# Kill report: g4-valuefield-grammar (the Blender/Painter field contract)

Date: 2026-10-15. Author: dragon. Lock: `g4-valuefield-grammar` over
`crates/umber-gui/src/widgets/value_field.rs` and this report. Plan:
`docs/specs/gui-plan-2026-10-15.md` row `g4-valuefield-grammar` (B:D1-D9,
P:D2, PC:#8). Every one of the 47 existing `ValueField::new` call sites
keeps its behavior unless it opts into the new builders.

## Built

- **Typed entry.** A click without a drag, or keyboard focus (Tab), opens
  a centred text edit over the field with all of the text selected.
  Enter or focus loss commits and Escape cancels. The edit buffer lives
  in egui temp data, so the edit survives across frames.
- **`eval_expr(text, unit)`.** A recursive-descent parser for numbers,
  `+ - * /`, parentheses and unary signs. It accepts a trailing unit that
  matches the field's. It returns `None` for junk, for division by zero,
  for non-finite results (`1e999`), and past a nesting depth of 32, so
  `((((…` cannot overflow the stack.
- **Ctrl/Cmd + wheel steps** the hovered field. egui folds Ctrl+wheel
  into zoom, so the step reads the raw `MouseWheel` events. Alt + wheel
  still works, and a plain wheel still scrolls the panel.
- **`soft_range`.** Dragging (and the drag fill bar) clamps to the soft
  range intersected with the hard range. Typing, pasting and the wheel
  reach the hard range.
- **Hover clipboard + reset.** Ctrl/Cmd+C copies the value, and
  Ctrl/Cmd+V pastes through `eval_expr`. The field consumes its own
  `Copy`/`Paste` events. With a `default_value` set, Backspace resets to
  it. None of the hover gestures fire while another text field owns the
  keyboard. Right-click copy stays.

## Tests (4 new, 13 in the module)

- `expressions_evaluate_and_reject_junk`: precedence, left
  associativity, unary signs, `.5`, the unit rule, eleven junk inputs,
  and the 200-deep nesting rejection next to an 8-deep pass.
- `soft_bounds_intersect_the_hard_range`.
- `click_types_an_expression_past_the_soft_range` (headless): click,
  type `2*256 px`, Enter, and the value is 512, past the soft maximum
  of 10.
- `hover_paste_ctrl_wheel_and_backspace_reset` (headless): paste `100/4`
  gives 25, a junk paste changes nothing, Ctrl+wheel adds exactly one
  0.01 step, a plain wheel does not step, and Backspace resets to 5.

## Judgment calls

- **No double-click reset.** The plan row lists one, but the first click
  of a double-click already opens typing, so a double-click reset would
  fight it. Backspace on hover is the reset instead (Blender also binds
  Backspace).
- **No percentage display for factors.** "Factor shown as 0-100 %" means
  a display mode per field, which has no consumer yet. It is deferred
  rather than added unused.
- **Focus on the frame after the opening click.** In the click's own
  frame the new text edit has no rect from last frame, so egui's
  click-elsewhere rule would drop the focus at once. The edit focuses
  on the first frame without a click. Found by the headless test.
- **Tab chains fields** through egui's own focus order: a field gains
  focus, opens its edit, and Tab moves focus on, which commits.

## Unverified

- Tab-chaining between two fields has no test of its own (it is egui's
  focus traversal over the `gained_focus` path the click test covers).
- Snapshot appearance of the editing state.

## Deferred

- Factor-as-percentage display.
- Per-property Reset / Copy / Paste on right-click (g4-property-context-menu).
