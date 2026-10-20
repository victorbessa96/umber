# Kill report — the W-C4 research plan lane (w-c4-research-plan)

Date: 2026-10-20. Session: local-10e6ed18 (main CLI). Dragon
authored. Doc/research lane, battery-exempt (no cargo by the
2026-10-20 code steer; centralhost dark, FREEZE-4 day 4).

## Claim

Lock `w-c4-research-plan` (3 paths at claim, 4 at land: the plan +
the two artifacts + this report). The STATE block was rewritten by
wake-13522 at 15:1x (kittest-egui-curation) — this lane read the
fresh block BEFORE claiming and composes on top of it. No
intersections: all four paths were new or nonexistent.

## Residue found and fixed on the way in

The pre-lane ground-truth census caught a real residue: a178913
(the commit-gate lane) committed the hook + its report but MISSED
`.dragon/dcommit.py` — HEAD's copy ran the commit subprocess with a
bare environment, so a fresh clone would have its installed hook
refuse every dcommit. Committed as 05ed71a (the stamp itself, the
first commit to ride the stamp it lands). The stylus dirt was
accounted as S1's pinned parked bytes (manifest-verified, zero
drift at the 9/9 dry-run).

## What landed

1. `docs/loop/w-c4-post-recovery-plan.md` — the §5 tail mapped into
   lanes L1-L7 with exact seams (main.rs:2501+ set-switch glue,
   umber-paint paint_state.rs:1653 + viewport.rs:1423-1427 dab
   density, main.rs:532 IdMask pre-warm, the hook design, masked
   bakes after L1, the unified timeline, G1's delegate), lock
   status per lane, a dispatch-order recommendation, and the
   partner-gated set restated.
2. `docs/claw-artifacts/stylus/eframe-0.36-rwh-surface.md` — the
   vendored eframe 0.36.2 RWH surface: HasWindowHandle on BOTH
   CreationContext and Frame (epi.rs:99-105, :695-701), the kittest
   NotSupported mocks (:134, :730) that make the hook headless-safe
   by construction, the Win32 variant's hwnd extraction, and the
   install-point design (one-shot at App::new, AtomicBool-guarded
   per-frame fallback).
3. `docs/claw-artifacts/stylus/windows-0.62-subclass-surface.md` —
   the vendored subclass API: SetWindowSubclass (Shell/mod.rs:4318),
   DefSubclassProc (:226), RemoveWindowSubclass (:2352), SUBCLASSPROC
   (:55755), the ALWAYS-DefSubclassProc contract (winit's Touch
   path survives), the dwrefdata state hand-off, the one-line
   manifest feature add (Win32_UI_Shell at stylus/Cargo.toml:19),
   and the already-enabled pointer-side symbols
   (GetPointerPenInfo, Pointer/mod.rs:95) the parked decode uses.

The §5 item 2 "API-UNVERIFIED" clause is CLOSED: both surfaces are
vendored with line-level evidence, and the hook lane can dispatch
post-recovery without a research pass.

## Method (and why every cite is trusted)

- The lock interface itself taught a lesson mid-lane: the first
  registration attempt used a wrong subcommand shape ("register" —
  a silent no-op, rc=0, no lock file) and only the post-call
  registry census caught it; the correct form is
  `kill-locks.sh add <name> <files...>`. The census-then-verify
  discipline held; no work started under the phantom lock.
- Every API cite was read from the vendored registry bytes
  (eframe-0.36.2, windows-0.62.2) before writing — the house
  never-guess rule. The two "API-UNVERIFIED" items of the W-C3a
  plan resolved to concrete signatures, not paraphrases.
- The seam line numbers (main.rs, viewport.rs, paint_state.rs,
  winink.rs, pen_flags.rs, Cargo.toml) were read from the live tree
  in the same session, several against the parked (uncommitted)
  bytes — they are accurate for the parked fleet and must be
  re-verified at recovery if any parked byte moves.

## Unverified / judgment calls

- The plan's lane ORDER (L1 → L2 → L3 → L5 → L6, L4 free-running)
  is a recommendation built on the dependency chain (L5 needs L1;
  L2/L3 need the b10/P1 files unlocked), not a hard graph — the
  dispatching session may reorder within those constraints.
- The hook design's AtomicBool/`Box`-leak shapes are design prose,
  not code; the lane that dispatches L4 owns the real state
  architecture and may replace them, provided the
  ALWAYS-DefSubclassProc contract and the kittest NotSupported
  degrade are preserved.
- SetWindowSubclass's "idempotent per (hwnd, uidsubclass), returns
  FALSE on a same-id re-install" is stated from the comctl32 API's
  documented contract, not from a vendored doc comment read — the
  vendored binding is a raw FFI decl with no commentary. Low risk,
  but the dispatching lane should not rely on the FALSE shape; the
  AtomicBool guard is the real defense.

## Deferred

- No new queue rows until the lanes dispatch (the recovery's docs
  row owns the queue's committed state).
- L4's real-pen leg is partner-gated (the Windows box + the pen).
- The plan doc stays DRAFT until W-C4 opens at HOST_BACK.
