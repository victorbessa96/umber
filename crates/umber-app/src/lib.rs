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
//! * The engine modules are re-exports of `umber-paint` (2026-10-14):
//!   one compile, shared by the shell (`main.rs` `use`s the same crate)
//!   and every headless front end. The old dual-declaration (the same
//!   files compiled into both the lib and the bin target) is gone.
//! * `paint_state`'s remaining `pub(crate)` test hooks stay private to
//!   `umber-paint`; the two cross-crate test hooks (`staged_dab_count`,
//!   `two_tile_strip`) sit behind its `test-hooks` feature, enabled only
//!   from this crate's dev-dependencies. The widened seams are
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

/// The paint engine lives in `umber-paint` (extracted 2026-10-14 so the
/// CLI agent and other headless front ends drive the same engine). The
/// re-exports keep every `umber_app::paint_state::…` / `crate::…` path
/// stable for the shell and the integration harnesses (tests/).
pub use umber_paint::{paint_state, retouch};

/// The command palette + the History panel + the document/actions/toolbar
/// types the behavior kittests drive (`tests/panel_kittest.rs`). The shell
/// keeps using them through `crate::`; these re-exports only widen the
/// library surface for the tests.
pub mod actions;
/// The viewport's axis gizmo — on the library surface for the kittest
/// snapshot harness (the pure projection / hit / snap functions and
/// `paint`; the shell keeps driving them through `crate::axis_gizmo`).
pub mod axis_gizmo;
/// The crash-reporter hook and the recent-operations ring: on the
/// library surface so `document`'s session-event recording reaches
/// `crash::note_op` from the lib target too (the shell keeps its own
/// `mod crash;` in main.rs).
pub mod crash;
/// The Display panel + the PSD import section — on the library surface
/// for the kittest snapshot harness (tests/panel_snapshot.rs renders both
/// headless; i18n::init() must run first — the tests call it).
pub mod display_panel;
pub mod document;
pub mod history_panel;
/// The i18n catalog (the snapshot tests init it before rendering).
pub mod i18n;
// `document` names `layers_panel`; `toolbar` names `rich_tooltip`: the
// lib target needs the whole `crate::` closure of the kittest modules.
pub mod layers_panel;
pub mod palette;
pub mod psd_import;
pub mod rich_tooltip;
/// The set-switch glue (the W-C4 L1 extraction): the texture-sets
/// panel's two non-UI cores, lib-reachable for the headless variant
/// (C2-9's named gap) and the headless test beside `gate0_journey.rs`.
pub mod set_switch;
/// The stencil engine's session half (moved from the bin so the
/// set-switch glue — and the headless test — can reach it; the bin
/// re-exports it at its crate root, so every existing
/// `crate::stencil::` path in the shell resolves unchanged).
pub mod stencil;
pub mod toolbar;

/// Dynamic material layers: the headless eval driver, now in
/// `umber-paint` (re-exported for path stability).
pub use umber_paint::material_layer;
