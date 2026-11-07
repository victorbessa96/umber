# Kill report — the `.abr` reader against REAL files (row 271)

- **Kill:** `abr-realfile` — the first verification of the clean-room
  `.abr` reader against files Photoshop actually wrote, and the two
  semantic corrections that verification forced.
- **Author:** dragon (session local-1625-rz6f2a), lock
  `razul-abr-realfile` (`crates/umber-brush/src/abr.rs`, the new test +
  fixtures, STATE/ROADMAP, this report).
- **Predecessor:** row 268 (`ca70ea80`, the clean-room reader itself,
  claw-authored + dragon-corrected).

## What the real files proved

The upstream `ag-psd` test corpus (MIT, Agamnentzar) carries three REAL
`.abr` files. They were trimmed to their `samp` sections (the section
bytes are untouched) and vendored under
`crates/umber-brush/tests/fixtures/` with the provenance and the MIT
text in `FIXTURES.md`.

Against them, the reader was run and compared with ag-psd's own
expected values (`data.json` beside each fixture) and with ag-psd's
oracle PNG (whose ALPHA CHANNEL is the sample plane):

| fixture | upstream | version | reader's result | oracle |
|---|---|---|---|---|
| `abr_real_v6_2.abr` | sample-and-pattern | 6.2 | 1 sample, id `a8f34db0-4814-634e-807c-c61aecde18fd`, at (2,1), 112×105 | **the 11,760-byte plane is byte-identical to the oracle PNG's alpha channel (0 differing bytes)** |
| `abr_real_v10_2.abr` | special | 10.2 | 1 sample, id `80b60ae0-c4b0-11d5-869d-ee7a7e70f1aa`, at (295,195), 23×20 | upstream `data.json` agrees |
| `abr_real_v6_2_no_samples.abr` | simple | 6.2 | 0 samples (a computed-only brush) | upstream `data.json` agrees (no samples) |

This closes the "no real `.abr` has been parsed" debt that the 2026-10-15
wave-abr kill report left open.

## The two semantic corrections it forced

1. **A `samp` record is a SAMPLE, not a brush.** Real files put the tip
   bitmaps in `samp` (each with a UUID-ish id, an absolute position and a
   size) and the BRUSHES — names, sizes, hardness, spacing, dynamics —
   in the descriptor (`desc`) section. The reader was renamed and
   reshaped accordingly: `AbrBrush` → `AbrSample` with `id`, `x`, `y`,
   `width`, `height`, `alpha`; `read_abr` → `read_samples`;
   `MAX_BRUSHES` → `MAX_SAMPLES`. The x/y position was previously
   DROPPED — a real data loss for a tip that sits at an offset.
2. **Real files use v6, v7, v9 AND v10.** The v6/v7-only gate refused a
   real v10 file; the gate now accepts the four versions the format
   authority accepts (v8 is not a thing, v1-v5 predate the `8BIM`
   section layout).

## Pins

`crates/umber-brush/tests/abr_real_file.rs` (4 tests):

- the v6.2 file's sample id/position/size, the plane's FNV-1a 64
  checksum, its byte sum, its non-zero count, and five exact texels —
  all taken from the upstream oracle;
- the v10.2 file's sample id/position/size;
- the computed-only v6.2 file parsing to zero samples;
- the v1 refusal with its named error.

## Gates

`cargo clippy -p umber-brush --all-targets -- -D warnings` clean;
`cargo test -p umber-brush --all-targets` → 190/0 (lib) + 4/0 (the new
integration test). Log: `/tmp/umber-abr-realfile-gate3.log`.

## Still open (named)

- **The descriptor wave**: the `desc` section holds the brush settings.
  Until it is parsed, an `.abr` file yields TIPS, not presets — and the
  app's existing ag-psd-based import path
  (`crates/umber-app/src/brush_panel/abr_import.rs`) remains the only
  route to a full `.umberbrush` + `.stamp.png` preset.
- **The mapping wave**: samples → presets (the sidecar convention the
  app already writes for hand-picked stamps).
