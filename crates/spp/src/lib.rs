//! umber-spp — read-only `.spp` (HDF5-subset) importer.
//!
//! The research contract is `docs/research/spp-hdf5-layout.md`; this crate
//! implements exactly its §6 subset and §7 model, nothing beyond. Import-only
//! FOREVER (binding ruling 1, `docs/loop/decisions.md`): there is no
//! write-shaped API anywhere in this crate, and none may be added.
//!
//! ## Architecture
//!
//! - [`hdf5`] parses the byte subset: superblock v0–v3 (§6.1), object
//!   headers v1/v2 plus `OCHK` continuations (§6.2), the §6.3 message set,
//!   and §6.4 data decode (compact / contiguous / chunked through a v1
//!   B-tree, deflate + shuffle + fletcher32). Dense (fractal-heap) groups
//!   are reported, never traversed (§6.5).
//! - [`model`] builds the [`SppProject`]: the `projectsettings.ini` version
//!   stamp (§2.1–2.2), HBO detection-only classification (§5.1–5.2, §5.8),
//!   channel heuristic H1 (§7-U1) and texture-set heuristic H2 (§7-U3).
//! - Everything out of subset is a typed [`SppError::Unsupported`] /
//!   [`SppError::Corrupt`] or a loud report line — never a panic, never
//!   silent (§6.6, the never-crash doctrine).
//!
//! ## Judgment calls (recorded per the agent contract)
//!
//! - Message-type numbers (dataspace 1, link-info 2, datatype 3, link 6,
//!   layout 8, filter 11, attribute 12, continuation 16, symbol-table 17)
//!   are the reader's best-effort recall of the HDF5 spec table; no spec
//!   text was available to this kill, so the first corpus file (§8) must
//!   re-pin them against a real `.spp`.
//! - `open` takes `&Path`; file I/O errors become `SppError::Io(String)` so
//!   no `std` or HDF5 type crosses the app seam.
//! - The crate is L0-standalone: `std` + `thiserror` + `flate2` only. The
//!   staged `Cargo.toml` is kept as-is (plain version requirements, no
//!   workspace deps); workspace membership (`members` entry) is the
//!   dragon's merge-time wiring, like the `main.rs` menu seam.
//! - `requirements.md` §7 still rows `.spp` as a hard-wall "Never". Ruling 1
//!   ratified the import direction, but rewriting that row is a spec edit
//!   outside this kill's surface (`crates/spp/**` + kill report), so the
//!   row is left untouched and the tension is named here instead.
//! - Per-dataset decoded payloads are capped at 256 MiB (`MAX_DATASET_BYTES`);
//!   beyond that the dataset is `Unsupported`, because a corrupt chunk
//!   geometry must not become an allocation abort.
//! - `fletcher32` presence is recorded per dataset and never verified
//!   (research doc §6.3: presence is recorded, not verified).

// Never-crash policy (clippy.toml + queue row 13, absorbed from the
// photocraft exploration): the engine crates deny unwrap/expect/panic
// in non-test code. The 2026-10-13 census found ZERO non-test uses,
// so this is a pin. Tests are exempt (the cfg_attr): the deny binds
// ONLY non-test builds, so test-code unwrap/expect stays legal while
// a shipped-code unwrap becomes a hard compile error.
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
#![forbid(unsafe_code)]

#[cfg(test)]
mod builder;
mod hdf5;
mod model;
pub mod texture_cache;

use std::path::Path;

/// Open an `.spp` file and import its raster channels.
///
/// Returns the version stamp, the texture sets (heuristic H2), the
/// per-channel images (heuristic H1), and the full import report. HBO
/// datasets are classified detection-only and reported, never parsed.
pub fn open(path: &Path) -> Result<SppProject, SppError> {
    // Size from metadata before reading: the reader holds the whole file,
    // so a hostile multi-GB path must be refused before `fs::read`.
    let len = std::fs::metadata(path)
        .map_err(|e| SppError::Io(e.to_string()))?
        .len();
    if len > MAX_SPP_FILE_BYTES {
        return Err(SppError::Unsupported(format!(
            "{len}-byte .spp is over the {MAX_SPP_FILE_BYTES}-byte cap"
        )));
    }
    let bytes = std::fs::read(path).map_err(|e| SppError::Io(e.to_string()))?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("project")
        .to_string();
    let tree = hdf5::parse(&bytes)?;
    // The datasets are copies; the file buffer is dead from here on.
    // Dropped before `build` decodes the channels, so peak memory is the
    // datasets plus the channels, not the file on top (741 MiB less on
    // the largest corpus file).
    drop(bytes);
    Ok(model::build(tree, &stem))
}

