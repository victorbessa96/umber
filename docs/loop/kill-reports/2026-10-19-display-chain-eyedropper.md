# Kill report: display-chain-aware eyedropper (B-10's named follow-up)

Date: 2026-10-19. Author: opencode claw (deepseek-v4.1-flash-free), for the
dragon. Lock: `b10-display-chain-eyedropper` over
`crates/umber-app/src/viewport.rs`, `crates/umber-app/tests/panel_kittest.rs`
and this report. `crates/umber-app/src/main.rs` also carries one sync line —
see the judgment call and the honest-state note; it is not under any live
kill-lock (`bash scripts/kill-locks.sh list`).

## Context

`docs/loop/queue.md:65` records B-10 as landed with one named follow-up:
"display-chain-aware sample = the named follow-up". The viewport's picker
(`crates/umber-app/src/viewport.rs`) read the paint target's LINEAR RGBA8
texel, converted it to sRGB bytes with `linear_to_srgb_byte`, and returned
`PickSample`. Its own comment said a display-transform-aware sample was the
follow-up. The artist meanwhile sees the mesh through the viewer chain
(`umber_color::apply_display_chain`, applied by the GPU display LUT), so an
edited Display panel made the pick diverge from the screen.

## The settings source (scouted, then read, not assumed)

- `AppState::display: umber_color::DisplaySettings` — `main.rs:1285`.
- The Display panel edits that same struct: `display_panel::show(ui,
  &mut DisplaySettings, …)` called from the `Panel::Display` arm
  (`main.rs:1172-1178`).
- The preview strip transforms the same struct:
  `PREVIEW_RAMP.map(|l| apply_display_chain([l, l, l], s))`
  (`display_panel.rs:601-603`).
- The shell's `PanelViewer` borrows it as `display: &'a mut
  DisplaySettings` (`main.rs:402`).

Read of `umber-color/src/display.rs`: `apply_display_chain` (no-ocio
definition at `:186`, ocio build delegates at `:180`) is exposure (`×2^ev`),
then `apply_display(view)`, then gamma `v^(1/gamma)`. **`DisplaySettings`'s
default is written by hand as `Raw / 0.0 EV / 1.0 gamma` (`:120-133`), not
the enum's `Srgb` default, and that chain returns the input bit-for-bit**
(identity fast paths for exposure 0, view Raw, gamma 1). So "default =
current v1 behavior" holds only if the v1 sRGB bytes are the chain's input.

## Census of eyedropper/pick sites

Only **one** site sets the brush color from a pick:

1. **Viewport pick** — `viewport.rs:730-766` (`ViewportClick::Pick(PickSample)`)
   → shell `main.rs:558-574` (`ViewportClick::Pick` arm) → `paint.set_brush_color`.
   This is the fixed site.
2. **UV view `read_tile`** (`uv_view.rs:441`) is the Shift-hold **pixel
   inspector** readout, not a brush-color pick; untouched.
3. The `g4-color-picker` swatch (`brush_panel.rs`) edits the brush color
   through its own popup; it is not an eyedropper and has no texel sample.
   Untouched.
4. The "strip" (`ToolKind::Pick`) is the toolbar tool state that arms the
   viewport picker (`main.rs:521-522`); it does not sample anything itself.

No other picker callback exists.

## What changed

**`crates/umber-app/src/viewport.rs`**

- `:181` — import `apply_display_chain, DisplaySettings` from `umber-color`.
- `:66-78` — new module-doc section recording the landed behavior and the
  two judgment calls (input space, OCIO not threaded).
- `:211-219`, `:224` — `PickSample`/`ViewportClick::Pick` docs now say
  "display-chain sRGBA".
- `:244-257` — new pure helper `display_chain_srgba([u8; 4],
  &DisplaySettings) -> [u8; 4]`: divides the three sRGB bytes by 255, calls
  `apply_display_chain`, quantizes with the preview strip's exact
  `(c.clamp(0,1) * 255).round()` rule, and passes alpha through.
- `:307-313` — new `display_settings: DisplaySettings` field on `Viewport`
  (derived `Default` gives the identity chain).
- `:738-766` — the pick site now builds the v1 sRGB sample and feeds it
  through `display_chain_srgba(…, &self.display_settings)` before returning
  `ViewportClick::Pick(PickSample(..))`.
- `:1481-1488` — `set_display_settings(&DisplaySettings)` syncs the shell's
  struct each frame (a `clone_from`; the settings are a small enum + two
  floats, plus the OCIO selection in `ocio` builds).

**`crates/umber-app/src/main.rs`**

- `:523-526` — one sync beside the existing picker-arm sync:
  `self.viewport.set_display_settings(self.display);`. This is the only
  way the settings reach the viewport: `PanelViewer.display` is the session
  state and `ui_with_home` does not carry it.

No i18n keys were added or changed. `crates/umber-app/tests/panel_kittest.rs`
was left untouched: the existing kittest pattern covers the Display panel's
widgets, and a viewport pick needs a GPU/paint/mesh fixture that is not a
cheap extension (`do not force it`, per the brief).

## Tests added (`viewport.rs` `#[cfg(test)]`)

