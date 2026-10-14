# The paint-what-you-see stroke buffer — architectural research record

Date: 2026-10-14. Kill: WAVE-5 PWYS RESEARCH (design-doc-first row).

**Scope box (binding).** This doc is a RESEARCH record, not an
implementation contract: it answers the roadmap's "deepest
architectural row" question set (stroke-space buffer decoupled from
camera; `docs/loop/wave-plan.md:27`, `docs/ROADMAP.md:176`) and
recommends the staged kills that follow. It implements nothing, changes
no behavior, and owns no code surface. Its predecessor is the 2026-10-12
wave-2 design pass (`docs/specs/pwys-stroke-buffer-design.md`), which
already chose frozen-matrix raycasts (its option A) over resolve-at-end
(its option B); this doc re-derives that choice from the tree AS IT
STANDS (per-layer pool stage 4, cross-tile routing, seam mirrors all
landed since), costs the full Mari-style buffer against it, and extends
the recommendation to the UDIM cross-tile follow-up. Every numbered
claim cites its source inline as `[path:line]` (in-tree, read live this
kill) or `[EXT-...]` (external, §7). Claims about Mari/Krita/Photoshop
behavior finer than what the in-tree research corpus records are marked
`(unverified-live)` with the reason in §7.

## 1. The problem, precisely

1.1. Today's Screen-projection paint path (`crates/umber-app/src/viewport.rs:292-360`):
a Ctrl/Cmd + primary drag with a `Screen` projection
(`crates/umber-app/src/paint_state.rs:450-457`, the default) resolves
EVERY pointer event through the live camera + mesh raycast to a UV at
event time — `pick_uv` (`viewport.rs:466-484`: NDC unproject via the
shared `umber_gpu::world_from_ndc` helper, `ray_intersect`, `uv_at`) —
and feeds the hit UV to `begin/extend_screen_point_pressured`
(`viewport.rs:354-358`). Misses skip the stamp (no orphan dabs); seam
mirrors fan out from the resolved UV exactly like a UV-view stroke
(`paint_state.rs:436-438`).

1.2. The raycast itself is Möller–Trumbore over the whole triangle list,
nearest-hit-wins (`crates/umber-mesh/src/raycast.rs:40-89`), with
barycentric UV interpolation (`raycast.rs:92-101`). The module docs
state the scaling wall plainly: no acceleration structure, sufficient
per-pointer picking up to ~100k tris, BVH arrives with the bake engine
(`raycast.rs:1-6`).

1.3. The camera cannot move mid-stroke today BY CONSTRUCTION:
`Viewport::ui` skips `handle_input` (orbit/pan/zoom) while a paint drag
is in flight (`viewport.rs:300-302`), so the view matrix is constant
across a stroke and every event's raycast answers the same question.
The v1 judgment in `paint_state.rs:440-449` names this explicitly: live
raycast, not a deferred Mari buffer, with the deferred
camera-snapshotting buffer as the named follow-up "for when camera
animation during strokes lands".

1.4. What breaks when the camera moves mid-stroke (each failure is a
live-raycast property, not a bug):
1.4.1. **Raycast re-hit drift.** Consecutive screen-adjacent samples
resolve through DIFFERENT view matrices, so the conditioned UV path
(`push_event_styled`, `paint_state.rs:4222-4349`) sees jumps the artist
never drew. The conditioner has no cross-matrix continuity concept; its
only discontinuity handling is the tile-crossing reset
(`paint_state.rs:4237-4246`), which fires spuriously or not at all for
intra-tile drift. Spacing (`dabs_per_radius` + residual carry, research
§5 ¶13 of `docs/research/05-brush-engine-architecture.md:13`) then
spaces dabs along a path that never existed on screen.
1.4.2. **Seam-set flips.** Mirror fan-out gates per event on UV-segment
distance (`paint_state.rs:4263-4291`) under the stroke-level
`stroke_near_seam` arm (`paint_state.rs:1616-1617`). A mid-stroke camera
move changes which island's triangle the ray hits first, so the primary
UV jumps islands mid-stroke and the emitted mirror set flips with it:
the stroke tears at the seam — the exact failure mode
`docs/specs/seam-aware-stamping-design.md:15-23` built the graph to
prevent, reintroduced through the camera.
1.4.3. **Miss-semantics flips.** `extend_screen_point` keeps the stroke
alive across misses so dragging off the mesh and back on continues one
stroke (`paint_state.rs:1781-1796`). Under camera motion, hits become
misses and misses become hits for reasons unrelated to the pointer:
silhouette-adjacent strokes stutter, and drags that opened on a miss
start strokes at camera-dependent positions
(`paint_state.rs:1788-1796`).
1.4.4. **The interaction wall itself.** Consequence (2) of the design
doc's problem statement (`docs/specs/pwys-stroke-buffer-design.md:16-18`):
the artist cannot orbit/zoom/pan while a stroke is active — a hard wall
Substance users do not have, because their stroke buffer owns
screen-space and the camera is free.

