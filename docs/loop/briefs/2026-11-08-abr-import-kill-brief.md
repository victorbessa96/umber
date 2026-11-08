# Kill brief — row 283: .abr → .umberbrush mapping (wave-abr, pure brush crate)

You are a claw in the Umber workspace (`/home/bessa/Projects/umber`, branch
master). You know nothing except this brief. READ first:
- `docs/loop/briefs/2026-11-07-wave-abr-design.md` — the target-shape table
  (the CONTRACT for this row);
- `docs/loop/briefs/2026-11-08-wave-abr-desc-design.md` — row 282's contract;
- `crates/umber-brush/src/abr.rs` — row 282's landed API: `read_abr(bytes) ->
  AbrFile { samples, brushes, skipped_shape_classes }`, `AbrBrush { tool_class,
  name, diameter_px, angle_deg, roundness, spacing_on, spacing, flip_x, flip_y,
  hardness, sampled_data_id, dynamics }`, `AbrSample { id, bounds { x, y, w, h },
  alpha }`, `match_sample`, `AbrError`; the `#[cfg(test)]` fixture builders in
  its test module (reuse them via `pub(crate)` if helpful);
- `crates/umber-brush/src/preset.rs` — `BrushPreset { name, params: BrushParams,
  one_euro, lazy_mouse: LazyMouseConfig { radius_px, .. }, spacing: SpacingConfig
  { dabs_per_radius }, .. }` with `#[serde(default)]` everywhere, `validate()`,
  `from_json`/`to_json`; the stamp sidecar convention at :815
  (`<stem>.stamp.png` RGBA8 next to the `.umberbrush`);
- `crates/umber-brush/src/dab_adapter.rs` — `BrushParams { color, alpha,
  hardness, stamped, stamp_angle: StampAngle, .. }`, `StampAngle::Fixed {
  degrees }`.

## Your file surface (touch NOTHING else)

- `crates/umber-brush/src/abr_import.rs` (NEW)
- `crates/umber-brush/src/lib.rs` (one `pub mod abr_import;` line)

Do NOT run cargo/clippy/tests. Do NOT commit or touch git. No new deps.

## What to build — `abr_import.rs`

The pure mapping from a parsed `.abr` to preset + stamp bytes:

```rust
pub struct AbrImportEntry {
    pub preset: BrushPreset,          // ready for to_json + validate()
    pub stamp: Option<(u32, u32, Vec<u8>)>, // (w, h, RGBA8) — Some for sampled brushes
    pub brush_index: usize,
}
pub struct AbrImport {
    pub entries: Vec<AbrImportEntry>,
    pub orphan_samples: u32,          // samp records no brush references
    pub brushes_without_sample: u32,  // computedBrush etc. (stamped=false presets)
    pub duplicate_stamps_skipped: u32,// identical stamp bytes already emitted
}
pub fn import_abr(bytes: &[u8]) -> Result<AbrImport, AbrError>
```

Mapping (the design table, concretely):
- `preset.name = brush.name` (empty → `format!("abr brush {}", index)`);
- `preset.lazy_mouse.radius_px = (brush.diameter_px / 2.0).clamp(1.0, 4096.0)`
  — non-finite/zero/negative diameters clamp to 1.0, never NaN into the preset;
- `preset.params.hardness = brush.hardness.clamp(0.0, 1.0) as f32`;
- spacing: `spacing_on == false` → `spacing.dabs_per_radius = 100.0` (dense,
  smooth); else `dabs_per_radius = (1.0 / (2.0 * brush.spacing)).clamp(1.0, 100.0)`
  (spacing 25% → dab every 0.25 diameters → 2 dabs per radius). Clamp guards
  spacing == 0;
- `preset.params.stamp_angle = StampAngle::Fixed { degrees: brush.angle_deg as f32 }`
  (finite guard);
- sampled brush (`sampled_data_id` Some, `match_sample` hits): `params.stamped = true`,
  stamp = `(w, h, rgba)` with **RGB = white (255,255,255), A = the samp alpha byte**
  — same white+coverage convention as the .svg stencil raster (check
  `PaintState::set_stamp` semantics to confirm white is multiplied by the stroke
  color); brushes with `sampled_data_id` Some but NO matching sample count as
  `brushes_without_sample` and emit a radial preset (stamped=false);
- computed brush (None id): radial preset, `brushes_without_sample += 1`;
- duplicate stamps: if an earlier entry's stamp bytes are exactly equal, skip the
  entry, `duplicate_stamps_skipped += 1`;
- orphan samples (no brush references the id): `orphan_samples += 1`, not an error;
- roundness/flip/dynamics/etc: non-goals, documented in the module docs.
- Every produced preset must pass `preset.validate()` — pin it in tests.

## Pins (synthetic fixtures — reuse abr.rs's test fixture builders)

- synthetic v6 .abr, one sampledBrush + matching samp (8x8 raw stamp, known
  gray ramp): `import_abr` → 1 entry; name/diameter→radius_px (32 px → 16.0),
  hardness 80% → 0.8, spacing 25% on → dabs_per_radius 2.0, angle 15° →
  Fixed(15.0); stamp w/h 8x8, alpha byte-for-byte equal to the fixture gray,
  RGB all 255; `preset.validate()` ok;
- spacing off → dabs_per_radius 100.0;
- diameter 0 / NaN → radius_px clamped to 1.0, preset still validates;
- computedBrush entry → stamped=false, stamp None, counted;
- sampledBrush with unmatched id → radial preset, counted;
- two brushes sharing one samp id → second one `duplicate_stamps_skipped`;
- one orphan samp → `orphan_samples == 1`, no error;
- every emitted preset round-trips `to_json` → `from_json` → `validate()`.

## Doc comments

Name row 283 and the design table; state the non-goals (patterns, v8+,
dual brush, texture, scattering, dynamics→engine wiring, .abr writing, and
the CLI fan-out = row 284).

Write the code, keep it compiling by careful reading (you cannot run cargo),
and stop. Report what you changed and where.
