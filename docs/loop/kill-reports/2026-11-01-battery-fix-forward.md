# Kill report — 2026-11-01 battery fix-forwards for the 10-31/11-01 parked kills

The 10-31 and 11-01 waves landed under a no-cargo envelope ("rustfmt clean,
no cargo per envelope"), so eight parked kills were committed uncompiled. The
first `cargo check --workspace` in this session found them all; each fix below
is minimal and behavior-preserving.

## Fixes (file: what was wrong → fix)

- `umber-gui/src/theme.rs`: `insert_persistent`/`get_persisted` don't exist in
  egui 0.36 → `insert_persisted`; the read takes `&mut IdTypeMap` in 0.36, so
  `token_overrides` reads through `ctx.data_mut`. (theme live-reload kill
  `1e9155d`)
- `umber-app/src/history_panel.rs`: `HistoryEntry` literals carried
  `target_layer` twice (E0560 duplicate) in the two push sites, and
  `SizedTexture::new` got a `TextureHandle` where `TextureId` is needed →
  single `target_layer: None` per literal, `tex.id()` at the SizedTexture
  site. (history-thumbs kill `01e5bf3`)
- `umber-app/src/layers_panel.rs`: the drag-out consts
  `ASSET_DRAG_IN_FLIGHT_ID`/`ASSET_DRAG_SEEN_ID`/`ASSET_DROP_CONSUMED_ID` lived
  in `mod gestures` but weren't re-exported → added to the `pub use`. (drag-out
  kill `010616c`)
- `umber-app/src/viewport.rs`: `brush_cursor::CursorShape` in a signature with
  the `use` inside the fn body → `crate::brush_cursor::CursorShape`. (OS-bitmap
  cursor kill `6d327e2`)
- `umber-app/src/main.rs`: `poll_theme_file` was declared inside
  `impl eframe::App` (E0407, not a trait member) → moved to the inherent
  `impl UmberApp`; `remove_temp::<AssetDrag>` requires `Default` →
  `remove::<T>` (raw-key removal, no Default bound).
- `umber-cli/src/serve.rs`: `eprintln!("...${security::TOKEN_ENV}")` parsed
  `${` as a format placeholder → `{}` with the value as an arg.
- `umber-cli/src/mcp.rs`: `parse_roots` closure mutably borrowed `i` while the
  loop matched on `args[i]` (E0596/E0503) → plain index walk, no closure;
  `confine_step(&mut wrapped, step_mode(&wrapped))` double-borrowed (E0502) →
  bind `mode` first; `id.unwrap_or(Value::Null)` → `let Some(id) = ... else`.
- `umber-paint/src/paint_state.rs`: 6 test `StrokeCommit` literals missing a
  comma after `depth: N` → commas added (test-only).
- `umber-app/src/preferences.rs`: test literal missing the new
  `ui_scale_default` field → added (`1.0` = the default).
- `umber-gpu/src/graph_eval.rs`: routing-coverage tripwire asserted
  `defs.len() == 25`; the gradient-map kill registered a 26th def
  (`gradient_map`) → bumped to 26. The route table already covers it
  (`CpuPath::GlobalPass`), and the per-def assertion passes.
- `umber-cli/src/workspace.rs`: REAL BUG in the absorbed texturecraft wave —
  `create_dir_all` confined through `confine(Write)`, which canonicalizes the
  *parent* chain, so creating a nested path whose parent doesn't exist yet
  always failed ENOENT (`reads_and_writes_stay_in_the_root` red). Fixed:
  `create_dir_all` now walks components one at a time, canonicalizing each
  existing prefix (symlink escape still caught) and `create_dir`ing the tail.

## Battery state (honest)

- `cargo check --workspace`: GREEN. `cargo clippy --workspace --all-targets`:
  GREEN, 0 errors, 5 pre-existing dead-code warnings.
- Per-crate tests GREEN: core 133, brush 164, gpu (incl. the fixed routing
  test), graph, gui, wasm, spp, plugin-sdk, mesh, bake, cli 57+6+1+10 (incl.
  the fixed workspace root test).
- umber-app suite: NOT completed — Bessa's no-cargo envelope landed mid-run;
  it rides the next battery-capable session before any push.
- rustfmt --edition 2021 --check on all touched files: clean. ftl parity
  834/834 both ways.

## State

LOCAL commit (rides the next wave push). The parked 10-31/11-01 kills are now
compile-clean and their unit tests green; the app-level battery is the named
remaining gate.
