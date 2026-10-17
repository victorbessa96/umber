# .spp importer — completeness audit and v2 requisites (2026-10-16)

Scope: validate that the `.spp` importer (`crates/spp` + the app seam +
the CLI extractor) is complete against its contract
(`docs/research/spp-hdf5-layout.md` §6, the binding scope box) and
against what real `.spp` files actually hold; enumerate the gap map;
author the v2 requisites. Evidence classes per the REA discipline:
OBSERVED (read from artifacts), DERIVED (computed from observations),
INFERRED (hypothesis, marked), UNKNOWN (named, not examined).

Container-knowledge source for this pass: exploration results
assembled from online sources (2026-10-16), cross-checked against the
corpus bytes and the community-doc contract. Nothing here derives
from executing Painter or from any Adobe-distributed material beyond
the user-provided corpus files.

## 1. Verdict on v1

**The importer is now complete for what it claims** — after one
critical fix this audit found and landed (see
`docs/loop/kill-reports/2026-10-16-spp-chunk-index.md`).

- The HDF5 subset reader (§6) implements the contract as written:
  superblocks v0-v3, object headers v1/v2 + continuations, the
  message allow-list, strict datatype allow-list, layouts, filter
  pipelines, chunked assembly through v1 b-trees, symbol-table and
  compact groups, loud skips everywhere else. Divergences: NONE open.
  (The chunk-key semantics were wrong inside §6.4 — keys are
  element-offsets, the odometer counted indices — fixed and
  regression-pinned.)
- The corpus gate (3 real Painter projects, 1.3 GB, sha256-pinned)
  now verifies CONTENT, not just shape: material names resolve
  (Body/Eyes/Fur/Material…), every channel's nonzero floor holds,
  no truncation notes, height clipping under 1%. All green.
- The end-to-end chain (reader → texture-cache decode → extract →
  mesh-map routing) round-trips: `extract-spp` writes
  `<PainterMaterial>_<channel>.png` files whose pixels match a
  reference decoder on the same bytes (mean colour within one
  quantization step; nonzero ratio 100% where the reference says
  100%).

What v1 imports from a real file: the flattened per-stack channels of
every texture set (baseColor, roughness, metallic, normal, height,
emissive, opacity as named; unestablished flags as `flag_<n>`), with
Painter's own material names, at the file's native resolution, as
RGBA8 (linear planes). What it deliberately reports-not-imports:
HBO graphs (document.bin et al., the layer stack), resources
(`.image`/`.shader`/`.sbsar` blobs), baking configs, editor state,
`projectsettings.ini` (version stamp is read), dense-link groups.

## 2. The completeness gap map (what a real .spp holds that v1 does not import)

Gap class A — decodable today with known techniques:

1. **16-bit and float channel precision.** The cache decoder clamps
   to RGBA8 (half floats clamped to [0,1], RG16F height remapped
   ±1). Painter's channels are 8/16/16F per `internalFormat`; normal
   maps in particular are RGBA16F — the clamp softens fine normals.
   Requisite: keep full precision through the import (f16 storage in
   Umber's planes exists per-channel: sRGB8/RGB16/RGB16F/RGB32F are
   the §2 row-47 formats).
2. **Non-main stacks (material layering).** v1 names them
   `<set>-stack-<id>` and imports their channels as separate sets —
   correct-but-flat. Painter models them as shader-declared sub-stacks
   with masks (the dynamic-material layering model). Requisite: map
   them onto Umber's per-set stack model when the dynamic-materials
   lane consumes them.
3. **The `list`/`header` index semantics** are read (v3.2 vs v3.3
   layouts) but the v3.3 STALE leftover datasets (unlisted, some
   byte-different) are only reported. Requisite: an
   `--include-stale` extract flag for forensic use; default stays
   live-channels-only.

Gap class B — needs new decode work (the HBO lane):

4. **The layer stack itself.** `paint/document.bin` (12.8 MB on one
   corpus file) is the HBO v11/12 registry-family stream: layer
   tree, per-layer params, effects, masks, anchors, instancing. v1
   reports "not imported (HBO graph, era v11/12)". THE central v2
   item — a corpus-derived schema (the §8 automap-style plan) would
   recover layers as paint/fill/folder layers with masks, not just
   flattened channels. Value: round-trip parity for real projects
   (a user's file keeps its layer structure, not just its baked
   result).
5. **Baking configs** (`baking/` group, HBO): baker definitions,
   per-baker parameters, linked groups. Useful for "re-bake the way
   the file was baked".
6. **`default_material.bin`** (HBO): the default material/shader
   instance + channel declarations — the `(format, flag)` records the
   reader already partially mines for channel names.
