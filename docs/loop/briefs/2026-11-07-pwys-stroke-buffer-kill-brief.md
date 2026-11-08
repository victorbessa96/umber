# Kill brief — pwys-stroke-buffer: the camera may move mid-stroke (row 272)

## Goal

`docs/specs/pwys-stroke-buffer-design.md` (Paint-What-You-See, the stroke
buffer) — option **A** (resolve-at-start with a frozen camera). Today
`Viewport::ui` skips `handle_input` during a paint drag, so the camera
cannot move while a stroke is live; the design lifts that wall by
freezing the stroke's RAYCAST against the camera state at stroke start,
so the live camera may move without disturbing the stroke in flight.

Requirements §1 names this as its last named follow-up ("the camera can't
move mid-stroke today") — the last architectural gap in the painting loop.

## Seat and surface

- Authoring claw for ONE kill: `claw-pwys-buffer`. The dragon reviews,
  gates and lands.
- **Files you may touch: `crates/umber-app/src/viewport.rs` ONLY.**
  - `crates/umber-paint` is NOT touched: `PaintState::begin_screen_point` /
    `extend_screen_point` keep taking already-resolved hits; the paint
    crate's per-event staging, undo capture, seam/symmetry accounting and
    the retouch mirror are unchanged.
  - `crates/umber-gpu` is NOT touched: `OrbitCamera` is already
    `#[derive(Debug, Clone, Copy, PartialEq)]` plain data
    (`crates/umber-gpu/src/camera.rs:55`), so a snapshot is a copy.
  - No new dependencies. No i18n strings (the change is invisible in the
    UI). No Cargo changes.
- Kill-lock `claw-pwys-buffer` is registered over `crates/umber-app/src/viewport.rs`
  by the orchestrator. Do NOT call `scripts/kill-locks.sh`.
- No commit, push, stash, checkout or reset. No cargo (the orchestrator
  gates); bash may be denied in this untrusted workspace — expected, named
  here.

## Read first

1. `docs/specs/pwys-stroke-buffer-design.md` — the design (read all of it;
   the "The v1 choice: A" paragraph is binding).
2. `crates/umber-app/src/viewport.rs` — the anchors below.
3. `crates/umber-app/src/navigation.rs` — the nav preset
   (`camera_op(button, mods)`): navigation is POINTER-button driven today,
   so there is no non-pointer camera source yet.
4. `crates/umber-paint/src/paint_state.rs` — `begin_screen_point_pressured`
   / `extend_screen_point_pressured` take a resolved `Option<Pos2>`; the
   paint crate is read-only for this kill.

## Anchors (all verified in the live tree)

- The frame loop's paint branch (`viewport.rs` ~:995-1060): when
  `painting` is true, `let picked = self.pick(rect, mesh, pos);` →
  `hit` → `paint.begin_screen_point_pressured(hit, pressure)` (:1054) on
  the stroke's first event and `paint.extend_screen_point_pressured(hit,
  pressure)` (:1056) afterwards.
- The guard (~:990-995): `if !painting { self.stroke_from_viewport =
  false; }` then `if !painting { if !stencil_owns { self.handle_input(ui,
  &response); } }` — during a paint drag the camera is frozen because
  input handling is skipped entirely.
- `fn pick(&self, rect: egui::Rect, mesh: &umber_mesh::MeshData, pos:
  egui::Pos2) -> Option<([f32; 2], f32)>` (~:1747): the raycast. It uses
  `self.camera.view_proj(aspect)`, `self.camera.eye()` and
  `self.camera.projection` — the LIVE camera. `fn pick_uv` (:1698) and
  `fn world_at` (:1710) are its siblings.
- The stroke's end (~:980-988): `ends_viewport_stroke(painting,
  p.is_stroking(), self.stroke_from_viewport)` → `p.end_stroke()` — the
  snapshot must be cleared with the stroke.
- The viewport's test module already carries a quad-mesh fixture
  (`world_at_center_hits_quad_center`, ~:2436) — reuse that pattern for
  the pins.

## What to build

- The viewport holds the stroke's frozen camera: a field like
  `stroke_camera: Option<OrbitCamera>` (or a small newtype around it).
  - It is SET when the viewport's stroke begins (the first
    `begin_screen_point_pressured` of a stroke that started in the
    viewport) — the camera state AT THAT MOMENT.
  - It is CLEARED when the stroke ends (the same place `end_stroke` runs).
- The stroke's raycast resolves against the SNAPSHOT, never against the
  live camera: give `pick` a camera argument (e.g. `pick_with(&self,
  rect, mesh, pos, camera: &OrbitCamera)`) and keep `pick` for the
  non-stroke picks (the eyedropper at :804, the ID-mask select at :851,
  the orbit pivot at :1823) — those keep using the LIVE camera.
  The stroke branch passes the snapshot when there is one, the live camera
  otherwise.
- The guard becomes conditional on the INPUT SOURCE, per the design: the
  artist's POINTER still belongs to the stroke (pointer-driven navigation
  stays blocked while a stroke is live), but the camera is not frozen
  forever — a non-pointer camera source (keyboard nav, turntable, a
  future gamepad) may move it. Today's nav preset is pointer-only, so the
  v1 delta is structural: do not block the camera path itself, block only
  the pointer-driven navigation while a stroke is live. If the cheapest
  honest shape is "the guard still skips pointer navigation while a
  stroke is live, and the camera is free otherwise", implement that and
  say so in the notes — do NOT invent a keyboard binding in this kill.

## The pins (in viewport.rs's own test module; the orchestrator runs them)

1. **The frozen rule.** Start a stroke, rotate the camera (the test drives
   the camera directly), send more events: the resolved UVs are the
   START-camera ones, NOT the rotated ones. Assert against the start
   camera's own pick.
2. **The snapshot is independent.** Mutating the live camera during the
   stroke does not change the snapshot (the snapshot's yaw/pitch/distance
   are the start values).
3. **The preview stays live.** The staged dabs are visible mid-stroke —
   the readback of the painted tile shows the stroke while it is still
   being drawn (the invariant the v1 judgment named as load-bearing).
4. **Miss-skip under a rotated camera.** A ray that HITS under the start
   camera still resolves (and paints) after the camera rotates away; the
   hit/miss decision is the start-camera's, never "skip because the live
   camera moved".

## Non-goals (v1, from the design)

- Screen-space GPU reprojection (Substance's true buffer).
- Resolve-at-end (option B) — the named follow-up.
- Authoring camera ANIMATION mid-stroke.
- Any change to the paint crate, the GPU crate, the i18n catalogs or the
  navigation preset.

## Style and gates

- rustfmt (edition 2021) clean and clippy `-D warnings` clean by
  construction: no unwrap/expect/panic outside tests (umber-app denies
  them in non-test code), doc comments adjacent to their items, no
  needless borrows.
- The module docs of `viewport.rs` must describe the LANDED behavior (the
  doc-comment-sync rule: if a doc comment says the camera is frozen during
  a stroke, it must name the new rule).

## Handoff

Return the landing notes INLINE (no report files): what changed, how the
snapshot is held and cleared, the exact shape of the conditional guard,
the four pins, the judgment calls you made where this brief allowed one,
and what you could not verify without cargo.
