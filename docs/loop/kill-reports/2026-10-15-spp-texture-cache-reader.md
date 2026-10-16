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
  - `channel_name_for_flag`: the default stack is named (`baseColor`,
    `height`, `roughness`, `metallic`, `normal`), from the document's own
    `(format, flag)` records cross-checked against the pixels (research
    §7a). `emissive` (16) and `opacity` (8) are named as inferred, with
    a report note. 2⁴⁶ stays `flag_<n>`. A test pins that every emitted
    name routes through the mesh-map import into the channel of that
    name (`height` via the bake route, the rest as user channels).
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

## User docs (uncommitted, ships with the code)

`docs/user/getting-started.md` §6b ("Bring in a Substance Painter
project") and the `extract-spp` line in §7 are written but held under
lock `spp-user-docs`. They commit WITH this kill, never before:
committing them now would document a feature HEAD lacks. One corpus set
(BessaCreatureV2 `652259…`, Bessa Synth `562550…`) has an empty cache
list, so the "imports nothing for that set" line is observed, not
assumed.

## Name check (corpus, 2026-10-15)

`expected_format` pins the texel format behind each named flag. A file
that pairs a known flag with another format keeps `flag_<n>`. All 61
corpus channels pass (1/16 → SRGB8_ALPHA8, 2 → RG16F,
8/128/8192 → RG8, 2²² → RGBA16F), so no real name reverts. The model
test pins the revert with a hand-made mismatch.

## Pixel values: linear colour, signed height (corpus-measured)

- **Base colour and emissive are linearized on import.** Umber's planes
  are linear: the picker and eyedropper linearize, and export applies
  the sRGB curve to baseColor and emissive (`driver::output_transfer`).
  Kept sRGB-encoded, an imported base colour would be curved twice and
  export washed out. Alpha is never curved. The test pins the anchors
  (sRGB 128 → 55, 188 → 128, 0 and 255 exact).
- **Height is signed, mapped ±1 → 0..255 with 0.0 on 128**, Umber's
  "no displacement" neutral (the C2-6 export neutral). Measured over the
  10 corpus height stacks:

  | | Value |
  |---|---|
  | Lowest | −1.116 (Bessa Synth main stack) |
  | Highest | +0.463 (HydrageaBessa) |
  | Clipped by ±0.5 | 5.5% of two stacks |
  | Clipped by ±1 | 0.02% of one |

  Painter does not store a height scale this reader can find, so ±1 is
  a measured choice, not a recovered one. The report line says so.
- **The corpus test** checks that height clips under 1%. It does not
  check that the median sits at 128: two corpus stacks are offset
  (−0.474, +0.453), so a median check would fail on Bessa's own files.

## OPEN: vertical orientation (unverified, must be checked before trusting)

Umber's planes hold GPU row order (row 0 = V 0); its PNG reader flips
files to match (`umber_export::png::read_png_rgba8`). The PNGs this kill
writes go through that reader on import. Whether Painter's cache stores
rows top-first or bottom-first (OpenGL-style) is NOT established. If it
is bottom-first, every extracted channel imports upside down.

Tried, to settle it from the files alone: rasterize the mesh UVs from
`paint/scene.assbin` and compare with the cache's coverage alpha.
Painter's dump is NOT stock Assimp assbin. The banner and version fields
match, but there is no `compressed` field, no chunk magics, and no scene
header: the data at offset 512 opens straight into the root node
(`8, "RootNode"`, identity matrix). Parsing it is its own reverse-
engineering job, so it was stopped there. A second try, scanning for
plain float UV runs (8- and 12-byte records in [0,1]), found none longer
than 8 in Bessa Synth's 7.6 MB scene: the mesh data is packed or
compressed. That track is closed. The host check below is the way.

Side finding: `resources/*.image` (here a 2048×1024 RGBA16F environment)
use the same QDataStream + LZ4 framing as the cache, plus a `pitch` and
`dataSizeLZ4` key. The other resources are `jp2` (JPEG 2000), which this
reader does not decode. Not needed for channel import.

How to settle it, cheapest first:
1. **On the host, with Bessa's source mesh** (the FBX the project was
   made from): extract, import the folder in Umber, and look. Flipped
   UV islands are obvious on any asymmetric model. One look settles it
   for the format.
2. Or rasterize the source mesh's UVs (Umber's importer gives them)
   against the cache's base-colour alpha, both ways; the correct
   orientation overlaps near 100%.

If it is flipped, the fix is one line in `texture_cache::decode` (reverse
the row order before returning), plus a test pinning it. Until checked,
the user docs must not promise correct orientation; they don't.

## Not done, and why

1. **The one-line hand-off in `main.rs`** waits for
   `g5-workspace-persist`'s lock. The exact patch, at the Assets panel
   (`Panel::Assets` in `PanelViewer::ui`):

   ```rust
   -                spp_import::show_section(ui, self.spp_import);
   +                if let Some(dir) = spp_import::show_section(ui, self.spp_import) {
   +                    // The extracted <set>_<channel>.png folder routes
   +                    // through the mesh-map import as new layers.
   +                    self.mesh_import.ingest_paths(&[dir]);
   +                }
   ```

   Until it lands, `show_section`'s new return value is discarded. That
   compiles warning-free (`Option` is not `#[must_use]`), and the
   button still writes the folder. Until then the button
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

Static review done here (no compiler): every lz4_flex call
(`block::compress`, `block::decompress_into`, `DecompressError:
Display`) was checked against the 0.14.0 registry source;
`Option::is_none_or` needs Rust 1.82 (the workspace MSRV is 1.85);
clippy's `or_fun_call` was fixed (`map_or_else`), and so was a
`matches!(…, Err(_))` → `is_err()`. The `tree.notes` move was ordered
after `cache_index(&tree)`.
