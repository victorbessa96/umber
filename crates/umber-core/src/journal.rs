//! The autosave journal (requirements.md §8 P1 row;
//! docs/specs/autosave-journal-design.md).
//!
//! An append-only `journal.jsonl` beside `project.json`: one JSON line per
//! document mutation, fsync'd per entry, so the document is always current
//! on disk between saves. On load, entries past the project's
//! `journal_seq` high-water mark replay onto the saved state (see
//! `project::load_from_dir`); a save stamps the high-water mark and
//! truncates the journal.
//!
//! The op vocabulary is [`LayerCommand`]'s, resolved to absolute state so
//! every op is idempotent on its own: `Add` carries the allocated id,
//! `Reorder` is recorded as `Move { id, to }` (index-based reorders are not
//! idempotent), and the `Set*` ops carry the value the stack actually holds
//! after the apply (post-clamp, always finite). Undo is not an op: the
//! journal records the *effective* forward edit an undo performed (undoing
//! an `Add` journals a `Remove`, undoing a `Remove` journals an `Insert`),
//! because the undo history behind the high-water mark is not on disk.
//!
//! The corruption rule: a line is committed only once its trailing `\n`
//! lands. Reading stops at the first line that is unterminated, malformed,
//! carries an unknown op, or does not strictly increase `seq` — everything
//! before it replays, nothing after it does. Never a hard error.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::layers::{BlendMode, Layer, LayerCommand, LayerKind, LayerStack, MaterialParams};
use crate::project::ProjectModel;
use crate::undo::Command;
use umber_mesh::FillParams;

/// The journal's file name inside a `.umber` project directory.
pub const JOURNAL_FILE_NAME: &str = "journal.jsonl";

/// The journal path for the project at `project_dir`.
pub fn journal_path(project_dir: &Path) -> PathBuf {
    project_dir.join(JOURNAL_FILE_NAME)
}

/// One journaled document mutation, resolved to absolute state (see the
/// module docs for why each op is shaped the way it is).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JournalOp {
    /// A new layer pushed onto the top of the stack under `id`
    /// ([`LayerCommand::Add`]).
    Add {
        /// The id the live apply allocated.
        id: u64,
        /// The new layer's name.
        name: String,
        /// The new layer's kind.
        kind: LayerKind,
    },
    /// A full layer re-inserted at `index` — what undoing a
    /// [`LayerCommand::Remove`] does to the stack.
    Insert {
        /// The stack position the layer landed at.
        index: usize,
        /// The complete layer body.
        layer: Layer,
    },
    /// A layer removed by id ([`LayerCommand::Remove`], or undoing an Add).
    Remove {
        /// The removed layer's id.
        id: u64,
    },
    /// A layer moved to stack position `to` ([`LayerCommand::Reorder`],
    /// resolved from indices to the moved layer's id).
    Move {
        /// The moved layer's id.
        id: u64,
        /// The position it landed at (already clamped by the live apply).
        to: usize,
    },
    /// A layer's opacity ([`LayerCommand::SetOpacity`]).
    SetOpacity {
        /// The layer's id.
        id: u64,
        /// The opacity the stack holds after the apply.
        value: f32,
    },
    /// A layer's visibility ([`LayerCommand::SetVisible`]).
    SetVisible {
        /// The layer's id.
        id: u64,
        /// The visibility after the apply.
        value: bool,
    },
    /// A layer's blend mode ([`LayerCommand::SetBlendMode`]).
    SetBlendMode {
        /// The layer's id.
        id: u64,
        /// The blend mode after the apply.
        mode: BlendMode,
    },
    /// A layer's fill parameters ([`LayerCommand::SetFill`]): `None` when
    /// the layer holds no fill params after the apply (a non-fill layer,
    /// or a fill layer whose params were cleared by undo).
    SetFill {
        /// The layer's id.
        id: u64,
        /// The fill parameters after the apply.
        fill: Option<FillParams>,
    },
    /// A layer's material parameters ([`LayerCommand::SetMaterial`]):
    /// `None` when the layer holds no material params after the apply (a
    /// non-material layer, or a material layer whose params were cleared
    /// by undo — the `SetFill` convention).
    SetMaterial {
        /// The layer's id.
        id: u64,
        /// The material parameters after the apply.
        material: Option<MaterialParams>,
    },
    /// An instance layer's source ([`LayerCommand::SetInstanceSource`]):
    /// the source the stack holds after the apply.
    SetInstanceSource {
        /// The instance layer's id.
        id: u64,
        /// The source after the apply.
        source: u64,
    },
    /// A layer's mask presence ([`LayerCommand::AttachMask`] forward,
    /// [`LayerCommand::DetachMask`] revert): `Some(name)` when the layer
    /// holds a mask after the op, `None` when it does not. The mask's
    /// PIXELS live in the paint session (the pool the app prunes against
    /// live stacks — see `LayerStack::attach_mask`'s docs); the journal
    /// carries only the metadata half, exactly like the document.
    SetLayerMask {
        /// The layer's id.
        id: u64,
        /// The mask after the op (`None` = no mask).
        mask: Option<String>,
    },
    /// A layer's mask enabled flag ([`LayerCommand::SetMaskEnabled`]).
    SetMaskEnabled {
        /// The layer's id.
        id: u64,
        /// The flag after the op.
        enabled: bool,
    },
    /// A layer's name ([`LayerCommand::Rename`]).
    Rename {
        /// The layer's id.
        id: u64,
        /// The name the stack holds after the op.
        name: String,
    },
}

