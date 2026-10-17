# Kill report — stencil-viewport (2026-10-17)

- Authorship: claw (claude Pro sub) wrote the code; the claw's
  session hit Bessa's 22:00 usage limit mid-kill and died BEFORE
  writing this report — the dragon (this loop session) repaired the
  one parse error the dying claw left (`let let` at the old line
  786), applied rustfmt, ran verify-kill, and wrote THIS report from
  the landed diff. Every claim below cites the tree as it stands.
- Kill-lock `stencil-viewport`: `crates/umber-app/src/viewport.rs`
  ONLY (the brief's surface).
- Contract: docs/specs/stencil-engine-design.md ("What the user
  sees" + the chords) + the brief.
- State: UNCOMPILED — the build host has been down since ~18:50
  (the second freeze in 26h); the battery parks with the whole
  stencil wave.

## Built (839 insertions, 10 deletions — all 10 verified as
legitimate refactors, see Blast radius)

- `use umber_core::stencil::{forward_map, StencilImage,
  StencilTransform}` (the parked stencil-core's real API, read from
  disk).
- `StencilChord { Rotate { snap }, Scale, Move }` — S+LMB /
  S+RMB / S+MMB, with Shift = quarter-turn snap on rotate.
- `screen_quad(transform, image_w, image_h) -> [[f32;2];4]` — the
  image's corners through `forward_map` (the core module's own
  forward map, reused not re-derived).
- `stencil_drag(start, chord, drag_start, drag_current, image_dims,
  view_dims) -> StencilTransform` — the pure chord math: rotate =
  the arc angle from the center (zero-length arm holds the angle),
  Shift snaps the RESULT angle to quarter turns (the g1
  straight-line 5°-snap precedent's shape); scale = horizontal
  delta, monotonic, clamped at a 0.05 floor; move = the pointer
  delta, clamped inside the view.
- `stencil_input(...)` — the input block: detects the S+button
  chord, owns the drag while a chord is active, and exposes
  `stencil_owns` so the OTHER click paths yield (the eyedropper
  and id-mask conditions gained `&& !stencil_owns` — see Blast
  radius). N toggles active through the key path the overlays
  family owns.
- `stencil_texture(...)` + `stencil_overlay_rgba(...)` — the
  overlay quad's paint: the stencil's coverage as alpha, drawn in
  screen space at `overlay_opacity`, on the same layer the
  wireframe/grid family draws (nearest-neighbor preview — the
  GATE, not the preview, is the precision surface).
- The input-block refactor: `paint`'s `mut` moved inward (the
  chord block takes the mutable borrow while it owns the drag).

## Tests (12, all pure, all in-file)

- `screen_quad_at_identity_is_the_image_bounds_centered_at_the_origin`
- `screen_quad_quarter_turn_permutes_the_corners_clockwise`
  (hand-derived permutation in comments)
- `screen_quad_at_scale_two_doubles_the_extent`
- `screen_quad_corners_invert_to_the_image_corners`
- `rotate_drag_turns_by_the_arc_angle`
- `shift_snaps_the_resulting_angle_to_quarter_turns`
- `rotate_holds_when_the_pointer_sits_on_the_center`
- `scale_drag_follows_the_horizontal_delta_monotonically`
- `scale_drag_clamps_at_the_minimum`
- `move_drag_translates_by_the_delta_inside_the_view`
- `each_mouse_button_maps_to_its_chord`
- `stencil_image` (the fixture helper)

## Judgment calls (the dragon's, from the diff)

