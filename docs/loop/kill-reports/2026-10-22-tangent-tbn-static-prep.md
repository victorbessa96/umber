# Kill report — tangent-tbn-static-prep (W-C4 research tail)

Date: 2026-10-22. Session: local-7e5953 (dragon). Host-free under the
2026-10-22 standing-charter envelope (full autonomy EXCEPT cargo
build/test — docs + a python port only; no battery, no app bytes).

## Claim

Lock `tangent-tbn-static-prep` (3 paths, all NEW): 
`docs/claw-artifacts/umber-gpu/tangent-wiring-tbn-algebra.md`,
`scripts/tbn_perturb_port.py`, this report. All three outside every
parked manifest and outside every dirty file (pure additions);
`tangents.rs` and the other parked bytes were READ-ONLY throughout.
Claim recorded in `.dragon/COORDINATION-next-wave.md` (STATE block
rewritten + claim section appended, the M4 discipline).

## What this closes

The static-prep half of the tangent brief
(`docs/loop/briefs/2026-10-20-tangent-wiring.md`, steps 3-5 + the
verification ladder's python-port leg): the exact TBN algebra, the f32
port, the flat-fixture byte proof, and the curved fixture's golden
numbers. The WIRING kill (post-recovery) consumes the artifact instead
of deriving from scratch.

## Built

- `docs/claw-artifacts/umber-gpu/tangent-wiring-tbn-algebra.md` —
  the derivation, the WGSL + CPU-mirror pair, both fixtures' tables,
  the encoding/layout map, the wiring kill's remaining-owes list.
- `scripts/tbn_perturb_port.py` — the f32 port (every arithmetic op
  rounded through IEEE f32): the current mikkelsen formula, the
  corrected TBN formula, the brief's no-stretch sketch (kept to
  quantify the breakage), `vertex_tangent_frames` with the recommended
  stretch extension, and the Lit golden byte path. Validates the
  foundation module against all 7 umber-mesh unit-test expectations,
  asserts the flat-golden byte agreement, prints the cylinder probe
  tables. Run: exit 0, "PORT: ALL CHECKS GREEN" (verified live this
  session).

## Judgment calls

1. **The correction is the kill's headline.** The brief's step-3
   sketch (`normalize(n − (dhdu·T̂ + dhdv·B))`, unit T̂) drops the
   parametrization stretch the screen-space formula carries
   implicitly; on the flat goldens' own fixture (su = sv = 2) it
   flips the +u golden's diffuse 0.2583 → 0 and breaks bytes the
   brief predicted identical ([95,100,122] → [69,75,98]). Corrected
   form: `n' = normalize(n − ((dhdu/su)·T̂ + (dhdv/sv)·B̂))`. The
   alternative (keeping the sketch, weakening the goldens) was
   rejected: it would silently make `height_strength` mesh-dependent.
2. **Stretch as per-vertex attributes [f32;2], not in-shader
   reconstruction.** The 2×2 dpdx/dpdy facet solve would reintroduce
   the derivatives the rewrite exists to delete and make the gradient
   magnitude per-facet (the piecewise defect in magnitude, if not
   direction). Vertex grows 32 → 56 bytes; both stride sites share
   the `size_of::<Vertex>()` constant so the openpbr pipeline needs
   no edit.
3. **Stretch = per-weld-group MEAN of contributing |t_face| /
   |b_face|**, fallback 1.0 for zero-contribution groups — matching
   the foundation module's unweighted accumulation convention
   (foundation judgment 2) and keeping a sane stretch even if
   opposite-u islands ever weld.
4. **v1 adopts the orthogonal-parametrization form** (F = 0); the
   non-orthogonal metric residual is derived, bounded, and named as
   the wiring kill's report duty, not smuggled in silently.
5. **The nostretch variant stays in the port** (not deleted) so the
   breakage table is reproducible, not folklore.

## Unverified

- Nothing GPU-side has run (no cargo under the envelope): the WGSL
  form, the attribute wiring, and the battery's GPU goldens are the
  wiring kill's. The flat agreement here is CPU-side f32-exact.
- The GS-collapses-to-analytic property (§5 of the artifact) is
  proven for the cylinder analytically and holds numerically at every
  column; it is NOT claimed for arbitrary meshes — curved-mesh frame
  quality in general is the foundation module's own unverified item.
- su's chord bias (−0.16% at 8 segments) is fixture-specific; the
  wiring kill's golden uses the accumulated (port) values, so the
  bias is pinned by construction, not corrected away.

## Deferred

- The wiring itself: the brief's timing decision stands (post
  w-c3a/w-c3b close, clean files, its own manifest re-pin).
- Carrying F (non-orthogonal shear) — one more attribute component
  if a real mesh ever shows it matters.
- The closing amendment to the 2026-10-12 heightnormal report —
  the wiring kill writes it with its own landing evidence.

## Process note

Solo dragon cycle under the standing charter: census (zero live
siblings, zero locks held, 78 dirty files all parked-fleet bytes
untouched), lock before any repo write, dcommit path-limited, lock
released in the commit step. The port ran green on first full
execution; the only post-write edit was cosmetic (a duplicate
constant line removed before the run).
