# Kill report — 2026-11-07 CI red fix: the GPU session's display question is the caller's (row 271)

- **Authorship:** Razul (dragon). Lock `razul-gpu-startup-ci` — `crates/umber-app/src/gpu_startup.rs`.
- **Trigger:** the row 269 push's release-gate CI run 42419285457 (tip `c87ebf3c`) went RED on BOTH runners in the Tests step: `gpu_startup::tests::session_confirms_on_the_third_frame_and_persists_a_fallback` panicked at `gpu_startup.rs:1564` with `left: None, right: Some(Gl)`. The project-watch cron recorded it as the WATCH row 270.

## What was wrong (and what was NOT)

- **Not the svg wave.** `git log da3ed59..HEAD -- crates/umber-app/src/gpu_startup.rs` names exactly one commit: `59779065` ("fix(app): headless launches never persist the GPU fallback", row 249, 2026-11-06 20:21). That commit is the regression, and it landed 21h before the first CI run since CI became a release gate (2026-11-06) — the latent red was simply never run on a runner.
- **The mechanism:** `Session::new` read the AMBIENT environment (`has_display()` → `WAYLAND_DISPLAY`/`WAYLAND_SOCKET`/`DISPLAY`) to decide whether a successful fallback may be persisted. A CI runner has no display, so `persist` was `None` and nothing was written after the confirm frame — the test's `read_persisted(&dir) == Some(Gl)` failed. On a desktop (this machine) the same test read the env the other way and passed. The test asserted a machine-dependent outcome; the CI runner is the machine that exposes it.

## Built

- `Session::new_with_display(dir, guard, decision, has_display)` — ONE constructor, the display question answered by the caller. The ambient read is gone from the session.
- The production call site (`gpu_startup.rs:900`) passes `has_display()`, so **production behavior is unchanged**: a headless launch still never persists its fallback (installers gap #5, row 249).
- Every test call site names the branch it means (`..., true` for a display-ful launch). The shorthand `Session::new` was REMOVED, not kept: in the bin target it had no caller (dead-code error under the crate's deny list), and two constructors would invite the ambient read back.
- New pin `a_headless_session_never_persists_its_fallback` — the session-level twin of the decision-level pin `a_headless_launch_never_persists_its_fallback`: a headless session that runs the full confirm frame persists nothing.
- Module docs and the constructor docs name the reason (CI RED on 2026-11-07, run 42419285457) so the next reader does not re-introduce an environment read inside a constructor.

## Judgment calls

- **One constructor, not two.** A `Session::new` shorthand for the display-ful case would have been tidier at the call sites but is dead code in the bin target and leaves two ways to build a session — one of which is the ambient-read trap this kill removes.
- **The fix is in the test seam, not the test.** The failing ASSERTION was right (a display-ful launch persists its fallback after the confirm frame); the CONSTRUCTOR it exercised was machine-dependent. Weakening the assertion would have hidden the row 249 behaviour on real runners.

## Gates (dragon, 2026-11-07)

- rustfmt --edition 2021 --check on gpu_startup.rs: CLEAN.
- **The CI condition, reproduced locally:** `env -u DISPLAY -u WAYLAND_DISPLAY -u WAYLAND_SOCKET cargo test -p umber-app --bin umber gpu_startup::tests` → **28 passed / 0 failed** — the exact no-display environment that failed on both runners.
- `cargo clippy -p umber-app --all-targets -- -D warnings`: **0 errors**, exit 0.
- `cargo test -p umber-app --all-targets`: **green** — lib 256/0, bin 757/0 (756 + the new pin), the integration suites all ok (the 28-test gpu_startup suite included).
- CI verdict on the new tip: dispatched after the push (release-gate run on the fix's tip).

## Deferred

- The row 249 behavior itself is unchanged and stays as designed; the sibling's row 249 kill report (docs/loop/kill-reports/2026-11-06-headless-no-persist.md) remains the record of that decision.
- The cron's WATCH row 270 stays as the watcher's record of the red; this kill is the fix record (row 271).