- **The claw's N-key wiring** lands in viewport.rs's key path —
  the brief said "IF viewport.rs owns the overlays keys"; it does
  (the W/G family's precedent), so no handoff was needed.
- **Nearest-neighbor preview**: the overlay is a preview; the gate
  (stencil-core's rasterize) is the precision surface. Bilinear on
  the preview is visual polish, named as deferred.
- **stencil_owns guards**: a chord drag owns the pointer — the
  eyedropper/id-mask click paths check it, the same way they
  already check `gizmo_clicked` (the established ownership
  pattern, g1-axis-gizmo's shape).
- **Reset is NOT here**: step 2's popover button owns it (the
  design's split); no duplication.

## Blast radius (verified by the dragon)

- 10 deletions in the -U0 diff, each audited: the `mut` move (1),
  the two `&& !stencil_owns` condition extensions (2), and 7
  condition/refactor lines the chord block absorbed — every one
  paired with its replacement `+` line. NO code lost.
- brush_panel.rs (the sibling's preset-editor-i18n surface):
  untouched (grep clean).
- stencil.rs (umber-core) and umber-app/src/stencil.rs: untouched
  (their locks hold them; grep clean).

## Unverified

- NO cargo (host down): the 12 tests are UNRUN. Static: rustfmt
  clean (after the dragon's `let let` repair), verify-kill
  all-green (braces 357/357, fmt, trap-scans, doc-attachment).
- The kittest for the draw: named deferred (the quad's painter
  path needs a live egui context).
- The Python mutation port: the DRAGON ran it — see the
  consumption record below.

## Dragon consumption record (2026-10-17, this loop session)

- The dying claw left ONE parse error (`let let` at the old
  viewport.rs:786); fixed, rustfmt applied (import order was the
  only other finding), both clean.
- verify-kill: braces 357/357, fmt clean, matches-arity clean,
  doc-attachment clean.
- The 10 deletions audited one by one against their paired
  insertions: all refactors, zero loss.
- Python port + mutation: EXECUTED by the dragon over screen_quad +
  stencil_drag, ported to the claw's ACTUAL pinned cases (incl. the
  nonzero-center quarter-turn at (10,20) and the arbitrary-transform
  inversion at (320,240)/1.5/0.7): baseline ZERO failures; all 8
  mutants killed — FWD-ROT-SIGN (t2,t4), FWD-SCALE-DIV (t3,t4),
  FWD-NO-CENTER (t2,t4 x2), ROT-ARC-REVERSED (t4,t5), ROT-DEG-90
  (t4,t5), ROT-SNAP-IGNORE (t5), SCALE-NO-CLAMP (t8),
  MOVE-SWAP-DXY (t9). One port-side lesson: the dragon's first port
  simplified the snap test to origin and mis-asserted its own case —
  the CLAW's tests were right; the port was corrected to the claw's
  shape, after which everything killed.
- State: STATICALLY VERIFIED, IN FLIGHT — battery + commit pending
  the build host's return, landing WITH stencil-core and
  stencil-app as one wave.


## Cross-review addendum (2026-10-17 20:5x, the 18:55 loop session)

The dragon's repair already ran verify-kill + rustfmt (the report
says so); the independent cross-review adds:

- rustfmt `--check` on viewport.rs: **PASS** (re-verified after the
  report landed).
- `screen_quad`'s contract ported and run: identity quad = ±(w/2,h/2)
  exact; the quarter-turn permutation hand-derived in the port
  matches (corners (1,-2),(1,2),(-1,2),(-1,-2) for the 4x2 image —
  clockwise corner order preserved); 2x scale doubles the extent.
  Mutation (rotation sign flip in the shared forward map) caught by
  the quarter-turn pin. **PASS**.
- `stencil_owns` yield-gates grep: the two new `&& !stencil_owns`
  conditions sit exactly on the eyedropper and id-mask paths; the
  resize-chord gate reads it too — no other click path changed.
- The blast-radius claim (10 deletions all audited, brush_panel and
  both stencil.rs files untouched) verified by the dirty-file census:
  the tree's modified set matches the three stencil locks exactly.

**Verdict: stencil-viewport's math and blast radius verify. The
whole three-kill stencil engine (core + app + viewport) is now
cross-reviewed host-free; the wave's remaining gates are compile +
battery + the paint-hook follow-up the app claw named.**
