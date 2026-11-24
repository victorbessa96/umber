# Design brief: generated capability parity table (2026-11-24)

Planned kill lock: `razul-parity-table`.
Base: GLB mesh export status tip `4036ff6649e94f5cd4cc2f51c373476998fbbde3`.

## What exists

`docs/specs/requirements.md` is the capability contract and already carries each row's status. `scripts/scorecard.py --check` gates headline counts. There is no generated `docs/parity.md` with a CI check, so a generated parity view is the remaining row-156 slice. The existing requirements table is the source of truth; this generator must not invent a second contract.

## Target

- Add `scripts/parity.py`, stdlib-only, with `--requirements` and `--output` options plus `--check`.
- Parse section headers and four-cell requirement rows from `docs/specs/requirements.md`, preserving order and the full status text. Rows with fewer than four cells are treated as open rather than silently dropped.
- Generate `docs/parity.md` with a generated-by header, a summary count, and one table per section: Requirement, Priority, Wave, Status. Escape pipes in requirement text.
- `--check` regenerates the exact expected bytes and compares them to the output file, exiting nonzero on missing or stale output. Default behavior writes the generated file.
- Add a `Parity current` step to `.github/workflows/ci.yml`'s cargo-free `gates` job next to layering and scorecard.
- Add a negative control in an OS-safe probe: generate, check clean, mutate a status in a scratch parity file, and prove `--check` fails and reports drift. Clean up the probe and scratch files afterward.

## Failure handling

Missing requirements, missing parity output, malformed rows, and stale output are hard failures with a clear error and nonzero exit. No silent acceptance of a missing file. No production source edit is allowed.

## Verification and acceptance

- `python3 scripts/parity.py` regenerates `docs/parity.md`; `python3 scripts/parity.py --check` exits 0.
- A second run produces byte-identical output.
- The negative-control probe proves a mutated status causes a nonzero `--check` result and names the mismatch.
- `python3 scripts/check-layers.py` → GREEN; `python3 scripts/scorecard.py --check` → current; `git diff --check` clean.
- Docs-only safe-push and `git ls-remote origin master` verify the landed status commit.

## Non-goals

No new capability semantics, requirements rewrite, CI release action, second-kill scope, or app source edit. Material/MTL references, EyeDepth, COW undo, theme matrix, and packaging deltas remain separately scoped candidates. Requirements.md stays authoritative; parity.md is generated evidence.
