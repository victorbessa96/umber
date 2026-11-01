# Kill report: roadmap hygiene — four stale plan parentheticals ticked (2026-11-01)

Date: 2026-11-01 ~11:5x (dragon local). Lock: `plan-stale-ticks-20261101`.
Docs-only kill; no code touched, no cargo per envelope.

## What

Four gui-plan rows still carried "open" parentheticals whose kills had
actually landed and are verifiable in the tree today:

- `g2-layer-thumbnails` "mask thumbnails open" → `2525f2f` landed them;
  evidence: `layers_panel.rs::mask_thumb` (the mask thumbnail slot + click).
- `g2-texture-set-list` "per-set visibility open" → `fe7d3d0` landed it;
  evidence: `texturesets.toggle-visible` wired at main.rs:2956 + the
  queue row (eye per set, Show/Hide all/Invert, hidden sets composite empty).
- `g4-valuefield-grammar` "factor-% display open" → percent mode has live
  consumers: `.percent(true)` in document.rs, display_panel.rs (x2),
  path_tool.rs (plus the widget's own test).
- `g6-display-mode-keys` "in-viewport label open" → landed; evidence:
  `viewport.rs::paint_channel_readout` (soloed channel / mesh map name at
  the viewport's top-left).

All four rows are now ticked closed with the landing SHAs / evidence named
in the row.

## Still genuinely open (named, not parked)

- `g4-property-context-menu` Assign Shortcut — needs an action-per-property
  registry.
- `g4-gradient-ramp` first consumer — a `gradient_map` graph node mapping a
  grayscale input through `ramp::Ramp` stops; needs a `ParamValue` stops
  variant (multi-crate: umber-graph + graph_panel editor). Next designed kill.
- `g2-raw-slider-sweep` snapshots for the bin-only panels — baselines are
  generated on the build host before the wave battery (A2 convention).
- True OS-level file drag-out (winit path); desktop-checks x4 (Bessa's
  desktop + a battery-capable session).
