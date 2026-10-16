# Kill report (UNRUN, host-gated): crash reports list the last 10 operations

Date: 2026-10-15. Author: dragon. Lock: `crash-recent-ops`
(`crash.rs`, this report). ROADMAP `wave-crash-reporter`: "writes the
last 10 journal ops + the frame state to a user-readable file". The
report file and the next-launch notice already shipped (C2-2); this kill
adds the operations.

## Built (`crash.rs`)

- `note_op(&'static str)`: a process-global ring of the last
  `RECENT_OPS` = 10 `(unix secs, label)` pairs.
  - The `&'static str` signature means user text cannot reach a report,
    which keeps it safe to paste publicly (the module's existing promise).
  - A poisoned lock is recovered (the Vec is still valid).
- The hook appends "last operations (unix time, oldest first):" using
  `try_lock`, so a panic while the ring is held reads "unavailable"
  instead of deadlocking the dying process.
- `push_capped` / `ring_text` are the pure halves, tested on a local ring
  (the global one is shared with parallel tests).
- The existing child-process hook test now records an op and asserts it
  is in the written report.

## Also built (`history_panel.rs`, added to the lock)

`HistoryKind::crash_label(self) -> &'static str`: fixed English names for
all 20 kinds (an exhaustive `match`, so a new kind is a compile error
there). A test requires them distinct and plain lowercase ASCII.

## Not done: the callers

Nothing calls `note_op` yet, so a report lists "(none recorded)" until
it is wired. The call sites are:
- `Document::run` / `record_session_event` (document.rs, locked by
  `mask-compose-perf`): one `crate::crash::note_op(kind.crash_label())`;
- the paint-stroke commit, `PaintState` via main.rs (locked by
  `g5-workspace-persist`).

Both are one line each once the locks free. "Frame state" (the ROADMAP's
other half) is the shell's per-frame tool/panel state: `main.rs` again.
It is deferred with the callers.

## Unverified

No compiler. Static: `Mutex::new` is `const` (stable since 1.63), so the
static compiles; the ring logic is pinned by the local-ring test.

## VERIFIED (2026-10-16, host battery GREEN)

`note_op` is wired: `document.rs`'s `record_session_event` calls
`crate::crash::note_op(kind.crash_label())` (fixed English labels in
`history_panel.rs`), and `crash.rs` is on the lib surface
(`lib.rs`) so the lib target reaches it. Gate 1: battery GREEN.
