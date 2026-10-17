# Kill report: held-key-tools (g7-polish-batch, data half)

- Authorship: dragon (solo, host-down pass).
- Kill-lock `wgui2-held-key-tools`:
  `crates/umber-app/src/toolbar.rs`, `crates/umber-app/src/actions.rs`
  (referenced only — Focus read; no edits to actions.rs).
- Implements the g7-polish-batch row's "held-key temporary tools
  (Space = pan, hold E = erase while held)" — the STATE MACHINE half.
  The shell wiring (the key poll + the dispatch + the pointer-mode
  switch) queues behind the gesture-integration lock on main.rs.

## Built

- `TempTools` (toolbar.rs): a pure hold-state machine.
  - `press(key, current_tool, focus) -> Option<ToolKind>`: text focus
    swallows the press entirely (typing an "e" in a layer name never
    switches tools); one hold at a time (a second key while one is
    held is ignored — no mode thrash); hold-E switches to Erase and
    remembers the previous tool; Pan reports no tool switch (the shell
    owns the navigate-mode interpretation); already-erasing holds
    nothing.
  - `release(key) -> Option<ToolKind>`: restores the remembered tool,
    once; a stray release is a no-op.
  - `panning() -> bool`: the shell's pointer-mode switch reads this.
- Tests (5, in contextual_tests): press-and-restore round-trip; the
  text-focus gate; already-erasing; one-hold-at-a-time (pan while
  erase-held ignored, erase release still restores); pan reports its
  hold without touching tools.

## Judgment calls

- **Restore on key-UP, not a timer**: a quick tap of E flashes nothing
  (press+release within the same frame pair restores before any
  visible commit), and a hold never persists the switch — release is
  the contract, matching Blender's own hold-to-temp behavior (the row
  cites PC:#16).
- **One hold at a time**: interleaved Space+E while both held would
  thrash the strip between erase and pan modes every frame; the
  machine refuses the second hold until the first releases.
- **Pan is not a ToolKind**: the pan hold switches the viewport's
  pointer interpretation (orbit/pan), which is viewport state, not a
  strip slot — `panning()` is the flag the shell reads; the strip is
  untouched.
- **Focus::Text gates, Focus::Widget does not**: a keyboard-focused
  BUTTON user can still hold E (the a11y keyboard-only row's
  audience); only a text edit owns the letter.

## Unverified

- Battery (host down since 18:2x). Static: rustfmt PASS; Focus +
  ToolKind verified against actions.rs:783 / toolbar.rs:148.

## Deferred

- The shell half (queued on the main.rs lock release): the raw key
  poll (Key::E down/up, Space down/up — the action registry binds
  Cmd+Space to ToggleMaximize, so the hold poll is raw input, below
  the registry), the `SelectTool` dispatch through the existing
  action path, and the viewport's pan-while-held pointer switch.
