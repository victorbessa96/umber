# Kill report: duplicate with content, the engine half (C2-8 follow-up)

Date: 2026-10-15. Author: dragon. Lock: `duplicate-planes` over
`crates/umber-paint/src/paint_state.rs`, `crates/umber-app/src/document.rs`
and this report. C2-8's duplicate copied settings only.

## Built

- `PaintState::copy_owner_planes(from, to)` reads back each of `from`'s
  `(tile, channel)` planes and publishes it whole into `to` through
  `publish_plane`, so budget accounting, cache dirt and the revision bump
  all apply. It returns the planes copied. A budget refusal propagates,
  and the planes copied before it stay copied (exact, onto a fresh
  layer). Mask planes are not copied, because they belong to a separate
  owner key.
- `LayersUiOutcome::duplicated = Some((source, copy))`. The panel reports
  the pair, and the shell does the plane copy, since the panel owns no
  paint session.

## Tests

`copy_owner_planes_duplicates_every_plane_exactly` (GPU, ran):
- baseColor (a byte pattern) and roughness copy byte-exact;
- the copy's revision bumps and the source is untouched;
- an owner with no planes copies nothing and materializes nothing.

Gate 1: umber-paint 161, umber-app 483.

## Pending: one shell line (main.rs is under the sibling's g1-axis-gizmo lock)

```rust
if let Some((source, copy)) = outcome.duplicated {
    if let Some(paint) = self.paint.as_deref_mut() {
        if let Err(refusal) = paint.copy_owner_planes(
            paint_state::PlaneOwner::Layer(source),
            paint_state::PlaneOwner::Layer(copy),
        ) {
            log::warn!("duplicate: plane copy refused: {refusal:?}");
        }
    }
}
```

Goes right after the `outcome.select` block in the LayerStack tab.
