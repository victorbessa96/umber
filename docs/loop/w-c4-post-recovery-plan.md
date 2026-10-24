# The W-C4 post-recovery plan — the §5 tail, mapped

Draft 2026-10-20 by local-10e6ed18 (local-10e6ed18, dragon). Status:
research/planning lane, host-free under the 2026-10-20 code steer —
every lane below is RESEARCH-VERIFIED against the live tree and the
vendored registry sources; no cargo ran, no bytes of app code moved.
Companion artifacts (the §5 item 2 vendoring precondition, DONE):
`docs/claw-artifacts/stylus/eframe-0.36-rwh-surface.md` and
`docs/claw-artifacts/stylus/windows-0.62-subclass-surface.md`.

## Phase 0 — the recovery (HOST_BACK, before any W-C4 lane)

`bash scripts/w-c3a-close.sh` detached from a session with
HERMES_SESSION_ID/DRAGON_SESSION exported. It: claims, verifies the
NINE manifests, generates A2's four baselines host-side then EXITS 5
for the recovery session's vision check (re-run skips), runs the
default-leg battery (never gpu), commits ten rows via dcommit
--path-limited, pushes ONCE, ci-watches by explicit run-id, then
re-dispatches packaging. Full detail: the script header + its report
`docs/loop/kill-reports/2026-10-20-w-c3a-close-script.md`.

## The lanes (dispatch order recommendation: L1 → L2 → L3 → L5 → L6,
## with L4 free-running once its preconditions land)

### L1. Set-switch glue extraction — main.rs:2501+ (b10-gated)