impl JournalOp {
    /// The forward op for `command` just applied to `stack` — call after
    /// `UndoStack::push` or `UndoStack::redo`, on
    /// [`UndoStack::peek_undo`](crate::undo::UndoStack::peek_undo).
    /// `None` when the apply was a no-op (unknown id, out-of-range index).
    pub fn applied(command: &LayerCommand, stack: &LayerStack) -> Option<Self> {
        match command {
            LayerCommand::Add { name, kind, id } => id.map(|id| JournalOp::Add {
                id,
                name: name.clone(),
                kind: kind.clone(),
            }),
            LayerCommand::Remove { id, removed } => {
                removed.as_ref().map(|_| JournalOp::Remove { id: *id })
            }
            LayerCommand::Reorder { applied_to, .. } => {
                let to = (*applied_to)?;
                let id = stack.layers.get(to)?.id;
                Some(JournalOp::Move { id, to })
            }
            // `old` is `Some` exactly when the layer existed; the value
            // is read back from the stack so the line carries the
            // post-clamp state (a NaN request would not even serialize).
            LayerCommand::SetOpacity { id, old, .. } => {
                old.as_ref()?;
                stack.layer(*id).map(|layer| JournalOp::SetOpacity {
                    id: *id,
                    value: layer.opacity,
                })
            }
            LayerCommand::SetVisible { id, old, .. } => {
                old.as_ref()?;
                stack.layer(*id).map(|layer| JournalOp::SetVisible {
                    id: *id,
                    value: layer.visible,
                })
            }
            LayerCommand::SetBlendMode { id, old, .. } => {
                old.as_ref()?;
                stack.layer(*id).map(|layer| JournalOp::SetBlendMode {
                    id: *id,
                    mode: layer.blend_mode,
                })
            }
            LayerCommand::SetFill { id, old, .. } => {
                old.as_ref()?;
                stack.layer(*id).map(|layer| JournalOp::SetFill {
                    id: *id,
                    fill: layer.fill.clone(),
                })
            }
            LayerCommand::SetMaterial { id, old, .. } => {
                old.as_ref()?;
                stack.layer(*id).map(|layer| JournalOp::SetMaterial {
                    id: *id,
                    material: layer.material.clone(),
                })
            }
            LayerCommand::SetInstanceSource { id, old, .. } => {
                old.as_ref()?;
                stack
                    .instance_source(*id)
                    .map(|source| JournalOp::SetInstanceSource { id: *id, source })
            }
            LayerCommand::AttachMask { id, old, .. } => {
                old.as_ref()?;
                stack
                    .layer(*id)
                    .and_then(|layer| layer.mask.as_ref())
                    .map(|mask| JournalOp::SetLayerMask {
                        id: *id,
                        mask: Some(mask.name.clone()),
                    })
            }
            LayerCommand::DetachMask { id, removed, .. } => {
                removed.as_ref()?;
                stack.layer(*id).map(|_| JournalOp::SetLayerMask {
                    id: *id,
                    mask: None,
                })
            }
            LayerCommand::SetMaskEnabled { id, old, .. } => {
                old.as_ref()?;
                stack
                    .layer(*id)
                    .and_then(|layer| layer.mask.as_ref())
                    .map(|mask| JournalOp::SetMaskEnabled {
                        id: *id,
                        enabled: mask.enabled,
                    })
            }
            LayerCommand::Rename { id, .. } => stack.layer(*id).map(|layer| JournalOp::Rename {
                id: *id,
                name: layer.name.clone(),
            }),
        }
    }

