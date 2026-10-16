# Kill report (UNRUN, host-gated): load Painter .spp channels

Date: 2026-10-15. Author: dragon. Bessa: "Build what's needed to load the
.spp files". Locks: `spp-texture-cache-reader` (the spp crate, the corpus
test, `spp_import.rs`) and `spp-extract-cli` (`umber-cli` main +
`spp_extract.rs`). Research: `docs/research/spp-hdf5-layout.md` §7a.

## What a real Painter file gives now

Measured with Python ports of the new Rust, line for line, on the three
corpus projects:

| File | Sets (Painter material names) | Channels |
|---|---|---|
| BessaCreatureV2 | Body, Eyes, Fur | 19 |
| Bessa Synth | Material + 2 layering stacks | 18 |
| HydrageaBessa | EYES, Horns, TEETHNSTUFF, Body | 24 |

All 61 channels decode, with no name collisions. Every set has a
`baseColor` (sRGB) and a `normal` (RGBA16F). Before this kill: 0.

## Written

- `crates/spp/src/texture_cache.rs` (new):
  - `parse_header`: the QDataStream map; every size checked before
    allocating; `bufferSize == w×h×bpp` and ≤ 256 MiB.
  - `decode`: `lz4_flex::block::decompress_into` into an exact-size
    buffer (never `decompress`, which can panic); converts SRGB8_ALPHA8,
    RG8, RG16F and RGBA16F to RGBA8, with clamp notes.
  - `live_channels` + `stamp_minor`: a stack's `list` names its live
    datasets. Cache v3.2 uses 20-byte entries with a u64 flag; v3.3 uses
    16-byte entries with a u32 bit index, and datasets are named by the
    index. v3.3 also keeps 20 unlisted leftovers in HydrageaBessa, some
    byte-identical and some stale (`0` ≠ `1`). Only listed datasets
    import, so there are no duplicate layers.
  - `material_names`: set names from `paint/scene.assbin` (an Assimp
    binary dump: `$mat.uid` = the cache group id, `?mat.name` before it).
    10/10 corpus sets resolve.
  - `channel_name_for_flag`: flag 1 = `baseColor`, 2²² = `normal` (the
    pixels admit one reading). Every other flag is `flag_<n>`: the
    statistics hint (2¹³ often all-zero; 2⁷ mid-grey) but do not settle
    roughness vs metallic vs height, and a wrong name is worse than an
    honest number.
- `model.rs`:
  - `texture-cache/` is decoded instead of skipped (`cache_index`
    pre-pass, `cache_channel`);
  - non-main stacks become `<set>-stack-<id>`;
  - every dataset still reports (imported / index / stale / undecodable
    with its reason).
- `spp/Cargo.toml`: `lz4_flex = "=0.14.0"` (already in the lock through
  openusd, MIT, safe-decode).
- `tests/corpus.rs`: every pinned file must open, yield ≥ 1 set, a
  `baseColor`, and full pixel buffers.
- `umber-cli extract-spp <file.spp> <out-dir>`: writes
  `<set>_<channel>.png` plus the reader's report.
- `spp_import.rs`: "Extract N channel(s) as layers" writes the same PNGs
  to `<stem>-channels/` beside the `.spp` and returns the folder. Two
  tests: the names route through the mesh-map import as user channels,
  and the folder is placed beside the file.

## Not done, and why

1. **The one-line hand-off in `main.rs`** (`if let Some(dir) =
   spp_import::show_section(..) { mesh_import.ingest_paths(&[dir]) }`)
   waits for `g5-workspace-persist`'s lock. Until then the button
   writes the folder and the status line points at it; dropping that
   folder on the window imports it.
2. **The button labels are English.** The catalogs are under
   `sec-import-channel`'s lock; keys follow with `spp-hint-honest`.
3. **Cargo.lock** gains two edges (spp → lz4_flex, cli → umber-spp) and
   no packages; it is regenerated on the host.
4. `umber-cli/Cargo.toml` is shared with `win-version-resource`: commit
   together.

## Host

`spp`, `umber-cli`, `umber-app` battery; the corpus test with the
corpus copied to the host (`docs/loop/build-host.md`); `umber-cli
extract-spp` on each file, opening a PNG to look at it. Mutation:
ignore the `list` (import every leaf), and the HydrageaBessa run must
show duplicate or stale channels.
