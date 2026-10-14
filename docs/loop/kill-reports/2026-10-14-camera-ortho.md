# Kill report — wave-camera-ortho: orthographic projection mode

Date: 2026-10-14. Lock `camera-ortho` (pre-registered; `kill-locks.sh`
not called per the brief). MAIN tree. NO cargo build/test/run, NO
commits, NO git writes. Surface: `crates/umber-gpu/src/camera.rs`,
`crates/umber-app/src/viewport.rs`, this report. `paint_state.rs`,
`main.rs`, the `.ftl` files were READ only (key census + shortcut
table); `umber-gpu/src/lib.rs` was read, never written.

Read before writing: `CLAUDE.md` (trap list) in full; `camera.rs` and
`viewport.rs` in full; `toolbar.rs:188-247` (SHORTCUT_KEYS + table);
`graph_canvas.rs` key usages; `postfx_viewport.rs` in full (route +
`callback(near, far)` at :497-507); `postfx_exec.rs:40-60, 389-393`
and `POSTFX_SHADER` `linearize_depth`/`coc_px`/`fs_dof` (:472-494);
`shaders.rs:822-931` (`GRID_SHADER`); `renderer.rs:242-272`
(`CameraUniform`), `:585-610` (`grid_callback`); `raycast.rs:37-40`
(`dir` need not be normalized); `glam-0.34.1/src/camera/rh/proj.rs`
(the three `orthographic` definitions) + `camera_impl.rs:248-273`
(the matrix body).

## 1. Built

**`crates/umber-gpu/src/camera.rs`**

- `pub enum Projection { #[default] Perspective, Orthographic }`
  (`Debug, Clone, Copy, PartialEq, Eq, Default` — the derivable-Default
  shape from the trap list). `pub projection: Projection` on
  `OrbitCamera`; `Default` and `framing()` both build `Perspective`.
- `projection_matrix` gains the Orthographic arm:
  `half_h = distance * tan(fov_y / 2)`, `half_w = half_h * aspect`,
  `proj::directx::orthographic(-half_w, half_w, -half_h, half_h, near,
  far)` — the `directx` module, matching the perspective arm
  (`camera.rs:114` pre-edit); verified live in the registry that
  `glam::camera::rh::proj::directx::orthographic(left, right, bottom,
  top, near, far) -> Mat4` exists (`proj.rs`, `camera_impl::
  orthographic::<true, true, false>`). Framing parity (target-plane
  footprint identical across a toggle) + the near-stays-eye-relative
  rationale are documented on the function.
- `pub fn toggle_projection(&mut self)` — flips the variant, keeps all
  orbit parameters.
- Module-doc paragraph (framing parity) + enum v1-boundary note (ortho
  is a view aid; DoF stays perspective-only, see below).
- Tests added: 4 (`default_and_framing_are_perspective_and_toggle_round_
  trips`; `ortho_frustum_corner_maps_to_ndc_one_one` at aspect 2.0,
  incl. a `w == 1` pin; `ortho_view_proj_round_trips_via_world_from_ndc`;
  `ortho_depth_does_not_move_ndc_xy`). Each fails loudly on the wrong
  matrix: the corner test misses (1, 1) under any half-extent/aspect
  error, and the depth-collapse test fails under perspective.

**`crates/umber-app/src/viewport.rs`**

- `load_mesh` preserves the current projection across re-framing, same
  as yaw/pitch (`let projection = self.camera.projection; …;
  self.camera.projection = projection`). `framing()` always builds
  Perspective, so without this the toggle resets on every load.
- `pick_uv`: `match` on projection — Perspective arm is EXACTLY today's
  code (`ray_intersect(mesh, self.camera.eye(), dir)`); Orthographic arm
  uses the unprojected NEAR point as origin with the same `far - near`
  direction (legal unnormalized per `raycast.rs:39`).
- `handle_input`: `P` toggle while hovered (`key_pressed(Key::P)`).
  Census: shell `SHORTCUT_KEYS` bind W/G/V/X/I/E/Z/S/O (`toolbar.rs:
  189-199`, table at :203-231); graph canvas takes
  Delete/Backspace/Escape (`graph_canvas.rs:747,891`); nothing binds P,
  so P (mnemonic: Projection) is free. Hover-gated, modifier-free. No
  new i18n strings (`.ftl` files locked) — documented at the call site.
