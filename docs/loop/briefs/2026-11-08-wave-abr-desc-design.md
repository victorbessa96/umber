# Design brief — wave-abr descriptor section (row 282, follows row 268 `3f0e232d`)

- **Status:** design only; the kill brief is
  `docs/loop/briefs/2026-11-08-abr-desc-kill-brief.md`.
- **The gap:** `crates/umber-brush/src/abr.rs` (row 268) reads only `samp`
  sections and skips `desc` by length. The brushes' real settings (name,
  diameter, hardness, spacing, angle, flips, dynamics) live in the `desc`
  8BIM sections as a PSD-style descriptor, and a `sampledBrush` links to its
  stamp through the `sampledData` TEXT id. Without `desc`, the samp reader
  returns orphan tip bitmaps.
- **Format authority (vendored, clean-room pattern-port only):**
  `docs/claw-artifacts/ag-psd/src-tree/src/descriptor.rs` (descriptor core:
  version-16 wrapper, class structure, OSType dispatch, UNITS_MAP) and
  `docs/claw-artifacts/ag-psd/src-tree/src/abr.rs` lines 905-1060 (the 8BIM
  walk: `desc` → `read_version_and_descriptor` → `Brsh` list of descriptors →
  `parse_brush`/`parse_brush_shape`; `phry` → parse-and-ignore) plus lines
  378-540 (the parse helpers: `parse_percent` default 1.0, `parse_angle`
  default 0.0, `parse_units_to_number`, `parse_dynamics`).
- **Target shape:**

  | .abr desc field | key / type | Umber field |
  |---|---|---|
  | tool class | outer class id `_`/`MixB`/`SmTl` | `AbrToolClass::{Brush,MixerBrush,Smudge}` |
  | brush shape | `Brsh` → descriptor, class `computedBrush`/`sampledBrush` | parsed; other classes skipped + counted |
  | name | `Nm  ` TEXT | `AbrBrush.name` |
  | diameter px | `Dmtr` UntF `#Pxl` | `diameter_px` |
  | angle deg | `Angl` UntF `#Ang` | `angle_deg` |
  | roundness | `Rndn` UntF `#Prc` | `roundness` 0..1 |
  | spacing | `Intr` bool + `Spcn` UntF `#Prc` | `spacing_on`, `spacing` 0..1 |
  | hardness | `Hrdn` UntF `#Prc` | `hardness` 0..1 |
  | flips | `flipX`/`flipY` bool | `flip_x`, `flip_y` |
  | stamp link | `sampledData` TEXT | `sampled_data_id` (Some only for sampledBrush) |
  | dynamics | `bVTy` long, `fStp` long, `jitter` #Prc, `Mnm ` #Prc | `AbrDynamics` |

- **API:** `read_abr(bytes) -> Result<AbrFile { samples, brushes, skipped_shape_classes }, AbrError>`;
  `read_samples` keeps its signature (wraps `read_abr`); `match_sample(&AbrBrush, &AbrFile) -> Option<&AbrSample>`
  matches `sampled_data_id` against `AbrSample.id`.
- **Safety caps (checked before allocation):** descriptor version, item counts
  (`MAX_DESC_ITEMS`, `MAX_LIST_ITEMS`), text chars, raw-data bytes, nesting
  depth, and the desc section's declared 8BIM length. New named `AbrError`
  variants for each refusal.
- **Non-goals (named):** `patt` patterns stay skipped; v8+ stay rejected;
  unknown OSType codes are a named strict error (extend the table in a later
  kill if a real file needs it); dual brush / texture / scattering beyond
  the four dynamics fields are skipped inside the descriptor; no .abr
  writing; no BrushPreset mapping yet (that is row 283).
- **Pins:** synthetic v6 file (one samp + one desc with one sampledBrush)
  parses to exact field values and matches its stamp by id; computedBrush →
  `sampled_data_id: None`; a `dBrush` entry is skipped and counted;
  version != 16, unknown OSType, oversized TEXT, truncated desc each refuse
  by name; `phry` parses and ignores; v8 header still refused; unmatched id →
  None; two brushes in one desc → two entries.
