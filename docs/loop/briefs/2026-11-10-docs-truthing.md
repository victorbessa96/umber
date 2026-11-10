# Kill brief: P0 doc truthing (docs pass follow-up)

- Lock: `razul-docs-truthing` over:
  - SPEC.md
  - DECISIONS.md
  - docs/specs/requirements.md
  - README.md
  - STATE.md
  - docs/loop/waves/2026-11-09-wave-plan.md (status line only)
  - .github/PULL_REQUEST_TEMPLATE.md (or wherever the .spp ban is repeated)
  - CHANGELOG.md (November section only)
- Source of truth for every claim: docs/research/2026-11-10-docs-workflows-pass.md (landed, row 341) + `git log --all` + the live tree. Verify each fix yourself with git/grep before editing; do not trust the pass doc blindly.

## Targets (P0/P1 from the pass)

1. **SPEC.md:82 + PR template**: the "no .sbsar/.spp execution ever" wall. .spp IMPORT shipped 2026-10-12 (crates/spp exists with real-file corpus tests). Fix: narrow the wall to what is actually banned (execution of .sbsar; .spp remains import-only, never executed), and record the binding ruling in root DECISIONS.md with date + scope (SPEC names DECISIONS.md as its amendment path — the pass found the ruling missing there).
2. **docs/specs/requirements.md**: three rows claim unbuilt features that landed (ortho camera, jitter quartet + panel, SVG import) — mark LANDED with commit hashes from git log; the perf-floor row inverted vs perf/baseline.json + the nightly gate — correct it. This file is "the CONTRACT of record", so each edit cites evidence.
3. **README.md + STATE.md**: stop advertising landed work as open (brush angle/jitter, bitmap stamps landed C2-11 2026-10-19; wave 1.2 and 2.1 landed). Update status lines to current truth; add the missing link from README to docs/user/getting-started.md and to the keyboard reference's viewport-local P (ortho) / F (frame) keys if absent.
4. **docs/loop/waves/2026-11-09-wave-plan.md**: status line says "wave 1 is in flight" — rewrite the header status to reflect landed waves (1.1, 1.2, 2.1, 3.1a/b, 3.2, 3.3, 3.4) with hashes, and mark the disproved wave-3 v8 premise as resolved-by-analysis (row 333).
5. **CHANGELOG.md**: add a November 2026 section in the file's existing format summarizing landed work by category (ABR import tail, projection occlusion, docs/loop) with links to docs/loop/kill-reports/ — do NOT enumerate all 98 kill reports.
6. **queue.md**: do NOT renumber existing rows (the tool refuses duplicate adds and old duplicates never block). If the tail's non-monotonic order needs a note, add a one-line comment at the top of the file stating the max+1 rule; nothing else.

## Rules

- Docs only; edit only the locked files. No code. No cargo. No commit/push.
- Every changed claim carries its evidence (commit hash or live count) in the text or an adjacent comment. No claim changes without evidence.
- Keep each file's existing voice/format. Minimal diffs; do not restructure.
- Attribution rules unchanged (no reverse-engineering language anywhere).

## Dragon verification

Link/target check (every referenced path exists), grep that no stale claim remains (e.g. grep for the old .spp ban wording), scorecard/check-layers unaffected, then commit+push via dragon.
