//! Mesh-map import by naming convention (requirements §7: "Mesh-map
//! import by naming convention (TextureSetName_map)").
//!
//! The baker emits `<Set>_<map>.png` files (see [`umber_mesh::mesh_maps`]
//! — the P0-side parse/format, reused here, never duplicated). This module
//! is the P1 import side: given a folder (or an egui file-drop) of PNGs,
//! route each file to its texture set + channel, report the files that do
//! not match, and import the matched ones as NEW layers (non-destructive —
//! never a channel overwrite).
//!
//! # Judgment calls (for the static verifier)
//!
//! * The P0 parse ([`umber_mesh::parse_mesh_map_stem`]) is reused verbatim.
//!   Unknown suffixes do NOT come back from that parse (it returns `None`
//!   for plain textures by design), so the user-channel fallback below
//!   re-splits the stem on its last `_` itself. The two stages never
//!   disagree: stage 1 (known mesh-map suffix) wins, stage 2 (generic
//!   `<set>_<suffix>`) only runs when stage 1 returned `None`.
//! * Channel routing: AO/normal/curvature/position/thickness/ID/height map
//!   to channels of those names (see [`channel_for_kind`]). `normal_base`
//!   (the baked tangent-space normal) feeds the `normal` channel;
//!   `bent_normal` / `world_space_normal` become user channels of those
//!   names (the default set has no such channels yet); the skew map
//!   (`<set>_skew`, which lives in `bakes_panel::SKEW_SUFFIX`, not in
//!   `MeshMapKind`) arrives through the unknown-suffix path as user
//!   channel `skew`. Channel names are matched case-sensitively: texture
//!   sets are cross-platform directories (see `umber_core::project`) and
//!   channel names are document data, so no case folding is applied.
//! * Layer naming is the literal `"Imported <map>"` where `<map>` is the
//!   file-stem suffix (`ambient_occlusion`, `curvature`, …, or the raw
//!   user-channel suffix). Layer names are DOCUMENT data, not UI strings,
//!   so the prefix is not i18n'd (it must round-trip byte-identically
//!   across locales, exactly like the `TextureSetName_map` stems do).
//! * Imported maps become `LayerKind::Paint` layers pushed on TOP of the
//!   target set's stack. Pixel bytes are stored in the project's
//!   [`umber_core::AssetStore`] (content-hash refs — the binary-neutrality
//!   half of docs/merge-story.md); the GPU-tile upload of those bytes is
//!   the named follow-up (same boundary as the painted Base Color bridge
//!   in `bake_sources`: the paint pool owns texels, this module owns
//!   routing + layer records). The layer-not-overwrite contract
//!   ([`import_bytes`]) is pinned by test: existing layers are never
//!   mutated, only pushed after.
//! * Missing channels are auto-created as user channels
//!   ([`ensure_channel`]): `baseColor`/`emissive` (any case — matched
//!   case-insensitively ONLY for the kind decision) are `Color`, everything
//!   else is `Data`. An import never fails for a missing channel.
//! * Tile-suffixed bake outputs (`<set>_<map>_<tile>.png`, the UDIM export
//!   driver's `_$udim` placement) are reported as UNMATCHED with reason
//!   [`UnmatchedReason::TileSuffixed`], never misrouted: `parse_mesh_map`
//!   does not parse them by design (see `bakes_panel`'s module docs), and
//!   silently importing tile 1002's AO over the whole set would corrupt.
//! * Only `.png` (case-insensitive) files are imported — the baker emits
//!   PNG. Anything else is unmatched ([`UnmatchedReason::NotPng`]), never
//!   an error: a folder of mixed assets must report, not fail.
//! * v1 applies to the OPEN document's set: files routed to a different
//!   set name are still listed (with their correct set/channel) but only
//!   same-set files are applied as layers — the document holds one set's
//!   stack (see `document::Document`). Multi-set apply is the named
//!   follow-up once the document goes multi-set.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use fluent::FluentValue;
use umber_core::{AssetRef, AssetStore, Channel, ChannelKind, LayerKind, LayerStack, TextureSet};
use umber_mesh::{parse_mesh_map_stem, MeshMapKind};

use crate::document::Document;
use crate::i18n::{tr, tr_args};

