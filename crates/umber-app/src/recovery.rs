//! Pixel autosave + "Recover unsaved work?" (audit C2-3).
//!
//! The autosave journal (`umber_core::journal`) replays LAYER ops after a
//! crash, but only for a project that has a directory, and never paint
//! pixels. This module adds the missing half: while the document has
//! unsaved edits, the shell periodically writes a FULL project snapshot
//! (`project.json` + the `paint/` sidecar, the ordinary crash-safe save
//! path) into `<state dir>/recovery/snapshot/`, never into the user's own
//! project directory. Never-saved documents are covered too.
//!
//! - [`RecoveryMeta`] sits beside the snapshot: which project it came from
//!   (if any), the mesh it painted on, and when. A snapshot without a
//!   readable meta is ignored, never half-trusted.
//! - [`AutosaveClock`] decides WHEN: every [`AUTOSAVE_SECS`] while dirty,
//!   never during a stroke, and never twice for the same edit state.
//! - [`pending`] finds a snapshot a previous run left (a crash, or a
//!   kill); the shell offers it once. [`discard`] removes it, which the
//!   shell also does after a clean save, Discard, or a normal exit.
//!
//! Recovering opens the snapshot as the project, then forgets the
//! snapshot's own directory: the user's next Ctrl+S asks where to save
//! (or saves to the original project, when the meta names one), so a
//! recovered session never silently writes into the state dir.

use std::path::{Path, PathBuf};

/// How often an unsaved document is snapshotted, in seconds.
pub const AUTOSAVE_SECS: f64 = 120.0;

/// `<state>/recovery/`.
pub fn recovery_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("recovery")
}

/// Where the snapshot project lives.
pub fn snapshot_dir(state_dir: &Path) -> PathBuf {
    recovery_dir(state_dir).join("snapshot")
}

fn meta_path(state_dir: &Path) -> PathBuf {
    recovery_dir(state_dir).join("recovery.txt")
}

/// What a snapshot came from. One `key=value` line each, so the file is
/// readable by hand; unknown keys are ignored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecoveryMeta {
    /// The project directory the session had open, if any.
    pub source_project: Option<PathBuf>,
    /// The mesh the session painted on, if any (reloaded before the
    /// snapshot opens, so the paint lands on its mesh).
    pub mesh: Option<PathBuf>,
    /// Seconds since the Unix epoch when the snapshot was written.
    pub written_unix: u64,
}

impl RecoveryMeta {
    /// The meta's text form.
    pub fn to_text(&self) -> String {
        let mut out = String::from("umber-recovery 1\n");
        if let Some(p) = &self.source_project {
            out.push_str(&format!("project={}\n", p.display()));
        }
        if let Some(p) = &self.mesh {
            out.push_str(&format!("mesh={}\n", p.display()));
        }
        out.push_str(&format!("written={}\n", self.written_unix));
        out
    }

    /// Parses [`Self::to_text`]. `None` without the version line: a file
    /// that is not ours is never trusted.
    pub fn from_text(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        if lines.next()? != "umber-recovery 1" {
            return None;
        }
        let mut meta = Self::default();
        for line in lines {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key {
                "project" if !value.is_empty() => meta.source_project = Some(PathBuf::from(value)),
                "mesh" if !value.is_empty() => meta.mesh = Some(PathBuf::from(value)),
                "written" => meta.written_unix = value.trim().parse().unwrap_or(0),
                _ => {}
            }
        }
        Some(meta)
    }
}

/// Writes the meta beside a freshly written snapshot (write-then-rename,
/// so a crash mid-write leaves the previous meta or none).
///
/// # Errors
///
/// The directory or the write failing.
pub fn write_meta(state_dir: &Path, meta: &RecoveryMeta) -> std::io::Result<()> {
    let path = meta_path(state_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("txt.tmp.{}", std::process::id()));
    std::fs::write(&tmp, meta.to_text())?;
    std::fs::rename(&tmp, &path)
}

/// A snapshot a previous run left, with its meta: `Some` only when both
/// the meta parses and the snapshot's `project.json` exists.
pub fn pending(state_dir: &Path) -> Option<RecoveryMeta> {
    let meta = RecoveryMeta::from_text(&std::fs::read_to_string(meta_path(state_dir)).ok()?)?;
    snapshot_dir(state_dir)
        .join("project.json")
        .is_file()
        .then_some(meta)
}

/// Removes the snapshot and its meta (Discard, a clean save, a normal
/// exit). Missing files are fine.
pub fn discard(state_dir: &Path) {
    let dir = recovery_dir(state_dir);
    if dir.exists() {
        if let Err(e) = std::fs::remove_dir_all(&dir) {
            log::warn!("recovery snapshot not removed ({}): {e}", dir.display());
        }
    }
}

/// When the next autosave runs (pure state, driven by egui's frame time).
#[derive(Debug, Clone, Default)]
pub struct AutosaveClock {
    /// Frame time of the last snapshot (or of the first dirty frame).
    last: Option<f64>,
    /// The edit generation last snapshotted: no second snapshot for an
    /// unchanged document.
    saved_generation: Option<u64>,
}

