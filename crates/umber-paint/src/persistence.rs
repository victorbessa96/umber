//! Paint persistence: the `.umber` project's `paint/` sidecar — the
//! byte-exact PNG planes (Base per channel, per-layer planes under
//! `paint/layers/<id>/`, layer masks), the set/channel size mapping, the
//! crash-safe two-phase commit (stage → manifest → apply → sweep), and
//! the dual-read loader (v1 flat + v2 per-layer, the dormancy header).
//!
//! Moved out of `umber-app`'s `main.rs` (2026-10-14) with the engine, so
//! every front end saves and opens paint through ONE implementation: the
//! shell's File > Save / Open, and the `umber-cli agent` session's
//! `project_save` (agent paint ops). The functions are free functions over
//! a [`PaintState`](crate::paint_state::PaintState) and a project dir —
//! no GUI state, no dialogs (those stay in the shell).
//!
//! The format contract (paths, codecs, the commit protocol, dormancy) is
//! unchanged by the move: the test module below is the one that pinned it
//! in `main.rs`, moved verbatim (one edit: the size-preset round trip
//! names its five sizes literally instead of reading the app's UI list).

use std::path::{Path, PathBuf};

/// The paint sidecar directory inside a `.umber` project: one
/// byte-exact `tile-<N>.png` per persisted paint tile (see the module
/// docs for why a sidecar, not `project.json`).
pub fn paint_sidecar_dir(project_dir: &Path) -> PathBuf {
    project_dir.join("paint")
}

/// The sidecar PNG for one baseColor paint tile — the legacy FLAT
/// `paint/tile-<N>.png` shape, byte-identical to the v2 layout and
/// untouched by the per-channel tile pool
/// (docs/specs/per-channel-tile-pool-design.md): every v2 project and
/// every one of today's single-channel sessions keeps this exact path.
/// [`paint_channel_tile_path`] is the channel-aware sibling for every
/// OTHER channel, which nests under its own `paint/<channel>/` directory
/// (baseColor deliberately does NOT move there — a smaller, zero-churn
/// diff than the design doc's sketch; see the kill report).
pub fn paint_tile_path(project_dir: &Path, tile: u16) -> PathBuf {
    paint_sidecar_dir(project_dir).join(format!("tile-{tile}.png"))
}

/// [`paint_tile_path`]'s channel-aware sibling: `paint/tile-<N>.png` for
/// the reserved baseColor channel (identical to `paint_tile_path`),
/// `paint/<channel>/tile-<N>.png` for every other channel.
pub fn paint_channel_tile_path(project_dir: &Path, channel: &str, tile: u16) -> PathBuf {
    if channel == crate::paint_state::BASE_COLOR_CHANNEL {
        paint_tile_path(project_dir, tile)
    } else {
        paint_sidecar_dir(project_dir)
            .join(channel)
            .join(format!("tile-{tile}.png"))
    }
}

/// The manifest `name`/sidecar-listing form of `(channel, tile)`: the
/// legacy flat `tile-<N>.png` for baseColor (byte-identical to the v2
/// commit shape — see [`paint_tile_path`]'s docs), else
/// `<channel>/tile-<N>.png`. NOT a 6th manifest column: `parse_paint_commit`
/// destructures an exact 5-field array (`main.rs`'s `PaintCommitEntry`
/// codec), so a literal new column would refuse every v2 manifest AND
/// every new manifest a v2 binary tries to roll forward. Encoding the
/// channel in the `name` field itself keeps the codec's arity exactly 5
/// and is what [`parse_paint_sidecar_name`] parses back.
pub fn paint_sidecar_name(channel: &str, tile: u16) -> String {
    if channel == crate::paint_state::BASE_COLOR_CHANNEL {
        format!("tile-{tile}.png")
    } else {
        format!("{channel}/tile-{tile}.png")
    }
}

/// Parses a manifest `name`/sidecar listing back to its `(owner,
/// channel, tile)` (per-layer-pool design §6.1's parse rule — the trust
/// boundary): a bare `tile-<N>.png` is the reserved baseColor channel's
/// Base plane; `<channel>/tile-<N>.png` is that channel's Base plane;
/// EXACTLY `layers/<u64>/<channel>/tile-<N>.png` is a layer-owned plane.
/// `None` for anything else — including `..`, a backslash, an empty
/// channel, a non-numeric id, and 3 components (`layers/7/tile-<N>.png`
/// rejects). Depth disambiguates a texture-set channel literally named
/// `layers`: `layers/tile-<N>.png` is that channel's Base plane. `None`
/// also for a channel component that is empty, `..`, or holds a backslash
/// (texture-set channel names are arbitrary user/import-derived strings,
/// and the manifest line is space-delimited — this is the trust boundary,
/// not `is_paint_sidecar_name`'s whitelist alone).
pub fn parse_paint_sidecar_name(
    name: &str,
) -> Option<(crate::paint_state::PlaneOwner, String, u16)> {
    let base = crate::paint_state::PlaneOwner::Base;
    let base_color = crate::paint_state::BASE_COLOR_CHANNEL.to_string();
    let parts: Vec<&str> = name.split('/').collect();
    match parts.as_slice() {
        [tile] => {
            let tile = parse_paint_tile_name(tile)?;
            Some((base, base_color, tile))
        }
        [channel, tile] => {
            if channel.is_empty() || *channel == ".." || channel.contains('\\') {
                return None;
            }
            let tile = parse_paint_tile_name(tile)?;
            Some((base, channel.to_string(), tile))
        }
        ["layers", id, channel, tile] => {
            if channel.is_empty() || *channel == ".." || channel.contains('\\') {
                return None;
            }
            let id: u64 = id.parse().ok()?;
            let tile = parse_paint_tile_name(tile)?;
            Some((
                crate::paint_state::PlaneOwner::Layer(id),
                channel.to_string(),
                tile,
            ))
        }
        _ => None,
    }
}

/// The manifest `name`/sidecar-listing form of one plane: Base keeps the
/// legacy flat/channel-qualified shape byte-for-byte
/// ([`paint_sidecar_name`]); a layer-owned plane nests under
/// `layers/<id>/<channel>/` (design §6.1). NOT a 6th manifest column (see
/// [`paint_sidecar_name`'s docs] — `parse_paint_commit`'s exact 5-field
/// destructure is unchanged).
pub fn paint_plane_sidecar_name(
    owner: crate::paint_state::PlaneOwner,
    channel: &str,
    tile: u16,
) -> String {
    match owner {
        crate::paint_state::PlaneOwner::Base => paint_sidecar_name(channel, tile),
        crate::paint_state::PlaneOwner::Layer(id) => {
            format!("layers/{id}/{channel}/tile-{tile}.png")
        }
    }
}

/// The sidecar file holding one plane's PNG: Base keeps the legacy paths
/// byte-for-byte ([`paint_channel_tile_path`]); a layer-owned plane lives
/// under `paint/layers/<id>/<channel>/`. Never under `layers/<set>/` (the
/// layer-JSON tree — `prune_stale_layer_files` deletes every entry there
/// that is not a live `<id>.json`, directories included).
pub fn paint_plane_path(
    project_dir: &Path,
    owner: crate::paint_state::PlaneOwner,
    channel: &str,
    tile: u16,
) -> PathBuf {
    match owner {
        crate::paint_state::PlaneOwner::Base => paint_channel_tile_path(project_dir, channel, tile),
        crate::paint_state::PlaneOwner::Layer(_) => {
            paint_sidecar_dir(project_dir).join(paint_plane_sidecar_name(owner, channel, tile))
        }
    }
}

/// One-line tag for a plane owner in save/load logs (format strings can't
/// hold field access, so the tag is built here, not at the log site).
pub fn plane_owner_tag(owner: crate::paint_state::PlaneOwner) -> String {
    match owner {
        crate::paint_state::PlaneOwner::Base => "base".to_string(),
        crate::paint_state::PlaneOwner::Layer(id) => format!("layer {id}"),
    }
}

/// The sidecar file holding the session's paint-target size
/// (`<project>/paint/target_size.txt`, body `"{width}x{height}"`): written
/// on every save — even with zero tiles, so a 4K choice with no paint yet
/// survives the round trip — and read on open only when the sidecar holds
/// no tiles (with tiles, the PNG dims lead: they must fit the pixels they
/// carry). A sidecar file, not `project.json`: the project schema is owned
/// by `umber-core` (another kill's surface) and this crate has no serde
/// dependency. Judgment call: any positive dims round-trip (the combo only
/// offers square presets, but the load path honors what it stored —
/// clamping a hand-grown size back down would drop pixels silently).
pub fn paint_target_size_path(project_dir: &Path) -> PathBuf {
    paint_sidecar_dir(project_dir).join("target_size.txt")
}

/// `"{width}x{height}"` (the `target_size.txt` body).
pub fn format_paint_target_size(width: u32, height: u32) -> String {
    format!("{width}x{height}")
}

/// Parses [`format_paint_target_size`]'s output; `None` for anything else
/// (a hand-edited file degrades to the session's live size — never a panic
/// and never a zero-size target).
pub fn parse_paint_target_size(text: &str) -> Option<(u32, u32)> {
    let (w, h) = text.trim().split_once('x')?;
    let (width, height): (u32, u32) = (w.trim().parse().ok()?, h.trim().parse().ok()?);
    (width > 0 && height > 0).then_some((width, height))
}

/// Parses a `tile-<N>.png` sidecar file name back to its tile number;
/// `None` for anything else (the temp files the atomic writers leave on
/// a crash, a foreign file dropped into the folder, …).
pub fn parse_paint_tile_name(name: &str) -> Option<u16> {
    let rest = name.strip_prefix("tile-")?.strip_suffix(".png")?;
    rest.parse::<u16>().ok()
}

/// The sidecar PNG for one layer-mask plane: `paint/mask-<layer>-tile-<N>.png`
/// (flat, not nested: mask planes are single-channel `u8` stored as RGBA8
/// PNGs through the same writer the paint tiles use, and the flat `mask-`
/// prefix keeps [`parse_paint_tile_name`] — which requires the `tile-`
/// prefix — from ever colliding, while [`collect_paint_tiles`] skips them
/// for the same reason: masks restore through their own pass, never as
/// paint).
pub fn paint_mask_name(layer: u64, tile: u16) -> String {
    format!("mask-{layer}-tile-{tile}.png")
}

/// Parses [`paint_mask_name`]'s output back to `(layer, tile)`; `None`
/// for anything else (paint tiles, size files, temps, foreign files).
pub fn parse_paint_mask_name(name: &str) -> Option<(u64, u16)> {
    let rest = name.strip_prefix("mask-")?.strip_suffix(".png")?;
    let (layer, tile) = rest.split_once("-tile-")?;
    Some((layer.parse::<u64>().ok()?, tile.parse::<u16>().ok()?))
}

/// Expands one grayscale mask plane to RGBA8 (gray in RGB, opaque alpha)
/// for the Linear-transfer PNG writer the paint tiles use — the mask
/// sidecar stays decodable by the same reader.
pub fn mask_to_rgba(values: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 4);
    for &v in values {
        out.extend_from_slice(&[v, v, v, 255]);
    }
    out
}

/// Takes the red channel back out of a decoded mask PNG (the inverse of
/// [`mask_to_rgba`] — writer and reader agree the plane rides in R; G and
/// B are the same gray by construction, alpha is the constant 255).
pub fn rgba_to_mask(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4).map(|px| px[0]).collect()
}

/// The sidecar file holding the PER-SET paint-target sizes
/// (`<project>/paint/target_sizes.txt`): one `<set>=<width>x<height>`
/// line per sized set plus one `tile-<N>=<set>` assignment line per saved
/// tile (see the module docs). Written on every save alongside the legacy
/// global below.
pub fn paint_set_sizes_path(project_dir: &Path) -> PathBuf {
    paint_sidecar_dir(project_dir).join("target_sizes.txt")
}

/// The parsed `target_sizes.txt`: set sizes, per-channel overrides, plus
/// tile assignments (all may be empty — an empty mapping means "no per-set
/// information", and the load path falls back to the legacy global).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PaintSetSizes {
    /// Set name → `(width, height)`, sorted (BTreeMap wire order keeps
    /// saves byte-deterministic).
    pub sizes: std::collections::BTreeMap<String, (u32, u32)>,
    /// `(set, channel)` → `(width, height)` overrides, sorted. Absent =
    /// inherit the set's size (see `paint_state`'s channel-sizing docs).
    pub channels: std::collections::BTreeMap<(String, String), (u32, u32)>,
    /// Tile id → set name, sorted.
    pub tiles: std::collections::BTreeMap<u16, String>,
}

/// Formats [`PaintSetSizes`] for `target_sizes.txt`: size lines first
/// (sorted by set name), then channel overrides (sorted by set, then
/// channel), then tile assignments (sorted by tile). A `#` header names
/// the version so a future format can branch on it. The header stays the
/// `v1` line on purpose: channel lines are ADDITIVE (no file without
/// them changes by a byte), and the parser never branched on the header
/// (it skips `#` lines), so old readers keep their exact behavior.
pub fn format_paint_set_sizes(mapping: &PaintSetSizes) -> String {
    let mut out = String::from("# Umber paint-set sizes v1\n");
    for (name, (width, height)) in &mapping.sizes {
        out.push_str(&format!("{name}={width}x{height}\n"));
    }
    for ((set, channel), (width, height)) in &mapping.channels {
        out.push_str(&format!("channel/{set}/{channel}={width}x{height}\n"));
    }
    for (tile, set) in &mapping.tiles {
        out.push_str(&format!("tile-{tile}={set}\n"));
    }
    out
}

/// Parses [`format_paint_set_sizes`]'s output; `None` (no mapping — the
/// load path falls back to the legacy global) when NOTHING parsed, even
/// if the file exists but holds only comments/garbage. Malformed lines
/// are skipped (never a panic, never a zero-size target). The `tile-<N>`
/// prefix is RESERVED for assignments: a set literally named `tile-1002`
/// cannot be sized through this file (its line parses as an assignment) —
/// set names are mesh/material-derived, so this never triggers in
/// practice, and the load path never strands a tile over it (an
/// assignment to an unknown name registers that name at the default size).
/// The `channel/` prefix is likewise RESERVED for per-channel overrides:
/// a line `channel/<set>/<channel>=<WxH>` records that channel's size in
/// that set (absent = inherit the set's size, so every pre-channel file
/// loads all-inherit). A set or channel name holding `/` cannot round-trip
/// (the first `/` splits set from channel — the same reserved-separator
/// class as the `tile-` prefix above); the sidecar PNG layout already
/// nests channels as `<channel>/tile-<N>.png`, so `/` in a channel name
/// never reaches this file in practice.
pub fn parse_paint_set_sizes(text: &str) -> Option<PaintSetSizes> {
    let mut mapping = PaintSetSizes::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((left, right)) = line.split_once('=') else {
            continue;
        };
        let (left, right) = (left.trim(), right.trim());
        if let Some(tile) = left
            .strip_prefix("tile-")
            .and_then(|n| n.parse::<u16>().ok())
        {
            if !right.is_empty() {
                mapping.tiles.insert(tile, right.to_string());
            }
            continue;
        }
        if let Some(rest) = left.strip_prefix("channel/") {
            if let Some((set, channel)) = rest.split_once('/') {
                if !set.is_empty() && !channel.is_empty() {
                    if let Some((width, height)) = parse_paint_target_size(right) {
                        mapping
                            .channels
                            .insert((set.to_string(), channel.to_string()), (width, height));
                    }
                }
            }
            continue;
        }
        if left.is_empty() || right.is_empty() {
            continue;
        }
        if let Some((width, height)) = parse_paint_target_size(right) {
            mapping.sizes.insert(left.to_string(), (width, height));
        }
    }
    (!mapping.sizes.is_empty() || !mapping.channels.is_empty() || !mapping.tiles.is_empty())
        .then_some(mapping)
}

/// One tile the open path refused to restore: its PNG's dims disagree
/// with its set's size (see `load_paint_tiles_reported`). Loud by
/// construction — the report is logged and returned, never swallowed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaintTileSkip {
    /// The refused tile.
    pub tile: u16,
    /// The refused channel (`baseColor` for the flat tiles — one tile can
    /// skip its channels independently, so the channel rides the record).
    pub channel: String,
    /// The set's size the tile was expected at (the CHANNEL's size when
    /// overridden — see `PaintState::tile_channel_size`).
    pub expected: (u32, u32),
    /// The PNG's actual dims.
    pub found: (u32, u32),
}