/// Largest `.spp` opened: 4 GiB. The corpus's largest is 742 MiB, and
/// the reader holds every dataset in memory, so a file several times
/// the RAM of a working machine is refused rather than half-read.
pub const MAX_SPP_FILE_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// Reader errors. `umber` never sees an HDF5 type through this API.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SppError {
    /// No HDF5 signature at any of the 8 probed offsets.
    #[error("not an HDF5 file (no signature at 0/512/1024/2048/4096/8192/16384/32768)")]
    NotHdf5,
    /// Structurally valid but outside the §6 subset (loud, typed).
    #[error("unsupported: {0}")]
    Unsupported(String),
    /// Bounds violation or internal inconsistency (loud, typed).
    #[error("corrupt: {0}")]
    Corrupt(String),
    /// File I/O failure, as a string so no `std` type crosses the seam.
    #[error("io: {0}")]
    Io(String),
}

/// An imported `.spp` project: version stamp, texture sets, import report.
#[derive(Debug, Clone, Default)]
pub struct SppProject {
    /// Display version (last-save preferred per component, §2.2).
    pub version: SppVersion,
    /// Texture sets per heuristic H2 (§7-U3).
    pub texture_sets: Vec<TextureSet>,
    /// One line per dataset encountered: imported / skipped / not-imported.
    pub datasets: Vec<DatasetReport>,
    /// One line per group encountered, including dense-link reports (§6.5).
    pub groups: Vec<GroupReport>,
    /// File-level loud lines (unknown messages, missing stamp, …).
    pub notes: Vec<String>,
}

/// Painter version stamp from `projectsettings.ini` (§2.1–2.2).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SppVersion {
    /// Display major (last-save preferred, creation fallback).
    pub major: u8,
    /// Display minor (a legitimate 0 never falls back, §2.2).
    pub minor: u8,
    /// Raw `versionAtCreation` pair, when present.
    pub creation: Option<(u8, u8)>,
    /// Raw `versionAtLastSave` pair, when present.
    pub last_save: Option<(u8, u8)>,
}

/// One texture set per heuristic H2 (§7-U3).
#[derive(Debug, Clone, Default)]
pub struct TextureSet {
    /// Top-level group name, or the file stem for root-level rasters.
    pub name: String,
    /// Imported channels (heuristic H1).
    pub channels: Vec<ChannelImage>,
}

/// One imported raster channel (heuristic H1, §7-U1), as 8-bit pixels.
#[derive(Debug, Clone, Default)]
pub struct ChannelImage {
    /// Dataset leaf name.
    pub name: String,
    /// Full dataset path inside the container.
    pub path: String,
    /// Width in texels.
    pub width: u32,
    /// Height in texels.
    pub height: u32,
    /// Component count (1 gray, 2–4 H×W×C or C×H×W unfolded).
    pub channels: u8,
    /// Row-major 8-bit samples (`width × height × channels` bytes).
    pub pixels: Vec<u8>,
    /// Per-channel loud lines (`u16` narrowing, `f32` clamps, …).
    pub notes: Vec<String>,
}

/// Import status of a single dataset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportStatus {
    /// Landed as a channel image.
    Imported,
    /// Deliberately not a channel (cache, stamp, shape, …) with the reason.
    Skipped(String),
    /// HBO graph payload: `not imported (HBO graph, era …)`, never an error.
    NotImported(String),
}

/// One line of the import report: every dataset reports (§7).
#[derive(Debug, Clone)]
pub struct DatasetReport {
    /// Full dataset path inside the container.
    pub path: String,
    /// Imported / skipped / not-imported.
    pub status: ImportStatus,
    /// Human-readable detail (shape, layout, filters, era, reason).
    pub detail: String,
}

impl DatasetReport {
    /// The exact report line, e.g.
    /// `paint/doc.bin: not imported (HBO graph, era v11/12)`.
    pub fn summary(&self) -> String {
        let path = self.path.clone();
        let detail = self.detail.clone();
        match &self.status {
            ImportStatus::Imported => format!("{path}: imported ({detail})"),
            ImportStatus::Skipped(reason) => {
                let reason = reason.clone();
                format!("{path}: skipped ({reason}; {detail})")
            }
            ImportStatus::NotImported(reason) => {
                let reason = reason.clone();
                // §5.8's line shape: the parens close at the reason; the
                // detail rides after a dash (the report keeps its
                // diagnostics without breaking the contract line).
                format!("{path}: not imported ({reason}) — {detail}")
            }
        }
    }
}

/// One line per group, including the §6.5 dense-link report.
#[derive(Debug, Clone)]
pub struct GroupReport {
    /// Full group path inside the container (`/` for the root).
    pub path: String,
    /// Storage family actually observed.
    pub kind: GroupKind,
    /// Human-readable detail; dense groups always carry the exact §6.5 line.
    pub detail: String,
}

/// Storage family of a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    /// Old-style: symbol-table message + `SNOD` + local `HEAP`.
    OldStyle,
    /// Compact links enumerated from the object header.
    Compact,
    /// Fractal-heap resident: reported, never traversed (§6.5).
    Dense,
}

impl GroupReport {
    /// The exact §6.5 line for dense groups.
    pub const DENSE_LINE: &'static str = "dense-link group (not traversed)";
}

