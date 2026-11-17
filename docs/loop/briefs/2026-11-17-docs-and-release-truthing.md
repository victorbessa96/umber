# Design brief: docs and release truthing after alpha

**Date:** 2026-11-17
**Kill:** `umber-doc-truthing-20261117`
**Status:** Completed in the documentation reconciliation commit. The live GitHub release was read back on 2026-11-17: pre-release, not draft, published 2026-11-16 17:34:39Z, targeting `bc4d642f0696233ac5293683c88b2e860e52b3b2`.
**Scope:** `README.md`, `STATE.md`, `docs/ROADMAP.md`, `docs/specs/requirements.md`, `docs/specs/release-checklist-0.1.0.md`, `docs/loop/waves/2026-11-09-wave-plan.md`, `docs/loop/queue.md`, and `.dragon/COORDINATION-next-wave.md`.

## Problem

The committed tree has moved past the documentation's live-state claims.
`v0.1.0-alpha.1` was tagged at `bc4d642f` and published as an unsigned
pre-release on 2026-11-16. The post-tag tree returned its workspace version
to `0.1.0` and pinned Flatpak to the tag (`d49d0acc`). The D2-D5 baker power
wave and displacement preview subsequently landed through `0654ba48`.
Several reader-facing and operational documents still say that no release tag
exists, that the tag is in flight, or that D2-D5/displacement are open.

## Target state

- Public docs accurately describe the released alpha without implying that
  0.1.0 itself is released.
- Release checklist is internally consistent: alpha and SPEC ratification are
  MET, dogfood is the remaining pre-beta gate, signing remains deliberately
  deferred.
- The roadmap and requirements distinguish landed work from the actual
  remaining gated or deliberate-deferral lanes.
- `STATE.md` and `.dragon/COORDINATION-next-wave.md` expose one current,
  compact handoff rather than presenting early-November state as live.
- The queue records this truthing kill with source commits and no fabricated
  test or CI claims.

## Sources of truth

- Tag: `v0.1.0-alpha.1` at `bc4d642f`.
- Alpha version bump: `010b7d72`; packaging source regeneration: `8941f718`;
  tag metadata repair: `bc4d642f`; post-tag version restore and Flatpak pin:
  `d49d0acc`.
- D2 auto-cage: `533560e8`; D3 opacity: `c499255f`; D4 multi-source ID:
  `35097df5`; D5 ground plane: `752bc991`; displacement preview:
  `9e0517bf` with report `8da51ce6`.
- Current master at brief time: `0654ba48`.
- Current scorecard and layering checks: `python3 scripts/scorecard.py --check`
  and `python3 scripts/check-layers.py` both pass.

## Constraints

- Documentation-only. No behavior, dependency, version, release-tag, or
  packaging change.
- Preserve historical audit material. Replace only live-state assertions that
  contradict the committed tree.
- State actual verification boundaries: this kill receives static docs gates;
  cargo and CI are not rerun for prose.
- Release remains alpha. The dogfood walkthrough, native-speaker review,
  Windows signing decision, Windows pen validation, and upstream drafts keep
  their named gates.

## Verification

1. Check the exact referenced commits and tag with Git.
2. Run `python3 scripts/check-layers.py` and `python3 scripts/scorecard.py --check`.
3. Search the changed live docs for superseded claims such as `no release tag`,
   `IN FLIGHT` for the alpha tag, and D2-D5/displacement described as open.
4. Re-read the changed document regions and validate internal links where
   feasible.

## Non-goals

- Do not create a beta or stable release.
- Do not claim the partner-gated dogfood walkthrough occurred.
- Do not rewrite dated kill reports, audit history, or decision records.