/// What `load_paint_tiles_reported` restored and what it refused.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PaintLoadReport {
    /// Tiles uploaded byte-identically.
    pub restored: usize,
    /// Layer-mask planes uploaded byte-identically.
    pub masks_restored: usize,
    /// Tiles skipped for size mismatch (loud — logged per tile).
    pub skipped: Vec<PaintTileSkip>,
    /// Whether any loaded Base plane holds a non-zero byte (§6.3): the
    /// dormancy decision's "Base is non-empty" arm. Scanned once at open
    /// (a fully-transparent Base baked nothing — its fills must evaluate,
    /// not doze).
    pub base_nonempty: bool,
}

/// Vertical row flip over tightly-packed RGBA8 bytes: output row 0 is
/// input row `height - 1`. Compensates `read_png_rgba8`'s equirect GPU
/// row-order flip on the load path (and is its own inverse, which the
/// round-trip test pins). A length mismatch returns the input unchanged
/// rather than panicking — the callers' bytes always match; this guards
/// only against a future caller passing a sliced region.
pub fn flip_rows_vertical(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let stride = width as usize * 4;
    let rows = height as usize;
    if rgba.len() != stride * rows {
        return rgba.to_vec();
    }
    let mut out = vec![0u8; rgba.len()];
    for row in 0..rows {
        let src = row * stride;
        let dst = (rows - 1 - row) * stride;
        out[dst..dst + stride].copy_from_slice(&rgba[src..src + stride]);
    }
    out
}

/// The pending paint commit (`<project>/paint/manifest.pending`): written
/// (tmp+rename) only once every artifact of a save is staged. Its rename
/// is the save's ONE commit point (see the module docs' commit protocol).
pub fn paint_commit_pending_path(project_dir: &Path) -> PathBuf {
    paint_sidecar_dir(project_dir).join("manifest.pending")
}

/// The applied commit, kept as the provenance record
/// (`<project>/paint/manifest.txt`: each sidecar name → its content hash
/// in the project's `AssetStore`). The load path never reads it — the
/// sidecar files stay authoritative, so v1/v2 readers are unaffected.
pub fn paint_manifest_path(project_dir: &Path) -> PathBuf {
    paint_sidecar_dir(project_dir).join("manifest.txt")
}

/// Whether `name` is a file a paint commit may write under `paint/`: a
/// flat or channel-qualified `tile-<N>.png`, a layer plane
/// `layers/<id>/<channel>/tile-<N>.png` (see
/// [`parse_paint_sidecar_name`]), a layer-mask `mask-<layer>-tile-<N>.png`
/// (see [`parse_paint_mask_name`]), or one of the two size files. Recovery
/// refuses a manifest naming anything else (a hand-edited `../x` never
/// escapes the folder).
pub fn is_paint_sidecar_name(name: &str) -> bool {
    parse_paint_sidecar_name(name).is_some()
        || parse_paint_mask_name(name).is_some()
        || name == "target_size.txt"
        || name == "target_sizes.txt"
}

/// The staged temp name for `name`: `<name>.tmp.<pid>` (the shape
/// `parse_paint_tile_name` already ignores).
pub fn paint_temp_name(name: &str) -> String {
    format!("{name}.tmp.{}", std::process::id())
}

/// Whether `name` is a staged temp (`<anything>.tmp.<digits>`) — what an
/// uncommitted save leaves behind, swept on the next open/save.
pub fn is_paint_temp_name(name: &str) -> bool {
    let Some((_, pid)) = name.rsplit_once(".tmp.") else {
        return false;
    };
    !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit())
}

/// One staged artifact of a paint commit (one manifest line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaintCommitEntry {
    /// Final sidecar name under `paint/` (`tile-1001.png`, …).
    pub name: String,
    /// Staged temp name under `paint/` (`<name>.tmp.<pid>`).
    pub temp: String,
    /// The content-hash blob in the project's `AssetStore` — dedupe plus
    /// provenance, and the roll-forward source once the temp is gone.
    pub asset: umber_core::AssetRef,
}

/// Formats a commit manifest: the `# Umber paint commit v2` provenance
/// header (§6.1 [B2, M11] — every save from stage 4 on, regardless of
/// `project.json`'s version), one `# dormant <id>` line per still-dormant
/// layer (§6.3 — so dormancy survives a save/reopen the user never
/// touched), then one `<name> <hash> <ext> <size> <temp>` line per entry
/// (names never hold spaces — `is_paint_sidecar_name` admits none).
pub fn format_paint_commit(entries: &[PaintCommitEntry], dormant: &[u64]) -> String {
    let mut out = String::from("# Umber paint commit v2\n");
    let mut dormant = dormant.to_vec();
    dormant.sort_unstable();
    dormant.dedup();
    for id in dormant {
        out.push_str(&format!("# dormant {id}\n"));
    }
    for entry in entries {
        let (name, temp, asset) = (&entry.name, &entry.temp, &entry.asset);
        let (hash, ext, size) = (&asset.hash, &asset.ext, asset.size);
        out.push_str(&format!("{name} {hash} {ext} {size} {temp}\n"));
    }
    out
}

/// Parses [`format_paint_commit`]'s output. `None` when ANY line is
/// malformed: the manifest is renamed into place whole, so a bad line
/// means tampering, and applying half of it would be the torn state the
/// protocol exists to prevent.
pub fn parse_paint_commit(text: &str) -> Option<Vec<PaintCommitEntry>> {
    let mut entries = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split(' ').collect();
        let [name, hash, ext, size, temp] = fields.as_slice() else {
            return None;
        };
        let asset = umber_core::AssetRef {
            hash: hash.to_string(),
            ext: ext.to_string(),
            size: size.parse().ok()?,
        };
        let temp_ok = temp
            .strip_prefix(*name)
            .is_some_and(|rest| rest.starts_with(".tmp.") && is_paint_temp_name(temp));
        if !is_paint_sidecar_name(name) || !temp_ok || asset.validate().is_err() {
            return None;
        }
        entries.push(PaintCommitEntry {
            name: name.to_string(),
            temp: temp.to_string(),
            asset,
        });
    }
    Some(entries)
}

/// The on-disk blob path of a stored asset. Mirrors `AssetStore`'s
/// private shard layout (`assets/<hash[..2]>/<hash>.<ext>`), needed only
/// to fsync the blob before the commit; the
/// `asset_blob_path_matches_the_store_layout` test pins it against a real
/// store, so a layout change fails a test instead of durability.
pub fn paint_asset_blob_path(project_dir: &Path, asset: &umber_core::AssetRef) -> PathBuf {
    let (hash, ext) = (&asset.hash, &asset.ext);
    project_dir
        .join("assets")
        .join(&hash[..2])
        .join(format!("{hash}.{ext}"))
}

/// Flushes a file to stable storage (write access: Windows'
/// `FlushFileBuffers` refuses a read-only handle).
pub fn sync_file(path: &Path) -> std::io::Result<()> {
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)?
        .sync_all()
}

/// Flushes a directory's entries (the renames) to stable storage. Unix
/// only: Windows cannot open a directory as a file, and NTFS journals
/// renames itself.
#[cfg(unix)]
pub fn sync_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::File::open(dir)?.sync_all()
}

#[cfg(not(unix))]
pub fn sync_dir(_dir: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Finishes staging the temp already written at `paint/<temp>`: fsyncs
/// it, stores its bytes in the project's `AssetStore` (deduped by
/// content hash) and fsyncs the blob — both are durable before any
/// manifest can name them.
pub fn finish_paint_stage(
    project_dir: &Path,
    store: &umber_core::AssetStore,
    name: &str,
    temp: String,
    bytes: &[u8],
) -> std::io::Result<PaintCommitEntry> {
    sync_file(&paint_sidecar_dir(project_dir).join(&temp))?;
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");
    let asset = store.store(bytes, ext).map_err(std::io::Error::other)?;
    sync_file(&paint_asset_blob_path(project_dir, &asset))?;
    Ok(PaintCommitEntry {
        name: name.to_string(),
        temp,
        asset,
    })
}

/// Stages `bytes` as the next commit's `name` (temp + store blob).
/// Creates `name`'s parent directory first — a no-op for a flat name
/// (the sidecar directory already exists), the channel subdirectory's
/// first write otherwise (`paint/<channel>/` nests one level under
/// `paint/`, created lazily like the tile pool it mirrors).
pub fn stage_paint_bytes(
    project_dir: &Path,
    store: &umber_core::AssetStore,
    name: &str,
    bytes: &[u8],
) -> std::io::Result<PaintCommitEntry> {
    let temp = paint_temp_name(name);
    let path = paint_sidecar_dir(project_dir).join(&temp);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, bytes)?;
    finish_paint_stage(project_dir, store, name, temp, bytes)
}

/// Stages one tile readback as a Linear-transfer PNG temp (the same
/// `write_png` call the sidecar always used — byte-exact), then stores
/// the encoded bytes. Creates `name`'s parent directory first, like
/// [`stage_paint_bytes`].
pub fn stage_paint_png(
    project_dir: &Path,
    store: &umber_core::AssetStore,
    name: &str,
    width: u32,
    height: u32,
    rgba: &[u8],
) -> std::io::Result<PaintCommitEntry> {
    let temp = paint_temp_name(name);
    let path = paint_sidecar_dir(project_dir).join(&temp);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    umber_export::png::write_png(
        &path,
        width,
        height,
        rgba,
        umber_export::png::Transfer::Linear,
    )
    .map_err(std::io::Error::other)?;
    let bytes = std::fs::read(&path)?;
    finish_paint_stage(project_dir, store, name, temp, &bytes)
}

/// Writes the commit manifest: temp, fsync, then the rename to
/// `manifest.pending` — THE commit point. Before it returns, an
/// interrupted save leaves the old sidecar intact; after, the next open
/// rolls the new one forward.
pub fn commit_paint_manifest(
    project_dir: &Path,
    entries: &[PaintCommitEntry],
    dormant: &[u64],
) -> std::io::Result<()> {
    let dir = paint_sidecar_dir(project_dir);
    let temp = dir.join(paint_temp_name("manifest.pending"));
    std::fs::write(&temp, format_paint_commit(entries, dormant))?;
    sync_file(&temp)?;
    std::fs::rename(&temp, paint_commit_pending_path(project_dir))?;
    // Past the commit point an error must not read as "not committed".
    if let Err(e) = sync_dir(&dir) {
        log::warn!("paint save: committed, directory fsync failed: {e:#}");
    }
    Ok(())
}

/// Every paint artifact currently on disk under `dir`, as the SAME
/// relative name the manifest uses: a flat `tile-<N>.png` at the top level
/// (the reserved baseColor channel), `<channel>/tile-<N>.png` one level
/// into a channel subdirectory, or `layers/<id>/<channel>/tile-<N>.png`
/// two levels into the layer tree (design §6.1 — exactly as deep as the
/// per-layer layout ever nests). Layer-mask `mask-<layer>-tile-<N>.png`
/// files are listed too (flat, top level only): the prune step must drop a
/// detached layer's mask just like a dropped tile, or re-saves resurrect
/// dead masks.
/// Used by the prune step so a dropped tile, a dropped channel's tile, or
/// a dropped layer's planes are pruned regardless of which directory they
/// live in. Anything the parse rule rejects (a 3-component `layers/…`
/// path, foreign junk) is never listed and never pruned — the prune only
/// removes what the manifest's trust boundary admits.
pub fn list_paint_tile_names(dir: &Path) -> std::io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir)?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if !path.is_dir() {
            if parse_paint_tile_name(&name).is_some() || parse_paint_mask_name(&name).is_some() {
                names.push(name);
            }
            continue;
        }
        if name == "layers" {
            for id_entry in std::fs::read_dir(&path)?.flatten() {
                let id_name = id_entry.file_name().to_string_lossy().into_owned();
                let id_path = id_entry.path();
                if !id_path.is_dir() {
                    continue;
                }
                for channel_entry in std::fs::read_dir(&id_path)?.flatten() {
                    let channel_name = channel_entry.file_name().to_string_lossy().into_owned();
                    let channel_path = channel_entry.path();
                    if !channel_path.is_dir() {
                        continue;
                    }
                    for inner in std::fs::read_dir(&channel_path)?.flatten() {
                        let inner_name = inner.file_name().to_string_lossy().into_owned();
                        if parse_paint_tile_name(&inner_name).is_some() {
                            names.push(format!("layers/{id_name}/{channel_name}/{inner_name}"));
                        }
                    }
                }
            }
            continue;
        }
        for inner in std::fs::read_dir(&path)?.flatten() {
            let inner_name = inner.file_name().to_string_lossy().into_owned();
            if parse_paint_tile_name(&inner_name).is_some() {
                names.push(format!("{name}/{inner_name}"));
            }
        }
    }
    Ok(names)
}

/// Removes the now-empty directories under `paint/layers/` bottom-up
/// (`<channel>/`, then `<id>/` — design §6.1 [m10]), so a deleted layer
/// leaves no empty tree behind. Only directories under `paint/layers/`
/// are candidates; a directory that still holds any file (including
/// parse-rejected junk the prune never lists) survives.
pub fn prune_empty_layer_dirs(project_dir: &Path) {
    let layers = paint_sidecar_dir(project_dir).join("layers");
    let Ok(ids) = std::fs::read_dir(&layers) else {
        return;
    };
    for id in ids.flatten() {
        let id_path = id.path();
        if !id_path.is_dir() {
            continue;
        }
        if let Ok(channels) = std::fs::read_dir(&id_path) {
            for channel in channels.flatten() {
                let channel_path = channel.path();
                let empty = channel_path.is_dir()
                    && std::fs::read_dir(&channel_path)
                        .is_ok_and(|mut entries| entries.next().is_none());
                if empty {
                    if let Err(e) = std::fs::remove_dir(&channel_path) {
                        log::warn!(
                            "paint save: empty layer dir {} not removed: {e}",
                            channel_path.display()
                        );
                    }
                }
            }
        }
        let empty = std::fs::read_dir(&id_path).is_ok_and(|mut entries| entries.next().is_none());
        if empty {
            if let Err(e) = std::fs::remove_dir(&id_path) {
                log::warn!(
                    "paint save: empty layer dir {} not removed: {e}",
                    id_path.display()
                );
            }
        }
    }
}

/// Applies a pending commit (idempotent — safe to re-run after a crash at
/// any step): renames each staged temp over its sidecar name; an entry
/// whose temp is already gone is checked against its store blob and
/// re-materialized from it if the sidecar differs; tile PNGs the manifest
/// does not list are pruned (a re-save never resurrects a dropped tile);
/// finally `manifest.pending` becomes `manifest.txt`. Returns whether a
/// commit was pending.
///
/// `max_steps` is the fault-injection seam: `None` runs to completion
/// (every production caller); `Some(k)` fails the (k+1)-th mutating step
/// with an injected error, leaving the disk exactly as a crash there
/// would — including a half-written temp when the step was a write.
pub fn apply_paint_commit(project_dir: &Path, max_steps: Option<usize>) -> std::io::Result<bool> {
    let dir = paint_sidecar_dir(project_dir);
    let pending = paint_commit_pending_path(project_dir);
    let text = match std::fs::read_to_string(&pending) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    };
    let entries = parse_paint_commit(&text).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "malformed paint manifest")
    })?;
    let store = umber_core::AssetStore::new(project_dir);
    let mut steps = 0usize;
    let mut step = || {
        if max_steps.is_some_and(|max| steps >= max) {
            return Err(std::io::Error::other("injected fault"));
        }
        steps += 1;
        Ok(())
    };
    for entry in &entries {
        let temp = dir.join(&entry.temp);
        let target = dir.join(&entry.name);
        if temp.exists() {
            step()?;
            std::fs::rename(&temp, &target)?;
            continue;
        }
        // The temp is gone: this rename already ran, or the temp was lost.
        // Either way the content-addressed blob is the committed truth.
        let blob = store.load(&entry.asset).map_err(std::io::Error::other)?;
        if std::fs::read(&target).is_ok_and(|bytes| bytes == blob) {
            continue;
        }
        // Re-materialize through a temp the manifest never names: a crash
        // mid-write leaves a torn `.restore` orphan (swept later), never a
        // torn temp the next recovery would rename into place.
        let restore = dir.join(paint_temp_name(&format!("{}.restore", entry.name)));
        if let Err(fault) = step() {
            let _ = std::fs::write(&restore, &blob[..blob.len() / 2]);
            return Err(fault);
        }
        std::fs::write(&restore, &blob)?;
        sync_file(&restore)?;
        std::fs::rename(&restore, &target)?;
    }
    let listed: std::collections::HashSet<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    for name in list_paint_tile_names(&dir)? {
        if !listed.contains(name.as_str()) {
            step()?;
            std::fs::remove_file(dir.join(&name))?;
        }
    }
    // A deleted layer's planes prune as files above; their now-empty
    // directories go here (bottom-up, `paint/layers/` only).
    prune_empty_layer_dirs(project_dir);
    sync_dir(&dir)?;
    step()?;
    std::fs::rename(&pending, paint_manifest_path(project_dir))?;
    sync_dir(&dir)?;
    Ok(true)
}

