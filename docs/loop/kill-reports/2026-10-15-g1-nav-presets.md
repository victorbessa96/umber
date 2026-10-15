# Kill report — g1-nav-presets (GUI plan G1)

Date: 2026-10-15. Author: dragon. Lock: `g1-nav-presets` over
`crates/umber-app/src/navigation.rs` (new), `viewport.rs`,
`status_bar.rs`, `main.rs`, `assets/i18n/*.ftl`, this report. Plan:
`docs/specs/gui-plan-2026-10-15.md` row `g1-nav-presets`. Sources:
Painter C7 (Alt+LMB rotate, Alt+MMB pan, Ctrl+Alt+LMB pan, Alt+RMB zoom);
Blender C1.1 (MMB orbit, Shift+MMB pan, Ctrl+MMB zoom) and C1.7
(Industry Compatible = Painter's scheme).

## Built

- `navigation.rs` (new), with 6 tests: `NavPreset::{Painter (default),
  Umber, Blender}` and `camera_op(button, mods) -> Option<CameraOp
  {Orbit, Pan, Zoom}>`.
  - `from_name` / `from_env` read `UMBER_NAV`.
  - `hints()` gives the preset's orbit / pan / zoom chords for the
    status bar.
- `viewport.rs`:
  - `handle_input` routes every drag through the preset. Alt+RMB is a
    new zoom drag (`ZOOM_DRAG_SENSITIVITY`); the wheel and P are
    unchanged.
  - The paint gate now refuses Alt, so Ctrl+Alt+LMB pans under Painter
    instead of painting.
  - The brush cursor hides on any preset navigation drag or while Alt is
    held.
  - `set_nav_preset` / `nav_preset`.
- `main.rs`: the preset is read from `UMBER_NAV` at startup (in
  `load_default_env`), and `StatusSnapshot.nav` passes it to the bar.
- `status_bar.rs`: the Paint context shows the paint chord, then the
  ACTIVE preset's chords, so the hints follow the preset and never lie
  about the binding. Pick / ID-mask show their click plus the preset's
  navigation.
- i18n: `hint.alt-lmb / alt-mmb / alt-rmb / mmb / shift-mmb`, en-US +
  pt-BR.

## Judgment calls

1. **The default flips to Painter's Alt+mouse.** This is a behaviour
   change for anyone used to Umber's bare-LMB orbit; `UMBER_NAV=umber`
   restores it. Design rule 1 of the plan: users come from Painter, so
   Painter's muscle memory sets the default. **Flag for Bessa:** this is
   the one visible default change in the wave.
2. **The paint chord stays Ctrl/Cmd+LMB under every preset.** Painter
   paints on bare LMB. Moving Umber's stroke to bare LMB touches the
   whole stroke/tool-arm path (eyedropper, ID-mask and fill-arm clicks)
   and is its own kill. `no_preset_steals_the_paint_chord` pins that
   navigation never claims it.
3. **An env var, not a UI.** The preference lives in the G3 preferences
   window. Until then `UMBER_NAV` keeps the preset choosable without a
   settings-persistence format being invented here.

## Unverified

- Live feel (sensitivities on Alt+RMB zoom): manual. The constant is a
  first guess.
- Whether Linux window managers swallow Alt+drag (GNOME's default
  Alt-drag moves windows): a known Painter-on-Linux issue. The Umber
  preset is the escape.

## Deferred

- Paint on bare LMB under the Painter preset (judgment call 2).
- Orbit around the last stroke, zoom-to-cursor, RMB cancels a
  navigation drag (Blender C1.1 / C1.5): the plan's g1-nav-presets
  follow-ups.
