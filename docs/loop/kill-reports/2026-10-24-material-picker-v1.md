# Kill report — the material picker v1 (R-A10, the second half on the channel target's shoulders)

Executed 2026-10-24 22:1x–22:4x by local-7de7d3ed, opened by
Bessa's "continue working autonomously" (the queue's next: the
picker's second half, unblocked by 0f55ebd's channel target).

## The shape the derivation chose

The row's census wording was "pick all channels at cursor → fill
layer"; the derivation's honest v1 (named in the channel-target
row): pick the ACTIVE channel's composite value at the cursor and
apply it to the selected fill layer's Solid — the all-channels
mode is the follow-up. The gesture had to be DISTINCT from the
two existing modifier conventions: the inspector's Shift-hover
(read-only) and the clone tool's Alt-drag (the source set) — so
the pick is **Ctrl+Alt+click**, and the status bar's modifier
hint context names it.

## What landed

### uv_view.rs

- **`pick_requested: Option<(u16, [f32; 2])>`** — the pending
  pick, handed to the shell ONCE (the take_hover_uv convention:
  the view records, the shell owns the read + the apply).
- **`take_pick()`** — the shell's drain.
- **The capture** — at the pointer_down seam (the stroke gate): a
  bare click with BOTH modifiers records `(active_tile, uv)` and
  never starts a stroke (the same exclusion the clone source's
  Alt-drag carries). Distinct from Shift-hover (inspect) and
  plain Alt-drag (clone).

### main.rs (the Panel::UvView arm)

- **The apply** — the shell drains the pick: `read_composite(tile,
  active_channel)` → the texel at the cursor (the inspector's OWN
  mirrored-row convention: UV y grows upward, the buffer's row 0
  is the texture's TOP row) → `byte / 255` (the linear domain —
  the composite's u8 IS the working space) → the PRIMARY selected
  layer: if it is a Fill layer, its Solid becomes the picked
  color (channel target + projection + mask untouched) through
  the SAME journalable `SetFill` the panel uses; a non-fill
  selection logs and no-ops (the v1's honest scope).

### status_bar.rs + i18n

- **The hint** — the PaintModifier context gains
  `Ctrl+Alt+LMB → pick material into fill layer`; both catalogs
  carry the pair (the pt-BR: "amostrar material para a camada de
  preenchimento").

## The derivation's own misses (named)

1. The capture block landed BEFORE the `let alt` binding it
   referenced — the block now reads the modifiers itself.
2. `paint.as_deref()` inside the capture — the enclosing `if let
   Some(paint)` had already consumed the Option; `paint` IS the
   `&mut PaintState` there — the direct `paint.active_tile()`.

## The named follow-ups

- The all-channels mode (pick every channel → N fill layers or
  the channel-mapped fill set) — the row's census wording, now a
  real extension on the channel target.
- The pick's e2e kittest (the gesture → the set_fill) — the same
  class as the browser's named kittest.
- The pick's toast/feedback (a visible confirmation beyond the
  log line).
