//! The `.umber` project format (requirements.md §8, architecture.md).
//!
//! A project is a directory:
//!
//! ```text
//! myproject.umber/
//!   project.json            # version, texture sets, per-set layer order, settings
//!   layers/<set>/<id>.json  # one JSON file per layer — diffable, mergeable
//!   journal.jsonl           # autosave journal: edits since the last save
//! ```
//!
//! `journal.jsonl` (see [`crate::journal`]) holds the mutations made since
//! `project.json` was written; `project.json`'s `journal_seq` is the
//! high-water mark of the entries it already reflects. [`load_from_dir`]
//! replays the entries past it (crash recovery), and [`save_to_dir`]
//! truncates the journal once the new state is committed.
//!
//! [`ProjectModel`] is the in-memory document: full [`Layer`] bodies, grouped
//! per texture set. On disk, layer bodies live in their own files and
//! `project.json` only records *which* layers belong to which set and in
//! what order (the private `ProjectFile`/`LayerSetOrder` schema) — that
//! split is exactly what makes the format diff- and merge-friendly.
//!
//! Save/load round-trip determinism is a hard acceptance criterion
//! (requirements.md §8: "save→load→export identical bytes"); see the
//! round-trip tests at the bottom of this file.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::journal;
use crate::layers::{Layer, LayerStack};
use crate::TextureSet;

/// The only `.umber` project format version this build understands.
pub const CURRENT_PROJECT_VERSION: u32 = 1;

/// Everything that can go wrong saving or loading a `.umber` project.
#[derive(Debug, Error)]
pub enum ProjectError {
    /// Reading or writing a file under the project directory failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// `project.json` or a layer file was not valid JSON for its expected shape.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// `project.json`'s `version` isn't one this build understands.
    #[error("unsupported project version {0} (this build understands version 1)")]
    UnsupportedVersion(u32),

    /// A texture-set name is empty, reserved, or unsafe as a directory name.
    #[error("invalid texture set name {0:?}")]
    InvalidSetName(String),

    /// Two texture sets share a name (case-insensitively — see
    /// `validate_set_name`).
    #[error("duplicate texture set name {0:?}")]
    DuplicateSetName(String),

    /// A per-set layer order names a texture set that doesn't exist.
    #[error("layer order references unknown texture set {0:?}")]
    UnknownSetInLayerOrder(String),

    /// The same layer id appears twice in one texture set's order.
    #[error("duplicate layer id {id} in texture set {set:?}")]
    DuplicateLayerId {
        /// The texture set the duplicate was found in.
        set: String,
        /// The id that appeared more than once.
        id: u64,
    },

    /// A layer id listed in a set's order has no `<id>.json` file on disk.
    #[error("missing layer file for id {id} in texture set {set:?}")]
    MissingLayerFile {
        /// The texture set the missing file belongs to.
        set: String,
        /// The id that has no corresponding file.
        id: u64,
    },

    /// A layer file's own `id` field doesn't match the order entry that
    /// pointed at it (e.g. the file was renamed or corrupted).
    #[error("layer file for id {expected} in texture set {set:?} actually contains id {found}")]
    LayerIdMismatch {
        /// The texture set the mismatch was found in.
        set: String,
        /// The id the order entry expected.
        expected: u64,
        /// The id the file actually contained.
        found: u64,
    },

    /// A layer id or a set's `next_layer_id` counter is `u64::MAX`, one
    /// increment away from overflowing.
    #[error("layer id overflow in texture set {0:?}")]
    LayerIdOverflow(String),
}

/// The tone-map curve family, as persisted: a plain-data mirror of
/// `umber_gpu::postfx::ToneMapMode`. `umber-core` takes no GPU dep (see
/// the workspace map), so the file schema mirrors the enum instead of
/// naming it; the exhaustive match in the app's carry conversion is the
/// drift guard — a variant added on either side breaks the build there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PostFxToneMapMode {
    /// The ACES 1.0 fitted curve.
    #[default]
    Aces,
    /// Hable's filmic shoulder.
    Filmic,
    /// The simple Reinhard curve.
    Reinhard,
}

/// One post-effect with its parameters, as persisted: a plain-data mirror
/// of `umber_gpu::postfx::PostEffect` (see [`PostFxToneMapMode`] for why
/// it is a mirror). Ranges are documented on the GPU side; the load path
/// clamps through `PostEffect::clamped`, so hand-edited files may carry
/// out-of-range values safely.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PostFxEffect {
    /// Depth-of-field: focus distance (world units) + aperture 0..=1.
    Dof { focus: f32, aperture: f32 },
    /// Bright-pass bloom: threshold + intensity (scene-linear) + blur radius.
    Bloom {
        threshold: f32,
        intensity: f32,
        radius: u32,
    },
    /// Radial vignette: edge darkening + inward reach.
    Vignette { strength: f32, softness: f32 },
    /// Unsharp-mask sharpen: blend amount + kernel radius.
    Sharpen { amount: f32, radius: u32 },
    /// Scene-linear tone-map: curve family + exposure stops.
    ToneMap {
        mode: PostFxToneMapMode,
        exposure: f32,
    },
}

