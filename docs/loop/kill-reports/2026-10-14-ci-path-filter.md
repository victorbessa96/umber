# Kill report — ci-path-filter (docs-only PRs skip the build matrix)

Date: 2026-10-14. Author: dragon. Lock: `ci-path-filter` over
`.github/workflows/ci.yml`, `scripts/ci-classify.py`, this report. Source:
ROADMAP wave 8b row `wave-ci-path-filter`; absorbed from photocraft
`.github/workflows/ci.yml` (`platforms` job), see
`docs/research/absorption-2026-10-14-photocraft-blender.md`.

## Built

- `scripts/ci-classify.py`: prints `code=true|false` for
  `$GITHUB_OUTPUT`. `false` only when EVERY changed file is docs-shaped:
  the root prose files (README, CHANGELOG, CONTRIBUTING, SECURITY,
  CODE_OF_CONDUCT, STATE, DECISIONS, CLAUDE, AGENTS, LICENSE), the issue
  and PR templates, and `.md`/image files under `docs/`. Everything else
  is code. `--check <paths…>` classifies a list offline.
- `ci.yml`: a `changes` job (plain `git diff base...HEAD`, full-depth
  checkout, no third-party action); `check` now `needs: changes` and runs
  only when `code == 'true'`. The `gates` job (layering) always runs.

## Judgment calls

1. **An allowlist of docs, not a list of platform paths.** photocraft
   adds its Windows leg only when platform-sensitive paths change. In
   Umber nearly every crate is platform-sensitive (wgpu backends, stylus,
   OCIO, file paths), so the only skip that is always safe is "nothing
   compilable changed". It saves less, but it can never skip a needed
   build.
2. **Fail open everywhere.** A push to master, a missing base, an
   unreachable base, an empty diff, and the `full-ci` label all give
   `code=true`. A classifier error must never be the reason CI didn't
   run.
3. **`docs/` is not all docs.** `umber-graph/src/mtlx_doc.rs` loads
   `docs/claw-artifacts/openpbr/open_pbr_surface.mtlx` in a test. Only
   `.md` and image files under `docs/` are skippable; a `.mtlx`, `.json`
   or anything else there runs the matrix. A `.md` inside `crates/` also
   counts as code (doc-tests and `include_str!`).

## Verified

`--check` cases, all as expected: README alone → false; ROADMAP +
CHANGELOG + issue template → false; ROADMAP + a `.rs` → true; a crate
README → true; `docs/*.json` → true; the openpbr `.mtlx` → true; its
README → false; `ci.yml`, a script, `Cargo.lock`, `rust-toolchain.toml`
→ true; an empty list → true. Env paths: push → true; `full-ci` → true;
bad base sha → true; no base → true.

## Unverified

- The workflow itself runs only on GitHub. The first PR after this lands
  is the real test (expected: `changes` green, then the matrix runs for a
  code PR or is skipped for a docs PR).
- **Required checks:** if `fmt + clippy + test` is ever made a REQUIRED
  status check, a skipped matrix job reports "skipped", which GitHub
  treats as passing for required checks only in some configurations.
  `master` has no branch protection today (checked: `gh api …/protection`
  → 404), so nothing is blocked; revisit if protection is added.

## Deferred

- photocraft's other CI savings (main-only cache saves,
  `CARGO_PROFILE_DEV_DEBUG=line-tables-only`), each a separate change
  with its own measurement.
