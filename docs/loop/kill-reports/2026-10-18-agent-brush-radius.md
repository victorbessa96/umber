# Kill report: agent brush radius (2026-10-18)

Dragon-authored host-free by the Umber /loop session "Work on Umber
project coordination #3" (local-d3deea1d3c7631cc80f8782e73c37fe384c4c50d1248d89def19e4801b4c9a45, wakeup #1, under Bessa's live steer
"do all the work that you can locally, keep developing the project").
Kill-lock `agent-brush-radius`, holding exactly three files:
`crates/umber-paint/src/paint_state.rs`,
`crates/umber-cli/src/agent.rs`,
`crates/umber-cli/tests/agent_paint_ops.rs`.

Context: docs/specs/agent-paint-ops-design.md names its own follow-up
— "an absolute-radius field needs a `set_brush_radius` pub seam —
follow-up". Scouting found the seam's multiplier half already shipped
(`PaintState::set_brush_size`, `sanitize_size`-clamped `0.05..=8.0`,
called by the GUI's main/toolbar/viewport); what v1 lacks is the
absolute form and the wire field. This kill closes both.

## Built

- `paint_state.rs`: `set_brush_radius_texels(radius)` — the pub seam,
  `set_brush_size(radius / BRUSH_RADIUS_TEXELS)`, so a finite radius
  lands in `1.2..=192.0` texels and a non-finite radius resets to the
  historical 24. A stroke in flight keeps the size it began with (the
  `set_brush_size` rule).
- `agent.rs`: `PaintStrokeStep.radius: Option<f32>`
  (`deny_unknown_fields` intact). Validated BEFORE `project_mut` — a
  refused radius costs no GPU device: non-finite or `<= 0` is a typed
  error naming the clamp range. Applied after `set_exact_input`,
  before `begin_stroke_styled`: it SETS the session brush (state, not
  per-stroke — the GUI's brush-size rule; `paint_undo` restores
  pixels, never the brush). The reply gains `radius` — the clamped
  radius actually in force for this stroke.
- `PROTOCOL_VERSION` stays 1: the bump contract is post-0.1.0-tag only
  (the comment now records the field + the date).
- `tests`: a `radius: 0` refusal case rides
  `paint_inputs_are_validated_before_any_gpu_work` (the no-GPU half);
  the new `an_absolute_radius_paints_wider_and_persists` pins the
  geometry (a 96-texel stroke paints a texel 48 past its segment end
  that the default 24 cannot reach; the gap between the two strokes
  stays byte-exact zero; the narrow stroke's centre still lands), the
  session-state rule (a later stroke without `radius` inherits 96),
  and the clamp (1000 → 192) — color/presence pins per the suite's
  exactness rule, never an exact alpha.

## Judgment calls

1. Session state, not per-stroke — `radius` persists across strokes
   exactly like the GUI's size slider. A per-stroke-only absolute
   radius was considered (radius / brush_size folded into the scale)
   and rejected: derived, fragile, and not the GUI's semantics.
2. Typed refusal for `<= 0` before any GPU work (the opacity rule:
   never silently store a meaningless value). The non-finite arm is
   unreachable from the wire (JSON carries no NaN/Inf tokens) — it
   guards the seam for internal callers, the same shape as
   `radius_scale`'s check.
3. The reply always carries `radius` (not only when the field was
   sent): a driver learns the session's live radius every stroke,
   clamped value included — mirrors `layer_set_opacity`'s
   stored-value reply.
4. No engine unit test for the seam: it is one delegated line into
   `set_brush_size` (`sanitize_size` is pinned in umber-brush's own
   tests); the integration test exercises the full path over the
   process boundary (spawn → JSONL → clamped reply).

## Unverified

- No compile run (host down since 10-17 21:48; the outage contract
  parks the battery). Compile risk swept statically: the seam and both
  getters exist `pub` with matching signatures (`(&mut self, f32)` /
  `(&self) -> f32`); umber-paint is umber-cli's direct dep (agent.rs
  already imports `PaintState`); `Option<f32>` + `#[serde(default)]`
  is the step-struct idiom; the reply's `f32` serializes through the
  existing `json!` path; the `if let Some(radius)` binding and the
  later capture live in disjoint scopes (no shadow clash).
- The GPU test legs run under the battery (lavapipe on the host / CI
  adapters); the suite's `is_gpu_skip` guard stands.

## Gates run (host-free, this session)

- `rustfmt --check --edition 2021`: CLEAN on all three files (the
  first pass flagged two long asserts in the new test; rustfmt's own
  wrap applied — the parked-bytes fmt-clean rule, the dialog_buttons
  lesson).
- `scripts/verify-kill.sh`: 3/3 PASS (brace census, trap greps,
  doc-attachment, lock audit — the `agent-brush-radius` lock live).
- Interface coherence census: `set_brush_radius_texels` +
  `brush_radius_texels` exist `pub` with matching call shapes; the
  doc chain updated end to end (module docs, `PaintStrokeStep`,
  `PROTOCOL_VERSION` comment, the design doc's dated appendix).

State: STATICALLY VERIFIED, IN FLIGHT — battery (scope: umber-paint,
umber-cli) + commit-by-path + push parked on the build host's return,
landing in the same recovery session as the stencil wave and e0063
(independent files, its own commit row per the manifest labels).

## Addendum — battery-caught defect in this kill's radius wiring, fixed + re-verified green (2026-10-19, wake session)

The local battery's --no-fail-fast pass (2026-10-19,
/tmp/local-battery.log) caught `an_absolute_radius_paints_wider_and_persists`
RED: the wide stroke (radius 96, reply 96) painted at the historical
24 — the readback's wide-edge texel came back empty while the reply
kept claiming 96. Root cause: the stroke machinery reads ONLY the
per-stroke scale — dabs take `BRUSH_RADIUS_TEXELS *
sanitize_size(radius_scale)`; the `brush_size` field that
`set_brush_radius_texels` sets never enters the stroke path by
itself. The live-event precedent composes the session brush at the
CALL SITE (`push_event` passes `brush_size` as the scale); the kill
set the state but passed the wire's `radius_scale` alone, so the set
radius painted nothing wider — and the persistence rule ("later
strokes inherit it") held in the REPLY only, nowhere behavioral. Fix:
`agent.rs` folds the session brush at the same call site —
`scale = radius_scale * paint.brush_size()` for BOTH
`begin_stroke_styled` and `extend_stroke_styled` (`sanitize_size` on
the product is the documented 0.05..=8.0 multiplier clamp; every
radiusless stroke is unchanged while `brush_size` is 1.0). Gates:
rustfmt CLEAN; re-verified live: the test passes (37.11s, the real
agent surface; 2026-10-19,
`cargo test -p umber-cli --test agent_paint_ops`). The `agent.rs`
row re-pinned below; this report's row re-pinned with it. The design
doc NEEDED NO CHANGE — its table already promises `radius_scale`
"multiplies the session brush's radius"; the code was the deviation.
Recovery row UNCHANGED: same combined row with agent-export-naming,
same battery scope (umber-paint + umber-cli), same lock releases.
