# Kill report — 2026-10-19 picker-sliders-recents (W-C3a lane G1)

- **Author:** claw: `opencode/deepseek-v4.1-flash-free`
- **Kill-lock:** `picker-sliders-recents`, files:
  - `crates/umber-gui/src/widgets/color_picker.rs`
  - `crates/umber-gui/src/widgets/swatch_grid.rs`
  - `crates/umber-gui/src/widgets/mod.rs`
  - `crates/umber-gui/src/lib.rs`
  - `docs/loop/kill-reports/2026-10-19-picker-sliders-recents.md`
- **Design source:** `docs/loop/next-wave-plan-draft.md` §1 lane G1, plus
  `docs/loop/briefs/2026-10-19-picker-sliders-recents.md`. The brief closes
  queue row C2-12's open half (the g4 spec at
  `docs/specs/gui-plan-2026-10-15.md:118`).

## Built

All changes are ADDITIVE. `color_button(ui, linear)` is byte-unchanged; no
caller signature moved, so umber-app compiles at every point of the wave and
none of its files were touched.

### `crates/umber-gui/src/widgets/color_picker.rs`

- `ColorState::set_srgb_component(i, v)` — sets one sRGB-encoded component
  (0..=1, clamped) through `set_linear`, so the grey rule (s = 0 keeps the
  hue) is the same one a typed hex grey uses. `i >= 3` and non-finite `v`
  are no-ops, like `set_hue`/`set_sv`.
- `PickerLabels { rgb: [String; 3], hsv: [String; 3] }` + `Default`
  (English glyphs R/G/B, H/S/V). umber-gui has no `tr()`; the caller
  translates. No ftl keys were added (the catalogs are not in this kill).
- `ColorPicker::labels(&PickerLabels)` builder. `show()` keeps its
  signature; without labels it uses `PickerLabels::default()`, which is how
  the untouched `color_button` gets the new rows.
- Six channel rows under the hex field, HSV first-class:
  - RGB rows edit **sRGB-encoded 0..255** (the hex field's space), readout
    as an integer byte.
  - HSV rows edit the kept HSV through `set_hue`/`set_sv`; H reads 0..360,
    S/V read 0..100. The readouts are numbers only: umber-gui has no i18n,
    so no unit glyphs are hardcoded (the row labels come from the caller).
  - Each track is an `egui::Mesh` vertex grid (32 segments, no texture —
    the SV-square precedent) tinted across that channel's own range at the
    current values of the other components. The handle is the hue strip's
    white bar. Rows are laid out at the picker's `size` (not
    `ui.available_width`, which inside an `Area` is the 600pt default).
- `color_button_with(ui, linear, &PickerLabels, &mut RecentColors)` — a
  sibling of `color_button`, same swatch/popup contract: the popup holds a
  `ColorPicker` plus the `SwatchGrid` of recents. Clicking a recent swatch
  sets the colour. A colour is pushed to `RecentColors` **once, on the
  open → closed transition**, and only when the final linear colour differs
  from `ColorState::previous` (the colour the popup opened with) by more
  than `SAME_COLOUR_EPS = 1e-6`. Nothing is pushed per drag frame.
- Module docs describe the rows, the labels and the recents contract.

### `crates/umber-gui/src/widgets/swatch_grid.rs`

- Module docs only: `RecentColors` now has a consumer (`color_button_with`)
  and persistence is explicitly a later kill. No API change.

### Registration (rule 13)

- `widgets/mod.rs`: `pub use color_picker::{color_button_with, ColorPicker,
  ColorState, PickerLabels};`
- `lib.rs`: both new public names re-exported.

### Tests written (8, never run — the dragon gates on the build host)