/// The persisted post-effects chain (requirements §7:84): the effect list
/// in execution order plus the per-effect enable flags index-aligned with
/// it. Empty (the default) = no effects = the identity, so pre-postfx
/// files (no `postfx` key) load unchanged via `serde(default)` — the same
/// additive rule as `display` below. Versioned by the project version
/// stamp; no separate version field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PostFxCarry {
    /// The effects in execution order.
    pub effects: Vec<PostFxEffect>,
    /// Per-effect enable flags, index-aligned with `effects`. A short vec
    /// reads as enabled (absent ⇒ enabled — the load path's rule).
    pub enabled: Vec<bool>,
}

/// Project-wide settings that aren't scoped to a single texture set.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectSettings {
    /// Name of the texture set shown/edited when the project was last
    /// saved, if any. Purely a UI convenience — the document is valid
    /// without it.
    pub active_texture_set: Option<String>,
    /// The viewer chain (Display panel: view transform, exposure,
    /// gamma). Additive wave-5 carry: `default` loads pre-display
    /// files as the identity chain (Raw / 0 EV / gamma 1).
    #[serde(default)]
    pub display: umber_color::DisplaySettings,
    /// The post-effects chain (Display panel's postfx section).
    /// Additive carry: `default` loads pre-postfx files as the empty
    /// (identity) chain — the same rule as `display` above.
    #[serde(default)]
    pub postfx: PostFxCarry,
}

/// One texture set's layer stack, as stored in a [`ProjectModel`].
#[derive(Debug, Clone, PartialEq)]
pub struct TextureSetLayers {
    /// Must match the `name` of one entry in [`ProjectModel::texture_sets`].
    pub texture_set: String,
    /// The set's layer stack.
    pub stack: LayerStack,
}

/// The full in-memory `.umber` document.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectModel {
    /// The `.umber` format version this document was built at or loaded from.
    pub version: u32,
    /// Every texture set in the project.
    pub texture_sets: Vec<TextureSet>,
    /// Each texture set's layer stack.
    pub layers: Vec<TextureSetLayers>,
    /// Project-wide settings.
    pub settings: ProjectSettings,
    /// Procedural node graphs, one `.mtlx` document string each
    /// (wave-5 slice 5: the graph panel's carry — `umber-graph`'s
    /// `to_mtlx`/`from_mtlx` strings, kept opaque here so `umber-core`
    /// stays graph-agnostic). Empty for pre-graph projects; old files
    /// load unchanged via the `serde(default)` on the file schema.
    pub graphs_mtlx: Vec<String>,
    /// The autosave journal's high-water mark: the seq of the last
    /// `journal.jsonl` entry this state already reflects (0 = none).
    /// Saves stamp it from the live journal; recovery advances it.
    pub journal_seq: u64,
}

impl ProjectModel {
    /// Builds a project at [`CURRENT_PROJECT_VERSION`], with no graphs
    /// (use [`Self::with_graphs_mtlx`] to carry node graphs).
    pub fn new(
        texture_sets: Vec<TextureSet>,
        layers: Vec<TextureSetLayers>,
        settings: ProjectSettings,
    ) -> Self {
        Self {
            version: CURRENT_PROJECT_VERSION,
            texture_sets,
            layers,
            settings,
            graphs_mtlx: Vec::new(),
            journal_seq: 0,
        }
    }

    /// Attaches node-graph `.mtlx` strings (builder — the struct stays
    /// constructible through [`Self::new`] unchanged).
    #[must_use]
    pub fn with_graphs_mtlx(mut self, graphs_mtlx: Vec<String>) -> Self {
        self.graphs_mtlx = graphs_mtlx;
        self
    }

    /// Stamps the journal high-water mark (builder): a save from a
    /// journaled session passes its journal's last seq, so the entries
    /// the saved state already holds never replay on top of it.
    #[must_use]
    pub fn with_journal_seq(mut self, journal_seq: u64) -> Self {
        self.journal_seq = journal_seq;
        self
    }
}

/// On-disk shape of `project.json`. Deliberately distinct from
/// [`ProjectModel`]: layer *bodies* never appear here, only the per-set
/// ordering needed to reassemble them from `layers/<set>/<id>.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct ProjectFile {
    version: u32,
    texture_sets: Vec<TextureSet>,
    layer_sets: Vec<LayerSetOrder>,
    settings: ProjectSettings,
    /// Additive wave-5 carry (see [`ProjectModel::graphs_mtlx`]):
    /// `default` keeps pre-graph files loading unchanged.
    #[serde(default)]
    graphs_mtlx: Vec<String>,
    /// See [`ProjectModel::journal_seq`]. `default` (0) keeps
    /// pre-journal files loading unchanged.
    #[serde(default)]
    journal_seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct LayerSetOrder {
    texture_set: String,
    /// Stack order, bottom to top — matches `LayerStack::layers` order.
    layer_ids: Vec<u64>,
    /// Persisted rather than re-derived as `max(layer_ids) + 1` on load, so
    /// ids stay monotonic even if the highest-id layer was deleted before
    /// the save that produced this file.
    next_layer_id: u64,
}

/// Just enough of `project.json` to check the version before attempting to
/// parse the rest of the schema — so a future-version file fails with
/// [`ProjectError::UnsupportedVersion`] instead of an opaque JSON error.
#[derive(Debug, Deserialize)]
struct VersionProbe {
    version: u32,
}

