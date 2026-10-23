# Kill report: snapshot-dir (W-C3a lane C1)

- Authorship: claw (opencode/deepseek-v4.1-flash-free), 2026-10-19.
- Kill-lock `snapshot-dir`: `crates/umber-cli/src/agent.rs`,
  `crates/umber-cli/tests/agent_paint_ops.rs`,
  `docs/specs/agent-paint-ops-design.md`,
  `docs/loop/kill-reports/2026-10-19-snapshot-dir.md`.
- Design source: `docs/loop/next-wave-plan-draft.md` §1 lane C1
  (`snapshot-dir` — "the harness-script report's own deferral",
  `2026-10-14-harness-script.md` Deferred: "A `snapshot_dir`
  convenience (auto-named files per step index)") + the brief
  `docs/loop/briefs/2026-10-19-snapshot-dir.md`.

## Built

1. `crates/umber-cli/src/agent.rs`:
   - `PaintSnapshotStep.out` became `Option<PathBuf>` and a new
     `out_dir: Option<PathBuf>` was added; both `#[serde(default)]`, so
     `deny_unknown_fields` keeps every old script valid (an absent
     `out_dir` changes nothing). Field/docs updated.
   - New pure fn `snapshot_out_path(out_dir: &Path, step: usize) ->
     PathBuf` = `out_dir.join(format!("{step:05}.png"))` — the naming
     rule, in one testable place.
   - `Session` gained `index: usize`; `Session::reply` stores the step
     index it already receives into it before dispatch. No new counter:
     this is the same 0-based index the protocol reports as `index`.
   - `paint_snapshot` resolves the destination FIRST (exactly one of
     `out` | `out_dir`; both/neither is a typed refusal) and only then
     calls `read_selected_plane`, so a malformed step never requests the
     paint device and its message is the protocol's. The reply's `out` is
     the RESOLVED path.
   - Module docs note the destination rule.
2. `crates/umber-cli/tests/agent_paint_ops.rs`: one new spawn test (3
   test fns total across the kill; see the summary).
3. `docs/specs/agent-paint-ops-design.md`: § The snapshot arm gained a
   `### out_dir (2026-10-19, ROADMAP snapshot-dir) — the deferral tick`
   subsection (contract, the reused counter, `PROTOCOL_VERSION` stays 1).

## Judgment calls

- **The pure test is inline, not in the integration harness.** The brief
  lists the path-naming test and the spawn test; the pure fn lives in the
  binary crate (`agent.rs`, no `lib.rs`), so an integration test in
  `tests/` cannot name it — there is no library surface, and `Cargo.toml`
  is outside this kill's files. The pure test therefore sits in the
  existing `#[cfg(test)] mod tests` in `agent.rs`; the spawn test stays in
  the integration harness as instructed. (Alternative — adding a `[lib]`
  target — needs `Cargo.toml`, which this kill does not own.)
- **The index is stored on `Session`, not threaded through `run`.**
  `reply` already receives the step index (it is the reply's `index`);
  storing it is the minimal reuse. Threading it as a `run`/`paint_snapshot`
  parameter would have moved two signatures for one caller. It is the
  SAME counter — not a second one — so a reply's `index` and its
  `out_dir` filename can never disagree.
- **Refusal ordering.** Validation is placed before `read_selected_plane`
  (the first GPU-touching call). The inline refusal test runs with NO
  project open: without the early check the reply would be "no project
  open", so the destination message proves the refusal precedes the
  project read and every GPU path.
- **`out` became optional, with no default.** The exclusivity requires
  both fields optional (an absent one is the other arm, not a fallback).
  Old scripts always carried `out`, so nothing regresses.
- **`PROTOCOL_VERSION` stays 1**, per contract item 4 / the 2026-10-14
  ruling: 0.1.0 is untagged, so the Beta-freeze "bump on any step change"
  rule has not started.
- **Naming past 99999 grows** (`100000.png`) rather than truncating;
  stated in the fn docs. No session realistically reaches it, and growth
  never overwrites an earlier snapshot.

## Unverified

- **No build, no test run.** The brief forbids cargo/rustc/build/test.
  Compilation and the new tests are unverified on this host; the dragon
  gates on the build host. The most likely compile risk is a clippy nit in
  the new test code (all APIs used — `decode_paint_png`, `blake3`,
  `scratch`, `Agent` — are copied from the neighbouring test in the same
  file, and `snapshot_out_path` from the new fn).
- **The spawn test's runtime behavior on a GPU host** (file writes land,
  hashes match) is asserted by construction and copied from the `out`-arm
  test's proven idiom; it needs the paint device (lavapipe/WARP/hardware)
  to actually run, and skips typed where none exists.

## Deferred

- Per-adapter golden-hash sets (the other 2026-10-14 deferral) remain
  out of scope — unrelated to naming, and the brief does not name them.
- No `--script` runner is added; the plan (§1 lane 4) rules that a GUI
  JSONL runner belongs in umber-app, not this lane.

## Process note

- Static checks run (python3 only, the sanctioned tool): brace balance on
  both `.rs` files (555/555 and 219/219, PASS) plus verify-kill's
  pure-python trap legs ported by hand — `matches!` arity (PASS),
  doc-comment attachment (PASS), thiserror `source` trap (PASS).
  `scripts/verify-kill.sh` was NOT run: it invokes rustfmt in place and
  reads `/tmp/umber-kill-locks/*`, both barred by this brief's discipline.
- No git mutations, no lock operations, no `/tmp` writes. Only the four
  owned files were touched.

## One-screen summary

**Files changed**

- `crates/umber-cli/src/agent.rs`
- `crates/umber-cli/tests/agent_paint_ops.rs`
- `docs/specs/agent-paint-ops-design.md`
- `docs/loop/kill-reports/2026-10-19-snapshot-dir.md` (this report)

**Tests added (3)**

- `out_dir_snapshot_names_are_the_zero_padded_step_index` (inline,
  `agent.rs`) — pins the pure naming fn directly: step 7 → `00007.png`,
  step 0 → `00000.png`.
- `snapshot_needs_exactly_one_destination_before_any_work` (inline,
  `agent.rs`) — pins the refusal for both bad shapes (`out` + `out_dir`,
  and neither) via a project-less session, so the message proves the
  check precedes the project read / GPU.
- `out_dir_snapshots_use_step_names_and_match_their_replies` (spawn,
  `tests/agent_paint_ops.rs`) — two `out_dir` snapshots → two files named
  from each reply's own `index`, `out` equal to the resolved path, exactly
  those two files present, and each file's decoded bytes hash (blake3) to
  the reply's `blake3`. Relational pins only — no golden hex.