`texture_sets_panel_ui` (main.rs:2501) mixes UI with the set-switch
dance: `ensure_set` → the set list (`set_names`/`size_of_set`/
`tiles_of_set`) → `set_active_set` → **the Amendment A restage**
(the active-tile quick-mask plane can differ per set; a guarded
`crate::viewport::restage_stencil(s, Some(&mut *paint))` keeps the
stencil gate staged at the new tile's size) → the size combo +
pending-resize staging. Extract the NON-UI core (switch + restage +
resize staging) into a lib fn (umber-app module or paint_state
helper) so C2-9's check gains a headless test — today D1's smoke is
the only instrument. Unlocks L5's per-set scope glue.

**CLOSED 2026-10-24 (the L1 kill, `crates/umber-app/src/set_switch.rs`):
`switch_active_set` + `reconcile_pending_resize` extracted to a lib
module with `restage_stencil` (moved from the bin's viewport, name and
signature unchanged) and `PendingResize` (fields pub for the headless
half); the panel calls both, behavior bit-identical. The headless test
(`crates/umber-app/tests/set_switch.rs`) pins all five contracts
through the real adapter: the gate-follows-the-tile re-stage on a
size-changing switch, the same-set no-op, the inactive and imageless
no-restage guards, and the stale-pending clear. The stencil session
module moved bin→lib (crate-root re-exported, every existing
`crate::stencil::` path resolving unchanged).**

### L2. Per-channel dab density — ONE kill, both seams (b10+P1-gated)

`texels_per_uv_for` (crates/umber-paint/src/paint_state.rs:1653 —
`tile_size(tile).0 as f32`, the per-tile density) feeds BOTH the
dab spacing and the brush ring's probe twin
(viewport.rs:1423-1427: `paint.texels_per_uv_for(tile_of_uv(uv))` →
`brush_cursor::texels_spanned`). The §5 item 1 risk: the ring
disagreeing with the dabs when channel density diverges. One kill
touches both arms so they can never disagree; the quick mask stays
set-scale by design (paint_state.rs:163-164, the plan's own note).

**CLOSED 2026-10-24 (the L2 kill, brief
`docs/loop/briefs/2026-10-24-l2-channel-density.md`, report
`docs/loop/kill-reports/2026-10-24-channel-density.md`):
`texels_per_uv_for_channel(tile, channel)` via `size_of_channel` —
both arms migrated together (the dab conversion's `local_texel` +
the ring's probe), the write-only `texels_per_uv` field found dead by
derivation and deleted. L2b named as the follow-up: the four
same-class arms (symmetry snapshot, clone offset, fill raster, seam
mirror radius) stay set-scale this kill, mapped in the report for the
next strike.**

### L3. ID-mask bake-on-tool-entry — main.rs:532 (b10+P1-gated)

The IdMask arm already computes `idmask_armed` and calls
`self.viewport.set_idmask_armed(idmask_armed)` at tool entry
(main.rs:532-534). The lane: pre-warm P1's id-mask tile cache at
that moment (the bake the first click currently pays), so the click
selects without the entry-latency hitch. P1's parked cache is the
target; its report names the cache seams.

### L4. Winink live hook — stylus + one eframe seam (RESEARCH DONE)

The vendoring precondition is landed (the two companion artifacts).
Design, now API-verified end to end:
- The stylus side is S1-gated (winink.rs parked until the recovery's
  row 4 lands it) — the hook extends the parked `PenState::decode`
  surface (winink.rs:78), reusing the existing WM_POINTER* decode
  and the `PenFlagTracker` edge detection (pen_flags.rs:116-131).
- Install: `SetWindowSubclass` (windows 0.62.2, Win32/UI/Shell/
  mod.rs:4318) at `App::new` via `cc.window_handle()` (the eframe
  artifact doc) — NotSupported-safe for kittest, AtomicBool-guarded
  against double install, per-frame fallback if creation-time
  refuses.
- The proc: observe WM_POINTER*, route to `PenState::decode`,
  **ALWAYS** `DefSubclassProc` (Shell/mod.rs:226) so winit's own
  Touch path survives unchanged — the passive-decode comment in the
  parked winink.rs header ("no window ownership") stays true: the
  subclass observes, it never owns.
- Manifest: add `"Win32_UI_Shell"` to the windows dep features
  (crates/stylus/Cargo.toml:19 — currently Pointer +
  WindowsAndMessaging + Foundation, target-gated cfg(windows)).
- Teardown: `RemoveWindowSubclass` at app exit (same vendored
  module) so a window close can't leak the subclass.
- Real-pen verification = D1's Windows leg (partner-gated: needs the
  Windows box + the pen; the Linux xi2 lane is unaffected).

### L5. Per-set MASKED bakes (after L1; A1+mesh-gated)

The existing note (docs/loop/briefs, the W-C3a plan's §5 tail
reference) maps the goal: per-set bakes constrained to a material
mask via `MeshData.triangle_materials` (umber-mesh). Dependency
chain: L1's extracted set-switch glue is the seam the mask bake
plugs into (bake_sources.rs is A1's parked file — the lane waits for
the recovery). 5 host-gated kills were pre-estimated in the note;
re-estimate after L1 lands.

### L6. Unified timeline (graph-history kill 2; GH-gated)

graph_panel/history.rs (parked, row 6) carries the per-node
procedural history; the unified timeline merges it with the paint
journal into one view. Independent of L1-L5; any order after the
recovery.

### L7. G1's color_button delegate conversion (G1-gated)

The stock `color_button` keeps its kittest consumers by design (the
A3 judgment call); the conversion moves the pinned kittests onto the
delegate. G1's parked bytes land at the recovery's row 3; the lane
re-estimates the pin migration then.

## Standing rules that govern W-C4

- M2: every CI claim cites the run's JOB LIST.
- M1 pass 3 at W-C4's close (the stale-row sweep).
- One push per wave; per-kill local commits + battery before it.
- The release lane (SPEC ratification, signing, the tag) stays
  partner-gated — never autonomous.
- Cargo discipline: the battery default-leg only, never gpu inside a
  recovery window.

## What this plan does NOT owe

No new queue rows until the lanes dispatch (the queue is the
recovery's docs row territory). No re-estimates of parked bytes.
No CI dispatches (scratch-ref stays partner-gated).
