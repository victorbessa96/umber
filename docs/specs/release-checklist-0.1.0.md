# The 0.1.0 Release Checklist

The tag fires on Bessa's word (the standing rule from the packaging
commit). This is the readiness state, so the word is the only
remaining input.

Updated 2026-10-12 per the strategy review's checklist audit: every
gate below checked process; none checked FUNCTION. Gate 0 (the
journey test) is now the first gate — the process gates follow it.

## Gate 0 — the journey test (the FUNCTION gate, added by the review)

FBX → 4K pressure paint → see lit → undo/redo → save/reopen → ORM
export decode-verified. One continuous session exercising the six
core-loop criticals JOINED — not each in isolation. The harness:
`crates/umber-app/tests/gate0_journey.rs` (in flight; the compositor
assertion is the headless 'see lit' equivalent, the shell frame named
as the shell-only remainder). [IN FLIGHT]

## The gate (what must be true when the tag fires)

1. **Gate 0 green** — the journey test passes on CI (lavapipe + WARP),
   the core-loop phase's exit gate.
2. **The P0 floor assembled** — the three hunting rows land + green:
   the fill layer + projections, the view modes, the eraser (the
   chain, 22:47). The blend census verifies the math. [LANDED — the
   fill layer, view modes, and eraser kills all landed and CI-green
   in the 2026-10-11/12 era; the blend census carried the math]
3. **The compile debt cleared** — every code-only commit gates on CI
   per commit (the de facto gate): the full workspace suite green on
   both platforms. [CI GREEN per commit through f867215]
4. **The changelog current** — ✓ done (`4b234a1`).
5. **The requirements marked** — ✓ the census current through the
   core-loop phase: all six criticals' rows marked ✓ code with the
   code-vs-reachable split (the last marks: `472b774`).
6. **The perf baselines** — the first row measured (d369886); the
   re-baseline at the 4K default = the named follow-up (the 512²
   baselines are invalidated by `133dd12`).

## The tag shape (drafted, not fired)

- `v0.1.0` on master, the annotated tag.
- The message: the one-paragraph 0.1.0 definition — "the professional
  floor: the brush engine (seam-aware, presets, symmetry, smudge/
  clone, the quick mask), six bakers, the export pipeline (glTF/
  Unreal/Unity/Blender presets + PSD/EXR/MTLX), UDIM, the node graph,
  WASM plugins, headless CLI + the JSONL agent surface, i18n (en/pt-BR),
  installers for Linux + Windows."
- The dist workflow (release.yml) builds on the tag push: the two
  targets, shell + PowerShell installers, checksums — measured at
  11 MB against the 150 MB budget.
- SPEC ratification + a dogfood cohort ride the release path as
  gates 7-8 (the review's IMPROVEMENTS §4.4): the SPEC docs
  ratify before the tag, the dogfood cohort (Bessa first) paints a
  real asset end to end and files what breaks.

## Release phases (added 2026-10-14, absorbed from Blender's release cycle)

Blender cuts each release through Alpha → Beta → RC, and freezes its
Python API at Beta so add-on authors get a stable target before the
release (developer.blender.org/docs/handbook/release_process/release_cycle).
Umber has two surfaces outside users build against — the JSONL agent
protocol and the `.umber` file format — so the same freeze applies.

| Phase | Entry | What may land | Exit |
|---|---|---|---|
| **Alpha** (now) | — | anything; risky changes go here, early | the gate list above all green except Gate 0's shell-frame remainder |
| **Beta** | Bessa calls it | bug fixes, docs, perf; **no** agent-protocol step changes, **no** `.umber` / paint-sidecar schema changes, **no** new user-visible strings (the pt-BR pass needs a frozen catalog) | one dogfood asset painted end to end (gate 8) with no open P0 |
| **RC** | Beta exit | critical fixes only, each reviewed | 1 week with no new critical, then the tag |

**What the freeze points at.** `.umber` carries `CURRENT_PROJECT_VERSION`
/ `LAYER_PLANES_PROJECT_VERSION`, the paint sidecar `paint_save_version`,
and the agent protocol `PROTOCOL_VERSION` (umber-cli `agent.rs`, answered
by the `version` step, pinned by a test; added 2026-10-14 because the
protocol had no number to freeze). After 0.1.0, any step change bumps it.

## The known-deferred (honest, in the tag's companion notes not the message)

- PSD import (P3-designed), .usdz read beyond the first layer, the
  full-32 blend set, OCIO configs (the adoption probe's rule), the
  path tools, the post-effects stack — all tracked, none blocking
  the 0.1.0 floor. (Review-audit note: nothing P0 hides here — the
  P0 holes all live in the census's code-vs-reachable split now.)
