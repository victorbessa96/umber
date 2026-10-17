# Kill report: polish-widgets (g7-polish-batch, umber-gui half)

- Authorship: dragon (solo, host-down pass).
- Kill-lock `wgui2-polish-widgets`:
  `crates/umber-gui/src/widgets/type_ahead.rs` (new),
  `crates/umber-gui/src/widgets/dialog_buttons.rs` (new).
- Implements g7-polish-batch's "dropdown type-ahead" (photocraft #13)
  and "platform-ordered dialog buttons" (photocraft #16) as NEW
  widgets. The `mod.rs`/`lib.rs` registration points sit under the
  landed context-rail claw's lock — registration happens at wave
  commit with the lock release, per the lock-protocol.

## Built

- `type_ahead.rs` — `TypeAhead`: pure prefix state + `frame(delta,
  keys, labels) -> Option<usize>`. Letter jumps to the next
  CONTAINING match (case-insensitive); same letter cycles through
  matches; quick typing builds a prefix; a 1 s pause resets it
  (photocraft's PAUSE); a non-matching extension drops the new key but
  preserves the working prefix (the Python port caught the first test
  pinning the wrong frame — the None frame then the preserved match —
  and the test now pins the real contract).
  5 tests: jump, cycle, prefix build, pause reset, preserve-on-miss.
- `dialog_buttons.rs` — `ButtonRole { Default, Cancel, Plain }`,
  `DialogButton { role, label }`, `platform_order(os, declared)` and
  the `dialog_buttons(ui, os, buttons) -> Option<declared index>`
  draw call. Windows/Linux: `[Plain…, Default, Cancel]` (Cancel
  rightmost); macOS: `[Default, Cancel, Plain…]` (primary leads). The
  Default role renders strong; the clicked result is the DECLARED
  index, stable across platforms, so dialog match arms never branch
  on OS.
  4 tests: windows order, nix==windows, mac order, declared-index
  stability under declaration reordering.

## Judgment calls

- **Contains-match, not prefix-match** (photocraft's own semantics):
  'c' must reach "Clone" in a list where nothing STARTS with 'c' —
  a prefix-only matcher would need exact knowledge of every list's
  vocabulary.
- **Preserve-on-miss, not clear-on-miss**: a stray keystroke in a
  5-option list must not kill a working prefix; dropping only the new
  key keeps the interaction forgiving. The alternative (clear
  everything) punishes every typo with a restart.
- **Declared-index click contract**: dialogs write ONE match arm per
  button; the platform order changes only WHERE it draws, never WHAT
  it means.
- **Cancel rightmost on Windows/Linux, primary first on mac** — the
  two conventions the row cites; GNOME is followed as Cancel-left
  (matching GTK's own dialogs).

## Unverified

- Battery (host down since 18:2x). Static: rustfmt PASS; every egui
  API resolved (OperatingSystem variants at os.rs:4 — `Nix` for
  Linux; `From<RichText> for WidgetText` widget_text.rs:852;
  `RichText::strong` style.rs). Pure halves ported to Python: 5/5
  type-ahead cases + 2/2 platform orders pass; the egui draw halves
  (Button rendering, horizontal layout) ride the battery.

## Deferred

- ~~`mod.rs` + `lib.rs` registration + re-exports~~ — LANDED
  (2026-10-17 21:58, the wave-end lock consolidation): both modules
  registered with re-exports (`TypeAhead`, `ButtonRole`,
  `DialogButton`, `dialog_buttons`).
- Consumers: the blend-mode combo (type-ahead), the ✕ confirm +
  resize-confirm dialogs (platform order) — both in umber-app,
  lock-queued like the other shell halves.
- The row's remaining item "single-line text-field drag fix (plugin)"
  is an upstream egui plugin note — needs its own investigation
  (which drag breaks), not built here.
