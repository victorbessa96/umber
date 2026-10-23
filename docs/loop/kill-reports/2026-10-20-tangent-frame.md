# Kill report — tangent-frame (W-C3b kill 4)

Date: 2026-10-20. Session: local-4d2e8a71 (dragon). Host-free under
the 2026-10-20 code steer (FREEZE-4, centralhost dark); battery at
HOST_BACK via scripts/w-c3b-close.sh's fourth manifest row (the
script grows with this kill — its dry-run re-verified after).

## Claim

Lock `tangent-frame` (3 paths: NEW `crates/umber-mesh/src/tangents.rs`,
`crates/umber-mesh/src/lib.rs` registration, this report). Both code
files verified OUTSIDE every parked manifest (umber-mesh's only
pinned file is id_mask.rs, the fleet's) and outside my own three
W-C3b manifests before the first edit.

## What this closes

- The foundation half of requirements row 79's named follow-up
  ("true per-vertex tangents"): the per-vertex tangent frame
  computation, pure CPU, unit-tested, python-ported. The CONSUMER half
  (the renderer wiring: vertex attribute, pipeline layouts, the TBN
  height gradient replacing the per-facet derivative frame, the lit
  goldens re-derived) is the named next-after-recovery kill — it
  re-pins the solo-height manifest's shaders.rs/renderer.rs rows and
  deserves fresh-context shader surgery, not a deep-context rush.

## Built

- `umber-mesh/src/tangents.rs` — `vertex_tangent_frames(positions,
  normals, uvs, indices) -> Vec<TangentFrame>`:
  - **Weld-group accumulation, the one design correction the scoping
    forced.** umber's importer output is per-corner EXPANDED (shared
    corners are bit-identical duplicate slots, indices are always
    sequential), so textbook index-sharing mikkTSpace accumulation
    would never smooth anything on this data. Corners weld into
    groups by the bit-exact (position, normal, uv) tuple — the
    graphics-standard tangent-seam rule: a UV seam legitimately
    splits groups and keeps per-island frames.
  - Per-triangle Lengyel UV-derivative tangents
    (`T = (dv2·e1 − dv1·e2)/det`, `B = (du1·e2 − du2·e1)/det`),
    unweighted per-corner accumulation into the weld groups.
  - Gram-Schmidt against the group's normal, handedness from
    `sign(dot(cross(N, T̂), B_sum))` — the compact encoding
    (`B = handedness · cross(N, T)`).
  - Never panics on untrusted input: out-of-range indices and
    missing-attribute triangles skipped, a `% 3` remainder ignored,
    degenerate (zero-UV-area, |det| < 1e-10) triangles contribute
    nothing, and a zero-contribution group takes the deterministic
    fallback (the normal's least-dominant-axis perpendicular, ties to
    the lowest index; a zero normal takes `[1, 0, 0]`).
- `umber-mesh/src/lib.rs` — `pub mod tangents;` (alphabetical slot;
  registration lands in this kill per rule 13). `pub` API in a lib
  crate — no dead-code lint while its consumer is the wiring kill.

## Judgment calls

1. **Weld by the full attribute tuple, not position alone.** Position-
   only welding would smear frames across UV islands and across
   normal seams; the tuple is the conservative weld (corners merge
   only when every attribute agrees), and the seam test pins the
   split. Alternative rejected: tolerance-based welding — importers
   produce bit-identical duplicates, so exact match is both correct
   and deterministic.
2. **Unweighted per-corner accumulation** (the mikkTSpace shape).
   Area-weighting would bias large triangles; mikkTSpace's own
   accumulation is unweighted and the literature considers the
   difference sub-visual. Named if the wiring kill ever shows bias.
3. **Absolute degeneracy threshold 1e-10.** Real importer UVs sit far
   above it; a relative epsilon needs a reference scale a pure fn
   does not have. Documented at the check.
4. **The fallback is a real frame, not a zero.** A zero tangent would
   poison the future shader gradient; the deterministic perpendicular
   keeps every downstream consumer total.
5. **The consumer wiring is deliberately NOT in this kill.** It
   re-pins my own solo-height bytes and re-derives the lit goldens —
   the heightnormal kill's deliberate derivative-frame choice
   ("no tangent attribute required") gets reversed by the WIRING
   kill, with its own report; this kill ships the reusable math
   both that kill and any future normal-map work consume.

## Unverified (until the battery at HOST_BACK)

- The compile + the 7 unit tests (rustfmt clean parse after one
  canonicalization pass; the python port agrees 7/7 with the Rust
  test expectations — the seam fixture's FIRST draft had a
  hand-derivation error (u ran along +X on the XZ triangle, making
  the test vacuous), caught during the python re-derivation BEFORE
  any test ran, fixed in the fixture).
- Real-mesh behavior (a curved imported mesh's frames) — the unit
  fixtures are synthetic; the wiring kill's goldens cover real data.
- The 1e-10 threshold on pathological-but-real UV layouts.

## Deferred

- The renderer wiring (the named next-after-recovery kill — see
  judgment 5).
- Area-weighted accumulation, if bias ever shows (judgment 2).

## Process note

The kill's files are all outside every manifest, so nothing re-pins.
The close script grows a fourth manifest row; its dry-run re-verified
byte-identical after the growth (the plan print now lists four rows).
