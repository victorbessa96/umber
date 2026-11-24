# Control channel + bridge live smoke

- Date: 2026-11-24.
- Author: Razul (dragon), session `local-f150e2eb08b49cfc70e9c18de051af45f6e45accdfc10fe757ff67a5476d841d`.
- Brief: `docs/loop/briefs/2026-11-24-umber-control-smoke.md` (`5961e6d8eef3`).
- Claim: `fcd0f12d51bb`, queue row 596.
- Status: docs and live verification only; no app source changed.
- Kill lock: `razul-control-smoke`.

## Built and verified

- `cargo build -p umber-app -p umber-cli` completed successfully at the current HEAD before the smoke.
- Wrong-token control connection returned exactly `{"error":"unauthorized","id":1,"ok":false}` and the next read returned EOF.
- Authed control returned `ping`, 11 methods, and `ui.inspect` with `panels`, `layers`, and `tool` present.
- `viewport.screenshot` decoded as a 1296×729 PNG with 3,056 distinct colors; IHDR matched the reply dimensions and the image was not one uniform color.
- Bridge `initialize` returned protocol `2025-06-18` and `serverInfo` `umber-cli 0.1.0`. Its raw response line matched a separately launched direct `umber-cli mcp` child byte-for-byte.
- The app exited on SIGTERM (`rc=-15`), the scratch config/state/cache/TMPDIR tree was removed, and the probe script was deleted. No token values were logged.

## Failure and correction

The first launch redirected `XDG_RUNTIME_DIR` into scratch and failed before the control channel with `WaylandError(Connection(NoCompositor))`. That environment choice detached the app from the live Wayland socket, so no smoke claim was made from it. The rerun inherited the real `/run/user/1000/wayland-1` runtime directory while keeping config/data/state/cache/TMPDIR isolated; it passed all checks.

## Verification

- Live probe: `ADHOC-VERIFY: PASS (control channel + bridge live smoke)`.
- Exact wire evidence: wrong-token unauthorized+EOF; authed `ping`/11 methods/`ui.inspect`; PNG 1296×729, 3,056 colors; bridge/direct-child raw `initialize` line identical.
- Cleanup evidence: `APP_CLEANUP rc=-15`, `SCRATCH_REMOVED=True`, `HERMES_VERIFY_SCRIPT_REMOVED=yes`.
- This is live ad-hoc verification against the current debug build, not a fresh full-suite run. `scripts/safe-push.py` remains the origin gate for the docs commit.

## Judgment calls

- Checks 1–4 were not claimed. They still require Bessa's desktop interaction, and check 4 remains Windows real-pen gated.
- The probe used the live display path rather than manufacturing a headless compositor. Config and state were still isolated from the real user profile.
- No production source, schema, dependency, or release artifact changed. Only `desktop-checks.md`, the report, queue, and current status surfaces were updated.

## Deferred

- Next independent kill: row-156 idea-backlog verification and scoping, followed by docs reconciliation.
- Checks 1–3 desktop interaction and check 4 Windows real-pen flags remain separate hardware/desktop follow-ups.