1.5. What Mari's buffer solves that a re-raycast per event doesn't: the
frozen stroke space. Re-raycast keeps asking "what is under the cursor
NOW" — under camera motion the question changes mid-stroke, so no
per-event fix (smoothing, wider spacing, re-hit thresholds) can recover
the stroke the artist saw at draw time. A stroke-space buffer answers
"what was under the stroke path at stroke time" instead: the pointer
trajectory is committed in a camera-independent space, and the camera is
free thereafter. The in-tree corpus records Mari's shape as a
screen-space "pane of glass" buffer baked onto the mesh via projection,
with buffer resolution/depth independent of patch resolution, enabling
pre-commit paint editing and 2D transforms
(`docs/research/05-brush-engine-architecture.md:73` [EXT-MARI-BUFFER],
`docs/research/02-competitor-landscape.md:56` [EXT-MARI-ARCH],
`docs/research/04-nextgen-capabilities.md:38` [EXT-MARI-UDIM];
fine-grained bake-mode behavior (AutoBakeAndClear/ClearOnly/Manual)
`(unverified-live)` — recorded in-tree at `02:56` from the vendor doc,
not re-fetched this kill, see §7).

## 2. The candidate architectures, costed

Common ground first. All three candidates sit ABOVE the same downstream
pipeline: conditioned `StrokeEvent`s → `DabAdapter::stamps_to_dabs` →
symmetry expansion → mask gating → per-event `Stage` into
`stroke_owner`'s plane (`paint_state.rs:4321-4349` and the stage-4 block
at `paint_state.rs:345-379`). The candidates differ only in WHEN
screen-space becomes UV-space and WHAT is stored between. The budget is
requirements §12 (`docs/specs/requirements.md:162-166`): input-to-photon
< 20 ms at 60 Hz, 4K texture sets with no dropped dabs at 200 Hz input,
viewport P95 < 16.7 ms on the reference GPU class. The routing reality
is `paint_state.rs:4096-4131`: per-event `route_tile` + lazy
`ensure_tile` (infallible since stage 1b, shared compositor — no shader
compiles on the stroke path), and the per-frame `process_pending` drain
the shell runs every update (`crates/umber-app/src/main.rs:2950,3024,3127`;
the drain contract `paint_state.rs:3724-3743`).

### (a) Full Mari-style offscreen buffer

Stroke space = screen space frozen at stroke start; pointer samples
accumulate as painted pixels in a viewport-resolution offscreen target;
per frame the buffer composites back over the mesh for preview; at
`end_stroke` the buffer bakes down onto the tile pool via projection.

