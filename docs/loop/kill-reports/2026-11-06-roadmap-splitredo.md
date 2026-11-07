# Kill report — ROADMAP debt-list truth pass (split-redo status)

- Date: 2026-11-06 ~21:2x -03. Session: local-1625-rz6f2a (Razul). Lock:
  `razul-roadmap-splitredo` (docs/ROADMAP.md, STATE.md, this report).
- Why: ROADMAP lines 210/175 still read "Split-redo — OPEN", but the engine
  landed in `850b839` ("paint: undo split-redo for merged entries (row 167
  follow-up)", verified: retouch.rs +535, paint_state.rs +26) and
  `PaintState::split_redo_stroke` is the engine entry point. Only the
  app-side History-panel "split" affordance is still open, and it is
  blocked on document.rs/main.rs (sibling lock `razul-bundled-assets`).
- Change: ROADMAP debt list now says ENGINE LANDED 2026-11-06 (850b839,
  rows 167/242) with the wiring remainder named; the wave-table row 175
  updated the same way. Merge-below-top stays OPEN with its precondition
  named (a redo-top accessor in `umber_core::undo`, retouch.rs:658's note;
  a below-top rewrite would invalidate the redo stack).
- Verification: docs-only commit — per the standing receipt rule (8af2058)
  no fresh battery at docs-only commits. SHA of 850b839 re-verified with
  `git log -S split_redo_stroke` before writing the claim.
- Queue row for this docs kill DEFERRED with the sibling lock (queue.md
  held by `razul-bundled-assets`).