/// Windows device names reserved regardless of extension (`CON`, `CON.txt`,
/// ... are all invalid). Compared case-insensitively.
const WINDOWS_RESERVED_STEMS: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Rejects anything unsafe as a texture-set *directory name* on Windows or
/// POSIX: path-traversal segments, reserved characters, trailing dot/space
/// (both silently stripped by Windows, which makes "Body" and "Body." the
/// same directory), and the Windows reserved device names.
///
/// Does not check for cross-name collisions (e.g. "Body" vs "body") — that
/// is a property of a *set* of names, not one name in isolation, and is
/// handled by [`validate_project_structure`].
fn validate_set_name(name: &str) -> Result<(), ProjectError> {
    const RESERVED_CHARS: &[char] = &['<', '>', ':', '"', '|', '?', '*', '/', '\\'];
    // Windows device names are reserved by their stem alone: "CON.txt" and
    // "nul.metal" are just as invalid as "CON" and "NUL".
    let stem = name.split_once('.').map_or(name, |(before, _)| before);
    let invalid = name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(RESERVED_CHARS)
        || name.ends_with('.')
        || name.ends_with(' ')
        || WINDOWS_RESERVED_STEMS.contains(&stem.to_ascii_lowercase().as_str());
    if invalid {
        return Err(ProjectError::InvalidSetName(name.to_string()));
    }
    Ok(())
}

/// Validates the structural invariants shared by [`save_to_dir`] and
/// [`load_from_dir`], so the two can never disagree about what counts as a
/// well-formed project: a version save/load both recognize, set names that
/// are each individually valid and pairwise unique (case-insensitively —
/// Windows directories collide on case, and this format may be cloned
/// cross-platform via git), every layer order naming a set that actually
/// exists, no duplicate layer ids within a set, and no id or counter one
/// increment away from overflowing.
///
/// Called *before* any path is joined or any file is touched, so a crafted
/// `texture_set` name (e.g. `"../../escape"`) is rejected on load rather
/// than used to read or write outside the project directory.
fn validate_project_structure(
    version: u32,
    texture_set_names: &[String],
    layer_sets: &[(String, Vec<u64>, u64)],
) -> Result<(), ProjectError> {
    if version != CURRENT_PROJECT_VERSION {
        return Err(ProjectError::UnsupportedVersion(version));
    }

    let mut seen_set_names = BTreeSet::new();
    for name in texture_set_names {
        validate_set_name(name)?;
        // Full Unicode case folding, not just ASCII: NTFS compares names
        // case-insensitively across the whole alphabet, not only A-Z.
        if !seen_set_names.insert(name.to_lowercase()) {
            return Err(ProjectError::DuplicateSetName(name.clone()));
        }
    }
    let known_sets: BTreeSet<&str> = texture_set_names.iter().map(String::as_str).collect();

    let mut seen_layer_set_names = BTreeSet::new();
    for (set_name, layer_ids, next_layer_id) in layer_sets {
        validate_set_name(set_name)?;
        if !seen_layer_set_names.insert(set_name.to_lowercase()) {
            return Err(ProjectError::DuplicateSetName(set_name.clone()));
        }
        if !known_sets.contains(set_name.as_str()) {
            return Err(ProjectError::UnknownSetInLayerOrder(set_name.clone()));
        }
        if *next_layer_id == u64::MAX {
            return Err(ProjectError::LayerIdOverflow(set_name.clone()));
        }

        let mut seen_ids = BTreeSet::new();
        for &id in layer_ids {
            if id == u64::MAX {
                return Err(ProjectError::LayerIdOverflow(set_name.clone()));
            }
            if !seen_ids.insert(id) {
                return Err(ProjectError::DuplicateLayerId {
                    set: set_name.clone(),
                    id,
                });
            }
        }
    }
    Ok(())
}

fn to_pretty_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, ProjectError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Writes `bytes` to `path` through a sibling temp file + rename, so
/// a crash leaves either the old file or the new one, never a torn one
/// (the [`crate::assets::AssetStore::store`] pattern).
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    let mut tmp_name = path.as_os_str().to_owned();
    tmp_name.push(format!(".tmp.{}", std::process::id()));
    let tmp = std::path::PathBuf::from(tmp_name);
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Removes everything under `layers_root` the just-saved `model` doesn't
/// reference: set directories of deleted sets, layer files of deleted
/// layers, and stray temp files. Set names compare case-insensitively
/// (the same rule as `validate_project_structure`): on a
/// case-insensitive filesystem a set renamed `body` → `Body` reuses the
/// old directory, which must not be pruned as stale.
fn prune_stale_layer_files(layers_root: &Path, model: &ProjectModel) -> Result<(), ProjectError> {
    let live: Vec<(String, BTreeSet<String>)> = model
        .layers
        .iter()
        .map(|entry| {
            (
                entry.texture_set.to_lowercase(),
                entry
                    .stack
                    .layers
                    .iter()
                    .map(|l| format!("{}.json", l.id))
                    .collect(),
            )
        })
        .collect();
    for set_entry in fs::read_dir(layers_root)? {
        let set_entry = set_entry?;
        let set_path = set_entry.path();
        let set_name = set_entry.file_name().to_string_lossy().to_lowercase();
        let live_files = live
            .iter()
            .find(|(name, _)| *name == set_name)
            .map(|(_, files)| files);
        match (set_path.is_dir(), live_files) {
            (true, Some(live_files)) => {
                for file in fs::read_dir(&set_path)? {
                    let file = file?;
                    let name = file.file_name().to_string_lossy().into_owned();
                    if !live_files.contains(&name) {
                        let path = file.path();
                        if path.is_dir() {
                            fs::remove_dir_all(&path)?;
                        } else {
                            fs::remove_file(&path)?;
                        }
                    }
                }
            }
            (true, None) => fs::remove_dir_all(&set_path)?,
            (false, _) => fs::remove_file(&set_path)?,
        }
    }
    Ok(())
}

