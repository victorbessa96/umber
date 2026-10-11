# The 0.1.0 Release Checklist

The tag fires on Bessa's word (the standing rule from the packaging
commit). This is the readiness state, so the word is the only
remaining input.

## The gate (what must be true when the tag fires)

1. **The P0 floor assembled** — the three hunting rows land + green:
   the fill layer + projections, the view modes, the eraser (the
   chain, 22:47). The blend census verifies the math. [IN FLIGHT]
2. **The compile debt cleared** — every code-only commit since the
   machine went off gates on the machine's return (or CI's runtime
   verdicts, which have been the de facto gate all day): the full
   workspace suite green on both platforms. [CI PER COMMIT; THE
   MACHINE'S FULL GATE PENDING ITS RETURN]
3. **The changelog current** — ✓ done (`4b234a1`).
4. **The requirements marked** — ✓ the 12 verified rows (`3f2318c`);
   the three gap rows mark when their kills land.
5. **The perf baselines** — the first row measured (d369886); the
   nightly job produces the rest on its first run (first-run mode).

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