/// Crash recovery, run before anything reads or stages `paint/`: rolls a
/// pending commit forward (an interrupted save's NEW state), then sweeps
/// staged temps no commit names (an interrupted save's uncommitted
/// leftovers — the OLD state stands). A failed roll-forward keeps the
/// temps (they are its source) and logs; the next open retries.
pub fn recover_paint_sidecar(project_dir: &Path) {
    match apply_paint_commit(project_dir, None) {
        Ok(true) => log::info!("paint: rolled an interrupted save forward"),
        Ok(false) => {}
        Err(e) => {
            log::warn!("paint: interrupted save not rolled forward yet: {e:#}");
            return;
        }
    }
    sweep_paint_temps(&paint_sidecar_dir(project_dir));
}

/// Removes every staged temp under `dir`, recursing fully (channel
/// subdirectories AND the `layers/` tree) so a crash mid-save on any plane
/// leaves no orphan `<name>.tmp.<pid>` behind either.
pub fn sweep_paint_temps(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            sweep_paint_temps(&path);
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if is_paint_temp_name(&name) {
            if let Err(e) = std::fs::remove_file(&path) {
                log::warn!("paint: orphan temp {} not removed: {e}", path.display());
            }
        }
    }
}

/// What `commit_paint_tiles` staged: `staged` counts manifest-listed
/// plane PNGs (a clean-skipped plane re-stages its on-disk bytes, so it
/// counts); `read_back` counts GPU readbacks — the §8 T20 observable (an
/// in-place save right after open reads back nothing).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PaintSaveReport {
    /// Plane PNGs listed in the committed manifest.
    pub staged: usize,
    /// Planes read back from the GPU (`read_plane` calls).
    pub read_back: usize,
}

/// §6.2 condition 1: the save's target project dir equals the plane's
/// committed dir. Compared after `std::fs::canonicalize`; a canonicalize
/// failure on either side counts as different.
pub fn same_project_dir(target: &Path, committed: &Path) -> bool {
    match (
        std::fs::canonicalize(target),
        std::fs::canonicalize(committed),
    ) {
        (Ok(current), Ok(recorded)) => current == recorded,
        _ => false,
    }
}

/// §6.3's minimal bump [M2]: the save writes version 2 iff it will stage
/// at least one plane whose owner is a `Layer(id)` LIVE in the stack.
/// Orphans and tombstones don't count (`live` is the stack's ids), and a
/// layer-less or headless session stays version 1 (readable by old
/// builds). Computable from the session before either file is written. A
/// loaded model is never re-stamped: the save recomputes the version every
/// time, so re-saving a version-2 project after deleting its only planed
/// layer writes 1 again (§8 T24).
pub fn paint_save_version(
    paint: &crate::paint_state::PaintState,
    live: &std::collections::HashSet<u64>,
) -> u32 {
    let has_live_layer_plane = paint.planes_present().iter().any(
        |(owner, _, _)| matches!(owner, crate::paint_state::PlaneOwner::Layer(id) if live.contains(id)),
    );
    if has_live_layer_plane {
        umber_core::project::LAYER_PLANES_PROJECT_VERSION
    } else {
        umber_core::project::CURRENT_PROJECT_VERSION
    }
}

/// Writes every materialized plane — Base AND every live layer-owned one —
/// into the project sidecar as a Linear-transfer PNG (byte-exact — the
/// export dialog's `Srgb` would move the bytes through the IEC curve),
/// crash-safely: every artifact is staged (temp + `AssetStore` blob)
/// before ONE manifest rename commits the save (see the module docs).
/// Reads through `PaintState::read_plane` for EVERY owner, Base included
/// (design §1.1 [B3]): the save loop is a PLANE reader, never a composite
/// reader — writing the cache into `paint/tile-<N>.png` would double-count
/// every layer on the next reopen (§8 T21).
///
/// A clean plane may skip its readback ONLY under §6.2's two-part test
/// [B1]: the target dir equals the plane's committed dir (canonicalized)
/// AND the target dir's sidecar file for that plane hashes (blake3) to the
/// recorded commit hash. The check re-reads and re-hashes the file — a
/// file read plus blake3, far cheaper than a GPU readback plus PNG encode.
/// Every other case reads back and encodes: `committed == None`, a
/// different dir (save-as), a missing file, a hash mismatch.
///
/// A failed readback keeps the artifact's previous sidecar (if any) ONLY
/// for an in-place save: re-staging reads the TARGET dir's file, and a
/// save-as must never re-stage bytes from a dir the plane was not committed
/// to (that was r1's silent-drop shape — every clean plane lost from a
/// save-as, Base included). In a save-as a failed readback is a loud
/// per-plane failure (warn log), never a silent omission.
///
/// `live` is the document stack's layer ids: orphan planes (an id no
/// longer in the stack) are never staged — the next save's manifest prunes
/// them (§6.4) — and never count toward the version decision (§6.3).
/// Prunes sidecar files for planes the session no longer holds, then
/// returns what was staged and what was read back (a zero-`staged` commit
/// still commits the size files; an `Err` means the commit point was never
/// reached — the previous paint stays intact).
pub fn save_paint_tiles(
    paint: &mut crate::paint_state::PaintState,
    project_dir: &Path,
    live: &std::collections::HashSet<u64>,
    dormant: &[u64],
) -> usize {
    match commit_paint_tiles(paint, project_dir, live, dormant) {
        Ok(report) => report.staged,
        Err(e) => {
            log::warn!("paint save: not committed, the previous paint stays intact: {e:#}");
            0
        }
    }
}

/// [`save_paint_tiles`]'s body: stage everything, commit, apply. An `Err`
/// means the commit point was never reached.
pub fn commit_paint_tiles(
    paint: &mut crate::paint_state::PaintState,
    project_dir: &Path,
    live: &std::collections::HashSet<u64>,
    dormant: &[u64],
) -> std::io::Result<PaintSaveReport> {
    std::fs::create_dir_all(paint_sidecar_dir(project_dir))?;
    // An earlier crash's pending commit lands (or its orphans go) before
    // this save stages anything.
    recover_paint_sidecar(project_dir);
    let store = umber_core::AssetStore::new(project_dir);
    let mut entries = Vec::new();
    // The session sizes ride along even with zero tiles (a size choice
    // with no paint yet must survive the round trip). The legacy global
    // first (the active set's size — old readers apply one size to every
    // tile, and with pixels the PNG dims lead anyway).
    let (width, height) = paint.target_size();
    entries.push(stage_paint_bytes(
        project_dir,
        &store,
        "target_size.txt",
        format_paint_target_size(width, height).as_bytes(),
    )?);
    // The per-set mapping: sizes for every set holding a saved tile
    // plus the active set (a chooser with no paint yet survives), and
    // one assignment line per saved tile. Sorted by construction
    // (BTreeMap), so saves are byte-deterministic.
    let tiles = paint.tiles_present();
    let mut mapping = PaintSetSizes::default();
    for tile in &tiles {
        let set = paint.set_of_tile(*tile).to_string();
        mapping.tiles.insert(*tile, set.clone());
        mapping.sizes.insert(set, paint.tile_size(*tile));
    }
    let active = paint.active_set().to_string();
    if !mapping.sizes.contains_key(&active) {
        mapping
            .sizes
            .insert(active.clone(), paint.size_of_set(&active));
    }
    // The per-channel overrides for exactly those same sets (the set-size
    // rule applied to channels — see the module docs); sorted by
    // construction (BTreeMap), so saves stay byte-deterministic.
    for ((set, channel), (width, height)) in paint.channel_sizes() {
        if mapping.sizes.contains_key(set) {
            mapping
                .channels
                .insert((set.clone(), channel.clone()), (*width, *height));
        }
    }
    entries.push(stage_paint_bytes(
        project_dir,
        &store,
        "target_sizes.txt",
        format_paint_set_sizes(&mapping).as_bytes(),
    )?);
    let mut report = PaintSaveReport::default();
    // The plane census (§1.1): every materialized `(owner, tile, channel)`
    // stages its own file. Collected up front (owned) so no session borrow
    // is held across the `&mut` readback/commit calls below.
    for (owner, tile, channel) in paint.planes_present() {
        if let crate::paint_state::PlaneOwner::Layer(id) = owner {
            if !live.contains(&id) {
                continue;
            }
        }
        let name = paint_plane_sidecar_name(owner, &channel, tile);
        let tag = plane_owner_tag(owner);
        let committed = paint.plane_committed(owner, tile, &channel);
        // §6.2's clean-skip [B1]: same dir AND same bytes on disk.
        if let Some((committed_dir, committed_hash)) = committed.as_ref() {
            if same_project_dir(project_dir, committed_dir) {
                let path = paint_sidecar_dir(project_dir).join(&name);
                match std::fs::read(&path) {
                    Ok(bytes) if blake3::hash(&bytes) == *committed_hash => {
                        let entry = stage_paint_bytes(project_dir, &store, &name, &bytes)?;
                        paint.mark_plane_committed(
                            owner,
                            tile,
                            &channel,
                            project_dir.to_path_buf(),
                            *committed_hash,
                        );
                        entries.push(entry);
                        report.staged += 1;
                        continue;
                    }
                    Ok(_) => {
                        log::warn!(
                            "paint save: {tag}/{channel} tile {tile} changed on disk since commit; reading back"
                        );
                    }
                    Err(e) => {
                        log::warn!(
                            "paint save: {tag}/{channel} tile {tile} sidecar unreadable ({e:#}); reading back"
                        );
                    }
                }
            }
        }
        let Some(image) = paint.read_plane(owner, tile, &channel) else {
            // A failed readback keeps the artifact's previous sidecar
            // (if any): it is re-staged as-is so the commit does not
            // prune it — but ONLY for an in-place save (condition 1
            // above). In a save-as there is no committed source to keep
            // from, so this is a loud per-plane failure instead.
            log::warn!("paint save: {tag}/{channel} tile {tile} readback failed");
            let in_place = committed
                .as_ref()
                .is_some_and(|(committed_dir, _)| same_project_dir(project_dir, committed_dir));
            if !in_place {
                log::warn!(
                    "paint save: {tag}/{channel} tile {tile} has no in-place sidecar to keep; skipping"
                );
                continue;
            }
            let path = paint_plane_path(project_dir, owner, &channel, tile);
            match std::fs::read(&path) {
                Ok(bytes) => {
                    entries.push(stage_paint_bytes(project_dir, &store, &name, &bytes)?);
                }
                Err(e) => {
                    log::warn!(
                        "paint save: {tag}/{channel} tile {tile} previous file unreadable ({e:#}); skipping"
                    );
                }
            }
            continue;
        };
        let entry = stage_paint_png(
            project_dir,
            &store,
            &name,
            image.width,
            image.height,
            &image.data,
        )?;
        // The commit hash is over the STAGED png bytes (not the readback:
        // the clean-skip compares against the file). Re-read the staged
        // temp — one extra file read per read-back plane, no GPU cost.
        let staged_bytes = std::fs::read(paint_sidecar_dir(project_dir).join(&entry.temp))?;
        paint.mark_plane_committed(
            owner,
            tile,
            &channel,
            project_dir.to_path_buf(),
            blake3::hash(&staged_bytes),
        );
        entries.push(entry);
        report.staged += 1;
        report.read_back += 1;
    }
    // Layer masks ride as their own flat sub-namespace (`mask-<layer>-
    // tile-<N>.png` — see `paint_mask_name`): one Linear-transfer PNG per
    // materialized plane, gray-expanded through `mask_to_rgba`. Only live
    // layers' planes are staged — the caller prunes orphans against the
    // document first (see `save_project`), so a deleted layer's mask can
    // never be re-committed here (and the apply step prunes whatever the
    // manifest does not list). A plane whose size disagrees with its tile
    // is skipped loudly, like a failed readback (a torn plane must never
    // gate the wrong texels on open).
    for (layer, tile) in paint.layer_mask_tiles() {
        let name = paint_mask_name(layer, tile);
        let Some(plane) = paint.layer_mask(layer, tile) else {
            continue;
        };
        let (width, height) = paint.tile_size(tile);
        if plane.len() != width as usize * height as usize {
            log::warn!(
                "paint save: mask {layer}/tile {tile} is {} bytes, want {}x{}; skipping",
                plane.len(),
                width,
                height
            );
            continue;
        }
        entries.push(stage_paint_png(
            project_dir,
            &store,
            &name,
            width,
            height,
            &mask_to_rgba(plane),
        )?);
        report.staged += 1;
    }
    commit_paint_manifest(project_dir, &entries, dormant)?;
    // Committed: from here an interruption rolls forward on the next open.
    if let Err(e) = apply_paint_commit(project_dir, None) {
        log::warn!("paint save: committed, apply stopped early ({e:#}); next open finishes it");
    }
    Ok(report)
}

/// Restores the paint sidecar into `paint` through the per-plane publish
/// path (`PaintState::publish_plane` — one full-plane write per plane, in
/// the sidecar's sorted order). Decodes each sidecar PNG, flips it back out
/// of the PNG reader's row order, applies the stored sizes first (each
/// resize drops the old contents and the session-scoped stroke-undo history
/// clears with the first one, so no stale rect can undo against the
/// restored pixels), then drains so the pixels have landed before
/// returning.
///
/// Dual read, keyed on the sidecar's CONTENT (§6.3): legacy flat and
/// channel-qualified paths always load into Base; `layers/…` paths load
/// into layer planes. A v1 project therefore lands in Base verbatim, and
/// the first frame is byte-identical (a Base-only `(tile, channel)` reads
/// Base directly). Every loaded plane starts committed to the opened dir
/// (`committed = (opened_dir, blake3(png_bytes))`), so an in-place save
/// right after open re-stages without a readback (§6.2).
///
/// Orphan planes (a `Layer(id)` no longer in the stack — `live` is the
/// loaded stack's ids) are ignored with a log: they never enter the
/// session, never count toward the version decision, and the next save's
/// manifest prunes them (§6.4). A budget refusal on any plane is a loud
/// skip (warn log — §1.3's load rule), never a silent hole.
///
/// Per-set sizes come from `target_sizes.txt` when present (authoritative:
/// each tile must match ITS set's size); without it the v1 shape holds —
/// every tile joins the active set at the first tile's dims, and the
/// legacy `target_size.txt` global only matters with no tiles (applied to
/// every set — the v1→v2 migration). Tiles whose dims disagree with their
/// set's size are skipped LOUDLY (warn log plus the report's skip list —
/// never a silent resample). A project with no sidecar tiles clears the
/// session to transparent instead of keeping the previous document's
/// pixels. Returns how many tiles were restored and what was refused.
/// Decodes one sidecar PNG, flipped back out of the reader's row order.
/// `None` (logged) on a decode failure — the caller skips that artifact.
pub fn decode_paint_png(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    match umber_export::png::read_png_rgba8(path) {
        Ok((width, height, bytes)) => {
            Some((width, height, flip_rows_vertical(&bytes, width, height)))
        }
        Err(e) => {
            log::warn!("paint open: {} decode failed: {e:#}", path.display());
            None
        }
    }
}
/// One layer's open-path seeding row: everything the fill/material
/// open-seeding needs, cloned out for the borrow-split.
pub type OpenSeedRow = (
    u64,
    umber_core::LayerKind,
    Option<umber_mesh::FillParams>,
    Option<umber_core::MaterialParams>,
    umber_core::BlendMode,
    f32,
);

/// One loaded plane sidecar's decoded payload, keyed by its plane
/// identity (owner, channel, tile) — the open-path plane census.
pub type LoadedPlaneSidecars =
    std::collections::BTreeMap<(crate::paint_state::PlaneOwner, String, u16), (u32, u32, Vec<u8>)>;