    /// The effective op that reverting `command` will perform on `stack` —
    /// call *before* `UndoStack::undo`, on `peek_undo`: `revert` `take()`s
    /// the captured state this reads. `None` when the revert is a no-op.
    pub fn reverting(command: &LayerCommand, stack: &LayerStack) -> Option<Self> {
        match command {
            LayerCommand::Add { id, .. } => id.map(|id| JournalOp::Remove { id }),
            LayerCommand::Remove { removed, .. } => {
                removed.as_ref().map(|(index, layer)| JournalOp::Insert {
                    index: *index,
                    layer: layer.clone(),
                })
            }
            // Revert runs `reorder(applied_to, from)`: the layer now at
            // `applied_to` lands back at `from` (in range — the stack
            // length is what it was when `from` was valid).
            LayerCommand::Reorder {
                from, applied_to, ..
            } => {
                let to = (*applied_to)?;
                let id = stack.layers.get(to)?.id;
                Some(JournalOp::Move { id, to: *from })
            }
            LayerCommand::SetOpacity { id, old, .. } => {
                old.map(|value| JournalOp::SetOpacity { id: *id, value })
            }
            LayerCommand::SetVisible { id, old, .. } => {
                old.map(|value| JournalOp::SetVisible { id: *id, value })
            }
            LayerCommand::SetBlendMode { id, old, .. } => {
                old.map(|mode| JournalOp::SetBlendMode { id: *id, mode })
            }
            LayerCommand::SetFill { id, old, .. } => {
                old.clone().map(|fill| JournalOp::SetFill { id: *id, fill })
            }
            LayerCommand::SetMaterial { id, old, .. } => old
                .clone()
                .map(|material| JournalOp::SetMaterial { id: *id, material }),
            LayerCommand::SetInstanceSource { id, old, .. } => {
                old.map(|source| JournalOp::SetInstanceSource { id: *id, source })
            }
            // Reverting AttachMask restores the captured `old` (Some(Some)
            // = had a different mask, Some(None) = had none); reverting
            // DetachMask re-attaches the removed one. Both map to the
            // mask's NAME (the pixels re-derive from the paint sidecars).
            LayerCommand::AttachMask { id, old, .. } => {
                old.clone().flatten().map(|mask| JournalOp::SetLayerMask {
                    id: *id,
                    mask: Some(mask.name),
                })
            }
            LayerCommand::DetachMask { id, removed, .. } => {
                removed.clone().map(|mask| JournalOp::SetLayerMask {
                    id: *id,
                    mask: Some(mask.name),
                })
            }
            LayerCommand::SetMaskEnabled { id, old, .. } => {
                old.map(|enabled| JournalOp::SetMaskEnabled { id: *id, enabled })
            }
            LayerCommand::Rename { id, old, .. } => old.as_ref().map(|name| JournalOp::Rename {
                id: *id,
                name: name.clone(),
            }),
        }
    }

