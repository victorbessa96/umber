//! Import model: version stamp (§2), HBO detection (§5), H1/H2 (§7).
//!
//! ## Judgment calls
//!
//! - The version-stamp field search looks for the UTF-16LE field bytes
//!   anywhere in the `projectsettings.ini` payload; the value byte rides at
//!   `field_offset + utf16_len + 2 + 5` (§2.1). Display prefers last-save per
//!   component and falls back per component only when the field is absent,
//!   so a legitimate minor of 0 never falls back (§2.2).
//! - HBO classification is detection-only (§5.1–5.2): the `0x69000B11`
//!   marker, else the 12-byte `0x1B7C2FDD` header with the 0/1 era split.
//!   No payload byte past the header is interpreted. An unrecognized
//!   `version_check` still reports as an HBO graph of era `unknown`.
//! - Heuristic H1 (§7-U1): rank-2 is H×W gray; rank-3 with last dim 1–4 is
//!   H×W×C, else first dim 1–4 is C×H×W (transposed to H×W×C on import),
//!   else skipped + reported. `u16` narrows with `>>8`, `f32`/`f64` clamp
//!   `[0,1]` then scale, `i32` clamps `[0,255]`; every conversion is a
//!   per-channel note. The channel name is the dataset leaf name.
//! - Heuristic H2 (§7-U3): one set per top-level group transitively holding
//!   imported rasters, named by that group; root-level rasters group under
//!   the file stem. Set and channel order follow file traversal order.
//! - Mesh payloads (§7-U4), shaders and baked blobs (§7-U5) have no
//!   published paths, so H1 governs them: HBO ones report as HBO graphs,
//!   the rest import or skip by shape. Corpus oracles (§8) will name the
//!   true skip paths.
//! - MurmurHash3 (§7-U7) rides the report as presence-only (`m3: yes/no`);
//!   no verification is attempted.

use std::collections::HashMap;

use crate::hdf5::{Dtype, RawDataset, Tree, HBO_MAGIC, HBO_MARKER_V11};
use crate::texture_cache;
use crate::{
    ChannelImage, DatasetReport, GroupKind, GroupReport, ImportStatus, SppProject, SppVersion,
    TextureSet,
};

/// HBO era from detection-only classification (§5.1–5.2, §5.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HboEra {
    /// First LE u32 is `0x69000B11`: newer v11+ binary.
    NewerV11Plus,
    /// 12-byte header, `version_check` 0: v8.1/9/10 inline tagged family.
    Inline810,
    /// 12-byte header, `version_check` 1: v11/12 registry family.
    Registry1112,
    /// Magic matched but `version_check` is neither 0 nor 1.
    Unknown,
}

impl HboEra {
    /// Report label for the `not imported (HBO graph, era …)` line.
    pub fn label(self) -> &'static str {
        match self {
            HboEra::NewerV11Plus => "newer v11+ binary",
            HboEra::Inline810 => "v8.1/9/10",
            HboEra::Registry1112 => "v11/12",
            HboEra::Unknown => "unknown",
        }
    }
}

/// Detection-only HBO classification (§5.1–5.2). No payload parsing.
pub fn classify_hbo(dtype: Dtype, bytes: &[u8]) -> Option<HboEra> {
    // Detection runs on uint8 payloads only.
    if dtype != Dtype::U8 {
        return None;
    }
    if bytes.len() >= 4 {
        let marker = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if marker == HBO_MARKER_V11 {
            return Some(HboEra::NewerV11Plus);
        }
    }
    if bytes.len() >= 12 {
        let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if magic == HBO_MAGIC {
            let check = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
            return Some(match check {
                0 => HboEra::Inline810,
                1 => HboEra::Registry1112,
                _ => HboEra::Unknown,
            });
        }
    }
    None
}

/// Version-stamp fields (§2.1): (field text, slot).
const STAMP_MAJOR_CREATION: &str = ".versionAtCreation/major";
const STAMP_MINOR_CREATION: &str = ".versionAtCreation/minor";
const STAMP_MAJOR_SAVE: &str = ".versionAtLastSave/major";
const STAMP_MINOR_SAVE: &str = ".versionAtLastSave/minor";

/// Find one stamp field's value byte (§2.1 byte shape).
fn stamp_field(bytes: &[u8], field: &str) -> Option<u8> {
    let utf16: Vec<u8> = field
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    if utf16.is_empty() || bytes.len() < utf16.len() {
        return None;
    }
    bytes
        .windows(utf16.len())
        .position(|window| window == utf16.as_slice())
        .and_then(|pos| bytes.get(pos + utf16.len() + 2 + 5).copied())
}

/// Parse the `projectsettings.ini` payload into per-component options:
/// creation (major, minor) plus last-save (major, minor). Each component is
/// independent so the display can fall back per component (§2.2).
/// One version component: (major, minor), each independently optional
/// (§2.2's per-component fallback).
pub type StampComponent = (Option<u8>, Option<u8>);
/// The parsed stamp: (creation, last_save).
pub type StampPair = (StampComponent, StampComponent);