#[cfg(test)]
mod tests {
    #[test]
    fn probe_hbo_summary_line() {
        let path = sb::write_temp("probe-hbo", &hbo_file());
        let project = open(&path).expect("probe open");
        for d in &project.datasets {
            eprintln!("PROBE: {}", d.summary());
        }
    }

    use super::*;
    use crate::builder as sb;

    /// File stem of a scratch path (H2 names root-level sets after it).
    fn stem_of(path: &std::path::Path) -> String {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("project")
            .to_string()
    }

    /// v0 file: old-style root with a nested old-style `Suit` group + stamp.
    /// `prefix` blank bytes may precede the superblock (probe offsets).
    fn old_style_file_at(prefix: usize) -> Vec<u8> {
        let mut b = sb::B::new();
        b.zeros(prefix);
        let (sb_base, btree_patch, heap_patch) = b.superblock_v0_slot();
        let albedo = b.ohdr_v1(&[
            (sb::M_DATASPACE, sb::dataspace_v1(2, &[2, 2], None)),
            (sb::M_DATATYPE, sb::dtype_u8()),
            (sb::M_LAYOUT, sb::layout_compact(&[10, 20, 30, 40])),
        ]);
        let stamp = sb::stamp_payload(Some(11), Some(2), Some(12), Some(0));
        let stamp_len = stamp.len() as u64;
        let stamp_ds = b.ohdr_v1(&[
            (sb::M_DATASPACE, sb::dataspace_v1(1, &[stamp_len], None)),
            (sb::M_DATATYPE, sb::dtype_u8()),
            (sb::M_LAYOUT, sb::layout_compact(&stamp)),
        ]);
        let normal = b.ohdr_v1(&[
            (sb::M_DATASPACE, sb::dataspace_v1(2, &[1, 1], None)),
            (sb::M_DATATYPE, sb::dtype_u8()),
            (sb::M_LAYOUT, sb::layout_compact(&[200])),
            (sb::M_ATTRIBUTE, sb::attr_msg("author")),
        ]);
        let (suit_heap, suit_offs) = b.heap(&["normal"]);
        let suit_snod = b.snod(&[(suit_offs[0] as u32, normal)]);
        let suit_btree = b.btree_group(&[(suit_offs[0] as u32, suit_snod)], 0);
        let suit = b.ohdr_v1(&[(sb::M_SYMBOL_TABLE, sb::symtab_msg(suit_btree, suit_heap))]);
        let (root_heap, root_offs) = b.heap(&["albedo", "projectsettings.ini", "Suit"]);
        let root_snod = b.snod(&[
            (root_offs[0] as u32, albedo),
            (root_offs[1] as u32, stamp_ds),
            (root_offs[2] as u32, suit),
        ]);
        let root_btree = b.btree_group(&[(root_offs[0] as u32, root_snod)], 0);
        b.patch_u64(btree_patch, root_btree);
        b.patch_u64(heap_patch, root_heap);
        b.finish(sb_base + 40);
        b.buf.clone()
    }

    /// v0 file: old-style root with a nested old-style `Suit` group + stamp.
    fn old_style_file() -> Vec<u8> {
        old_style_file_at(0)
    }

