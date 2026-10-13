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

use crate::hdf5::{Dtype, RawDataset, Tree, HBO_MAGIC, HBO_MARKER_V11};
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
    let m3 = if dataset.has_m3 { "yes" } else { "no" };
    format!("{dtype} {shape} {layout}{chunks} filters={filters} attrs={count} m3={m3}")
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
            project.datasets.push(DatasetReport {
                path,
                status: ImportStatus::Skipped("texture-cache (recomputed on open)".into()),
                detail,
            });
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
}
