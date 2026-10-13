# .spp (HDF5) layout — clean-room research record

Date: 2026-10-13. Kill: WAVE-3 KILL 1 (binding ruling 1,
`docs/loop/decisions.md`: import-only FOREVER; writing `.spp` stays the
hard wall).

**Scope box (binding).** This doc is the kill's contract: the importer
(`crates/spp`) implements what this doc documents, nothing more. The ONLY
research sources are the two named below — no Adobe code, no decompiled
Painter binaries, no `.spp` samples (none were provided; §8 records the
corpus-oracle plan for when they are). Every numbered claim cites its
source inline as `[USPP-README]`, `[USPP-ENGINE]`, `[USPP-EXTRACTOR]`,
`[USPP-MODELS]`, `[USPP-DECODER]`, or `[HDF5-SPEC]` (full URLs in §9).
Claims resolved from the spec's byte-grid rather than its converted text
are marked `(grid-resolved)`; they are corpus-unverified until a real
`.spp` traverses them (§8).

## 1. Container identity

1.1. An `.spp` project is an HDF5 container holding groups, datasets,
dataset attributes, project settings, embedded binary streams, shaders,
baked data, and checksums `[USPP-README]`.
1.2. Many project datasets contain HBO streams — Painter's binary object
graphs for the document, layer stack, editor state, materials, and
baking configuration `[USPP-README]`.
1.3. HDF5 files open with the 8-byte signature `137 72 68 70 13 10 26
10` (`\211 H D F \r \n \032 \n`), searched at offsets 0, 512, 1024,
2048, … `[HDF5-SPEC §II.A]`. The reader accepts the signature at any of
the first 8 doublings, else `NotHdf5`.
1.4. Superblock v0/v1 roots the graph at an inline root-group *symbol
table entry*; v2/v3 roots it at a root-group *object header address*
and add a trailing checksum `[HDF5-SPEC §II.A]`. The reader supports
v0–v3 and reads `Size of Offsets` / `Size of Lengths` from the
superblock — no width is assumed.
1.5. Addresses are file-relative to Base Address and `0xFF…FF` (offset
width) is the undefined address `[HDF5-SPEC §§II.A, V]`.

## 2. Version stamps

2.1. `projectsettings.ini` is a dataset whose bytes encode the source
Painter version as UTF-16LE text fields `.versionAtCreation/major`,
`.versionAtCreation/minor`, `.versionAtLastSave/major`,
`.versionAtLastSave/minor`, each followed by a single byte value at
`field_offset + utf16_len(field) + 2 + 5`
`[USPP-EXTRACTOR: _parse_project_settings]`.
2.2. Display version prefers `versionAtLastSave`, falling back to
`versionAtCreation` per component; a legitimate minor of 0 must NOT
fall back (that mislabels e.g. v12.0 as v12.1)
`[USPP-EXTRACTOR: extract]` (their `is not None` comment).
2.3. Downgrades patch `projectsettings.ini` so the target sees its own
version, and v10-and-older builds strip v11-only records such as
`projectUUID` `[USPP-ENGINE]`. v1 imports the stamp read-only.

## 3. Documented dataset paths

These are the only dataset paths named anywhere in the community docs;
everything else is per-file tree captured generically (§4):

| Path | Documented fact |
|---|---|
| `projectsettings.ini` | Version fields (§2); patched on downgrade `[USPP-ENGINE]` |
| `paint/document.bin` | HBO stream; for a v10 target its HBO `data_version` must be `81` (lower values trigger update handlers on already-projected data) `[USPP-ENGINE]` |
| `paint/default_material.bin` | HBO stream; same v10 `data_version` 81 rule `[USPP-ENGINE]` |
| `editor/viewersettings2.ini` | Renamed to `editor/viewersettings.ini` for v10 targets `[USPP-ENGINE]` |
| `texture-cache/*` | Source-version engine cache, stripped on every rebuild; Painter recomputes it on open. The pack path skips reading it (bulk bytes, wasted I/O) `[USPP-ENGINE]`, `[USPP-EXTRACTOR: skip_texture_cache]` |
| `*.m3_x64_128` (attribute) | Per-dataset MurmurHash3 x64 128 checksum, seed `0xF13A0239`; refreshed for changed datasets on rebuild `[USPP-ENGINE]` |

