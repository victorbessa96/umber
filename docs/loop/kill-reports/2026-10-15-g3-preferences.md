# Kill report — g3-preferences (GUI plan G3)

Date: 2026-10-15. Author: dragon. Lock: `g3-preferences` over
`crates/umber-app/src/preferences.rs` (new), `main.rs`, `actions.rs`,
`toolbar.rs`, `palette.rs`, `status_bar.rs` (added mid-kill),
`assets/i18n/*.ftl` and this report. Plan:
`docs/specs/gui-plan-2026-10-15.md` row `g3-preferences-window`. Sources:
Painter F2 (keyboard helper on/off, camera speeds), Blender F9 / F15 / E4
(UI scale separate from DPI, Reduce Motion, tooltips off), photocraft #10
(sync onto the style ONLY when it differs; a search box over the rows).

## Built

- **`preferences.rs` (new):** `Preferences { ui_scale, tooltips,
  reduce_motion, key_hints, nav }`.
  - Persisted as `key=value` lines in `preferences` in the per-user state
    dir (beside `locale` and the GPU startup files). This is the
    gpu_startup precedent: no format dependency for five scalars.
  - Lenient parse: unknown keys and junk lines are ignored, a bad value
    keeps that field's default, the scale is clamped to 0.75..=2, NaN is
    rejected.
  - `effective_nav(env)`: `UMBER_NAV` still wins for scripted launches,
    and the window says so when it is set.
  - `style_delta(...)`: per field, `Some(new)` only when egui differs, so
    the per-frame sync writes nothing at rest. Tooltips off sets an
    infinite delay; Reduce Motion sets `animation_time` to 0; scale is the
    zoom factor.
  - `row_matches` drives the search.
  - 6 tests: round trip and defaults; lenient + clamped parse; env
    override; the delta is empty at rest and after applying; search;
    a file save / load round trip.
- **The window** (`main.rs` `show_preferences`): Edit ▸ Preferences or
  Ctrl+, opens a searchable window.
  - Interface: UI scale, Show tooltips, Reduce motion, Status-bar key
    hints. Navigation: the preset.
  - A change saves immediately and re-applies the nav preset; the style
    sync runs every frame through `style_delta`.
- **The navigation preset now comes from Preferences** (with the env
  override) instead of `UMBER_NAV` alone. The nav-presets kill's
  "until the G3 preferences window exists" is closed.
- **Key hints can be turned off:** `HintContext::Hidden` (no hints, no
  nav chords), with a test.
- **Registry:** `ShellAction::Preferences` on Ctrl+, (it passes a text
  field like the other global commands; the focus test is updated),
  listed in the palette under Edit. The toolbar invariant counts move
  to 9 bare + 6 chords.
- i18n: 15 `prefs.*` keys, en-US and pt-BR.

## Judgment calls

1. **Five settings, not the plan's full list.** Tablet curves,
   simplified thumbnails and performance knobs each need their engine
   side wired first (the global pressure curve has no consumer yet). Each
   arrives with its feature, as a row in this window; the window and the
   persistence are the reusable part.
2. **No `egui::Modal`.** A modal would block the viewport behind it, and
   a scale change wants to be watched live. This is a plain closable
   window.

## Unverified

- The live look, and the UI-scale change on a HiDPI screen: manual.
