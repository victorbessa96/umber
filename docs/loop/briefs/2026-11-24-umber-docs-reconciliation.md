# Design brief: landed-state documentation reconciliation (2026-11-24)

Planned kill lock: `razul-docs-reconciliation`.
Base: generated parity status tip `3339df2a7dfac76e4e9bf9a43d895520e472a0e3`, clean and origin-equal.

## What exists

The generated parity table is landed. Live scouting still found contradictory reader-facing and operational claims:

- `AGENTS.md` and `CLAUDE.md` list dynamic materials, displacement preview, PSD export, the AI provider interface, and GUI 0b shell chrome as future work. GUI 0b is `ab9af1fa`; `9e0517bf` is displacement, not GUI 0b. PSD export is `40909ea`; the provider interface is `0dbda8e`; dynamic-material v1 is `f85cb0aa` + `2c6497ec`.
- The deferred-readiness review still calls projection occlusion and L7 unstarted despite `b228767f` and `eb2237a9`. It also needs to distinguish the landed f16 8-bit preview decode from the deferred native-precision payload planes.
- `desktop-checks.md` check 4 calls the landed `8d7bf8a0` pen-flag tracker parked. Its check-5 footer still says partial despite the 2026-11-24 PASS Result.
- The dogfood prep still presents Gate 9 as open and can lead a reader to run post-alpha D2–D5/displacement steps against the published alpha. Git ancestry proves D2 `533560e8`, D3 `c499255f`, D4 `35097df5`, D5 `752bc991`, and displacement `9e0517bf`/`0654ba48` are all post-alpha. Gate 9 is MET in `docs/specs/release-checklist-0.1.0.md`.
- The current next-steps map still says check 5's Result is unfilled and that only the Assets mesh category remains, although check 5 passed and mesh category `6ef03194` landed.
- The wave plan still says WinInk bytes are absent and implies release CI runs on every wave, contradicting the release-only CI + safe-push model.

## Target

- Patch live contracts and current maps in place. Add dated status corrections to dated review/brief instruments where that preserves their history better than rewriting the original audit narrative.
- Name only actual remaining tracks: full shader-declared dynamic-material sub-stacks, custom user WGSL shaders, upgrade-gated rendering, post-0.1 AI model/host binding and inference, and the already documented human/product/platform gates.
- Mark projection occlusion `b228767f`, L7 `eb2237a9`, GUI 0b `ab9af1fa`, Assets mesh category `6ef03194`, and pen-flag mapping `8d7bf8a0` landed. Do not close the live WndProc/HWND hook or real-pen checks.
- State that f16 preview decode exists, while native f16/f32 payload planes and `--precision native|8` remain the actual R1 gap.
- Mark Gate 9 MET and keep Gate 8/C2-24 partner-gated. Require a newer explicit build for post-alpha D2–D5/displacement dogfood steps; do not claim those steps run on `v0.1.0-alpha.1`.
- Keep the first-run walkthrough's C2-30 label distinct from the closed Windows Ink C2-30 trace.
- Make the wave plan say safe-push on every wave and release CI only at version bumps, tags, PRs, and the weekly schedule.

## Failure handling

If a cited commit or tag cannot be resolved, do not write the claim. Do not convert a missing runtime check into a pass. Preserve all genuine human, product, platform, and release gates. No app source, schema, dependency, version, packaging, tag, or public-PR change.

## Verification and acceptance

- `git cat-file`/`git show` resolves every cited implementation commit; `git merge-base --is-ancestor` confirms the D2–D5/displacement post-alpha boundary.
- Searches of the changed files no longer show the targeted stale live claims: GUI 0b next, PSD export/provider interface as future, check 5 unfilled, WinInk bytes absent, or release CI every wave.
- `cmp AGENTS.md CLAUDE.md` remains byte-identical after editing both.
- `python3 -B scripts/parity.py --check`, `python3 scripts/check-layers.py`, `python3 scripts/scorecard.py --check`, and `git diff --check` pass.
- Independent read-only review returns SHIP with zero unresolved findings.
- Kill report, queue row 606/607 sequence, current state surfaces, safe-push, and `git ls-remote origin master` complete the landing.

## Non-goals

No dogfood session, native-speaker review, Windows pen validation, release tag, beta/stable decision, upstream submission, or new engineering lane. Material/MTL references, EyeDepth, COW undo, theme matrix, and packaging deltas remain separately scoped candidates after this documentation strike.
