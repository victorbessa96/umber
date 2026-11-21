# Umber — Current State

> This file is the compact current handoff. Historical wave narratives belong in
> `CHANGELOG.md`, `DECISIONS.md`, `docs/loop/queue.md`, and dated kill reports.
> Do not use old queue or coordination entries as live status.

## Status — 2026-11-21

Umber is in its **published-alpha hardening phase**, with the path-tool vector
wave moving through ungated engineering in parallel with the human release gates.

- **Published artifact:** [`v0.1.0-alpha.1`](https://github.com/victorbessa96/umber/releases/tag/v0.1.0-alpha.1) is a GitHub pre-release, published 2026-11-16 17:34Z from `bc4d642f`. It is deliberately unsigned, carries SHA-256 checksums, and is **not** the final `v0.1.0` release.
- **Workspace state:** K8b SVG stroke miter limits is implemented at `22bdd3e4` with brief `069fbdb5`, on K8a compound fill `6f35557f` and K7f `2555d2d8`. K8b adds finite >=1 storage/validation, cache identity, legacy JSON compatibility, normative interior-angle miter geometry, root/group/path SVG inheritance, and native roundtrips. Core is 338/338; the full app battery is green; R1/R2 findings were fixed and independent R3 returned SHIP / APPROVE. Report: `docs/loop/kill-reports/2026-11-21-umber-vector-miter-limit.md`.
- **Specification:** Bessa ratified SPEC v1.1 on 2026-11-16. `SPEC.md` is the ratified contract; the v1.1 amendment records the changes from the original v1.0 draft.
- **Recent product work:** quick-mask undo (`c5909efe`), snapshot-breadth-2 (`e9d0926f`), vector model K1 (`46d4bff5`), vector raster K2 (`0b5cb72d`), vector canvas K3 (`98fdbb80`), vector widgets K4 (`a4373c51`), SVG export K5a (`09d01fbd`), SVG import K5b (`0d1b6ad7`), K5b review hardening (`6ffec3bc`), Fill cache-lifecycle parity (`8747c681`), K6a even-odd vector fill (`7372aa53`), and K6b stroke cap options (`1839de83`), K6c stroke join options (`68cbf388`), K6d dash patterns (`58f9e160`), K7a multi-path SVG import (`29a02f9a`), K7b stroke-only subpath import (`3d087302`), K7c root/group paint inheritance (`f04ed236`), K7d group/path affine transforms (`65957542`), K7e straight-line basic-shape import (`64dad2b1`), K7f arc-to-cubic conversion with curved basic-shape import (`2555d2d8`), K8a compound fill with one-layer-per-path subpath import (`6f35557f`), and K8b SVG stroke miter limits (`22bdd3e4`) are the latest landed kills. Earlier post-alpha D2–D5, replay-exact, and displacement-preview work is recorded in `docs/loop/queue.md` and dated kill reports.
- **Active engineering:** K2 adds nonzero fills, round-cap/join strokes, source-over alpha, typed resource budgets, and cached eval-owned baseColor planes. K3 adds reversible SetVector journal support and document-backed UV anchor/handle gestures. K4 adds a dedicated Vector Properties rail and native sRGBA fill/stroke controls. K5a adds deterministic UV-space SVG export with round-stroke attributes and a user-selected save flow. K5b adds bounded one-path SVG-to-vector import with exact cubic anchors, typed refusals, and one grouped undoable layer add; its review hardening closes presentation-effect gaps, capped streaming reads, precision loss, and failure-atomicity/test-honesty findings. Fill cache-lifecycle parity adds full-dimension cache identity, open/reset and open-seeding lifecycle, live pruning, dormant-safe observation, and prospective-budget retry. K6a adds additive even-odd vector fill with NonZero default, shared scanline evenodd, panel combo, and SVG roundtrip. K6b adds additive stroke cap options with Round default, closure-safe joins, panel combo, and SVG roundtrip. K6c adds additive Round/Miter/Bevel stroke joins with closure-safe geometry, panel combo, and SVG roundtrip. K6d adds bounded dash patterns with UV-scaled segmentation, cap/join behavior, closure continuity, panel controls, and SVG roundtrip. K7a adds bounded ordered multi-path SVG import with one grouped undoable layer per path. K7b adds stroke-only subpath splitting with closepath handling and typed compound-fill refusal. K7c adds scoped root/nested-group paint inheritance, grouped budgets, and one grouped undoable import. K7d adds nested group/path affine transforms with effective-CTM geometry and stroke/dash scaling. K7e adds exact rect/polygon/polyline/line import with closure, budget, and grouped-undo coverage. K7f adds shared SVG endpoint-to-center arc conversion, editable A/a cubics, rounded rect/circle/ellipse import, exact generated-anchor charging, and curved grouped undo/redo coverage. K8a adds additive primary+secondary compound geometry, combined nonzero/evenodd fill, per-contour strokes with fresh dash phase, aggregate geometry budgets, one-layer-per-path import, deterministic compound SVG roundtrip, secondary-safe cache invalidation, primary-edit preservation, and grouped compound undo/redo. K8b adds configurable SVG miter limits with normative interior-angle geometry, exact-boundary and near-reversal coverage, raw root/group/path validation, native roundtrips, and public solid/dashed raster tests.
- **Quality boundary:** the alpha proves release packaging produced downloadable app and CLI archives, installers, source, checksums, and a distribution manifest. It does not replace the final-release gates.

## Remaining release and product gates

1. **Dogfood, C2-24:** a two-material prop must be taken through first-run, painting, baking, save/reopen, and export using the published alpha. The prepared script is `docs/loop/briefs/2026-11-17-dogfood-walkthrough-prep.md`. This is a manual Bessa session, not a claim docs can manufacture.
2. **Beta/final readiness:** resolve dogfood findings, run the native-speaker pt-BR pass, decide the Windows signing route, and rerun release CI on the intended release tip before any beta or stable tag.
3. **Platform gates:** the live Windows Ink hook and real-pen validation remain blocked on a Windows session. The passive decode in `crates/stylus/src/winink.rs` is not the hook.
4. **Explicit deferrals:** the eframe/wgpu-upgrade lanes (WASM shaders, tessellation, HDR surface, ray query), staged `.spp` fidelity work, and projection product questions remain deliberately deferred. `.spp` writing and `.sbsar` execution remain permanent walls.
5. **Upstream and accessibility work:** upstream drafts remain subject to the public-PR gate; the i18n native-speaker review remains a human-quality gate.

## Evidence and source order

1. Git refs, tags, committed code, and GitHub release objects establish what has shipped.
2. `docs/specs/release-checklist-0.1.0.md` is the release-gate record.
3. `docs/ROADMAP.md` and `docs/specs/requirements.md` map capability state and deliberate deferrals.
4. `docs/loop/queue.md` is append-only execution history; dated briefs and kill reports are evidence, not current status.

## Working protocol

- One cargo stack at a time, protected by `flock /tmp/umber-cargo.lock`.
- Every behavior change: scoped kill lock, brief, implementation, independent verification, kill report, path-limited commit, and `scripts/safe-push.py`.
- Pushes are autonomous under the standing push approval. Public release tags and artifacts remain gated by the release checklist and Bessa's release decision.
- `cargo fmt --check`, `scripts/check-layers.py`, `scripts/scorecard.py --check`, targeted clippy with `-D warnings`, and targeted tests are the local battery. CI is a release gate at version bumps, tags, pull requests, and its weekly schedule.

## Historical pointers

- `CHANGELOG.md` — release and wave history.
- `DECISIONS.md` — append-only decision record.
- `docs/loop/queue.md` — append-only execution ledger.
- `docs/loop/reviews/2026-11-17-d2-d5-cross-review.md` — D2–D5 review receipt and subsequent hardening context.
- `docs/loop/briefs/2026-11-17-docs-and-release-truthing.md` — this documentation reconciliation's scope and source commits.
