# Kill report: C2-9, material layers stay inside their own texture set

Date: 2026-10-15. Author: dragon. Lock: `c2-9-parked-materials` over
`crates/umber-app/src/{main,document}.rs` and this report. Follow-up from
C2-9 stage 4's open item ("parked sets' material layers").

## Finding

The open item was framed as "parked material layers keep their last
composite until activated". Reading the drain shows that behavior is
already correct:

- on activation, `MaterialLayerStates::ensure` seeds a never-seen layer
  dirty;
- `sync` re-dirties one whose definition or set size changed while it
  was parked;
- the size census marks every material dirty when the set's sizes
  differ.

A parked set's materials therefore re-evaluate on their first frame as
the active set, and while parked their planes keep compositing on their
own tiles (stage 2's union census).

The real bug was the other way round. `PaintMaterialIo::tiles()` returned
EVERY present tile, so with two sets the active set's material layers
evaluated onto the parked set's tiles too. Those planes were invisible
there (each tile composites its own set's rows) and still spent the plane
budget and the eval time.

## Fix

`tiles()` returns `tiles_of_set(self.set)`, the IO's own set. A
single-set session's tiles are all of them, unchanged.

## Test (GPU, ran)

`a_material_evaluates_onto_its_own_sets_tiles_only`: tiles 1001 and 1002
are assigned to Body and Hilt (1002 materialized by a stroke). The Body
IO lists [1001] and the Hilt IO lists [1002]. **Mutation check:** every
present tile fails the test (`[1001, 1002]`). Gate 1: umber-app 532.