    /// Re-applies this op to `stack` through the same functions the live
    /// session used, idempotently: applying it twice leaves the stack as
    /// applying it once does.
    ///
    /// - `Add` → [`LayerCommand::Add`] with `id: Some` through
    ///   [`Command::apply`] (layers.rs `LayerCommand::apply`, the redo arm —
    ///   `add_layer_with_id` is private to `layers`); skipped if the id is
    ///   already present.
    /// - `Insert` → [`LayerStack::insert_layer`] (what
    ///   `LayerCommand::Remove`'s revert calls); skipped if present.
    /// - `Remove` → [`LayerStack::remove_layer`] (Remove's apply); absent
    ///   ids are a no-op.
    /// - `Move` → [`LayerStack::reorder`] from the id's current index
    ///   (Reorder's apply); a second apply moves it onto itself.
    /// - `Set*` → [`LayerStack::set_opacity`] / [`LayerStack::set_visible`]
    ///   / [`LayerStack::set_blend_mode`] / [`LayerStack::set_fill`] /
    ///   [`LayerStack::set_material`] (the `Set*` applies); absolute. A
    ///   `SetFill`/`SetMaterial` with `None` clears the layer's params (the
    ///   undo-of-first-set shape).
    ///
    /// Ids of `u64::MAX` are skipped: both re-insert paths compute `id + 1`
    /// for the stack's id counter, and `project` rejects that id anyway.
    pub fn apply_to(&self, stack: &mut LayerStack) {
        match self {
            JournalOp::Add { id, name, kind } => {
                if *id != u64::MAX && stack.layer(*id).is_none() {
                    LayerCommand::Add {
                        name: name.clone(),
                        kind: kind.clone(),
                        id: Some(*id),
                    }
                    .apply(stack);
                }
            }
            JournalOp::Insert { index, layer } => {
                if layer.id != u64::MAX && stack.layer(layer.id).is_none() {
                    stack.insert_layer(*index, layer.clone());
                }
            }
            JournalOp::Remove { id } => {
                stack.remove_layer(*id);
            }
            JournalOp::Move { id, to } => {
                if let Some(from) = stack.index_of(*id) {
                    stack.reorder(from, *to);
                }
            }
            JournalOp::SetOpacity { id, value } => {
                stack.set_opacity(*id, *value);
            }
            JournalOp::SetVisible { id, value } => {
                stack.set_visible(*id, *value);
            }
            JournalOp::SetBlendMode { id, mode } => {
                stack.set_blend_mode(*id, *mode);
            }
            JournalOp::SetFill { id, fill } => match fill.clone() {
                Some(params) => {
                    stack.set_fill(*id, params);
                }
                None => {
                    if let Some(layer) = stack.layer_mut(*id) {
                        layer.fill = None;
                    }
                }
            },
            JournalOp::SetMaterial { id, material } => match material.clone() {
                Some(params) => {
                    stack.set_material(*id, params);
                }
                None => {
                    if let Some(layer) = stack.layer_mut(*id) {
                        layer.material = None;
                    }
                }
            },
            JournalOp::SetInstanceSource { id, source } => {
                // Through the cycle guard (recovery never forces a cycle:
                // a retarget that now cycles is ignored, like the live
                // redo path).
                let _ = stack.set_instance_source(*id, *source);
            }
            JournalOp::SetLayerMask { id, mask } => {
                // Replay converges on the mask's PRESENCE: attach when the
                // op says one exists (a fresh metadata mask under the
                // recorded name — the pixel pool re-derives from the
                // sidecars), detach when it says none. A replay running on
                // a stack that already matches is a no-op by construction.
                match mask {
                    Some(name) => {
                        let _ = stack.attach_mask(*id, name.clone());
                    }
                    None => {
                        let _ = stack.detach_mask(*id);
                    }
                }
            }
            JournalOp::SetMaskEnabled { id, enabled } => {
                let _ = stack.set_mask_enabled(*id, *enabled);
            }
            JournalOp::Rename { id, name } => {
                let _ = stack.rename_layer(*id, name.clone());
            }
        }
    }
}

/// One `journal.jsonl` line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalEntry {
    /// Strictly increasing across the project's life — it continues past
    /// truncations, so the `journal_seq` high-water mark stays comparable.
    pub seq: u64,
    /// The texture set whose layer stack the op touched.
    pub texture_set: String,
    /// The mutation.
    pub op: JournalOp,
}

