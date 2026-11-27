# Audit truth-claim reconciliation

- Lock: `razul-audit-truth-claims`.
- Brief: `a8e472d10b8d` (`docs(audit): truth-claim reconciliation brief`).
- Outcome: performance, UDIM, and capability claims now distinguish landed code from the remaining external or scoped gates.

## Corrections

- The RTX 3070 record now reads 1,002,252 triangles plus a **resident-but-unsampled** 4K set. The full sampled-4K measurement is explicitly open in both requirements and the release checklist.
- UDIM multi-tile sets, per-tile resolution, and per-event cross-tile painting stay built; tile-transition conditioner/spacing continuity is marked partial.
- Dynamic material v1, AssetStore GC, merge-below-top, split-redo, and the complete icon atlas now have current source evidence instead of stale absence/deferral text.
- Module comments now describe the three-entry ID cache, landed per-channel density arms, and the existing padded cage/index buffers.

## Verification

- `python3 scripts/parity.py` regenerated the table; `parity.py --check`: current.
- `cargo fmt --all -- --check`: clean.
- `cargo clippy -p umber-paint -p umber-bake --all-targets -- -D warnings`: clean.
- `check-layers.py`: GREEN (16 crates).
- `scorecard.py --check` and `git diff --check`: clean.
- Independent review found three citation issues, then three remaining module-level contradictions; all were corrected. Final review `deleg_9c8cbd83`: **SHIP**, zero findings. The reviewer’s informational stale test-comment note was also fixed.
- Commit/push/current-tip CI: not yet claimed.

## Non-goals

No sampled benchmark, cross-tile implementation, EXR/PSD feature, hardware result, release, tag, upstream PR, or centralhost work was manufactured.
