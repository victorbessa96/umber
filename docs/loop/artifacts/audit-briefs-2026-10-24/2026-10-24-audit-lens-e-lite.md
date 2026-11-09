# Lens E-lite — the crate health census (pi, trimmed)

READ-ONLY task in ~/Projects/umber (cwd is already there). No
cargo, no builds. You may ONLY write one file:
docs/loop/audit-2026-10-24-lens-engine.md.

Census every crate in crates/ and produce a health table:

1. For each of the 16 crates: its name, its role (one line, from
   its lib.rs/main.rs module docs), its LOC
   (`find crates/<name> -name "*.rs" | xargs wc -l | tail -1`),
   its test-fn count (`grep -rn "#\[test\]" crates/<name> --include="*.rs" | wc -l`),
   its density (tests/LOC*1000).
2. The crate list itself: read Cargo.toml's [workspace] members.
3. Name any crate with density below 7/kLOC (the project's floor).
4. Read docs/perf/protocol.md's first 60 lines and summarize the
   perf state: the baseline, the nightly invocation, the compare
   gate, the latest triage record.
5. Read SECURITY.md and report its row count + whether any row
   reads stale (a PENDING claim or a HEAD anchor that names an old
   commit).

Write the report with a headline table, the perf summary, the
security verdict, and a "named gaps" list (anything the numbers
show that the docs don't admit). Cite file:line for every claim.
When the report file is written, print LENS-E-DONE on its own
line as the last line of your reply.
