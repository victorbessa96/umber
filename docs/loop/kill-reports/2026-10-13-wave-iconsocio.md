# Kill report: wave-2 iconsocio — the icon-boot swap + the OCIO panel adoption

Date: 2026-10-13
Killer: Razul (the dispatched opencode claw died mid-research — its 10-step
budget burned entirely on reads with zero writes; the kills landed by dragon
hands using the claw's brief anchors)

## KILL A — the icon-boot swap (§11, requirements row's NOT-REACHABLE clause)

The boot constructed `IconAtlas::empty()` (main.rs:625); the atlas was
rasterized + loadable since cb4d2bf but nothing consumed it. The swap:

- `UmberApp::new` now reads pixels-per-point from `cc.egui_ctx` (eframe
  publishes it BEFORE app creation — wgpu_integration.rs:246-252) and picks
  the 2x family at native scale >= 1.75 (egui's hidpi convention).
- `load_baked_atlas(&cc.egui_ctx, IconFamily::Tool, hidpi)` at boot; ANY
  failure (missing dir, corrupt PNG, manifest mismatch) logs a warning and
  keeps the empty atlas — every lookup misses cleanly, callers keep their
  text placeholders (icons.rs' standing contract). A missing atlas is never
  a startup error.
- Only the Tool family loads today: the toolbar's ToolRow is the one consumer.
  The Panel family's consumers arrive with the panel-icon wave; its loader
  call lands with them. (Loading both would REPLACE the atlas — IconAtlas
  is one texture, not a family map; the dual-load the claw almost shipped
  was wrong and would have dropped the tool glyphs.)

## KILL B — the OCIO panel adoption (§9 "in panel")

The bridge was live (60b04d0, real-mode tests) but nothing in the app used
it. The adoption, end to end:

- umber-color/ocio.rs: `OcioBuiltinConfig` — a held config (create /
  display_spaces / display_processor). The processor is scene_linear role →
  named display space (probed: ACEScg in both ACES 2.0 configs; a config
  without the role is a hard error, no silent sRGB fallback).
- umber-color/display.rs: `DisplaySettings` grows `ocio:
  Option<OcioDisplaySelection>` (config + display_space; serde-default, so
  pre-OCIO files load as the fixed path — additive). Copy dropped (the
  field is a String pair); the two Copy consumers clone.
  `apply_display_chain_with` threads the processor as the chain's view
  stage — exposure before, gamma after, the same chain semantics;
  `build_display_lut` tabulates through it.
- umber-app: the `ocio` cargo feature (`umber-color/ocio`). The Display
  panel grows the OCIO section: config picker (registry UI names) +
  display-space picker (the held config's spaces), adopting on change; the
  preview strip + the GPU LUT rebuild read the adopted processor
  (`OcioSectionState` caches the config + processor, rebuilt on selection
  change; the selection persists in the project via DisplaySettings).
- Stub arm: `builtin_configs()` empty ⇒ the section renders one disabled
  label with the visible reason (display-ocio.unavailable, both locales).
  No controls, no fake list.
- Real arm (this host, ocio-bundled): the ACES 2.0 CG config enumerates 25
  color spaces, adopts, and tabulates a REAL transform — black stays 0,
  mid-gray sits inside the tone curve (40..200 at i=128), near-white >180
  (pinned in `real_mode_config_tabulates_a_display_lut`).

## Verification

- fmt --all --check: clean.
- clippy -D warnings: umber-color + umber-app, BOTH arms (default + ocio,
  plus ocio-bundled for color): zero errors.
- Tests: umber-app 338/338 (both arms), umber-color 20/20 default, 24/24
  ocio-bundled (the real-mode pair + the new LUT tabulation test).
- The i18n completeness gate passes with the 6 new display-ocio keys
  (en-US + pt-BR).

## Judgment calls

- DisplaySettings lost Copy rather than boxing the selection — the two
  consumers cloned; a Box would have churned the whole panel for nothing.
- The ocio field serializes in every ocio-feature build (null when unset):
  a project saved on a real build loads identically on any other build.
- ocio-rs re-exported from umber-color (`pub use ocio_rs`) so umber-app
  never names the raw dependency — the crate boundary stays one-hop.
- Config switch auto-picks the first sRGB-named display space (probed CG:
  "sRGB - Display", index 0); a config without one keeps the space empty
  and the chain stays fixed until a space is picked.

## Deferred (named)

- The Panel icon family's boot load (needs its consumers first).
- The OCIO GPU path (GpuShaderDesc → 3D textures — display.rs's "future
  per-channel LUT a data change" note anticipates it; the CPU LUT is the
  v1 truth).
- Studio config's space list differs (not pinned by test — only CG is).
