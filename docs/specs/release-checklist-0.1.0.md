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

## The known-deferred (honest, in the tag's companion notes not the message)

- PSD import (P3-designed), .usdz read beyond the first layer, the
  full-32 blend set, OCIO configs (the adoption probe's rule), the
  path tools, the post-effects stack — all tracked, none blocking
  the 0.1.0 floor.
