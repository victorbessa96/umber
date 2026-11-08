# Kill brief — row 282: the .abr `desc` descriptor wave (wave-abr follow-up)

You are a claw in the Umber workspace (`/home/bessa/Projects/umber`, branch
master). You know nothing except this brief. READ first:
`docs/loop/briefs/2026-11-08-wave-abr-desc-design.md` (the contract —
target shape table, API, caps, non-goals, pins),
`crates/umber-brush/src/abr.rs` (row 268's reader: `AbrSample`, `AbrError`,
`read_samples`, the 8BIM section walk that currently skips `desc`/`patt`/
`phry` by declared length),
and the vendored format authority (pattern-port the LOGIC, never copy code
or prose):
- `docs/claw-artifacts/ag-psd/src-tree/src/descriptor.rs` — descriptor core:
  `read_version_and_descriptor` (u32 version must be 16), `read_descriptor_structure`
  (class structure = unicode name string + ascii-or-class id; u32 item count;
  items = key + type sig + OSType value), `read_os_type` dispatch
  (`Objc`/`GlbO`, `VlLs`, `doub`, `UntF`/`UnFl`, `TEXT`, `enum`, `long`,
  `comp`, `bool`, `type`/`GlbC`, `alis`, `tdta`, `obj ` reference with
  prop/Clss/Enmr/rele/Idnt/indx/name items, `ObAr`, `Pth `), `UNITS_MAP`
  (`#Ang` Angle, `#Prc` Percent, `#Pxl` Pixels, ...), `read_unicode_string`
  (u32 length, big-endian UTF-16 code units, trailing NUL dropped),
  `read_ascii_string_or_class_id` (i32 len; len==0 means a 4-byte class id),
  `read_class_structure`.
- `docs/claw-artifacts/ag-psd/src-tree/src/abr.rs` lines 905-1060 (the 8BIM
  walk; `desc` arm: read_version_and_descriptor, `Brsh` list, each Descriptor
  item → parse_brush; `phry`: parse and ignore) and lines 378-540
  (`parse_percent` default 1.0, `parse_angle` default 0.0,
  `parse_units_to_number`, `parse_dynamics` with keys `bVTy`/`fStp`/`jitter`/`Mnm `)
  and 498-560 (`parse_brush_shape` for class ids `computedBrush` and
  `sampledBrush` — keys `Dmtr`/`Angl`/`Rndn`/`Intr`/`Spcn`/`flipX`/`flipY`/
  `Hrdn`/`Nm  `/`sampledData`) and `to_brush_type` (`_`/`MixB`/`SmTl`).

## Your file surface (touch NOTHING else)

- `crates/umber-brush/src/abr_desc.rs` (NEW)
- `crates/umber-brush/src/abr.rs`
- `crates/umber-brush/src/lib.rs` (one `pub(crate) mod abr_desc;` line)

Do NOT run cargo/clippy/tests. Do NOT commit or touch git. No new deps
(std + thiserror only, like the rest of the crate).

## What to build

1. **abr_desc.rs** — the minimal clean-room descriptor reader:
   a bounds-checked big-endian cursor over `&[u8]` (every read checks
   remaining length; short read → `AbrError::MalformedDescriptor`);
   `Descriptor { class_id: String, name: String, items: Vec<(String, DescValue)> }`;
   `DescValue` enum covering every OSType the reference dispatches
   (Descriptor, List, Double, UnitDouble{units,value}, Text, Enum(String),
   Integer(i32), LargeInteger{low,high}, Boolean, Class{class_id,name},
   Alias(String), RawData(Vec<u8>), Reference(Vec<RefItem>), ObjectArray,
   Path); `read_version_and_descriptor` (version != 16 →
   `AbrError::UnsupportedDescriptorVersion(v)`); the UNITS_MAP port (unknown
   unit code → `AbrError::InvalidDescriptorUnits`); unknown OSType sig →
   `AbrError::UnsupportedDescriptorType(sig)`.
   Caps checked BEFORE any allocation: `MAX_DESC_ITEMS`, `MAX_LIST_ITEMS`,
   `MAX_TEXT_CHARS`, `MAX_RAW_BYTES`, `MAX_DESC_DEPTH` (recursion),
   `MAX_REFERENCE_ITEMS` — a breach → `AbrError::DescriptorTooLarge`.
   Helpers: `dget`, `as_descriptor`, `as_units`, `as_number`, `as_bool`,
   `as_text`, `parse_percent` (missing → 1.0), `parse_angle` (missing →
   0.0), `parse_units_to_number(value, expected)`.