/// Writes `model` to `dir`, creating it if needed.
///
/// A save into an existing project never leaves stale files behind —
/// neither for individual layers deleted from a set, nor for a texture
/// set deleted outright — and runs in a crash-safe order:
///
/// 1. every layer file is written (temp + rename each);
/// 2. `project.json` is written atomically — the commit point;
/// 3. stale layer files and set directories are pruned;
/// 4. the autosave journal is truncated (the saved state now holds it).
///
/// A crash before (2) leaves the old `project.json`, whose layer files
/// all still exist (nothing is deleted before the commit), plus the
/// intact journal; a crash after (2) leaves at worst stale files and a
/// journal whose entries are all at or below the new `journal_seq`. Not
/// fully transactional: a crash before (2) can leave a layer file
/// already holding its newer body under the old `project.json` — the
/// journal's absolute-state ops converge that on the next recovery.
pub fn save_to_dir(model: &ProjectModel, dir: &Path) -> Result<(), ProjectError> {
    let texture_set_names: Vec<String> = model
        .texture_sets
        .iter()
        .map(|ts| ts.name.clone())
        .collect();
    let layer_sets_for_validation: Vec<(String, Vec<u64>, u64)> = model
        .layers
        .iter()
        .map(|entry| {
            (
                entry.texture_set.clone(),
                entry.stack.layers.iter().map(|l| l.id).collect(),
                entry.stack.next_layer_id(),
            )
        })
        .collect();
    validate_project_structure(
        model.version,
        &texture_set_names,
        &layer_sets_for_validation,
    )?;

    fs::create_dir_all(dir)?;
    let layers_root = dir.join("layers");

    let mut layer_sets = Vec::with_capacity(model.layers.len());
    for entry in &model.layers {
        let set_dir = layers_root.join(&entry.texture_set);
        fs::create_dir_all(&set_dir)?;

        let mut layer_ids = Vec::with_capacity(entry.stack.layers.len());
        for layer in &entry.stack.layers {
            let bytes = to_pretty_json_bytes(layer)?;
            write_atomic(&set_dir.join(format!("{}.json", layer.id)), &bytes)?;
            layer_ids.push(layer.id);
        }

        layer_sets.push(LayerSetOrder {
            texture_set: entry.texture_set.clone(),
            layer_ids,
            next_layer_id: entry.stack.next_layer_id(),
        });
    }

    let file = ProjectFile {
        version: model.version,
        texture_sets: model.texture_sets.clone(),
        layer_sets,
        settings: model.settings.clone(),
        graphs_mtlx: model.graphs_mtlx.clone(),
        journal_seq: model.journal_seq,
    };
    write_atomic(&dir.join("project.json"), &to_pretty_json_bytes(&file)?)?;
    if layers_root.exists() {
        prune_stale_layer_files(&layers_root, model)?;
    }
    journal::truncate_journal(dir)?;
    Ok(())
}

/// Reads a project previously written by [`save_to_dir`].
pub fn load_from_dir(dir: &Path) -> Result<ProjectModel, ProjectError> {
    let bytes = fs::read(dir.join("project.json"))?;

    // Check the version against a minimal schema before attempting to parse
    // the rest, so a future-version project.json fails with
    // ProjectError::UnsupportedVersion rather than an opaque JSON error.
    let probe: VersionProbe = serde_json::from_slice(&bytes)?;
    if probe.version != CURRENT_PROJECT_VERSION {
        return Err(ProjectError::UnsupportedVersion(probe.version));
    }

    let file: ProjectFile = serde_json::from_slice(&bytes)?;

    let texture_set_names: Vec<String> =
        file.texture_sets.iter().map(|ts| ts.name.clone()).collect();
    let layer_sets_for_validation: Vec<(String, Vec<u64>, u64)> = file
        .layer_sets
        .iter()
        .map(|ls| {
            (
                ls.texture_set.clone(),
                ls.layer_ids.clone(),
                ls.next_layer_id,
            )
        })
        .collect();
    validate_project_structure(file.version, &texture_set_names, &layer_sets_for_validation)?;

    let mut layers = Vec::with_capacity(file.layer_sets.len());
    for layer_set in &file.layer_sets {
        let set_dir = dir.join("layers").join(&layer_set.texture_set);
        let mut stack_layers = Vec::with_capacity(layer_set.layer_ids.len());
        for &id in &layer_set.layer_ids {
            let layer_path = set_dir.join(format!("{id}.json"));
            let layer_bytes =
                fs::read(&layer_path).map_err(|_| ProjectError::MissingLayerFile {
                    set: layer_set.texture_set.clone(),
                    id,
                })?;
            let layer: Layer = serde_json::from_slice(&layer_bytes)?;
            if layer.id != id {
                return Err(ProjectError::LayerIdMismatch {
                    set: layer_set.texture_set.clone(),
                    expected: id,
                    found: layer.id,
                });
            }
            stack_layers.push(layer);
        }

        layers.push(TextureSetLayers {
            texture_set: layer_set.texture_set.clone(),
            stack: LayerStack::from_parts(stack_layers, layer_set.next_layer_id),
        });
    }

    let mut model = ProjectModel {
        version: file.version,
        texture_sets: file.texture_sets,
        layers,
        settings: file.settings,
        graphs_mtlx: file.graphs_mtlx,
        journal_seq: file.journal_seq,
    };
    recover_from_journal(&mut model, dir);
    Ok(model)
}

