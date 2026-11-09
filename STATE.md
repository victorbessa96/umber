# umber — State

> Updated every wave. This file is the bridge across context windows. Read this first on any new session before deciding what to do.
>
> ARCHITECTURE NOTE: this file was rewritten 2026-10-12 (A1 of the core-loop
> queue) from CURRENT TRUTH after the strategy review found it
> self-contradictory (status line said "wave 3 of 6" while the handoff said
> everything through wave 6 was built; Oct-9 overnight-grant language was
> still live). The wave-history narrative lives in CHANGELOG.md and
> DECISIONS.md; this file carries ONE current picture.

## Status line

Current phase (2026-11-09): **the tree lives on the HDD and the work is
planned in waves.** The tree moved 2026-11-09 to
`/mnt/hdd/data/Projects/umber` (`~/Projects/umber` is now a symlink;
MCP server + watch cron resolve through it; report
`docs/loop/kill-reports/2026-11-09-tree-move-to-hdd.md`). The decal
projector lane (R-A2) is **CLOSED to v2d + W1.1**: the one-pass raster
(`98fb6e8e`, CI `42646713045`; ~34 min UI freeze at 4K → ~0.6 s) and the
UV triangle conditioning rule `UV_COND_EPS = 1e-6` (`7aad67c3`, release
CI `42677040220` GREEN — needle triangles are not valid footprints, CPU
and GPU agree exactly). The plan of record is
`docs/loop/waves/2026-11-09-wave-plan.md` (six waves: W1 correctness —
1.1 landed, 1.2 the malformed-triangle policy asymmetry next; W2 the
R-A2 close; W3 the wave-abr tail; W4 the Wave-9 lanes; W5 the power-user
tail; W6 the gated rows). Live row log: `docs/loop/queue.md` (rows
307-310). Agent contracts: CLAUDE.md = AGENTS.md (kept identical).

Previous phase (2026-11-06): **charter-v2 T1-T3 LANDED (`5af3c84`, pushed
through the new `scripts/safe-push.py`)**: bake/export by command id +
`job.status`, the e2e smoke in CI, the push guard (see the 2026-11-06
local-3230b69f section below). Before it (2026-11-05): the UI/UX pass, waves
U1-U4 LANDED; U5 parked on egui_dock.
The status paragraph that follows is the 10-27 to 11-01 picture, kept for
its open tails.

