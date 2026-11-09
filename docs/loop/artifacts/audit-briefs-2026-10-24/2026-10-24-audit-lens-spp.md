# Audit lens C — the SPP parity comparison (2026-10-24, live-verified)

You are auditing Umber (~/Projects/umber), a Rust 3D texture-painting
app whose parity target is Adobe Substance Painter (SPP). READ-ONLY
TASK: no cargo, no builds, no file edits outside your report file.
Every claim cites file:line from the LIVE tree.

## The lens: Substance Painter, capability by capability, vs Umber TODAY

The baseline: `docs/research/spp-parity-census-2026-10-16.md` (the
full-spectrum census, every gap grepped against the tree at the
time) + `docs/research/spp-import-audit-2026-10-16.md` (the .spp
import audit). Your job: RE-VERIFY every row of that census against
TODAY's tree — the census is eight days and ~112 commits stale, and
the night's kills closed named gaps (the per-set masked bakes'
auto-mapping, the stencil lane, the set-switch glue).

For EVERY SPP capability in the census (read its Tier tables first,
then walk):

1. The census's 10-16 state (closed / gap / partial).
2. TODAY's live state: grep the implementation surface (file:line)
   — closed, still-gap, partial-with-boundary.
3. The delta: what landed since the census (cite the git log line
   if findable, or the tree's own doc-comments naming the landing).
4. A verdict per capability: **PARITY**, **EXCEEDS** (Umber does
   what SPP cannot: the JSONL agent, the node graph's MaterialX
   interchange, per-channel resolution...), **PARTIAL** (name the
   exact boundary), or **GAP** (name the missing half).

Cover AT LEAST these SPP capability families (extend from the
census's own structure):
- Painting: brushes, stamps, alphas, stencil/projection, symmetry,
  drag-rectangle/straight-line strokes, the particle brushes class.
- Fill/procedural: fill layers, smart materials, filters, the
  generator/mask stack (anchor relationships).
- Masking: paintable masks, geometry masks (per-mesh/tile
  selection), ID-mask selection, the cavity/ao generators.
- Layers: stack types, groups, opacity/blend, instancing,
  per-channel stacks, texture sets per material.
- Baking: the map list, cage/skew/margin controls, per-set bakes.
- Viewport: PBR rendering, IBL, postfx, display modes, the camera
  set (ortho/persp/DoF).
- UI: the panels SPP ships (Layers/Textures/Assets/History/
  Properties/Display), the shelf, drag-drop, the i18n story.
- Export/import: the format list, presets, the .spp/.psd/.sbsar
  boundaries (the .sbsar execution is a hard NEVER — cite
  docs/ROADMAP.md's Never section).
- Automation: scripting (SPP's python plugin API) vs the JSONL
  agent protocol.

## The output

Write `docs/loop/audit-2026-10-24-lens-spp.md`:
- The parity scorecard table: SPP capability | Umber today (with
  file:line) | verdict | the boundary if partial.
- The census-delta section: rows the 10-16 census called open that
  are NOW closed (with the landing evidence), and any row that has
  REGRESSED or was over-claimed.
- The honest EXCEEDS list — where Umber is genuinely ahead.
- The remaining-gap list, ordered by user pain (a painter's eye,
  not an engineer's).
