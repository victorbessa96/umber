# Design brief — wave-abr: .abr brush preset import (row 267)

- **Status:** design only. Implementation wave follows (brush crate lane,
  free of the sibling's svg-core work on umber-core).
- **The gap:** ROADMAP `wave-abr` — ".abr brush preset import, the one
  legal Adobe format" (Adobe's Additional Terms cover .sbsar/.spp
  execution; .abr READ is the sanctioned exception, same posture as the
  read-only .spp import already landed).
- **Reference, already vendored:** `docs/claw-artifacts/ag-psd/src-tree/src/abr.rs`
  — a Rust port of ag-psd's `read_abr` (upstream abr.ts). Clean-room
  rule satisfied by pattern-porting from that vendored source, NOT by
  adding ag-psd as a dependency. Format notes from its header: `8BIM`
  sections `samp` (v6/7 stamp+params), `desc` (descriptor-based params),
  `patt`, `phry`; the stamp is an 8-bit grayscale RLE bitmap (RLE with
  uint16 lengths — the local `read_data_rle` note explains why a
  separate copy is needed vs psdReader's budgeted reader).

## Target shape (what an .abr becomes)

A `BrushPreset` (`crates/umber-brush/src/preset.rs`) with:

| .abr field (samp/desc) | Umber field |
|---|---|
| stamp bitmap (grayscale, RLE) | `params.stamped = true` + `<stem>.stamp.png` RGBA8 sidecar, alpha = gray/255 (the existing sidecar convention, preset.rs:815) |
| diameter (px) | base dab radius = diameter/2 |
| hardness 0..100 | `params.hardness` = /100 |
| spacing % | `SpacingConfig` percent |
| angle / roundness / flip | `stamp_angle` + documented non-goals (roundness/flip need anamorphic dabs — out of scope, named) |
| name | `name` (Pascal string in the header) |
| everything else (dynamics, jitter, scattering, dual brush) | non-goals, listed in the import report |

## Scope decisions

- Versions: 6, 7 (samp) and 8+ (desc where present) — mirror the vendored
  parser's version handling; refuse 1/2/5 with a named error (ancient
  formats, different layout).
- One .abr file = a brush LIBRARY (many brushes); import writes one
  `.umberbrush` + sidecar per brush into `assets/brushes/` (or a
  `--out DIR`), skipping duplicates by blake3 of the stamp.
- CLI: `umber-cli import-asset --kind brush` gains `.abr` acceptance
  (multi-preset fan-out), reusing the existing validated-import lane and
  its report; the per-brush mapping table above is the contract.
- No .abr WRITING ever (same wall as .spp writing).

## Pins the implementation wave must land

- A synthetic v6 .abr (one brush: 8x8 RLE stamp, known params) parses to
  exactly the expected name/diameter/hardness/spacing and a stamp PNG
  whose alpha equals gray/255 byte for byte.
- RLE decode round-trips against a hand-computed fixture.
- v1/v2 refusal names the version.
- Multi-brush file fans out to N presets, duplicate stamps deduped.
- End-to-end: imported preset loads in `preset_library`, validates, and
  paints one dab headless with the stamp's coverage.
