# Umber Wave Plan — Substance-Parity March (2026-10-12)

Standing goal: feature-parity with Substance Painter. Wave protocol: dispatch
wide (4-5 claws in per-kill worktrees), land in waves (dragon gates each
branch under the flock, fast-forwards, CI runs per landed branch), no serial
ceremony.

## Wave 1 (IN FLIGHT, dispatched 2026-10-12 ~20:30)

| Kill | Surface | Parity target | Status |
|------|---------|---------------|--------|
| wave-masks | layers.rs/paint_state/compositor | Real mask pixels (critical 6) | claw writing |
| wave-blends | umber-core blend engine + WGSL | Full ~32-mode blend census | claw writing |
| wave-history | history_panel/document/main | Click-to-time-travel | claw writing |
| wave-dist | flatpak/ appimage/ + CI | Linux packaging beyond shell installers | claw writing |

Landing order: dist (independent, artifacts) → history (app-only) → masks →
blends (masks and blends both touch the compositor — land masks first, then
blends rebases onto it if the claw arms collide; both authored in isolated
worktrees so the collision is at most a rebase).

## Wave 2 (planned, dispatch when wave 1 lands)

| Kill | Surface | Parity target | Notes |
|------|---------|---------------|-------|
| wave-postfx-persist | postfx_panel + project save | Post-effects stack project-persisted (the C1 audit's 1c re-mark: session-only today) | The chain is fully wired (postfx_viewport.rs); the kill adds the persistence + the panel state round-trip. Small, independent. |
| wave-pwys | viewport + paint buffer | Mari-style paint buffer: the camera CAN move mid-stroke (b2306c6's named follow-up) | The live-raycast projection exists; the buffer decouples stroke-space from camera. Design-doc first — it's the deepest architectural row left. |
| wave-perchan-res | paint_state set_sizes | Per-channel resolution (the deferred half of the per-set sizing row) | Builds directly on the tilepool's channel dimension — trivially dispatchable once masks/blends land (they touch the same file; sequence AFTER wave-1 lands). |
| wave-ocio-panel | display_panel + ocio bridge | ACES 2.0 configs SELECTABLE in the Display panel (§9 row's "in panel" clause; the bundled build + built-ins landed, the panel wiring is open) | ocio.rs's real-mode APIs are verified live (60b04d0); the kill is panel UI + config adoption into the display chain. |

Wave-2 briefs get written when wave 1 lands (the anchors shift with every
landing; stale briefs teach claws wrong line numbers).

## Wave 3 (candidates, priority order by parity value)

1. **PSD export** (the designed-but-deferred row: the ag-psd reader exists;
   the export side gives round-trip with the industry interchange format —
   Substance-parity's collaboration story).
2. **The full HSV blend GPU goldens** (if wave-blends lands CPU-oracle only,
   the GPU arm's family goldens ride here).
3. **8K export path** (§6's single-source-of-truth row: the 8K-only-VRAM
   note + export-time supersampling).
4. **Dynamic material layering** (the 6+ P2 row — the deepest Substance
   feature left; design-doc first, likely wave 3's big kill).
5. **WASM plugin runtime** (wasm-plugins-design.md exists; the
   custom-shaders row's substrate).

## Never (hard walls, per requirements)

- .spp (undocumented HDF5) — no.
- .sbsar execution (Adobe Developer Additional Terms) — no FOSS license
  exists. Shader-graph parity arrives via MaterialX + WGSL plugins.

## The parity scoreboard (tracked per wave in requirements.md deltas)

Substance's core loops vs ours after wave 1: painting (✓ complete), texture
sets + channels (✓ + per-channel pool landed), layers + blending (wave-1
blends closes the census; masks close the gate), baking (✓ complete),
viewport modes (✓ + height→normal live), stylus (✓ XI2 + pressure;
winink-Wintab dual = open), export/import (PSD = wave 3), history (wave 1),
distribution (wave 1 + MSI on tag). Remaining named gaps after wave 1:
per-channel resolution (wave 2), dynamic materials (wave 3), the Mari buffer
(wave 2), 8K export (wave 3).