2.1. VRAM: one viewport-size RGBA target (3840×2160×4 B ≈ 33 MiB at 8-bit;
≈ 66 MiB at fp16 — and scene-linear working space, §9, argues for fp16),
PLUS a bake-down mapping (either a fullscreen UV-cache G-buffer written
by an extra mesh pass, or a CPU raycast per covered pixel). Bounded and
affordable — Mari's own buffer caps at 16384×16384 by GPU max texture
size (`05:57` [EXT-MARI-CONFIG]).
2.2. Frame cost: one fullscreen composite pass per frame for preview
(trivially inside 16.7 ms on the reference class) plus, at stroke end,
one bake-down dispatch over the stroke bbox (screen→mesh→UV resolve per
pixel + dab-equivalent splat). The bake-down shader is NEW surface (no
in-tree equivalent; the closest is the Psy-Fi UV-space raster design
recorded at `05:67-71` [EXT-PSYFI]).
2.3. Pipeline breakage (the load-bearing part): per-event staging dies —
nothing lands in tile targets until release, so the live UV-view/export
preview (`paint_state.rs:440-449` names both as the reason v1 rejected
deferral), the per-frame drain cadence the retouch readback depends on
(`paint_state.rs:31-39`: first-event drain + `read_back_rgba8`), and the
per-event staged-batch accounting the seam/symmetry hooks rely on
(`paint_state.rs:4398-4475`) ALL need re-architecture, not adaptation.
Undo's lazy per-tile before-snapshot (`paint_state.rs:48-51`) becomes a
bake-time snapshot of every touched tile at once: correct, but one
frame pays the whole stroke's readbacks instead of amortizing them.
2.4. Verdict on cost: fits VRAM and frame budgets, but maximizes
integration surface (new GPU target, new composite pass, new bake-down
projection shader, undo-capture reschedule, preview-path rework). The
payoff it alone offers over (b) is pre-commit paint editing + 2D
transforms of the wet stroke (`05:73`) — a real artist feature, but a
P1-or-lower feature, not the P1 row under research (which is camera
freedom, requirements §1 line 30).

### (b) Frozen-raycast cache

The hit ray per event is snapshotted against the stroke-START camera and
replayed per event — no GPU buffer, CPU-side. Concretely: `begin_stroke`
snapshots the camera (a plain-data copy — `OrbitCamera` is `Copy`,
`crates/umber-gpu/src/camera.rs:30-31`, with `eye()` at `:90-98` and
`view_proj(aspect)` at `:118-120` both pure functions of the snapshot);
every event builds its ray from the pointer position through the FROZEN
matrix and runs today's `ray_intersect` + `uv_at`; everything downstream
of the resolved UV is byte-identical to today.

2.5. Cost: one `Mat4` + one `Vec3` + aspect per live stroke (tens of
bytes); per-event CPU identical to today (the §1.2 raycast wall neither
grows nor shrinks — the 1M-tri BVH need is orthogonal and shared with
status quo). Zero GPU surface, zero VRAM, zero new shaders, zero new
targets. The per-event staging, drain cadence, undo capture, seam
mirrors, symmetry siblings, mask gating, and pressure folding are
untouched because they all operate post-UV.
2.6. Semantics: paint-what-you-see-AT-STROKE-START — a dab drawn "where
I see the mesh now" mid-orbit lands where the mesh WAS at stroke start.
This is the same contract the `handle_input` guard enforces today by
preventing the move (`viewport.rs:300-302`), so stationary-camera users
see byte-identical behavior by construction. It is pixel-correct at
stroke start and diverges only under wild mid-stroke orbits — a
subsequent-stroke concern, per the 2026-10-12 doc's non-goals
(`pwys-stroke-buffer-design.md:101-108`).
2.7. This candidate IS the 2026-10-12 design doc's option A
(`pwys-stroke-buffer-design.md:51-57`), re-derived here against the
current tree rather than imported on authority: every invariant its §"The
retouch/staging invariants, preserved" (`:79-85`) lists still holds
today (verified live this kill at the cited `paint_state.rs` lines), and
the stage-4 `stroke_owner` snapshot (`paint_state.rs:1596-1600`) is
compatible with it for free (owner snapshots at begin; frozen UVs route
into the same owner's planes).

### (c) UV-space deferral

Events accumulate as conditioned UVs (or raw screen points + pressures);
one splat stages at `end_stroke`. The cheapest to build (no new GPU
anything, no camera snapshot type — pure pipeline rescheduling).

2.8. Cost: minimal CPU/GPU — one batch per stroke, maximal amortization
of undo snapshots. But the latency tradeoff named in the brief is
disqualifying for the DEFAULT stroke path: the artist sees NOTHING until
release, i.e. stroke-length latency (seconds) against the < 20 ms
input-to-photon budget (`requirements.md:162-166`). It converts the
painter into a line tool.
2.9. Pipeline breakage equals (a)'s without (a)'s payoff: the live
preview dies (same two readers as §2.3), the retouch first-touch drain
reads back pre-stroke bytes and never sees the stroke (the before/after
capture inverts), and per-event staged accounting (seam/symmetry/undo
dirty-union over FINAL dabs, `paint_state.rs:52-57`) must be
reconstructed at bake time from stored samples — including faithful
reconstruction of the synthetic event clock (`EVENT_STEP_NS`,
`paint_state.rs:539-550`) or the one-euro/spacing behavior changes
between preview-less staging and today's path.
2.10. Honest place for (c): NOT the freehand default. It is the right
shape for the path tool's replay (`paint_state.rs:292-303` already
replays `PaintPath`s as one stroke through the same pipeline) and for
any future stabilize-and-commit tool — opt-in tools whose contract
already includes deferred commit, where the latency is priced into the
tool choice.

