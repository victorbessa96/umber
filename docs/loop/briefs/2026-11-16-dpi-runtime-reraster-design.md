# Design brief: DPI runtime re-raster (ROADMAP OPEN row, 2026-11-16)

Status: BRIEF for the next kill. No cargo run, nothing committed with it.
Verified against the working tree at `e6a77f08` on 2026-11-16.

## The question

`docs/ROADMAP.md:301` — "DPI runtime re-raster — OPEN (the manual
`ui_scale` exists; the runtime-change re-raster does not)." When the window
moves between monitors with different scale factors, egui's
`input.pixels_per_point` changes mid-session. Today nothing watches that:

- `crates/umber-app/src/main.rs:2531` reads `pixels_per_point` once at
  startup (`hidpi = input.pixels_per_point >= 1.75`, bakes the 2x icon
  atlas accordingly). `crates/umber-gui/src/icons.rs:36` names the follow-up
  in its module docs: "mid-session on `pixels_per_point` movement is a
  follow-up."
- `preferences::style_delta(&prefs, ctx.zoom_factor(), ...)`
  (`main.rs:2875-2886`) recomputes `zoom` from `ui_scale` only when prefs
  change; it does not observe `pixels_per_point`.
- The paint layer rasters are resolution-dependent (channel_sizes in
  `paint_state.rs`), so a runtime scale change must invalidate/re-raster
  them, not just rescale the UI.

## What exists (tree-verified)

- `Preferences { ui_scale, ui_scale_auto, ui_scale_default }`
  (`preferences.rs:117-125`), persisted as `ui_scale=...` lines
  (`preferences.rs:201-203`), edited in the panel at `main.rs:2745-2780`.
- Startup hidpi atlas pick: `main.rs:2525-2536`.
- Per-frame style sync: `main.rs:2873-2900` (`style_delta` →
  `ctx.set_zoom_factor(zoom)`).
- Paint state: `paint_state.rs` `channel_sizes` BTreeMap; bake path via
  `bake_and_select_id` etc.

## Target

1. Each frame (or on egui's `RawInput.pixels_per_point` change), compare
   `ctx.input(|i| i.pixels_per_point)` against the value the current zoom
   was computed from. On change:
   - recompute the effective zoom = `ui_scale * pixels_per_point` (verify
     against `style_delta`'s formula first — do not invent a second one),
   - `ctx.set_zoom_factor(...)`,
   - swap the icon atlas 1x/2x to match the new hidpi class (re-run the
     `main.rs:2531` logic lazily — the atlas load fn is
     `umber_gui::icons::load_baked_atlas_merged`),
   - invalidate and re-raster paint layers at the new resolution (find the
     existing re-raster/invalidate entry point in paint_state; the
     projection `disarm` precedent at `projection.rs:79` shows the
     re-rasterize-on-Apply shape).
2. Manual `ui_scale` edits keep working unchanged (the existing
   `prefs != before` branch).
3. `ui_scale_auto` (default on) means "follow the OS scale"; when it is
   OFF, a runtime `pixels_per_point` change must NOT override the manual
   zoom (only the manual pref does).

## Concurrency / security / migration

- Single-threaded UI frame path; no new channels. No persistence format
  change (prefs lines unchanged). No network surface.
- Migration: none — pure runtime behavior.

## Verification

- Battery: rustfmt --check on touched .rs, `scripts/check-layers.py`,
  `scripts/scorecard.py --check`, crate-scoped clippy + tests with
  `UMBER_ASSETS_DIR=<repo root>` under `flock /tmp/umber-cargo.lock`,
  full battery per `references/pre-commit-verification.md`.
- Ad-hoc probe (headless control channel): launch with
  `XDG_RUNTIME_DIR` redirected, change the window scale via the control
  port if a command exists, else assert the frame-loop watcher fires on a
  synthetic `pixels_per_point` change in a unit test (egui `Context` can
  be driven headless: `ctx.run(...)` with a crafted `RawInput`).
- kittest asserting `zoom_factor` follows `pixels_per_point` when
  `ui_scale_auto` and does not when off.

## Non-goals

- No per-monitor independent scaling of the canvas texture beyond what
  the re-raster does.
- No Windows-only winink hook work (that is L4, gated).
- No prefs-format change.

## Kill surface (disjoint)

`crates/umber-app/src/main.rs` (frame watcher + atlas swap call),
`crates/umber-gui/src/icons.rs` (expose a reload fn if the atlas is
cached), `crates/umber-app/src/preferences.rs` (`style_delta` gains the
ppp input or a sibling fn), `crates/umber-paint/src/paint_state.rs`
(re-raster entry), the new test file, the kill report, queue row.
Off-limits per standing walls: `flatpak/**`, `.github/workflows/packaging.yml`,
the stencil lane files.
