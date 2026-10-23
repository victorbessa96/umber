# Brief: 2026-10-19 picker-sliders-recents (W-C3a lane G1)

Mandate: Bessa, 2026-10-19: "Continue working on the umber long horizon
project in all the steps, do not stop." Kill plan:
`docs/loop/next-wave-plan-draft.md` §1 lane G1 (verified anchors).
This closes queue row C2-12's open half (the g4 spec: sliders +
persisted-swatches groundwork; the gui plan at
`docs/specs/gui-plan-2026-10-15.md:118`).

## What you are building

`crates/umber-gui/src/widgets/color_picker.rs:265-290` has the SV
square, hue strip, swatch pair and hex — but NO HSV/RGB sliders, and
`RecentColors` (`crates/umber-gui/src/widgets/swatch_grid.rs:23`) has
no consumer outside umber-gui. You add, ADDITIVELY:

1. **HSV and RGB slider rows under the hex field**, each track tinted
   across its own channel range: an `egui::Mesh` gradient (the
   SV-square precedent in the same file — read it; NO texture).
   RGB rows edit sRGB-ENCODED 0..255, matching the hex field. HSV rows
   edit the kept HSV (hue survives greys — `set_sv`/`set_hue` already
   do this; read them first).
2. **New pure setters on `ColorState`**: `set_srgb_component(i, v)`
   which keeps the hue on greys through `set_linear`'s rule (grep
   `set_linear` and the grey/hue rule before writing).
3. **`PickerLabels { rgb: [String; 3], hsv: [String; 3] }`** passed in
   by the caller. umber-gui has NO `tr()`; the app translates. You add
   NO ftl keys in this kill (the catalogs live at the workspace root
   and are NOT yours).
4. **A new `color_button_with(ui, linear, &PickerLabels, &mut
   RecentColors)`** sitting beside the UNCHANGED
   `color_button(ui, linear)`. umber-app must compile at every point
   of this wave — no signature breaks, no removals. A colour is
   pushed to `RecentColors` ON POPUP CLOSE when it differs from
   `previous`, NEVER per drag frame. The popup shows the
   `SwatchGrid` of the recents, and clicking one sets the colour.
5. Registration is yours (rule 13): new public types re-export
   through `crates/umber-gui/src/widgets/mod.rs` and
   `crates/umber-gui/src/lib.rs` in this same kill.
6. A LATER kill (A3, not yours) migrates the app's three call sites
   and persists swatches to a file. Do NOT touch umber-app. Do NOT
   delete or delegate `color_button` — it stays as-is.

## Files you own (the ONLY files you may write)

- `crates/umber-gui/src/widgets/color_picker.rs`
- `crates/umber-gui/src/widgets/swatch_grid.rs`
- `crates/umber-gui/src/widgets/mod.rs`
- `crates/umber-gui/src/lib.rs`
- `docs/loop/kill-reports/2026-10-19-picker-sliders-recents.md`

## Tests you write (do NOT run them — the dragon gates on the build host)

Hand-derived pins:
- an sRGB component round-trip: 128 -> `srgb_to_linear(128/255)` ->
  back to 128 (grep the srgb helpers first).
- a grey keeps its hue: set R=G=B on a red state; hue still 0; setting
  S back restores red.
- recents push-on-close-only: open, drag 3 frames, close -> exactly 1
  entry; close unchanged -> 0 entries.
- a kittest that a slider drag changes the value (the SV kittest seam
  at `color_picker.rs:618` — read the existing kittest pattern there
  first; egui 0.36: widget call idiom `ui.add(...)`, held modifiers
  via `Event::ModifiersChanged`, `Harness::key_press` takes ONLY the
  key).
- a python port of the gradient vertex colours for ONE track
  (compute the expected tints at 0%, 50%, 100% for the R row and the
  Hue row; include the derivation in the report).

## Discipline (binding)

- The tree is SHARED. Dirty or untracked files you did not create
  (`main.rs`, `viewport.rs`, `.dragon/*`, other kills' reports) belong
  to other live sessions: never touch, restore, checkout, or commit
  anything. `git log/diff/grep` reads are fine; NO git mutations.
- NO cargo, rustc, build, or test runs of any kind. You may grep/read
  files and run `python3` for derivations. Match the files' existing
  formatting style; the dragon runs rustfmt at review.
- NO lock operations (`scripts/kill-locks.sh` any subcommand,
  `/tmp/umber-kill-locks/*`), never write to `/tmp` or anywhere
  outside the five files above.
- If any file:line anchor has drifted, re-derive from live source.
  Never invent an API — grep its definition first. egui 0.36 gotchas:
  `ui.add(ValueField::new(..))`-style `Widget` impls, NO `.ui(ui)`
  builder; `WidgetInfo` constructors via `widget_info(|| ...)`.

## Report shape

Write `docs/loop/kill-reports/2026-10-19-picker-sliders-recents.md`:
header naming authorship (claw: opencode/deepseek-v4.1-flash-free),
kill-lock `picker-sliders-recents` + its files, design source
(`docs/loop/next-wave-plan-draft.md` §1 lane G1 + this brief); then
**Built**, **Judgment calls**, **Unverified**, **Deferred** (the
inline eyedropper, the Linear/Perceptual toggle, linear-space tinted
tracks — named), **Process note**. End with a one-screen summary:
files changed + every test name + what it pins.
