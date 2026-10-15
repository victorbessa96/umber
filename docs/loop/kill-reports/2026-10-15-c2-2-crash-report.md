# Kill report: C2-2 crash reporter (module landed, wiring pending)

Date: 2026-10-15. Author: dragon. Lock: `c2-2-crash-report` over
`crates/umber-app/src/crash.rs` and this report. Source:
`docs/loop/audit-2026-10-15-c2.md` C2-2; ROADMAP `wave-crash-reporter`.

## Status: written + ad-hoc verified, NOT registered

`main.rs` sits under the sibling session's live `g1-axis-gizmo` lock
(re-registered with main.rs in it, and actively edited at 12:47). The
module is committed unregistered, the curve-widget precedent. Wiring,
once the lock frees, is three edits:

```rust
mod crash;                                         // main.rs mod list
crash::install(gpu_startup::state_dir());          // first line of main()
// + on the first frame: for each crash::pending(&gpu_startup::state_dir())
//   post a sticky notice naming the report path, then crash::acknowledge.
```

## Built (`crash.rs`)

- `install(state_dir)` chains a `panic::set_hook` in front of the default
  hook, which still prints. One report per panic goes to
  `<state>/crashes/crash-<unix>-<pid>.txt`, holding the message, the
  location, the thread name, a forced backtrace, the version and the
  commit (the build.rs `UMBER_GIT_HASH`, the same facts as Help ▸ About).
  The hook resolves its directory before any panic, never panics itself,
  and on a failed write prints to stderr and carries on.
- `pending` returns unseen reports, oldest first; other files are
  ignored. `acknowledge` renames a report to `.seen`, so it is offered
  once and kept on disk.
- Nothing from the document or the environment goes into a report, so
  one is safe to paste into a public issue.

## Tests (5, verified in a remote copy with `mod crash;` added)

- `format_report`'s facts, with and without a location.
- Payload text for `&str`, `String` and other payload types.
- write → pending → acknowledge round trip.
- **A real panic through the real hook, in a CHILD PROCESS.** The hook is
  process-global and libtest runs tests in parallel, so installing it
  in-process would also catch sibling tests' deliberate panics. The
  parent re-runs this test binary filtered to `crash_hook_child`; the
  child installs the hook, panics on a named thread and exits 3. The
  parent then asserts on exactly one report (message, thread, location)
  and on the stderr line.
- After it, the full umber-app suite ran in parallel (415 passed) and
  wrote no stray report to the real crash dir.
- clippy: clean except dead-code on the unregistered module's public
  functions, which is expected and goes away with the wiring.

## Deferred

- The wiring above (after the lock).
- Attaching the last journal ops to a report. The C2-3 pixel autosave
  will own a per-session journal, and a report can name it then.
