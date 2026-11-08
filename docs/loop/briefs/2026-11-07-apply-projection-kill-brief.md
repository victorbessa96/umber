# Kill brief — row 279: `PaintState::apply_projection` (R-A2 decal projector)

You are a claw in the Umber workspace (`/home/bessa/Projects/umber`, branch
master). You know nothing except this brief. READ the plan first:
`docs/specs/projection-fill-plan.md` (YOUR row is 279). The app-side half
(row 278) produces the input; the contract between you is FIXED below.

## Your file surface (touch NOTHING else)

- `crates/umber-paint/src/paint_state.rs` ONLY (it is huge — read the anchors
  below before writing anything).

Do NOT run cargo/clippy/tests. Do NOT commit or touch git. No new deps.

## The contract (from the plan, row 279)

```rust
pub fn apply_projection(
    &mut self,
    tile: u16,
    w: u32,
    h: u32,
    rgba: &[u8],   // w*h*4, straight alpha, the rasterized decal
    opacity: f32,  // clamped 0..=1
) -> bool  // true when at least one texel was written
```

Semantics:
1. Refuse (`false`, no writes, no history) while `self.stroking`.
2. Refuse on `w == 0 || h == 0 || rgba.len() != (w*h*4) as usize`.
3. Clamp `opacity` to `0.0..=1.0`. `opacity == 0.0` -> `false`, no history.
4. ONE undo step, exactly like a stroke: snapshot the active layer's tile
   before, write, then record the SAME history entry a stroke records
   (read `end_stroke` at `paint_state.rs:4521` and the before/after snapshot
   machinery it uses — reuse that path; do not invent a second one).
5. Per texel (x, y): gate weight `g = gate_plane(tile)[y*w + x]` (the fn at
   `:5840` already folds quick-mask x cavity x THE STENCIL GATE — so the
   existing stencil gate masks the projection for free; that is the plan's
   "one screen-space gate, two consumers"). Skip when `g <= 0.0`.
   Effective alpha = `rgba[4i+3] * opacity * g` (float math, clamp).
6. Blend the source color over the active layer's current texel in the
   active paint channel(s), using the SAME per-texel blend the dab write
   path uses (find it via `stage_stamps` at `:5413` -> `DabAdapter` -> the
   write loop; reuse that blend function with a flat full-coverage dab —
   NOT the brush's soft falloff).
7. v1 cut: NO seam expansion, NO symmetry mirror of the projection (name it
   in the doc comment as a follow-up). Write the tile as given.
8. `mark_dirty_rect` (`:3247`) the full tile + queue the GPU re-upload the
   stroke path queues (find where staged dabs get uploaded and do the same).
9. Return whether any texel actually changed.
10. `PaintProjection` gains NO new variant (the plan §1) — do not touch the
    enum at `:456`.

## Anchors to read first

- `gate_plane` `:5840`, `set_stencil_gate` `:5772`, `stencil_gate` field `:1366`
- `begin_stroke` `:1930`, `end_stroke` `:4521`, `stage_stamps` `:5413`
- `mark_dirty_rect` `:3247`
- the tests at `:6450` (`stencil_gate_composes_into_the_gate_plane...`) show
  how tests build a `PaintState` and read texels back — mirror that.

## Pins (append to the existing tests module in the same file)

- writes land in the active layer's tile and read back blended;
- ONE undo step reverts the whole projection (`undo_stroke` `:2870` or the
  undo API the tests use);
- the stencil gate blocks gated texels (`set_stencil_gate` a half-open gate,
  apply, assert the blocked half unchanged and the open half written);
- `opacity = 0.5` halves the written alpha versus `opacity = 1.0`;
- mid-stroke refusal: `begin_stroke(uv)` then `apply_projection(...)` returns
  false and writes nothing;
- bad dims (`rgba.len()` mismatch) -> false;
- all-zero-alpha source -> false and NO history entry queued.

## Doc comment

Name row 279 of `docs/specs/projection-fill-plan.md`; the v1 cut line (no
seam/symmetry expansion, no depth test — the caller rasterizes); the stencil
gate composes through `gate_plane`.

Write the code, keep it compiling by careful reading (you cannot run cargo),
and stop. Report what you added and where.