## 4. Tree capture (what the extractor records per object)

4.1. Pack walks every group and dataset, recording creation properties,
dataset attributes, and (for HBO-looking datasets) the HBO header; the
normal path stores raw bytes, not decoded objects
`[USPP-ENGINE: Pipeline/1. Pack]`.
4.2. Per group: path, creation properties (`track_times`,
link/attribute creation order, attribute phase change), attributes,
dataset names, subgroup names `[USPP-EXTRACTOR: _extract_group]`.
4.3. Per dataset: path, shape, maxshape, chunks, compression,
compression options, shuffle, fletcher32, scaleoffset, fill value,
allocation/fill times, layout, datatype byte order, attributes with
dtypes and byte orders, raw bytes `[USPP-EXTRACTOR: _extract_dataset]`.
4.4. The `.uspp` handoff ZIP carries `manifest.json`, `metadata.json`,
`structure.json` (group/dataset tree), `groups.json`, `datasets.json`,
`data/*.bin` raw payloads, optional `decoded/*.json`
`[USPP-README]`, `[USPP-ENGINE]`. (`.uspp` is out of scope for v1 —
recorded here only because it proves which tree facts the community
considers load-bearing.)

## 5. HBO stream families (detection is documented; semantics are not)

5.1. Detection order per dataset (uint8 payloads): if the first 4 bytes
(LE u32) equal `0x69000B11`, the stream is newer v11+ binary with the
object count as LE u32 at bytes 4–8; else if the first 12 bytes parse
as an HBO header, it is an HBO stream with a format version and data
version `[USPP-EXTRACTOR: _extract_dataset]`, `[USPP-MODELS]`.
5.2. The 12-byte header is `magic:u32 LE, version_check:u32 LE,
data_version:u32 LE` with `magic == 0x1B7C2FDD`; `version_check 0`
means the v8.1/9/10 inline tagged family, `1` the v11/12 registry
family `[USPP-MODELS: HBOHeader]`.
5.3. Era rule: v11/12 write a registry-style binary format; v8.1/9/10
an older inline tagged format; the engine reads and writes both,
chosen per profile (`source_format` / `target_format`)
`[USPP-README]`, `[USPP-ENGINE: HBO Stream Level]`.
5.4. v11 registry skeleton: `[name_len:u32][name][type_code:u32]`
entries, an `FFFFFFFF` terminator, then definition blocks
`[name_len][name][member_count]` with members
`[len][name][type_code]`; sanity caps (names ≤ 256 bytes, members ≤
1000) mark best-effort parses `[USPP-DECODER: _parse_v11_registry]`.
5.5. v10 tagged skeleton: entries `[len:u32][name][type_code:u32]`
with heuristic name recovery (length 3–64, ASCII alnum, capital
first) `[USPP-DECODER: _decode_v10_data]` — heuristic, hence §7-U2.
5.6. Documented primitive type codes: 0 null, 1 uint, 2 float, 3 bool,
4 int, 5–8 float2/3/4 + int2, 16 string, 18 object, 19 array; sizes
1→4, 2→4, 3→1, 4→4, 5→8, 6→12, 7→16, 8→8, 9→1, 10→4, 11→4, 12→8,
13→36, 14→64, 15→4, 21→8 `[USPP-MODELS]`.
5.7. Downgrade-time HBO rules (read-only context for v1's report
strings): object IDs are never rewritten (cross-stream references);
source null/empty nameless objects become target nulls; each object is
projected to the target schema (unknown members dropped, order
normalized, missing required members synthesized from target
defaults); profiles rename types/fields, wrap/unwrap containers,
translate values (e.g. enum→bitmask), and retype primitives per-field
or globally; baking IDs/tweak names/order/types project to the target
baking schema `[USPP-ENGINE: HBO Stream Level]`, `[USPP-ENGINE:
Migration Profiles]`.
5.8. Only genuinely lossy categories (blacklist drops, field retypes)
are loss-reported; renames, wraps, and enum→bitmask translations are
faithful and stay silent `[USPP-ENGINE: 2. Plan]`. v1 mirrors this:
HBO datasets report as `not imported (HBO graph, era X)`, never as
errors.

