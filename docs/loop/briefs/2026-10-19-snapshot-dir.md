# Brief: 2026-10-19 snapshot-dir (W-C3a lane C1)

Mandate: Bessa, 2026-10-19: "Continue working on the umber long horizon
project in all the steps, do not stop." Kill plan:
`docs/loop/next-wave-plan-draft.md` §1 lane C1 — the harness-script
report's own deferral.

## Contract

1. The agent protocol's `paint_snapshot` step
   (`crates/umber-cli/src/agent.rs` — grep its current shape first)
   takes EXACTLY ONE of `out` | `out_dir`. Both, or neither, is a
   typed refusal BEFORE any GPU work runs.
2. `out_dir` writes `<out_dir>/<step:05>.png`, where `step` is the
   session's 0-based step index. GREP agent.rs for an existing step
   counter first and reuse it; only add one if none exists.
3. `deny_unknown_fields` keeps every old script valid (an absent
   `out_dir` changes nothing for existing callers).
4. `PROTOCOL_VERSION` STAYS 1 (pre-tag; the 10-14 report's ruling).
5. The pure path-naming logic is its own small fn so the test can hit
   it directly.

## Files you own (the ONLY files you may write)

- `crates/umber-cli/src/agent.rs`
- `crates/umber-cli/tests/agent_paint_ops.rs`
- `docs/specs/agent-paint-ops-design.md` (§ The snapshot arm — the
  deferral tick)
- `docs/loop/kill-reports/2026-10-19-snapshot-dir.md`

## Tests you write (do NOT run them — the dragon gates on the build host)

- a pure test of the path-naming fn: step 7 -> `00007.png`; step 0 ->
  `00000.png`.
- a spawn test in the existing harness (read
  `tests/agent_paint_ops.rs` first and follow its exact spawn idiom):
  two snapshots into an `out_dir` -> two files exist, and each file's
  blake3/sha256 equals the hash the reply carried. Relational pins
  ONLY — no golden hex values.

## Discipline (binding)

- The tree is SHARED. Dirty or untracked files you did not create
  (umber-app/*, umber-gui/*, stylus/*, paint files, `.dragon/*`,
  other kills' reports) belong to other live sessions: never touch,
  restore, checkout, or commit anything. `git log/diff/grep` reads
  are fine; NO git mutations.
- NO cargo, rustc, build, or test runs of any kind. You may grep/read
  files and run `python3`. Match the files' existing formatting style;
  the dragon runs rustfmt at review.
- NO lock operations (`scripts/kill-locks.sh` any subcommand,
  `/tmp/umber-kill-locks/*`), never write to `/tmp` or anywhere
  outside the four files above.
- If any anchor drifted, re-derive from live source. Never invent an
  API — grep its definition first.

## Report shape

Write `docs/loop/kill-reports/2026-10-19-snapshot-dir.md`: header
naming authorship (claw: opencode/deepseek-v4.1-flash-free), kill-lock
`snapshot-dir` + its files, design source
(`docs/loop/next-wave-plan-draft.md` §1 lane C1 + this brief); then
**Built**, **Judgment calls**, **Unverified**, **Deferred**, **Process
note**. End with a one-screen summary: files changed + every test name
+ what it pins.
