# Kill report — 2026-11-02 hostile-input sweep (queue row 156)

Lock: `hostile-input-sweep`. Envelope: code-only, NO cargo — rustfmt +
check-layers + grep verification only; the 2 pinned tests are UNRUN until
the next wave battery.

## Built

Decompression-bomb / allocation-abort preflights on every untrusted read
path (the `catch_unwind` boundary in `umber_mesh::load` cannot catch an
allocator abort, so size gates must run BEFORE any read):

- `umber-mesh/src/lib.rs`: `MAX_IMPORT_BYTES = 1 GiB`,
  `ensure_import_size()` (metadata check), `read_capped_with()` /
  `read_capped()` (metadata check + `take(cap+1)` so a file growing
  between the two is refused unread), new `ImportError::TooLarge`.
  `load()` runs `ensure_import_size` before dispatch — covers the
  crate-based importers (gltf, ufbx, openusd) that read the file
  themselves; `load_obj`/`load_gltf`/`load_fbx` also preflight directly
  (they are pub entry points that bypass `load`).
- `usd.rs::load_usda`, `usdc.rs::load_usdc`: uncapped `fs::read` →
  `crate::read_capped`.
- `usdc.rs::load_usdz`: `ensure_import_size` before `Archive::open`
  (openusd reads the whole zip before decompressing entries).
- `umber-core/src/project.rs` (the `.umber` reader): `read_project_file`
  caps `project.json` at `MAX_PROJECT_JSON_BYTES` = 8 MiB (bounds every
  vec capacity derived from the parsed json) and each layer file at
  `MAX_LAYER_FILE_BYTES` = 64 MiB; new `ProjectError::OversizedFile`;
  a missing layer file still maps to `MissingLayerFile`.

## Sweep findings (audit of the remaining surfaces)

- Panic boundary: EXISTS (`load`'s catch_unwind → `ImportError::Panicked`);
  2026-10-14 hunt pinned the dep panics (gltf 1.4.1, tobj 4.0.5,
  openusd 0.7.0). Recursive usda parser carries depth caps (usd.rs).
- `.umbersmart` preset reader: ALREADY capped (`smart.rs`
  `MAX_PRESET_BYTES` 1 MiB + layer/mask-op count caps).
- umber-mesh own parse code validates before indexing (per lib.rs docs).
- NAMED BOUNDARY (not fixed, would need a fork): openusd's per-ENTRY
  decompression inside `.usdz` is uncapped — a 1 GiB zip of a 100 GiB
  stored entry still bombs. File-level cap bounds the zip itself; entry
  caps need an openusd patch or a hand-rolled zip reader. Deferred.
- gltf buffer views: `gltf::import` reads buffers from the capped file
  (data: URIs embedded, so bounded by the same cap). OK.

## Judgment calls

- 1 GiB import cap: generous for paintable meshes; a bigger legit file
  fails with a typed error naming the cap, not an abort.
- project.json 8 MiB / layer 64 MiB: layer json carries ops, not pixels.
- Missing-layer mapping preserved: io error → MissingLayerFile;
  oversized → OversizedFile (distinct, names the cap).

## Unverified (no cargo per envelope)

- 2 pinned tests UNRUN: `read_capped_refuses_over_cap_without_reading_whole_file`
  (umber-mesh), `project_files_over_cap_are_refused` (umber-core).
- Compile not type-checked: rustfmt --edition 2021 clean x6,
  check-layers GREEN (16 crates), no new deps, grep-verified call sites.
- Battery (cargo test -p umber-mesh -p umber-core + clippy + full wave
  battery) rides the next sanctioned wave push.

## Deferred

- openusd per-entry decompression caps (needs fork) — named boundary above.
- Hostile `.umbersmart` mask payloads beyond count caps: covered by
  MAX_PRESET_BYTES + MAX_MASK_OPS; deeper op validation = separate kill.
