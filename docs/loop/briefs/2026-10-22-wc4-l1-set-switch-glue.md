# Brief — W-C4 L1: the set-switch glue extraction (main.rs:2501+)

Written 2026-10-22 12:2x by local-7de7d3ed (charter cycle, host-free) so
the kill executes from this file instead of re-deriving. Source: the
W-C4 plan's L1 lane (`docs/loop/w-c4-post-recovery-plan.md`, §5 tail
item 4) + C2-9's queue row ("a headless variant needs the set-switch
glue extracted to a lib fn"). Anchors derived from the WORKTREE state
2026-10-22 — which carries the parked b10 bytes in main.rs/viewport.rs;
the W-C3a close commits exactly those bytes, so these anchors are the
post-close truth UNLESS the close fix-forwards. Re-derive every anchor
at dispatch time anyway (the standing rule).

## Timing (the load-bearing decision)

POST-CLOSE only, like the tangent brief: `main.rs` + `viewport.rs`
carry parked b10 bytes until the W-C3a close's row commits them. Order:
w-c3a-close.sh → w-c3b-close.sh → the tangent wiring (the first W-C4
dev kill) → THIS KILL → L2 → L3 → L5 → L6 (the plan's dispatch
recommendation). Running before the close means editing dirty files
another row owns — trespass, not hunting.

## The current implementation (what gets extracted)

`texture_sets_panel_ui` (main.rs:2501, signature: `ui, paint:
Option<&mut PaintState>, stencil: Option<&mut StencilState>,
session_set: &str, channels: &[String], pending: &mut
Option<PendingResize>`) mixes five things; two are non-UI core:

1. **The switch + Amendment A restage** (main.rs:2527-2541): after the
   selectable list mutates `active`, `if active != paint.active_set()`
   → `paint.set_active_set(&active)` → the guarded restage: only when
   `stencil.active && stencil.image.is_some()` →
   `crate::viewport::restage_stencil(s, Some(&mut *paint))`. The guard
   is Amendment A point 4: the new tile's set can carry a
   differently-sized quick mask, and the new tile would otherwise paint
   stencil-ungated; an inactive/imageless stencil holds no gate worth
   re-staging.
2. **The resize staging** (main.rs:2554-2574): the size combo arms
   `PendingResize { set, from, to }` (armed only — the destructive
   change waits for an explicit confirm); the pending group's reconcile:
   `if p.set != active { *pending = None }` (a pending for a different
   set is stale the moment the switch lands).

The panel's other halves (the set list labels, the confirm buttons,
the per-channel rows at main.rs:2584+ with
`channel_override`/`clear_channel_size`/`resize_channel`) are UI and
STAY in the panel fn untouched.

`restage_stencil` itself (viewport.rs:1763, `pub(crate)`) needs NO
signature change — the extracted fn wraps it. Its contract, for the
test's asserts: it stages the gate from `p.active_tile()`'s quick mask
(`target = mask.dimensions()`, bytes via `quick_mask_gate_bytes`), hooks
it into paint through `set_stencil_gate` (which refuses mid-stroke),
and records `stencil.staged_for = Some((tile, w, h))` only on an
adopted hand-off (`true` return) — `None` on an adopted clear.

## The extraction (the kill's shape)

Two small pub fns in a lib-reachable umber-app module (a `set_switch`
module beside the panel code, re-exported through the lib surface the
way B6's `publish_fill`/`set_brush_color` seams are — read lib.rs's
current export list at dispatch and follow it):

1. `switch_active_set(paint: &mut PaintState, stencil: Option<&mut
   StencilState>, new_set: &str) -> bool` — early-out (returns false)
   when `new_set == paint.active_set()`; else `set_active_set` + the
   guarded restage (the guard lives INSIDE the fn so the headless test
   exercises it); returns true.
2. `reconcile_pending_resize(pending: &mut Option<PendingResize>,
   active: &str)` — the stale-pending clear (`p.set != active →
   None`). The arming itself stays in the panel (it reads the combo's
   pick — UI); the reconcile is the non-UI half.

`texture_sets_panel_ui` then calls both; its behavior is BIT-IDENTICAL
(this is a behavior-neutral refactor + a new test, nothing else — any
observed behavior change is a defect, not a judgment call).

## The headless test (C2-9's check, the point of the kill)

`crates/umber-app/tests/` (beside gate0_journey.rs — re-derive the
PaintState/StencilState construction seams from the existing tests at
dispatch; the journey test's fixture pattern is the precedent). Pins:

1. Live stencil (active + image), two sets at different quick-mask
   sizes: switch → `staged_for == Some((tile, w, h))` matching the NEW
   set's active tile's quick-mask dims (Amendment A: the gate follows
   the tile, not the label).
2. Same-set call → false, `staged_for` unchanged.
3. Inactive stencil → true (the set still switches), no restage
   attempted (staged_for unchanged).
4. Imageless stencil → same as 3.
5. `reconcile_pending_resize`: a pending naming a different set
   clears; one naming the active set survives.

The desktop check 3 page (`docs/loop/desktop-checks.md`) keeps its
Result line (the smoke stays the end-to-end instrument); this test is
the headless half that closes the C2-9 check row's named gap.

## Verification ladder

rustfmt --check per touched file (the formatter binary — allowed under
the envelope); the battery's umber-app leg when sanctioned (the new
test compiles + passes there for the first time); cross-review by the
other claw per the standing recipe; no CI claims until the wave push
(the M2 rule). Mark the C2-9 row + the desktop-checks note + the
requirements row in the same commit.

## Non-goals

- NO UI change of any kind (labels, order, combo contents identical).
- NO `restage_stencil` signature or behavior change.
- NO ensure_set move (it is the panel's session-set precondition, not
  part of the switch dance; the headless test constructs its own state).
- NO per-channel row extraction (that surface is L5's prerequisite
  scope, not this kill's).
- NO edit before the close lands — the b10 dirt in main.rs/viewport.rs
  is another row's cargo until its commit releases it.

## Unlocks

L5 (per-set masked bakes' scope glue) per the plan; the C2-9 headless
check row; D1's smoke stops being the only instrument for the
set-switch path.
