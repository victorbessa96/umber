# Umber Long-Horizon Roadmap — the Substance-Parity March and Beyond

Version: 2026-10-24 (the consolidation era), delta through 2026-11-09;
landed-wave and gate rows refreshed through 2026-11-17 (`v0.1.0-alpha.1`
published as a pre-release 2026-11-16; SPEC v1.1 ratified 2026-11-16).
Supersedes the 2026-10-14 edition. requirements.md stays the CONTRACT of record
(each kill's Doc-delta updates its rows); this roadmap ORDERS the
work. The 2026-10-24 full-project audit
(`docs/loop/audit-2026-10-24-full-project.md` + its six lens
reports beside it) is the census this refresh draws from — every
row here re-verified against the live tree, not carried from
older docs.

Method: the audit's finding first — the project has crossed from
feature-march to consolidation. The core painter's path closed
10-15; the depth week since was quality on already-ticked rows.
The open risk is no longer "can a painter finish an asset" but
"what does the release gate need, and what stays gated on the
partner."

---

## Current hardening wave (2026-11-23)

Control channel + bridge live smoke is complete. Check 5 records wrong-token unauthorized+EOF, authed control methods and UI inspection, a decoded 1296×729 PNG with 3,056 colors and matching dimensions, and bridge `initialize` byte-identical to a direct `umber-cli mcp` child. App cleanup and scratch removal verified; no app source changed. Final report/queue hashes and origin verification remain. Row-156 idea-backlog verification is next. The complete dependency/gate map is `docs/loop/next-steps-2026-11-23.md`.

## Current vector wave (2026-11-21)

K8i CSS2 functional RGB editable-vector import is landed at `0e543038`, with brief
`7af220fb`. Integer and percentage `rgb()` fill/stroke support optional signs and
percentage decimals, exact component validation, gamut clipping including 400-digit
values, shared root/group/leaf/inline-style behavior, opacity composition,
hidden-leaf validation, basic shapes, canonical uppercase hex export, and typed
malformed/other-function refusals. Raster/model/app/schema are unchanged.
Committed-HEAD core is 356/356 with zero doc-test failures; strict
clippy/fmt/layers/scorecard are clean. Independent OpenCode/DeepSeek R1
overflow/docs/coverage findings were fixed and final R2 returned SHIP with zero
findings. Report: `docs/loop/kill-reports/2026-11-23-umber-vector-functional-rgb.md`.

K8h bounded inline SVG style declarations are landed at `6e2e5b40`, with brief
`3d86ad23`. Fourteen existing paint/display/visibility/vector-effect properties
support inline declarations with inline-over-presentation precedence,
last-wins, compact or spaced `!important`, typed unknown/invalid failures, and
canonical presentation-attribute export. Root/group vector-effect remains
refused; hidden leaves validate; raster/model/app are unchanged. Core is
351/351 and the full app battery is green (321 lib + 892 bin + every
integration/doc suite). R1 compact-important, R2 precedence coverage, and R3
comment findings were fixed; R4 `deleg_c14a1b14` returned APPROVED. Report:
`docs/loop/kill-reports/2026-11-22-umber-vector-inline-style.md`.

K8g non-scaling-stroke preservation is landed at `1f49a533`, with brief
`4371f5b8`. `Stroke.non_scaling_stroke` now survives native JSON with
false-default omission, participates in cache identity, imports from K8f, and
exports conditionally as SVG 2 `vector-effect`. Canonical export→import→export
is byte-stable. Every existing Stroke literal defaults false; app UI behavior
and raster are unchanged. Core is 349/349 and the full app battery is green
(321 lib + 892 bin + every integration/doc suite). R1/R2 documentation findings
were fixed and R3 `deleg_ad7fa1ee` returned APPROVED. Report:
`docs/loop/kill-reports/2026-11-21-umber-vector-non-scaling-stroke-preservation.md`.

K8f leaf non-scaling-stroke import is landed at `9071c3df`, with brief
`7718320e`. SVG 2 host-viewport units drive width, dash lengths, and offset
only when selected; ordinary effective-CTM scaling is unchanged. Local
non-inherited plumbing and validation before the no-stroke return cover hidden
and unstroked leaves. Root/group forms and other vector-effect values remain
refused. Export does not preserve the source flag. Core is 348/348 and the full
app battery is green (321 lib + 892 bin + every integration/doc suite).
Independent `deleg_c0427ff2` returned APPROVED. Report:
`docs/loop/kill-reports/2026-11-21-umber-vector-non-scaling-stroke.md`.

K8e raster display/visibility semantics are landed at `c2831099`, with brief
`661f6f14`. Raster Style now carries effective display and visibility; hidden
shapes bypass geometry and MAX_SHAPES while transforms still validate. Display
cannot be re-enabled below a pruned ancestor, child-visible override works, and
all-hidden input yields zero-filled coverage. Editable import, app, model, and
export are unchanged. Core is 346/346 and the full app battery is green (321
lib + 892 bin + every integration/doc suite). Independent `deleg_c0a26c8a`
returned APPROVED. Report: `docs/loop/kill-reports/2026-11-21-umber-raster-visibility.md`.

K8d editable vector-import display/visibility semantics are landed at
`4f1858aa`, with brief `eb9dff7c`. Scoped frames now carry effective display
and inherited visibility alongside paint and CTM. Pruned ancestors still
validate descendants without building geometry or charging budgets; hidden
leaves validate transforms and paint without parsing path data. Child-visible
override, all-hidden typed refusal, document order, visible zero-geometry
behavior, and grouped panel undo/redo are pinned. Core is 343/343 and the full
app battery is green (321 lib + 892 bin + every integration/doc suite). R1's
two blockers were reproduced RED and fixed; R2 `deleg_263daeff` returned
APPROVED. Report: `docs/loop/kill-reports/2026-11-21-umber-vector-visibility.md`.

K8c Vector Properties miter-limit row is landed at `803d6a35`, with brief
`81088f54`. The panel now exposes a localized Miter-only ValueField after the
join combo. Its production mediator changes only the stored limit, reports
no-ops, and rides one undoable `SetVector`; Round/Bevel hide the row without
losing the value. The 1..=1000 range is a deliberate UI cap inside core's finite
>=1 domain. Full app battery and strict clippy are green; R2 `deleg_d39ed2ad`
returned APPROVED after the R1 test-binding and brief-wording fixes. Report:
`docs/loop/kill-reports/2026-11-21-umber-vector-panel-miter-limit.md`.

K8b SVG stroke miter limits is landed at `22bdd3e4`, with brief `069fbdb5`.
`Stroke.miter_limit` defaults to 4, participates in cache identity, omits the
default from JSON, and validates finite values >=1 at the shared builder. Miter
geometry now uses SVG's normative interior-angle ratio, a documented inclusive
floating-point bound, normalized supporting directions, and finite-result
guards. Root/group/path inheritance and native roundtrips preserve non-default
limits while invalid values fail closed, including when masked by descendants.
Core 338/338 and the complete app battery are green; three independent review
rounds closed with SHIP / APPROVE. Report:
`docs/loop/kill-reports/2026-11-21-umber-vector-miter-limit.md`.

K8a compound fill and one-layer-per-path subpath import remains landed at
`6f35557f`, on K7f arcs/curved shapes `2555d2d8`, K7e straight-line shapes
`64dad2b1`, K7d transforms `65957542`, K7c group paint inheritance `f04ed236`,
K7b subpath import `3d087302`, K7a multi-path import `29a02f9a`, K6d dash
patterns `58f9e160`, K6c stroke joins `68cbf388`, K6b stroke caps `1839de83`,
and K6a even-odd fill `7372aa53`. Primary and secondary contours share one
deterministic fill rule, paint, stroke, cache key, and grouped undo history;
nonzero/evenodd holes, primary-empty rendering, per-contour closure/dash state,
aggregate geometry budgets, compound SVG byte stability, and secondary-safe
cache invalidation remain pinned. Next: further scoped SVG hardening or a
separately designed secondary-contour editing wave; release work remains gated.

## Delta since 2026-10-24 (as of 2026-11-09)

### The R-A2 decal lane is closed to v2d (2026-11-09)

The projector went v1 → v2a (tiling) → v2b (follow-camera) → v2c
(backface culling) → **v2d (one pass)**. v2d was a PERFORMANCE kill: the
raster was `O(texels × triangles)` because its per-texel mesh lookup
scanned every triangle, so one Apply click at the default 4096² target
took **~34 minutes on the UI thread** (the app froze) on an
8 192-triangle mesh. Now one triangle-driven streaming pass:
**34.7 s → 46 ms at 1024²/2 048 triangles (754×)**, **131.6 s → 51 ms at
8 192 triangles (2 602×)**, **~34 min → ~0.6 s at 4K**, zero extra
memory. Code `98fb6e8e`, CI run `42646713045` (all four jobs green), report
`docs/loop/kill-reports/2026-11-09-projection-one-pass.md`.

Two lessons are now doctrine (skill `umber-wave-ops` →
`references/perf-kill-cost-discipline.md`): a plan that states a false
complexity is a bug later kills inherit (the v1 plan's "one O(1) lookup
chain per texel" hid the freeze across three kills), and a byte-identity
promise is proven by fuzzing BOTH implementations against each other on
hostile inputs, not by a fixture list.

### The primitive bug the fuzz exposed — FIXED by W1.1 (2026-11-09, `7aad67c3`)

`UvTri::bary` tests only the LOWER bound of each barycentric weight
(`b_i >= -UV_BARY_EPS`) and never the upper bound. On a degenerate
**needle** (area −1.49e-8 — which still cleared `UV_DET_EPS`, i.e. was a
"valid" footprint) the edge-function formula stops being a barycentric
decomposition: a texel centre far outside the footprint got weights like
`[0, 0, 2.0]` that reconstruct to `(1.1419, 1.1419)` instead of the
texel centre. The old per-texel lookup CLAIMED and PAINTED those texels.
The GPU bake shader quoted the same formula
(`crates/umber-gpu/src/bake_shaders.rs`), so it had the same blind spot.

**The fix is measured, not guessed** (probes under
`~/.hermes/cache/scratch/probe-raster-cost/`):

| candidate | verdict |
| --- | --- |
| per-point upper bound (`b_i <= 1 + eps`) | **REJECTED** — makes it worse (lost claims 7→16, lookup-vs-walk set diff 31→54) |
| per-point reconstruction tolerance | **REJECTED** — not separable: legitimate claims of a thin triangle carry up to 3.5e-1 reconstruction error |
| **per-triangle conditioning** (reject `norm_area = \|area\| / longest_edge²` below a threshold) | **ACCEPTED** — garbage 272→**0**, lookup-vs-walk set diff 31→**0**, **0** production triangles rejected at threshold 1e-6 |

The threshold is **1e-6 on the normalized area**, landed as
`UV_COND_EPS` in `UvTri::from_uvs` (wave 1 kill 1.1, commit `7aad67c3`,
release CI run `42677040220` GREEN). At 1e-6: grids **0/10 912**
triangles rejected, random triangles **0/4 263** rejected, and the only
coverage lost is the degenerate needles themselves. The rule also makes
the two lookup paths agree EXACTLY, which is the property row 298 needed
and could not get from the per-point route. The GPU inherits the rule
through the existing all-zero-record upload channel — **no WGSL change**
(`crates/umber-bake/src/gbuffer.rs:108` routes every triangle through
`UvTri::of`). Report:
`docs/loop/kill-reports/2026-11-09-uv-triangle-conditioning.md`.

This is **wave 1 kill 1.1** of the plan in
`docs/loop/waves/2026-11-09-wave-plan.md` (the plan of record).

### The perf-nightly went green again (2026-11-09)

Run `42604373218` on `061237ac`: **SUCCESS** — the first green since the
11-08 fleet-red `42468823675`. That matches the 11-08 verdict (the reds
were fleet, not code). The fire-time drift study stands: the cron says
`0 3 * * *` but GitHub dispatches 6-7 h late (nine consecutive nights,
09:15Z-10:29Z), so the nightly measures the tip as of its FIRE time.

### Delta since 2026-10-24 (as of 2026-11-05)

- Row 156-B CLOSED: the UV G-buffer is the single texel->barycentric
  source for paint, composite, masks, export padding and every bake
  (CPU-vs-GPU parity goldens pin it).
- Row 154 command registry (dispatch by id, `command.list`/`command.run`
  on the control channel and MCP), row 155 rmcp 3.5 MCP server + cap-std
  workspace confinement, row 156-A hostile-input corpus + 200k-iteration
  fuzz (two glTF bug classes found and fixed).
- The UI/UX pass U1-U4 (spec `docs/specs/uiux-2026-11-05-audit-requirements-plan.md`):
  first-launch correctness, layer row + panel-group menus, brush asset
  shelf / popup / catalogs / pressure curves, themes + Reduce Motion.
  U5 (egui 0.37 + icon rail) waits on egui_dock.

## Where we stand (2026-10-24)

**230,737 LOC across 16 crates, 2,879 `#[test]` attributes** (12.5/kLOC,
lowest crate 7.3 — live, 2026-11-10: `find crates -name '*.rs' | xargs cat |
wc -l`; `python3 scripts/scorecard.py --print`). The core loop is Gate-0-proven end to end:
mesh → 4K pressured paint → compositor → undo/redo → save/reopen →
export, both CI platforms, verified as recently as the 10-24 green
battery runs (umber-paint 178 + umber-app 851). The perf nightly is
GREEN on the pinned 24.04 image (run 40075780768, 10-24 08:51Z —
the stale-baseline class is dead, killed by `ec7ccb8`'s pin). The
requirements contract: 89 of 94 requirement rows carry a ✓ status cell,
1 ◐, 4 none (the three open P2 rows + the `.spp` reversed row; §12's
budget rows excluded) — live count 2026-11-10, by status-cell inspection
of `docs/specs/requirements.md`, not a carried number.
The scorecard generator's `--check` passes.

The night of 10-23/24 landed the W-C4 runway's first four kills
(all battery-green + CI-green on the full matrix): the set-switch
glue (C2-9's headless half), the per-channel dab density (both
arms), the ID-mask pre-warm (the first-click hitch), and the
paint-tiles auto-map by material (the composite's per-set rows
line up at load). The stencil lane is fully closed (W-C3a/C3b +
FREEZE-4). The flatpak chain landed with the honest appstream-
compose revert. The two-gate wave model runs the march; the
battery's machine-fit defaults + the feature-on RUN leg hardened
it this same week.

## ✅ LANDED — the completed archive (2026-10-10 → 2026-11-10)

Every closed row of the march, consolidated. Proof cites the
commit or the live tree; details live in each era's kill reports
under `docs/loop/kill-reports/`.

### The engine core (the march's waves 1-3 + the pool arc)

| Kill | Proof |
|---|---|
| Per-layer paintable masks | 770162f + 21e1290 |
| The 32-mode blend census | d861482 + the HSV/normal-map family goldens |
| History panel + click-to-time-travel | the History panel's journey rows |
| Postfx persistence | f35cf48 |
| OCIO panel + ACES configs | 9a1a808 |
| Per-channel resolution end to end | f4e7524 |
| Icon-first toolbar + boot atlas | 40c754f |
| Perf baseline + the nightly's fixed invocation | fd49627 |
| .spp import (chunk-index fixed 10-16) | 30b966e + 0a5c20d + the b-tree fix; corpus-armed |
| PSD import (flatten-on-import) | a4bf81a |
| Dynamic materials end to end | 83aa242 → 7ecea60 → Kill A f85cb0a → Kill B 2c6497e |
| Ortho camera | 40f3e5d |
| 8K export | cd668a3 |
| The per-layer pool arc (stages 1a-5) | b1ceb2c, f2aff9f, ed64a58, dd6e1cd, ec6d552 |
| Distribution (Flatpak + AppImage proven) | the shell+PS+CI stack |

### The C2 close — the painter's path (2026-10-15, W-C2a..f)

Every channel paints and exports (C2-5/6); a quit guard + silent
Ctrl+S (C2-1/4); pixel autosave + recovery (C2-3); one undo
timeline with row travel (C2-7); a stack + undo + History +
channel list per texture set (C2-9). All same-day, all CI green.

### The depth arcs (2026-10-14 → 10-16)

| Kill | Proof |
|---|---|
| Bake margin: Adjacent Faces across seams | engine 3818da0; CLI bca634f + agent c237e4f; Bakes panel 741ec47 |
| Brush falloff curves + custom | d0309e1 + panel 9a47fa0 |
| Bitmap stamps + angle modes + jitter | 47d8fed + 487fd49 + 90ec048 (C2-11) |
| Front-face falloff | cabe283 + 9a47fa0 |
| Stroke methods end to end | ee2dce5 + 51d9047 |
| Stencil: paint-through screen mask | the full lane below |
| Brush cavity mask | 405e0e2 + 4bebc72 |
| GPU safe-start + backend fallback | a221a85 |
| Save completeness by exhaustive destructuring | 17900df |
| Release phases (alpha/beta/RC) + the agent `version` step | 2c19dcd |
| openusd 0.8 (the last importer abort) | 5b5b0ba |
| The mesh-importer panic hunt | 24 findings, 23→24 closed |
| Crash reporter core + last-10-ops ring | ddb59e0 (C2-2) |
| `umber-cli bench` public benchmark | landed 10-16 (the GPU/lavapipe timing lines owed to protocol.md) |
| The WASM plugin SDK authoring docs | b0c1aa0 |
| The procedural history panel k1 (wave-procedural-history's first half) | 2a84fb8 (10-23): graph_panel/history.rs, 651 lines + the .ftl keys + the kill report |
| The kittest harness + the panel snapshots | the G0 foundation |

### The stencil lane (W-C3a/C3b + FREEZE-4 — fully closed)

Engine + app 0955ac8; paint hook 3a8b4cb; panel wiring 6c95e26;
viewport shell + N 2bb8c60; set-switch re-stage dcc5e89; cross-tile
observer 45ad4e4; the tangent-wiring third site + its postfx wall
(CI-green); the C2-9/M-C3a panel seats; the 2026-10-12
heightnormal amendment (480ad62). The stencil session module moved
bin→lib with zero path churn (the W-C4 L1).

### The W-C4 runway's first kills (the night of 10-23/24)

| Kill | Proof |
|---|---|
| L1: the set-switch glue (C2-9's headless half) | b79f0c6 — umber_app::set_switch, the five-pin headless test |
| L2: per-channel dab density (both arms) | 306a502 — texels_per_uv_for_channel; the write-only field deleted |
| L3: ID-mask bake-on-tool-entry | 6430374 — prewarm_id_bake_cache; the first click hits |
| L5c: paint tiles auto-map by material | 0ec64bd — auto_assign_tiles_by_material at set_mesh's tail |
| R-B1: the selected-layers flatten (the census P0) | 2026-10-24: bake_layers + the panel button + the shell handler; battery GREEN 179+851; report in kill-reports |
| R-C1 v1: the Assets panel's library browser (the census's bigger P0) | 2026-10-24: the search + three categories over the existing engines + the compose-flow extraction; battery GREEN 851; report in kill-reports |

### The per-set masked bakes (the spec's stages — all landed)

§1+§2 triangle_materials + all three importers (a23bcac + the
gltf/fbx/obj attach); §3 raster half (the per-set submesh bakes,
bakes_panel.rs:638/1781); §3 ray half — the with_occluder split +
`OccluderScope` (0955ac8); §4 the per-set fan-out; §5 the
auto-mapping (0ec64bd). Remaining: none; the OccluderScope panel row
landed 2026-10-24 (see the landed rows below).

### The tooling + machinery (the loop's own hardening)

wave-battery.sh (machine-fit defaults + the feature-on RUN leg);
ci-watch.sh; scorecard.py (+`--check`); the perf-nightly image pin
(ec7ccb8); pre-push.sh; verify-kill.sh + land-kill.sh; the kill
locks + the main-tree pattern; the brief-template; the
freeze-recovery close scripts; the loop restart machinery
(2026-10-24 04:05).

## Landed waves + gates (dependency-ordered; each row names its gate and its status)

### Landed engineering archive — current runway is reconciled in `docs/loop/next-steps-2026-11-23.md`

| Kill | Closes | Notes |
|---|---|---|
| L6: the unified timeline | **LANDED 2026-10-27** (`75c9594`, report in kill-reports): the graph journal merged as the History window's third arm — layers + strokes + graph interleaved by op_id, unified undo/redo/travel across all three, the GraphJournal trait bridging the lib/bin split, the bin-side commit queue drained per frame, a load = one row at depth 0 (a position, never a target), availability rewired to the unified targets. Battery GREEN umber-app 873. | the graph-history kill 2 closed on the landed k1; claw-authored (opencode/deepseek) with dragon review |
| The OccluderScope panel row + i18n keys | **LANDED 2026-10-24** (the panel half, report in kill-reports): the Bakes panel's scope combo (Whole mesh / Baked set only — the enum's two simple variants; `Sets` deferred with its set-list picker named) + the four live call sites resolving the occluder inline (OwnSet's occluder IS the raster source, the set's own submesh) + the RayLength dirty class + the three keys in both catalogs. Battery GREEN 851. | L5(b) closed: the enum (the ray half) had landed long ago; this row is the panel half. FOLLOW-UP CORRECTED 2026-10-25: the OwnSet two-set bake test ALREADY EXISTS — the :2840-class pin bakes the ground submesh against the whole vs against ITSELF (the OwnSet occluder), asserting equal coverage and the shadow's removal in `min_lit`; the panel's combo→request→resolution threading is the kittest-class residue, named honestly |
| The AssetStore GC shell caller | ~~the 10-16 engine's missing consumer~~ **LANDED `1f0e438` (row 238)** — `umber gc <project> [--delete]` headless lane over `asset_gc::run_cli` (live-set builder = manifest-referenced safety); row marked landed 2026-11-07 (row 258) | closed |
| L2b: the four same-class density arms | **LANDED 2026-10-25** (report in kill-reports): the symmetry snapshot, the clone offset, the fill raster, and the seam mirror radius all migrated to `texels_per_uv_for_channel(tile, &active_channel)` on the L2 kill's own pattern — every arm serves the active channel's stroke/fill. The set-level scalar stays (its definition + the two pinning tests). Battery GREEN 179+851. | closed; the differing-channel pin rides the L2 report's class |
| L3b: the multi-entry ID cache | **LANDED 2026-10-25** (report in kill-reports): the three-entry FIFO ring (`ID_BAKE_CACHE_CAP`) kills the A↔B tile-switch thrash the one-entry judgment accepted — the hit path's position/swap_remove/restore-push-to-tail, the shared `push_id_bake` FIFO insert, the prewarm's whole-mesh gate unchanged; the pin proves two bakes for three alternating clicks. Battery GREEN 180+851. | closed; the cap's tuning rides live use |
| Quick mask persistence | **LANDED 2026-10-31 (`73f8724`, in origin/master)** — DECIDED PERSIST, 8-bit sidecar as amended: `paint/quick_mask/<set-hash>.{json,bin}` + own `manifest.txt` commit protocol under the paint sidecar (no core schema change); round(v*255) makes save → load → save byte-identical to the GPU gate. Undoable mask strokes and ID clicks LANDED `c5909efe` (one journal step per gesture, filter-op stack precedent) | lens A's gap 1 and its undo follow-up are both closed |
| Symmetry persistence | **LANDED 2026-10-24**: the `.umber` carries it (core's `ProjectSymmetry` mirror + the app's boundary conversions + the additive-carry rule; the `.umberbrush` half = the documented non-goal — a preset is a brush shape, not a tool state); report in kill-reports | lens A's gap 2 closed; the quick mask's design question stays the open half of the pair |
| wave-a11y-first-pass | keyboard navigation + focus states | **LANDED 2026-11-12 (row 378, lock razul-a11y-first-pass)**: kittest keyboard journeys (Tab traversal, focus ring, Esc, number-key menus) — brief `docs/loop/briefs/2026-11-11-a11y-first-pass.md` |
| **The Assets panel (R-C1, P0)** | **LANDED 2026-10-24 (the v1 discoverability layer)**: the library browser (search + Brushes/Materials/Masks) over the existing engines; the `select_preset_by_name` seam; the smart-mask compose flow extracted (one path, two doors); the placeholder dead; report `docs/loop/kill-reports/2026-10-24-rc1-assets-panel.md`. Landed follow-ups: drag-to-stack `769c2544`, thumbnails `e9c02bd0`, custom dirs `68b45a69`, import dialog `77f58d41`, browser kittest `09867156`, saved searches `38a4fd21`, env `0d6f08f8`, filters `05105aa4`. Only the mesh category remains. |
| **Layer-stack flatten (R-B1, P0)** | ~~group → baked fill layer~~ → LANDED 2026-10-24 as the SELECTED-LAYERS flatten: `bake_layers` (the isolated fold) + the Layers panel button + the shell's handler; fully undoable (tombstones + journal reverts); report `docs/loop/kill-reports/2026-10-24-rb1-flatten.md`. Folder kind landed `cc381ce6`; grouped flatten undo landed `e008dcc8`. |
| Material picker (R-A10) | **LANDED 2026-10-24 (the v1)**: Ctrl+Alt+click picks the ACTIVE channel's composite texel at the cursor into the selected fill layer's Solid (the inspector's mirrored-row convention, the panel's own journalable SetFill path; the status-bar hint in both catalogs); report in kill-reports. The two landed halves: the channel target (0f55ebd) + the pick gesture. Follow-ups: the all-channels mode, the e2e kittest, ~~the pick toast~~ **CLOSED 2026-10-25: the toast rides the home_actions rail's own pattern (the viewer's pick_notices vec, the dock closure's tuple return, the app's post loop) — `uv.pick-applied` in both catalogs with the hex color**. | the census's "small" row grew a prerequisite + this v1 across two kills; both banked |
| Projection fill (R-A2) | **CLOSED to v3 (2026-11-10)**: v1 decal projector landed 11-08 (rows 277-281, `c573dc87`; load PNG/`.svg`, Arm freezes camera+viewport rect, Apply rasterizes through the frozen camera, one undo step, stencil-gated); v2a tiling, v2b follow-camera, v2c backface culling, then **v2d one-pass raster** (`98fb6e8e`, CI `42646713045` — ~34 min UI freeze at 4K → ~0.6 s), **W1.1 the UV conditioning rule** (`7aad67c3`, CI `42677040220` — needle triangles are not valid footprints, CPU+GPU) and **v3 depth/occlusion culling** (`b228767f`, 2026-11-10 — depth raster + `ProjectionState::occlusion`, default on). Remaining: seam/symmetry expansion of projections (a product question), a decal channel + the bake family | plan of record: `docs/loop/waves/2026-11-09-wave-plan.md`; plan `docs/specs/projection-fill-plan.md` |
| Geometry masks (R-B2) | **LANDED 2026-11-14** (K1 `5ff1510c`, K2 `0e4e5035`, K5 `744e114f`, K3 `c98a7ea6`, K4 `4170fac5`, K6 `fa31f7df`; design `docs/loop/briefs/2026-11-12-geomask-design.md`; kill reports `docs/loop/kill-reports/2026-11-14-w5-geomask-*.md`) | per-mesh/per-tile include/exclude on layers, hide-excluded, copy/paste — rides UDIM |
|| Baker sync (R-D1) + channel manage (R-B4's tail) + the four post nodes (R-E5) | the power-user tail | **LANDED 2026-11-11/12 (rows 370, 375-377)** |
| wave-fuzz-corpus (b) | coverage-guided fuzzing | **LANDED 2026-11-14 `fc8b9ce8`**: cargo-fuzz `importers` target, first 900 s run clean (2.54M execs, 0 panics) |
| wave-abr | .abr brush preset import — **FULL WAVE CLOSED 2026-11-10 (rows 268/271 + 282-285 + wave 3)**: the clean-room v6/v7/v9/v10 reader is complete end-to-end: `samp` reader (`ca70ea80`, oracle-exact vs real files row 271), the `desc` descriptor parser (`a68f97f4`, `abr_desc.rs` + `read_abr`/`AbrBrush`/`match_sample`, pinned against UNTRIMMED upstream files `577bc07c`), the `.abr` → `.umberbrush` + `.stamp.png` mapping (`c9ee71e5`, `abr_import.rs`), and the CLI fan-out `umber-cli import-asset --kind brush <file.abr>` (`4cc2bdfe`). The wave-3 tail landed too: `patt` pattern reader (`72b409cc`) + `PatternTexture` preset block / CLI sidecar (`14d9dbf1`), per-brush skip-and-count (wave 3.2, row 338), dynamics→engine wiring (wave 3.3, `3079e4cb`), cross-library stamp dedup (`0bcc1687`, wave 3.4) — wave 3 CLOSED (row 340). v8/11 stay refused as **unattested** (row 333: no public sample exists; "descriptor-only" is a content class, not a header number). Live-verified on real Photoshop files: "Soft Round" (computed) and "Bobbys Brush 743" (sampled, 112x105 stamp). | the one legal Adobe format |
| wave-svg-import | SVG vector assets — **the IMPORT wave LANDED 2026-11-07 (rows 266 + 269)**: the clean-room `umber_core::svg` parser + 4x4-supersampled scanline rasterizer (`5d4fcccc`, cross-reviewed with hostile-input budgets in `107f90d1`), the app stencil loader accepting `.svg` next to `.png` (`399dce90`) with the 16 MiB input budget (`57f88d6f`), and the headless `umber-cli stencil` converter (`06d29936`). An `.svg` rasterizes AT IMPORT into the stencil coverage plane — no vector state persists. The PATH-TOOL wave is OPEN in stages: K1 (`46d4bff5`, additive vector model) and K2 (`0b5cb72d`, nonzero fills + round-cap/join strokes + source-over alpha into cached eval-owned baseColor planes) are landed; K3 canvas editing (`98fdbb80`, reversible SetVector journaling, fresh-layer adoption, UV anchor/handle gestures, exact deletion, collapsed-handle routing, and hidden-tab release) is landed; K4 vector property widgets (`a4373c51`, dedicated Vector rail, visible new-path defaults, native sRGBA fill/stroke controls through SetVector) are landed; K5a deterministic SVG export (`09d01fbd`, exact UV path/paint serialization and a user-selected save flow) is landed; K5b bounded SVG-to-vector import (`0d1b6ad7`, geometry-sink split, exact cubic anchors, root-transform-once, typed refusals, one grouped undoable Import SVG flow) is landed; K5b review hardening (`6ffec3bc`, full presentation-effect refusals, capped streaming reads, roundtrip/alpha/precision/test-honesty closeout) is landed; Fill cache-lifecycle parity (`8747c681`, full-dimension cache identity, open/reset and open-seeding lifecycle, live pruning, dormant-safe observation, prospective-budget retry) is landed; K6a even-odd vector fill (`7372aa53`, additive fill_rule with NonZero default, shared scanline evenodd, panel combo, SVG roundtrip) is landed; K6b stroke cap options (`1839de83`, additive Butt/Round/Square with Round default, closure-safe joins, panel combo, SVG roundtrip) is landed. K2/K3 reuse the clean-room scanline core, add no dependency, and preserve project/sidecar formats | the one legal vector format artists hand over |
| wave-replay-exact | path replays paint exactly | **DECIDED 2026-11-16: default-off stands; the opt-in toggle LANDED `aa1f5903` (report `bf9f5f25`)** — no open work remains |
| Split-redo (engine landed 850b839; RedoPending lift landed 1347e13; **History-panel wiring LANDED 2026-11-07 `22e0efb`, row 262**: `edit.split_redo` command + the panel's "Split redo" button, outcome notices, both i18n catalogs, snapshots regenerated**) + merge-below-top (**WIRED 2026-11-07 `bbdc08b`, row 257**) | the undo journal's two named follow-ups — both closed | retouch.rs's debts |
| wave-procedural-history | graph topology edits as undoable commands | **LANDED `2a84fb89`** |
| wave-smart-material-params | exposed parameter UI | **LANDED 4.1a `7a9745c4` + 4.1b `db08895a` 2026-11-14** |
| wave-community-templates | 10 curated .umber templates | **LANDED 2026-11-10 (row 349)** |

### Gated — named walls, no derivation until the gate opens

| Lane | The gate |
|---|---|
| L4: winink live hook | the file IS in the tree (`crates/stylus/src/winink.rs`, 307 lines of passive decode — `GetPointerInfo`/`GetPointerPenInfo`, `#[cfg(all(windows, feature = "winink"))]`); the gate is the **WndProc hook + HWND hand-off**, not the bytes (module doc: "Live pointer-pump integration is Wave 2") |
| wave-stylus-win32 | the Windows dual-path; real-pen verification = D1's Windows leg (partner-gated) |
| wave-wasm-shaders / tessellation / hdr / ray-query | the eframe/wgpu upgrade postures (the probes built) |
| The .spp staged import R1-R4 | the fidelity report's staged lanes (the formats lens reports which are landed) |

### Partner-gated — his calls, queued and named

The published alpha and SPEC v1.1 ratification are complete (`v0.1.0-alpha.1` at `bc4d642f`, 2026-11-16). Remaining human release work is the dogfood walkthrough (C2-24), final signing decision (C2-22), release-checklist walkthrough, upstream bug drafts (docs/upstream/), and the i18n native-speaker pass. The .ftl lock is **CLOSED 2026-11-16**: it was lifted per the 2026-10-24 audit; only the native-speaker pass remains a human-quality gate.

## Standing debts (refreshed 2026-11-17; rows below are closed landed items unless explicitly named OPEN)

- **Added 2026-11-06 (charter-v2 T1-T3, `5af3c84`):** the Export panel
  button path lacks `flush_for_export` (export.run has it); `job.status`
  keeps stale paths after a failed job; export > 30 s times out over
  `--control` (`UI_REPLY_TIMEOUT`). CLOSED the same day: audit A2-4's
  bake/export-by-id tail, the e2e smoke in CI, the push guard, the button
  flush + failed-job fields (row 237), and the last sub-item, the
  export > 30 s timeout: `command.run` of `bake.run`/`export.run` now
  waits `JOB_REPLY_TIMEOUT` (30 min) — queue row 246.

- ~~The cached ID-mask bake~~ **LANDED SILENTLY** — the P1 tile
  cache + the L3 pre-warm; this row was stale for a week.
- ~~Split-redo under merged undo entries~~ **CLOSED 2026-11-07**: the engine (`850b839`) and the History-panel `edit.split_redo` command + button (`22e0efbf`) are landed with byte-exact replay pins.
- Merge below the top undo entry — **WIRED 2026-11-07** (`bbdc08b`, row
  257): the splice primitive landed `592df58` (row 256) —
  `UndoStack::merge_below_top(k, doc, build)` + `push_unapplied` rewrite
  the entry `k` below the top without re-applying the entries above them
  (taken unreverted, re-seated unapplied) and preserve any pending redo
  via `take_redo`/`restore_redo`. `record_stroke` (retouch.rs) now scans a
  bounded window (8) below the top through the new `peek_undo_at`, but
  only when the redo stack is empty and the stroke intersects NO entry
  above the target (redoing those would clobber the merged after over the
  overlap); both directions pinned byte-exact. The companion History-panel
  split-redo affordance landed in `22e0efbf`.
- The winink live-HWND question — OPEN: the decode IS in the tree
  (`crates/stylus/src/winink.rs`, passive decode); the WndProc hook +
  HWND hand-off is the gate.
- DPI runtime re-raster — **LANDED 2026-11-16** (kill dpi-runtime-reraster):
  `preferences::hidpi_class(ppp)` (>= 1.75, egui's convention, now also the
  startup pick) plus a per-frame watcher in `fn ui` that reloads the 1x/2x
  icon atlas when the hidpi class flips mid-session and requests a repaint
  so the viewport re-rasterizes; under manual `ui_scale` the zoom stays
  pinned by `style_delta`. Battery: clippy -D warnings clean, 1335 passed
  / 0 failed (incl. `hidpi_class_boundary_is_1_75`).
- ~~The AssetStore GC's shell caller~~ **LANDED `1f0e438` (row 238)** —
  `umber gc <project> [--delete] [--min-age-secs N]` headless lane in
  main.rs over `asset_gc::run_cli`; live-verified 2026-11-07: dry-run on a
  fresh project prints the zero report JSON exit 0, a missing dir prints
  usage exit 2. This row was stale.
- ~~Stamp images persisted with presets~~ **LANDED 2026-11-06 (`b800297`
  presets + stamp PNGs, `6c95e26` panel wiring)** — stamped presets carry
  their `<stem>.stamp.png` sidecar, round-trip byte-identical
  (`starter_wave_presets_round_trip_byte_identical`,
  `starter_wave_stamp_sidecars_exist_and_parse_as_rgba8_png`). This row
  was stale.

## Never (hard walls — restated)

- .spp WRITING (import is ratified + landed; the .umber directory
  format is the kill-shot).
- .sbsar execution (Adobe Additional Terms; the node graph +
  MaterialX interchange is the legal answer, fully built).
- Fake interchange: MaterialX exports declared custom nodedefs;
  no full-fidelity claim is ever made.

## The parity scoreboard (2026-10-24 — from the SPP lens, live-verified)

| SPP family | Verdict |
|---|---|
| Painting core (stamps/flow/spacing/falloff/jitter/angle/methods/stabilizers) | PARITY |
| Stencil/projection | PARITY — stencil lane closed; **projection fill CLOSED to v2d + W1.1 (rows 277-281 + `98fb6e8e` one-pass + `7aad67c3` conditioning, CI `42677040220` green)** |
| Retouch + masks (quick/cavity/ID) | PARITY (the persistence gaps closed: quick mask `73f8724`, 2026-10-31; symmetry `c88b4f6a`, 2026-10-24) |
| Path tools | PARITY (paint-along + filled + polygon + straight-line); ribbon = decided NON-GOAL (`gui-audit-and-roadmap.md:265`: "The ribbon? No. Never.") |
| Fill/material layers + anchors + instances | PARITY (materials composite-only by design) |
| Texture sets / channels / UDIM | PARITY (auto-assign landed; channel add/remove/rename LANDED 2026-11-11, rows 370/374) |
|| Baking | PARITY (per-set masked bakes closed; D1 baker sync landed 2026-11-11; D2 auto-cage `533560e8`, D3 opacity `c499255f`, D4 multi-source ID `35097df5`, and D5 ground plane `752bc991`, subsequently hardened) |
| Viewport | PARITY (four post nodes landed 2026-11-11; height-map displacement preview landed `9e0517bf`) |
| Color management | PARITY |
| Export/Import | PARITY (the .spp v2 staged lanes; text unsupported) |
| History/undo | PARITY (the unified timeline LANDED — L6, `75c9594`, 2026-10-27) |
| UI/UX density | PARTIAL → closing (36/43 GUI kills by 10-24; the 2026-11-05 UI/UX pass landed U1-U4: brush shelf + Shift+Space popup + catalogs, Dark/Light/High Contrast themes with WCAG floors, Reduce Motion; U7 landed the icon rail + collapsible columns through `1fc6ca12`/`89249567`; only the U5 egui 0.37/egui_dock migration remains) |
| Assets/ecosystem | **GAP → narrowing** (the Assets panel + brush shelf landed; starter content waves K5-K9 landed 2026-11-05: import-asset CLI, ten materials, studio environments; R-C1 drag/thumbnails/custom dirs/import dialog/kittest/saved searches/env/filters are landed, with only the mesh category remaining) |
| Scripting/automation | **EXCEEDS** (headless-first, versioned JSONL) + MCP stdio server + token-auth loopback control landed 2026-11-01 `3331bcb` (Hermes registration = queue row 152) + app-side control v2 (`ui.inspect`, `viewport.screenshot`) and `--bridge` MCP relay 2026-11-05 (queue row 153) + the client-library gap |
| Interchange (MaterialX, diffable format) | **EXCEEDS** |
| Community/local-first/no-telemetry/never-crash | **EXCEEDS** |

The census-delta arithmetic: Tier-1's 10 rows — 2 closed this week
(stencil, filled path); 8 remain, led by the two P0s above. The
full comparison: `docs/loop/audit-2026-10-24-lens-spp.md`.

## The wave protocol (standing, the two-gate model)

Bessa's ruling: big development waves, tests before the push. Gate 1 =
scripts/wave-battery.sh per kill (fmt + clippy -D warnings + full
suites, local, the flock at /tmp/umber-cargo.lock, the machine-fit
defaults). Gate 2 = ONE safe-push per wave, which gates the affected
crates itself, followed by `git ls-remote` verification. CI is a release
gate at version bumps, tags, pull requests, and its weekly schedule, not
a per-push run; `scripts/release-ci.sh` supplies that release verdict.
Kills batch as unpushed commits between gates. Briefs are written fresh
from live anchors at dispatch time; kill reports + the touched docs' rows
update in the same commit as the kill.