/// Every `(owner, channel, tile)` artifact currently on disk under `dir`
/// (design §6.1's dual read, keyed on the sidecar's CONTENT, not on
/// `project.json`): flat `tile-<N>.png` files first (the reserved
/// baseColor channel's Base plane), then one level into every channel
/// subdirectory EXCEPT `layers/` (an explicit channel's Base plane), then
/// the `layers/<id>/<channel>/` tree (layer-owned planes). The flat pass
/// runs first so a (forward-compatible, never written by this crate today)
/// literal `paint/baseColor/` wins over a same-tile flat collision. Every
/// relative name goes through [`parse_paint_sidecar_name`] — the single
/// parse rule for both passes.
/// trust boundary — so a rejected shape never reaches the session.
pub fn collect_paint_tiles(dir: &Path) -> LoadedPlaneSidecars {
    let mut found = std::collections::BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    let mut channel_dirs = Vec::new();
    let mut layers_dir = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if path.is_dir() {
            if name == "layers" {
                layers_dir = Some(path);
            } else {
                channel_dirs.push((name, path));
            }
            continue;
        }
        let Some(tile) = parse_paint_tile_name(&name) else {
            continue;
        };
        if let Some(decoded) = decode_paint_png(&path) {
            found.insert(
                (
                    crate::paint_state::PlaneOwner::Base,
                    crate::paint_state::BASE_COLOR_CHANNEL.to_string(),
                    tile,
                ),
                decoded,
            );
        }
    }
    for (dir_name, path) in channel_dirs {
        let Ok(inner) = std::fs::read_dir(&path) else {
            continue;
        };
        for inner_entry in inner.flatten() {
            let inner_name = inner_entry.file_name().to_string_lossy().into_owned();
            let key = parse_paint_sidecar_name(&format!("{dir_name}/{inner_name}"));
            let Some((owner, channel, tile)) = key else {
                continue;
            };
            if let Some(decoded) = decode_paint_png(&inner_entry.path()) {
                found.insert((owner, channel, tile), decoded);
            }
        }
    }
    let Some(layers) = layers_dir else {
        return found;
    };
    let Ok(ids) = std::fs::read_dir(&layers) else {
        return found;
    };
    for id_entry in ids.flatten() {
        let id_name = id_entry.file_name().to_string_lossy().into_owned();
        let id_path = id_entry.path();
        if !id_path.is_dir() {
            continue;
        }
        let Ok(channels) = std::fs::read_dir(&id_path) else {
            continue;
        };
        for channel_entry in channels.flatten() {
            let channel_name = channel_entry.file_name().to_string_lossy().into_owned();
            let channel_path = channel_entry.path();
            if !channel_path.is_dir() {
                continue;
            }
            let Ok(files) = std::fs::read_dir(&channel_path) else {
                continue;
            };
            for file in files.flatten() {
                let file_name = file.file_name().to_string_lossy().into_owned();
                let rel = format!("layers/{id_name}/{channel_name}/{file_name}");
                let Some((owner, channel, tile)) = parse_paint_sidecar_name(&rel) else {
                    continue;
                };
                if let Some(decoded) = decode_paint_png(&file.path()) {
                    found.insert((owner, channel, tile), decoded);
                }
            }
        }
    }
    found
}

/// Every `(layer, tile)` mask artifact currently on disk under `dir`:
/// flat `mask-<layer>-tile-<N>.png` files only (masks never nest — see
/// [`paint_mask_name`]). Decoded like paint tiles (the PNG reader's row
/// order flipped back); a decode failure skips that plane loudly.
pub fn collect_paint_masks(
    dir: &Path,
) -> std::collections::BTreeMap<(u64, u16), (u32, u32, Vec<u8>)> {
    let mut found = std::collections::BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some((layer, tile)) = parse_paint_mask_name(&name) else {
            continue;
        };
        if let Some(decoded) = decode_paint_png(&path) {
            found.insert((layer, tile), decoded);
        }
    }
    found
}

pub fn load_paint_tiles_reported(
    paint: &mut crate::paint_state::PaintState,
    project_dir: &Path,
    live: &std::collections::HashSet<u64>,
) -> PaintLoadReport {
    // An interrupted save resolves first (rolled forward when it had
    // committed, its orphans swept when not), so the sidecar read below
    // is the old project or the new one — never a torn mix.
    recover_paint_sidecar(project_dir);
    let dir = paint_sidecar_dir(project_dir);
    let found = collect_paint_tiles(&dir);
    let masks = collect_paint_masks(&dir);
    let mapping = std::fs::read_to_string(paint_set_sizes_path(project_dir))
        .ok()
        .and_then(|text| parse_paint_set_sizes(&text));
    let active = paint.active_set().to_string();
    if found.is_empty() && masks.is_empty() {
        // No paint persisted: the newly opened document must not show the
        // previous document's pixels (the layer stack is replaced
        // wholesale on open; the paint follows the same rule).
        match &mapping {
            Some(known) => {
                // Every known set goes to its mapped size (unmapped sets
                // resize to their live size — the resize still drops their
                // stale contents); a missing/unparseable mapping degrades
                // below, never a panic. Assignment-only sets (a tile line
                // with no size line) join the resize at the default size,
                // so no previous document's pixels survive on them either.
                // Channel-only sets (an override line with no size line)
                // join for the same reason — their override applies below.
                let mut names = paint.set_names();
                for name in known
                    .sizes
                    .keys()
                    .chain(known.tiles.values())
                    .chain(known.channels.keys().map(|(set, _)| set))
                {
                    if !names.contains(name) {
                        names.push(name.clone());
                    }
                }
                for name in &names {
                    let (width, height) = known
                        .sizes
                        .get(name)
                        .copied()
                        .unwrap_or_else(|| paint.size_of_set(name));
                    paint.resize_set(name, width, height);
                }
                // Channel overrides land AFTER the set resizes (a set
                // resize rebuilds inheriting channels at the set's size —
                // the override must win). No paint persisted, so this only
                // records sizes for later tiles and drops stale targets.
                for ((set, channel), (width, height)) in &known.channels {
                    paint.resize_channel(set, channel, *width, *height);
                }
            }
            None => {
                // The v1 shape: the saved global (or the live size when the
                // sidecar stored none) applies to EVERY set — the
                // migration. Either way the pool is dropped: the clear
                // still runs.
                let (width, height) = std::fs::read_to_string(paint_target_size_path(project_dir))
                    .ok()
                    .and_then(|text| parse_paint_target_size(&text))
                    .unwrap_or_else(|| paint.target_size());
                for name in paint.set_names() {
                    paint.resize_set(&name, width, height);
                }
            }
        }
        if let Err(e) = paint.process_pending() {
            log::warn!("paint open: clearing the session failed: {e:#}");
        }
        return PaintLoadReport::default();
    }
    match &mapping {
        Some(known) => {
            // The mapping is authoritative: every known set goes to its
            // mapped size first (unmapped sets resize to their live size,
            // dropping stale pixels), then assignments re-derive from the
            // sidecar so a previous document's sets never steer this one.
            // Channel-only sets join the resize (their override applies
            // below); see the empty-sidecar branch above.
            let mut names = paint.set_names();
            for name in known
                .sizes
                .keys()
                .chain(known.tiles.values())
                .chain(known.channels.keys().map(|(set, _)| set))
            {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
            // Assignments first, then the resize: a pre-existing tile's
            // TARGET keeps its old size when the mapping lands (assignment
            // steers future creations only — the field docs), so the set's
            // resize must run AFTER the assignment for the tile's target
            // to actually take the set's size on reopen.
            paint.clear_tile_assignments();
            for (tile, set) in &known.tiles {
                paint.assign_tile_to_set(*tile, set);
            }
            for name in &names {
                let (width, height) = known
                    .sizes
                    .get(name)
                    .copied()
                    .unwrap_or_else(|| paint.size_of_set(name));
                paint.resize_set(name, width, height);
            }
            // Channel overrides land AFTER the set resizes (which rebuild
            // inheriting channels at the set's size — the override must
            // win) and BEFORE the PNG uploads below (which size-check
            // against the channel's size): a reopened override is in place
            // before its pixels land, so a later `ensure_channel` — or the
            // upload's own materialization — builds at the override.
            for ((set, channel), (width, height)) in &known.channels {
                paint.resize_channel(set, channel, *width, *height);
            }
        }
        None => {
            // The v1 shape, byte-identical: every tile joins the active
            // set and the first tile's dims lead (a resize drops the old
            // contents and clears the session-scoped stroke-undo history
            // with them). No mapping means no `target_sizes.txt`, which
            // this crate always writes alongside any channel data — so
            // `found` here holds ONLY Base baseColor entries, and `found`'s
            // `(owner, channel, tile)` key order reduces to tile order
            // (every key shares owner and channel).
            paint.clear_tile_assignments();
            let mut tile_ids: Vec<u16> = found.keys().map(|(_, _, tile)| *tile).collect();
            tile_ids.dedup();
            for tile in &tile_ids {
                paint.assign_tile_to_set(*tile, &active);
            }
            let (first_width, first_height, _) = found
                .values()
                .next()
                .cloned()
                .expect("found is non-empty here");
            paint.resize_set(&active, first_width, first_height);
        }
    }
    if let Err(e) = paint.process_pending() {
        log::warn!("paint open: resize to the saved size failed: {e:#}");
        return PaintLoadReport::default();
    }
    let mut report = PaintLoadReport::default();
    for ((owner, channel, tile), (width, height, bytes)) in found {
        // Orphans (a saved plane whose layer is not in the loaded stack)
        // never enter the session (§6.4): ignored with a log, pruned by
        // the next save's manifest.
        if let crate::paint_state::PlaneOwner::Layer(id) = owner {
            if !live.contains(&id) {
                log::warn!(
                    "paint open: layer {id}/{channel} tile {tile} has no live layer; ignoring"
                );
                continue;
            }
        }
        // §6.3's "Base is non-empty" arm (see `PaintLoadReport`): any
        // non-zero byte in any loaded Base plane.
        if matches!(owner, crate::paint_state::PlaneOwner::Base)
            && !report.base_nonempty
            && bytes.iter().any(|&b| b != 0)
        {
            report.base_nonempty = true;
        }
        // Tiles the mapping never assigned resolve to the active set;
        // recording the resolution keeps the next save's mapping exact.
        let set = paint.set_of_tile(tile).to_string();
        paint.assign_tile_to_set(tile, &set);
        // The CHANNEL's size (its override, else the set's — a channel
        // PNG must match ITS size, never silently resample).
        let expected = paint.tile_channel_size(tile, &channel);
        if (width, height) != expected {
            let (ew, eh) = expected;
            let tag = plane_owner_tag(owner);
            log::warn!(
                "paint open: {tag}/{channel} tile {tile} is {width}x{height}, want {ew}x{eh}; skipping"
            );
            report.skipped.push(PaintTileSkip {
                tile,
                channel: channel.clone(),
                expected,
                found: (width, height),
            });
            continue;
        }
        // The commit hash is over the sidecar PNG's bytes: re-read the
        // file (it decoded a moment ago, so this only fails under a
        // concurrent delete — which then skips the commit mark loudly).
        let sidecar =
            paint_sidecar_dir(project_dir).join(paint_plane_sidecar_name(owner, &channel, tile));
        let png_bytes = std::fs::read(&sidecar);
        match paint.publish_plane(owner, tile, &channel, width, height, bytes) {
            Ok(()) => {
                report.restored += 1;
                match png_bytes {
                    Ok(png) => {
                        paint.mark_plane_committed(
                            owner,
                            tile,
                            &channel,
                            project_dir.to_path_buf(),
                            blake3::hash(&png),
                        );
                    }
                    Err(e) => {
                        let tag = plane_owner_tag(owner);
                        log::warn!(
                            "paint open: {tag}/{channel} tile {tile} sidecar vanished ({e:#}); commit mark skipped"
                        );
                    }
                }
            }
            Err(refusal) => {
                let tag = plane_owner_tag(owner);
                log::warn!(
                    "paint open: {tag}/{channel} tile {tile} publish refused ({refusal:?}); skipping"
                );
            }
        }
    }
    // Layer masks restore through the size-checked publish path (the
    // plane rides in the PNG's red channel — see `rgba_to_mask`); a plane
    // whose dims disagree with its tile's set size is skipped loudly, the
    // same rule as paint tiles. Masks for layers the document no longer
    // holds stay session-side here — the caller prunes them against the
    // loaded stack (see `open_project`), so a previous document's masks
    // never gate the new one.
    for ((layer, tile), (width, height, bytes)) in masks {
        let expected = paint.tile_size(tile);
        if (width, height) != expected {
            let (ew, eh) = expected;
            log::warn!(
                "paint open: mask {layer}/tile {tile} is {width}x{height}, want {ew}x{eh}; skipping"
            );
            continue;
        }
        if paint.publish_layer_mask(layer, tile, rgba_to_mask(&bytes)) {
            report.masks_restored += 1;
        } else {
            log::warn!("paint open: mask {layer}/tile {tile} upload failed; skipping");
        }
    }
    if let Err(e) = paint.process_pending() {
        log::warn!("paint open: upload drain failed: {e:#}");
    }
    report
}

/// One fill/material layer's open-seeding verdict (§6.3 [B2, M11]):
/// dormant (provenance says its result is baked into Base), clean (its
/// planes hold the result), or dirty (nothing holds a result — evaluate
/// when a mesh is present).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerDormancy {
    /// Seeded CLEAN + badged: a legacy flat plane already contains the
    /// output (v1 header, no planes, non-empty Base), or a previous save
    /// listed it still-dormant (v2 header + `# dormant <id>`). Never
    /// re-evaluates until an explicit edit.
    Dormant,
    /// Seeded CLEAN, unbadged: its planes loaded (or it never needed
    /// any).
    Clean,
    /// Seeded DIRTY: planeless with nothing baked anywhere (v1 + empty
    /// Base, or v2 + not listed) — evaluates normally on mesh arrival.
    Dirty,
}

/// The dormancy decision (§6.3, pure — §8 T18/T31 pin it headless):
/// provenance (the paint manifest's header), not absence. "Planeless"
/// alone cannot tell legacy output baked into Base from never-evaluated.
pub fn dormancy_for(
    has_planes: bool,
    header_is_v2: bool,
    listed_dormant: bool,
    base_nonempty: bool,
) -> LayerDormancy {
    if has_planes {
        LayerDormancy::Clean
    } else if header_is_v2 {
        if listed_dormant {
            LayerDormancy::Dormant
        } else {
            // A v2 planeless fill/material never evaluated (a fill first
            // seen without a mesh, or a material whose every eval failed):
            // its result lives nowhere, so it is dirty, never dormant.
            LayerDormancy::Dirty
        }
    } else if base_nonempty {
        // A legacy flat plane already contains every fill's and
        // material's destructive output (Multiply and compounded publishes
        // are not invertible): re-evaluating over it would double-count.
        LayerDormancy::Dormant
    } else {
        // A v1 project with an empty Base baked nothing: the layer must
        // evaluate (nothing to double-count).
        LayerDormancy::Dirty
    }
}

/// Reads the paint manifest's header lines (§6.1 [B2, M11]): whether the
/// commit is v2 (any stage-4+ save), and which layer ids are still
/// dormant. An absent or unreadable manifest counts as legacy (v1, no
/// dormant ids) — erring toward dormancy, the byte-identical side.
pub fn parse_manifest_header(text: &str) -> (bool, Vec<u64>) {
    let mut is_v2 = false;
    let mut dormant = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line == "# Umber paint commit v2" {
            is_v2 = true;
        } else if let Some(id) = line.strip_prefix("# dormant ") {
            if let Ok(id) = id.trim().parse::<u64>() {
                dormant.push(id);
            }
        }
    }
    (is_v2, dormant)
}

