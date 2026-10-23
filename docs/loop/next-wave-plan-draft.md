# Next-wave plan (DRAFT, 2026-10-19) — W-C3a "residuals"

Status: DRAFT for the dragon. Planning only. No cargo was run and nothing
was committed. Every "verified" claim below was checked by grep/read
against the working tree on 2026-10-19, with the file:line cited. Locks
in `/tmp/umber-kill-locks/` could NOT be read from the planning session
(the sandbox refused the path), so **step 0 of every dispatch is
`bash scripts/kill-locks.sh check <the kill's files>`**. Treat any
CONFLICT as authoritative over this plan.

## 0. Ground truth: lane-by-lane verification (the brief's 9 lanes)

| # | lane as briefed | verified state in the tree | consequence for the plan |
|---|---|---|---|
| 1 | per-CHANNEL sizing is the named follow-up | **LANDED** (`f4e7524`). Requirements row 43 marks it. `paint_state.rs:140-167` (module docs), `channel_sizes` BTreeMap `:1122`, `resize_channel`, the sidecar `channel/<set>/<channel>=WxH` lines (`persistence.rs:1008`), the panel's `ChannelSizeChoice` rows (`main.rs:2635`) | queue C1 #4's "per-channel = named follow-up" is STALE. **The real residual is a silent export drop:** `bake_sources.rs:573` `painted_channel_tile` returns `None` when the channel target's dims ≠ `(size,size)`, so a size-overridden channel exports as flat/baked with no notice. `validate_set` (`umber-export/src/driver.rs:584`) enforces ONE size per `MapSet`, so packed outputs (ORM) need a resample. A second residual is per-channel dab density (`paint_state.rs:160-162`, `texels_per_uv_for` `:1612`). Its brush-ring twin is `viewport.rs:1423`, which is LOCKED, so it goes to the tail |
| 2 | winink, the Windows Ink half | open. `winink.rs` is a passive decode with no window hook. `open_system_stream` returns `None` on Windows (`stylus/src/lib.rs:95-118`). C2-30 established that pressure already works through winit `Touch` (`2026-10-15-c2-30-winink.md`). winink's real value is **eraser end, barrel, tilt**, none of which Touch carries | split it: pure flag mapping (Linux-testable) this wave; WndProc subclass + app hook in the tail (needs `main.rs` and an unvendored eframe HWND API) |
| 3 | cached-bake + multi-tile ID-mask | open, both. `paint_state.rs:2390-2398` re-bakes on every click ("no cached store", the named follow-up). `umber-brush/src/id_mask.rs:46-49`: the bake spans the 0..1 frame, so per-tile UDIM bakes are deferred | one kill, `paint_state.rs` only (signature of `bake_and_select_id` unchanged, so `main.rs:580` needs no edit) |
| 4 | `--script` control-call arm (B/15) | **LANDED as protocol form 2026-10-14.** `paint_snapshot` step (`umber-cli/src/agent.rs:1611`) + "a script is a JSONL file piped into `umber-cli agent`" (`2026-10-14-harness-script.md` judgment 1; ROADMAP row `wave-harness-script` ✓). Its open finding (2-point wire strokes paint nothing) is also fixed: `agent.rs:1506` ("Wire strokes are intended geometry: no pointer smoothing") | queue row 15's "still owed" is STALE. Residuals: the report's own deferrals (`snapshot_dir`, per-adapter golden hashes). **Reading stated:** if "control-call arm" meant a GUI-panel snapshot driven by protocol calls, that cannot live in umber-cli (the layering gate keeps egui out of it). It would be a JSONL runner inside umber-app's kittest tests, a design question for the dragon, NOT planned here |
| 5 | more headless panels for the snapshot harness | open. Only 3 baselines exist (`tests/snapshots/`: axis_gizmo, display_panel, psd_import_section). **Doc defect:** `panel_snapshot.rs:7-8` claims a History-panel snapshot that does not exist | one kill, test-only. Baselines need a host generation pass (see §3) |
| 6 | C2-9's desktop check | open, and NOT battery-verifiable: the set-switch glue runs in `App::ui` / `texture_sets_panel_ui` (`main.rs:2501-2544`, locked) with no harness (`2026-10-15-c2-9-stage3b.md` Unverified) | a written manual check script (doc-only) on the existing `cube_2mat_ascii7400.fbx` fixture, bundled with the other owed GUI smokes (C2-1, C2-3) |
| 7 | C2-12 sliders + persisted swatches | open, both. `color_picker.rs:265-290` has the SV square, hue strip, swatch pair and hex, but no HSV/RGB sliders (the g4 spec, `gui-plan-2026-10-15.md:118`). `RecentColors` (`swatch_grid.rs:23`) has **no consumer** outside umber-gui (dead by rule 6) and no persistence | two kills: umber-gui widget, then umber-app sites + persistence. The spec's "inline eyedropper" is EXCLUDED (lane 8) |
| 8 | display-chain-aware eyedropper | EXCLUDED. Live in the tree: dirty `main.rs` (+4, `set_display_settings` at :523) and `viewport.rs` (+132) | **`main.rs` + `viewport.rs` are treated as HELD all wave.** Every kill routes around them |
| 9 | release lane | partner-gated (SPEC ratification C2-23, signing C2-22, the tag C2-25) | non-gated prep only: a docs-only truth refresh of `release-checklist-0.1.0.md`. The version bump stays out (it needs `Cargo.lock` regenerated on the host, the metainfo pair sits beside the flatpak lock, and it's Bessa's) |

