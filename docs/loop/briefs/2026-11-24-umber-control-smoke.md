# Design brief: control channel + bridge live smoke (2026-11-24)

Planned kill lock: `razul-control-smoke` (docs and verification only; no app source changes).
Base: bounded SVG `rgba()` status tip `75b4e50d74492fd4cb2c68afe9ce3b6a4179eb54`.

## What exists

`docs/loop/desktop-checks.md` check 5 has a successful headless partial from 2026-11-05, but its Result line is still blank. `ControlClient` and `BridgeServer` already implement token auth, bounded JSONL, UI-thread inspection, PNG screenshots, and verbatim MCP relay to an `umber-cli mcp` child. The current build contains those implementations plus all recent hardening.

## Target

- Build the current `umber` and `umber-cli` debug binaries.
- Launch with `--control PORT --bridge PORT` under a scratch XDG/TMPDIR and a per-run `UMBER_CONTROL_TOKEN`, never the real config.
- Prove a wrong token returns `{"id":1,"ok":false,"error":"unauthorized"}` and then EOF on the next read.
- Authenticate and prove `ping`, `methods`, `ui.inspect` (panels/layers/tool), and `viewport.screenshot` return real replies. Decode the PNG magic and assert IHDR dimensions match the reply and the image is not one uniform color.
- Authenticate on the bridge and send one MCP `initialize` JSON-RPC line; assert the relay returns the child's `initialize` result with the pinned protocol version and server info.
- Fill check 5's Result line with the actual observations and note checks 1-4 remain owed on Bessa's desktop.
- Record the report and update the current remaining-work order, commit docs-only, safe-push, and verify origin.

## Failure handling

A failed launch, timeout, wrong-token mismatch, malformed auth, missing UI fields, bad PNG, non-matching dimensions, or bridge relay failure is a hard failure and leaves the Result line untouched. No claim is made for checks 1-4 or real-pen Windows flags. No production source edit is allowed.

## Verification and acceptance

- Use an OS-safe temporary `hermes-verify-` script under the scratch directory, run it against the current binaries, and delete it afterward. This is ad-hoc verification, not a workspace-green claim.
- Run rustfmt on no source files (docs-only), `python3 scripts/check-layers.py`, `python3 scripts/scorecard.py --check`, and `git diff --check`.
- Independent review of the smoke evidence is optional for this docs-only tail; the report records the exact wire replies and exit/cleanup behavior.
- Commit path-limited through dcommit, run `scripts/safe-push.py` bare, and independently verify `git ls-remote origin master`.

## Non-goals

No checks 1-4 claims, no real-pen Windows run, no app refactor, no new dependency, no second-kill scope. Row-156 idea backlog and docs reconciliation remain later work.