pub fn parse_stamp(bytes: &[u8]) -> StampPair {
    let creation = (
        stamp_field(bytes, STAMP_MAJOR_CREATION),
        stamp_field(bytes, STAMP_MINOR_CREATION),
    );
    let last_save = (
        stamp_field(bytes, STAMP_MAJOR_SAVE),
        stamp_field(bytes, STAMP_MINOR_SAVE),
    );
    (creation, last_save)
}

/// Display version: last-save preferred per component, creation fallback
/// per component; a present minor of 0 never falls back (§2.2).
pub fn display_version(
    creation: (Option<u8>, Option<u8>),
    last_save: (Option<u8>, Option<u8>),
) -> SppVersion {
    let pair = |slot: (Option<u8>, Option<u8>)| match slot {
        (Some(major), Some(minor)) => Some((major, minor)),
        _ => None,
    };
    SppVersion {
        major: last_save.0.or(creation.0).unwrap_or(0),
        minor: last_save.1.or(creation.1).unwrap_or(0),
        creation: pair(creation),
        last_save: pair(last_save),
    }
}

/// Leaf name of a `/`-separated container path.
fn leaf(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Top-level group of a `/`-separated path (`/a/b` -> `a`; `/a` -> none).
fn top_level(path: &str) -> Option<&str> {
    let trimmed = path.strip_prefix('/').unwrap_or(path);
    let mut parts = trimmed.split('/');
    let first = parts.next().unwrap_or("");
    if parts.next().is_none() {
        None
    } else {
        Some(first)
    }
}

/// §4.3 report facts for one dataset.
fn describe(dataset: &RawDataset) -> String {
    let dtype = dataset.dtype.name();
    let shape = format!("{:?}", dataset.shape);
    let layout = dataset.layout_name;
    let chunks = match &dataset.chunk_dims {
        Some(dims) => format!(" chunks={dims:?}"),
        None => String::new(),
    };
    let filters = if dataset.filters.is_empty() {
        "none".to_string()
    } else {
        dataset
            .filters
            .iter()
            .map(|f| f.name())
            .collect::<Vec<_>>()
            .join("+")
    };
    let count = dataset.attr_count;
    // R6: the checksum state, not just presence — verified / mismatch /
    // unverified (no stored hash). A mismatch is a report line, never a
    // failed open.
    let m3 = match crate::hdf5::m3_verifies(dataset) {
        Some(true) => "verified",
        Some(false) => "MISMATCH",
        None if dataset.has_m3 => "present",
        None => "no",
    };
    format!("{dtype} {shape} {layout}{chunks} filters={filters} attrs={count} m3={m3}")
}

/// The texture-cache stack id present in every corpus set (research §7a).
const MAIN_STACK: &str = "3864292196";

/// Every stack's live channels, keyed `"<set-id>/<stack>"`, from the
/// `list` datasets and the `texture-cache/header` stamp's minor
/// ([`texture_cache::live_channels`]). A stack whose list does not parse
/// is absent, so none of its datasets import.
fn cache_index(tree: &Tree) -> HashMap<String, Vec<(String, u64)>> {
    // The header stamp dataset names the minor in v3.3 files; the v3.2
    // corpus files have no header dataset. Their list self-identifies
    // by shape: 20-byte entries hold u64 flags, 16-byte entries hold
    // u32 bit indexes - and every entry name must be a dataset leaf
    // the stack actually carries.
    let stamped = tree
        .datasets
        .iter()
        .find(|d| d.path.trim_start_matches('/') == "texture-cache/header")
        .and_then(|d| texture_cache::stamp_minor(&d.bytes));
    let candidates: Vec<i64> = match stamped {
        Some(minor) => vec![minor],
        None => vec![2, 3],
    };
    let mut leaves: HashMap<String, std::collections::BTreeSet<String>> = HashMap::new();
    for d in &tree.datasets {
        let parts: Vec<&str> = d.path.trim_start_matches('/').split('/').collect();
        if parts.len() == 4 && parts[0] == "texture-cache" && parts[3] != "list" {
            leaves
                .entry(format!("{}/{}", parts[1], parts[2]))
                .or_default()
                .insert(parts[3].to_string());
        }
    }
    let mut index = HashMap::new();
    for d in &tree.datasets {
        let parts: Vec<&str> = d.path.trim_start_matches('/').split('/').collect();
        if parts.len() != 4 || parts[0] != "texture-cache" || parts[3] != "list" {
            continue;
        }
        let stack = format!("{}/{}", parts[1], parts[2]);
        let Some(names) = leaves.get(&stack) else {
            continue;
        };
        for minor in &candidates {
            let Some(entries) = texture_cache::live_channels(&d.bytes, *minor) else {
                continue;
            };
            if entries.iter().all(|(name, _)| names.contains(name)) {
                index.insert(stack.clone(), entries);
                break;
            }
        }
    }
    index
}

/// One `texture-cache/<set-id>/<stack>/<n>` dataset as `(set, channel)`.
///
/// `Ok(None)`: not a channel (the `list` index, the `header` stamp, an
/// empty dataset). `Err`: a channel-shaped dataset that is not live or
/// did not decode, with the reason. The set is `cache-<set-id>` here;
/// `build` swaps in the material name.
fn cache_channel(
    dataset: &RawDataset,
    live: &HashMap<String, Vec<(String, u64)>>,
) -> Result<Option<(String, ChannelImage)>, String> {
    let trimmed = dataset.path.strip_prefix('/').unwrap_or(&dataset.path);
    let parts: Vec<&str> = trimmed.split('/').collect();
    let leaf = parts.last().copied().unwrap_or("");
    if parts.len() != 4 || leaf == "list" || dataset.bytes.is_empty() {
        return Ok(None);
    }
    // Only datasets the stack's `list` names are live; the flag comes
    // from the list (a v3.3 leaf is a bit index, not a flag).
    let stack = format!("{}/{}", parts[1], parts[2]);
    let Some(entries) = live.get(&stack) else {
        return Err("stack index unreadable (no channels taken from it)".into());
    };
    let Some(&(_, flag)) = entries.iter().find(|(name, _)| name == leaf) else {
        return Err("not in the stack's index (stale cache copy)".into());
    };
    if dataset.dtype != Dtype::U8 {
        let at = dataset.path.clone();
        return Err(format!("{at}: not a u8 blob"));
    }
    // Height is signed around Umber's flat 128; every other channel is
    // unit-range.
    let remap = if flag == texture_cache::HEIGHT_FLAG {
        texture_cache::Remap::Signed
    } else {
        texture_cache::Remap::Unit
    };
    let decoded = match texture_cache::decode_with(&dataset.bytes, remap) {
        Ok(decoded) => decoded,
        Err(texture_cache::CacheError::NotAChannel) => return Ok(None),
        Err(err) => return Err(err.to_string()),
    };
    let (mut name, mut named) = texture_cache::channel_name_for_flag(flag);
    // A name is only as good as the format behind it: if a file pairs a
    // known flag with the wrong texel format (another Painter version,
    // a different stack), the channel keeps its number instead of
    // landing under a wrong name.
    if let Some(expected) = texture_cache::expected_format(flag) {
        if decoded.header.format != expected {
            name = format!("flag_{flag}");
            named = false;
        }
    }
    let mut notes = decoded.notes;
    notes.insert(
        0,
        format!(
            "{} {}x{}",
            decoded.header.format.name(),
            decoded.header.width,
            decoded.header.height
        ),
    );
    if !named {
        notes.push(if name.starts_with("flag_") {
            format!("channel flag {flag}: name unknown (research §7a), imported as {name}")
        } else {
            format!("channel flag {flag}: name inferred as {name} (research §7a)")
        });
    }
    // `texture-cache/<set-id>/<stack>/<flag>`. Stack 3864292196 is in
    // every set of every corpus file at the set's full resolution, so it
    // is the main stack and keeps the set's name. Any other stack (Bessa
    // Synth's two 2048² siblings: material-layering stacks) becomes its
    // own set, `<set>-stack-<id>`, so no channel name collides and none
    // is dropped (research §7a).
    let set = if parts[2] == MAIN_STACK {
        format!("cache-{}", parts[1])
    } else {
        format!("cache-{}-stack-{}", parts[1], parts[2])
    };
    Ok(Some((
        set,
        ChannelImage {
            name,
            path: dataset.path.clone(),
            width: decoded.header.width,
            height: decoded.header.height,
            channels: 4,
            pixels: decoded.rgba,
            notes,
        },
    )))
}

/// Map one dataset to an 8-bit channel image (heuristic H1, §7-U1).
fn channel_from(dataset: &RawDataset) -> Result<ChannelImage, String> {
    let (width, height, channels, order) = match dataset.shape.as_slice() {
        [] | [_] => {
            let rank = dataset.shape.len();
            let at = dataset.path.clone();
            return Err(format!("{at}: rank-{rank} dataset (not a raster channel)"));
        }
        [h, w] => (*w, *h, 1u8, ChannelOrder::RowMajor),
        [h, w, c] if (1..=4).contains(c) => (*w, *h, *c as u8, ChannelOrder::RowMajor),
        [c, h, w] if (1..=4).contains(c) => (*w, *h, *c as u8, ChannelOrder::ChannelFirst),
        _ => {
            let shape = format!("{:?}", dataset.shape);
            let at = dataset.path.clone();
            return Err(format!("{at}: shape {shape} is not a channel image"));
        }
    };
    let width32: u32 = width.try_into().map_err(|_| {
        let at = dataset.path.clone();
        format!("{at}: width exceeds 32 bits (not a raster channel)")
    })?;
    let height32: u32 = height.try_into().map_err(|_| {
        let at = dataset.path.clone();
        format!("{at}: height exceeds 32 bits (not a raster channel)")
    })?;
    let texels = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(channels as u64))
        .ok_or_else(|| {
            let at = dataset.path.clone();
            format!("{at}: channel size overflows (not a raster channel)")
        })?;
    if texels == 0 {
        let empty = dataset.path.clone();
        return Err(format!("{empty}: empty dataset (not a raster channel)"));
    }
    if dataset.dtype != Dtype::Str(0)
        && texels != dataset.bytes.len() as u64 / dataset.dtype.byte_size() as u64
    {
        // Defensive: the reader guarantees this; a mismatch is corruption
        // surfacing late rather than a silent misread.
        let at = dataset.path.clone();
        return Err(format!("{at}: payload does not match shape (corrupt)"));
    }
    let mut notes = Vec::new();
    let pixels = convert_samples(dataset, width, height, channels as usize, order, &mut notes)?;
    let name = leaf(&dataset.path).to_string();
    Ok(ChannelImage {
        name,
        path: dataset.path.clone(),
        width: width32,
        height: height32,
        channels,
        pixels,
        notes,
    })
}

