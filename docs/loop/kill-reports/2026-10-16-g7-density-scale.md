# Kill report: g7-density-scale (data half)

- Authorship: dragon (solo, host-down pass).
- Kill-lock `wgui2-density-scale`:
  `crates/umber-app/src/preferences.rs`,
  `crates/umber-gui/src/theme.rs`; the two catalogs (unlocked surface,
  shared with the first-run kill's committed-pending hunks).
- Implements GUI plan row `g7-density-scale` (gui-plan:151; B:F9
  from the Blender research — resolution scale relative to native —
  and B:F10's larger hit zones for area edges, aimed at tablet
  painters): the DATA half. The shell half (the Preferences row + the
  dock style application) is queued behind the gesture-integration
  lock on main.rs.

## Built

- `preferences.rs`:
  - `ui_scale_auto: bool` (default ON — Auto is the row's specified
    default: "Auto = native"). While on, `style_delta` emits NO zoom
    write (the OS's native `pixels_per_point` rules); a manual
    `ui_scale` value persists and survives a toggle back.
  - Serialize/parse round-trip (`ui_scale_auto=` line; a bad value
    keeps the default, the same leniency as every other key).
  - Tests: `auto_scale_follows_native_until_disabled` (auto → no
    zoom delta at ui_scale=1.5; manual → Some(1.5); round-trip keeps
    both); the lenient-parse test extended with the auto line's bad
    value.
  - The existing `style_delta_is_empty...` test re-pinned to the new
    contract: a manual scale write turns auto off (the editor's
    contract — writing a number IS leaving Auto).
- `theme.rs`:
  - `HitZone { Standard, Tablet }` (Default = Standard), resolving to
    `extra_interact_width` 4.0 / 8.0 pt; `SPLITTER_HIT_ZONE`
    (4.0 = egui_dock 0.21.1 `SeparatorStyle::extra_interact_width`'s
    own default, verified against the registry source) and
    `SPLITTER_HIT_ZONE_TABLET` (8.0).
  - Test: `hit_zones_resolve_to_grab_widths` (defaults + both widths).
- i18n: `prefs.ui-scale-auto` + `prefs.splitter-hit-zone` in BOTH
  catalogs (en-US, pt-BR).

## Judgment calls

- **Auto as a bool beside the f32, not 0.0-as-Auto or an enum**: the
  manual value must SURVIVE a toggle back to Auto (the row says Auto
  = native; a user's 1.5 should still be there when they leave Auto).
  A `0.0 = Auto` sentinel destroys the value; an enum
  `ScalePreference::{Auto, Manual(f32)}` rewrites the whole field's
  parse surface for the same behavior. The bool + f32 pair keeps the
  existing `ui_scale=` lines valid (back-compat: old files parse with
  auto defaulting ON).
- **Auto defaults ON**: the row's "Auto = native" is the listed first
  mode, and first-run users get native scale until they choose.
  Existing prefs files (with `ui_scale=1.0`) behave identically.
- **The re-pin of `style_delta_is_empty...`**: under the new
  contract a scale change with auto=on emits no delta — the test's
  "scale moves" case now sets `ui_scale_auto: false` explicitly,
  which IS what the Preferences editor will write when a user edits
  the number.
- **HitZone in umber-gui (not the app)**: the token belongs with
  Density in the theme crate where every style token lives; the app
  only persists the choice.

## Unverified

- Battery (host down since 18:2x). Static: rustfmt PASS on both
  files; the serialize/parse/delta logic is covered by the tests
  (13 prefs tests, 9 theme tests, all host-parked); serde derives
  already imported in theme.rs.

## Deferred

- The shell half (queued on the main.rs lock release): the
  Preferences row's Auto checkbox + hit-zone combo, and the dock
  style application (`dock_style.separator.extra_interact_width =
  hit_zone.extra_interact_width()`).

## Process note

- The catalogs carry this kill's keys beside the first-run kill's
  committed-pending `setup.*` hunks — both ride W-GUI-2's commit.
