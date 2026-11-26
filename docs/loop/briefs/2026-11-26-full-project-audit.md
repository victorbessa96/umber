# Full-project audit delta — 2026-11-26

- Lock: `razul-full-audit-2026-11-26`.
- Base: `ec979743f701d20071685e48c658bf9cf6e3536e`, clean and origin-equal.
- Baseline: the 2026-10-24 full-project audit and its six lens reports.
- Current CI evidence: `45829837272` succeeded at `c14d1baf` on Ubuntu and Windows. The final docs-only close-out is `ec979743` and does not alter code.

## Scope

1. Re-derive mechanical headline numbers from the live tree.
2. Verify every standing debt from the 2026-10-24 headline against current code, not old queue prose.
3. Complete independent engine, formats, tools, UI/UX/requirements, and platform lens deltas. Each lens is read-only and must return actionable file:line findings or an explicit no-finding verdict.
4. Synthesize one current audit document and per-lens reports. Separate genuine code defects from human/product/platform gates. Any code fix receives its own brief, lock, regression, review, and push.
5. Refresh only the status surfaces required by the findings. No stale completion claim.

## Mechanical evidence already measured

- Workspace crates: **16**.
- Exact `#[test]` attributes: **3,295**.
- `pygount` via `uv tool run`: **372,122 Rust code lines**; **399,235 total code lines** across recognized languages; **84,056 comment lines**. Dependency/build/artifact directories excluded.
- 2026-10-24 standing debts: split-redo, merge-below-top, winink, DPI re-raster, AssetStore GC caller, cached ID-mask, and the OccluderScope/L7 lanes are all closed or have a current concrete gate. No debt is reopened from the old document alone.

## Non-goals

No code changes in the audit kill, no new feature wish list, no release/tag/signing/native-speaker/hardware work, no upstream PR, no centralhost, and no generic style-only findings.

## Completion

The audit closes only when every lens is consumed, every actionable defect is either fixed under its own verified kill or carries a concrete gate, the report/status surfaces are committed and pushed, and origin is read back. Human and release gates remain explicitly separate from code-ready work.