    /// v2 file: compact-link root over chunked f32, contiguous u16, i32,
    /// f64 and string datasets.
    fn v2_raster_file() -> Vec<u8> {
        let mut b = sb::B::new();
        let (sb_base, root_patch) = b.superblock_v2_slot();
        // 4x4 f32, chunks of 2x2, shuffled + deflated + fletcher32-marked.
        let heights: [f32; 16] = [
            -1.0, 0.0, 0.25, 0.5, 0.75, 1.0, 2.0, 0.0, 0.25, 0.5, 0.75, 1.0, 2.0, 3.0, 100.0, -2.0,
        ];
        let mut chunks = Vec::new();
        for cr in 0..2u64 {
            for cc in 0..2u64 {
                let mut raw = Vec::new();
                for r in 0..2u64 {
                    for c in 0..2u64 {
                        let v = heights[(cr * 2 + r) as usize * 4 + (cc * 2 + c) as usize];
                        raw.extend_from_slice(&v.to_le_bytes());
                    }
                }
                let stored = sb::deflate_bytes(&sb::shuffle_bytes(&raw, 4));
                let at = b.bytes(&stored);
                chunks.push((0u32, stored.len() as u64, vec![cr, cc], at));
            }
        }
        let btree = b.btree_chunk_leaf(2, &chunks);
        let height = b.ohdr_v1(&[
            (sb::M_DATASPACE, sb::dataspace_v1(2, &[4, 4], None)),
            (sb::M_DATATYPE, sb::dtype_f32()),
            (sb::M_LAYOUT, sb::layout_chunked(2, btree, &[2, 2], 4)),
            (
                sb::M_FILTER,
                sb::pipeline(
                    1,
                    &[
                        (sb::F_SHUFFLE, &[]),
                        (sb::F_DEFLATE, &[6]),
                        (sb::F_FLETCHER32, &[]),
                    ],
                ),
            ),
            (sb::M_ATTRIBUTE, sb::attr_msg("m3_x64_128")),
            (sb::M_ATTRIBUTE, sb::attr_msg("units")),
        ]);
        let mask_raw: Vec<u8> = [0x1234u16, 0xABCD, 0x00FF, 0xFF00]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let mask_at = b.bytes(&mask_raw);
        let mask = b.ohdr_v1(&[
            (sb::M_DATASPACE, sb::dataspace_v1(2, &[2, 2], None)),
            (sb::M_DATATYPE, sb::dtype_u16()),
            (sb::M_LAYOUT, sb::layout_contiguous(mask_at, 8)),
        ]);
        let depth = b.ohdr_v1(&[
            (sb::M_DATASPACE, sb::dataspace_v1(2, &[1, 1], None)),
            (sb::M_DATATYPE, sb::dtype_f64()),
            (sb::M_LAYOUT, sb::layout_compact(&0.5f64.to_le_bytes())),
        ]);
        let mut ramp = Vec::new();
        for v in [-5i32, 0, 300, 127] {
            ramp.extend_from_slice(&v.to_le_bytes());
        }
        let ramp_ds = b.ohdr_v1(&[
            (sb::M_DATASPACE, sb::dataspace_v1(2, &[2, 2], None)),
            (sb::M_DATATYPE, sb::dtype_i32()),
            (sb::M_LAYOUT, sb::layout_compact(&ramp)),
        ]);
        let greet = b.ohdr_v1(&[
            // A scalar-rank dataspace (rank 0): one 5-byte string element.
            (sb::M_DATASPACE, sb::dataspace_v1(0, &[], None)),
            (sb::M_DATATYPE, sb::dtype_string(5)),
            (sb::M_LAYOUT, sb::layout_compact(b"hello")),
        ]);
        let root = b.ohdr_v2(&[
            (sb::M_LINK_INFO as u8, sb::link_info(None)),
            (sb::M_LINK as u8, sb::link_hard("height", height)),
            (sb::M_LINK as u8, sb::link_hard("mask", mask)),
            (sb::M_LINK as u8, sb::link_hard("depth", depth)),
            (sb::M_LINK as u8, sb::link_hard("ramp", ramp_ds)),
            (sb::M_LINK as u8, sb::link_hard("greet", greet)),
        ]);
        b.patch_u64(root_patch, root);
        b.finish(sb_base + 28);
        b.buf.clone()
    }

    /// One v1 dataset under an old-style v0 root (malformed-input helper).
    fn one_dataset_v0(messages: Vec<(u16, Vec<u8>)>, name: &str) -> Vec<u8> {
        let mut b = sb::B::new();
        let (sb_base, btree_patch, heap_patch) = b.superblock_v0_slot();
        let dataset = b.ohdr_v1(&messages);
        let (heap, offs) = b.heap(&[name]);
        let snod = b.snod(&[(offs[0] as u32, dataset)]);
        let btree = b.btree_group(&[(offs[0] as u32, snod)], 0);
        b.patch_u64(btree_patch, btree);
        b.patch_u64(heap_patch, heap);
        b.finish(sb_base + 40);
        b.buf
    }

    #[test]
    fn old_style_nested_groups_h2_and_stamp() {
        let path = sb::write_temp("oldstyle", &old_style_file());
        let project = open(&path).expect("old-style file must open");
        assert_eq!((project.version.major, project.version.minor), (12, 0));
        assert_eq!(project.version.creation, Some((11, 2)));
        assert_eq!(project.version.last_save, Some((12, 0)));
        assert_eq!(project.texture_sets.len(), 2);
        let stem = stem_of(&path);
        assert_eq!(project.texture_sets[0].name, stem);
        let albedo = &project.texture_sets[0].channels;
        assert_eq!(albedo.len(), 1);
        assert_eq!(albedo[0].name, "albedo");
        assert_eq!(
            (albedo[0].width, albedo[0].height, albedo[0].channels),
            (2, 2, 1)
        );
        assert_eq!(albedo[0].pixels, vec![10, 20, 30, 40]);
        assert_eq!(project.texture_sets[1].name, "Suit");
        assert_eq!(project.texture_sets[1].channels[0].pixels, vec![200]);
        let stamp = project
            .datasets
            .iter()
            .find(|d| d.path == "/projectsettings.ini")
            .expect("stamp is reported");
        assert_eq!(stamp.status, ImportStatus::Skipped("version stamp".into()));
        assert_eq!(project.groups.len(), 2);
        assert_eq!(project.groups[0].path, "/");
        assert_eq!(project.groups[0].kind, GroupKind::OldStyle);
        assert_eq!(project.groups[1].path, "/Suit");
        let line = project
            .datasets
            .iter()
            .find(|d| d.path == "/albedo")
            .expect("albedo is reported")
            .summary();
        assert_eq!(
            line,
            "/albedo: imported (u8 [2, 2] compact filters=none attrs=0 m3=no)"
        );
        let normal = project
            .datasets
            .iter()
            .find(|d| d.path == "/Suit/normal")
            .expect("nested raster is reported")
            .summary();
        assert_eq!(
            normal,
            "/Suit/normal: imported (u8 [1, 1] compact filters=none attrs=1 m3=no)"
        );
    }

