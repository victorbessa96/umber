# Kill brief — row 287: projection tiling (R-A2 v2a)

You are a claw in the Umber dragon loop. Read
`docs/loop/briefs/2026-11-08-projection-tiling-design.md` FIRST — it is the
contract; this brief is the dispatch wrapper. Implement exactly that design.

## Hard rules

- NEVER commit, push, stash, checkout, reset, or run cargo/rustfmt — the
  dragon owns all of those. Your only writes are the source edits below.
- Your lock is `claw-projection-tiling` over EXACTLY:
  `crates/umber-app/src/projection.rs`,
  `crates/umber-app/src/display_panel.rs`,
  `crates/umber-app/src/main.rs`,
  `assets/i18n/en-US.ftl`,
  `assets/i18n/pt-BR.ftl`.
  Do not touch any other file. Do not call `scripts/kill-locks.sh`.
- Match existing style; no new dependencies (std + existing crates only).
- The i18n catalogs must stay complete: add the two new keys to BOTH
  en-US.ftl and pt-BR.ftl (the completeness test scans both).
- No em-dashes in user-facing strings.

## The change (summary — the design doc has the full contract)

1. `projection.rs`:
   - `ProjectionState` gains `pub tiling: bool`; `Default` sets `false`;
     `adopt()` does not touch it (pin).
   - `RgbaImage::sample_bilinear_wrapped(img_pt)`: same bilinear core as
     `sample_bilinear` but tap indices wrap with `rem_euclid` into the image
     bounds (seamless tiling, not clamp, not zero-border). NaN/inf -> `[0;4]`.
   - `rasterize_projection`: when `state.tiling`, wrap `img_pt` with
     `rem_euclid` into `[0, w) x [0, h)` image px and sample wrapped; else the
     existing path, byte-identical.
   - Pins 1-6 from the design doc (tiling-off identity, wrap equality, seam
     adjacency, NaN guard, adopt-keeps-flag, outside-frozen-rect still 0).
2. `display_panel.rs`: `ProjectionRow` gains `pub tiling: &'a mut bool`;
   `projection_rows` draws a Tile checkbox (enabled with an image) that
   writes through the borrow, next to Arm. Panel pin (pin 7).
3. `main.rs` `projection_section`: pass `tiling: &mut state.tiling` into the
   row.
4. Both .ftl catalogs: `overlays.projection-tile` and
   `overlays.projection-tile-hint` (pt-BR in matching register).

## Out of scope (named cuts)

No repeat count, no overlay change, no mirror tiling, no UV-space tiling, no
live projector, no serialization. Do not "improve" beyond the design.

## When done

Report: files changed, pin names added, any deviation from the design
(named, with why). Exit 0. The dragon runs fmt/layers/scorecard/clippy/test,
commits with the lock, pushes, and watches release CI.
