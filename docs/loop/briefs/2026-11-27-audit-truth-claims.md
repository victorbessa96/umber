# Kill brief: audit truth claims

- Lock: `razul-audit-truth-claims`.
- Trigger: engine audit findings 4–6 at `ec979743` remain live. Current prose overstates sampled-4K measurement and UDIM continuity, while denying capabilities that already landed.

## Required corrections

- Performance: state that the RTX 3070 record is 1,002,252 triangles plus a resident-but-unsampled 4K set. The full sampled-4K requirement remains an explicit hardware gate, not MET.
- UDIM: multi-tile sets, per-tile resolution, and per-event cross-tile painting are built; conditioner/spacing continuity across a tile transition remains partial. Do not reopen or invent an implementation in this docs kill.
- Mark dynamic material layering, AssetStore GC, merge-below-top, split-redo, and the complete icon atlas with current code evidence.
- Synchronize stale module comments about the multi-entry ID cache, landed per-channel density arms, and the existing cage buffer.
- Regenerate `docs/parity.md` from the requirements table and require its check to pass.

## Verification and constraints

Every changed claim must have a live file:line oracle or an explicit external gate. No benchmark result, cross-tile behavior, EXR/PSD feature, or hardware evidence is manufactured. Docs/comments only; no release, tag, upstream PR, or centralhost work. Independent review checks every edited row against source.
