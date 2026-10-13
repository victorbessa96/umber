//! umber-app library surface — the paint session for integration tests.
//!
//! The desktop shell is a binary (`main.rs`); integration tests under
//! `tests/` can only link against a library target, so this crate exposes
//! the two modules the Gate-0 journey test drives:
//!
//! * [`paint_state`]: the live paint session (`PaintState` — 4K target,
//!   pressure entry points, stroke undo, the fill-upload path the project
//!   open path restores through).
//! * [`retouch`]: the session's undo-command types (re-exported for the
//!   `paint_state` signatures that name them).
//!
//! Judgment calls (v1 boundaries):
//!
//! * This surface is additive: `main.rs` keeps its own `mod paint_state;`
//!   / `mod retouch;` declarations (same files, compiled once per crate —
//!   legal, and zero risk to the shell). Nothing is moved or re-wired.
//! * Only what the journey test needs is reachable: `paint_state`'s
//!   remaining `pub(crate)` test hooks (`set_brush_params`, `read_tile`,
//!   …) stay crate-private to each crate. The widened seams are
//!   [`paint_state::PaintState::publish_fill`] and
//!   [`paint_state::PaintState::set_brush_color`] (`pub(crate)` → `pub`):
//!   the project open path restores sidecar PNGs through the former, and
//!   the journey test paints its color-identifiable stroke through the
//!   latter — so a save/reopen leg that cannot restore, or a paint leg
//!   that cannot name its color, is not Gate 0.
//! * The eframe shell itself (`save_project` / `open_project`'s dialogs
//!   and the `project.json` commit) stays shell-only; the journey test
//!   replicates their sidecar file round-trip through the same helpers
//!   (`umber_export::png::{write_png, read_png_rgba8}`) and the same
//!   session calls, and names that boundary in its own module docs.

pub mod paint_state;
pub mod retouch;

/// Dynamic material layers: the headless eval driver (Kill A). Declared
/// here — not only behind the binary's `mod` list — so the driver and its
/// goldens are reachable and testable without the shell (the
/// `paint_state`/`retouch` library-surface precedent). Kill B adds the
/// `main.rs` `mod` line and the frame wiring; until then nothing in the
/// binary names this module.
pub mod material_layer;
