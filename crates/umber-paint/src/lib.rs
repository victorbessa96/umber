//! Umber's headless paint engine — the product's core, outside the GUI.
//!
//! Extracted from `umber-app` (2026-10-14) so every front end drives the
//! SAME engine: the egui shell, the `umber-cli agent` JSONL session (agent
//! paint ops), perf harnesses, and future automation. Nothing here
//! depends on egui, eframe or windowing; GPU work rides `umber-gpu`'s
//! wgpu re-exports, and points are `emath::Pos2` (identical to
//! `egui::Pos2`, so the shell passes its points through unchanged).
//!
//! - [`paint_state`] — the paint session: per-layer planes keyed
//!   `(owner, tile, channel)`, strokes and retouch routing, the
//!   composite-on-read flush, tombstones, anchors and instances (the
//!   per-layer pool design, docs/specs/per-layer-pool-design.md).
//! - [`retouch`] — smudge/clone/erase stroke mirrors and their undo
//!   records.
//! - [`material_layer`] — the dynamic-material refresh driver.
//! - [`snapshot`] — the stack → composite-rows builder (shell + agent).
//! - [`persistence`] — the `paint/` sidecar: byte-exact planes, the
//!   two-phase commit, the dual-read loader (one implementation for the
//!   shell's Save/Open and the agent session's `project_save`).

pub mod material_layer;
pub mod paint_state;
pub mod persistence;
pub mod retouch;
pub mod snapshot;
