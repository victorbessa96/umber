# Paint-What-You-See — The Stroke Buffer (camera moves mid-stroke)

Date: 2026-10-12 (wave-2 design pass; the implementation brief derives from
this doc when wave 2 dispatches it)
Unblocks: requirements §1's named follow-up ("the camera can't move mid-stroke
today") — the last architectural gap in the painting loop.

## The problem, precisely

Today's live-raycast projection (`PaintProjection::Screen`,
`paint_state.rs:309-330`) resolves every pointer event through the camera +
mesh raycast to a UV at event time. This is correct while the camera holds
still — and `Viewport::ui` deliberately skips `handle_input` during a paint
drag, so it does. The two consequences of that guard:

1. The artist CANNOT orbit/zoom/pan while a stroke is active — a hard
   interaction wall Substance users don't have (their stroke buffer owns
   screen-space; the camera is free).
2. When camera animation arrives (turntable + paint, any future fly-mode),
   the guard would have to break.

The v1 judgment (paint_state.rs:318-327) names the deferred design: snapshot
screen-space points mid-stroke, resolve to UVs at stroke END. That judgment
was right about what breaks — the live preview (UV view + export bridge read
tile targets every frame), the per-frame drain, and the per-event staged
accounting. The design must keep all three working while the buffer exists.

## The buffer design

### The two projections a stroke owns

A stroke in buffer mode carries BOTH spaces:

- **Screen points**: the raw pointer positions + the CAMERA STATE at stroke
  start (the full `CameraUniform`-equivalent: view-projection matrix, eye).
  Snapshotted at `begin_screen_point` — never mutated mid-stroke.
- **The resolved cache**: `(screen_pt, uv)` pairs, resolved lazily against
  the START camera (the same raycast as today, but against the frozen
  matrix, not the live one). A miss resolves to `None` (the same skip-no-
  orphan-dab rule).

The live camera continues to move freely (the `handle_input` guard lifts);
the STROKE's raycasts never consult it again. This is the cheap correct
version of the Mari buffer: no GPU reprojection, no screen-space texture —
just frozen-matrix raycasts at resolution time.

### When resolution happens

Two options, both viable:

- **A. Resolve-at-start (per-event, frozen camera)**: identical to today's
  flow except the raycast uses the snapshotted matrix. The live preview
  keeps working exactly as now (dabs stage per event); the camera can move
  freely because nothing consults the live camera. The only change: a dab
  drawn "where I see the mesh now" during a camera move lands where the
  mesh WAS at stroke start — correct paint-what-you-see-at-stroke-start
  semantics, the same contract the guard enforced by preventing the move.
- **B. Resolve-at-end**: collect screen points, raycast them all at end
  (interpolating the spacing curve through hit/miss gaps), stage the whole
  stroke as one batch. Maximum correctness under camera motion, but breaks
  the per-event preview the v1 judgment documented as load-bearing.

**The v1 choice: A.** It keeps every existing invariant (per-event staging,
the live UV-view preview, the drain cadence, the seam/symmetry hooks' per-
event accounting) with a one-matrix substitution. B is the named follow-up
if real usage shows stroke-time camera motion demanding it (turntable
painting at speed).

### The camera snapshot

`OrbitCamera` (or its `CameraUniform` derivation) gains a `snapshot()`
producing a plain-data copy at stroke start. The stroke stores it; the
raycast helper takes the snapshot instead of `&self`. The live camera's
mutability is untouched. The `handle_input` guard becomes conditional:
STILL skips input while a stroke is live (the artist's pointer belongs to
the stroke), but the camera is free when the INPUT source is non-pointer
(keyboard nav, turntable animation, a future gamepad).

### The retouch/staging invariants, preserved

- Per-event staging stays (option A stages per event against frozen UVs).
- `process_pending` drains per frame as today.
- The undo capture, seam mirrors, symmetry siblings: all operate on UVs
  post-resolution — unchanged.
- The retouch CPU mirror: same.

### Tests

1. A stroke started, camera rotated mid-stroke (the test drives the camera
   directly), more events: the dabs land at the START-camera positions
   (the frozen-matrix rule pinned: rotate 90°, the second half of the
   stroke still paints the ORIGINAL screen ray).
2. The guard lift: camera mutations during a stroke don't touch the
   stroke's snapshot (the camera's live state changes, the stroke resolves
   against the old).
3. The UV-view/export preview stays live during the stroke (readback
   mid-stroke shows the staged dabs — the invariant the v1 judgment named).
4. Miss-skip under a rotated camera (a ray that misses after rotation
   resolves against the frozen matrix — hit or miss per the START state).

## Non-goals (v1)

- Screen-space GPU reprojection (Substance's true buffer) — the frozen-matrix
  raycast is pixel-correct at stroke start; the divergence during a wild
  orbit is a subsequent-stroke concern, not this stroke's.
- Resolve-at-end (option B) — named follow-up.
- Camera ANIMATION authored mid-stroke — the snapshot covers the camera
  state, not the animation timeline.

## Rollout

One kill, app-side only (paint_state.rs + viewport.rs + the camera type);
no GPU changes (the raycast already exists); the guard change is the only
behavioral delta for stationary-camera users (none — identical flow).