    #[test]
    fn v2_chunked_filters_and_narrowing() {
        let path = sb::write_temp("v2raster", &v2_raster_file());
        let project = open(&path).expect("v2 raster file must open");
        assert_eq!(project.texture_sets.len(), 1);
        let set = &project.texture_sets[0];
        assert_eq!(set.name, stem_of(&path));
        assert_eq!(set.channels.len(), 4);
        let height = &set.channels[0];
        assert_eq!(height.name, "height");
        assert_eq!(
            height.pixels,
            vec![0, 0, 64, 128, 191, 255, 255, 0, 64, 128, 191, 255, 255, 255, 255, 0]
        );
        assert_eq!(height.notes, vec!["float clamped to [0,1]".to_string()]);
        let detail = project
            .datasets
            .iter()
            .find(|d| d.path == "/height")
            .expect("height is reported")
            .detail
            .clone();
        assert_eq!(
            detail,
            "f32 [4, 4] chunked chunks=[2, 2] filters=shuffle+deflate+fletcher32 attrs=2 m3=yes"
        );
        let mask = &set.channels[1];
        assert_eq!(mask.pixels, vec![18, 171, 0, 255]);
        assert_eq!(mask.notes, vec!["u16 narrowed to 8-bit (>>8)".to_string()]);
        assert_eq!(set.channels[2].pixels, vec![128]);
        assert_eq!(set.channels[3].pixels, vec![0, 0, 255, 127]);
        assert_eq!(
            set.channels[3].notes,
            vec!["i32 clamped to [0,255]".to_string()]
        );
        let greet = project
            .datasets
            .iter()
            .find(|d| d.path == "/greet")
            .expect("string is reported");
        assert!(matches!(greet.status, ImportStatus::Skipped(_)));
        assert!(
            greet.detail.starts_with("string []"),
            "scalar string: {}",
            greet.detail
        );
        assert_eq!(project.groups.len(), 1);
        assert_eq!(project.groups[0].kind, GroupKind::Compact);
    }

    /// v0 file whose datasets are HBO graphs plus one HBO-looking u16 raster.
    fn hbo_file() -> Vec<u8> {
        let mut b = sb::B::new();
        let (sb_base, btree_patch, heap_patch) = b.superblock_v0_slot();
        let mut made = Vec::new();
        let newer = sb::hbo_newer_payload();
        let tagged0 = sb::hbo_tagged_payload(0);
        let tagged1 = sb::hbo_tagged_payload(1);
        let raw_mystery = sb::hbo_tagged_payload(1);
        let blob = b.bytes(&raw_mystery);
        b.patch_u32(blob + 4, 7);
        let mystery = b.buf[blob as usize..blob as usize + raw_mystery.len()].to_vec();
        // u16 raster whose bytes open with the HBO magic: not a graph (§5.1).
        let wide: Vec<u8> = [0xDD, 0x2F, 0x7C, 0x1B, 0, 0, 0, 0].to_vec();
        let payloads = [
            ("newer.bin", newer),
            ("doc.bin", tagged1),
            ("old.bin", tagged0),
            ("mystery.bin", mystery),
        ];
        for (name, payload) in payloads {
            let len = payload.len() as u64;
            let dataset = b.ohdr_v1(&[
                (sb::M_DATASPACE, sb::dataspace_v1(1, &[len], None)),
                (sb::M_DATATYPE, sb::dtype_u8()),
                (sb::M_LAYOUT, sb::layout_compact(&payload)),
            ]);
            made.push((name, dataset));
        }
        let plain = b.ohdr_v1(&[
            (sb::M_DATASPACE, sb::dataspace_v1(2, &[2, 2], None)),
            (sb::M_DATATYPE, sb::dtype_u16()),
            (sb::M_LAYOUT, sb::layout_compact(&wide)),
        ]);
        made.push(("plain", plain));
        let names = ["newer.bin", "doc.bin", "old.bin", "mystery.bin", "plain"];
        let (heap, offs) = b.heap(&names);
        let entries: Vec<(u32, u64)> = offs
            .iter()
            .zip(made.iter())
            .map(|(off, (_, addr))| (*off as u32, *addr))
            .collect();
        let snod = b.snod(&entries);
        let btree = b.btree_group(&[(offs[0] as u32, snod)], 0);
        b.patch_u64(btree_patch, btree);
        b.patch_u64(heap_patch, heap);
        b.finish(sb_base + 40);
        b.buf.clone()
    }