impl AutosaveClock {
    /// Whether to snapshot this frame. `now` is the frame time in seconds,
    /// `generation` a counter that moves with every edit (history rows),
    /// `dirty` / `stroking` the document's state.
    pub fn due(&mut self, now: f64, generation: u64, dirty: bool, stroking: bool) -> bool {
        if !dirty {
            // Saved (or never edited): the next edit starts a fresh
            // interval.
            self.last = None;
            self.saved_generation = None;
            return false;
        }
        let since = *self.last.get_or_insert(now);
        if stroking || self.saved_generation == Some(generation) || now - since < AUTOSAVE_SECS {
            return false;
        }
        true
    }

    /// Records a snapshot taken at `now` for `generation`.
    pub fn snapshotted(&mut self, now: f64, generation: u64) {
        self.last = Some(now);
        self.saved_generation = Some(generation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("umber-recovery-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn meta_round_trips_and_rejects_foreign_files() {
        let meta = RecoveryMeta {
            source_project: Some(PathBuf::from("/work/sword.umber")),
            mesh: Some(PathBuf::from("/work/sword.fbx")),
            written_unix: 1_760_000_000,
        };
        assert_eq!(RecoveryMeta::from_text(&meta.to_text()), Some(meta));
        let bare = RecoveryMeta::default();
        assert_eq!(RecoveryMeta::from_text(&bare.to_text()), Some(bare));
        assert_eq!(
            RecoveryMeta::from_text("project=/x\n"),
            None,
            "no version line"
        );
        assert_eq!(RecoveryMeta::from_text(""), None);
    }

    #[test]
    fn pending_needs_both_the_meta_and_the_snapshot() {
        let state = tmp("pending");
        assert_eq!(pending(&state), None);
        write_meta(&state, &RecoveryMeta::default()).unwrap();
        assert_eq!(
            pending(&state),
            None,
            "a meta without a snapshot is not offered"
        );
        std::fs::create_dir_all(snapshot_dir(&state)).unwrap();
        std::fs::write(snapshot_dir(&state).join("project.json"), "{}").unwrap();
        assert!(pending(&state).is_some());
        discard(&state);
        assert_eq!(pending(&state), None, "discard removes it");
        assert!(!recovery_dir(&state).exists());
        discard(&state); // idempotent
        let _ = std::fs::remove_dir_all(&state);
    }

    #[test]
    fn a_real_project_snapshot_is_offered_and_opens() {
        // The snapshot is an ordinary project: what the shell writes with
        // `write_project_files` (`save_to_dir` underneath) is what
        // `pending` offers and `load_from_dir` reads back.
        let state = tmp("real");
        let model = umber_core::project::ProjectModel::new(
            vec![umber_core::TextureSet::new_default("Set")],
            vec![umber_core::project::TextureSetLayers {
                texture_set: "Set".into(),
                stack: umber_core::LayerStack::default(),
            }],
            Default::default(),
        );
        umber_core::project::save_to_dir(&model, &snapshot_dir(&state)).expect("snapshot writes");
        let meta = RecoveryMeta {
            source_project: None,
            mesh: Some(PathBuf::from("/m/sword.fbx")),
            written_unix: 42,
        };
        write_meta(&state, &meta).unwrap();
        assert_eq!(pending(&state), Some(meta), "offered");
        let back = umber_core::project::load_from_dir(&snapshot_dir(&state)).expect("opens");
        assert_eq!(back.texture_sets[0].name, "Set");
        let _ = std::fs::remove_dir_all(&state);
    }

    #[test]
    fn the_clock_waits_an_interval_skips_strokes_and_unchanged_edits() {
        let mut clock = AutosaveClock::default();
        assert!(!clock.due(0.0, 1, false, false), "clean: never");
        assert!(
            !clock.due(10.0, 1, true, false),
            "the first dirty frame starts the interval"
        );
        assert!(
            !clock.due(10.0 + AUTOSAVE_SECS - 1.0, 1, true, false),
            "not yet"
        );
        assert!(
            !clock.due(10.0 + AUTOSAVE_SECS, 1, true, true),
            "never mid-stroke"
        );
        assert!(clock.due(10.0 + AUTOSAVE_SECS, 1, true, false), "due");
        clock.snapshotted(10.0 + AUTOSAVE_SECS, 1);
        assert!(
            !clock.due(10.0 + 3.0 * AUTOSAVE_SECS, 1, true, false),
            "an unchanged document is not snapshotted twice"
        );
        assert!(
            clock.due(10.0 + 3.0 * AUTOSAVE_SECS, 2, true, false),
            "a new edit is"
        );
        // Saving clears the clock: the next edit waits a full interval.
        assert!(!clock.due(500.0, 2, false, false));
        assert!(!clock.due(501.0, 3, true, false));
        assert!(!clock.due(501.0 + AUTOSAVE_SECS - 1.0, 3, true, false));
    }
}