/// Paint-sidecar persistence tests (CORE-LOOP CRITICAL 4): the filename
/// codec, the row-flip inverse, and the save→load byte comparison.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint_state;

    /// Requests a paint-capable device (graceful skip where no wgpu
    /// adapter exists — the same pattern as
    /// `paint_state::tests::try_request_device`; lavapipe covers CI).
    /// Returns `None` when the test must be skipped.
    fn try_request_device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::default();
        let Ok(adapter) =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        else {
            eprintln!("skipping: no wgpu adapter available");
            return None;
        };
        if !adapter
            .features()
            .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
        {
            eprintln!("skipping: adapter lacks TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES");
            return None;
        }
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
            ..Default::default()
        }))
        .ok()
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "umber-app-paint-{label}-{}-{n}",
            std::process::id()
        ))
    }

    /// The stage-2 orphan filter for Base-only sessions: no live layers,
    /// so only Base planes stage and load (orphan planes never stage).
    fn no_live_layers() -> std::collections::HashSet<u64> {
        std::collections::HashSet::new()
    }

    #[test]
    fn paint_tile_filename_parses_exactly() {
        assert_eq!(parse_paint_tile_name("tile-1001.png"), Some(1001));
        assert_eq!(parse_paint_tile_name("tile-65535.png"), Some(65535));
        assert_eq!(parse_paint_tile_name("project.json"), None);
        assert_eq!(parse_paint_tile_name("tile-1001.png.tmp.123"), None);
        assert_eq!(parse_paint_tile_name("tile-.png"), None);
        assert_eq!(parse_paint_tile_name("tile-abc.png"), None);
        assert_eq!(parse_paint_tile_name("tile-1001.exr"), None);
        assert_eq!(
            paint_tile_path(Path::new("/p"), 1001),
            PathBuf::from("/p/paint/tile-1001.png")
        );
    }

    #[test]
    fn target_size_round_trips_exactly_and_rejects_garbage() {
        // Every offered preset formats and parses back exactly (not a
        // range check — a dropped size would fail); trailing newlines
        // (hand-edited files) parse too.
        for size in [512u32, 1024, 2048, 4096, 8192] {
            let text = format_paint_target_size(size, size);
            assert_eq!(text, format!("{size}x{size}"));
            assert_eq!(parse_paint_target_size(&text), Some((size, size)));
            assert_eq!(
                parse_paint_target_size(&format!("{text}\n")),
                Some((size, size))
            );
        }
        assert_eq!(parse_paint_target_size("4096x4096"), Some((4096, 4096)));
        // Garbage degrades to None (the load path keeps the live size):
        // empty, one-sided, zero-sided, non-numeric, wrong separator.
        for bad in [
            "", "512", "x512", "512x", "0x512", "512x0", "0x0", "ax512", "512xa", "512*512",
        ] {
            assert_eq!(parse_paint_target_size(bad), None, "{bad:?} must reject");
        }
        assert_eq!(
            paint_target_size_path(Path::new("/p")),
            PathBuf::from("/p/paint/target_size.txt")
        );
    }

    #[test]
    fn flip_rows_vertical_is_its_own_inverse_with_exact_bytes() {
        // 2x3 RGBA: every row one distinct value, so the order is pinned
        // exactly (not a range check — a mirror would fail).
        let (width, height) = (2u32, 3u32);
        let mut bytes = Vec::new();
        for row in 0..3u8 {
            bytes.extend_from_slice(&[row, row, row, 255, row, row, row, 255]);
        }
        let flipped = flip_rows_vertical(&bytes, width, height);
        let mut expected = Vec::new();
        for row in [2u8, 1, 0] {
            expected.extend_from_slice(&[row, row, row, 255, row, row, row, 255]);
        }
        assert_eq!(flipped, expected, "row order must reverse exactly");
        assert_eq!(
            flip_rows_vertical(&flipped, width, height),
            bytes,
            "a second flip must restore the input (the load path depends on this)"
        );
    }

    #[test]
    fn save_open_round_trip_is_byte_identical() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut paint =
            paint_state::PaintState::new(device.clone(), queue.clone()).expect("paint builds");
        // A 25-event diagonal stroke (the conditioner needs a run of
        // events before it emits its first dab).
        paint.begin_stroke(emath::pos2(0.2, 0.2));
        for i in 1..=24 {
            let t = i as f32 * 0.02;
            paint.extend_stroke(emath::pos2(0.2 + t, 0.2 + t * 0.5));
        }
        paint.end_stroke();
        paint.process_pending().expect("drain succeeds");
        let before = paint
            .read_tile(umber_mesh::FIRST_TILE)
            .expect("readback succeeds");
        // Can-fail guard against a vacuous pass: a stroke that staged
        // nothing would compare two blank targets equal. Fresh targets
        // are zero-initialized (transparent black), so any painted texel
        // is nonzero somewhere.
        assert!(
            before.data.iter().any(|&b| b != 0),
            "the stroke must leave non-zero texels (else the comparison below is vacuous)"
        );

        let dir = unique_temp_dir("roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let saved = save_paint_tiles(&mut paint, &dir, &no_live_layers(), &[]);
        assert_eq!(saved, 1, "a single-tile session stores exactly tile 1001");
        assert!(paint_tile_path(&dir, umber_mesh::FIRST_TILE).exists());
        // Store-routed: the commit applied (no pending manifest, no temps)
        // and the provenance record names a store blob byte-identical to
        // the sidecar PNG the reopen reads.
        assert!(!paint_commit_pending_path(&dir).exists());
        let manifest = std::fs::read_to_string(paint_manifest_path(&dir)).unwrap();
        let entries = parse_paint_commit(&manifest).expect("the applied manifest parses");
        let tile_entry = entries
            .iter()
            .find(|e| e.name == "tile-1001.png")
            .expect("the manifest names the tile");
        assert_eq!(
            umber_core::AssetStore::new(&dir)
                .load(&tile_entry.asset)
                .unwrap(),
            std::fs::read(paint_tile_path(&dir, umber_mesh::FIRST_TILE)).unwrap(),
            "the store blob is the sidecar's exact bytes"
        );

        let mut fresh =
            paint_state::PaintState::new(device, queue).expect("fresh paint state builds");
        let restored = load_paint_tiles_reported(&mut fresh, &dir, &no_live_layers());
        assert_eq!(restored.restored, 1);
        let after = fresh
            .read_tile(umber_mesh::FIRST_TILE)
            .expect("fresh readback succeeds");
        assert_eq!(after.width, before.width);
        assert_eq!(after.height, before.height);
        assert_eq!(
            after.data, before.data,
            "save→load must restore byte-identical texels"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_sizes_codec_round_trips_exactly_and_rejects_garbage() {
        // Sizes sorted by set name, then assignments sorted by tile — the
        // save is byte-deterministic, so the exact text is pinned (not a
        // contains-check — a reordered save would fail).
        let mut mapping = PaintSetSizes::default();
        mapping.sizes.insert("Sword".to_string(), (4096, 4096));
        mapping.sizes.insert("Shield".to_string(), (2048, 1024));
        mapping.tiles.insert(1002, "Shield".to_string());
        let text = format_paint_set_sizes(&mapping);
        assert_eq!(
            text,
            "# Umber paint-set sizes v1\nShield=2048x1024\nSword=4096x4096\ntile-1002=Shield\n"
        );
        assert_eq!(parse_paint_set_sizes(&text), Some(mapping));
        // A stored non-square survives (the load path honors stored dims —
        // only the UI is square-only).
        assert_eq!(
            parse_paint_set_sizes("Shield=2048x1024\n"),
            Some(PaintSetSizes {
                sizes: std::collections::BTreeMap::from([("Shield".to_string(), (2048, 1024))]),
                channels: std::collections::BTreeMap::new(),
                tiles: std::collections::BTreeMap::new(),
            })
        );
        // Comments, blanks and malformed lines skip; the tile- prefix is
        // reserved for assignments even when the right side is a size.
        let mixed = "# header\n\nSword=512x512\nbogus\n=512x512\nA=\nA=0x512\ntile-1002=Sword\n";
        let parsed = parse_paint_set_sizes(mixed).expect("one size + one assignment parse");
        assert_eq!(parsed.sizes.len(), 1);
        assert_eq!(parsed.sizes.get("Sword"), Some(&(512, 512)));
        assert_eq!(parsed.tiles.get(&1002), Some(&"Sword".to_string()));
        assert!(
            parsed.channels.is_empty(),
            "no channel lines means all-inherit"
        );
        // Nothing parseable at all degrades to None (the load path falls
        // back to the legacy global): empty, comments-only, and every
        // malformed shape.
        for bad in [
            "",
            "# only a comment\n\n",
            "no-equals-here\n",
            "=512x512\n",
            "A=\n",
            "A=0x512\n",
            "tile-abc=X\n",
            "tile-=X\n",
        ] {
            assert_eq!(
                parse_paint_set_sizes(bad),
                None,
                "{bad:?} must yield no mapping"
            );
        }
        assert_eq!(
            paint_set_sizes_path(Path::new("/p")),
            PathBuf::from("/p/paint/target_sizes.txt")
        );
    }

    #[test]
    fn channel_sizes_codec_round_trips_and_old_files_load_all_inherit() {
        // Channel lines sort between sizes and assignments (set, then
        // channel) — the save is byte-deterministic, so the exact text is
        // pinned, and the header stays the v1 line on purpose (additive —
        // see `format_paint_set_sizes`).
        let mut mapping = PaintSetSizes::default();
        mapping.sizes.insert("Sword".to_string(), (4096, 4096));
        mapping
            .channels
            .insert(("Sword".to_string(), "roughness".to_string()), (512, 512));
        mapping
            .channels
            .insert(("Sword".to_string(), "height".to_string()), (1024, 2048));
        mapping.tiles.insert(1001, "Sword".to_string());
        let text = format_paint_set_sizes(&mapping);
        assert_eq!(
            text,
            "# Umber paint-set sizes v1\n\
             Sword=4096x4096\n\
             channel/Sword/height=1024x2048\n\
             channel/Sword/roughness=512x512\n\
             tile-1001=Sword\n"
        );
        assert_eq!(parse_paint_set_sizes(&text), Some(mapping));
        // A channel-only file still parses (never None while something
        // parsed — the load path must see the overrides).
        let only = parse_paint_set_sizes("channel/Body/height=256x256\n")
            .expect("a lone channel line parses");
        assert!(only.sizes.is_empty() && only.tiles.is_empty());
        assert_eq!(
            only.channels
                .get(&("Body".to_string(), "height".to_string())),
            Some(&(256, 256))
        );
        // Malformed channel lines skip without killing the good ones:
        // empty set, empty channel, missing size, zero size, and the
        // double-`=` shape (the second `=` lands in the size field, which
        // rejects it — the codec stays single-`=`).
        let messy = "channel/Body/height=256x256\n\
                     channel//height=128x128\n\
                     channel/Body/=128x128\n\
                     channel/Body/roughness\n\
                     channel/Body/metallic=0x128\n\
                     channel=Body/ao=64x64\n";
        let parsed = parse_paint_set_sizes(messy).expect("the one good line parses");
        assert_eq!(parsed.channels.len(), 1);
        assert_eq!(
            parsed
                .channels
                .get(&("Body".to_string(), "height".to_string())),
            Some(&(256, 256))
        );
        // A pre-channel file (no channel lines — the shape the tile-pool
        // kill writes) loads all-inherit: sizes and tiles parse, channels
        // stay empty.
        let old = "# Umber paint-set sizes v1\nBody=2048x2048\ntile-1001=Body\n";
        let parsed_old = parse_paint_set_sizes(old).expect("a pre-channel file parses");
        assert!(parsed_old.channels.is_empty());
        assert_eq!(parsed_old.sizes.get("Body"), Some(&(2048, 2048)));
    }

    /// Deterministic RGBA8 pattern: texel `(x, y)` channel `c` holds
    /// `(x*31 + y*17 + c*71 + seed) mod 251` (251 is prime, so the values
    /// spread; the seed tells two sets' paint apart).
    fn pattern_bytes(width: u32, height: u32, seed: u8) -> Vec<u8> {
        let mut out = Vec::with_capacity(width as usize * height as usize * 4);
        for y in 0..height {
            for x in 0..width {
                for c in 0..4u32 {
                    out.push(((x * 31 + y * 17 + c * 71 + u32::from(seed)) % 251) as u8);
                }
            }
        }
        out
    }

    #[test]
    fn two_sets_at_different_sizes_round_trip_byte_identical_per_set() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // Set A ("Body") at 2048, set B ("Cloak") at 4096 — the audit's
        // gate: two sets, two sizes, one session.
        let mut paint =
            paint_state::PaintState::new(device.clone(), queue.clone()).expect("paint builds");
        paint.set_active_set("Body");
        // Tile 1001 is the constructor's pre-assigned default-set tile
        // (tile_sets maps it to TextureSet at build); switching the ACTIVE
        // set never re-routes recorded tiles (the field docs: assignment
        // steers FUTURE ensure_tile calls). Re-assign it to Body so the
        // 2048 publish lands in a 2048 target.
        paint.assign_tile_to_set(1001, "Body");
        paint.resize_set("Body", 2048, 2048);
        paint.ensure_set("Cloak");
        assert_eq!(paint.size_of_set("Cloak"), (4096, 4096));
        paint.assign_tile_to_set(1002, "Cloak");

        let before_a = pattern_bytes(2048, 2048, 3);
        let before_b = pattern_bytes(4096, 4096, 77);
        // Can-fail guards against a vacuous pass: the patterns are
        // non-blank and distinct, and the (0,0) texels pin the formula
        // exactly (seed 3 → [3, 74, 145, 216]; seed 77 → [77, 148, 219, 39]).
        assert_eq!(&before_a[..4], &[3u8, 74, 145, 216]);
        assert_eq!(&before_b[..4], &[77u8, 148, 219, 39]);
        assert_ne!(before_a[..64], before_b[..64]);
        assert!(before_a.iter().any(|&b| b != 0));

        assert!(paint.publish_fill(1001, 2048, 2048, before_a.clone()));
        assert!(paint.publish_fill(1002, 4096, 4096, before_b.clone()));
        paint.process_pending().expect("uploads land");
        for (tile, bytes, (w, h)) in [
            (&1001u16, &before_a, (2048u32, 2048u32)),
            (&1002u16, &before_b, (4096u32, 4096u32)),
        ] {
            let image = paint.read_tile(*tile).expect("readback succeeds");
            assert_eq!((image.width, image.height), (w, h));
            assert_eq!(&image.data, bytes, "tile {tile} stages its set's bytes");
        }

        let dir = unique_temp_dir("perset-roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let saved = save_paint_tiles(&mut paint, &dir, &no_live_layers(), &[]);
        assert_eq!(saved, 2, "both sets' tiles persist");
        assert!(paint_tile_path(&dir, 1001).exists());
        assert!(paint_tile_path(&dir, 1002).exists());
        // The mapping is byte-exact (sorted sizes, then sorted tiles);
        // the legacy global still carries the active ("Body") size.
        let mapping_text = std::fs::read_to_string(paint_set_sizes_path(&dir)).unwrap();
        assert_eq!(
            mapping_text,
            "# Umber paint-set sizes v1\nBody=2048x2048\nCloak=4096x4096\ntile-1001=Body\ntile-1002=Cloak\n"
        );
        assert_eq!(
            std::fs::read_to_string(paint_target_size_path(&dir)).unwrap(),
            "2048x2048"
        );

        // Reopen in a fresh session (default set, 4K): each set's paint
        // restores byte-identical AT ITS size — the load path re-derives
        // sizes and assignments from the mapping, not from the live state.
        let mut fresh = paint_state::PaintState::new(device.clone(), queue.clone())
            .expect("fresh paint state builds");
        let report = load_paint_tiles_reported(&mut fresh, &dir, &no_live_layers());
        assert_eq!(
            report,
            PaintLoadReport {
                restored: 2,
                masks_restored: 0,
                skipped: Vec::new(),
                // The fixture paints Base (non-zero bytes): the §6.3
                // non-empty arm fires.
                base_nonempty: true,
            }
        );
        assert_eq!(fresh.size_of_set("Body"), (2048, 2048));
        assert_eq!(fresh.size_of_set("Cloak"), (4096, 4096));
        assert_eq!(fresh.set_of_tile(1001), "Body");
        assert_eq!(fresh.set_of_tile(1002), "Cloak");
        let after_a = fresh.read_tile(1001).expect("tile 1001 reads back");
        assert_eq!((after_a.width, after_a.height), (2048, 2048));
        assert_eq!(
            after_a.data, before_a,
            "set A restores byte-identical at 2048"
        );
        let after_b = fresh.read_tile(1002).expect("tile 1002 reads back");
        assert_eq!((after_b.width, after_b.height), (4096, 4096));
        assert_eq!(
            after_b.data, before_b,
            "set B restores byte-identical at 4096"
        );

        // A size-mismatched restore fails LOUDLY: clobber tile 1002's PNG
        // with 512² bytes (its set says 4096²) and reopen — tile 1001
        // still restores, 1002 is refused with its expected/found dims on
        // the record (never silently resampled).
        let wrong = pattern_bytes(512, 512, 9);
        umber_export::png::write_png(
            &paint_tile_path(&dir, 1002),
            512,
            512,
            &wrong,
            umber_export::png::Transfer::Linear,
        )
        .expect("clobber writes");
        let mut mismatch =
            paint_state::PaintState::new(device, queue).expect("mismatch session builds");
        let report = load_paint_tiles_reported(&mut mismatch, &dir, &no_live_layers());
        assert_eq!(report.restored, 1, "only the matching tile restores");
        assert_eq!(
            report.skipped,
            vec![PaintTileSkip {
                tile: 1002,
                channel: paint_state::BASE_COLOR_CHANNEL.to_string(),
                expected: (4096, 4096),
                found: (512, 512),
            }],
            "the mismatch is on the record with exact dims"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_channels_round_trip_byte_identical_per_channel() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // 512² keeps the test fast; the sizes must MATCH the target —
        // a 512 publish into a 4096 target would land a 512 region at
        // the origin, not the pattern — so the session is built at 512
        // via with_target_size (tile 1001 pre-exists at that size).
        let mut paint =
            paint_state::PaintState::with_target_size(device.clone(), queue.clone(), 512)
                .expect("paint builds");
        let before_base = pattern_bytes(512, 512, 3);
        let before_height = pattern_bytes(512, 512, 77);
        // Can-fail guards against a vacuous pass: the patterns are
        // non-blank and distinct, and the (0,0) texels pin the formula
        // exactly (seed 3 → [3, 74, 145, 216]; seed 77 → [77, 148, 219,
        // 39] — the same derivation as the two-set test).
        assert_eq!(&before_base[..4], &[3u8, 74, 145, 216]);
        assert_eq!(&before_height[..4], &[77u8, 148, 219, 39]);
        assert_ne!(before_base, before_height);
        assert!(before_base.iter().any(|&b| b != 0));
        assert!(paint.publish_fill(1001, 512, 512, before_base.clone()));
        assert!(paint.publish_fill_channel(1001, "height", 512, 512, before_height.clone()));
        paint.process_pending().expect("uploads land");
        let staged_base = paint.read_tile(1001).expect("baseColor stages");
        assert_eq!(staged_base.data, before_base);
        let staged_height = paint
            .read_channel_tile(1001, "height")
            .expect("height stages");
        assert_eq!(staged_height.data, before_height);

        let dir = unique_temp_dir("per-channel-roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let saved = save_paint_tiles(&mut paint, &dir, &no_live_layers(), &[]);
        assert_eq!(saved, 2, "baseColor + height persist as two artifacts");
        // baseColor keeps the v2 FLAT layout byte-for-byte
        // (paint/tile-1001.png — paint_tile_path, deliberately
        // unchanged by the per-channel pool; see its docs), while any
        // other channel nests one level under paint/<channel>/.
        assert!(paint_tile_path(&dir, 1001).exists());
        assert_eq!(
            paint_channel_tile_path(&dir, paint_state::BASE_COLOR_CHANNEL, 1001),
            paint_tile_path(&dir, 1001),
            "the reserved channel resolves to the flat v2 path"
        );
        let height_path = paint_channel_tile_path(&dir, "height", 1001);
        assert_eq!(
            height_path,
            paint_sidecar_dir(&dir).join("height").join("tile-1001.png")
        );
        assert!(height_path.exists());
        // The manifest names both artifacts — the channel rides in the
        // NAME field (paint_sidecar_name), not a new column — and the
        // commit applied (no pending manifest left behind).
        assert!(!paint_commit_pending_path(&dir).exists());
        let manifest = std::fs::read_to_string(paint_manifest_path(&dir)).unwrap();
        let entries = parse_paint_commit(&manifest).expect("the applied manifest parses");
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert!(
            names.contains(&"tile-1001.png"),
            "baseColor keeps its flat v2 name, got {names:?}"
        );
        assert!(
            names.contains(&"height/tile-1001.png"),
            "height nests under its channel dir, got {names:?}"
        );

        // Reopen in a fresh session (default set, 4K): both channels
        // restore byte-identical AT the saved size.
        let mut fresh =
            paint_state::PaintState::new(device.clone(), queue.clone()).expect("fresh builds");
        let report = load_paint_tiles_reported(&mut fresh, &dir, &no_live_layers());
        assert_eq!(
            report,
            PaintLoadReport {
                restored: 2,
                masks_restored: 0,
                skipped: Vec::new(),
                // The fixture paints Base (non-zero bytes): the §6.3
                // non-empty arm fires.
                base_nonempty: true,
            }
        );
        assert_eq!(fresh.channels_present(1001), vec!["height".to_string()]);
        let after_base = fresh.read_tile(1001).expect("baseColor reads back");
        assert_eq!((after_base.width, after_base.height), (512, 512));
        assert_eq!(
            after_base.data, before_base,
            "baseColor restores byte-identical"
        );
        let after_height = fresh
            .read_channel_tile(1001, "height")
            .expect("height reads back");
        assert_eq!((after_height.width, after_height.height), (512, 512));
        assert_eq!(
            after_height.data, before_height,
            "height restores byte-identical"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn channel_size_override_round_trips_and_mismatches_skip_per_channel() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // The default set at 512 with height overridden to 256: two sizes,
        // one session (the per-channel sizing gate).
        let mut paint =
            paint_state::PaintState::with_target_size(device.clone(), queue.clone(), 512)
                .expect("paint builds");
        paint.resize_channel(paint_state::DEFAULT_TEXTURE_SET, "height", 256, 256);
        let before_base = pattern_bytes(512, 512, 3);
        let before_height = pattern_bytes(256, 256, 77);
        assert_eq!(&before_base[..4], &[3u8, 74, 145, 216]);
        // Seed 77 pins (0,0) to [77, 148, 219, 39] — the same derivation
        // as the two-channel test (dims never enter it).
        assert_eq!(&before_height[..4], &[77u8, 148, 219, 39]);
        assert!(paint.publish_fill(1001, 512, 512, before_base.clone()));
        assert!(paint.publish_fill_channel(1001, "height", 256, 256, before_height.clone()));
        paint.process_pending().expect("uploads land");

        let dir = unique_temp_dir("per-channel-size-roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let saved = save_paint_tiles(&mut paint, &dir, &no_live_layers(), &[]);
        assert_eq!(saved, 2, "baseColor + height persist as two artifacts");
        // The mapping pins the override line exactly (sorted between the
        // size and the assignment — the codec test pins the grammar; this
        // pins the save path emitting it).
        let mapping_text = std::fs::read_to_string(paint_set_sizes_path(&dir)).unwrap();
        assert_eq!(
            mapping_text,
            "# Umber paint-set sizes v1\n\
             TextureSet=512x512\n\
             channel/TextureSet/height=256x256\n\
             tile-1001=TextureSet\n"
        );

        // Reopen fresh (4K default): the override restores BEFORE its
        // pixels land, so height materializes at 256 while baseColor
        // restores at 512 — both byte-identical.
        let mut fresh =
            paint_state::PaintState::new(device.clone(), queue.clone()).expect("fresh builds");
        let report = load_paint_tiles_reported(&mut fresh, &dir, &no_live_layers());
        assert_eq!(
            report,
            PaintLoadReport {
                restored: 2,
                masks_restored: 0,
                skipped: Vec::new(),
                // The fixture paints Base (non-zero bytes): the §6.3
                // non-empty arm fires.
                base_nonempty: true,
            }
        );
        assert_eq!(
            fresh.size_of_channel("TextureSet", "height"),
            (256, 256),
            "the override survives the round trip"
        );
        assert_eq!(
            fresh.size_of_channel("TextureSet", "roughness"),
            (512, 512),
            "no line means inherit the set's size"
        );
        let after_base = fresh.read_tile(1001).expect("baseColor reads back");
        assert_eq!((after_base.width, after_base.height), (512, 512));
        assert_eq!(
            after_base.data, before_base,
            "baseColor restores byte-identical"
        );
        let after_height = fresh
            .read_channel_tile(1001, "height")
            .expect("height reads back");
        assert_eq!((after_height.width, after_height.height), (256, 256));
        assert_eq!(
            after_height.data, before_height,
            "height restores byte-identical at ITS size"
        );

        // A size-mismatched CHANNEL refuses loudly per channel: clobber
        // height's PNG with 512² bytes (its override says 256²) and
        // reopen — baseColor still restores, height refuses with its
        // channel on the record (never silently resampled).
        let wrong = pattern_bytes(512, 512, 9);
        umber_export::png::write_png(
            &paint_channel_tile_path(&dir, "height", 1001),
            512,
            512,
            &wrong,
            umber_export::png::Transfer::Linear,
        )
        .expect("clobber writes");
        let mut mismatch =
            paint_state::PaintState::new(device, queue).expect("mismatch session builds");
        let report = load_paint_tiles_reported(&mut mismatch, &dir, &no_live_layers());
        assert_eq!(report.restored, 1, "only the matching channel restores");
        assert_eq!(
            report.skipped,
            vec![PaintTileSkip {
                tile: 1001,
                channel: "height".to_string(),
                expected: (256, 256),
                found: (512, 512),
            }],
            "the channel mismatch is on the record with exact dims"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn v2_flat_manifest_loads_all_base_color() {
        // The codec has NO channel column (verified against source):
        // parse_paint_commit destructures an exact 5-field array
        // `<name> <hash> <ext> <size> <temp>` and refuses any other
        // arity — a literal 6th column fails the whole manifest. The
        // channel instead rides in the NAME field
        // (paint_sidecar_name/parse_paint_sidecar_name): a bare
        // `tile-<N>.png` IS the reserved baseColor channel. A v2
        // project (saved before channels existed) therefore names only
        // flat tiles, and the load path restores every one through
        // publish_fill. Hand-written codec pins first:
        let zeros = "0".repeat(64);
        let flat_line = format!("tile-1001.png {zeros} png 123 tile-1001.png.tmp.42");
        let flat = parse_paint_commit(&format!("# Umber paint commit v1\n{flat_line}\n"))
            .expect("a v2-shaped 5-field flat line parses");
        assert_eq!(flat.len(), 1);
        assert_eq!(flat[0].name, "tile-1001.png");
        // A 6-field line (the "add a channel column" shape the design
        // doc's draft text sketches) refuses the WHOLE manifest —
        // which is why the landed codec encodes the channel in the
        // name instead (see paint_sidecar_name's docs).
        let six_field = format!("tile-1001.png baseColor {zeros} png 123 tile-1001.png.tmp.42");
        assert_eq!(
            parse_paint_commit(&format!("# Umber paint commit v1\n{six_field}\n")),
            None,
            "a 6-field line must refuse"
        );
        // The name codec itself, exact (not contains-checks): flat is
        // baseColor, one level nests, anything else rejects.
        assert_eq!(
            parse_paint_sidecar_name("tile-1001.png"),
            Some((
                paint_state::PlaneOwner::Base,
                paint_state::BASE_COLOR_CHANNEL.to_string(),
                1001
            ))
        );
        assert_eq!(
            parse_paint_sidecar_name("height/tile-1001.png"),
            Some((paint_state::PlaneOwner::Base, "height".to_string(), 1001))
        );
        // A literal paint/baseColor/ subdirectory parses as the
        // reserved channel too — and wins over a same-tile flat file
        // on load (collect_paint_tiles inserts the flat pass first;
        // the channel pass overwrites). Pinned so a future rename of
        // the reserved name fails loudly instead of forking the pool.
        assert_eq!(
            parse_paint_sidecar_name("baseColor/tile-1001.png"),
            Some((
                paint_state::PlaneOwner::Base,
                paint_state::BASE_COLOR_CHANNEL.to_string(),
                1001
            ))
        );
        for bad in [
            "",
            "tile-1001.png/",
            "/tile-1001.png",
            "../tile-1001.png",
            "height\\tile-1001.png",
            "a\\b/tile-1001.png",
            "height/tile-abc.png",
            "height/tile-1001.exr",
            "tile-1001.exr",
        ] {
            assert_eq!(parse_paint_sidecar_name(bad), None, "{bad:?} must reject");
        }

        // End to end: a baseColor-only session saved through the REAL
        // save path produces exactly the v2 shape (flat names only, no
        // channel directory), and reopens all-baseColor with no
        // channel materialized.
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut paint =
            paint_state::PaintState::with_target_size(device.clone(), queue.clone(), 512)
                .expect("paint builds");
        let before = pattern_bytes(512, 512, 11);
        assert_eq!(&before[..4], &[11u8, 82, 153, 224]);
        assert!(paint.publish_fill(1001, 512, 512, before.clone()));
        paint.process_pending().expect("upload lands");

        let dir = unique_temp_dir("channel-migration");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            save_paint_tiles(&mut paint, &dir, &no_live_layers(), &[]),
            1
        );
        assert!(
            !paint_sidecar_dir(&dir).join("height").exists(),
            "no channel was routed, so no channel directory is written"
        );
        let manifest = std::fs::read_to_string(paint_manifest_path(&dir)).unwrap();
        let entries = parse_paint_commit(&manifest).expect("the applied manifest parses");
        for entry in &entries {
            if entry.name == "target_size.txt" || entry.name == "target_sizes.txt" {
                continue;
            }
            assert_eq!(
                parse_paint_sidecar_name(&entry.name),
                Some((
                    paint_state::PlaneOwner::Base,
                    paint_state::BASE_COLOR_CHANNEL.to_string(),
                    1001
                )),
                "every tile entry in a channel-less save is flat baseColor"
            );
        }

        let mut fresh = paint_state::PaintState::new(device, queue).expect("fresh session builds");
        let report = load_paint_tiles_reported(&mut fresh, &dir, &no_live_layers());
        assert_eq!(
            report,
            PaintLoadReport {
                restored: 1,
                masks_restored: 0,
                skipped: Vec::new(),
                // The fixture paints Base (non-zero bytes): the §6.3
                // non-empty arm fires.
                base_nonempty: true,
            }
        );
        assert!(
            fresh.channels_present(1001).is_empty(),
            "the migration must not materialize any channel"
        );
        let after = fresh.read_tile(1001).expect("readback succeeds");
        assert_eq!((after.width, after.height), (512, 512));
        assert_eq!(
            after.data, before,
            "v2 pixels restore byte-identical as baseColor"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn v1_project_without_mapping_migrates_its_global_to_every_set() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // A v1-shaped project: 512² tile PNGs + the global, NO mapping.
        // Saved through the real save path, then the mapping deleted (a v1
        // build never wrote one). Note v1 stored no set NAMES — only the
        // global size migrates; tiles join the opening session's active set.
        let mut paint =
            paint_state::PaintState::with_target_size(device.clone(), queue.clone(), 512)
                .expect("paint builds");
        paint.set_active_set("Body");
        // 1001 is pre-assigned to the default set at build; the global
        // sidecar records the ACTIVE set's size, so route the tile to
        // Body (512) before saving — else the global reads the default
        // set's 4096 and the v1 round-trip premise breaks. The resize
        // REBUILDS the pre-existing 1001 target at 512 (assignment alone
        // steers future creations only — the field docs).
        paint.assign_tile_to_set(1001, "Body");
        paint.resize_set("Body", 512, 512);
        let before = pattern_bytes(512, 512, 11);
        assert_eq!(&before[..4], &[11u8, 82, 153, 224]);
        assert!(paint.publish_fill(1001, 512, 512, before.clone()));
        paint.process_pending().expect("upload lands");

        let dir = unique_temp_dir("legacy-migration");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            save_paint_tiles(&mut paint, &dir, &no_live_layers(), &[]),
            1
        );
        std::fs::remove_file(paint_set_sizes_path(&dir)).expect("a v1 project has no mapping");
        assert_eq!(
            std::fs::read_to_string(paint_target_size_path(&dir)).unwrap(),
            "512x512"
        );

        // A fresh 4K session opens it: the 512 global leads (first tile's
        // dims, exactly the v1 rule), and the pixels restore byte-identical.
        let mut fresh =
            paint_state::PaintState::new(device.clone(), queue.clone()).expect("fresh builds");
        let report = load_paint_tiles_reported(&mut fresh, &dir, &no_live_layers());
        assert_eq!(
            report,
            PaintLoadReport {
                restored: 1,
                masks_restored: 0,
                skipped: Vec::new(),
                // The fixture paints Base (non-zero bytes): the §6.3
                // non-empty arm fires.
                base_nonempty: true,
            }
        );
        assert_eq!(fresh.target_size(), (512, 512));
        assert_eq!(
            fresh.set_of_tile(1001),
            fresh.active_set(),
            "v1 tiles join the opening session's active set (v1 stored no names)"
        );
        let after = fresh.read_tile(1001).expect("readback succeeds");
        assert_eq!((after.width, after.height), (512, 512));
        assert_eq!(
            after.data, before,
            "v1 pixels restore byte-identical under the migrated size"
        );

        // Tiles-less and mapping-less: the legacy global still applies to
        // every set (a size choice with no paint yet survives the round
        // trip) while the pool clears.
        std::fs::remove_file(paint_tile_path(&dir, 1001)).unwrap();
        let mut empty = paint_state::PaintState::new(device, queue).expect("empty session builds");
        let report = load_paint_tiles_reported(&mut empty, &dir, &no_live_layers());
        assert_eq!(
            report,
            PaintLoadReport {
                restored: 0,
                masks_restored: 0,
                skipped: Vec::new(),
                // The fixture's Base is empty (fully transparent): the
                // dormancy decision's non-empty arm stays off.
                base_nonempty: false,
            }
        );
        assert_eq!(
            empty.target_size(),
            (512, 512),
            "the stored global sizes the session with no paint yet"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The stage-2 orphan filter for sessions WITH live layers.
    fn live_layers(ids: &[u64]) -> std::collections::HashSet<u64> {
        ids.iter().copied().collect()
    }

    /// Stage-2 §8 T15: the §6.1 parse rule, exactly as the doc derives it
    /// (no GPU needed — pure codec).
    #[test]
    fn layer_plane_sidecar_names_follow_the_parse_rule() {
        let base = paint_state::PlaneOwner::Base;
        let base_color = paint_state::BASE_COLOR_CHANNEL.to_string();
        assert_eq!(
            parse_paint_sidecar_name("layers/7/baseColor/tile-1001.png"),
            Some((
                paint_state::PlaneOwner::Layer(7),
                "baseColor".to_string(),
                1001
            ))
        );
        // Depth disambiguates a channel literally named `layers`.
        assert_eq!(
            parse_paint_sidecar_name("layers/tile-1001.png"),
            Some((base, "layers".to_string(), 1001))
        );
        for bad in [
            "layers/7/tile-1001.png",
            "layers/x/baseColor/tile-1001.png",
            "layers/7/../tile-1001.png",
            "layers/7/a\\b/tile-1001.png",
            "layers//baseColor/tile-1001.png",
            "layers/7//tile-1001.png",
            "layers/7/baseColor/tile-1001.png/",
            "layers/7/baseColor/tile-abc.png",
        ] {
            assert_eq!(parse_paint_sidecar_name(bad), None, "{bad:?} must reject");
        }
        // The name builder round-trips through the parser exactly (not a
        // contains-check — a renamed nesting would fail).
        for (owner, channel, tile) in [
            (base, base_color.clone(), 1001),
            (base, "height".to_string(), 1002),
            (
                paint_state::PlaneOwner::Layer(7),
                "baseColor".to_string(),
                1001,
            ),
            (
                paint_state::PlaneOwner::Layer(12),
                "normal".to_string(),
                1002,
            ),
        ] {
            let name = paint_plane_sidecar_name(owner, &channel, tile);
            assert_eq!(
                parse_paint_sidecar_name(&name),
                Some((owner, channel, tile)),
                "{name:?} must parse back to its plane"
            );
        }
        assert_eq!(
            paint_plane_sidecar_name(paint_state::PlaneOwner::Layer(7), "baseColor", 1001),
            "layers/7/baseColor/tile-1001.png"
        );
        assert_eq!(
            paint_plane_path(
                Path::new("/p"),
                paint_state::PlaneOwner::Layer(7),
                "baseColor",
                1001
            ),
            PathBuf::from("/p/paint/layers/7/baseColor/tile-1001.png")
        );
        assert_eq!(
            paint_plane_path(Path::new("/p"), base, "baseColor", 1001),
            paint_tile_path(Path::new("/p"), 1001),
            "Base keeps the legacy flat path byte-for-byte"
        );
        // The manifest gate admits layer names (else the commit would
        // refuse its own entries) and still refuses the 3-component shape.
        assert!(is_paint_sidecar_name("layers/7/baseColor/tile-1001.png"));
        assert!(!is_paint_sidecar_name("layers/7/tile-1001.png"));
    }

    /// Stage-2 §8 T16: a v2 save → load gives byte-identical planes per
    /// owner; orphan planes (no live layer) are ignored with a log.
    #[test]
    fn layer_planes_round_trip_per_owner_with_orphans_ignored() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut paint =
            paint_state::PaintState::with_target_size(device.clone(), queue.clone(), 512)
                .expect("paint builds");
        let before_base = pattern_bytes(512, 512, 3);
        let before_layer = pattern_bytes(512, 512, 77);
        // Can-fail guards against a vacuous pass (the seeds pin (0,0) to
        // [3, 74, 145, 216] and [77, 148, 219, 39]).
        assert_eq!(&before_base[..4], &[3u8, 74, 145, 216]);
        assert_eq!(&before_layer[..4], &[77u8, 148, 219, 39]);
        assert_ne!(before_base, before_layer);
        assert!(paint
            .publish_plane(
                paint_state::PlaneOwner::Base,
                1001,
                "baseColor",
                512,
                512,
                before_base.clone()
            )
            .is_ok());
        assert!(paint
            .publish_plane(
                paint_state::PlaneOwner::Layer(7),
                1001,
                "baseColor",
                512,
                512,
                before_layer.clone()
            )
            .is_ok());
        paint.process_pending().expect("uploads land");

        let dir = unique_temp_dir("plane-roundtrip");
        std::fs::create_dir_all(&dir).unwrap();
        let live = live_layers(&[7]);
        assert_eq!(
            save_paint_tiles(&mut paint, &dir, &live, &[]),
            2,
            "Base + one layer plane persist as two artifacts"
        );
        // The manifest header is v2 from stage 4 on (§6.1 [B2, M11] —
        // every save, regardless of `project.json`'s version).
        let manifest = std::fs::read_to_string(paint_manifest_path(&dir)).unwrap();
        assert_eq!(
            manifest.lines().next(),
            Some("# Umber paint commit v2"),
            "stage 4 writes the v2 header on every save"
        );
        let entries = parse_paint_commit(&manifest).expect("the applied manifest parses");
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert!(
            names.contains(&"tile-1001.png"),
            "Base keeps its flat v2 name, got {names:?}"
        );
        assert!(
            names.contains(&"layers/7/baseColor/tile-1001.png"),
            "the layer plane nests under layers/<id>/, got {names:?}"
        );
        assert!(paint_sidecar_dir(&dir)
            .join("layers/7/baseColor/tile-1001.png")
            .exists());
        assert!(
            !dir.join("layers").exists(),
            "planes never land under layers/<set>/ (the layer-JSON tree)"
        );

        // Open with the layer live: both planes restore byte-identical.
        let mut fresh =
            paint_state::PaintState::new(device.clone(), queue.clone()).expect("fresh builds");
        let report = load_paint_tiles_reported(&mut fresh, &dir, &live);
        assert_eq!(
            report,
            PaintLoadReport {
                restored: 2,
                masks_restored: 0,
                skipped: Vec::new(),
                // The fixture paints Base (non-zero bytes): the §6.3
                // non-empty arm fires.
                base_nonempty: true,
            }
        );
        let after_base = fresh
            .read_plane(paint_state::PlaneOwner::Base, 1001, "baseColor")
            .expect("Base restores");
        assert_eq!(after_base.data, before_base);
        let after_layer = fresh
            .read_plane(paint_state::PlaneOwner::Layer(7), 1001, "baseColor")
            .expect("the layer plane restores");
        assert_eq!(after_layer.data, before_layer);

        // Open with NO live layer: the layer plane is an orphan — ignored
        // (never enters the session), Base still restores.
        let mut orphaned =
            paint_state::PaintState::new(device, queue).expect("orphan session builds");
        let report = load_paint_tiles_reported(&mut orphaned, &dir, &no_live_layers());
        assert_eq!(report.restored, 1, "only Base restores for an orphan plane");
        assert!(
            orphaned
                .read_plane(paint_state::PlaneOwner::Layer(7), 1001, "baseColor")
                .is_none(),
            "the orphan plane never enters the session"
        );
        let orphan_base = orphaned
            .read_plane(paint_state::PlaneOwner::Base, 1001, "baseColor")
            .expect("Base restores");
        assert_eq!(orphan_base.data, before_base);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Stage-2 §8 T20 [B1]: save-as into a new dir keeps every plane; an
    /// in-place save right after open reads back NOTHING; a corrupted
    /// on-disk sidecar reads back (hash mismatch) instead of re-staging.
    #[test]
    fn save_as_into_new_dir_keeps_every_plane() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut paint =
            paint_state::PaintState::with_target_size(device.clone(), queue.clone(), 512)
                .expect("paint builds");
        let base_bytes = pattern_bytes(512, 512, 3);
        let l1_bytes = pattern_bytes(512, 512, 77);
        let l2_bytes = pattern_bytes(512, 512, 11);
        assert_ne!(l1_bytes, l2_bytes);
        for (owner, bytes) in [
            (paint_state::PlaneOwner::Base, base_bytes.clone()),
            (paint_state::PlaneOwner::Layer(1), l1_bytes.clone()),
            (paint_state::PlaneOwner::Layer(2), l2_bytes.clone()),
        ] {
            assert!(paint
                .publish_plane(owner, 1001, "baseColor", 512, 512, bytes)
                .is_ok());
        }
        paint.process_pending().expect("uploads land");
        let live = live_layers(&[1, 2]);

        let dir_a = unique_temp_dir("saveas-a");
        std::fs::create_dir_all(&dir_a).unwrap();
        assert_eq!(save_paint_tiles(&mut paint, &dir_a, &live, &[]), 3);

        // Corrupt L2's sidecar on disk, then save in place: L2 reads back
        // (hash mismatch), Base + L1 skip (same dir, same bytes).
        let l2_path =
            paint_plane_path(&dir_a, paint_state::PlaneOwner::Layer(2), "baseColor", 1001);
        umber_export::png::write_png(
            &l2_path,
            512,
            512,
            &pattern_bytes(512, 512, 5),
            umber_export::png::Transfer::Linear,
        )
        .expect("clobber writes");
        let report =
            commit_paint_tiles(&mut paint, &dir_a, &live, &[]).expect("in-place save commits");
        assert_eq!(
            report,
            PaintSaveReport {
                staged: 3,
                read_back: 1
            },
            "only the corrupted plane reads back"
        );
        let (width, height, repaired) =
            decode_paint_png(&l2_path).expect("the re-saved L2 decodes");
        assert_eq!((width, height), (512, 512));
        assert_eq!(
            repaired, l2_bytes,
            "a hash mismatch reads back; the corrupt file never re-stages"
        );

        // In-place save right after that commit reads back NOTHING.
        let report = commit_paint_tiles(&mut paint, &dir_a, &live, &[]).expect("re-save commits");
        assert_eq!(
            report,
            PaintSaveReport {
                staged: 3,
                read_back: 0
            },
            "an in-place save of clean planes stages without a readback"
        );

        // Change L1 only, then save-as into a NEW dir: every plane reads
        // back (a save-as shares no committed dir), and B reopens with
        // every plane byte-identical (the r1 sentinel dropped them).
        let l1_new = pattern_bytes(512, 512, 99);
        assert_ne!(l1_new, l1_bytes);
        assert!(paint
            .publish_plane(
                paint_state::PlaneOwner::Layer(1),
                1001,
                "baseColor",
                512,
                512,
                l1_new.clone()
            )
            .is_ok());
        paint.process_pending().expect("the L1 change lands");
        let dir_b = unique_temp_dir("saveas-b");
        std::fs::create_dir_all(&dir_b).unwrap();
        let report = commit_paint_tiles(&mut paint, &dir_b, &live, &[]).expect("save-as commits");
        assert_eq!(
            report,
            PaintSaveReport {
                staged: 3,
                read_back: 3
            },
            "a save-as shares no committed dir, so every plane reads back"
        );
        let manifest = std::fs::read_to_string(paint_manifest_path(&dir_b)).unwrap();
        let entries = parse_paint_commit(&manifest).expect("B's manifest parses");
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        for want in [
            "tile-1001.png",
            "layers/1/baseColor/tile-1001.png",
            "layers/2/baseColor/tile-1001.png",
        ] {
            assert!(
                names.contains(&want),
                "B's manifest lists {want}, got {names:?}"
            );
        }

        let mut fresh = paint_state::PaintState::new(device, queue).expect("fresh builds");
        let report = load_paint_tiles_reported(&mut fresh, &dir_b, &live);
        assert_eq!(report.restored, 3);
        let reopened_base = fresh
            .read_plane(paint_state::PlaneOwner::Base, 1001, "baseColor")
            .expect("Base reopens");
        assert_eq!(reopened_base.data, base_bytes);
        let reopened_l1 = fresh
            .read_plane(paint_state::PlaneOwner::Layer(1), 1001, "baseColor")
            .expect("L1 reopens");
        assert_eq!(reopened_l1.data, l1_new);
        let reopened_l2 = fresh
            .read_plane(paint_state::PlaneOwner::Layer(2), 1001, "baseColor")
            .expect("L2 reopens");
        assert_eq!(reopened_l2.data, l2_bytes);

        let _ = std::fs::remove_dir_all(&dir_a);
        let _ = std::fs::remove_dir_all(&dir_b);
    }

    /// Stage-2 §8 T21 [B3]: the save loop reads Base via `read_plane`,
    /// never the composite — `paint/tile-1001.png` decodes to Base's plane
    /// exactly, and a reopen shows no double-count.
    #[test]
    fn save_writes_base_not_the_cache() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut paint =
            paint_state::PaintState::with_target_size(device.clone(), queue.clone(), 512)
                .expect("paint builds");
        let base_bytes = pattern_bytes(512, 512, 3);
        let layer_bytes = pattern_bytes(512, 512, 77);
        assert_eq!(&base_bytes[..4], &[3u8, 74, 145, 216]);
        assert_eq!(&layer_bytes[..4], &[77u8, 148, 219, 39]);
        assert!(paint
            .publish_plane(
                paint_state::PlaneOwner::Base,
                1001,
                "baseColor",
                512,
                512,
                base_bytes.clone()
            )
            .is_ok());
        assert!(paint
            .publish_plane(
                paint_state::PlaneOwner::Layer(1),
                1001,
                "baseColor",
                512,
                512,
                layer_bytes.clone()
            )
            .is_ok());
        paint.process_pending().expect("uploads land");
        let live = live_layers(&[1]);

        let dir = unique_temp_dir("base-not-cache");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(save_paint_tiles(&mut paint, &dir, &live, &[]), 2);

        let (width, height, file_bytes) =
            decode_paint_png(&paint_tile_path(&dir, 1001)).expect("the flat tile decodes");
        assert_eq!((width, height), (512, 512));
        assert_eq!(
            file_bytes, base_bytes,
            "the flat path carries Base's plane, never the composite"
        );
        assert_ne!(
            file_bytes, layer_bytes,
            "the layer's bytes must not leak into the flat path"
        );

        // Reopen: Base is still exactly Base (no layer folded in) and the
        // layer plane is intact — no per-cycle copy.
        let mut fresh = paint_state::PaintState::new(device, queue).expect("fresh builds");
        let report = load_paint_tiles_reported(&mut fresh, &dir, &live);
        assert_eq!(report.restored, 2);
        let reopened_base = fresh
            .read_plane(paint_state::PlaneOwner::Base, 1001, "baseColor")
            .expect("Base reopens");
        assert_eq!(
            reopened_base.data, base_bytes,
            "reopening must not composite the layer into Base"
        );
        let reopened_layer = fresh
            .read_plane(paint_state::PlaneOwner::Layer(1), 1001, "baseColor")
            .expect("the layer reopens");
        assert_eq!(reopened_layer.data, layer_bytes);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Stage-2 §8 T17 (+ [m10]): a save → save cycle keeps every
    /// `paint/layers/…` file (the `prune_stale_layer_files` guard — planes
    /// never live under `layers/<set>/`), and deleting a layer then saving
    /// leaves no `paint/layers/<id>/` directory behind, empty or not.
    #[test]
    fn layer_plane_directories_prune_with_their_layer() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut paint =
            paint_state::PaintState::with_target_size(device.clone(), queue.clone(), 512)
                .expect("paint builds");
        for (owner, seed) in [
            (paint_state::PlaneOwner::Base, 3u8),
            (paint_state::PlaneOwner::Layer(7), 77u8),
            (paint_state::PlaneOwner::Layer(9), 11u8),
        ] {
            assert!(paint
                .publish_plane(
                    owner,
                    1001,
                    "baseColor",
                    512,
                    512,
                    pattern_bytes(512, 512, seed)
                )
                .is_ok());
        }
        paint.process_pending().expect("uploads land");

        let dir = unique_temp_dir("layer-dirs");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            save_paint_tiles(&mut paint, &dir, &live_layers(&[7, 9]), &[]),
            3
        );
        let l7 = paint_sidecar_dir(&dir).join("layers/7/baseColor/tile-1001.png");
        let l9 = paint_sidecar_dir(&dir).join("layers/9/baseColor/tile-1001.png");
        assert!(l7.exists());
        assert!(l9.exists());
        let l9_bytes = std::fs::read(&l9).unwrap();

        // Save → save keeps every paint/layers file byte-identical.
        assert_eq!(
            save_paint_tiles(&mut paint, &dir, &live_layers(&[7, 9]), &[]),
            3
        );
        assert!(l7.exists());
        assert_eq!(
            std::fs::read(&l9).unwrap(),
            l9_bytes,
            "a re-save keeps the surviving layer's bytes"
        );

        // Delete layer 7 (its planes go orphan), then save: no
        // paint/layers/7 directory survives, and layer 9's does.
        assert_eq!(
            save_paint_tiles(&mut paint, &dir, &live_layers(&[9]), &[]),
            2,
            "the orphan plane never stages"
        );
        assert!(
            !paint_sidecar_dir(&dir).join("layers/7").exists(),
            "no layers/7 directory survives, empty or not"
        );
        assert_eq!(
            std::fs::read(&l9).unwrap(),
            l9_bytes,
            "every other layer's directory survives"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Stage-2 §8 T24 (app half): the minimal bump — version 2 iff a LIVE
    /// layer plane exists; orphans don't count. The other half (a loaded
    /// version-2 project re-saved layer-less writes 1) follows from the
    /// same helper `save_project` calls every save: a loaded model is
    /// never re-stamped. `ProjectModel::new` (every headless/CLI session's
    /// path) stays version 1.
    #[test]
    fn save_version_is_minimal_and_new_model_stamps_1() {
        assert_eq!(umber_core::CURRENT_PROJECT_VERSION, 1);
        assert_eq!(
            umber_core::ProjectModel::new(vec![], vec![], umber_core::ProjectSettings::default())
                .version,
            1,
            "the default write version never moves"
        );

        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut paint =
            paint_state::PaintState::with_target_size(device, queue, 512).expect("paint builds");
        // No planes at all (beyond Base): version 1.
        assert_eq!(paint_save_version(&paint, &no_live_layers()), 1);
        assert!(paint
            .publish_plane(
                paint_state::PlaneOwner::Base,
                1001,
                "baseColor",
                512,
                512,
                pattern_bytes(512, 512, 3)
            )
            .is_ok());
        assert!(paint
            .publish_plane(
                paint_state::PlaneOwner::Layer(7),
                1001,
                "baseColor",
                512,
                512,
                pattern_bytes(512, 512, 77)
            )
            .is_ok());
        paint.process_pending().expect("uploads land");
        // A layer plane with no live layer is an orphan: still 1.
        assert_eq!(
            paint_save_version(&paint, &no_live_layers()),
            1,
            "orphans don't count toward the version decision"
        );
        assert_eq!(
            paint_save_version(&paint, &live_layers(&[9])),
            1,
            "a different live set leaves layer 7 an orphan"
        );
        assert_eq!(
            paint_save_version(&paint, &live_layers(&[7])),
            2,
            "one live layer plane writes version 2"
        );
    }

    /// Stage-2 §8 T30 (paint half): the session lifecycle clear drops
    /// every layer-owned plane (and nothing else), so the next save
    /// writes version 1. (The `fill_states` half lives beside
    /// `reset_for_new`'s own tests.)
    #[test]
    fn clear_layer_planes_drops_layer_planes_not_base() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        let mut paint =
            paint_state::PaintState::with_target_size(device, queue, 512).expect("paint builds");
        let base_bytes = pattern_bytes(512, 512, 3);
        assert!(paint
            .publish_plane(
                paint_state::PlaneOwner::Base,
                1001,
                "baseColor",
                512,
                512,
                base_bytes.clone()
            )
            .is_ok());
        assert!(paint
            .publish_plane(
                paint_state::PlaneOwner::Layer(3),
                1001,
                "baseColor",
                512,
                512,
                pattern_bytes(512, 512, 77)
            )
            .is_ok());
        paint.process_pending().expect("uploads land");
        assert!(
            paint
                .read_plane(paint_state::PlaneOwner::Layer(3), 1001, "baseColor")
                .is_some(),
            "the layer plane exists before the clear (else the test is vacuous)"
        );

        paint.clear_layer_planes();

        assert!(
            paint
                .read_plane(paint_state::PlaneOwner::Layer(3), 1001, "baseColor")
                .is_none(),
            "no (Layer(3), …) plane survives the lifecycle clear"
        );
        let base = paint
            .read_plane(paint_state::PlaneOwner::Base, 1001, "baseColor")
            .expect("Base survives the layer clear");
        assert_eq!(base.data, base_bytes);
        assert_eq!(
            paint_save_version(&paint, &no_live_layers()),
            1,
            "a cleared session saves version 1"
        );
    }

    /// The paint-relevant files under `paint/` (sidecar names only — the
    /// provenance manifest and temps are not what a reopen reads).
    fn sidecar_snapshot(project: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
        let mut out = std::collections::BTreeMap::new();
        if let Ok(entries) = std::fs::read_dir(paint_sidecar_dir(project)) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if is_paint_sidecar_name(&name) {
                    out.insert(name, std::fs::read(entry.path()).unwrap());
                }
            }
        }
        out
    }

    fn snapshot_of(files: &[(&str, &[u8])]) -> std::collections::BTreeMap<String, Vec<u8>> {
        files
            .iter()
            .map(|(name, bytes)| (name.to_string(), bytes.to_vec()))
            .collect()
    }

    /// Leftovers a reopen must never leave: staged/restore temps and an
    /// unapplied manifest.
    fn commit_leftovers(project: &Path) -> Vec<String> {
        let mut out: Vec<String> = std::fs::read_dir(paint_sidecar_dir(project))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| is_paint_temp_name(name) || name == "manifest.pending")
            .collect();
        out.sort();
        out
    }

    /// The save sequence `commit_paint_tiles` runs (stage each file →
    /// commit the manifest → apply), driven from bytes so no GPU is
    /// needed, with ONE step budget spanning all three phases: the
    /// `budget`-th step (0-based) "crashes". A crashing stage leaves a
    /// half-written temp; a crashing commit a half-written manifest temp;
    /// apply's own faults come from its `max_steps` seam.
    fn save_files_faulted(
        project: &Path,
        files: &[(&str, &[u8])],
        budget: usize,
    ) -> std::io::Result<()> {
        let dir = paint_sidecar_dir(project);
        std::fs::create_dir_all(&dir)?;
        let store = umber_core::AssetStore::new(project);
        let mut used = 0usize;
        let mut entries = Vec::new();
        for (name, bytes) in files {
            if used == budget {
                std::fs::write(dir.join(paint_temp_name(name)), &bytes[..bytes.len() / 2])?;
                return Err(std::io::Error::other("injected fault"));
            }
            used += 1;
            entries.push(stage_paint_bytes(project, &store, name, bytes)?);
        }
        if used == budget {
            let text = format_paint_commit(&entries, &[]);
            std::fs::write(
                dir.join(paint_temp_name("manifest.pending")),
                &text.as_bytes()[..text.len() / 2],
            )?;
            return Err(std::io::Error::other("injected fault"));
        }
        used += 1;
        commit_paint_manifest(project, &entries, &[])?;
        apply_paint_commit(project, Some(budget - used)).map(|_| ())
    }

    const OLD_FILES: [(&str, &[u8]); 4] = [
        ("tile-1001.png", b"old tile 1001 bytes"),
        ("tile-1002.png", b"old tile 1002 bytes"),
        ("target_size.txt", b"512x512"),
        (
            "target_sizes.txt",
            b"# Umber paint-set sizes v1\nBody=512x512\ntile-1001=Body\ntile-1002=Body\n",
        ),
    ];

    /// The NEW save differs in every way a torn mix could show: tile 1001
    /// repainted, tile 1002 dropped (pruned), tile 1003 added, both size
    /// files changed.
    const NEW_FILES: [(&str, &[u8]); 4] = [
        ("tile-1001.png", b"NEW tile 1001 bytes, repainted"),
        ("tile-1003.png", b"NEW tile 1003 bytes"),
        ("target_size.txt", b"1024x1024"),
        (
            "target_sizes.txt",
            b"# Umber paint-set sizes v1\nBody=1024x1024\ntile-1001=Body\ntile-1003=Body\n",
        ),
    ];

    #[test]
    fn paint_commit_codec_round_trips_and_rejects_tampering() {
        let asset = umber_core::AssetRef {
            hash: "ab".repeat(32),
            ext: "png".to_string(),
            size: 42,
        };
        let entries = vec![PaintCommitEntry {
            name: "tile-1001.png".to_string(),
            temp: "tile-1001.png.tmp.77".to_string(),
            asset: asset.clone(),
        }];
        let text = format_paint_commit(&entries, &[]);
        let hash = "ab".repeat(32);
        let line = format!("tile-1001.png {hash} png 42 tile-1001.png.tmp.77\n");
        assert_eq!(text, format!("# Umber paint commit v2\n{line}"));
        assert_eq!(parse_paint_commit(&text), Some(entries));
        // Any bad line refuses the WHOLE manifest (never half-applied):
        // an escaping name, a temp for another name, a non-temp, a short
        // hash, a non-numeric size, a missing field.
        for bad in [
            format!("../x.png {hash} png 42 ../x.png.tmp.77\n"),
            format!("tile-1001.png {hash} png 42 tile-1002.png.tmp.77\n"),
            format!("tile-1001.png {hash} png 42 tile-1001.png\n"),
            "tile-1001.png abc png 42 tile-1001.png.tmp.77\n".to_string(),
            format!("tile-1001.png {hash} png x tile-1001.png.tmp.77\n"),
            format!("tile-1001.png {hash} png 42\n"),
        ] {
            assert_eq!(parse_paint_commit(&bad), None, "{bad:?} must refuse");
        }
        assert!(is_paint_temp_name("tile-1001.png.tmp.123"));
        assert!(is_paint_temp_name("tile-1001.png.restore.tmp.9"));
        assert!(!is_paint_temp_name("tile-1001.png"));
        assert!(!is_paint_temp_name("tile-1001.png.tmp."));
        assert!(!is_paint_temp_name("manifest.txt"));
    }

    #[test]
    fn asset_blob_path_matches_the_store_layout() {
        // The fsync before the commit targets this mirrored path; pin it
        // against a real store so a layout change fails here, not at a
        // power cut.
        let project = unique_temp_dir("blob-path");
        std::fs::create_dir_all(&project).unwrap();
        let asset = umber_core::AssetStore::new(&project)
            .store(b"blob path probe", "png")
            .unwrap();
        let path = paint_asset_blob_path(&project, &asset);
        assert_eq!(std::fs::read(&path).unwrap(), b"blob path probe");
        let _ = std::fs::remove_dir_all(&project);
    }

    #[test]
    fn interrupted_paint_save_reopens_old_or_new_never_torn() {
        let old = snapshot_of(&OLD_FILES);
        let new = snapshot_of(&NEW_FILES);
        // The old project in both shapes a save can meet: one an earlier
        // atomic save committed, and a legacy (pre-commit) direct write.
        for legacy_old in [false, true] {
            let mut saw_torn_disk = false;
            let mut budget = 0usize;
            loop {
                let project = unique_temp_dir("torn");
                if legacy_old {
                    for (name, bytes) in OLD_FILES {
                        std::fs::create_dir_all(paint_sidecar_dir(&project)).unwrap();
                        std::fs::write(paint_sidecar_dir(&project).join(name), bytes).unwrap();
                    }
                } else {
                    save_files_faulted(&project, &OLD_FILES, usize::MAX).unwrap();
                }
                assert_eq!(sidecar_snapshot(&project), old);

                let completed = save_files_faulted(&project, &NEW_FILES, budget).is_ok();
                let on_disk = sidecar_snapshot(&project);
                if on_disk != old && on_disk != new {
                    saw_torn_disk = true;
                }
                // "Reopen": the load path's recovery, then what it reads.
                recover_paint_sidecar(&project);
                let reopened = sidecar_snapshot(&project);
                // Exact pin, not just old-or-new: 4 stages + the commit
                // are steps 0..=4, so a crash at budget ≤ 4 predates the
                // commit (OLD); any later crash rolls forward (NEW).
                let committed = budget > NEW_FILES.len();
                let expected = if committed { &new } else { &old };
                assert_eq!(
                    &reopened, expected,
                    "legacy_old={legacy_old} crash at step {budget}: committed={committed}"
                );
                assert_eq!(
                    commit_leftovers(&project),
                    Vec::<String>::new(),
                    "legacy_old={legacy_old} step {budget}: reopen sweeps every temp"
                );
                let _ = std::fs::remove_dir_all(&project);
                if completed {
                    break;
                }
                budget += 1;
            }
            // 4 stages + 1 commit + 4 renames + 1 prune (tile 1002) + 1
            // manifest finalize = 11 steps; budget 11 is the first clean run.
            assert_eq!(budget, 11, "legacy_old={legacy_old}: the step count");
            // Can-fail guard: the disk WAS torn mid-apply at some step (a
            // protocol that only ever exposed old-or-new states would make
            // the recovery assertions above vacuous).
            assert!(
                saw_torn_disk,
                "legacy_old={legacy_old}: no crash tore the disk"
            );
        }
    }

    #[test]
    fn reopen_after_a_post_commit_crash_restores_the_new_paint() {
        let Some((device, queue)) = try_request_device() else {
            return;
        };
        // OLD saved for real; then a NEW tile staged and committed but
        // never applied (the crash lands right after the commit point).
        // The load path's own recovery must roll it forward: without it
        // the reopen reads the OLD tile-1001.png and this test fails.
        let mut paint =
            paint_state::PaintState::with_target_size(device.clone(), queue.clone(), 512)
                .expect("paint builds");
        let old = pattern_bytes(512, 512, 5);
        let new = pattern_bytes(512, 512, 99);
        assert_ne!(old, new);
        assert!(paint.publish_fill(1001, 512, 512, old.clone()));
        paint.process_pending().expect("upload lands");
        let dir = unique_temp_dir("post-commit-crash");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            save_paint_tiles(&mut paint, &dir, &no_live_layers(), &[]),
            1
        );

        let store = umber_core::AssetStore::new(&dir);
        let entry = stage_paint_png(&dir, &store, "tile-1001.png", 512, 512, &new).unwrap();
        commit_paint_manifest(&dir, &[entry], &[]).unwrap();
        assert!(paint_commit_pending_path(&dir).exists());

        let mut fresh = paint_state::PaintState::with_target_size(device, queue, 512)
            .expect("fresh paint state builds");
        let report = load_paint_tiles_reported(&mut fresh, &dir, &no_live_layers());
        assert_eq!(report.restored, 1);
        assert_eq!(
            fresh.read_tile(1001).expect("readback succeeds").data,
            new,
            "the committed NEW paint is what reopens"
        );
        assert!(!paint_commit_pending_path(&dir).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn committed_save_rolls_forward_from_the_store_when_a_temp_is_lost() {
        let project = unique_temp_dir("lost-temp");
        save_files_faulted(&project, &OLD_FILES, usize::MAX).unwrap();
        // Commit NEW, then lose tile 1001's staged temp: the store blob is
        // the roll-forward source. A first recovery attempt crashes mid
        // re-materialization (leaving a half `.restore` temp); the next
        // one must still land NEW byte-exactly and sweep the torn temp.
        let store = umber_core::AssetStore::new(&project);
        let entries: Vec<PaintCommitEntry> = NEW_FILES
            .iter()
            .map(|(name, bytes)| stage_paint_bytes(&project, &store, name, bytes).unwrap())
            .collect();
        commit_paint_manifest(&project, &entries, &[]).unwrap();
        std::fs::remove_file(paint_sidecar_dir(&project).join(&entries[0].temp)).unwrap();
        assert!(apply_paint_commit(&project, Some(0)).is_err());
        let torn_restore = paint_temp_name("tile-1001.png.restore");
        assert_eq!(
            std::fs::read(paint_sidecar_dir(&project).join(&torn_restore)).unwrap(),
            &NEW_FILES[0].1[..NEW_FILES[0].1.len() / 2],
            "the injected crash left a half-written restore temp"
        );
        recover_paint_sidecar(&project);
        assert_eq!(sidecar_snapshot(&project), snapshot_of(&NEW_FILES));
        assert_eq!(commit_leftovers(&project), Vec::<String>::new());
        let _ = std::fs::remove_dir_all(&project);
    }

    #[test]
    fn store_routed_save_reopens_byte_identical_and_dedupes() {
        let project = unique_temp_dir("store-routed");
        save_files_faulted(&project, &NEW_FILES, usize::MAX).unwrap();
        recover_paint_sidecar(&project);
        let reopened = sidecar_snapshot(&project);
        assert_eq!(reopened, snapshot_of(&NEW_FILES));
        // Every sidecar has a manifest line whose store blob is its exact
        // bytes (the provenance record is complete and truthful).
        let manifest = std::fs::read_to_string(paint_manifest_path(&project)).unwrap();
        let entries = parse_paint_commit(&manifest).expect("the applied manifest parses");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        let want: Vec<&str> = NEW_FILES.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, want, "one manifest line per sidecar, in save order");
        let store = umber_core::AssetStore::new(&project);
        for entry in &entries {
            let blob = store.load(&entry.asset).unwrap();
            assert_eq!(&blob, &reopened[&entry.name], "{} blob", entry.name);
            assert_eq!(entry.asset.size, blob.len() as u64);
        }
        // Dedupe: re-saving identical content stores no new blob — 4
        // distinct contents, 4 blobs, before and after.
        let count_blobs = || {
            std::fs::read_dir(project.join("assets"))
                .unwrap()
                .flatten()
                .map(|shard| std::fs::read_dir(shard.path()).unwrap().count())
                .sum::<usize>()
        };
        assert_eq!(count_blobs(), 4);
        save_files_faulted(&project, &NEW_FILES, usize::MAX).unwrap();
        assert_eq!(count_blobs(), 4, "identical content dedupes in the store");
        let again = std::fs::read_to_string(paint_manifest_path(&project)).unwrap();
        let again = parse_paint_commit(&again).unwrap();
        assert_eq!(
            again.iter().map(|e| &e.asset).collect::<Vec<_>>(),
            entries.iter().map(|e| &e.asset).collect::<Vec<_>>(),
            "the same bytes hash to the same refs"
        );
        assert_eq!(sidecar_snapshot(&project), snapshot_of(&NEW_FILES));
        let _ = std::fs::remove_dir_all(&project);
    }
}