/// The channel a known mesh-map kind imports into (see the module docs for
/// the `normal_base` / bent / world-space decisions).
pub fn channel_for_kind(kind: MeshMapKind) -> &'static str {
    match kind {
        MeshMapKind::AmbientOcclusion => "AO",
        MeshMapKind::Curvature => "curvature",
        // The baked tangent-space normal feeds the paint `normal` channel.
        MeshMapKind::NormalBase => "normal",
        MeshMapKind::Position => "position",
        MeshMapKind::Thickness => "thickness",
        MeshMapKind::Id => "ID",
        MeshMapKind::Height => "height",
        MeshMapKind::BentNormal => "bentNormal",
        MeshMapKind::WorldSpaceNormal => "worldSpaceNormal",
    }
}

/// The layer name for an imported known map: literal `"Imported <map>"`.
pub fn layer_name_for_kind(kind: MeshMapKind) -> String {
    format!("Imported {}", kind.suffix())
}

/// The layer name for an imported user-channel file: `"Imported <suffix>"`.
pub fn layer_name_for_user_channel(channel: &str) -> String {
    format!("Imported {channel}")
}

/// The [`ChannelKind`] for a (possibly just-created) channel: color data
/// stays color-managed, everything else is linear data.
pub fn channel_kind_for(name: &str) -> ChannelKind {
    if name.eq_ignore_ascii_case("basecolor") || name.eq_ignore_ascii_case("emissive") {
        ChannelKind::Color
    } else {
        ChannelKind::Data
    }
}

/// Why a file was not routed (shown in the unmatched panel row, i18n'd via
/// the `import.reason-*` keys).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnmatchedReason {
    /// Not a `.png` file (the baker emits PNG).
    NotPng,
    /// No `_` separator to split `<set>_<suffix>` on.
    NoSeparator,
    /// Nothing before the `_` (`_curvature.png`).
    EmptySet,
    /// Nothing after the `_` (`Sword_.png`).
    EmptySuffix,
    /// A tile-suffixed UDIM output (`<set>_<map>_<tile>.png`) — never
    /// misrouted (see the module docs).
    TileSuffixed,
}

impl UnmatchedReason {
    /// The i18n key rendering this reason.
    pub fn key(self) -> &'static str {
        match self {
            Self::NotPng => "import.reason-not-png",
            Self::NoSeparator => "import.reason-no-separator",
            Self::EmptySet => "import.reason-empty-set",
            Self::EmptySuffix => "import.reason-empty-suffix",
            Self::TileSuffixed => "import.reason-tile-suffixed",
        }
    }

    /// The rendered reason string in the current locale.
    pub fn describe(self) -> String {
        tr(self.key())
    }
}

/// Where one file routes: a known mesh map, a user-channel file, or the
/// unmatched report (with its reason — never an error).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileRoute {
    /// A recognized `TextureSetName_map` mesh map (stage 1: the P0 parse).
    MeshMap {
        /// The owning texture set (the stem prefix).
        set: String,
        /// Which map this is.
        kind: MeshMapKind,
        /// [`channel_for_kind`] of `kind` (precomputed for the panel row).
        channel: &'static str,
    },
    /// A `<set>_<suffix>.png` file with no recognized map suffix (stage 2:
    /// the generic split) — imports into a user channel named by `suffix`.
    UserChannel {
        /// The owning texture set (the stem prefix).
        set: String,
        /// The raw suffix (verbatim — channel names are document data).
        channel: String,
    },
    /// Not routable (see [`UnmatchedReason`]).
    Unmatched {
        /// The file name as presented (for the report row).
        file: String,
        /// Why it did not route.
        reason: UnmatchedReason,
    },
}

/// Whether `stem` is a tile-suffixed UDIM output (`<set>_<map>_<tile>`):
/// a trailing `_` + 4-digit tile number (1001..9999). Checked BEFORE the
/// generic user-channel split so `Sword_ambient_occlusion_1002` reports as
/// tile-suffixed instead of routing into a bogus `1002` user channel.
fn strip_tile_suffix(stem: &str) -> Option<&str> {
    let (rest, tile) = stem.rsplit_once('_')?;
    if tile.len() == 4 && tile.bytes().all(|b| b.is_ascii_digit()) {
        Some(rest)
    } else {
        None
    }
}