/// Memory order of a rank-3 dataset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChannelOrder {
    RowMajor,
    ChannelFirst,
}

/// Convert decoded element bytes to 8-bit samples (H1 type rules).
fn convert_samples(
    dataset: &RawDataset,
    width: u64,
    height: u64,
    channels: usize,
    order: ChannelOrder,
    notes: &mut Vec<String>,
) -> Result<Vec<u8>, String> {
    let elem = dataset.dtype.byte_size();
    let at = dataset.path.clone();
    // Reorder C×H×W into H×W×C element runs first.
    let ordered: Vec<u8> = match order {
        ChannelOrder::RowMajor => dataset.bytes.clone(),
        ChannelOrder::ChannelFirst => {
            let plane = width
                .checked_mul(height)
                .ok_or_else(|| format!("{at}: channel plane overflows"))?
                as usize;
            if dataset.bytes.len() != plane * channels * elem {
                return Err(format!("{at}: payload does not match shape (corrupt)"));
            }
            let mut flat = vec![0u8; dataset.bytes.len()];
            for c in 0..channels {
                for h in 0..height as usize {
                    for w in 0..width as usize {
                        let src = (c * plane + h * width as usize + w) * elem;
                        let dst = (h * width as usize + w) * channels * elem + c * elem;
                        flat[dst..dst + elem].copy_from_slice(&dataset.bytes[src..src + elem]);
                    }
                }
            }
            flat
        }
    };
    let sample = |bytes: &[u8]| -> u8 {
        match dataset.dtype {
            Dtype::U8 => bytes[0],
            Dtype::U16 => {
                let v = u16::from_le_bytes([bytes[0], bytes[1]]);
                (v >> 8) as u8
            }
            Dtype::F16 => {
                // IEEE-754 binary16 → f32, then the same [0,1] clamp as
                // the other floats (R1: normals keep their float range
                // in the NATIVE payload; this 8-bit preview is the v1
                // flattened path's view).
                let bits = u16::from_le_bytes([bytes[0], bytes[1]]);
                let v = crate::texture_cache::f16_to_f32(bits);
                if v.is_nan() || v <= 0.0 {
                    0
                } else if v >= 1.0 {
                    255
                } else {
                    (v * 255.0).round() as u8
                }
            }
            Dtype::I32 => {
                let v = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                v.clamp(0, 255) as u8
            }
            Dtype::F32 => {
                let v = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                if v.is_nan() || v <= 0.0 {
                    0
                } else if v >= 1.0 {
                    255
                } else {
                    (v * 255.0).round() as u8
                }
            }
            Dtype::F64 => {
                let mut raw = [0u8; 8];
                raw.copy_from_slice(&bytes[0..8]);
                let v = f64::from_le_bytes(raw);
                if v.is_nan() || v <= 0.0 {
                    0
                } else if v >= 1.0 {
                    255
                } else {
                    (v * 255.0).round() as u8
                }
            }
            Dtype::Str(_) => 0,
        }
    };
    if dataset.dtype == Dtype::U16 {
        notes.push("u16 narrowed to 8-bit (>>8)".to_string());
    }
    if dataset.dtype == Dtype::F32 || dataset.dtype == Dtype::F64 {
        notes.push("float clamped to [0,1]".to_string());
    }
    if dataset.dtype == Dtype::F16 {
        notes.push("f16 narrowed to 8-bit (preview; native payload kept)".to_string());
    }
    if dataset.dtype == Dtype::I32 {
        notes.push("i32 clamped to [0,255]".to_string());
    }
    if matches!(dataset.dtype, Dtype::Str(_)) {
        return Err(format!("{at}: string dataset (not a raster channel)"));
    }
    if ordered.len() % elem != 0 {
        return Err(format!("{at}: payload does not match shape (corrupt)"));
    }
    Ok(ordered.chunks_exact(elem).map(sample).collect())
}