| Test | Pins |
|---|---|
| `an_srgb_component_round_trips_128` | `set_srgb_component(0, 128/255)` stores `srgb_to_linear(128/255) = 0.21586050`, reads back to the byte 128 and `#800000`; the other two components are untouched |
| `setting_the_three_components_to_a_grey_keeps_the_hue` | Red → G=0.5, B=0.5, R=0.5 → grey with hue still exactly 0 and s = 0; `set_sv(1, 1)` restores red |
| `the_channel_tracks_are_tinted_across_their_range` | The exact `Color32` tints at 0 / 50 / 100 % for the R row of sRGB (0.2, 0.4, 0.6) and the Hue row at s = v = 1 |
| `dragging_a_channel_slider_changes_the_colour` | kittest: press at 25 % of the G row, drag to 75 %, release → `srgb()[1] ≈ 0.75`, R/B unchanged |
| `a_channel_slider_reports_a_slider_role_to_accesskit` | All six rows expose `Role::Slider` under their labels (g7 seam) |
| `recents_are_pushed_once_when_the_popup_closes` | open, three drag frames in the SV square → recents still empty; close → exactly 1 entry, equal to the final colour and not the opening red |
| `closing_without_a_change_records_nothing` | open + close with no edit → 0 entries |
| `clicking_a_recent_swatch_sets_the_colour` | the popup's `SwatchGrid` cell click sets `linear` to green and marks the button changed |

### Gradient derivation (python port of the vertex colours)

Port of `color32` (Rust `f32::round` is half-away-from-zero, mirrored with
`+ 0.5` here), `hsv_to_rgb`, and `srgb_to_linear` from the file:

```python
import math

def c8(c):
    c = min(max(c, 0.0), 1.0)
    return int(c * 255.0 + 0.5)

def srgb_to_linear(c):
    c = min(max(c, 0.0), 1.0)
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4

def linear_to_srgb(c):
    c = min(max(c, 0.0), 1.0)
    return c * 12.92 if c <= 0.0031308 else 1.055 * c ** (1 / 2.4) - 0.055

def hsv_to_rgb(h, s, v):
    h = (h - math.floor(h)) * 6.0
    i = int(math.floor(h)) % 6
    f = h - math.floor(h)
    p, q, t = v * (1 - s), v * (1 - s * f), v * (1 - s * (1 - f))
    return [[v, t, p], [q, v, p], [p, v, t], [p, q, v], [t, p, v], [v, p, q]][i]

# R row: the current sRGB colour is (0.2, 0.4, 0.6); sweep R.
for t in (0.0, 0.5, 1.0):
    c = [t, 0.4, 0.6]
    print(t, c, tuple(c8(x) for x in c))

# Hue row: current HSV (0, 1, 1); sweep H.
for t in (0.0, 0.5, 1.0):
    rgb = hsv_to_rgb(t, 1.0, 1.0)
    print(t, rgb, tuple(c8(x) for x in rgb))

enc = 128.0 / 255.0
lin = srgb_to_linear(enc)
print(enc, lin, linear_to_srgb(lin) * 255)
```

Output:

```
R row    t=0.0  [0.0, 0.4, 0.6]  -> (0,   102, 153)
R row    t=0.5  [0.5, 0.4, 0.6]  -> (128, 102, 153)   # 0.5 * 255 = 127.5 -> 128
R row    t=1.0  [1.0, 0.4, 0.6]  -> (255, 102, 153)
Hue row  t=0.0  (1, 0, 0)        -> (255, 0, 0)
Hue row  t=0.5  (0, 1, 1)        -> (0, 255, 255)
Hue row  t=1.0  (1, 0, 0)        -> (255, 0, 0)       # hue 1 == hue 0
128/255 = 0.5019607843137255
srgb_to_linear(128/255) = 0.21586050011389926
back to bytes: 128.0 -> 128
```

These are exactly the pins in `the_channel_tracks_are_tinted_across_their_range`
and `an_srgb_component_round_trips_128`.

## Judgment calls

1. **Custom slider rows, not `egui::Slider`.** `Slider` has no tinted-track
   hook, and the brief asks for a `Mesh` gradient. Each row is
   `allocate_exact_size` (the row) + `ui.interact` over the track's x-range
   at the full 15pt row height, so the target is bigger than the 9pt track.
   The response carries `WidgetType::Slider` for AccessKit.