    #[test]
    fn hbo_reports_never_errors() {
        let path = sb::write_temp("hbo", &hbo_file());
        let project = open(&path).expect("HBO file must open without errors");
        for (name, era) in [
            ("newer.bin", "newer v11+ binary"),
            ("doc.bin", "v11/12"),
            ("old.bin", "v8.1/9/10"),
            ("mystery.bin", "unknown"),
        ] {
            let report = project
                .datasets
                .iter()
                .find(|d| d.path == format!("/{name}"))
                .unwrap_or_else(|| panic!("{name} is reported"));
            assert_eq!(
                report.status,
                ImportStatus::NotImported(format!("HBO graph, era {era}"))
            );
            assert!(
                report
                    .summary()
                    .contains(&format!("not imported (HBO graph, era {era})")),
                "loud HBO line for {name}"
            );
        }
        // The u16 lookalike imports as a channel: HBO is uint8-only.
        let plain = project
            .texture_sets
            .iter()
            .flat_map(|s| s.channels.iter())
            .find(|c| c.name == "plain")
            .expect("u16 lookalike imports");
        assert_eq!(plain.pixels, vec![47, 27, 0, 0]);
    }

    #[test]
    fn dense_group_not_traversed() {
        let mut b = sb::B::new();
        let (sb_base, root_patch) = b.superblock_v2_slot();
        let (heap, _) = b.heap(&["ghost"]);
        let dense_child = b.ohdr_v2(&[(sb::M_LINK_INFO as u8, sb::link_info(Some(heap)))]);
        let root = b.ohdr_v2(&[
            (sb::M_LINK_INFO as u8, sb::link_info(None)),
            (sb::M_LINK as u8, sb::link_hard("dep", dense_child)),
        ]);
        b.patch_u64(root_patch, root);
        b.finish(sb_base + 28);
        let path = sb::write_temp("dense", &b.buf);
        let project = open(&path).expect("dense file must open");
        assert_eq!(project.groups.len(), 2);
        assert_eq!(project.groups[1].path, "/dep");
        assert_eq!(project.groups[1].kind, GroupKind::Dense);
        assert_eq!(project.groups[1].detail, GroupReport::DENSE_LINE);
        assert!(project.datasets.is_empty());
        assert!(project.texture_sets.is_empty());
    }

    #[test]
    fn texture_cache_skipped() {
        let mut b = sb::B::new();
        let (sb_base, root_patch) = b.superblock_v2_slot();
        let tile = b.ohdr_v1(&[
            (sb::M_DATASPACE, sb::dataspace_v1(2, &[1, 1], None)),
            (sb::M_DATATYPE, sb::dtype_u8()),
            (sb::M_LAYOUT, sb::layout_compact(&[9])),
        ]);
        let cache = b.ohdr_v2(&[
            (sb::M_LINK_INFO as u8, sb::link_info(None)),
            (sb::M_LINK as u8, sb::link_hard("tile", tile)),
        ]);
        let root = b.ohdr_v2(&[
            (sb::M_LINK_INFO as u8, sb::link_info(None)),
            (sb::M_LINK as u8, sb::link_hard("texture-cache", cache)),
        ]);
        b.patch_u64(root_patch, root);
        b.finish(sb_base + 28);
        let path = sb::write_temp("cache", &b.buf);
        let project = open(&path).expect("cache file must open");
        assert_eq!(project.datasets.len(), 1);
        assert_eq!(
            project.datasets[0].status,
            ImportStatus::Skipped("texture-cache index/stamp".into())
        );
        assert!(project.texture_sets.is_empty());
    }

    #[test]
    fn filter_mask_skip_honored() {
        // Pipeline v2, one chunk skipping deflate (mask bit 1).
        let mut b = sb::B::new();
        let (sb_base, root_patch) = b.superblock_v2_slot();
        let stored0 = sb::deflate_bytes(&sb::shuffle_bytes(&[1, 2], 1));
        let at0 = b.bytes(&stored0);
        let stored1 = sb::shuffle_bytes(&[3, 4], 1);
        let at1 = b.bytes(&stored1);
        let btree = b.btree_chunk_leaf(
            2,
            &[
                (0u32, stored0.len() as u64, vec![0, 0], at0),
                (2u32, stored1.len() as u64, vec![1, 0], at1),
            ],
        );
        let masked = b.ohdr_v1(&[
            (sb::M_DATASPACE, sb::dataspace_v1(2, &[2, 2], None)),
            (sb::M_DATATYPE, sb::dtype_u8()),
            (sb::M_LAYOUT, sb::layout_chunked(2, btree, &[1, 2], 1)),
            (
                sb::M_FILTER,
                sb::pipeline(2, &[(sb::F_SHUFFLE, &[]), (sb::F_DEFLATE, &[6])]),
            ),
        ]);
        let root = b.ohdr_v2(&[
            (sb::M_LINK_INFO as u8, sb::link_info(None)),
            (sb::M_LINK as u8, sb::link_hard("masked", masked)),
        ]);
        b.patch_u64(root_patch, root);
        b.finish(sb_base + 28);
        let path = sb::write_temp("maskbit", &b.buf);
        let project = open(&path).expect("mask file must open");
        let channel = &project.texture_sets[0].channels[0];
        assert_eq!(channel.pixels, vec![1, 2, 3, 4]);
    }

