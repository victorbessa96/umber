# Kill report (IN FLIGHT, not committed): G5 workspace layouts persist

Date: 2026-10-15. Author: dragon. Locks: `g5-workspace-persist`
(Cargo.toml, Cargo.lock, `crates/umber-app/{Cargo.toml, src/main.rs,
src/workspaces.rs}`, the GUI plan, this report) and
`g5-workspace-persist-sm` (`shell_menu.rs`).

## State

The edits are in the working tree, NOT committed. The build host
(192.168.15.64) went down mid-battery (SSH refused, then no ping) at
about 18:43. Gate 1 has not run on these edits, so they are not
verified. A watcher (`proc_3c3ed24bb38b`) re-runs the battery when the
host returns.

## Built (unverified)

- `egui_dock`'s `serde` feature. The Cargo.lock delta was resolved ON the
  build host (`cargo metadata`, online):
  - it adds `enumn 0.1.14` (dtolnay, MIT OR Apache-2.0, checksum pinned
    in the lock);
  - it adds `serde` edges inside the egui stack.
  A `cargo check -p umber-app` passed on the host with the feature on,
  before the outage.
- `Panel` and `Workspace` derive serde.
  - `Workspaces::snapshot(live)` / `restore(saved)`.
  - `save_layouts` writes `<state>/layouts.json` (write-then-rename);
    `load_layouts` ignores a missing, malformed or other-version file.
  - `restore` drops any stored layout that lost a panel, so no panel can
    go missing.
- `FocusState::real_layout`: a save during a focus mode keeps the real
  layout, never the one-tab stand-in.
- The shell saves on every workspace switch and in `eframe::App::on_exit`.

## The `on_exit` signature risk: RESOLVED from evidence (no cargo)

eframe 0.36.2 calls `on_exit(None)` with its `glow` feature and
`on_exit()` without it (`native/wgpu_integration.rs:588-592`). Nothing
enables `glow`:

- the workspace dep is `eframe = "0.36"` (defaults only);
- the only other eframe dependent, egui_kittest 0.36.2, pulls
  `eframe/accesskit` + `eframe?/wgpu` and declares eframe
  `default-features = false`;
- the compiler's own record agrees. Every
  `target/debug/.fingerprint/eframe-*` (local builds 2026-10-09..11,
  read-only) lists `accesskit, default, default_fonts, links, wayland,
  web_screen_reader, wgpu, wgpu_no_default_features, x11`, with no `glow`.

`egui_glow`/`glutin` appear in Cargo.lock only because the lock records
optional deps; they are not built. So `fn on_exit(&mut self)` is the
compiling signature.

## Tests written (not yet run)

- `layouts_round_trip_through_a_file_with_the_edit_and_the_active_workspace`.
- `a_bad_layouts_file_falls_back_to_the_presets` (malformed JSON, another
  version, a lossy layout).

## Plan-file edits owed by this kill's commit (it holds the plan lock)

Doc drift found during the host outage, to apply in the same commit:

- `g4-color-picker`: ✓. The widget (C2-12), registered in `umber-gui`
  (wire-unlocked), and the popup used at all three sites (W-C2c).
- `g4-curve-widget`: ✓. Promoted into `umber-gui` and registered
  (wire-unlocked).
- `g5-workspaces`: ✓ once this kill's battery is green (F1–F4 viewport
  presets are still a separate open line inside the row).

## A design bug caught in static review (before any run)

`restore` dropped any stored layout missing a panel, and the test pinned
that behaviour. But Window ▸ Panels HIDES panels by removing their tab
(`toggle_panel`), so a user who hid History would have had their whole
layout silently reset on every launch: a persistence feature that
forgets exactly the edit it exists to remember.

Fixed: `is_showable` accepts any layout with ≥1 tab and no duplicate
panel, so a hidden panel survives. A duplicate (only a hand edit can
make one) still falls back to the preset. The test now pins both: hidden
History survives the reload, and a doubled Viewport falls back.