2. **Hue row tints at the current s/v** (the other two components), not at
   a fixed s = v = 1. S and V rows likewise sweep at the current other
   components. The python pin uses the test state's s = v = 1, so it pins
   the rendered row exactly for that state.
3. **`PickerLabels::default()` is the English glyph set.** This is what
   keeps `color_button` byte-unchanged while its picker still gains the
   rows. `color_button_with` always takes caller labels; the app translates.
4. **Recents identity is `ColorState::previous`, compared in linear RGB**
   with a 1e-6 epsilon. The picker's HSV round-trip error is ~1e-7, so the
   epsilon absorbs float noise but not a deliberate edit. The pushed value
   is `color32(state.srgb())` because `RecentColors` stores `Color32` (the
   `SwatchGrid` type).
5. **Push happens on the open → closed transition only**, detected before
   the frame's `open`/`was_open` update, and it also covers closing by the
   swatch click, a click outside, and Escape. It never runs on the opening
   frame (a fresh `ColorState` would otherwise push instantly).
6. **No `ui.separator()` in the popup.** An `Area`'s first-frame max width
   is `Spacing::default_area_size.x` (600pt); a separator would stretch the
   popup to it. A 4pt gap separates the picker from the recents grid
   instead. (Commented in the code.)
7. **`set_srgb_component` is a per-call rule, like `set_linear`.** The grey
   test edits G, then B, then R: the intermediate colours keep hue 0
   (both are red-hued), and the third edit is the one that makes the grey,
   so `set_linear`'s grey rule keeps 0. Editing R first would legitimately
   produce an intermediate hue; the guarantee is the documented per-call
   rule, not a transaction across three edits. This is the honest reading
   of the brief's "through `set_linear`'s rule".
8. **Row geometry.** The label column is 16pt and the readout column 34pt;
   `channel_rows` clamps the row width to at least 74pt so the track never
   collapses. Caller labels are documented as short glyphs (that is what
   the 16pt column buys); a long translated label will overlap the track.
9. **Recents capacity/columns** come from `RecentColors`/`SwatchGrid`
   defaults (16, 8 columns); the popup does not override them.
10. **No ftl keys and no requirements-row edit.** The ftl catalogs and
    `docs/specs/requirements.md` are outside this kill's five files, and
    the g4 picker row is not complete (deferrals below), so rule 9 does not
    apply yet. A3 names the row state when it lands.

## Unverified

- **Nothing was compiled or run** (binding discipline). The type-level risks
  are the `ui.interact` hit test over an already-allocated hover-sensed row
  (the track is registered later, so it should win), AccessKit exposing a
  manually interacted widget on its 15pt interact rect, and the `Area`
  first-frame width claim behind judgment 6.
- **kittest drag latching is unverified by execution.** `harness.drag_at`
  + a moved pointer should make `Response::dragged()` true on the same
  frame; if egui's drag threshold does not latch on a 15pt-tall target, the
  slider's `clicked()` arm still writes the value on release, but the test
  asserts the dragged result, so a failure would surface as a value miss,
  not a silent pass.
- **Visuals:** the mesh tint's smoothness at 32 segments, the white
  handle's contrast on light tracks, and the popup's height (six rows +
  recents) on a small screen. No snapshot was taken.
- **Recents grid hit-testing** relies on `SwatchGrid`'s existing
  whole-grid `Sense::click` + per-cell `contains` logic, which this kill
  did not change.

## Deferred

Named deferrals from the brief (not built here):

- **Inline eyedropper** — excluded by the wave plan (lane 8 holds
  `main.rs`/`viewport.rs`), recorded in the g4 row and the plan.
- **Linear/Perceptual readout toggle** — the g4 row's remaining UX item;
  needs colour-management knowledge that the umber-gui layer ban keeps out
  of this crate.
- **Linear-space tinted tracks** — the tracks tint in the picker's
  sRGB-encoded space (the hex field's space, the same space the SV square
  and hue strip read in). A linear-light track is a separate visual
  decision.

Also deferred, by contract:

- **A3's app migration** (the three `color_button` call sites) and
  **swatch persistence to a file** — explicitly another kill. `RecentColors`
  stays file-agnostic.
