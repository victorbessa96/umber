# Kill report: stamp preset persistence (2026-10-18)

Dragon-authored host-free by the Umber /loop session "Work on Umber
project coordination #3" (local-d3deea1d3c7631cc80f8782e73c37fe384c4c50d1248d89def19e4801b4c9a45, wakeup #2). Kill-lock
`stamp-preset-persist`, holding exactly one file:
`crates/umber-app/src/brush_panel.rs`.

Context: C2-11's open tail — "stamp-image persistence with presets
open". Scouting found the LOAD side already ships
(`load_stamp_for_active`: a preset with `params.stamped` restores the
`<stem>.stamp.png` sidecar beside it — the `.abr` import convention).
The gap was the SAVE side: a hand-picked stamp (`stamp_image_rows`)
lived only in the paint session — the picked path was dropped in the
closure, `save_active` wrote JSON only, and the stamp died with the
session.

## Built

- `BrushPanel.picked_stamp: Option<PickedStamp>` — the picker's
  decoded RGBA (w, h, bytes) retained when `set_stamp` ACCEPTS the
  image (a refused upload retains nothing; a later explicit save then
  writes what actually stamped). The Reset button clears it with the
  engine stamp.
- `load_stamp_for_active` clears the pick on a preset SWITCH (the
  pick belonged to the preset being edited; the sidecar loading on
  the switch — if any — is the new preset's own tip). The per-frame
  early-return guard means a pick made while a stamped preset stays
  active SURVIVES frames — only a real switch clears it.
- `save_active` / `save_as`: a retained pick marks
  `params.stamped = true` (the flag the load path keys on) and rides
  the same explicit save through one shared helper —
  `write_stamp_sidecar` writes the `<stem>.stamp.png` beside the
  preset (`.abr`'s exact shape: RGBA PNG, linear transfer, the picked
  image's own bytes; `stamp_coverage` re-derives at load exactly as
  the picker did live). A sidecar write failure reports through the
  existing `save_error` surface and leaves `dirty` set — a retry
  rewrites both halves. No pick: neither file nor flag is touched
  (a sidecar preset's own tip stands).
- No new i18n keys (the picker's existing choose/reset buttons carry
  it); no engine changes (`PaintState` untouched).

## Judgment calls

1. Retain RGBA, not coverage: the sidecar format is `.abr`'s (a real
   RGBA PNG), and coverage is a derived value — storing bytes keeps
   the round trip lossless and the load path unchanged.
2. `stamped = true` on save is a semantic commit: the user picked a
   stamp image for this preset, so the preset IS a stamp brush now —
   the same flag `.abr` import sets. Reset does NOT clear the flag or
   sidecar (reset is a session action; the preset still "is" a stamp
   brush until saved otherwise).
3. Sidecar failure keeps `dirty` set: a partial save (JSON written,
   sidecar not) must not read as clean — the honest retry rewrites
   both.

## Unverified

- No compile run (host down since 10-17 21:48; the outage contract
  parks the battery). Compile risk swept statically: every symbol
  grep-verified in-file (`PickedStamp`, `write_stamp_sidecar` defined
  once + called twice, `picked_stamp` wired at constructor, show,
  picker, loader, both save paths); `umber_export::png::write_png` /
  `read_png_rgba8` match the exact shapes `abr_import` and
  `load_stamp_for_active` already use; the picker closure's captures
  mirror the existing `paint` capture with one more `&mut`.
- Test legs run at the battery (pure fs + PNG codec, no GPU needed).
  The coverage pin was corrected by the python port BEFORE parking:
  `stamp_coverage([10,20,30,128])` = luma 18.596 × 128/255 = 9.33 →
  9, not the 10 first written — the port caught a wrong literal in
  the kill's own test.

## Gates run (host-free, this session)

- `rustfmt --check --edition 2021`: CLEAN (write-mode pass applied
  its own wrap to the new signature).
- `scripts/verify-kill.sh`: PASS (brace census, trap greps,
  doc-attachment, lock audit — the `stamp-preset-persist` lock live).
- Interface coherence: single-file kill, no cross-file imports added;
  all helpers local; tests use the module's existing
  `fresh_tmpdir` / `write_preset` / `test_preset` idioms.

State: STATICALLY VERIFIED, IN FLIGHT — battery (scope: umber-app) +
commit + push parked on the build host's return, landing in the same
recovery session as the stencil wave, e0063, and the two agent kills
(own commit row per the manifest; brush_panel.rs is on no other
manifest).
