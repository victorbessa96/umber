# Landed-state documentation reconciliation

- Date: 2026-11-24.
- Author: Razul (dragon), session `20261124_193811_e97c1e`.
- Brief: `docs/loop/briefs/2026-11-24-umber-docs-reconciliation.md` (`2afbbca593ac`).
- Claim: `c7337d081796`, queue row 606.
- Implementation: `81f89a057d891c8d47538cdd90751e22c3d271e8`.
- Reviewed implementation-delta SHA-256: `ef3b3d68ebc213c422aaf7d962a5a3eaaa41b7a9309dfd534e6544b45945dd5b`.
- Kill lock: `razul-docs-reconciliation`.

## Built

- Updated `AGENTS.md` and `CLAUDE.md` byte-identically. GUI 0b now names `ab9af1fa`, PSD export `40909ea`, displacement `9e0517bf`, and the AI provider interface `0dbda8e` as landed. Only full shader-declared dynamic-material sub-stacks, custom WGSL shaders, upgrade-gated rendering, and post-0.1 AI model/host binding and inference remain as deferred pointers. The generated `docs/parity.md` is linked as the requirements view.
- Corrected `docs/loop/reviews/2026-11-17-deferred-readiness.md`: projection occlusion `b228767f` and L7 `eb2237a9` are landed, with their original dispatch rows closed. The `.spp` R1 text distinguishes the landed f16 8-bit preview decode from deferred native f16/f32 payload planes and `--precision native|8`.
- Corrected `docs/loop/desktop-checks.md`: pen-flag mapping `8d7bf8a0` is landed; the live WndProc/HWND hook and real Windows pen remain required. The old check-5 PARTIAL footer is explicitly superseded by the 2026-11-24 PASS Result.
- Corrected `docs/loop/briefs/2026-11-17-dogfood-walkthrough-prep.md`: Gate 9 is MET, Gate 8/C2-24 remains partner-gated, and the first-run C2-30 label is distinct from the closed Windows Ink trace. D2–D5 and displacement are post-alpha and require a newer explicit build; the alpha URLs cover only tag-contained behavior.
- Reconciled `docs/loop/next-steps-2026-11-23.md`: docs-truthing items are closed, all eight prior engineering implementations are marked LANDED + origin-verified, parity origin read-back reached `3339df2a`, R-C1 includes mesh category `6ef03194`, check 5 is filled, and the occluder kittest is no longer deferred residue.
- Corrected `docs/loop/waves/2026-11-09-wave-plan.md`: safe-push + origin read-back is the wave gate; release CI runs only at version bumps, tags, PRs, and the weekly schedule. Passive WinInk decode and pen flags are landed while the live hook/pen gate remains. Completed SPEC/tag/.ftl items are removed from the partner-gated list.
- Corrected both ROADMAP asset rows with mesh category `6ef03194`; R-C1's named category work is closed.
- Closed the scoping record's GLB `f949199c` and generated parity `72774f67` rows, leaving material/MTL, EyeDepth, the theme matrix, and the COW A/B measurement as the remaining code-ready candidates.

## Review and dispositions

- R1 `deleg_a2e97f9d`: **REQUEST_CHANGES**, one valid finding. The copyable Gate 8 session-log template still hard-coded `v0.1.0-alpha.1 (bc4d642f)` beside post-alpha D2/D4/D5/displacement rows, contradicting the newer-build rule. Fixed by replacing it with `<newer explicit build> (<commit SHA>; required for post-alpha rows)`. All cited commit mappings and post-alpha ancestry checks passed review.
- R2 `deleg_11eae051`: **SHIP, zero findings** on the fixed session-log template and prior diff context.

## Verification

- Every cited implementation commit was resolved with `git show`/`git cat-file` before use. GUI 0b was independently corrected from the stale `9e0517bf` attribution to `ab9af1fa`.
- `git merge-base --is-ancestor` confirmed L7, PerChannel, history, material picker, Assets mesh, occluder, XI2, and SVG rgba are all contained in `origin/master`.
- Separate ancestry checks proved D2 `533560e8`, D3 `c499255f`, D4 `35097df5`, D5 `752bc991`, and displacement `9e0517bf`/`0654ba48` are POST_ALPHA, not in `v0.1.0-alpha.1`.
- `cmp -s AGENTS.md CLAUDE.md` → exit 0 after the edits.
- **28 stale-claim assertions PASS** across the changed documents. The assertions cover retired GUI/P2 claims, occlusion/L7 status, f16 precision language, desktop check 4/5, dogfood Gate 9 and build coverage, next-steps pending tails, wave CI/WinInk gates, and both ROADMAP asset rows.
- `python3 -B scripts/parity.py --check` → `parity: current`, exit 0.
- `python3 scripts/check-layers.py` → `=== layers: GREEN (16 crates) ===`, exit 0.
- `python3 scripts/scorecard.py --check` → `scorecard: README.md current`, exit 0.
- `git diff --check` → clean after removing three introduced hard-break whitespace tails.
- Fresh self-deleting `hermes-verify-docs-parity-export-*.sh` probe after the R1 fix: parity **8/8 PASS**, docs **29 assertions PASS** including the session-log template, and `cargo test -p umber-export` **105 passed / 0 failed** across 98 unit + 7 integration + 0 doc-tests. The script was verified deleted. This is focused ad-hoc evidence, not a workspace-green claim.

## Judgment calls

- Dated review and brief documents keep their historical role; only live-state assertions are corrected, with 2026-11-24 markers where a supersession note is clearer than rewriting history.
- The first-run dogfood script retains its alpha URLs as an alpha-only baseline, but now explicitly forbids recording post-alpha D2–D5/displacement results against them.
- No human, product, hardware, or release gate is closed by prose. Dogfood, native-speaker review, Windows signing, Windows pen validation, product choices, and beta/final decisions remain open.

## Unverified

- Documentation-only strike: no app behavior changed and no cargo suite or fresh CI dispatch was run. Static docs gates and commit/ancestry inspection are the verification, not a workspace-green claim.
- `scripts/safe-push.py` and independent `git ls-remote origin master` remain the origin gate at this writing.

## Deferred

- Close-out verification of the clean origin and the final autonomous status report remain after this kill.
- Material/MTL references, EyeDepth, COW undo, theme matrix, packaging deltas, and other separately scoped engineering candidates are not opened by this documentation strike.