2. **abr.rs** — wire the `desc` 8BIM arm (parse via abr_desc, walk the
   `Brsh` list, parse each Descriptor item) and the `phry` arm (parse,
   ignore); `parse_brush_shape` handles `computedBrush` and `sampledBrush`,
   every other class id is SKIPPED and counted in
   `AbrFile.skipped_shape_classes`. Public API:
   ```rust
   pub enum AbrToolClass { Brush, MixerBrush, Smudge }
   pub struct AbrDynamics { pub control_index: i32, pub steps: i32, pub jitter: f64, pub minimum: f64 }
   pub struct AbrBrush {
       pub tool_class: AbrToolClass, pub name: String,
       pub diameter_px: f64, pub angle_deg: f64, pub roundness: f64,
       pub spacing_on: bool, pub spacing: f64,
       pub flip_x: bool, pub flip_y: bool, pub hardness: f64,
       pub sampled_data_id: Option<String>, pub dynamics: Option<AbrDynamics>,
   }
   pub struct AbrFile { pub samples: Vec<AbrSample>, pub brushes: Vec<AbrBrush>, pub skipped_shape_classes: u32 }
   pub fn read_abr(bytes: &[u8]) -> Result<AbrFile, AbrError>
   pub fn match_sample<'a>(brush: &AbrBrush, file: &'a AbrFile) -> Option<&'a AbrSample>
   ```
   `read_samples` keeps its exact current signature and behavior (wrap
   `read_abr`, return `.samples`) — every existing pin must stay green.
   v1/v2 and v8+ refusals unchanged. `patt` and unknown sections stay
   skipped by declared length. Update the module doc: the "descriptor wave
   is the named follow-up" lines now describe the LANDED desc parsing.
3. **Pins** (synthetic v6 fixtures built in-test with inline big-endian
   writers — look at the existing abr.rs tests for the samp fixture shape):
   - one samp (raw 8-bit 4x4, id "tip1") + one desc whose Brsh list holds one
     sampledBrush (`Nm  ` TEXT "Test Tip", `Dmtr` UntF #Pxl 32.0, `Angl` UntF
     #Ang 15.0, `Rndn` #Prc 100, `Intr` bool true, `Spcn` #Prc 25, `Hrdn`
     #Prc 80, `flipX`/`flipY` bool false, `sampledData` TEXT "tip1", dynamics
     `bVTy` long 1, `fStp` long 4, `jitter` #Prc 10, `Mnm ` #Prc 0) →
     `read_abr` returns exactly those field values and
     `match_sample` returns the tip with exact alpha bytes;
   - a computedBrush entry (no `sampledData`) → `sampled_data_id: None`;
   - a `dBrush` entry in the same list → skipped, `skipped_shape_classes == 1`;
   - descriptor version 15 → `UnsupportedDescriptorVersion(15)`;
   - unknown OSType sig `zzzz` → `UnsupportedDescriptorType("zzzz")`;
   - TEXT longer than `MAX_TEXT_CHARS` → `DescriptorTooLarge` (refused before
     the string is allocated);
   - truncated desc (8BIM declared size runs past EOF) → `MalformedDescriptor`;
   - a `phry` section parses and is ignored;
   - v8 header → the existing unsupported-version refusal (unchanged);
   - unmatched `sampledData` id → `match_sample` None;
   - two sampledBrush entries in one desc → two brushes.

## Doc comments

Name row 282 of the wave-abr design brief and the landed behavior. Non-goals
stay named: patt, v8+, unknown-OSType extension, dual brush/texture,
BrushPreset mapping (row 283), writing.

Write the code, keep it compiling by careful reading (you cannot run cargo),
and stop. Report what you changed and where.