**Off-limits surfaces this wave:** `crates/umber-app/src/main.rs`,
`crates/umber-app/src/viewport.rs` (the eyedropper);
`crates/umber-core/src/stencil.rs`, `crates/umber-app/src/stencil.rs`,
`crates/umber-app/src/display_panel.rs`,
`docs/specs/stencil-engine-design.md` (the stencil lane); `flatpak/**`,
`appimage/*.metainfo.xml`, `.github/workflows/packaging.yml` (the
flatpak lane). No kill below writes any of them.

## 1. The kill list (dependency order)

Naming: lock = kill name. Each kill writes its report at
`docs/loop/kill-reports/2026-10-2x-<name>.md` (Built / Judgment calls /
Unverified / Deferred / Gates, the CLAUDE.md shape). The report is in the
lock list.

### Lane P — umber-paint + umber-brush (ID-mask)

**P1 `idmask-tile-cache`**
- Files: `crates/umber-paint/src/paint_state.rs`,
  `crates/umber-brush/src/id_mask.rs` (module-doc boundary update only),
  the report.
- Contract:
  1. **Multi-tile.** `bake_and_select_id(mesh, uv)` keeps its signature.
     It routes `tile` as today. When the mesh spans more than tile 1001
     (`umber_mesh::present_tiles`), it bakes
     `umber_bake::ao::filter_mesh_for_tile(mesh, tile)`. That helper is
     pub and already used by the CLI at `umber-cli/src/main.rs:959`; it
     rebases the tile's UVs onto 0..1. The kill **must rebase the click
     UV by the same integer tile offset** before `select_id_mask`, or the
     wrong texel is picked (select_id clamps an out-of-frame UV to the
     edge). Reuse an existing tile-local helper (`local_texel`'s
     neighbourhood in paint_state, or umber_mesh's udim fns), grep-verified.
     Don't re-derive it. A single-tile mesh takes the whole-mesh path
     unchanged (the CLI's `whole_mesh` precedent), byte-identical to v1.
  2. **Cache.** A single-entry `Option<CachedIdBake { key, tile, width,
     height, bytes }>` on PaintState. The key is content-derived
     *inside* paint_state, because no mesh generation counter exists and
     shell-side invalidation would need `main.rs`: a std `DefaultHasher`
     over `indices` + the `uvs` f32 bit patterns + `positions.len()`.
     The key also includes `IdFlavor`, so a future material/island
     flavor never hits a Triangle bake. Hit = the same (key, flavor,
     tile, dims), so no GPU bake. Miss = bake +
     replace. A resize changes dims and misses naturally. State the cost
     in the docs: one O(tris) CPU hash per click (~20 MB at 1M tris)
     against a 16M-texel compute bake + 64 MB readback at 4K, and 64 MB
     resident for one 4K entry. Single entry is the judgment call: the
     last-clicked tile only, no LRU.
  3. The refusal mid-stroke stays first. The cache is never touched on
     an error path.
- Static verification: rustfmt; verify-kill on both files; a python
  port of the UV rebase (tile 1002 click at u=1.25 → local 0.25 → texel
  `round(0.25*w-0.5)`). Pinned tests:
  - a CPU test of the pure key fn (same mesh → same key; one UV nudged
    → different key)
  - a CPU test of the rebase helper (tile 1001 identity, 1002, 1011)
  - a GPU test (skip-graceful, the paint_state id-mask precedent at
    `:9590`): a two-tile mesh, click on tile 1002 selects the region
    there, and a second click on the same tile is a cache hit, observed
    through a `#[cfg(test)]`/test-hooks counter of bakes run. Count the
    tests in the report.
- Battery: umber-paint (+ umber-brush, docs-only touch).
- **Fallback:** `paint_state.rs` hosts the stencil gate hook (the TAKEN
  lane) and is the likeliest file a sibling holds. If step 0's `check`
  conflicts, P1 moves whole to the §5 tail. Lane P then empties, and the
  battery drops umber-paint/umber-brush. Nothing else depends on P1.

### Lane G — umber-gui (picker)

**G1 `picker-sliders-recents`**
- Files: `crates/umber-gui/src/widgets/color_picker.rs`,
  `crates/umber-gui/src/widgets/swatch_grid.rs`,
  `crates/umber-gui/src/widgets/mod.rs` + `crates/umber-gui/src/lib.rs`
  (only if a new type is re-exported; rule 13 lands registration in
  this kill), the report.
- Contract:
  - HSV and RGB slider rows under the hex field, each track tinted
    across its own channel range: an `egui::Mesh` gradient, the
    SV-square precedent, no texture. RGB rows edit **sRGB-encoded**
    0..255, matching hex. HSV rows edit the kept HSV (hue survives
    greys, as `set_sv`/`set_hue` already do).
  - New pure setters on `ColorState`: `set_srgb_component(i, v)`, which
    keeps the hue on greys through `set_linear`'s rule.
  - A `PickerLabels { rgb: [String; 3], hsv: [String; 3] }` passed in.
    umber-gui has no `tr()`; the app translates.
  - **Additive, no signature break:** a new
    `color_button_with(ui, linear, &PickerLabels, &mut RecentColors)`
    sits beside the UNCHANGED `color_button(ui, linear)`, so umber-app
    compiles at every point of the wave and G1 stays out of umber-app's
    battery scope. A colour is pushed to recents **on popup close when
    it differs from `previous`**, never per drag frame. The popup shows
    the `SwatchGrid` of recents, and clicking one sets the colour.
  - After A3 migrates all three sites, the old `color_button` has no app
    consumer. Rule 6: it becomes a thin delegate to `color_button_with`
    with English-glyph default labels and a throwaway recents. It's kept
    as the stock-button drop-in, and its existing kittests are its
    consumer. A3's report names that state; it is not deleted.
- Static verification: rustfmt; verify-kill. Pinned unit tests,
  hand-derived:
  - an sRGB component round-trip (128 → `srgb_to_linear(128/255)` →
    back to 128)
  - a grey keeps its hue (set R=G=B on a red state; hue still 0, and
    setting S back restores red)
  - recents push-on-close-only (open, drag 3 frames, close → exactly
    1 entry; close unchanged → 0)
  - a kittest that a slider drag changes the value (the SV kittest
    seam at `color_picker.rs:618`)
  - a python port of the gradient vertex colours for one track
- Battery: umber-gui.
- Deferred and recorded in the module docs: inline eyedropper (lane 8),
  the Linear/Perceptual readout toggle, colour-tinted tracks for
  linear-space display.

### Lane A — umber-app (serial inside the lane: one crate scope)

**A1 `channel-export-resample`** (independent; first)
- Files: `crates/umber-app/src/bake_sources.rs`,
  `crates/umber-app/src/export_dialog.rs`, the report. (The requirements
  row-43 note moves to D3: no shared ledger is locked from t0, so the
  eyedropper session can mark its own row.)
- **Scope is wider than overridden channels.** Both readbacks carry the
  same guard:
  - `painted_channel_tile` (`bake_sources.rs:573`)
  - `painted_base_color_tile` (its `target.dimensions() != (size, size)`
    check)

  The export size is the dialog's own preset combo
  (`export_dialog.rs:566`), independent of the set size. So today a
  **4K-painted set exported at 2K drops ALL paint**. Base Color is
  declined by design: `painted_at_size` (`export_dialog.rs:245-252`)
  documents "the driver's painted bridge declines any other size". A1
  closes both.
- Contract:
  - Both readbacks, on a dims mismatch, read back and fit to
    `size×size` through `umber_paint::material_layer::resample_image_rgba8`
    (pub, `material_layer.rs:429`). That function is bilinear and
    texel-centre aligned, and renormalizes when `channel == "normal"`.
    Neither returns `None` for a present-but-differently-sized target.
  - `painted_at_size` becomes "a session exists". Its existing tests
    pin the OLD decline. They are **deliberately re-pinned**, a
    behaviour change named in the report, not a silent test edit.
    Bilinear downsampling of 4K→1K aliases; box-filtering is the
    deferral, documented.
  - The decision is a pure fn `fit_channel_to_export(bytes, (w, h),
    size, channel) -> Vec<u8>` (identity passes bytes through exactly).
  - Export always writes at the export size; per-channel native-size
    export of passthrough files is the deferral, since packed outputs
    force one size anyway.
  - Before claiming the CLI is clean, grep the CLI/agent export path.
    `umber-cli/src/main.rs:965` builds MapSets from bakes only, so
    there's likely no painted-channel drop there. Confirm and record it.
- Static verification: rustfmt; verify-kill. Hand-derived pins, with a
  python port of `resample_image_rgba8`'s formula computing the expected
  bytes:
  - identity byte-exact
  - constant image stays constant on 2→4 (exact)
  - a 2×1 ramp `[0, 255]` upsampled to 4×1 gives the exact edge-clamped
    bilinear bytes
  - the normal arm: a flat (128, 128, 255) stays flat after renormalize
  - the existing GPU test at `bake_sources.rs:1019`/`:1333` keeps passing
- Battery: umber-app.

**A2 `snapshot-breadth`** (independent of A1; serial only for crate scope)
- Files: `crates/umber-app/tests/panel_snapshot.rs`, the new PNGs
  `crates/umber-app/tests/snapshots/{history_panel_empty,history_panel_ops,toolbar,rich_tooltip}.png`,
  the report.
- Contract: add snapshots for headless-constructible surfaces only:
  - `history_panel::history_ui(ui, &mut Document::default(), None)`, which
    makes the module doc's History claim true
  - the same panel after two `doc.run` layer ops
  - `toolbar::show` with `IconAtlas::new()` or `load_embedded_atlas`
    (whichever constructs headless; verify at dispatch)
  - `rich_tooltip::draw` with fixed `TooltipParts`

  Each test is ≤ 15 lines on the existing pattern. **Never regenerate
  `display_panel.png`**: the stencil/eyedropper lanes may move that
  panel, and its drift is theirs.
- Static verification: rustfmt; every API call grep-verified pub in
  `lib.rs` (`history_panel`, `toolbar`, `rich_tooltip` are pub,
  `lib.rs:60-69`); every `tr` key the surfaces render present in both
  catalogs. The PNGs CANNOT be produced host-free (see §3 step 2).
- Battery: umber-app (`--test panel_snapshot`).

**A3 `picker-app-sites`** (depends on G1)
- Files: `crates/umber-app/src/brush_panel.rs` (`:567`),
  `crates/umber-app/src/fill_layer.rs` (`:1161`, `:1182`),
  `crates/umber-app/src/preferences.rs`, `assets/i18n/en-US.ftl`,
  `assets/i18n/pt-BR.ftl`, the report.
- Contract:
  - The three sites build `PickerLabels` from new `picker.*` keys under
    an existing group (kebab attrs, both catalogs).
  - Shared recents live in egui temp memory under one fixed `Id`, so all
    three sites share one list with **no `main.rs` edit**. They are
    lazily seeded on first access from a `swatches` file in the state
    dir: new `preferences::load_swatches/save_swatches`, beside `PREFS_FILE`
    (`preferences.rs:57`), the same `key=value`-free plain format, one
    `#rrggbb` per line, unknown lines skipped. They are saved on change.
  - No new module, so no registration problem. Module graph verified:
    `brush_panel`, `fill_layer`, `preferences`, `bake_sources`,
    `export_dialog` are ALL bin-only (`mod` lines at
    `main.rs:118/121/128/129/142`; none in `lib.rs`). So
    `crate::preferences::load_swatches` called from `brush_panel.rs`
    resolves in the one target that compiles both, and no held-file
    `mod` line is needed.
- Static verification: rustfmt; verify-kill on all `.rs` + `.ftl`. A
  codec unit test (round-trip; a garbage line skipped; a cap-16 trim; a
  missing file → empty). The i18n completeness test covers the keys. A
  python check that both catalogs carry the identical new attribute set.
- Battery: umber-app. Requirements/queue: C2-12 tick at close.

### Lane S — stylus (winink, the host-testable half)

**S1 `winink-pen-flags`**
- Files: NEW `crates/stylus/src/pen_flags.rs` (ungated), `crates/stylus/src/lib.rs`
  (`mod pen_flags;` + re-export, same kill per rule 13),
  `crates/stylus/src/winink.rs` (consume it), the report. Vendoring prep
  (dragon, before dispatch): nothing new needed for S1. The constants
  are already vendored and verified: `PEN_FLAG_BARREL = 1`,
  `PEN_FLAG_INVERTED = 2`, `PEN_FLAG_ERASER = 4`
  (`docs/claw-artifacts/stylus/win32_pointer/windowsandmessaging_mod.rs:5477-5479`),
  `penFlags: u32` (`win32_pointer/mod.rs:305`).
- Contract:
  - A pure, platform-free `PenFlagTracker` (last flags) that maps
    `(down: bool, pen_flags: u32, time_ns)` to a contact event:
    `Eraser { active }` when `INVERTED|ERASER`, else `Contact`, matching
    the XI2 precedent of emitting Eraser *instead of* Contact
    (`lib.rs:37-44`).
  - A barrel edge detector that yields `Button { button: 0, down }`
    only on a transition.
  - The local constant mirrors carry a doc line citing the vendored
    values.
  - `winink.rs` `decode` fetches `POINTER_PEN_INFO` for DOWN/UP too and
    routes through the tracker. This Windows-only change is kept to the
    minimum lines.
- Static verification: rustfmt; verify-kill. The tracker's tests run on
  **Linux** in the battery (ungated): eraser-down, eraser-up, plain tip,
  barrel press/release edges, no repeat on steady flags. The
  `winink.rs` delta compiles **only on Windows CI**. That gate exists
  (verified): `.github/workflows/ci.yml:122-124`, "Stylus winink feature
  tests (Windows only)", `cargo test -p stylus --features winink`. Read
  the delta twice. Every `windows::` symbol must be grep-verified in the
  vendored file.
- **Decision at dispatch, not "optional":** does the host have
  `x86_64-pc-windows-msvc`? (UNVERIFIED; check `rustup target list
  --installed`, no cargo.)
  - **If yes:** the battery adds `cargo check -p stylus --features
    winink --target x86_64-pc-windows-msvc`. Check needs no linker.
  - **If no:** S1 still ships the delta, with the push row naming
    Windows CI as its sole compile gate. A Windows red costs one CI
    round, contained to stylus.
  - The dragon may instead hold the `winink.rs` delta for the tail. In
    that case `pen_flags.rs` ships consumed only by its tests, a rule-6
    note in the report naming the tail kill as its consumer.
- Battery: stylus.

### Lane C — umber-cli (optional; cheap)

**C1 `snapshot-dir`** (the harness-script report's own deferral)
- Files: `crates/umber-cli/src/agent.rs`,
  `crates/umber-cli/tests/agent_paint_ops.rs`,
  `docs/specs/agent-paint-ops-design.md` (§ The snapshot arm), the report.
- Contract: `paint_snapshot` takes exactly one of `out` | `out_dir`.
  Both or neither is a typed refusal before any GPU work. `out_dir`
  writes `<out_dir>/<step:05>.png`, where `step` is the session's 0-based
  step index (reuse an existing counter if `agent.rs` has one; grep at
  dispatch, else add one). `deny_unknown_fields` keeps old scripts
  valid. PROTOCOL_VERSION stays 1 (pre-tag, per the 10-14 report).
- Static verification: rustfmt; verify-kill. A pure test of the
  path-naming fn (step 7 → `00007.png`). A spawn test in the existing
  harness: two snapshots into an `out_dir` → two files, hashes equal the
  replies. The relational-pins rule applies: no golden hex.
- Battery: umber-cli.
- Drop this kill if the wave runs long. It's value-light, and it's
  listed so lane 4 has an honest residual.

### Lane D — docs (no battery legs)

**D1 `desktop-check-pack`**
- Files: NEW `docs/loop/desktop-checks.md`, the report.
- Contract: one executable manual script, each check with steps,
  expected observation, and a result line to fill:
  - **C2-9**: `cube_2mat_ascii7400.fbx`, paint set A, switch, paint
    set B, switch back; Layers follows; neither set's pixels disturbed;
    Save, reopen, both intact
  - **C2-1**: dirty close → Save/Discard/Cancel
  - **C2-3**: kill -9 → "Recover unsaved work?"
  - **S1's** Windows pen eraser end + barrel (the real-hardware leg)

  Doc-only. It closes nothing by itself: it is the instrument a
  desktop session runs.
- Verification: every menu label quoted is grep-checked against
  `en-US.ftl`; the fixture path exists.

**D2 `release-checklist-truth`**
- Files: `docs/specs/release-checklist-0.1.0.md`, the report.
- Contract: grep-verify, then correct, three stale claims:
  - item 6 says NOT MET, but C2-14/15/16 are closed (`queue.md:104`).
    Write it as **"met pending the first drift-hardened nightly green
    on a pushed tip"**, not MET outright: that nightly is still pending
    per the stencil-chain push row (`queue.md:123`)
  - item 8 says "painting reaches Base Color only, export writes Base
    Color only" (`:48-50`), but C2-5 + C2-6 are done (`queue.md:96-97`)
  - Known-deferred lists "PSD import (P3-designed)" (`:138`), but PSD
    flatten-on-import landed `a4bf81a` (`queue.md:75`)

  Gates 7/8/9 stay NOT MET (Bessa's). No version strings touched.
- Verification: census, every corrected claim cited at its landing site.

**D3 `wave-close-docs`** (dragon, LAST, after CI green)
- Files: `docs/loop/queue.md`, `STATE.md`, `CHANGELOG.md`,
  `docs/specs/requirements.md` (row 43: the export boundary A1 closed;
  the C2-12 row),
  `.dragon/COORDINATION-next-wave.md` (consume).
- Contract:
  - queue C1 #4: per-channel ✓ `f4e7524`, plus the export residual
    closed by A1
  - row 15: the `--script` arm landed 10-14, with the residual list
  - C2-12: sliders + swatches
  - C1 #11: cache + multi-tile
  - the push row (§4)
  - STATE "Next": drop the stale "C2-8's drag reorder" (closed in
    `f2c87d4`)
  - the `panel_snapshot.rs` doc fix rides A2, not here

## 2. Lanes, parallelism, sequencing

```
t0  (parallel)  P1 ─────────────┐
                G1 ──────┐      │
                A1 ──┐   │      │
                S1   │   │      │
                C1   │   │      │      D1, D2 (any time, docs)
t1              A2 ◄─┘   │      │
t2              A3 ◄─────┘(G1)  │
t3  ONE battery over P1 G1 A1 A2 A3 S1 C1 (+ D1 D2 riding) ◄──┘
t4  ONE push → CI (ubuntu + windows) → D3
```

- **Parallel at t0:** P1 (umber-paint/brush), G1 (umber-gui), A1
  (umber-app), S1 (stylus), C1 (umber-cli). The file surfaces are
  disjoint and each kill sits in its own crate. Reading of "must not
  share the battery's scope": **no two concurrently-live kills author in
  the same crate**. That satisfies both readings (file locks and suite
  scope), so a red points at one kill.
- **Serial inside lane A:** A1 → A2 → A3. A3 also waits on G1, because
  it consumes `color_button_with`. G1 is additive, so umber-app compiles
  whether or not A3 lands. If A3 slips, G1 ships with
  `color_button_with` consumed only by its kittests, and the C2-12 tick
  waits.
- D1/D2 are doc-only and float. D3 is the close.

## 3. The ONE battery (Gate 1) — procedure and scope

Scope: `bash scripts/wave-battery.sh umber-paint umber-brush umber-gui
umber-app umber-cli stylus`, with C1 dropped → omit umber-cli. Default
features only (no `--features gpu`: the 94469e8 wedge class). The
host's real GPU runs the skip-graceful GPU tests.

1. **Coordinate with the eyedropper owner first.** The battery rsyncs
   the whole tree, so its uncommitted `main.rs`/`viewport.rs` hunks are
   in umber-app's compile. Either its kill commits (or reaches its own
   green) before this battery, or the battery runs with its explicit
   consent, and a red inside its hunks routes to it, not to this wave.
   Write the agreement into `.dragon/COORDINATION-next-wave.md`.
2. **Snapshot baselines (A2), before the battery.** On the host, run
   `UPDATE_SNAPSHOTS=1 cargo test -p umber-app --test panel_snapshot`
   (one cargo command, under the flock). Pull the 4 new PNGs back,
   vision-check each against its contract (widgets visible, not blank;
   the g1-axis-gizmo precedent, `2026-10-15-g1-axis-gizmo.md:119-124`),
   and confirm `git status` shows NO change to the 3 existing PNGs.
   Without this step the battery is red by construction: a missing
   baseline fails.
3. The battery itself, with the scope verified from its LOG.
4. Optional, if the host has the target: the S1 Windows cross-check
   (§1 S1). If it's unavailable, Windows CI is S1's only compile gate.
   Name that in the push row.
5. Fix-forward rounds stay inside the owning kill's lock. Re-pin, then
   re-run.

## 4. Push-row draft (queue push-log style)

```
| W-C3a residuals (2026-10-2x) | push pending | The verified-open residuals after the stale-note sweeps: ID-mask cache + multi-tile (`idmask-tile-cache` — single-entry content-keyed bake cache inside paint_state, per-tile filtered bake with the click UV rebased; single-tile path byte-identical), the export-size paint drop closed (`channel-export-resample` — Base Color and every painted channel now resample to the dialog's export size through resample_image_rgba8 instead of silently exporting flat/baked when the paint size differs; the painted_at_size decline re-pinned deliberately), C2-12's sliders + persisted swatches (`picker-sliders-recents` + `picker-app-sites` — HSV/RGB tinted rows, RecentColors finally consumed, a `swatches` state file shared by all three sites), snapshot breadth (+4 baselines incl. the History panel the harness doc already claimed), winink's eraser/barrel flag mapping (`winink-pen-flags` — Linux-tested tracker; the winink.rs delta Windows-CI-compiled only), `snapshot-dir`, the desktop-check pack + the release-checklist truth refresh (docs) | battery <N> tests / 6 crates (snapshot baselines host-generated + vision-checked first) | pending — Windows CI is the winink delta's only compile gate |
```

## 5. Named follow-ups this wave leaves open

Lock-gated on the eyedropper's `main.rs`/`viewport.rs` (the
post-release tail, next wave):
1. **Per-channel dab density.** The `texels_per_uv_for` channel arm in
   paint_state, plus its brush-ring twin `viewport.rs:1423`, in ONE kill,
   so the ring never disagrees with the dabs. The quick mask stays
   set-scale by design (`paint_state.rs:163-164`).
2. **winink live hook.** A WndProc subclass (`SetWindowSubclass`,
   `DefSubclassProc` ALWAYS called so winit's Touch path survives) in
   stylus, plus the HWND hand-off from eframe's creation context in
   `main.rs`. **API-UNVERIFIED:** eframe 0.36's raw-window-handle
   surface and the `windows` 0.62 `Win32_UI_Shell` feature are not
   vendored. Vendor them into `docs/claw-artifacts/stylus/` before
   dispatch. Real-pen check = D1's Windows leg.
3. ID-mask bake-on-tool-entry (pre-warm the P1 cache when the strip
   enters IdMask; `main.rs:532`).
4. Extracting the set-switch glue (`main.rs:2501-2544`) into a lib fn
   so C2-9's check gains a headless test; until then D1 is the
   instrument.

Deliberate deferrals:
- Per-adapter golden-hash sets (the harness-script deferral, needs one
  run per adapter class).
- A GUI-panel `--script` runner, if wanted (§0 lane 4 reading).
- The picker's Linear/Perceptual toggle and inline eyedropper.
- Per-channel native-size export of passthrough outputs; a box-filter
  downsample (A1's bilinear aliases at large ratios).
- Per-tile channel sizes (`paint_state.rs:166-167`).
- Multi-tile ID regions spanning tiles (P1 selects within the clicked
  tile only).

Partner-gated (unchanged): SPEC ratification (C2-23), the dogfood asset
(C2-24), Windows signing (C2-22), the `0.1.0-alpha.1` bump + tag
(C2-25), and memtest86+ on centralhost.

Stale notes found by this planning pass (D3 fixes them):
- queue C1 #4 (per-channel landed)
- queue row 15 ("--script still owed")
- STATE "Next" (C2-8 drag reorder)
- `panel_snapshot.rs:7-8` (History snapshot claimed; A2 makes it true)
- release-checklist items 6/8 + the PSD known-deferred line (D2)