/// Parses the committed prefix of `bytes` (see the module's corruption
/// rule), returning the entries and the byte length of that prefix.
fn parse_valid_prefix(bytes: &[u8]) -> (Vec<JournalEntry>, u64) {
    let mut entries: Vec<JournalEntry> = Vec::new();
    let mut valid_len = 0usize;
    let mut rest = bytes;
    // Only newline-terminated lines are considered: a trailing segment
    // without its `\n` is a torn write and is never parsed.
    while let Some(newline) = rest.iter().position(|&b| b == b'\n') {
        let Ok(entry) = serde_json::from_slice::<JournalEntry>(&rest[..newline]) else {
            break;
        };
        if entries.last().is_some_and(|prev| entry.seq <= prev.seq) {
            break;
        }
        entries.push(entry);
        valid_len += newline + 1;
        rest = &rest[newline + 1..];
    }
    (entries, valid_len as u64)
}

/// Reads the committed entries of the journal at `project_dir`. A missing
/// journal is an empty one.
pub fn read_journal(project_dir: &Path) -> io::Result<Vec<JournalEntry>> {
    match fs::read(journal_path(project_dir)) {
        Ok(bytes) => Ok(parse_valid_prefix(&bytes).0),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(err),
    }
}

/// Truncates the journal at `project_dir` to empty (in place — a live
/// append handle on it keeps working) and syncs. A missing journal is
/// already empty.
pub fn truncate_journal(project_dir: &Path) -> io::Result<()> {
    set_len_synced(&journal_path(project_dir), 0)
}

