# Umber — Current State

> This file is the compact current handoff. Historical wave narratives belong in
> `CHANGELOG.md`, `DECISIONS.md`, `docs/loop/queue.md`, and dated kill reports.
> Do not use old queue or coordination entries as live status.

## Status — 2026-11-17

Umber is in its **published-alpha hardening phase**.

- **Published artifact:** [`v0.1.0-alpha.1`](https://github.com/victorbessa96/umber/releases/tag/v0.1.0-alpha.1) is a GitHub pre-release, published 2026-11-16 17:34Z from `bc4d642f`. It is deliberately unsigned, carries SHA-256 checksums, and is **not** the final `v0.1.0` release.
- **Workspace state:** the post-tag tree restored its development version to `0.1.0` and pinned Flatpak's source to the alpha tag in `d49d0acc`. Current tip is `0654ba48` at this handoff.
- **Specification:** Bessa ratified SPEC v1.1 on 2026-11-16. `SPEC.md` is the ratified contract; the v1.1 amendment records the changes from the original v1.0 draft.
- **Recent product work:** D2 auto-cage (`533560e8`), D3 opacity (`c499255f`), D4 multi-source ID (`35097df5`), D5 ground-plane baking (`752bc991`, hardened by `3b72c98f`/`baec964b`), the replay-exact opt-in toggle (`aa1f5903`), and viewport displacement preview (`9e0517bf`) have landed after the alpha tag. Their reports and review receipts are under `docs/loop/kill-reports/` and `docs/loop/reviews/`.
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