/// Build the [`SppProject`] from the parsed tree (H1/H2 + report).
pub fn build(tree: Tree, file_stem: &str) -> SppProject {
    // Before `tree.notes` moves into the project below.
    let live = cache_index(&tree);
    let mut project = SppProject {
        version: SppVersion::default(),
        texture_sets: Vec::new(),
        datasets: Vec::new(),
        groups: Vec::new(),
        notes: tree.notes,
    };
    // Version stamp first (§2): the dataset is consumed, not imported.
    let mut stamp_seen = false;
    for dataset in &tree.datasets {
        if leaf(&dataset.path) == "projectsettings.ini" {
            stamp_seen = true;
            let (creation, last_save) = parse_stamp(&dataset.bytes);
            project.version = display_version(creation, last_save);
            let any = creation.0.is_some()
                || creation.1.is_some()
                || last_save.0.is_some()
                || last_save.1.is_some();
            if !any {
                let at = dataset.path.clone();
                project
                    .notes
                    .push(format!("{at}: no version fields (version 0.0)"));
            }
            break;
        }
    }
    if !stamp_seen {
        project
            .notes
            .push("projectsettings.ini not found (version 0.0)".to_string());
    }
    // Datasets: stamp, cache, HBO, then H1.
    let mut imported: Vec<(String, ChannelImage)> = Vec::new();
    let mut cache_imported: Vec<(String, ChannelImage)> = Vec::new();
    for dataset in &tree.datasets {
        let detail = describe(dataset);
        let path = dataset.path.clone();
        if leaf(&dataset.path) == "projectsettings.ini" {
            project.datasets.push(DatasetReport {
                path,
                status: ImportStatus::Skipped("version stamp".into()),
                detail,
            });
            continue;
        }
        if top_level(&dataset.path) == Some("texture-cache") {
            // The flattened channels (research §7a): decoded, never
            // trusted. A failure is a report line, not an error.
            match cache_channel(dataset, &live) {
                Ok(Some((set, channel))) => {
                    let detail = format!("{detail}; {}", channel.notes.join("; "));
                    project.datasets.push(DatasetReport {
                        path: path.clone(),
                        status: ImportStatus::Imported,
                        detail: detail.trim_end_matches("; ").to_string(),
                    });
                    cache_imported.push((set, channel));
                }
                Ok(None) => project.datasets.push(DatasetReport {
                    path,
                    status: ImportStatus::Skipped("texture-cache index/stamp".into()),
                    detail,
                }),
                Err(reason) => project.datasets.push(DatasetReport {
                    path,
                    status: ImportStatus::Skipped(format!("texture-cache: {reason}")),
                    detail,
                }),
            }
            continue;
        }
        if let Some(era) = classify_hbo(dataset.dtype, &dataset.bytes) {
            let era_label = era.label().to_string();
            project.datasets.push(DatasetReport {
                path,
                status: ImportStatus::NotImported(format!("HBO graph, era {era_label}")),
                detail,
            });
            continue;
        }
        match channel_from(dataset) {
            Ok(channel) => {
                project.datasets.push(DatasetReport {
                    path: path.clone(),
                    status: ImportStatus::Imported,
                    detail: detail.clone(),
                });
                imported.push((path, channel));
            }
            Err(reason) => {
                project.datasets.push(DatasetReport {
                    path,
                    status: ImportStatus::Skipped(reason),
                    detail,
                });
            }
        }
    }
    // Heuristic H2: one set per top-level group with imported rasters.
    let mut order: Vec<String> = Vec::new();
    let mut sets: Vec<TextureSet> = Vec::new();
    for (path, channel) in imported {
        let set_name = top_level(&path).unwrap_or(file_stem).to_string();
        let index = match order.iter().position(|name| *name == set_name) {
            Some(i) => i,
            None => {
                order.push(set_name.clone());
                sets.push(TextureSet {
                    name: set_name,
                    channels: Vec::new(),
                });
                sets.len() - 1
            }
        };
        sets[index].channels.push(channel);
    }
    // The cache's sets, after H1's: one set per `texture-cache/<set-id>`,
    // channels in traversal order. A real Painter file fills only these.
    // Names come from the scene's material uids (`material_names`); a set
    // the scene does not name keeps `cache-<id>`.
    let names: Vec<(u64, String)> = tree
        .datasets
        .iter()
        .find(|d| d.path.trim_start_matches('/') == "paint/scene.assbin")
        .map(|d| texture_cache::material_names(&d.bytes))
        .unwrap_or_default();
    for (set_id, channel) in cache_imported {
        // `cache-<id>` or `cache-<id>-stack-<stack>`: the id's material
        // name replaces `cache-<id>`, the stack suffix stays.
        let rest = set_id.strip_prefix("cache-").unwrap_or(&set_id);
        let (id, stack) = match rest.split_once("-stack-") {
            Some((id, stack)) => (id, Some(stack)),
            None => (rest, None),
        };
        let base = id
            .parse::<u64>()
            .ok()
            .and_then(|id| names.iter().find(|(uid, _)| *uid == id))
            .map_or_else(|| format!("cache-{id}"), |(_, name)| name.clone());
        let set_name = match stack {
            Some(stack) => format!("{base}-stack-{stack}"),
            None => base,
        };
        match sets.iter_mut().find(|s| s.name == set_name) {
            Some(set) => set.channels.push(channel),
            None => sets.push(TextureSet {
                name: set_name,
                channels: vec![channel],
            }),
        }
    }
    project.texture_sets = sets;
    // Group lines, root first (traversal order already is).
    for group in &tree.groups {
        let detail = match group.kind {
            GroupKind::OldStyle => "old-style symbol-table group".to_string(),
            GroupKind::Compact => "compact-link group".to_string(),
            GroupKind::Dense => GroupReport::DENSE_LINE.to_string(),
        };
        project.groups.push(GroupReport {
            path: group.path.clone(),
            kind: group.kind,
            detail,
        });
    }
    project
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A texture-cache dataset in the corpus layout (see
    /// `texture_cache`'s tests for the byte format).
    fn cache_blob(w: i32, h: i32, gl: i32, pixels: &[u8]) -> Vec<u8> {
        fn qs(s: &str) -> Vec<u8> {
            let u: Vec<u8> = s.encode_utf16().flat_map(u16::to_be_bytes).collect();
            let mut o = (u.len() as u32).to_be_bytes().to_vec();
            o.extend(u);
            o
        }
        let int = |k: &str, v: i32| {
            let mut o = qs(k);
            o.extend(2u32.to_be_bytes());
            o.push(0);
            o.extend(v.to_be_bytes());
            o
        };
        let mut o = 5u32.to_be_bytes().to_vec();
        o.extend(int("bufferSize", pixels.len() as i32));
        let mut c = qs("compression");
        c.extend(10u32.to_be_bytes());
        c.push(0);
        c.extend(qs("lz4"));
        o.extend(c);
        o.extend(int("height", h));
        o.extend(int("internalFormat", gl));
        o.extend(int("width", w));
        let block = lz4_flex::block::compress(pixels);
        o.extend((block.len() as u32).to_be_bytes());
        o.extend(block);
        o
    }

    fn blob(path: &str, bytes: Vec<u8>) -> RawDataset {
        RawDataset {
            path: path.to_string(),
            shape: vec![bytes.len() as u64],
            dtype: Dtype::U8,
            layout_name: "chunked",
            chunk_dims: Some(vec![65536]),
            filters: Vec::new(),
            attr_count: 0,
            has_m3: false,
            m3: None,
            bytes,
        }
    }

    #[test]
    fn the_texture_cache_imports_as_one_set_per_cache_group() {
        // The real-file shape: rank-1 u8 blobs only, which H1 never
        // imports, plus the cache.
        let rgba: Vec<u8> = (0..2 * 2 * 4).map(|i| i as u8 * 9).collect();
        // `paint/scene.assbin` names set 777 (texture_cache's Assimp layout).
        let mut scene = b"ASSIMP.binary-dump.".to_vec();
        for (key, kind, data) in [
            ("?mat.name", 3u32, {
                let mut d = 4u32.to_le_bytes().to_vec();
                d.extend(b"Body\0");
                d
            }),
            ("$mat.uid", 5u32, 777u64.to_le_bytes().to_vec()),
        ] {
            scene.extend((key.len() as u32).to_le_bytes());
            scene.extend(key.as_bytes());
            scene.extend([0u8; 8]);
            scene.extend(kind.to_le_bytes());
            scene.extend((data.len() as u32).to_le_bytes());
            scene.extend(data);
        }
        // The v3.3 layout (HydrageaBessa's): the stamp says minor 3, each
        // stack's `list` holds 16-byte entries with a u32 BE bit index at
        // offset 12, and datasets are named by that index.
        let list = |bits: &[u32]| -> Vec<u8> {
            bits.iter()
                .flat_map(|b| {
                    let mut e = vec![0u8; 12];
                    e.extend(b.to_be_bytes());
                    e
                })
                .collect()
        };
        let mut stamp = 2u32.to_be_bytes().to_vec();
        for (k, v) in [("version", 3i32), ("version.minor", 3)] {
            let u: Vec<u8> = k.encode_utf16().flat_map(u16::to_be_bytes).collect();
            stamp.extend((u.len() as u32).to_be_bytes());
            stamp.extend(u);
            stamp.extend(2u32.to_be_bytes());
            stamp.push(0);
            stamp.extend(v.to_be_bytes());
        }
        let tree = Tree {
            datasets: vec![
                blob("/paint/document.bin", vec![1, 2, 3]),
                blob("/paint/scene.assbin", scene),
                blob("/texture-cache/header", stamp),
                // bit 0 = flag 1 (baseColor), bit 7 = flag 128.
                blob(
                    "/texture-cache/777/3864292196/0",
                    cache_blob(2, 2, 0x8C43, &rgba),
                ),
                blob(
                    "/texture-cache/777/3864292196/7",
                    cache_blob(1, 1, 0x822B, &[50, 255]),
                ),
                // A v3.2-named leftover the list does not name: stale.
                blob(
                    "/texture-cache/777/3864292196/128",
                    cache_blob(1, 1, 0x822B, &[9, 9]),
                ),
                blob("/texture-cache/777/3864292196/list", list(&[0, 7])),
                // Listed, but the payload is garbage: a skip with its reason.
                blob("/texture-cache/888/3864292196/0", vec![0, 0, 0, 1, 9]),
                blob("/texture-cache/888/3864292196/list", list(&[0])),
                blob(
                    "/texture-cache/777/42/0",
                    cache_blob(1, 1, 0x8C43, &[1, 2, 3, 4]),
                ),
                // Bit 1 (flag 2, `height`) stored as SRGB8_ALPHA8 instead of
                // RG16F: the format check keeps its number.
                blob(
                    "/texture-cache/777/42/1",
                    cache_blob(1, 1, 0x8C43, &[5, 6, 7, 8]),
                ),
                blob("/texture-cache/777/42/list", list(&[0, 1])),
            ],
            groups: Vec::new(),
            notes: Vec::new(),
        };
        let project = build(tree, "demo");
        assert_eq!(project.texture_sets.len(), 2, "{:?}", project.texture_sets);
        // A non-main stack is its own set: no collision with Body's
        // baseColor, nothing dropped.
        assert_eq!(project.texture_sets[1].name, "Body-stack-42");
        // sRGB bytes land LINEARIZED (the note says so), alpha as stored:
        // 1..4/255 all linearize to 0, and alpha 4 stays 4.
        assert_eq!(project.texture_sets[1].channels[0].pixels, vec![0, 0, 0, 4]);
        assert_eq!(project.texture_sets[1].channels[1].name, "flag_2");
        let set = &project.texture_sets[0];
        assert_eq!(set.name, "Body", "the scene's material uid names the set");
        let names: Vec<&str> = set.channels.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["baseColor", "roughness"]);
        // sRGB colour bytes land LINEARIZED (the importer's note says
        // so), alpha as stored: expected = table(rgba.rgb) + rgba.a.
        let linearized: Vec<u8> = rgba
            .chunks_exact(4)
            .flat_map(|t| {
                let f = |s: u8| {
                    let s = f32::from(s) / 255.0;
                    let l = if s <= 0.040_45 {
                        s / 12.92
                    } else {
                        ((s + 0.055) / 1.055).powf(2.4)
                    };
                    (l * 255.0 + 0.5) as u8
                };
                [f(t[0]), f(t[1]), f(t[2]), t[3]]
            })
            .collect();
        assert_eq!(
            set.channels[0].pixels, linearized,
            "sRGB bytes land linearized"
        );
        assert_eq!(set.channels[1].pixels, vec![50, 50, 50, 255]);
        // Established names carry no naming note.
        assert!(set.channels[1].notes.iter().all(|n| !n.contains("name ")));
        // Every dataset reports; the list is an index, the broken 888 blob
        // a skip with its reason, never an error.
        let status = |p: &str| {
            project
                .datasets
                .iter()
                .find(|d| d.path == p)
                .map(|d| d.status.clone())
        };
        assert_eq!(
            status("/texture-cache/777/3864292196/0"),
            Some(ImportStatus::Imported)
        );
        assert!(matches!(
            status("/texture-cache/777/3864292196/128"),
            Some(ImportStatus::Skipped(r)) if r.contains("stale")
        ));
        assert!(matches!(
            status("/texture-cache/777/3864292196/list"),
            Some(ImportStatus::Skipped(r)) if r.contains("index")
        ));
        assert!(matches!(
            status("/texture-cache/888/3864292196/0"),
            Some(ImportStatus::Skipped(r)) if r.starts_with("texture-cache: ")
        ));
    }
    use crate::builder as sb;

    /// The value byte rides exactly at field + utf16_len + 2 + 5 (§2.1).
    #[test]
    fn stamp_byte_shape_is_exact() {
        let field = ".versionAtLastSave/minor";
        let utf16_len = field.encode_utf16().count() * 2;
        let mut payload = vec![0xAA, 0xBB];
        let start = payload.len();
        for unit in field.encode_utf16() {
            payload.extend_from_slice(&unit.to_le_bytes());
        }
        payload.extend_from_slice(&[0, 0]);
        payload.extend_from_slice(&[9, 9, 9, 9, 9]);
        payload.push(7);
        payload.push(0xCC);
        let (creation, last_save) = parse_stamp(&payload);
        assert_eq!(creation, (None, None));
        assert_eq!(last_save, (None, Some(7)));
        // The pinned offset, not just the value: a 1-byte shift changes the
        // value from 7 to the gap byte 9, proving the +2+5 rule is exact.
        let mut shifted = payload.clone();
        shifted.insert(start + utf16_len, 0x00);
        let (_, shifted_save) = parse_stamp(&shifted);
        assert_eq!(shifted_save, (None, Some(9)));
    }

    /// A legitimate minor of 0 never falls back (§2.2).
    #[test]
    fn stamp_minor_zero_never_falls_back() {
        let bytes = sb::stamp_payload(Some(11), Some(1), Some(12), Some(0));
        let (creation, last_save) = parse_stamp(&bytes);
        assert_eq!(creation, (Some(11), Some(1)));
        assert_eq!(last_save, (Some(12), Some(0)));
        let version = display_version(creation, last_save);
        assert_eq!((version.major, version.minor), (12, 0));
    }

    /// Fallback is per component: a missing save minor keeps creation's.
    #[test]
    fn stamp_fallback_is_per_component() {
        let bytes = sb::stamp_payload(Some(11), Some(2), Some(13), None);
        let (creation, last_save) = parse_stamp(&bytes);
        let version = display_version(creation, last_save);
        assert_eq!((version.major, version.minor), (13, 2));
        assert_eq!(version.creation, Some((11, 2)));
        assert_eq!(version.last_save, None);
    }

    /// HBO detection order and eras (§5.1–5.2); non-u8 never classifies.
    #[test]
    fn hbo_detection_order_and_eras() {
        assert_eq!(
            classify_hbo(Dtype::U8, &sb::hbo_newer_payload()),
            Some(HboEra::NewerV11Plus)
        );
        assert_eq!(
            classify_hbo(Dtype::U8, &sb::hbo_tagged_payload(0)),
            Some(HboEra::Inline810)
        );
        assert_eq!(
            classify_hbo(Dtype::U8, &sb::hbo_tagged_payload(1)),
            Some(HboEra::Registry1112)
        );
        assert_eq!(
            classify_hbo(Dtype::U8, &sb::hbo_tagged_payload(7)),
            Some(HboEra::Unknown)
        );
        assert_eq!(classify_hbo(Dtype::U8, &[1, 2, 3]), None);
        // Magic bytes in a u16 raster are not an HBO graph.
        let mut wide = vec![0xDD, 0x2F, 0x7C, 0x1B];
        wide.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(classify_hbo(Dtype::U16, &wide), None);
        // The marker wins over the magic when both could match.
        let mut both = vec![0x11, 0x0B, 0x00, 0x69];
        both.extend_from_slice(&[0; 12]);
        assert_eq!(classify_hbo(Dtype::U8, &both), Some(HboEra::NewerV11Plus));
    }

    #[test]
    fn f16_bits_decode_to_exact_f32_values() {
        // Ground truth: Python's struct round-trip of each pattern.
        let cases: &[(u16, f32)] = &[
            (0x0000, 0.0),
            (0x3C00, 1.0),
            (0x3800, 0.5),
            (0xBC00, -1.0),
            (0x3400, 0.25),
            (0x7BFF, 65504.0),
            (0x7C00, f32::INFINITY),
            (0xFC00, f32::NEG_INFINITY),
            (0x3555, 0.33325195),
        ];
        for (bits, want) in cases {
            assert_eq!(
                crate::texture_cache::f16_to_f32(*bits),
                *want,
                "0x{bits:04x}"
            );
        }
    }

    #[test]
    fn f16_subnormals_decode_to_the_smallest_representable() {
        // 0x0001 = 2^-24, exactly representable in f32.
        assert_eq!(crate::texture_cache::f16_to_f32(0x0001), 2f32.powi(-24));
        // And its negative.
        assert_eq!(crate::texture_cache::f16_to_f32(0x8001), -2f32.powi(-24));
        // A mid subnormal: 0x0003 = 3 * 2^-24.
        assert_eq!(
            crate::texture_cache::f16_to_f32(0x0003),
            3.0 * 2f32.powi(-24)
        );
    }

    #[test]
    fn an_f16_channel_decodes_to_the_8bit_preview() {
        // A 1x1x4 f16 dataset: 0x3800 (0.5) → 128 (0.5 * 255 = 127.5 →
        // 128) per channel.
        let d = RawDataset {
            path: "root/channel".to_string(),
            shape: vec![1, 1, 4],
            dtype: Dtype::F16,
            layout_name: "contiguous",
            chunk_dims: None,
            filters: Vec::new(),
            attr_count: 0,
            has_m3: false,
            m3: None,
            bytes: vec![0x00, 0x38, 0x00, 0x38, 0x00, 0x38, 0x00, 0x38],
        };
        let img = channel_from(&d).expect("f16 channel decodes");
        assert_eq!(img.pixels, vec![128, 128, 128, 128]);
        assert!(
            img.notes.iter().any(|n| n.contains("f16 narrowed")),
            "the f16 note lands: {:?}",
            img.notes
        );
    }
}