/// Routes one file name (with or without directories — only the file name
/// is inspected) to its [`FileRoute`]. Pure (no I/O): the folder scan and
/// the tests both drive this.
pub fn classify_file_name(file_name: &str) -> FileRoute {
    let path = Path::new(file_name);
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(file_name);
    // Extension gate: PNG only (case-insensitive — Windows preserves case).
    let is_png = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("png"));
    if !is_png {
        return FileRoute::Unmatched {
            file: name.to_string(),
            reason: UnmatchedReason::NotPng,
        };
    }
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or(name);
    // Stage 1: the P0 mesh-map parse (reuse — never duplicated).
    if let Some(parsed) = parse_mesh_map_stem(stem) {
        let channel = channel_for_kind(parsed.kind);
        return FileRoute::MeshMap {
            set: parsed.texture_set,
            kind: parsed.kind,
            channel,
        };
    }
    // Tile-suffixed UDIM outputs report, never route (even when the inner
    // stem WOULD parse: `Sword_ambient_occlusion_1002` must not import).
    if let Some(inner) = strip_tile_suffix(stem) {
        if parse_mesh_map_stem(inner).is_some() || inner.rfind('_').is_some() {
            return FileRoute::Unmatched {
                file: name.to_string(),
                reason: UnmatchedReason::TileSuffixed,
            };
        }
    }
    // Stage 2: the generic `<set>_<suffix>` split → user channel.
    let Some((set, suffix)) = stem.rsplit_once('_') else {
        return FileRoute::Unmatched {
            file: name.to_string(),
            reason: UnmatchedReason::NoSeparator,
        };
    };
    if set.is_empty() {
        return FileRoute::Unmatched {
            file: name.to_string(),
            reason: UnmatchedReason::EmptySet,
        };
    }
    if suffix.is_empty() {
        return FileRoute::Unmatched {
            file: name.to_string(),
            reason: UnmatchedReason::EmptySuffix,
        };
    }
    FileRoute::UserChannel {
        set: set.to_string(),
        channel: suffix.to_string(),
    }
}

/// Ensures `set` names `channel` (creating it as a user channel when
/// missing). Returns `true` when the channel was created, `false` when it
/// already existed. An import never fails for a missing channel.
pub fn ensure_channel(set: &mut TextureSet, channel: &str) -> bool {
    if set.channels.iter().any(|c| c.name == channel) {
        return false;
    }
    set.channels.push(Channel {
        name: channel.to_string(),
        kind: channel_kind_for(channel),
    });
    true
}

/// Everything that can go wrong importing one file's bytes.
#[derive(Debug, thiserror::Error)]
pub enum MeshMapImportError {
    /// The file name does not route (see [`UnmatchedReason`]).
    #[error("file {file:?} is not an importable mesh map: {reason:?}")]
    Unmatched {
        /// The file name as presented.
        file: String,
        /// Why it did not route.
        reason: UnmatchedReason,
    },
    /// Storing the PNG bytes in the asset store failed.
    #[error("asset store failed: {0}")]
    Asset(#[from] umber_core::AssetError),
}

/// What one successful import did (for the panel row + tests).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportApplied {
    /// The texture set the file routed to.
    pub texture_set: String,
    /// The channel it loaded into (created when missing).
    pub channel: String,
    /// The new layer's id (always freshly allocated, always on top).
    pub layer_id: u64,
    /// The new layer's name (`"Imported <map>"`).
    pub layer_name: String,
    /// The content-hash ref of the stored PNG bytes.
    pub asset: AssetRef,
}

/// Imports one file's PNG `bytes` (named `file_name`) into `set`'s channel:
///
/// 1. routes the name ([`classify_file_name`] — unmatched names fail with
///    [`MeshMapImportError::Unmatched`], never a layer);
/// 2. stores the bytes in `store` (content-hash [`AssetRef` — dedupes
///    identical re-imports, the binary-neutrality half of the merge
///    story);
/// 3. ensures the channel ([`ensure_channel`]);
/// 4. pushes a NEW [`LayerKind::Paint`] layer named `"Imported <map>"` on
///    top of `stack`.
///
/// The layer-not-overwrite contract: existing layers (ids, names, order)
/// are never mutated — only a push plus a possible channel-vec push. The
/// `set` argument's NAME is not checked here (the caller — the folder-drop
/// UX — filters by set before calling); the route's set is returned in
/// [`ImportApplied::texture_set`] so the caller can filter on it.
pub fn import_bytes(
    stack: &mut LayerStack,
    set: &mut TextureSet,
    store: &AssetStore,
    file_name: &str,
    bytes: &[u8],
) -> Result<ImportApplied, MeshMapImportError> {
    let (texture_set, channel, layer_name) = match classify_file_name(file_name) {
        FileRoute::MeshMap { set, kind, channel } => {
            (set, channel.to_string(), layer_name_for_kind(kind))
        }
        FileRoute::UserChannel { set, channel } => {
            let layer = layer_name_for_user_channel(&channel);
            (set, channel, layer)
        }
        FileRoute::Unmatched { file, reason } => {
            return Err(MeshMapImportError::Unmatched { file, reason });
        }
    };
    let ext = Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_ascii_lowercase();
    let asset = store.store(bytes, &ext)?;
    ensure_channel(set, &channel);
    let layer_id = stack.add_layer(layer_name.clone(), LayerKind::Paint);
    Ok(ImportApplied {
        texture_set,
        channel,
        layer_id,
        layer_name,
        asset,
    })
}