    #[test]
    fn maxdims_mismatch_is_unsupported() {
        let bytes = one_dataset_v0(
            vec![
                (sb::M_DATASPACE, sb::dataspace_v1(2, &[2, 2], Some(&[2, 4]))),
                (sb::M_DATATYPE, sb::dtype_u8()),
                (sb::M_LAYOUT, sb::layout_compact(&[0, 0, 0, 0])),
            ],
            "wide",
        );
        let path = sb::write_temp("maxdims", &bytes);
        match open(&path) {
            Err(SppError::Unsupported(msg)) => assert!(msg.contains("max-dims"), "{msg}"),
            other => panic!("max-dims mismatch must be Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn out_of_subset_datatypes_are_unsupported() {
        let be = one_dataset_v0(
            vec![
                (sb::M_DATASPACE, sb::dataspace_v1(2, &[1, 1], None)),
                (sb::M_DATATYPE, sb::dtype_be_u16()),
                (sb::M_LAYOUT, sb::layout_compact(&[0, 0])),
            ],
            "be",
        );
        let path = sb::write_temp("bedtype", &be);
        match open(&path) {
            Err(SppError::Unsupported(msg)) => assert!(msg.contains("big-endian"), "{msg}"),
            other => panic!("big-endian must be Unsupported, got {other:?}"),
        }
        let compound = one_dataset_v0(
            vec![
                (sb::M_DATASPACE, sb::dataspace_v1(2, &[1, 1], None)),
                (sb::M_DATATYPE, sb::dtype_compound()),
                (sb::M_LAYOUT, sb::layout_compact(&[0; 8])),
            ],
            "compound",
        );
        let path = sb::write_temp("compound", &compound);
        match open(&path) {
            Err(SppError::Unsupported(msg)) => assert!(msg.contains("compound"), "{msg}"),
            other => panic!("compound must be Unsupported, got {other:?}"),
        }
    }

    #[test]
    fn malformed_inputs_are_typed_never_panic() {
        let zeros = sb::write_temp("zeros", &[0u8; 100]);
        assert!(matches!(open(&zeros), Err(SppError::NotHdf5)));
        let short = sb::write_temp("short", &[137, 72, 68]);
        assert!(matches!(open(&short), Err(SppError::NotHdf5)));
        // Bad superblock version.
        let mut bad_version = old_style_file();
        bad_version[8] = 4;
        let path = sb::write_temp("badversion", &bad_version);
        match open(&path) {
            Err(SppError::Unsupported(msg)) => {
                assert!(msg.contains("superblock version 4"), "{msg}")
            }
            other => panic!("bad version must be Unsupported, got {other:?}"),
        }
        // Truncated mid-superblock: signature present, widths unreadable.
        let mut truncated = old_style_file();
        truncated.truncate(20);
        let path = sb::write_temp("truncated", &truncated);
        assert!(matches!(open(&path), Err(SppError::Corrupt(_))));
    }

    #[test]
    fn signature_at_512_opens() {
        let path = sb::write_temp("offset512", &old_style_file_at(512));
        let project = open(&path).expect("signature at 512 must open");
        assert_eq!((project.version.major, project.version.minor), (12, 0));
        assert_eq!(project.texture_sets.len(), 2);
    }

    #[test]
    fn out_of_bounds_message_is_corrupt() {
        let mut b = sb::B::new();
        let (sb_base, btree_patch, heap_patch) = b.superblock_v0_slot();
        let dataset = b.ohdr_v1(&[
            (sb::M_DATASPACE, sb::dataspace_v1(2, &[1, 1], None)),
            (sb::M_DATATYPE, sb::dtype_u8()),
            (sb::M_LAYOUT, sb::layout_compact(&[7])),
        ]);
        // Corrupt the first message's size far past the header end.
        b.patch_u16(dataset + 18, 0x7000);
        let (heap, offs) = b.heap(&["broken"]);
        let snod = b.snod(&[(offs[0] as u32, dataset)]);
        let btree = b.btree_group(&[(offs[0] as u32, snod)], 0);
        b.patch_u64(btree_patch, btree);
        b.patch_u64(heap_patch, heap);
        b.finish(sb_base + 40);
        let path = sb::write_temp("oob", &b.buf);
        match open(&path) {
            Err(SppError::Corrupt(msg)) => assert!(msg.contains("overruns"), "{msg}"),
            other => panic!("OOB message must be Corrupt, got {other:?}"),
        }
    }

    #[test]
    fn channel_first_transposes() {
        // H1's two branches on the SAME byte payload: [3,2,2] reads as
        // H×W×C last-dim (3 high, 2 wide, 2 chan), while [2,3,2] has a
        // last dim > 4 so it takes the C×H×W first-dim branch (2 chan,
        // 3 high, 2 wide) — the transpose is observable as W/H swapping.
        let hwc = one_dataset_v0(
            vec![
                (sb::M_DATASPACE, sb::dataspace_v1(3, &[3, 2, 2], None)),
                (sb::M_DATATYPE, sb::dtype_u8()),
                (
                    sb::M_LAYOUT,
                    sb::layout_compact(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]),
                ),
            ],
            "hwc",
        );
        let path = sb::write_temp("hwc", &hwc);
        let project = open(&path).expect("H×W×C file must open");
        let channel = &project.texture_sets[0].channels[0];
        assert_eq!((channel.width, channel.height, channel.channels), (2, 3, 2));
        // The C×H×W branch: first dim 2 = channels, but the arm only
        // fires when the LAST dim is NOT 1..=4 — [2, 3, 2] has last dim
        // 2 (in range), so it stays H×W×C; a true channel-first shape is
        // [2, 3, 9] (channels 2, last dim 9 > 4).
        let chxw = one_dataset_v0(
            vec![
                (sb::M_DATASPACE, sb::dataspace_v1(3, &[2, 3, 9], None)),
                (sb::M_DATATYPE, sb::dtype_u8()),
                (sb::M_LAYOUT, sb::layout_compact(&[0; 54])),
            ],
            "chanfirst",
        );
        let path = sb::write_temp("chanfirst2", &chxw);
        let project = open(&path).expect("C×H×W file must open");
        let channel = &project.texture_sets[0].channels[0];
        assert_eq!(
            (channel.width, channel.height, channel.channels),
            (9, 3, 2),
            "C×H×W [2,3,9]: c=2, h=3, w=9"
        );
    }

    #[test]
    fn ochk_continuation_decodes() {
        let mut b = sb::B::new();
        let (sb_base, btree_patch, heap_patch) = b.superblock_v0_slot();
        let chunk = b.ochk(&[
            (sb::M_DATATYPE as u8, sb::dtype_u8()),
            (sb::M_LAYOUT as u8, sb::layout_compact(&[5, 6])),
        ]);
        let mut cont = Vec::new();
        cont.extend_from_slice(&chunk.to_le_bytes());
        cont.extend_from_slice(&0u64.to_le_bytes());
        let dataset = b.ohdr_v1(&[
            (sb::M_DATASPACE, sb::dataspace_v1(2, &[1, 2], None)),
            (sb::M_CONTINUATION, cont),
        ]);
        let (heap, offs) = b.heap(&["split"]);
        let snod = b.snod(&[(offs[0] as u32, dataset)]);
        let btree = b.btree_group(&[(offs[0] as u32, snod)], 0);
        b.patch_u64(btree_patch, btree);
        b.patch_u64(heap_patch, heap);
        b.finish(sb_base + 40);
        let path = sb::write_temp("ochk", &b.buf);
        let project = open(&path).expect("OCHK file must open");
        assert_eq!(project.texture_sets[0].channels[0].pixels, vec![5, 6]);
    }

    #[test]
    fn soft_and_external_links_are_loud() {
        let mut b = sb::B::new();
        let (sb_base, root_patch) = b.superblock_v2_slot();
        let real = b.ohdr_v1(&[
            (sb::M_DATASPACE, sb::dataspace_v1(2, &[1, 1], None)),
            (sb::M_DATATYPE, sb::dtype_u8()),
            (sb::M_LAYOUT, sb::layout_compact(&[42])),
        ]);
        let root = b.ohdr_v2(&[
            (sb::M_LINK_INFO as u8, sb::link_info(None)),
            (sb::M_LINK as u8, sb::link_hard("real", real)),
            (sb::M_LINK as u8, sb::link_soft("alias", "real")),
            (sb::M_LINK as u8, sb::link_external("ext")),
        ]);
        b.patch_u64(root_patch, root);
        b.finish(sb_base + 28);
        let path = sb::write_temp("links", &b.buf);
        let project = open(&path).expect("link file must open");
        // The alias never duplicates the import.
        assert_eq!(project.datasets.len(), 1);
        assert!(project
            .notes
            .iter()
            .any(|n| n.contains("same object, not duplicated")));
        assert!(project.notes.iter().any(|n| n.contains("not followed")));
    }

    #[test]
    fn null_dataspace_is_skipped() {
        let bytes = one_dataset_v0(
            vec![
                (sb::M_DATASPACE, vec![2, 0, 0, 2]),
                (sb::M_DATATYPE, sb::dtype_u8()),
                (sb::M_LAYOUT, sb::layout_compact(&[])),
            ],
            "void",
        );
        let path = sb::write_temp("nullspace", &bytes);
        let project = open(&path).expect("null dataspace must not error");
        assert!(project.datasets.is_empty());
        assert!(project.notes.iter().any(|n| n.contains("null dataspace")));
    }

    #[test]
    fn missing_file_is_io() {
        let path = std::env::temp_dir().join("umber-spp-no-such-file-7f3a.spp");
        assert!(matches!(open(&path), Err(SppError::Io(_))));
    }
}