## 6. HDF5 subset the v1 reader implements

Traversal and decode cover exactly these spec sections; anything else
in a file is a loud skip or error, never a panic and never silent
(strict-validation doctrine: every structural read is bounds-checked,
every message walk is total-size-checked, and a mismatch is
`Unsupported`/`Corrupt`, not best-effort):

6.1. Superblock v0–v3 `[HDF5-SPEC §II.A]`; root via inline symbol
table entry (v0/v1) or object header address (v2/v3).
6.2. Object headers v1 (16-byte prefix `(grid-resolved)`: version u8,
reserved u8, message count u16, reference count u32, header size u32,
reserved u32; messages type u16 / size u16 / flags u8 / reserved 3 B,
data 8-padded) and v2 (`OHDR`, packed, chunk-size code in flags
bits 0–1; messages type u8 / size u16 / flags u8 [+ creation order
u16]) `[HDF5-SPEC §IV.A.1]`, plus `OCHK` continuation blocks
`[HDF5-SPEC §IV.A.2.q]`.
6.3. Messages: Dataspace v1/v2 (scalar rank-0, simple rank 1–3;
dimension sizes are Size-of-Lengths bytes; v1 data starts at offset 8
`(grid-resolved)`; max-dims must equal dims when present, else
unsupported) `[HDF5-SPEC §IV.A.2.b]`; Datatype fixed-point and
floating-point only, little-endian only, under a strict allow-list
(u8/u16/i32-LE with offset 0 and full precision; IEEE-754 f32/f64-LE
with exact property bytes; fixed strings) `[HDF5-SPEC §IV.A.2.d]`;
Layout v3 compact/contiguous/chunked (contiguous address O + size L;
chunked: 1-byte dimensionality `(grid-resolved)`, v1 B-tree address,
u32 chunk dims `(grid-resolved)`, u32 element size, cross-checked
against the datatype) `[HDF5-SPEC §IV.A.2.i]`; Filter pipeline v1/v2
with deflate(1)/shuffle(2)/fletcher32(3) only (deflate client data is
an opaque u32 array per the spec — the level value is ignored;
fletcher32 presence is recorded, not verified) `[HDF5-SPEC
§IV.A.2.l]`; Link v1 (hard/soft; external/user-defined links are
reported, not followed) and inline compact-group links
`[HDF5-SPEC §IV.A.2.g]`; Symbol Table message + `SNOD` nodes + local
`HEAP` names (old-style groups) `[HDF5-SPEC §§IV.A.2.r, III.A.1,
III.C, III.D]`; Link Info (compact groups enumerated, dense groups
§6.5) `[HDF5-SPEC §IV.A.2.c]`; Attribute messages are counted, not
parsed (their values, including `m3_x64_128`, ride the report as
presence-only — §7-U7).
6.4. Data: compact inline, contiguous slices, chunked assembly through
v1 B-tree chunk indices (key: byte-size u32, filter-mask u32,
`(D+1)` u64 offsets `[HDF5-SPEC §III.A.1]`), honoring per-chunk
filter-mask skip bits; unshuffle by element size; raw inflate.
6.5. Dense (fractal-heap) groups are NOT traversed: heap-managed
objects carry no inline envelope and are located externally via heap
IDs/doubling-table math `[HDF5-SPEC §III.G]` (confirmed against the
spec text — there is no per-object prefix to scan for), so a blind
traversal would enumerate fiction. They report as `dense-link group
(not traversed)`. Fractal-heap traversal is the named v1.x
follow-up, unblocked (spec §III.G + v2 B-tree types 5/6).
6.6. Out of subset (loud skip/error, never crash): virtual/external
storage, committed/named datatypes, shared messages, compound/enum/
variable-length/array datatypes, big-endian data, object references,
superblock extensions, group info / B-tree-K / driver-info messages.

