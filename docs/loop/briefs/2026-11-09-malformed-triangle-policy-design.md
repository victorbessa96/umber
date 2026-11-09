# Design brief — 2026-11-09 — wave 1.2: ONE mesh-wide malformed-triangle policy

Author: dragon (Razul). Status: LIVE. Plan of record: `docs/loop/waves/2026-11-09-wave-plan.md` wave 1, item 1.2.

## The question (named by the wave plan and row 298)

After W1.1 (`7aad67c3`, UV_COND_EPS conditioning, landed+green), the two
geometry lookup paths agree on the TEXEL SET. The only remaining
divergence is what they DO with a malformed winner:

- **The fill skips malformed triangles.** `fill::rasterize_uv_geometry`
  and `fill::uv_coverage` pass `triangle_positions_finite`
  (`crates/umber-mesh/src/fill.rs:623`) as the walk's `filter`
  (`fill.rs:562-573`, `fill.rs:608-615`): a triangle with a non-finite
  position, or a position index out of range, is never offered, so an
  EARLIER finite triangle keeps the texel.
- **The projector keeps winner semantics.** `rasterize_projection`
  (`crates/umber-app/src/projection.rs:700-713`) passes `|_| true`
  (`projection.rs:705`), so a malformed winner IS visited; its
  non-finite world point fails the `clip.w` guard (`projection.rs:674`),
  `shade` returns `None`, and the v2d rule "every yield WRITES" turns
  that into transparent `[0,0,0,0]` — the malformed triangle BLANKS its
  texels, erasing any earlier finite triangle's paint.

Row 298 preserved this asymmetry deliberately because unifying changes
bytes. W1.1 has since landed; the wave plan now calls for one policy.

## The decision

**A triangle whose positions are non-finite (or whose position indices
are out of range) covers NOTHING, on every path, structurally.**

Rationale:

1. A NaN position has no defined world point; "the winner decides and
   blanks the texel" lets invisible garbage erase paint. That
   contradicts the projector's own stated principle at
   `projection.rs:733`: "malformed geometry never silently kills
   paint."
2. It is the same rule W1.1 already established for UV footprints:
   ill-conditioned triangles are not valid footprints. A triangle with
   no finite positions is the position-space degenerate of the same
   class.
3. The fill already behaves this way; the projector adopting it makes
   the two paths agree by construction, not by convention.

Rejected alternative (do not re-derive): make the fill adopt the
projector's winner semantics (malformed winner writes "no value" =
uncovered). That would change FILL bytes and `uv_coverage`, and export
padding's dilation consumes `uv_coverage` — a wider blast radius for a
policy that erases paint. The fill's rule is the safer and already
shipped one.

## Byte-change analysis (the honest cost)

- `fill::rasterize_uv_geometry`, `fill::uv_coverage`: **byte-identical**
  (they already filter).
- Projector raster: changes ONLY on meshes containing a triangle with
  non-finite positions or out-of-range position indices whose UV
  footprint is valid and was the winner of some texel. Those texels now
  keep the earlier finite triangle's value, or stay transparent when no
  finite triangle covers them (identical to today when the malformed
  triangle stands alone). Well-formed meshes: byte-identical.
- GPU bake path: out of scope and unaffected — the G-buffer is UV-space;
  it never reconstructs world positions. Named, not changed.
- Export padding (consumes `uv_coverage`): unchanged.

## The change (structural, not conventional)

Make the policy a property of the WALK, so no caller can accidentally
keep malformed winners:

1. `uv_gbuffer::for_each_uv_geometry` composes the caller's `filter`
   with the position-finiteness rule internally:
   `|tri| triangle_positions_finite(mesh, tri) && filter(tri)`.
   The walk's own docs already say malformed/non-finite-UV/
   ill-conditioned triangles "cover nothing and are never offered"
   (`uv_gbuffer.rs:450-452`) — this extends that sentence to
   non-finite positions.
2. Move `triangle_positions_finite` (and its `pos_finite` helper) from
   `fill.rs` into `uv_gbuffer.rs` as `pub`, re-exported from
   `crates/umber-mesh/src/lib.rs` next to `for_each_uv_geometry`
   (`lib.rs:51`), so "one formula" is literally one function.
3. `fill.rs`: delete the private `triangle_positions_finite`/
   `pos_finite`, both call sites (`fill.rs:568`, `fill.rs:613`) pass
   `|_| true` — the walk enforces.
4. `projection.rs:705`: stays `|_| true`; the enforcement is now
   structural. Doc comment updated to name the landed policy.
5. The `[NaN; 3]` fallback for out-of-range position indices
   (`uv_gbuffer.rs:486`) becomes unreachable through the walk and is
   kept only as the defensive `unwrap_or` (or simplified — the claw
   decides, names it in the report).
6. **Doc-comment sync in the same kill** (doc-sync rule): the bullet at
   `uv_gbuffer.rs:464-469` ("Malformed positions are still visited ...
   today's malformed winner blanks the texel") must be rewritten to
   name the LANDED behavior: malformed-position triangles are never
   offered, on every path; the projector no longer blanks their texels.
   Same for `fill.rs:538-542` and `projection.rs:29-31, 696-699`.

## Pins (the kill's proof)

- `walk_never_offers_a_non_finite_position_triangle`: mesh = finite
  quad + one triangle with a NaN position and valid UV footprint
  overlapping the quad; walk with filter `|_| true`; assert zero yields
  belong to the malformed triangle (count via a wrapping filter that
  records offered indices — note the filter is called only for
  UV-valid triangles, so also assert the composed rule: wrap
  `triangle_positions_finite` visibility through the public API).
- `walk_never_offers_out_of_range_position_indices`: same with an
  index like 999.
- `projector_keeps_the_finite_winner_under_a_malformed_one`:
  projection raster on that mesh — texels under the malformed
  triangle's footprint get the FINITE triangle's shaded colour, not
  `[0,0,0,0]`.
- `projector_alone_malformed_triangle_stays_transparent`: malformed
  triangle with no finite competitor — texels transparent, same as
  pre-1.2.
- `fill_and_projector_agree_on_a_hostile_corpus`: fill coverage mask ==
  set of texels the projector walk claims, over a corpus with NaN
  positions, inf positions, out-of-range indices, degenerate UV,
  needles (post-W1.1) — the two paths cannot diverge because one walk
  enforces one rule.
- Rewrite the row-298 pins that pinned the OLD asymmetry (search
  `projection.rs` / `uv_gbuffer.rs` tests for "visits" / "winner" /
  "blank"): they now assert the unified policy. Named in the kill
  report as deliberately reversed, with the byte-change note above.

## Gate

clippy `-D warnings` on umber-mesh + umber-app (`--all-targets`), their
test suites, rustfmt on touched files, `check-layers.py` GREEN,
`scorecard.py --check` current (regenerate if the test count moved),
plus the release CI after push. No WGSL change, no GPU leg needed for
this kill (umber-gpu untouched), but the release gate runs it anyway.

## Non-goals

- No change to UV-space rules (W1.1's domain) or to the GPU path.
- No change to `world_and_face_normal_at_uv` / the per-point lookup
  winner rule — after W1.1 the point lookup and the walk already agree
  on the texel set; 1.2 unifies the malformed-winner handling.
- Depth/occlusion culling is wave 2.1, a separate kill.
