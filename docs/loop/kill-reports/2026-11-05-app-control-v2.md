# Kill report — app control channel v2 + bridge (queue row 153, steps 3-5), 2026-11-05

Session `local-27c7e3d1cd45c392f8dde6c551fb2af65ae2044e1566296b7f45f5084a56a3d1`.
Bessa authorized local cargo build/test in-session ("Cargo build and test is
approved locally"), lifting the 2026-11-02 no-cargo envelope for this session.

## What landed

Per `docs/loop/briefs/2026-11-02-app-control-channel-design.md`:

- **Step 3 `viewport.screenshot`** — egui 0.36 has no callback API; the
  request sends `egui::ViewportCommand::Screenshot(UserData::default())` and
  stashes the reply sender in `UmberApp::pending_screenshot`; `fn ui`
  completes it when `egui::Event::Screenshot { image: Arc<ColorImage>, .. }`
  arrives on a later frame (still the UI thread — no pixels cross threads by
  hand). The ColorImage is rasterized to RGBA8, encoded by the new in-memory
  `umber_export::png::encode_png` (the file-writing `write_png_rgba8` now
  delegates to it), base64'd (`base64 = "0.22"`, already in Cargo.lock), and
  replied as `{ok, png_base64, width, height}`.
- **Step 4 `ui.inspect`** — drained on the UI thread at the top of `fn ui`
  (`drain_control`, the `UMBER_THEME_FILE` poll's pattern): dock leaves
  (path, rect, tabs, active), active tool/strip/preset, brush params, active
  set + layer census (id/name/kind/visible/opacity/blend/masked), dirty and
  mesh-path flags. Read-only.
- **Step 5 `--bridge <port>`** — loopback MCP relay: token-auth first frame
  like `--control`, then the app spawns `umber-cli mcp` lazily on the first
  authed client and pumps bytes verbatim both ways (client→child stdin,
  child stdout→client). One client at a time (`BridgeState` mutex); the
  child is kill+wait()ed by the pump thread on client EOF (and on the no-pipes
  early path), so it never zombies.
- Arg parsing accepts `--control <port>` / `--bridge <port>`, bare ports only.
  Startup failures are fatal with a clear message.

## Latent breakage this battery surfaced (all pre-existing, all fixed)

The 2026-11-01 no-cargo envelope meant the last full umber-app/umber-cli
battery predated several parked kills; CI has been red since 2026-11-02
(clippy). This session's batteries found and fixed:

1. **Row 151 re-export gap** — `run_preset_tiled_dilated`, `DilateHook`,
   `DEFAULT_PADDING_TEXELS` (and the other `*_dilated*` fns) were `pub` in
   `umber-export/src/driver.rs` but missing from `lib.rs`'s `pub use`, so
   umber-cli and umber-app **did not compile** (E0425). Added the whole
   dilated family to the re-export list.
2. **CI clippy red** — `write_outputs` 8-arg `too_many_arguments`
   (pre-existing; umber-export had never been in the gated crate list) →
   `#[allow]` with a comment; umber-core windows-only lints (`unused import
   Read` under cfg(unix) use, `fn_to_numeric_cast` on `generate_token as
   usize`) → cfg-gated import + fn-pointer cast; umber-core test
   `to_string`-on-Display lint → `&token[..8]`; umber-app dead-code (test-only
   `PropKeymap::get`, `ControlServer.token`/`BridgeServer.token` fields) →
   `#[cfg_attr(not(test), allow(dead_code))]`; umber-cli absorbed-but-unwired
   helpers (`generate_token`/`client_token`, workspace `read`/`write`/
   `create_dir_all`/`list_dir`/`MAX_READ_BYTES`, re-exported `Permit`) → same
   treatment; my own control.rs imports + `Error::other` + zombie-process
   lint fixed.
3. **Two never-green tests** (landed 10-31/11-01, battery-pending since):
   `viewport::tests::angle_chord_sweeps_linearly_and_clamps` — `angle_step`
   had the drag sign inverted vs its own test contract (egui dy is
   screen-down-positive; drag up must sweep +): impl fixed to
   `angle0 - dy*360/SWEEP`. `brush_cursor::tests::ring_bitmap_...` — the rim
   pixel at d = r+0.51 (even-side bitmap) fell in the dark band, not the
   light one: light rim band widened 0.5 → 0.75*scale. Also a `#[test]`
   attribute restored on `flow_chord_scales_alpha_exponentially_and_clamps`.
4. **Fonts gate vs fresh environments** — `tests/support::fonts_gate`
   (428cdc2) panicked when `assets/fonts/*.ttf` are absent, so kittest could
   never run on a fresh CI runner; the snapshot baselines (2026-10-23)
   predate the gate and are font-dependent. The gate now checks
   `umber_fonts_present()` (fonts_dir + INTER_PATH/MONO_PATH leaves): present
   → install + assert as before; absent → keep egui stock fonts, set
   `FONTS_REALIZED=false`, and `support::snapshot()` skips the pixel
   comparison with a loud eprintln. The `font-assets` follow-up (pin
   canonical OFL TTFs, re-record baselines) stays open — see queue.

## Verification (real runs, this session)

- `scripts/wave-battery.sh umber-core umber-color umber-graph umber-spp umber-export umber-cli umber-gpu umber-app` → **GREEN** (layers 16, fmt, clippy -D warnings per crate, feature-on compile, tests: core 142, color 20, graph 142, spp 48, export 89, cli 97, gpu 151+213(gpu leg), app 932 incl. 14 new control/bridge tests).
- `cargo clippy --workspace --all-targets -- -D warnings` → **GREEN** (0 errors).
- `cargo fmt --all -- --check` → clean. `scripts/check-layers.py` → GREEN x16.
- `cargo test --workspace --no-fail-fast` → green except panel_snapshot pixel
  comparisons, which skip loudly without canonical fonts (see above); with
  `UMBER_FONTS_DIR=~/.local/share/umber-fonts` (OFL Inter + JetBrains Mono
  fetched from upstream this session) kittest passes fully and the snapshot
  drift vs the pre-gate baselines is visible — expected until baselines are
  re-recorded under pinned fonts.
- Control channel unit tests: auth-first (wrong/missing token → unauthorized
  + close), ping/version/methods, ui.inspect shape, screenshot round-trip
  (PNG magic + dims via encode_png), bridge handshake + verbatim relay +
  child respawn — all in `control.rs`'s test module.

## Not done here

- Row 152 (Hermes `mcp add` registration) needs a fresh **release** build of
  umber-cli; queued next.
- Desktop live smoke of `--control`/`--bridge` added as desktop-checks.md
  check 5 (needs a desktop session with the app running).