7. **Editor state** (`editor/`): viewersettings, camera, display
   config. Low value; report-only is fine.

Gap class C — resources:

8. **`resources/*.image`** — QDataStream-serialized `Alg::ResourceImage`
   blobs (the ICC-tagged bitmaps): imported bitmaps/fill sources.
   Decodeable with the same QDataStream reader the cache already
   uses; would recover fill-layer source images.
9. **`resources.toc`** + shelf resources — the project's resource
   inventory (TOC is a small QDataStream map; the blobs are `.sbsar`
   (untouchable), `.shader` (GLSL, out of scope), fonts, SVGs).
10. **`paint/scene.assbin` as a MESH** — v1 reads it only for
    material names. The assbin IS the project's scene-of-record
    (Assimp binary): full mesh + material + UV data. Importing it
    would let an .spp drop load with its own geometry, no original
    mesh file needed. (Assimp's ASSBIN format is documented in
    assimp's source; a minimal reader for the mesh/material subset
    is a bounded kill.)

Gap class D — infrastructure:

11. **MurmurHash3 verification** (§3, U7): every dataset carries an
    `m3_x64_128` attribute (seed 0xF13A0239); v1 records presence
    only. Verifying it would make the import self-checking.
12. **Dense (fractal-heap) groups** (§6.5): not traversed; if
    Painter ever writes them the report says so per group. Named
    v1.x follow-up; unblocked by the spec.
13. **Multi-file corpus CI**: the corpus gate runs only where the
    files exist (never committed). A CI job with a redacted
    synthetic corpus derived from the real tree shapes would keep
    the HDF5 subset honest without the bytes.

## 3. v2 requisites (ordered, each a bounded kill)

R1. **Full-precision channel import** (gap 1): f16 → Umber plane
    formats; normals keep their float range; the extract CLI gains a
    `--precision native|8` flag. Test: a synthetic f16 channel
    round-trips; the corpus's RGBA16F normal matches the reference
    decoder within tolerance.
R2. **assbin mesh import** (gap 10): read the scene's mesh/material
    subset; `extract-spp --mesh` writes the mesh beside the channels
    and the app's drop flow offers "open with the project's mesh".
    Risk-bounded: assbin is append-only stable; a partial parse
    reports loudly.
R3. **Resource image decode** (gap 8): the QDataStream reader
    generalized from the cache to `Alg::ResourceImage` blobs; fill
    sources recover as bitmap layers.
R4. **HBO layer-stack schema** (gap 4) — the flagship. Automap-style
    corpus derivation: dump registry + member names across the three
    corpus files, cluster by shape, and decode the layer tree into
    Umber's layer model. Explicitly staged: (a) schema extraction
    from the corpus, (b) decoder, (c) mapping onto paint/fill/folder/
    mask/effect nodes, (d) fidelity report per file (what
    round-tripped, what flattened). Never a silent partial import —
    a project that does not fully decode falls back to the v1
    flattened-channel import with a report line.
R5. **Baking-config decode** (gap 5): baker params from `baking/`;
    maps onto Umber's baker settings; a "matching bake" one-click.
R6. **m3 verification** (gap 11) — LANDED (2026-10-16, verified
    post-audit): `hdf5::m3_verifies` (hdf5.rs:2192) computes the
    MurmurHash3 x64 128 of the decoded payload with the corpus seed
    (0xF13A_0239) and compares against the stored attribute;
    `model.rs:189` consumes it at open. None = "unverified" report
    line; Some(false) = loud mismatch report, never a failed open.
    Reference vectors tested (hdf5.rs:2315+). Row closed by tree
    re-derivation, not re-planned.
R7. **Dense-group traversal** (gap 12) — PHASE 1 LANDED: dense
    groups are detected (`LinkInfo.dense`, hdf5.rs:530/844/1938) and
    reported, never traversed. Phase 2 (v2 b-tree types 5/6 + fractal
    heap IMPORT) is corpus-gated — develops against the real files on
    the build host.

Non-requisites (deliberate): .spp WRITING (hard wall, binding);
.sbsar execution (hard wall); shader/GLSL import (out of scope,
Umber's OpenPBR + WGSL is the answer); editor-state import (report
only).

## 4. Process fixes landed with this audit

- The corpus gate now pins content, not shape (nonzero floors,
  material-name resolution, truncation-note refusal) — the gate that
  stayed green while 95% of every channel was zeros.
- The lesson, stated once: **a length-satisfying decode is not a
  complete decode.** Every importer over compressed chunked storage
  gets a content oracle or it ships blind. The mesh importer's
  panic-hunt suite and the PSD report follow the same doctrine; the
  .spp gate now does too.