- HAZARD outcome — perspective-only, routed Direct: the post chain's
  ONLY depth reader is DoF (`coc_px` → `linearize_depth`,
  `postfx_exec.rs:477-485`; WGSL `fn linearize_depth` at
  `POSTFX_SHADER` :473-475; CPU mirror `postfx_exec::linearize_depth`
  at :391-393; module docs at :48-53 name it "the directx-convention,
  non-reversed perspective"). The formula
  `near*far/(far - d*(far-near))` inverts perspective depth; under ortho
  the buffer is linear (`near + d*(far-near)`, cf. glam
  `camera_impl.rs:267-272`, `(RH,ZO)=(true,true)` arm), so DoF would
  defocus by the wrong distance. Bloom/Vignette/Sharpen/ToneMap are
  color-only (no `depth_tex` read). Orthographic frames therefore force
  `ViewportRoute::Direct` in `ui()` (before the release check, so the
  HDR target/stack release on the same frame). Module-doc note cites
  the shader lines + the formula; named follow-up recorded there (an
  ortho-aware DoF linear-depth branch re-admitting PostFx).
- Ortho-safety confirm (item 8): `world_from_ndc` (`camera.rs:20-23`)
  divides by `w`; the ortho matrix keeps `w = 1` (last row `0,0,0,1` —
  `camera_impl.rs:248-280`, pinned by the new `w == 1` assert), so the
  divide is a no-op. Grid: `grid_callback` (`renderer.rs:585`) just
  uploads `inv(view_proj)`; `GRID_SHADER`'s WGSL `world_from_ndc`
  (`shaders.rs:876-879`) rebuilds the ray from the NDC near/far
  unprojections (:891-892) and intersects y=0 (:898-902) — ortho rays
  come out parallel, the `abs(ray_dir.y) < 1e-6` guard (:895) still
  applies, arithmetic unchanged. Module-doc paragraph cites these
  lines. Reachable through `umber_gpu::camera::Projection` (no
  `lib.rs` edit — out of surface; the `camera` module is already
  `pub`).
- Tests added: 1 (`ortho_pick_at_center_matches_perspective_pick` —
  ortho centre pick returns (0.5, 0.5) and matches the perspective pick
  within 1e-5). `load_mesh` preservation is NOT unit-tested (needs a
  `GpuContext`); covered by code-reading only — see §3.

## 2. Judgment calls

1. **No `lib.rs` re-export of `Projection`.** Out of surface; the
   `pub mod camera` path suffices (`umber_gpu::camera::Projection`).
   If the dragon prefers the root re-export, that is a one-line
   out-of-surface edit (see §5 — empty by design, listed here instead).
2. **P key, viewport-local, not a shell shortcut.** A shell
   (`SHORTCUT_KEYS`/`ShellAction`) binding would touch `toolbar.rs` +
   `.ftl` strings — both locked. Hover-gated `key_pressed` matches the
   existing drag/scroll input style and cannot collide with typing
   (pointer is over the viewport; text-edit focus implies otherwise).
3. **Ortho forces Direct rather than gating DoF off.** Keeping the
   router (`ViewportRoute::select`) projection-agnostic and overriding
   at the call site means `PostFxStatus`/panel text and the stack plan
   stay coherent; the alternative (teaching `select` about projection)
   would touch `postfx_viewport.rs` — out of surface.
4. **`framing()` stays Perspective-only** (spec item 1); preservation
   is the viewport's job, like yaw/pitch — not a `framing` parameter —
   because `framing` is also called by non-viewport code (golden/
   ref-scene paths) that must keep today's behavior.
5. **Exact-value tests throughout** (1e-5/1e-6/1e-4 tolerances on
   analytic identities, not ranges); the 1e-4 in the pick test mirrors
   the existing `pick_uv_center_hits_center_uv` tolerance. All judgment
   calls above are also written into the two modules' docs.

## 3. Unverified (no cargo per the hard rules — nothing below was compiled)

- Every new/modified signature: `Projection`, `projection_matrix`'s
  `match`, `toggle_projection`, `pick_uv`'s `origin`, the `P` branch,
  the route override, all 5 tests. Read thrice against the trap list
  (`matches!` arity — none used; struct/enum placement — module scope;
  format-string field access — `let`-bound or positional; redundant
  field shorthand — none; `question_mark`/lifetimes — none introduced;
  `#[must_use]` results — none; doc attachment — verify-kill PASS).
- `glam::camera::rh::proj::directx::orthographic` argument order
  `(left, right, bottom, top, near, far)` — read from the registry
  source (`proj.rs` directx module delegates to
  `camera_impl::orthographic::<true, true, false>`), not from memory,
  but uncompiled.
- `Vec3::ONE`, `Vec3::X/Y`, `Vec4::extend`, `clip.w/truncate` — standard
  glam 0.34 API, read-consistent but uncompiled.
- `egui::Key::P` exists in egui 0.36 (consistent with the `Key::W…O`
  bindings in the same crate, uncompiled).
- Intra-doc links (`[`umber_gpu::camera::Projection`]` etc.) resolve
  by reading, not by rustdoc.
- Runtime behavior needing a GPU/run: the toggle key path, ortho
  render output, ortho picking on a real mesh, Direct-forcing with an
  active chain. Request the dragon's Gate-1 battery
  (`umber-gpu, umber-app`) to execute the 5 new tests.
- `load_mesh` projection preservation: code-reading only (needs
  `GpuContext`); the battery cannot cover it headless either —
  reviewer to eyeball the three lines.

## 4. Deferred

- Ortho-aware DoF (linear-depth branch in `linearize_depth` + WGSL,
  re-admitting the PostFx route under Orthographic) — recorded in the
  viewport module docs as the named follow-up.
- Root re-export (`pub use camera::{…, Projection}`) in
  `umber-gpu/src/lib.rs` — left for the dragon (one line, out of
  surface).
- Requirements-row marking (`docs/specs/requirements.md` — out of
  surface) and any `.ftl` strings: none added by design.

## 5. Required out-of-surface edits

None. (The optional `lib.rs` re-export is §4, not required.)