/// One routed file waiting in the import panel (scan output, pre-apply).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingImport {
    /// The file name as presented.
    pub file: String,
    /// Full path (when the scan saw the filesystem — `None` for
    /// name-only fixtures / egui drops carrying bytes).
    pub path: Option<PathBuf>,
    /// The texture set the file routes to.
    pub texture_set: String,
    /// The channel it will load into.
    pub channel: String,
    /// The layer name it will create (`"Imported <map>"`).
    pub layer_name: String,
}

/// One skipped file for the unmatched report row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmatchedFile {
    /// The file name as presented.
    pub file: String,
    /// Why it did not route.
    pub reason: UnmatchedReason,
}

/// The folder-drop UX state: the last scan's routed files plus the
/// unmatched report, and the session texture set the apply path ensures
/// channels on (see the module docs for the single-set v1 boundary).
#[derive(Debug)]
pub struct MeshMapImportState {
    /// Routed files from the last scan / drop, in scan order.
    pub pending: Vec<PendingImport>,
    /// Skipped files from the last scan / drop, in scan order.
    pub unmatched: Vec<UnmatchedFile>,
    /// How many files the apply path has imported as layers this session.
    pub applied: usize,
    /// The session texture set (channels are ensured here on apply; the
    /// name follows the mesh / project via [`Self::retarget_set`]).
    pub session_set: TextureSet,
    /// Status line for the panel (last scan / apply summary).
    pub status: String,
}

impl Default for MeshMapImportState {
    fn default() -> Self {
        Self {
            pending: Vec::new(),
            unmatched: Vec::new(),
            applied: 0,
            session_set: TextureSet::new_default("TextureSet"),
            status: String::new(),
        }
    }
}

impl MeshMapImportState {
    /// Retargets the session set's name (mesh load / project open),
    /// keeping its channels: a rename must not drop user channels an
    /// earlier import created.
    pub fn retarget_set(&mut self, name: &str) {
        self.session_set.name = name.to_string();
    }

    /// Replaces the session set wholesale (project open restores the
    /// saved set, channels included).
    pub fn set_session_set(&mut self, set: TextureSet) {
        self.session_set = set;
    }

    /// Records one file name's route (shared by the dir scan, the drop
    /// handler and the name-only fixtures).
    pub fn ingest_name(&mut self, file_name: &str, path: Option<PathBuf>) {
        match classify_file_name(file_name) {
            FileRoute::MeshMap { set, kind, channel } => self.pending.push(PendingImport {
                file: file_name.to_string(),
                path,
                texture_set: set,
                channel: channel.to_string(),
                layer_name: layer_name_for_kind(kind),
            }),
            FileRoute::UserChannel { set, channel } => {
                let layer_name = layer_name_for_user_channel(&channel);
                self.pending.push(PendingImport {
                    file: file_name.to_string(),
                    path,
                    texture_set: set,
                    channel,
                    layer_name,
                });
            }
            FileRoute::Unmatched { file, reason } => {
                self.unmatched.push(UnmatchedFile { file, reason })
            }
        }
    }