- `eyedropper_picks_the_display_chained_texel` — known linear texel
  `[64, 128, 200, 173]`, known non-default settings
  (`view: Srgb`, `exposure: -0.5`, `gamma: 0.9`). Asserts the v1 step gives
  `[137, 188, 229, 173]`, computes the expectation by **calling
  `apply_display_chain`** on the sRGB sample and quantizing like
  `display_panel::to_color32`, and pins the exact result
  `[158, 185, 204, 173]` (hand-derived, independently re-derived with
  floor(x + 0.5)).
- `eyedropper_default_settings_keep_the_v1_srgb_bytes` — asserts
  `DisplaySettings::default().is_identity()`, that a fresh `Viewport` carries
  the identity chain, and that `[137, 188, 229, 173]` passes through
  byte-for-byte (the v1 no-LUT path).

## Honest state

- **cargo UNRUN** on this host (no cargo by contract). The build host's
  battery gates `cargo fmt`, clippy `-D warnings` and the full suites at
  commit.
- `rustfmt --edition 2021 --check crates/umber-app/src/viewport.rs
  crates/umber-app/src/main.rs` → **FMT_CLEAN**.
- `bash scripts/verify-kill.sh viewport.rs main.rs`:
  viewport braces 371/371, fmt clean, matches!-arity clean, doc attachment
  clean. `main.rs` reports "braces 878/879 — UNBALANCED"; that count is
  **identical at HEAD** (verified with `git show HEAD:…`), a pre-existing
  brace-in-string false positive, not from this diff. The thiserror
  `source:` WARNs in `main.rs` are pre-existing `CopyPlanes`/fixture lines.
- No GPU or runtime run: the chain helper is pure and unit-tested; the click
  wiring is reasoned, not executed.

## Adoption (local-10e6ed18, 2026-10-20, host-free under the code steer)

The dispatching session is dead (fleet-era, 10-19; no live writer on
either file — mtimes static since, the session census clean). This
session adopts the kill under the full rules: holder exit confirmed,
a fresh lock registered (`b10-eyedropper-adoption`: viewport.rs,
main.rs, panel_kittest.rs, this report), the files treated as
UNVERIFIED and the static gate re-run by the adopter rather than
trusted from the claw's report above:

- `rustfmt --edition 2021 --check` on both files: CLEAN (re-derived
  this session; the claw's FMT_CLEAN claim was not trusted).
- `verify-kill.sh` re-run: balanced, trap-scanned; the main.rs brace
  count "878/879 UNBALANCED" re-confirmed pre-existing at HEAD (the
  same in-string false positive, checked via `git show HEAD:`).
- The two unit tests read in full; their expectations re-derived
  independently in Python (the linear→sRGB step, then the exact
  `apply_display_chain` math ported from umber-color's source, then
  the strip's quantize rule): the pinned bytes match the port's
  output. The port and its output live in the adoption record of the
  coordination note.
- A manifest is pinned for the commit row (`.dragon/b10-eyedropper-
  manifest.txt`: viewport.rs, main.rs, this report). panel_kittest.rs
  was never edited (the report's own honest-state note) and is not
  in the manifest or the row.

The kill joins the W-C3a recovery fleet: the close script's
eyedropper row commits it (battery-green gate unchanged), releasing
this adoption lock at its commit. The first live run of its two
tests is the recovery battery.

## Judgment calls

1. **Chain input is the display-encoded sRGB sample, per the brief's stage
   order** (`linear_to_srgb_byte`, then `apply_display_chain`). This is what
   makes the default identity settings reproduce the v1 bytes exactly. Note
   for the record: the GPU LUT's input is the *linear* post-lighting value,
   so under a non-Raw view this is the "display-encoded sample" convention
   the existing picker already used, not a bit-exact match to the LUT's
   linear-input chain. If the intent is the latter, the follow-up is to feed
   the linear texel and invert the chain on the consumer (a main.rs change).
   The shell still decodes the returned sample with sRGB→linear
   (`main.rs:558-574`), so it does not invert exposure/gamma; the brief scopes
   this kill to the sample, and a full chain-inverse on the consumer is a
   separate follow-up.
2. **`main.rs` touched outside the declared lock.** The settings only exist
   in `PanelViewer`/`AppState`; a viewport-local implementation cannot reach
   them. The edit is one sync line beside the existing `set_pick_armed` sync.
   No live lock covers `main.rs`; it is flagged here for the dragon's review.
3. **OCIO not threaded.** `apply_display_chain` uses the fixed view stage
   (it delegates to `apply_display_chain_with(…, None)` in `ocio` builds and
   falls back to the fixed view when a selection has no processor). The
   preview strip threads the adopted processor only in `ocio` builds; the
   viewport has no processor handle, so an adopted config's pick uses the
   fixed view. Recorded in the module docs; deferred.
4. **Alpha passes through** unchanged, exactly as before.
5. **No kittest extension** (see above); the pure helper is the pinned
   surface.

## Addendum (2026-10-21 00:1x, local-4d2e8a71): the battery catch, fixed forward

The first local battery (the degraded-mode grant) flagged the adoption's
test block: the DisplaySettings literal ended with a needless
`..DisplaySettings::default()` — all three fields already specified
(clippy::needless_update, a `-D warnings` red). Removed; the b10
manifest's viewport.rs row re-pinned with this note's reason. The
adoption's re-derived pins ([137,188,229,173] -> [158,185,204,173])
are untouched by the edit — the literal's three specified values are
the chain's inputs, exactly as re-derived.
