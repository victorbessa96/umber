# Kill report: per-channel tile pool design closure (2026-10-18)

Dragon-authored host-free by the Umber /loop session "Work on Umber
project coordination #3" (local-d3deea1d3c7631cc80f8782e73c37fe384c4c50d1248d89def19e4801b4c9a45, under Bessa's live steer
"do all the work that you can locally, keep developing the project").
Kill-lock `pcpool-design-review`, holding exactly one file:
`docs/specs/per-channel-tile-pool-design.md`.

Context: the design doc was DRAFT PRE-REVIEW and named its own next
step ("the implementation brief derives from this doc") — but the
review found the implementation outran the review: the design
LANDED 2026-10-12 (dc5c2c9), and the tree has since evolved a
generation beyond it (per-channel resolution; the per-layer pool's
owner dimension). The doc was never closed and its header still
claimed to BLOCK the heightnormal wiring that shipped weeks of kills
ago. The honest kill is a closure, not a brief.

## Built (the doc's first review, as its tail closure)

- Status line: DRAFT → LANDED/CLOSED with a dated pointer to the
  closure section (the doc-contract status bump; the closure section
  itself carries the full record).
- The closure section, every claim verified against the live tree
  the day it was written:
  - landed-as-designed: `TileTargets` verbatim (now
    `umber-paint/src/paint_state.rs:572` — the doc's `:408` anchor is
    pre-extraction), `active_channel`/`set_active_channel`,
    `ensure_channel`, `read_channel_tile`;
  - the named blockers closed: the binding seam lives
    (`viewport.rs:1915-1942`, `bind_paint_target` →
    `set_height_texture` — the heightnormal kill's unreachable call);
  - outgrown by successors, both cited: wave-perchanres un-deferred
    this doc's v1 non-goal (per-channel resolution, landed same day);
    the per-layer pool's `(owner, tile, channel)` planes superseded
    the doc's session-only channel model (stages 1b + 4, 10-14);
  - residual authority named (channel identity rules, the v2→v3
    migration contract — pinned live by
    `v2_flat_manifest_loads_all_base_color` — the tile-major
    rationale);
  - corrections: none of the five test gates falsified; the only
    stale text positional (pre-extraction paths) + the status line.
- Attribution preserved from the tilepool finish kill's report
  (claw-authored ~581 lines, died on budget, dragon-landed with 3
  borrow fixes) — the closure cites it rather than re-deriving it.

## Judgment calls

1. Closure over brief: the doc promised an implementation brief; the
   implementation exists and its successors own the living state — a
   brief for already-built work would be fiction. The closure records
   verified truth instead.
2. Status-line edit + tail append, nothing between: the doc's
   historical sections stay as written (the problem statement's
   BLOCKS paragraph included) — the closure names their resolution;
   rewriting history would falsify the doc's own record.
3. Doc-only kill: no crates touched, no battery scope; rides the
   recovery's commit sequence via its manifest (its own tiny row or
   the docs fold — the recovery script's owner's call; the extension
   owe is recorded in the coordination note per the script's own
   header instruction for kills pinned after its version).

## Unverified

- Nothing compile-shaped: every anchor in the closure was grep-verified
  against the live tree this session (`TileTargets`, `active_channel`,
  the viewport seam, the migration test name, `planes_present`, the
  91-test count). A doc-only kill has no battery legs.

## Gates run (host-free, this session)

- `scripts/verify-kill.sh`: PASS (trap scan, doc-attachment, lock
  audit — the `pcpool-design-review` lock live).
- Closure-claims census: the ad-hoc verifier
  (`hermes-verify-pcpool-closure.py`, standing evidence) re-checks
  every live anchor the closure cites plus the manifest pins.

State: VERIFIED (host-free by nature), IN FLIGHT — commits at the
recovery via its manifest; lock release at that commit.