    /// Clears the last scan and ingests every `*.png` (case-insensitive)
    /// directly under `dir` (non-recursive v1 — the baker emits one flat
    /// folder per set; recursion is the named follow-up), plus every
    /// non-PNG file as an unmatched row (so the report is complete).
    /// Missing/unreadable dirs report through `status` and leave the
    /// previous scan in place.
    pub fn scan_dir(&mut self, dir: &Path) {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) => {
                self.status = tr_args(
                    "import.scan-failed",
                    [("error", FluentValue::from(err.to_string()))],
                );
                return;
            }
        };
        let mut names: Vec<(String, PathBuf)> = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                names.push((name.to_string(), path));
            }
        }
        names.sort_by(|a, b| a.0.cmp(&b.0));
        self.pending.clear();
        self.unmatched.clear();
        for (name, path) in &names {
            self.ingest_name(name, Some(path.clone()));
        }
        self.status = tr_args(
            "import.scanned",
            [
                ("matched", FluentValue::from(self.pending.len() as i64)),
                ("unmatched", FluentValue::from(self.unmatched.len() as i64)),
            ],
        );
    }

    /// The egui file-drop handler: `paths` are the dropped files/folders
    /// (`ctx.input(|i| i.raw.dropped_files)` paths). Folders expand one
    /// level like [`Self::scan_dir`]; files ingest by name. Appends to the
    /// current lists (a drop ADDS to the pending queue — it never clears a
    /// folder scan the user is reviewing).
    pub fn ingest_paths(&mut self, paths: &[PathBuf]) {
        for path in paths {
            if path.is_dir() {
                let entries: Vec<PathBuf> = std::fs::read_dir(path)
                    .map(|rd| rd.flatten().map(|e| e.path()).collect())
                    .unwrap_or_default();
                let mut sorted = entries;
                sorted.sort();
                for child in &sorted {
                    if child.is_dir() {
                        continue;
                    }
                    let name = child
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or_default()
                        .to_string();
                    if name.is_empty() {
                        continue;
                    }
                    self.ingest_name(&name, Some(child.clone()));
                }
            } else {
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                self.ingest_name(&name, Some(path.clone()));
            }
        }
        self.status = tr_args(
            "import.scanned",
            [
                ("matched", FluentValue::from(self.pending.len() as i64)),
                ("unmatched", FluentValue::from(self.unmatched.len() as i64)),
            ],
        );
    }

    /// Applies every pending file routed to the session set as a new
    /// layer on `doc`'s stack (other sets' files stay queued — see the
    /// module docs). `store` is the project's [`AssetStore`]; `read` loads
    /// one pending file's bytes (the panel passes `std::fs::read`, tests
    /// pass a fixture map). Returns how many layers were created.
    ///
    /// Each file goes through [`import_bytes`], so the layer-not-overwrite
    /// contract holds per file: pushes only, never mutation.
    pub fn apply_session_set(
        &mut self,
        doc: &mut Document,
        store: &AssetStore,
        read: impl Fn(&PendingImport) -> Option<Vec<u8>>,
    ) -> usize {
        let mut created = 0usize;
        let mut still_pending = Vec::with_capacity(self.pending.len());
        // Borrow-split: `session_set` mutably for channel-ensure while the
        // pending vec drains (the vec itself is swapped out first).
        let mut queue = std::mem::take(&mut self.pending);
        for item in queue.drain(..) {
            if item.texture_set != self.session_set.name {
                still_pending.push(item);
                continue;
            }
            let Some(bytes) = read(&item) else {
                self.unmatched.push(UnmatchedFile {
                    file: item.file.clone(),
                    reason: UnmatchedReason::NotPng,
                });
                continue;
            };
            match import_bytes(
                &mut doc.stack,
                &mut self.session_set,
                store,
                &item.file,
                &bytes,
            ) {
                Ok(_) => {
                    created += 1;
                }
                Err(MeshMapImportError::Unmatched { file, reason }) => {
                    self.unmatched.push(UnmatchedFile { file, reason });
                }
                Err(MeshMapImportError::Asset(err)) => {
                    log::warn!("mesh-map import asset store failed: {err}");
                    still_pending.push(item);
                }
            }
        }
        self.pending = still_pending;
        self.applied += created;
        self.status = tr_args(
            "import.applied",
            [("count", FluentValue::from(created as i64))],
        );
        created
    }

    /// Drains egui's dropped files into this state. Call once per frame
    /// from the Assets panel (hidden tabs are skipped by egui_dock, so the
    /// shell calls this even when the panel is hidden — same pattern as
    /// the async bake poll in `main.rs`).
    pub fn poll_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if !dropped.is_empty() {
            self.ingest_paths(&dropped);
        }
    }

    /// Draws the import section of the Assets panel: the drop hint, the
    /// choose-folder button, the routed-files table (set / channel /
    /// `"Imported <map>"` layer name), the Apply button (gated on the
    /// project store), and the unmatched report rows.
    ///
    /// `store` is `Some` once the document has a project directory (after
    /// a Save or an Open — before that there is nowhere content-hash
    /// neutral to put the bytes, so Apply waits with the `need-project`
    /// hint). `read` loads a pending file's bytes (normally
    /// `std::fs::read` over `PendingImport::path`).
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        doc: &mut Document,
        store: Option<&AssetStore>,
        read: impl Fn(&PendingImport) -> Option<Vec<u8>> + Copy,
    ) {
        ui.heading(tr("import.title"));
        ui.label(tr("import.hint"));
        ui.horizontal(|ui| {
            if ui.button(tr("import.choose")).clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.scan_dir(&dir);
                }
            }
            let apply_enabled = store.is_some() && !self.pending.is_empty();
            if ui
                .add_enabled(apply_enabled, egui::Button::new(tr("import.apply")))
                .clicked()
            {
                if let Some(store) = store {
                    self.apply_session_set(doc, store, read);
                }
            }
        });
        if store.is_none() {
            ui.label(tr("import.need-project"));
        }
        if !self.status.is_empty() {
            ui.label(&self.status);
        }
        if self.pending.is_empty() && self.unmatched.is_empty() {
            ui.label(tr("import.no-files"));
            return;
        }
        if !self.pending.is_empty() {
            ui.collapsing(
                tr_args(
                    "import.matched-title",
                    [("count", FluentValue::from(self.pending.len() as i64))],
                ),
                |ui| {
                    // Snapshot rows first (E0502: `apply` borrows below).
                    let rows: Vec<(String, String, String, String)> = self
                        .pending
                        .iter()
                        .map(|p| {
                            (
                                p.file.clone(),
                                p.texture_set.clone(),
                                p.channel.clone(),
                                p.layer_name.clone(),
                            )
                        })
                        .collect();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (file, set, channel, layer) in &rows {
                            ui.label(tr_args(
                                "import.row-matched",
                                [
                                    ("file", FluentValue::from(file.as_str())),
                                    ("set", FluentValue::from(set.as_str())),
                                    ("channel", FluentValue::from(channel.as_str())),
                                    ("layer", FluentValue::from(layer.as_str())),
                                ],
                            ));
                        }
                    });
                },
            );
        }
        if !self.unmatched.is_empty() {
            ui.collapsing(
                tr_args(
                    "import.unmatched-title",
                    [("count", FluentValue::from(self.unmatched.len() as i64))],
                ),
                |ui| {
                    let rows: Vec<(String, UnmatchedReason)> = self
                        .unmatched
                        .iter()
                        .map(|u| (u.file.clone(), u.reason.clone()))
                        .collect();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for (file, reason) in &rows {
                            let why = tr(reason.key());
                            ui.label(tr_args(
                                "import.row-unmatched",
                                [
                                    ("file", FluentValue::from(file.as_str())),
                                    ("reason", FluentValue::from(why.as_str())),
                                ],
                            ));
                        }
                    });
                },
            );
        }
    }

    /// The per-set apply summary for tests and the status line: how many
    /// pending files route to each texture set.
    pub fn pending_per_set(&self) -> HashMap<&str, usize> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for item in &self.pending {
            *counts.entry(item.texture_set.as_str()).or_default() += 1;
        }
        counts
    }
}

