# Kill report — g1-status-hints (GUI plan G1)

Date: 2026-10-15. Author: dragon. Lock: `g1-status-hints` over
`crates/umber-app/src/status_bar.rs`, `assets/i18n/en-US.ftl`,
`assets/i18n/pt-BR.ftl`, `main.rs` (the `StatusSnapshot` literal only;
the registration was extended when that site needed the new field), and
this report. Plan: `docs/specs/gui-plan-2026-10-15.md` row
`g1-status-hints`. Sources: Blender A4.1 (status-bar tool hints) and
Painter A9 (keyboard helper while a modifier is held).

## Built

- `status_bar.rs`: `HintContext` covers six states: NoMesh, Paint,
  PaintModifier, PanModifier, Pick and IdMask.
  - `HintContext::of(has_mesh, tool, modifiers)` picks the state. The
    strip tools win over modifiers, because they never paint.
  - `hints()` returns the state's `(input, action)` i18n key pairs. The
    bar draws them on its left: the input in bold text colour, the action
    muted, then a separator before UV / Tile / Pen.
- Holding Ctrl swaps the hints to "LMB drag: paint the stroke · Ctrl+Z:
  undo". Holding Shift shows "LMB drag: pan".
- Every hint is a binding the viewport actually has: Ctrl/Cmd+LMB paints,
  LMB drag orbits, Shift+LMB / MMB pans, the wheel zooms
  (`viewport.rs::ui` / `handle_input`). The module doc says a hint for a
  binding that does not exist is a bug.
- i18n: 16 `hint.*` keys in en-US and pt-BR.
- Tests:
  - `hint_context_follows_tool_and_modifier`.
  - `every_hint_key_is_in_both_catalogs`: every state's pairs are present
    in BOTH catalogs. That is stricter than the crate's en-US
    completeness test.

## Judgment calls

1. **The hints describe today's bindings, not the planned Painter
   preset.** g1-nav-presets will change the defaults (Alt+mouse
   navigation). Hints that already advertised Alt-orbit would lie until
   then, so that kill rewrites `hints()` with it.
2. **The hints are hard-coded per context, not derived from the action
   registry.** The viewport's mouse chords are not registry actions yet
   (the registry holds key actions). Folding the mouse chords into the
   registry is part of g1-nav-presets.

## Unverified

- No snapshot of the bar. The G0 kittest-harness kill adds the shell
  snapshot.