Earlier phase: **THE FRESH CHARTER CYCLE — L6 THE UNIFIED TIMELINE
LANDED (2026-10-27, `75c9594`).** Bessa's launch word opened the
cycle ("continue working autonomously... all the claws"); the W-C4
runway's L6 — the graph journal joining the History window as the
timeline's third arm (layers + strokes + graph, interleaved by
op_id, one Ctrl+Z across all three) — landed through the opencode
claw (deepseek-v4.1-flash-free; the claude lane is auth-dead pending
Bessa's re-login), dragon-reviewed across three fix-forward rounds
(the `&mut dyn` elided-object-lifetime trap; the queue test's
fixture lie), battery GREEN: umber-app 873 tests + clippy + the gpu
leg. The 10-24/25 storm before it — the six-lens audit + ROADMAP
refresh, R-B1 flatten, R-C1 Assets browser v1, symmetry persistence,
the fill channel target, the material picker v1, the OccluderScope
row, the pressure curve, L2b, the L3b ring, the pick's toast — all
both-gates-green (the coordination body + kill reports carry the
ledger). The W-C3b sweep and the flatpak five-wall chain stay closed
(history: the 10-23 status era + CHANGELOG).

**2026-10-29 wave (local-4e9bde4c, `ad6f44b`):** g5-viewport-presets —
the g5-workspaces open tail, F1/F2/F4/F5 registry actions over the 3D/2D
views, applied to the active workspace's live dock and persisted — plus
l6-low-followups (the pi review's two LOWs: the History-arm same-frame
graph drain; the load test's undrained-pending pin). One commit for both
kills (they share main.rs). Battery GREEN: umber-app 879 tests 0 failed
+ clippy + fmt + layers. Desktop checks 1-3 deferred — Bessa at the
desktop, no screenshots per his word.

**Next:** the open tail — g5-workspaces CLOSED 10-29 (`ad6f44b`) ·
g7-keyboard-only CLOSED 10-30 (`e008dcc`) · the
Painter-faithful default layout CLOSED 10-30 (`0f18fa7`, the viewport
dominates the centre; LAYOUTS_VERSION 2 resets saved v1 layouts) ·
the 10-30 perf-nightly red VERDICTED FLEET, not code (the bench binary
is byte-identical across a red and a green reading; the §12 gate's
measured noise band is x1.51 against a x1.10 band — the instrument fix
P1..P4 plus the reseed is queued as its own two-gate wave; leg log in
docs/perf/protocol.md) · the perf-gate instrument wave CLOSED 10-30
(`0981d2f` — P1 landed: the nightly's verdict is now the same-job A/B
RATIO current/reference, band x1.10, not the absolute band; the
absolute compare is report-only) · quick-mask persistence LANDED 10-31 (`73f8724`, the `paint/quick_mask/` 8-bit sidecar; undoable mask strokes = the named follow-up) · the follow-up
queue (the browser's saved searches/custom dirs/import dialog/kittest +
mask thumbnails LANDED 10-31; the flatten's compound one-step shipped
earlier as `run_grouped` — queue line stale; remaining: none of the named tails — drag-out LANDED 10-31 (local unpushed); OS-bitmap brush cursor LANDED 10-31 (local unpushed); per-entry history thumbs LANDED 10-31 (local unpushed); zoom-to-cursor LANDED `1730125`) · the g1 Ctrl+LMB chords LANDED 10-31/11-01 (`9d70bbc` flow + `3d9ed88` angle, local unpushed, ride the next wave push) · the desktop-checks; orbit-around-last-stroke LANDED 11-01 (`173fc3b`, local unpushed) — g1-nav-presets row closed
Result lines x4 (need the live desktop) ·
queue.md's push-log row for this wave (rides the next docs pass) · the
CLAUDE.md trap-line addition (the `&mut dyn` object-lifetime trap —
a protected-file write, Bessa's hands) · the different-family
cross-review debt if the pi attempts stay dead (the claude lane's
revival takes it). centralhost: HOST_BACK 2026-10-21 ~13:32,
unreachable since — FRAGILE, memtest86+ owed at Bessa's next
physical access (boot straight into it, skip the OS), checking STOOD
DOWN by his order. The release lane stays Bessa-gated: SPEC
ratification (SPEC.md still DRAFT), the dogfood asset, Windows
signing, the alpha tag.

## 2026-11-06 (local-3230b69f, Razul) — charter-v2 T1-T3: bake/export by id, e2e smoke, safe-push

- **T1, `bake.run` / `export.run`** (audit A2-4's open tail): command ids that
  run the Bakes / Export panels' own button paths with their current
  settings, disabled with the button's own reason. Read-only control method
  `job.status` reports both jobs (monotonic completed counters, absolute
  output paths): read it, `command.run`, wait for the counter to grow.
- **Two user-visible bugs found by the smoke, fixed at the root.**
  `FillParams` derived `Default`, so every fill built in code (Layers > Add
  fill, `layer.add_fill`, the agent) targeted channel `""` and composited
  onto nothing; only projects loaded from disk got `baseColor`. And a fresh
  session's tile 1001 stayed in the placeholder set, so the first mesh's set
  owned no tile (`PaintState::adopt_placeholder_tiles`).
- **T2, `scripts/e2e-smoke.py`**: release GUI over `--control`, cube + fill
  + bake + export, asserts files, non-black base colour and a bright 3D
  viewport crop. CI runs it on Linux under xvfb-run. Mutation-proven: with
  the derived Default restored it fails.
- **T3, `scripts/safe-push.py`**: the only push path (ledger by session with
  patch-id for rebases, fast-forward, fmt/scorecard/layers on the pushed
  commit's tree, pushes the checked SHA). 12 scratch-repo tests.
- Open: none of the T1-T3 debts. The Export button flush and the
  failed-job `job.status` fields closed in row 237; the export-longer-
  than-30 s control timeout closed in row 246 (`JOB_REPLY_TIMEOUT` for
  `bake.run`/`export.run`, pinned by `job_commands_get_the_job_reply_
  timeout`). Spec §8's A2-4 row was marked fixed in row 237.
- Evidence and the kill table: `docs/loop/kill-reports/2026-11-06-charter-v2-t1-t3.md`, queue row 229.

## 2026-11-06 (local-1625-rz6f2a, Razul) — rows 246, 247, 248 + the 2026-11-06 cargo clean

- **Row 246 LANDED+PUSHED `567a7db`:** the last T1-T3 ROADMAP debt closed —
  `command.run` of `bake.run`/`export.run` over `--control` waits
  `JOB_REPLY_TIMEOUT` (30 min), not `ui_timeout after 30s`
  (`control.rs`: `JOB_COMMANDS`, `reply_timeout_for`, `forward_ui_with`;
  pin `job_commands_get_the_job_reply_timeout`). Stale hunt b09ae253
  swept as completed (landed 0981d2f + 6ef5b3d).
- **Row 247 LANDED+PUSHED `ec45651` (+`22255d4` row citation):** installers
  gap #1 — `--version`/`-V` on both binaries via
  `umber_core::control::is_version_arg` (pin `version_flag_predicate`);
  live-verified on the dev bins, headless-safe.
- **Row 248 (installers gap #4) in flight, lock `razul-bench-budget`:**
  bench reference mesh resized 64→18 subdivisions (49,152→3,888 tris) to
  fit `MAX_TRIS_PER_BAKE` (4096); pin
  `the_reference_mesh_fits_the_bake_paths_triangle_budget`. Battery log
  /tmp/umber-bench-battery.log. Queue row + COORDINATION line deferred:
  queue.md/COORDINATION are held by sibling lock `razul-bundled-assets`
  (dirty uncommitted bundled-assets work, no live process at claim).
- **Housekeeping per Bessa:** `cargo clean` removed 105,557 files /
  72.3 GiB from `target/`; / went 94% → 55% (14G → 101G free). Registry
  kept for offline rebuilds. Next battery rebuilds target from scratch.
- Next: installers gap #5 (no-display launch poisons GPU fallback to GL),
  gap #2 (exe-relative runtime assets, sibling lane), row 242 split-redo
  wiring; v0.1.0 tag stays partner-gated.
- **Row 249 (installers gap #5) in flight, lock `razul-headless-persist`:**
  gpu_startup.rs — headless launches (no WAYLAND_DISPLAY/WAYLAND_SOCKET/
  DISPLAY) never persist the backend (`Decision::persist_on_success_in` +
  `has_display()`; `Session::new` gates `persist`). Pins
  `a_headless_launch_never_persists_its_fallback` +
  `confirmation_writes_gpu_backend_only_when_the_session_carries_a_persist`.
  main.rs untouched (sibling-locked); prepare() signature unchanged.
  Battery log /tmp/umber-headless-battery.log. Queue row + COORDINATION
  line deferred until `razul-bundled-assets` releases.
- **Row 250 (row 232's tail) in flight, lock `razul-export-failed-flag`:**
  `job.status` export object gains `failed` — `ExportDialog::last_export_failed`
  set in export_now's Err arm, cleared on Ok; `commands.rs` wires it into
  `job_status_json` (signature unchanged, main.rs untouched). Pins in
  export_dialog.rs (`a_failed_export_reports_no_files_...` extended +
  `the_failed_flag_starts_clear`). Battery log
  /tmp/umber-exportflag-battery.log. Queue row + COORDINATION line deferred
  with the sibling lock.
- **Docs kill (lock `razul-roadmap-splitredo`, docs-only, no battery per the
  8af2058 receipt rule):** ROADMAP split-redo debt marked ENGINE LANDED
  (`850b839`, rows 167/242 engine; History-panel wiring still waits on the
  sibling's document.rs/main.rs lock); merge-below-top annotated OPEN with
  its retouch.rs:658 precondition (redo-top accessor in umber_core::undo).
- **Row 251 (row 222(c) seam-clip golden) in flight, lock
  `razul-udim-seam-golden`:** tests-only pin
  `udim_seam_dab_clips_at_the_tile_boundary_and_mirrors_byte_exact` +
  `udim_seam_quad` fixture in paint_state.rs: a dab near a UDIM seam clips
  at its own tile and mirrors byte-exact into the neighbor tile (checked
  through the engine's own `mirror_uv`); seam-free control leaves tile 1002
  untouched. Battery /tmp/umber-udim-battery.log (paint 194/0 incl. pin).
  Queue row + COORDINATION line deferred with the sibling lock.
- **Row 252 (redo-top accessor) LANDED `03181ec`:** `UndoStack::peek_redo` +
  `redo_len` in umber-core undo.rs — the named precondition (retouch.rs:658)
  for lifting split_redo's `RedoPending` refusal and for the merge-below-top
  debt. Pin `peek_redo_reads_the_next_redo_without_touching_the_stacks` green
  (worktree battery showed 192/0 with only the sibling-dirty lib.rs breaking
  bundled::tests; commit-tree safe-push gates green on all 7 crates, pushed
  8eee11b..03181ec). Queue row + COORDINATION line deferred with the sibling
  lock.
- **Row 253 (cli paint_stroke radius validation pins) in flight, lock
  `razul-cli-radius-pins`:** session-level pin
  `paint_stroke_validates_radius_fields_before_any_project_or_mesh` — the
  conflict / negative / zero-radius / zero-scale rejections all fire BEFORE
  any project or mesh, and a valid radius reaches only "no project open".
  Closes row 208's cli-wiring follow-up at the validation seam. Battery
  /tmp/umber-cliradius-battery3.log: clippy 0, 8 suites ok 0 failed (bin
  94/0 incl. the pin). Queue row 253 + COORDINATION line deferred with the
  sibling lock.
- **Row 253 LANDED `f340fdd`** (cli radius pins pushed 03181ec..f340fdd).
- **Row 254 (split_redo RedoPending lift) in flight, lock `razul-splitredo-lift`:**
  `UndoStack::take_redo`/`restore_redo` (undo.rs, pin
  `take_redo_restore_redo_roundtrip_preserves_order_and_budget`) +
  split_redo now preserves pending redos below the split parts instead of
  refusing; `SplitOutcome::RedoPending` retained-but-unreturned (sibling's
  History match arms still compile). Rewritten test proves redo_len 1+3 and
  byte-exact replay of the preserved stack. Battery
  /tmp/umber-splitlift-battery4.log (core 187/0 modulo the sibling-dirty
  worktree bundled:: failure; paint 194/0). Queue row 254 + COORDINATION
  line deferred with the sibling lock.
- **Row 254 LANDED `1347e13`** (split_redo RedoPending lift pushed
  f340fdd..1347e13).
- **Docs kill (row 255) in flight, lock `razul-roadmap-mergebelow`:**
  ROADMAP merge-below-top updated — primitives landed (peek_redo/redo_len
  03181ec, take_redo/restore_redo 1347e13); remaining blocker named as a
  no-reapply splice in umber_core::undo. Docs-only, no battery per the
  8af2058 receipt rule. Queue row 255 + COORDINATION line deferred with the
  sibling lock.
- **Row 255 LANDED `6b5af7b`** (ROADMAP docs kill pushed 1347e13..6b5af7b).
- **Row 256 (merge-below-top splice primitive) in flight, lock
  `razul-merge-below-splice`:** `UndoStack::merge_below_top(k, doc, build)`
  + `push_unapplied` in undo.rs — rewrites the entry k below the top without
  re-applying the entries above, preserves pending redo, declined/out-of-
  range leaves the journal untouched; both pins green (core 189/0 modulo the
  sibling-dirty worktree bundled:: skip). ROADMAP debt updated: wiring
  record_stroke to use the splice is next. Battery
  /tmp/umber-mergebelow-battery4.log. Queue row 256 + COORDINATION line
  deferred with the sibling lock.
- **Row 257 (record_stroke below-top wiring) LANDED `bbdc08b`, push in
  flight (safe-push proc_95bba5ac8a64), lock `razul-recordstroke-below`:**
  `UndoStack::peek_undo_at(k)` added in undo.rs; `record_stroke`
  (retouch.rs) now scans a bounded window (8) below the top and splices
  the stroke into an older entry via `merge_below_top`, guarded by: redo
  stack empty, and the stroke intersects no entry above the target
  (redo-clobber guard). Pins:
  `record_stroke_merges_below_top_when_the_stroke_skips_the_top_entry`
  and `record_stroke_stacks_when_the_stroke_touches_an_entry_above_the_target`,
  both byte-exact roundtrip. Battery /tmp/umber-below-battery4.log:
  clippy -D warnings 0, core 189/0 (bundled:: skipped, sibling-dirty
  worktree), paint 196/0.  ROADMAP merge-below-top debt marked WIRED.
- **Row 258 (docs kill, stale ROADMAP debt) LANDED:** the AssetStore GC
  shell caller landed `1f0e438` (row 238) — `umber gc` headless lane;
  live-verified 2026-11-07 on the dev binary (dry-run JSON exit 0,
  missing dir usage exit 2). ROADMAP row marked landed. Docs-only, no
  battery per the 8af2058 receipt rule. Queue row deferred with the
  sibling lock.
- **Row 259 (docs kill, stale ROADMAP rows) LANDED `9d998569`:** quick-mask
  persistence landed `73f8724` (in origin/master) — `paint/quick_mask/`
  sidecar, 8-bit planes, own manifest commit protocol; GC shell caller
  landed `1f0e438`; stamp-images-with-presets landed `b800297`/`6c95e26`.
  All three ROADMAP rows marked landed. Docs-only, no battery per the
  8af2058 receipt rule.
- **Row 260 (doc-comment sync) LANDED+PUSHED `f78f7f4c`:** retouch.rs's
  merge judgment-calls and undo.rs's `merge_below_top`/`peek_redo` doc
  comments now describe the landed below-top splice (row 257) instead of
  "top entry only". clippy -D warnings green via safe-push gates.
- **Row 262 (History-panel split-redo wiring) LANDED `22e0efbf`, lock
  `razul-splitredo-panel`:** new `ShellAction::SplitRedo`
  (`edit.split_redo`, palette + command surface), dispatch arm calling
  `PaintState::split_redo_stroke()` with outcome notices
  (`history.split-done`/`split-refused`), a "Split redo" button in the
  History panel controls row (enabled on `can_redo_stroke()`), both i18n
  catalogs, snapshots regenerated. Battery
  /tmp/umber-splitpanel-battery7.log: clippy -D warnings 0, umber-app
  --all-targets 1048/0 across 10 suites. ROADMAP split-redo row marked
  fully closed. Queue row + COORDINATION line deferred (queue.md under
  the sibling's lock pattern).
- **Row 266 cross-reviewed + row 268 landed (dragon session
  local-1625-rz6f2a):**
  - **Cross-review of the sibling's svg-core (`5d4fcccc`)**: independent
    clippy + 213/0 umber-core run; the raster cap-before-allocation, the
    edge-sort invariant, the subsample accounting and the iterative
    parse all verified; ONE finding (F1 MEDIUM: no structural budgets —
    a shape flood bypassed the raster cap) FIXED by the reviewer in
    `107f90d1` (`MAX_SHAPES`, `MAX_POINTS_PER_SHAPE`,
    `SvgError::TooComplex`, pin
    `complexity_budgets_refuse_shape_and_point_floods`); receipt at
    docs/loop/kill-reports/2026-11-07-svg-core-cross-review.md.
  - **Row 268 `abr-core` LANDED `3f0e232d`**: the claw (opencode /
    deepseek-v4.1-flash-free) wrote the clean-room .abr v6/v7 stamp
    reader; the dragon corrected it (the per-brush parameter block is
    PADDING, not the brush's settings — the three fabricated parameter
    fields were removed; the total-plane budget was added; the clippy
    manual_is_multiple_of class fixed), gated it (clippy -D warnings 0,
    umber-brush 190/0) and landed it. Kill report
    docs/loop/kill-reports/2026-11-07-abr-core.md; the claw's own notes
    preserved at 2026-11-07-abr-core-claw-result.md.
  - **Row 267**: the wave-abr design brief landed `f8d6d84f`.
- **Row 271 (the `.abr` reader against REAL files) LANDED:** the upstream
  ag-psd corpus's real `.abr` files (MIT, trimmed to their `samp`
  sections, provenance in `crates/umber-brush/tests/fixtures/FIXTURES.md`)
  were parsed by the clean-room reader: the v6.2 file's 11,760-byte alpha
  plane is BYTE-IDENTICAL to ag-psd's oracle PNG alpha channel (0
  differing). The real files forced two semantic corrections: a `samp`
  record is a SAMPLE (tip bitmap with an id and a position), not a brush
  — so `AbrBrush`/`read_abr` became `AbrSample`/`read_samples` with the
  x/y position no longer dropped — and real files use v6/v7/v9/v10, so
  the version gate widened from v6/v7. Gate: clippy -D warnings 0,
  umber-brush 190/0 + the new 4/0 real-file test. Report:
  docs/loop/kill-reports/2026-11-07-abr-realfile.md. The descriptor wave
  (the brushes' settings) and the samples→presets mapping wave stay
  named and open.
- **Row 266 (SVG core: clean-room parser + rasterizer) LANDED `5d4fcccc`,
  lock `claude-svg-core`:** new `umber_core::svg` module (2,110 lines, 16
  tests, std + thiserror only — no new dependency). Hand-rolled XML
  tokenizer with byte-offset errors, element subset `svg g path rect circle
  ellipse polygon polyline line`, the full path-data command set with arcs
  converted endpoint→center→segment fan, transform composition
  (matrix/translate/scale/rotate/skew), curve flattening (Wang/sagitta,
  0.05 px), viewBox stretch mapping, scanline fill (nonzero + evenodd) with
  4x4 supersampling per pixel row, coverage = round(255·hits/16). API:
  `rasterize_svg`, `rasterize_svg_default`, `SvgRaster` (+`to_rgba`),
  `SvgError`, cap `DEFAULT_SVG_RASTER_PIXELS` (2048²). Claw-authored
  (claude/opus session 9b36591f, $3.11), dragon reviewed + gated: fmt clean,
  layers GREEN 16, scorecard current, clippy `-p umber-core --all-targets
  -D warnings` 0, umber-core **213/0** with 16/16 svg pins green (scratch
  worktree battery; the repo flock was held by a sibling's workspace
  hygiene run). Kill report docs/loop/kill-reports/2026-11-07-svg-core.md.
  NEXT wave = row 269 (loader wiring: app `.svg` stencil loader + `umber-cli
  stencil` converter), brief docs/loop/briefs/2026-11-07-svg-loader-wiring-design.md.
- **Row 269 (SVG loader wiring) LANDED+PUSHED `57f88d6f` + `399dce90` +
  `06d29936`:** the app stencil loader accepts `.svg` next to `.png` —
  `load_image_rgba` rasterizes at import (`umber_core::svg`, 2048² cap) and
  thresholds through `StencilImage::from_alpha` exactly like a PNG (alpha ≥
  128); both load sites (panel + viewport) switched, the picker filter is
  `PNG / SVG`, and a new i18n key `overlays.stencil-unsupported-svg` landed
  in both catalogs. `umber-cli stencil <in.svg> [--size N] <out.png>`
  rasterizes headlessly to a PNG (`--size` is the rasterizer's PIXEL cap,
  8192² ceiling refused by name before the file is touched, `Transfer::Linear`
  because the plane is data, one JSON summary line, exit 2 refusals). The
  dragon's review pass closed the cross-review's F2: core gained
  `MAX_SVG_INPUT_BYTES` (16 MiB) + `SvgError::InputTooLarge` + a pin, and
  BOTH consumers refuse an oversized file from its metadata BEFORE the read;
  an over-cap SVG maps to `overlays.stencil-too-large`. Battery: fmt clean,
  layers GREEN 16, scorecard current, clippy `-p umber-core -p umber-app
  -p umber-cli --all-targets -D warnings` 0, **1,407 tests / 0 failed**
  across 21 suites. Kill report
  docs/loop/kill-reports/2026-11-07-svg-loader-wiring.md. (Row numbering:
  the sibling lane's abr-core kill took row 268); this wave is row 269.)
- **Row 274 (CI red fix) LANDED:** the row 269 push's release-gate CI run
  42419285457 was RED on BOTH runners in the Tests step —
  `gpu_startup::tests::session_confirms_on_the_third_frame_and_persists_a_fallback`
  (`left: None, right: Some(Gl)`). NOT the svg wave: the regression is
  `59779065` (row 249, 2026-11-06 20:21), which made `Session::new` read
  the AMBIENT display env — a CI runner has no display, so the session
  persisted nothing and the test failed; this was the FIRST CI run since
  CI became a release gate, so it surfaced a latent red that never ran on
  a runner. FIX: the display question is the CALLER's —
  `Session::new_with_display(dir, guard, decision, has_display)`; the
  production site passes `has_display()` (production behavior unchanged:
  a headless launch still never persists its fallback); the shorthand
  constructor is gone; every test names its branch; new pin
  `a_headless_session_never_persists_its_fallback`. Gates: the CI
  condition reproduced locally (`env -u DISPLAY -u WAYLAND_DISPLAY
  -u WAYLAND_SOCKET cargo test -p umber-app --bin umber gpu_startup::tests`
  → 28/0), clippy `-D warnings` 0, `cargo test -p umber-app --all-targets`
  green (lib 256/0, bin 757/0). Kill report
  docs/loop/kill-reports/2026-11-07-gpu-startup-ci-red.md.
- **Row 272 (the paint-what-you-see stroke buffer) LANDED `93877796`:** the
  camera may now move mid-stroke. The viewport freezes the camera at the
  stroke's first event (`stroke_camera: Option<OrbitCamera>`, a plain
  `Copy`), the stroke's events resolve against that snapshot through
  `pick_with`, and the guard is split by INPUT SOURCE — the pointer
  navigation (drag chords, RMB cancel, wheel zoom) still belongs to the
  stroke, while the camera KEYS (P projection, F frame) now run during
  it. Everything else (the eyedropper, the ID-mask select, the brush
  cursor, the straight line, the orbit pivot) keeps picking against the
  live camera; the paint crate, the GPU crate and the i18n catalogs were
  not touched. Closes requirements §1's named follow-up ("the camera
  can't move mid-stroke today"). Claw-authored (claude/opus, $2.79) with
  a dragon review pass that corrected three test-side claims (the
  control's hand-derived constant, the GPU pin's device features, and a
  wrong exact-texel literal replaced by the painted-SPAN contract).
  Battery: fmt clean, layers GREEN 16, scorecard current, clippy
  `-p umber-app --all-targets -D warnings` 0, umber-app **1,056/0** across
  10 suites with all four pins green. Kill report
  docs/loop/kill-reports/2026-11-07-pwys-stroke-buffer.md.

## 2026-11-05 (local-8dc7062e, Razul) — row 156-B closed + the UI/UX pass U1-U4

**Row 156-B (the UV G-buffer single source of truth) CLOSED.** One
texel->barycentric primitive (`umber_mesh::uv_gbuffer`) feeds paint,
composite, masks, export padding (`uv_coverage`) and the bakes; on the GPU
`UV_GBUFFER_SHADER` is the only UV point-in-triangle solve left (the ID and
position bakes gather from its prim/bary textures). Pinned by CPU-vs-GPU
parity goldens (rows 172-180, `386e149`).

**The UI/UX pass** (spec `docs/specs/uiux-2026-11-05-audit-requirements-plan.md`,
kill report `docs/loop/kill-reports/2026-11-05-uiux-u1.md`; Blender 4.x,
photocraft and craft-ui as references; every change live-checked on the
release binary through `--control` screenshots):

- U1 first-launch correctness + U2 part 1 (rows 182-183): empty-viewport
  backdrop, compact layer-creation row, panel-group menu on every tab bar,
  status-bar readouts.
- U3 brush assets (rows 184-191): the brush shelf under the 3D view
  (swatches render each preset's own dabs), the Shift+Space brush popup,
  catalogs (Basic/Detail/Texture/Erase/Utility), and the preset pressure
  curves wired into the dab engine (they were saved but never read).
- U4 themes (rows 192, 196): Dark / Light / High Contrast with WCAG floors
  pinned by test, the real theme preference (Quick Setup's Light had been
  a no-op), Reduce Motion honoured by every animation, the UV canvas and
  brush swatch cards themed.
- `dcommit.py` now refuses a commit that adds a duplicate queue row number
  (five sibling collisions in one day, `3f855a1`).

**Parked:** U5 = the egui 0.37 migration plus the icon rail and collapsible
columns. egui 0.37.0 is out but egui_dock 0.21.1 still requires egui 0.36
(crates.io, checked 2026-11-05). **Desktop checks 1-4** still need Bessa's
desktop. CI: `9c7c4b4`, `a72e44a`, `3f855a1` green; `b474395` (a sibling's docs-only row) went red on one Windows WARP timing flake, `bake_now_runs_on_a_worker_and_writes_the_map` past its 60 s deadline, green on the runs either side; its deadline is now 600 s like the high-poly transfer test.

## 2026-11-05 (local-27c7e3d1...) — row 153 completed under a lifted cargo envelope

Bessa authorized local cargo build/test in-session; the 2026-11-02 no-cargo
envelope is lifted for this session only. Row 153 (app control channel) is now
COMPLETE: steps 3-5 of the 2026-11-02 design brief — `viewport.screenshot`
(egui 0.36 `ViewportCommand::Screenshot` → `Event::Screenshot` on the UI
thread, PNG via new in-memory `umber_export::png::encode_png`, base64 reply),
`ui.inspect` (drained top-of-frame), and `--bridge <port>` (lazy `umber-cli
mcp` child, verbatim relay, reaped on all paths). The first battery-capable
run since 2026-11-01 also flushed the parked kills' latent breakage: the row
151 `pub use` gap in umber-export (umber-cli/umber-app did not compile; CI
clippy red since 11-02), windows-only umber-core clippy lints, the
`angle_step` drag-sign bug, the brush-ring rim band, dead-code allows, and a
kittest fonts gate that now tolerates absent TTFs (snapshot pixel comparison
skips loudly; `font-assets` follow-up stays open — canonical OFL fonts fetched
to ~/.local/share/umber-fonts this session, NOT committed). Gates: battery
GREEN on 8 crates (core/color/graph/spp/export/cli/gpu/app), workspace
clippy --all-targets GREEN, cargo test --workspace --no-fail-fast green modulo
font-gated snapshot skips. Kill report 2026-11-05-app-control-v2.md. As-is
census: 196,691 LOC, 16 crates, 2,366 test fns, requirements 90 ✓ + split
rows (docs/specs/requirements.md). Next: row 152 (hermes mcp add after a
fresh release build), desktop-checks check 5, then row 154 CommandSpec wave.

## The core-loop queue (the ONLY active hunt)

The strategy review (docs/specs/2026-10-12-strategy-review.md, accepted in
full by the dragon 2026-10-12, d3516c6) found six critical gaps on the user
path. Queue truth and per-item state live in docs/loop/queue.md (single
source of loop truth). Summary:

- B1 viewport texture binding (critical 1) — DONE (e802f92 + 9455885)
- B2 4K live target + per-layer/per-channel routing (criticals 2+6) —
  DONE v1 (133dd12: DEFAULT_TARGET_SIZE=4096, Texture Sets size combo,
  target_size.txt sidecar; per-set/per-channel sizing = named follow-up)
- B3 stroke undo journal (critical 3) — DONE (f78e6c1; smudge test
  stacking contract pinned bed9e0a)
- B4 pixel save/load (critical 4) — DONE (d834d77 + c999da6)
- B5 pen pressure end-to-end (critical 5) — DONE (f867215 pressured API +
  shaping + can-fail test; 6430623 the source fix: raw `Event::Touch`
  force, single pens included — the multi_touch 2+ gate never carried a
  pen, cross-review finding fixed same-night; stylus-crate backends =
  named follow-up)
- B6 Gate 0 journey test — DONE IN CODE (15c072e: the six-leg journey
  test landed dragon-verified; umber-app gains a lib target for it; the
  compositor's WGSL isNan corpse fixed — no such builtin in naga 30's
  front, verified). Runtime verdict = CI at push; the phase closes on
  that green.

Phase A (truth pass): COMPLETE — A0 d3516c6, A1 e116729 (this rewrite),
A2+A3 3e0278f; queue.md records the evidence per item.

## Machine-load + wave model (standing)

One cargo command at a time, no gate stacking, never chained after
other commands in the same shell line — stacked full-parallelism gates
crashed Hermes (2026-10-12, Bessa's order). Local cargo now runs under
the flock at /tmp/umber-cargo.lock.

TWO-GATE WAVES (Bessa's ruling 2026-10-14, replacing per-push CI):
Gate 1 = scripts/wave-battery.sh per kill (fmt + clippy -D warnings +
full suites, local, minutes — this gate catches what CI catches);
Gate 2 = ONE push per wave + ONE CI run (scripts/ci-watch.sh
backgrounded with a notify pattern; the verdict pings between turns).
Kills batch as commits unpushed between gates; nothing idles on a
foreground sleep. Proven Waves 1-4: every real bug caught by Gate 1.

## Claw dispatch recipe (proven)

Vendor dependency sources into docs/claw-artifacts/<crate>/ + verified-API
facts README before dispatch. opencode: brief points at the artifacts.
claude: --allowedTools with cargo read/write allowlist. Cross-review by the
OTHER claw. Never trust claw self-reports: verify from artifacts (fmt/
clippy/test + behavioral probes) before committing. Claws never commit.

**Claw lessons (both claws hit sandbox walls, both rescued the same way):**
headless opencode cannot read the cargo registry (auto-rejects
external_directory); headless claude cannot WebFetch/cargo-doc without
pre-approved allowlists. Vendor the dep sources and pass --allowedTools.

## Blocked / waiting

- centralhost: unreachable since 2026-10-21 ~13:32 (HOST_BACK that day,
  plain boot, FRAGILE); memtest86+ owed at Bessa's next physical access
  (boot straight into it, skip the OS); host checking STOOD DOWN by his
  order. The cargo envelope (Bessa 2026-10-22 ~13:1x): cargo LIFTED,
  run slow — ONE cargo stack machine-wide, heavy legs -j1..-j3 under
  the flock, the 8G/2G scope shield, heavy jobs serialized one at a
  time. The W-C3a close ran fully LOCAL; the W-C3b close does the same.
- Windows Ink: winink stays default-off and hook-less (C2-30 closed);
  the pen-flags half (S1) LANDED with the W-C3a fleet (`8d7bf8a`).
- Flatpak bundle build: GREEN END TO END (2026-10-23, verdict run
  40040567126 on `3f1f307`) — the five-wall chain closed (vendor
  parser, runtime 26.08, install trio, rofiles hardening, svg
  loader); Umber.flatpak (12,635,392 bytes) uploads as the CI
  artifact; the job stays pinned to ubuntu-24.04 (`f62d112`) against
  the runner rotation.
- Perf-nightly: RED since 10-21 (the drift gate flipped on an unchanged
  tip) — its own hunt; the segv hunt's evidence names the opening
  hypothesis (perf-nightly.yml:30 rides ubuntu-latest — the same
  rotation); pin vs re-baseline is that hunt's call.
- Pushes are autonomous: push-gate.json is STANDING approval (Bessa's
  directive: keep it true, push when needed) — for WAVE pushes (Gate 2,
  charter + umber-wave-ops skill). The release lane is Bessa-gated:
  SPEC ratification, the dogfood asset, Windows signing, the version
  bump + tag (`docs/specs/release-checklist-0.1.0.md`).

## Handoff note (what the next session must know)

1. **Read the standing charter first** — vault
   `projects/umber/loop-charter.md` (v1, 2026-10-22; skill
   `umber-wave-ops` + the STATE block in
   `.dragon/COORDINATION-next-wave.md` beside it). docs/loop/loop-prompt.md
   carries the supersession banner (ff92379) and stays as the
   public-facing historical contract. Read docs/loop/queue.md second —
   per-item state lives there.
2. The criticals landed as a commit chain in one evening and the CI red
   streak on their wake is closed: bed9e0a (the smudge stacking-contract
   fix — the sibling test two_paint_strokes_undo_reverts_only_the_latest
   pins the contract) went green on both platforms, and the wave
   continued green through f867215 (pressure API), 6430623 (the raw
   Event::Touch source fix), and 15c072e (Gate 0 itself).
3. B2 landed v1 (133dd12): DEFAULT_TARGET_SIZE=4096,
   with_target_size for pinned test geometry, the Texture Sets size
   combo, target_size.txt sidecar persistence — per-set/per-channel
   sizing remains the named follow-up (C1 worklist #4).
4. The smudge-test lesson generalizes: when a feature changes a
   cross-cutting invariant (every stroke = one history entry), grep the
   test suite for every consumer of the OLD invariant before marking done.
5. Kill-locks before dispatch (scripts/kill-locks.sh add|check|remove|list)
   — locks live per-kill again (the L6 lock held 18:22 to
   commit-time on 10-27); claw lanes 10-27: opencode live
   (deepseek-v4.1-flash-free pinned by canary), pi live (glm-5.3),
   claude AUTH-DEAD pending Bessa's re-login. Dragon-loop wake
   and leisure sessions write into the shared main tree too: a hold set
   in a live session (e.g. "no cargo") must be written into any queued
   hunt's goal, or a wake session will not see it.
6. Test counts: 1,700+ `#[test]`s across 16 crates (2026-10-16: the wave
   battery ran 967 suite tests green across the five app/engine crates;
   599+ at the core-loop close). The C1 audit's Pass 2 verified zero
   stale-premise tests then; the 2026-10-15 truth pass found two stale
   requirements MARKS instead (brush angle / alignment / jitter were
   marked built and are not — rows 15 and 154 corrected).

## History pointers (audit trail, preserved deliberately)

- docs/specs/2026-10-09-full-project-audit.md — the full-project audit
- docs/specs/2026-10-12-strategy-review.md — the adversarial review that
  re-cut the roadmap around the six criticals (claude-opus, preserved
  verbatim + the dragon's disposition)
- DECISIONS.md — append-only, all-time decision log
- CHANGELOG.md — the wave-by-wave build history (waves 0-6 + frontier)
- Public repo: https://github.com/victorbessa96/umber (old patina URL 301s)
- Rename trail: Patina → Umber (2026-10-09, DECISIONS.md [02:41]; Bessa
  can revert — full trail there)