fn set_len_synced(path: &Path, len: u64) -> io::Result<()> {
    // A separate write handle: `set_len` on an append-only handle is
    // refused on Windows (FILE_APPEND_DATA lacks FILE_WRITE_DATA).
    match OpenOptions::new().write(true).open(path) {
        Ok(file) => {
            file.set_len(len)?;
            file.sync_data()
        }
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

/// Applies every entry past `model.journal_seq` to its texture set's
/// stack, in order, advancing `model.journal_seq` to each applied entry's
/// seq. Entries at or below the high-water mark are already reflected in
/// the saved state and are skipped — so replaying the same journal twice
/// is a no-op the second time. An entry naming a texture set the model
/// lacks is consumed without effect (best-effort recovery). Returns how
/// many entries were past the high-water mark.
pub fn replay(model: &mut ProjectModel, entries: &[JournalEntry]) -> usize {
    let mut replayed = 0;
    for entry in entries {
        if entry.seq <= model.journal_seq {
            continue;
        }
        if let Some(set) = model
            .layers
            .iter_mut()
            .find(|set| set.texture_set == entry.texture_set)
        {
            entry.op.apply_to(&mut set.stack);
        }
        model.journal_seq = entry.seq;
        replayed += 1;
    }
    replayed
}

/// The open, appendable journal of one project directory.
#[derive(Debug)]
pub struct Journal {
    path: PathBuf,
    file: File,
    /// The seq of the last committed entry (or the high-water mark the
    /// journal was opened at, whichever is greater).
    last_seq: u64,
    /// Byte length of the committed prefix — where a failed append rolls
    /// back to, so a half-written line never glues onto the next one.
    committed_len: u64,
}

impl Journal {
    /// Opens (creating if needed) the journal of the project at
    /// `project_dir`, whose `project.json` carries `high_water` as its
    /// `journal_seq`. New entries continue from
    /// `max(high_water, last committed seq) + 1` — never restarting after a
    /// truncate, or recovery would skip them as already saved. A torn tail
    /// left by a crash is cut back to the last committed line first.
    pub fn open(project_dir: &Path, high_water: u64) -> io::Result<Self> {
        let path = journal_path(project_dir);
        let (entries, valid_len) = match fs::read(&path) {
            Ok(bytes) => {
                let (entries, valid_len) = parse_valid_prefix(&bytes);
                if valid_len != bytes.len() as u64 {
                    set_len_synced(&path, valid_len)?;
                }
                (entries, valid_len)
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => (Vec::new(), 0),
            Err(err) => return Err(err),
        };
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let last_seq = entries.last().map_or(0, |e| e.seq).max(high_water);
        Ok(Self {
            path,
            file,
            last_seq,
            committed_len: valid_len,
        })
    }

    /// Appends `op` (touching `texture_set`) as the next entry and fsyncs
    /// before returning its seq. Call only after the in-memory apply
    /// succeeded and any bytes the op references are on disk.
    pub fn record(&mut self, texture_set: &str, op: JournalOp) -> io::Result<u64> {
        let seq = self.last_seq + 1;
        let entry = JournalEntry {
            seq,
            texture_set: texture_set.to_string(),
            op,
        };
        let mut line = serde_json::to_vec(&entry)?;
        line.push(b'\n');
        // One write for the whole line: the newline is the commit marker.
        let written = self
            .file
            .write_all(&line)
            .and_then(|()| self.file.sync_data());
        if let Err(err) = written {
            // Best-effort rollback of a partial line; if even that fails,
            // the next `open` cuts the torn tail.
            let _ = set_len_synced(&self.path, self.committed_len);
            return Err(err);
        }
        self.committed_len += line.len() as u64;
        self.last_seq = seq;
        Ok(seq)
    }

    /// The seq of the last committed entry — the value a save stamps into
    /// `project.json` as its `journal_seq`.
    pub fn last_seq(&self) -> u64 {
        self.last_seq
    }

    /// Truncates the journal to empty. The seq does not reset.
    pub fn truncate(&mut self) -> io::Result<()> {
        set_len_synced(&self.path, 0)?;
        self.committed_len = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{load_from_dir, save_to_dir, ProjectSettings, TextureSetLayers};
    use crate::undo::UndoStack;
    use crate::TextureSet;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn unique_temp_dir(label: &str) -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "umber-core-journal-{label}-{}-{n}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// A saved one-set project ("Body", one paint layer) at `dir`.
    fn saved_base_project(dir: &Path) -> ProjectModel {
        let mut stack = LayerStack::new();
        stack.add_layer("Base", LayerKind::Paint);
        let model = ProjectModel::new(
            vec![TextureSet::new_default("Body")],
            vec![TextureSetLayers {
                texture_set: "Body".into(),
                stack,
            }],
            ProjectSettings::default(),
        );
        save_to_dir(&model, dir).expect("base save");
        model
    }

    /// A live session mirroring the app's `Document` chokepoints: apply
    /// through the undo stack, then journal the effective op.
    struct Session {
        stack: LayerStack,
        history: UndoStack<LayerCommand>,
        journal: Journal,
    }

    impl Session {
        fn start(dir: &Path, model: &ProjectModel) -> Self {
            Self {
                stack: model.layers[0].stack.clone(),
                history: UndoStack::new(256),
                journal: Journal::open(dir, model.journal_seq).expect("open journal"),
            }
        }

        fn record(&mut self, op: Option<JournalOp>) {
            if let Some(op) = op {
                self.journal.record("Body", op).expect("record");
            }
        }

        fn run(&mut self, command: LayerCommand) {
            self.history.push(command, &mut self.stack);
            let op = self
                .history
                .peek_undo()
                .and_then(|c| JournalOp::applied(c, &self.stack));
            self.record(op);
        }

        fn undo(&mut self) {
            let op = self
                .history
                .peek_undo()
                .and_then(|c| JournalOp::reverting(c, &self.stack));
            assert!(self.history.undo(&mut self.stack));
            self.record(op);
        }

        fn redo(&mut self) {
            assert!(self.history.redo(&mut self.stack));
            let op = self
                .history
                .peek_undo()
                .and_then(|c| JournalOp::applied(c, &self.stack));
            self.record(op);
        }
    }

    /// Every op kind plus undo/redo of each shape; returns the stack
    /// snapshot after each step (index k = state after k+1 steps).
    fn run_full_session(session: &mut Session) -> Vec<LayerStack> {
        let mut states = Vec::new();
        session.run(LayerCommand::add("A", LayerKind::Paint)); // id 1
        states.push(session.stack.clone());
        session.run(LayerCommand::add("B", LayerKind::Fill)); // id 2
        states.push(session.stack.clone());
        session.run(LayerCommand::set_opacity(1, 0.25));
        states.push(session.stack.clone());
        session.run(LayerCommand::reorder(2, 0)); // B to the bottom
        states.push(session.stack.clone());
        session.run(LayerCommand::remove(0));
        states.push(session.stack.clone());
        session.undo(); // Base re-inserted at its old index
        states.push(session.stack.clone());
        session.run(LayerCommand::set_blend_mode(2, BlendMode::Multiply));
        states.push(session.stack.clone());
        session.run(LayerCommand::set_visible(1, false));
        states.push(session.stack.clone());
        session.undo(); // visible again
        states.push(session.stack.clone());
        session.redo(); // hidden again
        states.push(session.stack.clone());
        session.run(LayerCommand::reorder(0, 2));
        states.push(session.stack.clone());
        session.undo(); // the reorder reverted
        states.push(session.stack.clone());
        session.run(LayerCommand::add("C", LayerKind::Paint)); // id 3
        states.push(session.stack.clone());
        session.undo(); // C removed; next id stays 4
        states.push(session.stack.clone());
        states
    }

    #[test]
    fn journal_lines_have_the_entry_shape() {
        let dir = unique_temp_dir("shape");
        let model = saved_base_project(&dir);
        let mut session = Session::start(&dir, &model);
        session.run(LayerCommand::add("A", LayerKind::Paint));
        session.run(LayerCommand::set_opacity(1, 0.5));
        session.run(LayerCommand::set_opacity(1, 7.0)); // clamped to 1.0 live
        session.undo(); // back to 0.5

        let text = fs::read_to_string(journal_path(&dir)).unwrap();
        assert!(text.ends_with('\n'), "every committed line ends in \\n");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 4);
        assert!(
            lines[0].contains(r#""seq":1"#)
                && lines[0].contains(r#""texture_set":"Body""#)
                && lines[0].contains(r#""type":"add""#),
            "{}",
            lines[0]
        );
        assert_eq!(
            read_journal(&dir).unwrap(),
            vec![
                JournalEntry {
                    seq: 1,
                    texture_set: "Body".into(),
                    op: JournalOp::Add {
                        id: 1,
                        name: "A".into(),
                        kind: LayerKind::Paint,
                    },
                },
                JournalEntry {
                    seq: 2,
                    texture_set: "Body".into(),
                    op: JournalOp::SetOpacity { id: 1, value: 0.5 },
                },
                JournalEntry {
                    seq: 3,
                    texture_set: "Body".into(),
                    op: JournalOp::SetOpacity { id: 1, value: 1.0 },
                },
                JournalEntry {
                    seq: 4,
                    texture_set: "Body".into(),
                    op: JournalOp::SetOpacity { id: 1, value: 0.5 },
                },
            ]
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn set_material_journals_absolute_state_and_replays() {
        use crate::layers::{MaterialChannel, MaterialParams};
        let dir = unique_temp_dir("set-material");
        let model = saved_base_project(&dir);
        let mut session = Session::start(&dir, &model);
        session.run(LayerCommand::add("Steel", LayerKind::Material)); // id 1
        let params = MaterialParams {
            material_id: "umber.weathered-steel".into(),
            channels: vec![MaterialChannel::named("baseColor")],
            params: vec![],
        };
        session.run(LayerCommand::set_material(1, params.clone()));
        session.undo(); // back to None
        assert_eq!(session.stack.layer(1).unwrap().material, None);
        session.redo();
        assert_eq!(
            session.stack.layer(1).unwrap().material,
            Some(params.clone())
        );
        let live = session.stack.clone();
        drop(session); // the crash: no save

        // The committed prefix carries Add + SetMaterial + clear + SetMaterial.
        let entries = read_journal(&dir).unwrap();
        assert_eq!(entries.len(), 4);
        assert!(
            entries[1].op
                == JournalOp::SetMaterial {
                    id: 1,
                    material: Some(params.clone()),
                },
            "the journal carries post-apply state, got {:?}",
            entries[1].op
        );
        assert_eq!(
            entries[2].op,
            JournalOp::SetMaterial {
                id: 1,
                material: None
            }
        );

        let recovered = load_from_dir(&dir).expect("recovering load");
        assert_eq!(recovered.layers[0].stack, live);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn replay_reaches_the_live_session_end_state() {
        let dir = unique_temp_dir("replay");
        let model = saved_base_project(&dir);
        let mut session = Session::start(&dir, &model);
        run_full_session(&mut session);
        let live = session.stack.clone();
        drop(session); // the crash: no save

        let recovered = load_from_dir(&dir).expect("recovering load");
        // LayerStack's PartialEq includes the id counter: the undone Add
        // of id 3 must still leave it at 4.
        assert_eq!(recovered.layers[0].stack, live);
        assert_eq!(recovered.layers[0].stack.next_layer_id(), 4);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn torn_final_line_replays_to_the_last_valid_line() {
        let dir = unique_temp_dir("torn");
        let model = saved_base_project(&dir);
        let mut session = Session::start(&dir, &model);
        let states = run_full_session(&mut session);
        let committed = session.journal.last_seq();
        drop(session);

        // The crash mid-append: half a line, no newline.
        let mut file = OpenOptions::new()
            .append(true)
            .open(journal_path(&dir))
            .unwrap();
        file.write_all(br#"{"seq":999,"texture_set":"Body","op":{"type":"rem"#)
            .unwrap();
        drop(file);

        let recovered = load_from_dir(&dir).expect("a torn journal never errors");
        assert_eq!(&recovered.layers[0].stack, states.last().unwrap());
        assert_eq!(recovered.journal_seq, committed);

        // A torn tail left in place is cut by `open`, so the next append
        // starts a clean line instead of gluing onto the fragment.
        fs::write(journal_path(&dir), b"{\"seq\":").unwrap();
        let mut journal = Journal::open(&dir, recovered.journal_seq).unwrap();
        journal
            .record(
                "Body",
                JournalOp::SetVisible {
                    id: 0,
                    value: false,
                },
            )
            .unwrap();
        assert_eq!(read_journal(&dir).unwrap().len(), 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn replaying_twice_yields_the_same_state() {
        let dir = unique_temp_dir("idempotent");
        let model = saved_base_project(&dir);
        let mut session = Session::start(&dir, &model);
        run_full_session(&mut session);
        drop(session);
        let journal_bytes = fs::read(journal_path(&dir)).unwrap();
        let entries = read_journal(&dir).unwrap();

        // Crash between recovery's project.json rewrite and its truncate:
        // the journal survives beside an already-advanced project.json.
        let first = load_from_dir(&dir).expect("first recovery");
        fs::write(journal_path(&dir), &journal_bytes).unwrap();
        let second = load_from_dir(&dir).expect("second recovery");
        assert_eq!(second, first);

        // In memory: the high-water mark makes a second replay a no-op...
        let mut model_twice = model.clone();
        assert_eq!(replay(&mut model_twice, &entries), entries.len());
        let once = model_twice.clone();
        assert_eq!(replay(&mut model_twice, &entries), 0);
        assert_eq!(model_twice, once);

        // ...and each op is idempotent on its own, high-water aside.
        let mut stack = model.layers[0].stack.clone();
        for entry in &entries {
            entry.op.apply_to(&mut stack);
            entry.op.apply_to(&mut stack);
        }
        assert_eq!(stack, once.layers[0].stack);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recovery_advances_high_water_and_truncates() {
        let dir = unique_temp_dir("high-water");
        let model = saved_base_project(&dir);
        let mut session = Session::start(&dir, &model);
        run_full_session(&mut session);
        let last = session.journal.last_seq();
        drop(session);

        let recovered = load_from_dir(&dir).expect("recovery");
        assert_eq!(recovered.journal_seq, last);
        let json = fs::read_to_string(dir.join("project.json")).unwrap();
        assert!(
            json.contains(&format!(r#""journal_seq": {last}"#)),
            "{json}"
        );
        assert_eq!(fs::metadata(journal_path(&dir)).unwrap().len(), 0);

        // The seq continues past the high-water mark after the truncate,
        // so the next session's ops are not mistaken for saved ones.
        let mut session = Session::start(&dir, &recovered);
        session.run(LayerCommand::set_visible(0, false));
        assert_eq!(session.journal.last_seq(), last + 1);
        let live = session.stack.clone();
        drop(session);
        assert_eq!(load_from_dir(&dir).unwrap().layers[0].stack, live);
        let _ = fs::remove_dir_all(&dir);
    }
}