## 7. Known-unknowns (v1 boundaries)

- U1. Painted-channel raster dataset paths/shapes: NOT published. The
  community tools capture the tree per file (`structure.json`) and
  only name the paths in §3. v1 imports every non-HBO numeric raster
  dataset outside `texture-cache/` as a candidate channel image keyed
  by dataset path (heuristic H1: rank-2 → gray; rank-3 with last dim
  1–4 → H×W×C, else first dim 1–4 → C×H×W, else skipped+reported;
  u16 gray scales `>>8`; f32 gray clamps; channel name = dataset leaf
  name).
- U2. HBO object semantics (layer stack, materials, bake config): the
  decoder recovers registry/type names only, heuristically for v10.
  v1 parses NO HBO payload — every HBO dataset reports `not imported
  (HBO graph, era X)`. Flat-layer import from HBO graphs waits on a
  corpus-derived schema (automap-style, §8).
- U3. Texture-set grouping: no published grouping rule. Heuristic H2:
  one set per top-level group transitively containing imported
  rasters, named by that group; root-level rasters group under the
  file stem. Reported per set so a real file exposes the true rule.
- U4. Mesh payloads: skipped and reported (`geometry stays FBX/USD`
  per the ruling); no mesh dataset paths are consumed.
- U5. Shaders, baked data, substance-graph blobs: skipped+reported.
- U6. Dense-link prevalence in real files: unknown until the first
  corpus file (§8). If Painter's writer uses dense storage for the
  root group, v1 imports the version stamp plus whatever compact
  groups exist, and says so per group.
- U7. Attribute values (incl. `m3_x64_128` verification): presence-only
  in v1; MurmurHash3 verification is a follow-up (needs no new
  research — seed `0xF13A0239` is documented in §3).
- U8. 16-bit channel precision: the feature inventory notes lossless
  compression on 16-bit images; v1 imports u16 rasters downscaled to
  8-bit (`>>8`) and reports the narrowing per channel. Full-depth
  import waits on per-channel storage formats (§2 row 47).

## 8. Corpus plan (no samples provided)

No `.spp` samples were available to this kill by any permitted means.
When Bessa provides real `.spp` files they become corpus oracles per
the absorbed corpus pattern (queue row 16): never committed,
sha256-pinned, with per-file oracle notes (tree shape, dense vs
symbol-table groups, raster paths resolving U1/U3, HBO eras present).
The first corpus file decides: fractal-heap traversal priority (U6),
the true channel-raster paths (U1), and the HBO schema-design kill.

## 9. Sources (the only permitted research surface)

- `[USPP-README]` https://github.com/Yeusepe/Universal-Painter-Files
  (README.md, main branch; fetched 2026-10-13).
- `[USPP-ENGINE]`
  https://github.com/Yeusepe/Universal-Painter-Files/blob/main/spp_downgrader/README.md
  (fetched 2026-10-13).
- `[USPP-EXTRACTOR]`
  `spp_downgrader/spp_extractor/spp_extractor.py` (raw; fetched
  2026-10-13).
- `[USPP-MODELS]` `spp_downgrader/spp_extractor/spp_ext_models.py`
  (raw; fetched 2026-10-13).
- `[USPP-DECODER]` `spp_downgrader/spp_extractor/spp_ext_decoder.py`
  (raw; fetched 2026-10-13).
- `[HDF5-SPEC]` https://support.hdfgroup.org/documentation/hdf5/latest/_f_m_t3.html
  (HDF5 File Format Specification Version 3.0; fetched 2026-10-13;
  §§I.A, II.A–C, III.A.1, III.C–E, III.G–H, IV.A–B, VII as cited).