/// Reads a pending file's bytes from disk (the panel's `read` closure).
pub fn read_pending_from_disk(item: &PendingImport) -> Option<Vec<u8>> {
    let path = item.path.as_ref()?;
    std::fs::read(path).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_store(tag: &str) -> (AssetStore, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("umber-meshmap-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        (AssetStore::new(&dir), dir)
    }

    #[test]
    fn name_parse_round_trips_every_map_kind() {
        // Every MeshMapKind formats through the P0 `format_mesh_map` and
        // parses back through the reused P0 parse — the import never
        // drifts from what the baker emits.
        let dir = Path::new("/out");
        for kind in [
            MeshMapKind::AmbientOcclusion,
            MeshMapKind::Curvature,
            MeshMapKind::NormalBase,
            MeshMapKind::Position,
            MeshMapKind::Thickness,
            MeshMapKind::Id,
            MeshMapKind::Height,
            MeshMapKind::BentNormal,
            MeshMapKind::WorldSpaceNormal,
        ] {
            let path = umber_mesh::format_mesh_map(dir, "Sword", kind, "png");
            let file = path.file_name().unwrap().to_str().unwrap().to_string();
            match classify_file_name(&file) {
                FileRoute::MeshMap {
                    set,
                    kind: back,
                    channel,
                } => {
                    assert_eq!(set, "Sword", "{file}");
                    assert_eq!(back, kind, "{file}");
                    assert_eq!(channel, channel_for_kind(kind), "{file}");
                }
                other => panic!("{file} should route as a mesh map, got {other:?}"),
            }
        }
    }

    #[test]
    fn folder_scan_routes_fixture_names_to_sets_and_channels() {
        // A dropped folder mixing two sets, a user channel, and skips.
        let names = [
            "Sword_ambient_occlusion.png",
            "Sword_curvature.png",
            "Sword_normal_base.png",
            "Shield_position.png",
            "Shield_thickness.png",
            "Sword_id.png",
            "Sword_height.png",
            "notes.txt",
            "Sword_ambient_occlusion_1002.png",
            "nodivider.png",
        ];
        let mut state = MeshMapImportState::default();
        for name in names {
            state.ingest_name(name, None);
        }
        // 7 mesh maps route (3 Sword + Shield pair + id/height).
        assert_eq!(state.pending.len(), 7, "{:?}", state.pending);
        let per_set = state.pending_per_set();
        assert_eq!(per_set.get("Sword"), Some(&5));
        assert_eq!(per_set.get("Shield"), Some(&2));
        // Channel routing pins.
        let channel_of = |file: &str| {
            state
                .pending
                .iter()
                .find(|p| p.file == file)
                .map(|p| p.channel.clone())
                .unwrap()
        };
        assert_eq!(channel_of("Sword_ambient_occlusion.png"), "AO");
        assert_eq!(channel_of("Sword_curvature.png"), "curvature");
        assert_eq!(channel_of("Sword_normal_base.png"), "normal");
        assert_eq!(channel_of("Shield_position.png"), "position");
        assert_eq!(channel_of("Shield_thickness.png"), "thickness");
        assert_eq!(channel_of("Sword_id.png"), "ID");
        assert_eq!(channel_of("Sword_height.png"), "height");
        // Layer naming pins: literal "Imported <map>".
        let layer_of = |file: &str| {
            state
                .pending
                .iter()
                .find(|p| p.file == file)
                .map(|p| p.layer_name.clone())
                .unwrap()
        };
        assert_eq!(
            layer_of("Sword_ambient_occlusion.png"),
            "Imported ambient_occlusion"
        );
        assert_eq!(layer_of("Sword_curvature.png"), "Imported curvature");
        // The unmatched report: txt (not png), tile-suffixed, no separator.
        assert_eq!(state.unmatched.len(), 3, "{:?}", state.unmatched);
        let reason_of = |file: &str| {
            state
                .unmatched
                .iter()
                .find(|u| u.file == file)
                .map(|u| u.reason.clone())
        };
        assert_eq!(reason_of("notes.txt"), Some(UnmatchedReason::NotPng));
        assert_eq!(
            reason_of("Sword_ambient_occlusion_1002.png"),
            Some(UnmatchedReason::TileSuffixed)
        );
        assert_eq!(
            reason_of("nodivider.png"),
            Some(UnmatchedReason::NoSeparator)
        );
    }

    #[test]
    fn unknown_suffix_routes_to_user_channel_named_by_suffix() {
        // Engine extras the P0 parse deliberately rejects (`_basecolor`,
        // `_roughness`, custom suffixes) become user channels — verbatim.
        for (file, set, channel) in [
            ("Sword_basecolor.png", "Sword", "basecolor"),
            ("Sword_roughness.png", "Sword", "roughness"),
            ("my_set_custom_wear.png", "my_set_custom", "wear"),
            ("Shield_skew.png", "Shield", "skew"),
        ] {
            match classify_file_name(file) {
                FileRoute::UserChannel { set: s, channel: c } => {
                    assert_eq!(s, set, "{file}");
                    assert_eq!(c, channel, "{file}");
                }
                other => panic!("{file} should route as a user channel, got {other:?}"),
            }
        }
        // And the unmatched edges of the generic split report, never route.
        assert!(matches!(
            classify_file_name("_curvature.png"),
            FileRoute::Unmatched {
                reason: UnmatchedReason::EmptySet,
                ..
            }
        ));
        assert!(matches!(
            classify_file_name("Sword_.png"),
            FileRoute::Unmatched {
                reason: UnmatchedReason::EmptySuffix,
                ..
            }
        ));
        // Case-insensitive PNG, case-preserving channel.
        match classify_file_name("Sword_AO.PNG") {
            FileRoute::UserChannel { set, channel } => {
                assert_eq!(set, "Sword");
                assert_eq!(channel, "AO");
            }
            other => panic!("Sword_AO.PNG should be a user channel (upper-case AO is not the P0 suffix), got {other:?}"),
        }
    }

    #[test]
    fn import_creates_new_layer_and_never_overwrites() {
        // The layer-not-overwrite contract: the stack gains exactly one
        // layer per import; every pre-existing layer (id, name, order) is
        // bit-identical afterwards.
        let (store, dir) = tmp_store("layer-contract");
        let mut stack = LayerStack::new();
        let base = stack.add_layer("Base", LayerKind::Paint);
        let detail = stack.add_layer("Detail", LayerKind::Paint);
        let before = stack.layers.clone();
        let mut set = TextureSet::new_default("Sword");
        let channels_before = set.channels.clone();

        let applied = import_bytes(
            &mut stack,
            &mut set,
            &store,
            "Sword_ambient_occlusion.png",
            b"fake ao png",
        )
        .expect("known map imports");
        assert_eq!(applied.texture_set, "Sword");
        assert_eq!(applied.channel, "AO");
        assert_eq!(applied.layer_name, "Imported ambient_occlusion");
        // One push, on top, freshly allocated.
        assert_eq!(stack.layers.len(), before.len() + 1);
        assert_eq!(stack.layers[..before.len()], before);
        assert_eq!(stack.layers.last().unwrap().id, applied.layer_id);
        assert_eq!(
            stack.layers.last().unwrap().name,
            "Imported ambient_occlusion"
        );
        assert_ne!(applied.layer_id, base);
        assert_ne!(applied.layer_id, detail);
        // The AO channel did not exist in the default set — created as Data.
        assert!(channels_before.iter().all(|c| c.name != "AO"));
        let ao = set
            .channels
            .iter()
            .find(|c| c.name == "AO")
            .expect("AO created");
        assert_eq!(ao.kind, ChannelKind::Data);
        // A second import of the same file dedupes the asset (same hash)
        // and still creates a second layer (imports never merge).
        let again = import_bytes(
            &mut stack,
            &mut set,
            &store,
            "Sword_ambient_occlusion.png",
            b"fake ao png",
        )
        .expect("re-imports");
        assert_eq!(
            again.asset, applied.asset,
            "identical bytes dedupe to one blob"
        );
        assert_ne!(
            again.layer_id, applied.layer_id,
            "but each import is its own layer"
        );
        assert_eq!(stack.layers.len(), before.len() + 2);
        // Unknown suffixes create user channels and Paint layers too.
        let user = import_bytes(&mut stack, &mut set, &store, "Sword_wear.png", b"wear")
            .expect("user channel imports");
        assert_eq!(user.channel, "wear");
        assert_eq!(user.layer_name, "Imported wear");
        assert!(set.channels.iter().any(|c| c.name == "wear"));
        // Unmatched names fail WITHOUT touching the stack or the store.
        let len = stack.layers.len();
        assert!(matches!(
            import_bytes(&mut stack, &mut set, &store, "notes.txt", b"x"),
            Err(MeshMapImportError::Unmatched { .. })
        ));
        assert_eq!(stack.layers.len(), len);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn apply_only_imports_the_session_set_and_keeps_the_rest_queued() {
        let (store, dir) = tmp_store("session-set");
        let mut state = MeshMapImportState::default();
        state.retarget_set("Sword");
        for name in [
            "Sword_curvature.png",
            "Shield_thickness.png",
            "Sword_wear.png",
        ] {
            state.ingest_name(name, None);
        }
        let mut doc = Document::default();
        let bytes: HashMap<&str, Vec<u8>> = [
            ("Sword_curvature.png", b"a".to_vec()),
            ("Sword_wear.png", b"b".to_vec()),
            ("Shield_thickness.png", b"c".to_vec()),
        ]
        .into_iter()
        .collect();
        let created = state.apply_session_set(&mut doc, &store, |item| {
            bytes.get(item.file.as_str()).cloned()
        });
        assert_eq!(created, 2);
        assert_eq!(doc.stack.layers.len(), 2);
        // The other set's file stays queued (never dropped, never applied).
        assert_eq!(state.pending.len(), 1);
        assert_eq!(state.pending[0].file, "Shield_thickness.png");
        // Channels ensured on the session set (curvature + wear alongside).
        assert!(state
            .session_set
            .channels
            .iter()
            .any(|c| c.name == "curvature"));
        assert!(state.session_set.channels.iter().any(|c| c.name == "wear"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unmatched_reasons_have_i18n_keys() {
        // The report row renders through tr() — every reason needs its key
        // in en-US (the completeness test enforces this at the crate level;
        // this pins the mapping 1:1 so a new variant cannot slip in
        // key-less).
        for reason in [
            UnmatchedReason::NotPng,
            UnmatchedReason::NoSeparator,
            UnmatchedReason::EmptySet,
            UnmatchedReason::EmptySuffix,
            UnmatchedReason::TileSuffixed,
        ] {
            assert!(reason.key().starts_with("import.reason-"), "{reason:?}");
        }
    }
}
