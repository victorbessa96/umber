# Kill report — the set-switch glue extraction (W-C4 L1, C2-9's headless half)

Executed 2026-10-24 04:0x–04:3x by local-7de7d3ed (the restarted
loop's first wake), from
`docs/loop/briefs/2026-10-22-wc4-l1-set-switch-glue.md` verbatim, every
anchor re-derived at dispatch per the standing rule (all held — the
panel's blocks matched the brief's map line for line).

## What landed

- `crates/umber-app/src/set_switch.rs` (NEW, a lib module): the
  texture-sets panel's two non-UI cores —
  `switch_active_set(paint, stencil, new_set) -> bool` (the early-out
  becomes the fn's; the switch plus Amendment A point 4's guarded
  re-stage, the guard INSIDE so the headless test exercises it) and
  `reconcile_pending_resize(pending, active)` (the stale-pending
  clear; the ARMING stays in the panel — it reads the combo, UI).
- `restage_stencil` MOVED from the bin's `viewport.rs` (was
  `pub(crate)` in a bin-only module — no lib caller could ever reach
  it; the move IS the kill) — name and signature unchanged, the
  record-after-hook contract doc carried with it. Viewport's four
  internal callers repointed by one import line; the panel's call
  died with the extraction (the glue fn owns the guard now).
- `PendingResize` moved from `main.rs` to the module, fields pub (the
  headless test and a headless variant construct it; the panel stays
  the only in-tree constructor).
- The stencil session module moved bin→lib: `pub mod stencil;` in
  lib.rs, the bin re-exporting it at its crate root
  (`pub use umber_app::stencil;`) — every existing `crate::stencil::`
  path in the shell resolves unchanged (zero viewport edits for the
  move; the lib module `display_panel`'s adapter pattern — field
  borrows instead of naming the type — was the WRONG precedent for
  this kill: the test pins the typed contract itself).
- `texture_sets_panel_ui` calls both glue fns; its behavior is
  bit-identical (the brief's rule: any observed change is a defect,
  not a judgment call).

## The headless test (the kill's point, C2-9's named gap)

`crates/umber-app/tests/set_switch.rs` — the five pins, all green on
first strike through the real adapter (3.18s):

1. Live stencil (active + a 2×2 all-passing image), a switch to a set
   with a different quick-mask size → `staged_for ==
   Some((tile, mw, mh))` with the dims READ at runtime from
   `quick_mask_for_tile` — the pin is the MATCH (Amendment A: the gate
   follows the tile, not the label).
2. Same-set call → false, the record untouched.
3. Inactive stencil → true (the set still switches), no re-stage.
4. Imageless stencil → same as 3.
5. The reconcile: a pending for a different set clears; one for the
   active set survives.

## Evidence ladder

- `cargo check -p umber-app --all-targets` clean (1m11s, the whole
  move resolving at once — the lib's two new modules, the bin's
  re-exports, the panel's calls, viewport's cession).
- The headless test green on first strike.
- The battery's umber-app leg (the new test + the journey suite).

## Docs marked in the same kill

- `docs/loop/w-c4-post-recovery-plan.md`: the L1 item CLOSED with the
  kill's shape.
- `docs/loop/desktop-checks.md`: check 3's headless-half note — the
  page KEEPS its Result line (the smoke stays the end-to-end
  instrument; the headless test is the other half).
- No requirements.md row exists for the glue itself (the C2-9 audit
  row lives in the ROADMAP's already-✓ line; the closure is the
  plan's row above).

## Non-goals held

- No UI change of any kind (labels, order, combo contents identical).
- No restage_stencil signature change (the move is the whole change).
- No viewport.rs logic edits (one import line; four call sites
  untouched).