- **Translations** for the row labels — the app passes `PickerLabels`; no
  ftl keys landed.
- **`color_button` delegation** — the brief forbids touching it. When A3
  migrates every site, the plan records it becoming a thin delegate; it was
  not deleted or delegated here.

## Process note

- Read the brief, then the live `color_picker.rs`, `swatch_grid.rs`,
  `widgets/mod.rs`, `lib.rs`, the g4 spec row and the wave plan's lane G1.
  Checked egui 0.36 sources for `Ui::interact`, `Painter::text`,
  `Rect::from_min_max`/`from_center_size`, `Area`'s first-frame max rect,
  `Separator`'s available-width sizing, `WidgetType::Slider` → AccessKit
  `Role::Slider`, and `egui_kittest::Node::rect`.
- No cargo, rustc, build, test, git mutation, lock or `/tmp` operation.
  `python3` ran only the derivation above (and a brace/doc-attachment
  census). Formatting was checked with read-only
  `rustfmt --edition 2021 --check` (cargo-free); it is clean on all four
  Rust files. Brace balance and doc-attachment checks are clean.
- Other sessions' dirty files (`main.rs`, `viewport.rs`, `bake_sources.rs`,
  `paint_state.rs`, `id_mask.rs`, the two untracked reports) were read-only
  context and were not touched.

## One-screen summary

**Files changed (4 code + this report):**
`crates/umber-gui/src/widgets/color_picker.rs` (sliders, `set_srgb_component`,
`PickerLabels`, `color_button_with`, 8 tests),
`crates/umber-gui/src/widgets/swatch_grid.rs` (docs),
`crates/umber-gui/src/widgets/mod.rs` (exports + docs),
`crates/umber-gui/src/lib.rs` (exports).

**Tests added (8):**

1. `an_srgb_component_round_trips_128` — 128 → `srgb_to_linear(128/255)` →
   128, `#800000`, other components untouched.
2. `setting_the_three_components_to_a_grey_keeps_the_hue` — grey from red
   keeps hue 0; S back restores red.
3. `the_channel_tracks_are_tinted_across_their_range` — R row
   (0,102,153)/(128,102,153)/(255,102,153); hue row
   red/cyan/red at 0/50/100 %.
4. `dragging_a_channel_slider_changes_the_colour` — kittest drag on the G
   row lands at sRGB 0.75, R/B unchanged.
5. `a_channel_slider_reports_a_slider_role_to_accesskit` — six `Role::Slider`
   nodes.
6. `recents_are_pushed_once_when_the_popup_closes` — 0 entries during three
   drag frames, exactly 1 on close, equal to the final colour.
7. `closing_without_a_change_records_nothing` — untouched open/close keeps
   0 entries.
8. `clicking_a_recent_swatch_sets_the_colour` — the popup's swatch grid
   click applies green and marks the button changed.

## Addendum — the first battery run (2026-10-22, local-7de7d3ed)

The close's battery — the first real run of these bytes — caught one
red: `setting_the_three_components_to_a_grey_keeps_the_hue`, whose
first assert (`hsv() == [0.0, 1.0, 1.0]` right after a full-bright
construction) read v as 0.99999994. Root cause: `linear_to_srgb`'s f32
rational at exactly 1.0 — `f32(1.055) - f32(0.055) = 0.99999994`, the
constant-subtraction artifact — reached through `new()`'s
`rgb_to_hsv(linear.map(linear_to_srgb))`. The test's expectation is the
IEC-correct endpoint (exactly 1.0), so the fix is production-side:
`linear_to_srgb` is now boundary-exact at the top (`c >= 1.0 → 1.0`).
An independent f32 port verified the fix: every value below the
endpoint is bit-identical to the old formula (0.9999999 included), and
at the endpoint the output is exactly 1.0; the leg's other 134 tests
are untouched. `color_picker.rs` re-pinned (6a61d1a607bf4ede, the
reason recorded in the manifest header); the test itself is
byte-unchanged — the pin was right, the production was half a ULP
wrong.
