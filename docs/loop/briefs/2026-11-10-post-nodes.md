# Kill brief: wave-5 the four post nodes (R-E5)

- Lock: `razul-post-nodes` over:
  - `crates/umber-graph/src/nodes/` (new node module(s) + registry)
  - `crates/umber-app/src/postfx_panel.rs` (panel slots)
  - `assets/i18n/en-US.ftl` + `assets/i18n/pt-BR.ftl`
- Census source: docs/research/spp-parity-census-2026-10-16.md §16 (R-E5): "glare, lens flare, lateral aberration, film grain ABSENT; compositor slots exist". PostStack already has DoF/bloom/vignette/sharpen/tonemap.

## Build

Add the four missing post-process nodes to the graph/postfx chain, each small and deterministic on CPU (gpu routing may be Cpu(...) like existing synth nodes):

1. **Glare** (threshold + soft-knee bloom-ish highlight glow), 2. **Lens flare** (anamorphic-ish streak from highlight threshold), 3. **Lateral (chromatic) aberration** (radial RGB channel shift by distance from center), 4. **Film grain** (deterministic seeded noise added per frame — seed as param so replays are exact).

Each: node_def registration in the registry, `ParamValue` params with defaults, CPU eval in umber-graph (or umber-paint if that's where post ops live — CHECK postfx_panel.rs + the existing PostStack nodes for the real home first), a panel slot in postfx_panel.rs, ftl keys in both catalogs, unit tests with golden/property assertions (deterministic given seed).

## Rules

- Edit only the locked surface. No changes to existing nodes' behavior. Verify: `cargo test -p umber-graph` (+ app if touched), clippy -D warnings on touched crates, rustfmt, ftl completeness both catalogs. No commit/push.
- If one of the four turns out non-small, land the other three and name the deferred one in the summary with the reason.

## Dragon verification

Battery + commit (all payload paths locked) + safe-push.
