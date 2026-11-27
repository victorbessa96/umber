# CLI/control security and lifecycle hardening

- Lock: `razul-cli-control-security`.
- Brief: `90ff32f4` (`docs/cli-control-security`, commit message `docs(cli): control security hardening brief`).
- Implementation: OpenCode completed the first three areas before its free-usage wall; the dragon completed and reviewed the remainder. No claw test claims were accepted without local execution.

## Original six findings and fixes

1. **Workspace confinement bypass.** `confine_step` is variant-aware and covers the actual `AgentStep` path fields, including transfer low/high reads and asset file/library writes. An implicit per-user asset library must resolve inside the write root; there is no silent unconfined write mode.
2. **Direct MCP envelope mismatch.** Direct passthrough tools now dispatch the full `{name: body}` object. A real wire regression drives all six affected tools.
3. **Outer success on agent error.** `serve` propagates inner step and batch failures to the top-level envelope while retaining the detailed result.
4. **Long-job caller timeout regression.** `command.run` uses the server's 30-minute window plus 30 seconds of grace; other calls retain 30 seconds. `control_call_with_timeout` makes socket-deadline behavior directly testable.
5. **Predictable token-file clobbering and permissions.** Tokens use a random private create-new sibling, regular-file/symlink checks, rename replacement without delete-first, and 0600 enforcement on every write.
6. **Dead MCP child left bridge busy and client hanging.** Both pumps share cleanup, reaping the child and shutting down the owning client stream.

## Independent review round

Review `deleg_1a2d0706` returned five further findings. Dispositions:

- **Stale cleanup could destroy a newer session:** cleanup is generation-scoped. A stale pump reaps only its own child and cannot clear or shut down a newer client's stream.
- **Windows delete-first replacement could lose the old token:** removed. Rust's documented Windows rename path replaces existing files without deleting the destination first.
- **POSIX-only child fixture:** tests spawn `current_exe --version`, portable across supported hosts.
- **Default-library test failed before its target code:** it now creates the source and tests both specific outside-root refusal and allowed default-library insertion under a hermetic per-user directory.
- **50 ms test did not prove the long timeout:** the new socket test proves a short injected deadline fails against a 100 ms delayed server and a one-second deadline accepts the same delayed response.

## Verification

- Core control: **9 passed, 0 failed**; full core: **359 passed, 0 failed**.
- App binary control: **12 passed, 0 failed**, including the stale-generation interleaving.
- Workspace confinement: **8 passed, 0 failed**.
- Real socket deadline: **1 passed, 0 failed**.
- Full CLI suite: green, including direct MCP envelopes and integration tests.
- Full app: **1,289 passed, 0 failed**.
- Strict clippy across all touched crates and targets: clean. Layers, scorecard, parity, rustfmt, and `git diff --check`: clean.
- Desktop-requested ad-hoc verifier: **8/8 focused regressions passed**, including bridge cleanup, default-library confinement, real socket timeout, and token safety; temporary script deleted. This is separate from suite evidence.
- Final independent review `deleg_240b61e8-2`: **SHIP**, all six original areas and all five review findings fixed, with test-honesty analysis accepted.
- Commit/push/current-tip CI: local commit next; origin and CI are not yet claimed.

## Non-goals

No public agent-schema change, no new unconfined mode, no release packaging, no upstream PR, no centralhost work.