/// Crash recovery (docs/specs/autosave-journal-design.md §3): replays the
/// journal entries past `model.journal_seq`, then persists — `save_to_dir`
/// rewrites `project.json` with the advanced high-water mark and only then
/// truncates the journal. A crash at any point is safe: before the
/// `project.json` commit the next load replays the same entries onto the
/// same saved state; after it, every entry is at or below the new mark and
/// is skipped.
///
/// Best-effort by design, never an error: an unreadable journal loads the
/// saved state as-is, and a failed rewrite (read-only directory, full
/// disk) still returns the recovered model, leaving the journal in place
/// for the next load to replay again.
fn recover_from_journal(model: &mut ProjectModel, dir: &Path) {
    let Ok(entries) = journal::read_journal(dir) else {
        return;
    };
    if entries.is_empty() {
        return;
    }
    if journal::replay(model, &entries) == 0 {
        // Every entry already reflected: the crash fell between a
        // rewrite and its truncate. Finish the truncate.
        let _ = journal::truncate_journal(dir);
        return;
    }
    let _ = save_to_dir(model, dir);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::{BlendMode, LayerKind, LayerMask};
    use crate::{Channel, ChannelKind};
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_temp_dir(label: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "umber-core-test-{label}-{}-{n}",
            std::process::id()
        ))
    }

    /// Exercises every variant the task calls out: a folder with
    /// passthrough, a populated mask, fractional and edge-case opacities,
    /// multiple sets, several blend modes, and a non-default setting.
    fn fixture_model() -> ProjectModel {
        let set_a = TextureSet::new_default("Body");
        let set_b = TextureSet {
            name: "Helmet".to_string(),
            resolution: 4096,
            channels: vec![Channel {
                name: "baseColor".into(),
                kind: ChannelKind::Color,
            }],
            tiles: BTreeMap::new(),
        };

        let mut stack_a = LayerStack::new();
        let base = stack_a.add_layer("Base", LayerKind::Paint);
        stack_a.set_opacity(base, 0.1);
        stack_a.set_blend_mode(base, BlendMode::Multiply);

        let group = stack_a.add_layer("Details", LayerKind::Folder { passthrough: true });
        stack_a.set_opacity(group, 1.0 / 3.0);
        stack_a.set_blend_mode(group, BlendMode::Overlay);
        stack_a.layer_mut(group).unwrap().mask = Some(LayerMask {
            name: "Details Mask".into(),
            enabled: true,
        });

        let fill = stack_a.add_layer("Tint", LayerKind::Fill);
        stack_a.set_blend_mode(fill, BlendMode::SoftLight);
        stack_a.set_visible(fill, false);
        stack_a.remove_layer(fill).unwrap(); // bump next_id past a deleted layer

        let mut stack_b = LayerStack::new();
        stack_b.add_layer("Metal", LayerKind::Paint);

        ProjectModel::new(
            vec![set_a, set_b],
            vec![
                TextureSetLayers {
                    texture_set: "Body".into(),
                    stack: stack_a,
                },
                TextureSetLayers {
                    texture_set: "Helmet".into(),
                    stack: stack_b,
                },
            ],
            ProjectSettings {
                active_texture_set: Some("Body".into()),
                ..ProjectSettings::default()
            },
        )
    }

    #[test]
    fn project_file_json_bytes_are_deterministic_across_serializations() {
        let model = fixture_model();
        let file = ProjectFile {
            version: model.version,
            texture_sets: model.texture_sets.clone(),
            layer_sets: model
                .layers
                .iter()
                .map(|l| LayerSetOrder {
                    texture_set: l.texture_set.clone(),
                    layer_ids: l.stack.layers.iter().map(|x| x.id).collect(),
                    next_layer_id: l.stack.next_layer_id(),
                })
                .collect(),
            settings: model.settings.clone(),
            graphs_mtlx: model.graphs_mtlx.clone(),
            journal_seq: model.journal_seq,
        };

        let bytes_1 = to_pretty_json_bytes(&file).unwrap();
        let bytes_2 = to_pretty_json_bytes(&file).unwrap();
        assert_eq!(bytes_1, bytes_2);
    }

    #[test]
    fn save_load_save_round_trip_is_byte_identical() {
        let model = fixture_model();
        let dir_a = unique_temp_dir("a");
        let dir_b = unique_temp_dir("b");

        save_to_dir(&model, &dir_a).expect("first save");
        let loaded = load_from_dir(&dir_a).expect("load");
        assert_eq!(loaded, model, "loaded model must equal the original");

        save_to_dir(&loaded, &dir_b).expect("second save");
        assert_dirs_byte_identical(&dir_a, &dir_b);

        let _ = fs::remove_dir_all(&dir_a);
        let _ = fs::remove_dir_all(&dir_b);
    }

    /// A two-tile texture set: tile 1001 keeps the default channels at
    /// 2048, tile 1002 carries baseColor only at 1024.
    fn two_tile_texture_set() -> TextureSet {
        let mut set = TextureSet::new_default("Body");
        set.tiles.insert(
            1001,
            crate::TileChannels::new(set.resolution, set.channels.clone()),
        );
        set.tiles.insert(
            1002,
            crate::TileChannels::new(
                1024,
                vec![Channel {
                    name: "baseColor".into(),
                    kind: ChannelKind::Color,
                }],
            ),
        );
        set
    }

    #[test]
    fn save_load_save_round_trip_is_byte_identical_with_two_tile_set() {
        let mut model = fixture_model();
        model.texture_sets[0] = two_tile_texture_set();
        let dir_a = unique_temp_dir("tiles-a");
        let dir_b = unique_temp_dir("tiles-b");

        save_to_dir(&model, &dir_a).expect("first save");
        let loaded = load_from_dir(&dir_a).expect("load");
        assert_eq!(loaded, model, "loaded model must equal the original");
        assert_eq!(
            loaded.texture_sets[0].tiles, model.texture_sets[0].tiles,
            "the additive tiles field must survive the round trip"
        );
        assert_eq!(
            loaded.texture_sets[0]
                .tiles
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec![1001, 1002],
            "both tiles present, sorted (BTreeMap wire order)"
        );

        save_to_dir(&loaded, &dir_b).expect("second save");
        assert_dirs_byte_identical(&dir_a, &dir_b);

        let _ = fs::remove_dir_all(&dir_a);
        let _ = fs::remove_dir_all(&dir_b);
    }

    #[test]
    fn pre_tile_project_json_loads_with_implied_1001() {
        // Old files have no "tiles" key on texture sets: serde(default)
        // yields the empty map, which the TileSet conversion reads as
        // the implied 1001-only set.
        let dir = unique_temp_dir("pre-tile");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("project.json"),
            br#"{"version":1,"texture_sets":[{"name":"Body","resolution":2048,"channels":[{"name":"baseColor","kind":"Color"}]}],"layer_sets":[{"texture_set":"Body","layer_ids":[],"next_layer_id":0}],"settings":{"active_texture_set":null}}"#,
        )
        .unwrap();

        let loaded = load_from_dir(&dir).expect("pre-tile file must load");
        assert!(
            loaded.texture_sets[0].tiles.is_empty(),
            "no tiles key -> empty map (1001 implied)"
        );
        let tiled = crate::TileSet::from(&loaded.texture_sets[0]);
        assert_eq!(tiled.tiles_present(), vec![1001]);
        assert_eq!(tiled.tile(1001).unwrap().resolution, 2048);
        assert_eq!(tiled.tile(1001).unwrap().channels.len(), 1);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn pre_display_project_json_loads_identity_display() {
        // Old files have no "display" key in settings: serde(default)
        // yields the identity chain (Raw / 0 EV / gamma 1) — the
        // additive rule.
        let dir = unique_temp_dir("pre-display");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("project.json"),
            br#"{"version":1,"texture_sets":[{"name":"Body","resolution":2048,"channels":[]}],"layer_sets":[{"texture_set":"Body","layer_ids":[],"next_layer_id":0}],"settings":{"active_texture_set":"Body"}}"#,
        )
        .unwrap();

        let loaded = load_from_dir(&dir).expect("pre-display file must load");
        assert_eq!(loaded.settings.active_texture_set.as_deref(), Some("Body"));
        assert_eq!(
            loaded.settings.display,
            umber_color::DisplaySettings::default()
        );
        assert_eq!(
            loaded.settings.display.view,
            umber_color::DisplayTransform::None
        );
        assert_eq!(loaded.settings.display.exposure, 0.0);
        assert_eq!(loaded.settings.display.gamma, 1.0);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn display_settings_round_trip_byte_identical() {
        // Default settings: save -> load -> save is byte-identical, and
        // the default chain lands on disk under its stable names.
        let model = fixture_model();
        let dir_a = unique_temp_dir("display-default-a");
        let dir_b = unique_temp_dir("display-default-b");
        save_to_dir(&model, &dir_a).expect("save default");
        let json = fs::read_to_string(dir_a.join("project.json")).unwrap();
        assert!(json.contains(r#""view": "Raw""#), "{json}");
        let loaded = load_from_dir(&dir_a).expect("load default");
        assert_eq!(loaded, model);
        save_to_dir(&loaded, &dir_b).expect("resave default");
        assert_dirs_byte_identical(&dir_a, &dir_b);

        // Modified settings: every field survives exactly.
        let mut model = fixture_model();
        model.settings.display = umber_color::DisplaySettings {
            view: umber_color::DisplayTransform::Rec709,
            exposure: 1.37,
            gamma: 2.2,
        };
        let dir_c = unique_temp_dir("display-mod-c");
        let dir_d = unique_temp_dir("display-mod-d");
        save_to_dir(&model, &dir_c).expect("save modified");
        let loaded = load_from_dir(&dir_c).expect("load modified");
        assert_eq!(loaded.settings.display, model.settings.display);
        assert_eq!(
            loaded.settings.display.exposure.to_bits(),
            1.37f32.to_bits()
        );
        assert_eq!(loaded, model);
        save_to_dir(&loaded, &dir_d).expect("resave modified");
        assert_dirs_byte_identical(&dir_c, &dir_d);

        for dir in [dir_a, dir_b, dir_c, dir_d] {
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn pre_postfx_project_json_loads_empty_chain() {
        // Old files have no "postfx" key in settings: serde(default)
        // yields the empty chain (no effects) — the additive rule, same
        // as display.
        let dir = unique_temp_dir("pre-postfx");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("project.json"),
            br#"{"version":1,"texture_sets":[{"name":"Body","resolution":2048,"channels":[]}],"layer_sets":[{"texture_set":"Body","layer_ids":[],"next_layer_id":0}],"settings":{"active_texture_set":"Body"}}"#,
        )
        .unwrap();

        let loaded = load_from_dir(&dir).expect("pre-postfx file must load");
        assert_eq!(loaded.settings.active_texture_set.as_deref(), Some("Body"));
        assert_eq!(loaded.settings.postfx, PostFxCarry::default());
        assert!(loaded.settings.postfx.effects.is_empty());
        assert!(loaded.settings.postfx.enabled.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn postfx_carry_round_trip_byte_identical() {
        // One effect of each kind with exact params (the bloom row
        // disabled): save -> load is exact, and save -> load -> save is
        // byte-identical. The JSON assertions pin the stable wire names.
        let mut model = fixture_model();
        model.settings.postfx = PostFxCarry {
            effects: vec![
                PostFxEffect::Dof {
                    focus: 5.0,
                    aperture: 0.3,
                },
                PostFxEffect::Bloom {
                    threshold: 1.0,
                    intensity: 0.5,
                    radius: 8,
                },
                PostFxEffect::Vignette {
                    strength: 0.3,
                    softness: 0.5,
                },
                PostFxEffect::Sharpen {
                    amount: 0.5,
                    radius: 1,
                },
                PostFxEffect::ToneMap {
                    mode: PostFxToneMapMode::Filmic,
                    exposure: -1.5,
                },
            ],
            enabled: vec![true, false, true, true, true],
        };
        let dir_a = unique_temp_dir("postfx-a");
        let dir_b = unique_temp_dir("postfx-b");

        save_to_dir(&model, &dir_a).expect("save postfx");
        let json = fs::read_to_string(dir_a.join("project.json")).unwrap();
        assert!(json.contains(r#""Dof""#), "{json}");
        assert!(json.contains(r#""Bloom""#), "{json}");
        assert!(json.contains(r#""Vignette""#), "{json}");
        assert!(json.contains(r#""Sharpen""#), "{json}");
        assert!(json.contains(r#""ToneMap""#), "{json}");
        assert!(json.contains(r#""Filmic""#), "{json}");

        let loaded = load_from_dir(&dir_a).expect("load postfx");
        assert_eq!(loaded.settings.postfx, model.settings.postfx);
        // Bit-exactness on a non-representable float (0.3): serde_json's
        // shortest round-trip repr parses back to the same bits (the same
        // guarantee the display test pins for 1.37).
        match loaded.settings.postfx.effects[0] {
            PostFxEffect::Dof { aperture, .. } => {
                assert_eq!(aperture.to_bits(), 0.3f32.to_bits());
            }
            other => panic!("expected Dof first, got {other:?}"),
        }
        assert_eq!(
            loaded.settings.postfx.enabled,
            vec![true, false, true, true, true]
        );
        assert_eq!(loaded, model);

        save_to_dir(&loaded, &dir_b).expect("resave postfx");
        assert_dirs_byte_identical(&dir_a, &dir_b);

        for dir in [dir_a, dir_b] {
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn save_overwrite_prunes_deleted_layer_files_and_removed_sets() {
        let mut model = fixture_model();
        let dir = unique_temp_dir("prune");

        save_to_dir(&model, &dir).expect("first save");
        let body_dir = dir.join("layers").join("Body");
        let helmet_dir = dir.join("layers").join("Helmet");
        let before = fs::read_dir(&body_dir).unwrap().count();
        assert_eq!(before, 2); // base + group survive; the removed fill layer never wrote a file
        assert!(helmet_dir.exists());

        // Remove the "Details" group, and drop the "Helmet" set entirely.
        let group_id = model.layers[0].stack.layers[1].id;
        model.layers[0].stack.remove_layer(group_id);
        model.layers.remove(1);
        model.texture_sets.remove(1);
        save_to_dir(&model, &dir).expect("second save");

        let after = fs::read_dir(&body_dir).unwrap().count();
        assert_eq!(after, 1); // stale file for the removed group must be gone
        assert!(!helmet_dir.exists()); // the removed set's whole directory must be gone

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_rejects_case_insensitive_duplicate_set_names() {
        let dir = unique_temp_dir("case-dup");
        let model = ProjectModel::new(
            vec![TextureSet::new_default("Body")],
            vec![
                TextureSetLayers {
                    texture_set: "Body".into(),
                    stack: LayerStack::new(),
                },
                TextureSetLayers {
                    texture_set: "body".into(),
                    stack: LayerStack::new(),
                },
            ],
            ProjectSettings::default(),
        );

        assert!(matches!(
            save_to_dir(&model, &dir),
            Err(ProjectError::DuplicateSetName(_))
        ));
    }

    #[test]
    fn save_rejects_windows_reserved_device_name() {
        let dir = unique_temp_dir("reserved-name");
        let model = ProjectModel::new(
            vec![],
            vec![TextureSetLayers {
                texture_set: "COM1".into(),
                stack: LayerStack::new(),
            }],
            ProjectSettings::default(),
        );

        assert!(matches!(
            save_to_dir(&model, &dir),
            Err(ProjectError::InvalidSetName(_))
        ));
    }

    #[test]
    fn save_rejects_windows_reserved_device_name_with_extension() {
        let dir = unique_temp_dir("reserved-name-ext");
        let model = ProjectModel::new(
            vec![],
            vec![TextureSetLayers {
                texture_set: "nul.metal".into(),
                stack: LayerStack::new(),
            }],
            ProjectSettings::default(),
        );

        assert!(matches!(
            save_to_dir(&model, &dir),
            Err(ProjectError::InvalidSetName(_))
        ));
    }

    #[test]
    fn load_rejects_unsupported_version() {
        let dir = unique_temp_dir("version");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("project.json"),
            br#"{"version":2,"texture_sets":[],"layer_sets":[],"settings":{"active_texture_set":null}}"#,
        )
        .unwrap();

        let result = load_from_dir(&dir);
        assert!(matches!(result, Err(ProjectError::UnsupportedVersion(2))));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_rejects_duplicate_set_names() {
        let dir = unique_temp_dir("dup-name");
        let model = ProjectModel::new(
            vec![TextureSet::new_default("Body")],
            vec![
                TextureSetLayers {
                    texture_set: "Body".into(),
                    stack: LayerStack::new(),
                },
                TextureSetLayers {
                    texture_set: "Body".into(),
                    stack: LayerStack::new(),
                },
            ],
            ProjectSettings::default(),
        );

        assert!(matches!(
            save_to_dir(&model, &dir),
            Err(ProjectError::DuplicateSetName(_))
        ));
    }

    #[test]
    fn load_rejects_path_traversal_set_name_before_touching_disk() {
        let dir = unique_temp_dir("traversal");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("project.json"),
            br#"{"version":1,"texture_sets":[{"name":"../escape","resolution":1,"channels":[]}],"layer_sets":[{"texture_set":"../escape","layer_ids":[],"next_layer_id":0}],"settings":{"active_texture_set":null}}"#,
        )
        .unwrap();

        assert!(matches!(
            load_from_dir(&dir),
            Err(ProjectError::InvalidSetName(_))
        ));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_rejects_layer_id_at_overflow_boundary() {
        let dir = unique_temp_dir("overflow");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("project.json"),
            format!(
                r#"{{"version":1,"texture_sets":[{{"name":"Body","resolution":1,"channels":[]}}],"layer_sets":[{{"texture_set":"Body","layer_ids":[{}],"next_layer_id":0}}],"settings":{{"active_texture_set":null}}}}"#,
                u64::MAX
            ),
        )
        .unwrap();

        assert!(matches!(
            load_from_dir(&dir),
            Err(ProjectError::LayerIdOverflow(_))
        ));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_rejects_invalid_set_name() {
        let dir = unique_temp_dir("invalid-name");
        let model = ProjectModel::new(
            vec![],
            vec![TextureSetLayers {
                texture_set: "../escape".into(),
                stack: LayerStack::new(),
            }],
            ProjectSettings::default(),
        );

        assert!(matches!(
            save_to_dir(&model, &dir),
            Err(ProjectError::InvalidSetName(_))
        ));
    }

    fn assert_dirs_byte_identical(a: &Path, b: &Path) {
        let files_a = collect_relative_files(a);
        let files_b = collect_relative_files(b);
        assert_eq!(
            files_a, files_b,
            "file listings differ between {a:?} and {b:?}"
        );

        for rel in files_a {
            let bytes_a = fs::read(a.join(&rel)).unwrap();
            let bytes_b = fs::read(b.join(&rel)).unwrap();
            assert_eq!(bytes_a, bytes_b, "byte mismatch in {rel:?}");
        }
    }

    fn collect_relative_files(root: &Path) -> Vec<PathBuf> {
        fn walk(dir: &Path, root: &Path, out: &mut Vec<PathBuf>) {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, root, out);
                } else {
                    out.push(path.strip_prefix(root).unwrap().to_path_buf());
                }
            }
        }
        let mut out = Vec::new();
        walk(root, root, &mut out);
        out.sort();
        out
    }
}
