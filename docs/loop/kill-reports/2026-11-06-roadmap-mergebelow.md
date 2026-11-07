# Kill report — ROADMAP merge-below-top status update (docs kill, queue row 255)

- Date: 2026-11-06 ~00:5x -03. Session: local-1625-rz6f2a (Razul).
  Kill-lock: `razul-roadmap-mergebelow` (docs/ROADMAP.md, STATE.md, this
  report).
- The merge-below-top debt (retouch.rs:34's judgment call) named a
  redo-top accessor as its blocker. Both primitives have since landed:
  `peek_redo`/`redo_len` in `03181ec` (row 252) and
  `take_redo`/`restore_redo` in `1347e13` (row 254, already exercised by the
  split_redo lift). ROADMAP now says so, and names the real remaining
  blocker: a splice primitive in `umber_core::undo` that rewrites an entry
  below the top WITHOUT re-applying the entries above (`push` applies its
  command, so pop-merge-repush would double-apply).
- Docs-only commit: no fresh battery per the standing 8af2058 receipt rule.
- Queue row 255 + COORDINATION line DEFERRED (sibling lock
  `razul-bundled-assets` still live at census: 1 umber proc, its ~13 dirty
  files untouched).