### Cost summary

| Dimension | (a) full buffer | (b) frozen raycast | (c) uv deferral |
|---|---|---|---|
| Extra VRAM | ~33–66 MiB + UV-cache pass | ~0 (one Mat4+eye) | ~0 |
| Per-frame GPU | fullscreen composite | none (today's path) | none until release |
| New shaders/passes | bake-down projection + composite | none | none |
| Live preview (UV view + export bridge) | rework both readers | untouched | dead until release (budget fail) |
| Undo capture | reschedule to bake-time bulk | untouched (lazy per-tile) | inverted (before/after same instant) |
| Per-event seam/symmetry/mask | re-derive at bake time | untouched | reconstruct at bake time |
| 16.7 ms frame risk | low (fullscreen passes are cheap) | none (identical to today) | none, but violates < 20 ms input-to-photon by construction |
| 200 Hz / 4K drop risk | bake-time spike | unchanged (today's raycast wall) | best (one batch) |
| Payoff | pre-commit editing + 2D transforms | camera freedom at ~zero cost | cheapest bulk path for commit-tools |

## 3. The interaction surface: which candidate breaks what

3.1. **Symmetry mirrors** (`paint_state.rs:200-214` docs; snapshot at
`paint_state.rs:1618-1626` into tile-local texel transforms at the
stroke-START tile's density; expansion AFTER the conditioner so siblings
are exact images of the smoothed path). (b): untouched — expansion still
sees per-event staged dabs. (a)/(c): siblings can only be computed at
bake time from stored samples; the bake-time conditioner run is a SECOND
conditioning of the path (or a stored-dab replay that skips conditioning
— either way the "siblings are exact images of the smoothed path"
property must be re-pinned, and the sibling preview during the stroke
dies). Known v1 boundary riding along: symmetry is tile-local and
siblings neither seam-mirror nor are mirrored-from
(`paint_state.rs:210-213`) — bake-time re-derivation is where that
follow-up would have to land, enlarging the kill.
3.2. **Seam-aware stamping** (`seam-aware-stamping-design.md:156-193`
slice-3 wiring; `push_event_styled` expansion at `paint_state.rs:4247-4291`
with the two-deep gate: stroke-level `stroke_near_seam` + per-event
`mirror_positions`; retouch strokes skip fan-out in v1,
`paint_state.rs:4258-4260`). (b): untouched, including the zero-cost
seam-blind fast path (test plan item 5 of the seam doc, `:134-138`).
(a)/(c): the mirror math is UV-space so it RUNS at bake time — but the
v1 property "mirrors recurse through the SAME texel conversion +
conditioning path (spacing, one-euro, lazy-mouse)" (`paint_state.rs:4247-4252`)
is lost: bake-time mirrors are placed, not conditioned, so the fringe
approximation the seam doc bounds by UV stretch (`:85-91`) compounds with
unconditioned placement. Re-pinning the golden seam-stroke test
(`:124-138` items 2–4) under bake-time mirrors is a research task of its
own.
3.3. **Per-layer pool stage-4 routing (`stroke_owner`)**
(`paint_state.rs:345-401` docs; snapshot at `:1596-1600`; staging into
`plane_of_mut(stroke_owner, tile, &channel)` per batch). (b): untouched —
frozen UVs route per event into the snapshotted owner's planes exactly
as today. (a)/(c): routing moves to bake time; a stroke crossing tiles
(and channels — the active channel snapshots at begin,
`paint_state.rs:1634-1637`) must split its baked dabs across planes at
bake time, and the undo capture must bulk-snapshot every touched tile's
before at bake-down instead of lazily per first touch. Correct but
lumpy: one frame pays N tiles' readbacks + N planes' stages.
3.4. **Pen-pressure event streams.** Two sources fold per frame through
the shared resolver — fresh stylus `Motion` wins, then Touch force, else
1.0, with non-positive/non-finite folding to 1.0
(`crates/umber-app/src/stylus_input.rs:48-57`; the Touch-stream
rationale — `multi_touch()` returns `None` below two touches — at
`viewport.rs:307-323`; fresher-than-frame rule at
`stylus_input.rs:13-21`; tilt carried-not-consumed at `:22-26`).
(b): untouched — pressure folds per event as today. (a)/(c): pressure
must be STORED per sample and replayed at bake time together with a
faithful inter-event clock, or the conditioner (which freezes on
duplicate/backwards timestamps, `paint_state.rs:541-550`) and the
spacing math change behavior between the stored and live paths. The
synthetic `EVENT_STEP_NS` clock makes this REPRODUCIBLE (store the seq,
replay the same `time_ns`), but the tilt follow-up (tilt-driven dab
shape, `stylus_input.rs:22-26`) would need its per-sample tilt stored
too — a schema decision baked into the buffer format on day one.
3.5. **Cross-tile UDIM strokes** (`docs/specs/udim-design.md:75-82`: v1
routes per-event to the event's tile; cross-tile continuity is the named
follow-up; tile seams extend the seam graph with tile-offset UV shifts).
Per-event routing today resets the conditioner at tile crossings
(`paint_state.rs:4237-4246`) — the streak guard that continuity must
replace with intent. (b) composes with the follow-up directly: frozen
UVs are still per-event UVs, so tile-seam mirror fan-out (same
`mirror_positions` call, tile-offset correspondence) rides the EXISTING
expansion site — one kill surface closes BOTH the camera row and the
UDIM continuity row. (a)/(c): bake-time routing could route continuously
(no per-event resets at all), which is arguably the better end state —
but it requires the bake-down to own the tile-split + conditioner-span
policy, i.e. re-solving the continuity design inside the buffer kill.
3.6. Net: (b) breaks NOTHING on the interaction surface (all five hooks
operate post-UV and are never touched); (a) and (c) each require
re-deriving all five at bake time, with (a) additionally requiring new
GPU surface to do it on.

## 4. Recommendation, staged kills, test plan

4.1. **Recommendation: (b) the frozen-raycast cache as v1, extended with
tile-seam mirror fan-out so the same kill surface closes the UDIM
cross-tile continuity follow-up.** Reasoning: it is the only candidate
that buys camera freedom without touching any of the five interaction
hooks (§3.6); its stationary-camera behavior is byte-identical by
construction (§2.6), which makes the never-crash doctrine + exact-byte
pins (§4.3) maximally strong; its cost is tens of bytes per stroke
(§2.5); and it affirms rather than second-guesses the 2026-10-12 design
pass (option A), whose invariants were re-verified live against the
current tree (§2.7). (a)'s unique payoff (pre-commit editing, 2D
transforms) is real but outranks nothing on the parity march — record it
as the named follow-up with a demand signal (stencil/projection tools,
`05:73`'s own adoption note: "Adopt as the projection/stencil tool
architecture, not the freehand brush default"). (c) is rejected for the
default path (§2.8) and recommended only for commit-semantics tools
(§2.10).

4.2. Staged kills (each stage is probe → partial → full WITHIN the (b)
architecture; no stage touches GPU code — app-side only, per the 2026-10-12
rollout note at `pwys-stroke-buffer-design.md:110-114`):

| Stage | Surface | Content | Closes |
|---|---|---|---|
| PROBE | `umber-gpu` camera type (snapshot accessor) + `paint_state.rs` (snapshot field + frozen resolve, guard UNCHANGED) | Snapshot the camera at `begin_stroke`; resolve every event against the frozen matrix; camera still cannot move mid-stroke, so the tests pin the frozen rule by driving the camera directly (rotate 90° mid-stroke → second-half dabs land at START-camera positions, exact bytes) | Frozen-matrix rule, zero behavioral delta |
| PARTIAL | `viewport.rs` (conditional guard) + `paint_state.rs` (tile-seam fan-out) | Lift the `handle_input` guard for NON-pointer inputs only (keyboard nav, turntable animation, future gamepad — the pointer still belongs to the stroke, `pwys-stroke-buffer-design.md:74-77`); add tile-offset correspondence to the seam fan-out so cross-tile strokes paint both tiles (the `udim-design.md:75-82` follow-up) | Camera freedom for animation + UDIM continuity |
| FULL | `viewport.rs` + `paint_state.rs` + goldens | Full guard lift (orbit/zoom/pan mid-stroke resolve against the snapshot); golden seam-stroke + frozen-camera tests in the `umber-gpu/golden.rs` harness (the seam doc's items 3–4, `:130-137`); perf-floor assertion (seam-blind + tile-single strokes emit zero extra batches — the seam doc's item 5, `:134-138`) | The §1 P1 row's follow-up ("the camera can't move mid-stroke today", `requirements.md:30`) |

4.3. Test plan shape (never-crash doctrine + exact-byte pins where
derivable):
4.3.1. **Frozen-rule pin (exact bytes):** stroke started, camera rotated
90° mid-stroke by directly driving the camera, more events → second-half
dabs byte-identical to a no-rotation control run (derivable: same frozen
matrix + same pointer UVs ⇒ same rays ⇒ same `ray_intersect`/`uv_at`
outputs — `raycast.rs:40-101` is pure). (Probe stage.)
4.3.2. **Snapshot isolation:** live-camera mutations during a stroke never
touch the stroke's snapshot (assert the stored matrix equals the
start-of-stroke matrix after arbitrary live moves). (Probe.)
4.3.3. **Miss-skip under rotation (exact):** a ray that would miss under
the live camera still hits per the START state and vice versa — assert
both directions on the `viewport.rs:669-697` quad fixture (center hits,
corners miss). (Probe.)
4.3.4. **Live-preview invariant:** mid-stroke tile-target readback shows
staged dabs (the v1 judgment's load-bearing invariant,
`paint_state.rs:440-449`) — assert non-empty dirty union mid-stroke.
(Partial — guards the guard lift.)
4.3.5. **Cross-tile continuity (exact tile routing):** two-tile fixture
(`udim.rs` tests at `crates/umber-mesh/src/udim.rs:136-148` as the mesh
shape) → a stroke crossing the tile seam stages into BOTH tiles'
targets with the mirrored positions from the mirror suite (extends the
seam doc's item 3, `:130-133`, across tiles). (Partial.)
4.3.6. **Perf floor (counts, not images):** seam-blind strokes emit zero
extra batches; single-tile strokes emit zero tile-mirror batches —
assert staged-command counts (the seam doc's item 5). Range checks ONLY
where implementation-defined (GPU timing, lavapipe vs WARP float
rounding on goldens — reason stated per case, per the house test rule).
(Full.)
4.3.7. **Never-crash:** NaN pointer UVs route to `FIRST_TILE`
(`udim.rs:48-55`), OOB triangles default without panicking
(`udim.rs:150-162`), malformed `begin/extend_screen_point(None)` keeps
the stroke alive without staging (`paint_state.rs:1765-1806`) — all
pre-existing properties the buffer kill must not regress; each gets a
frozen-camera variant (NaN/miss UNDER a rotated live camera resolves
against the snapshot).

## 5. Open questions (named open)

- O1 (OPEN): per-event raycast at 1M tris. The frozen cache preserves
  today's O(tris)-per-event cost (§1.2); the perf budget's 1M-tri mesh
  row (`requirements.md:166`) needs the bake engine's BVH regardless of
  this kill. Does the BVH land first and the buffer rides it, or does the
  buffer ship with today's raycast and inherit the wall?
- O2 (OPEN): resolve-at-end (option B, `pwys-stroke-buffer-design.md:58-61`)
  demand signal. The v1 choice (A) diverges under wild mid-stroke orbits;
  what turntable-painting usage would promote B, and who measures it?
- O3 (OPEN): pre-commit editing / 2D transforms (buffer (a)'s payoff,
  `05:73` [EXT-MARI-BUFFER]) — is there a stencil/projection-tool demand
  that justifies the full buffer as a SECOND, opt-in stroke mode beside
  the frozen default?
- O4 (OPEN): real tablet timestamps vs the synthetic `EVENT_STEP_NS` clock
  (`paint_state.rs:539-550`; the Touch-race note at `viewport.rs:307-323`
  and the XI2/Windows-Ink backends row, `requirements.md:141-142`).
  Frozen resolution preserves event timing trivially (per-event flow
  unchanged), but any future bake-time path (O2/O3) must decide the clock
  schema up front (§3.4).
- O5 (OPEN): ortho camera (`docs/ROADMAP.md:177`, open = wave 5). The
  snapshot is convention-agnostic (any `view_proj` matrix freezes), but
  the NDC-unproject path (`viewport.rs:471-484`) assumes the directx
  perspective convention (`camera.rs:107-115`) — ortho changes the ray
  construction, not the buffer, yet both kills touch `pick_uv`'s
  neighborhood and must sequence.
- O6 (OPEN): dynamic-materials interplay. `docs/specs/dynamic-materials-design.md:142`
  cites the pwys design as its doc-form precedent; eval-owned planes
  re-eval from graphs, not strokes — does a frozen stroke landing on a
  material layer need any eval-invalidation beyond today's per-event
  staging? Presumed no (staging is unchanged), but unproven until the
  pool arc's stage 4 settles.

## 6. Judgment calls recorded here (state for the next agent)

- J1. This doc AFFIRMS the 2026-10-12 option-A choice instead of
  reopening A-vs-B from first principles: the tree changed (pool,
  cross-tile, mirrors) but every invariant option A relied on was
  re-verified live. Re-derivation, not deference, is the claim in §2.7.
- J2. The v1 boundary is drawn at frozen-per-event (A), NOT at
  resolve-at-end (B) and NOT at the GPU buffer: B and the full buffer
  are named follow-ups with demand signals (O2, O3), not deferred work
  with designs.
- J3. Tile-seam continuity is pulled INTO the v1 kill surface (§4.2
  PARTIAL) rather than left as the UDIM doc's standalone follow-up:
  the same `mirror_positions` expansion site serves both, and splitting
  them would condition one kill's tests on another's landing.
- J4. Symmetry's tile-local + no-cross-expansion boundary
  (`paint_state.rs:210-213`) is deliberately NOT reopened here: it is
  orthogonal to the camera question (symmetry operates post-UV), and
  bake-time re-derivation (§3.1) is documented as the reason (a)/(c)
  would force it open.
- J5. Costs are argued against the §12 budgets by construction and
  arithmetic (bytes, passes, pipeline touch-count), not by measurement:
  no GPU or runtime run backs any number in §2 (no cargo per the kill
  protocol). The staged kills' test plans carry the measurement
  obligation (goldens + perf floor, §4.3).

## 7. Sources

In-tree (every citation read live this kill):
- `crates/umber-app/src/viewport.rs:225-360` (the `ui` paint arm,
  painting gate, pressure fold), `:466-484` (`pick_uv`),
  `:300-302` (the `handle_input` guard), `:669-697` (picking tests),
  `:511-564` (`bind_paint_target`, active-tile bind).
- `crates/umber-app/src/paint_state.rs:429-457` (`PaintProjection` +
  the v1 live-raycast judgment), `:539-557` (clocks, mirror margin),
  `:1596-1655` (`begin_stroke_styled`: owner snapshot, seam arm,
  symmetry/mask/channel snapshots), `:1765-1806`
  (begin/extend_screen_point), `:200-214` (symmetry docs),
  `:345-401` (stage-3/4 docs), `:4096-4131` (routing + tile creation),
  `:4222-4349` (`push_event_styled`: routing, seam expansion,
  conditioner, adapter), `:3724-3743` (drain), `:4797` (`local_texel`).
- `crates/umber-app/src/stylus_input.rs:48-57` (`resolve_pressure`),
  `:13-39` (freshness, tilt, eraser judgments).
- `crates/umber-mesh/src/raycast.rs:1-101` (no-BVH contract,
  Möller–Trumbore, `uv_at`), `:103-119` (surface-path walk).
- `crates/umber-mesh/src/udim.rs:43-101` (`tile_of_uv`,
  `tile_of_triangle` first-vertex policy, clamps, NaN rule).
- `crates/umber-gpu/src/camera.rs:28-50` (`OrbitCamera` fields),
  `:68-120` (`framing`, `eye`, perspective `view_proj`).
- `docs/research/05-brush-engine-architecture.md:57-73` (§5–§6: GPU
  dab compositing proof, Psy-Fi projection design, Mari buffer
  alternative + adoption note).
- `docs/research/02-competitor-landscape.md:56-58` (Mari paint-buffer
  architecture + the >100-UDIM lesson), `04-nextgen-capabilities.md:38`
  (seam handling + Mari buffer as alternative paradigm).
- `docs/specs/seam-aware-stamping-design.md:7-27` (problem), `:156-193`
  (slice-3 wiring + tests).
- `docs/specs/udim-design.md:39-82` (model, routing, the continuity
  follow-up).
- `docs/specs/pwys-stroke-buffer-design.md` (whole doc — predecessor;
  option A `:51-57`, option B `:58-61`, guard `:74-77`, non-goals
  `:101-108`, rollout `:110-114`).
- `docs/specs/requirements.md:30` (the §1 P1 row + its follow-up note),
  `:162-166` (§12 budgets), `:141-142` (stylus row).
- `docs/specs/dynamic-materials-design.md:142` (pwys as doc-form
  precedent).
- `docs/loop/wave-plan.md:27` (wave-pwys: design-doc-first, deepest
  row), `docs/ROADMAP.md:176-177` (pwys candidate + ortho open).

External (recorded in-tree, NOT fetched live this kill — no network
fetch per the research-kill protocol; each URL below is transcribed
from the in-tree research corpus where it was fetched and dated by the
original research kills, and any behavior claim finer than the
transcribed text is marked `(unverified-live)` in §§1–3):
- [EXT-MARI-BUFFER] https://learn.foundry.com/mari/Content/user_guide/painting/paint_buffer.html
  (via `05:73`, `02:56`, `04:38`).
- [EXT-MARI-APPLY] https://learn.foundry.com/mari/4.5/Content/user_guide/controlling_paint/controlling_the_way_mari_applies_paint.html
  (via `05:73`).
- [EXT-MARI-CONFIG] https://learn.foundry.com/mari/Content/user_guide/controlling_paint/configure_paint_buffer.html
  (via `05:57`).
- [EXT-PSYFI] https://archive.blender.org/wiki/2015/index.php/User:Psy-Fi/GPU_Texture_Painting_Design
  (via `05:67-71`).
- [EXT-MARI-ARCH] https://www.foundry.com/products/mari/features
  (via `02:56`).
- [EXT-MARI-UDIM] https://www.foundry.com/products/mari/features
  (UDIM-at-scale claim, via `04:38`).
- Krita/Photoshop approaches (wash/indirect painting, pulled-string
  smoothing) are cited ONLY to `05:31-33,49` as recorded — no new
  external claims are made by this doc.
