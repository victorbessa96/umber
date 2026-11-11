# Wave-5 post nodes wired end-to-end (2026-11-11)

**Kill:** `razul-post-nodes-wiring` (census gap R-E5 completion; follows
`2026-11-11-w5-post-nodes.md`, which landed the four CPU node_defs).
Dispatched subagent (claw).

## Landed

| layer | change |
|---|---|
| `umber-gpu/src/postfx.rs` | `PostEffect::{Glare, LensFlare, ChromaticAberration, FilmGrain}` + `PostPass` variants; `PostPass::ALL` 5 → 9; clamps + `default_for` (defaults = the CPU node defaults); `GLARE_MAX_RADIUS` = 8, `LENS_FLARE_MAX_DECAY` = 0.98 |
| `umber-gpu/src/postfx_exec.rs` | four single-stage WGSL fragment passes (`fs_glare`, `fs_lens_flare`, `fs_chromatic_aberration`, `fs_film_grain`) in `POSTFX_SHADER`, op-for-op ports of `umber_graph::nodes::post` with `textureLoad` at edge-clamped integer texels; `PostStage` + `pack_uniform` arms; `LENS_FLARE_TAPS` = 512 |
| `umber-gpu/src/graph_eval.rs` | router arm `glare \| lens_flare \| chromatic_aberration \| film_grain => Cpu(GlobalPass)` (was the `PluginGuest` catch-all); test-local registry adds `register_post_nodes`, pin 26 → 30 |
| `umber-core/src/project.rs` | `PostFxEffect` carry variants (wire names `Glare`, `LensFlare`, `ChromaticAberration`, `FilmGrain`) |
| `umber-app/src/postfx_panel.rs` | both carry conversions, `effect_label`, param rows (threshold/knee/intensity/radius; threshold/intensity/decay/tint colour button; strength/centre X/centre Y; amount/seed/grain size/monochrome checkbox) |
| `umber-graph/src/lib.rs` | `full_registry` calls `nodes::register_post_nodes` |
| `assets/i18n/{en-US,pt-BR}.ftl` | 12 `postfx.*` keys each, all read by the panel |

### GPU vs CPU judgment calls

- **Tap budgets clamp in the model.** Glare is one `(2r+1)²` gather (no
  separable scratch, so it stays inside the 2-slot pool contract);
  radius clamps to 8 on the post pass (the CPU node allows 256). The
  flare streak gathers at most 512 px each side; decay clamps to 0.98
  (`0.98^512 ≈ 3e-5`, the truncated tail is far below a byte; the CPU
  node allows 0.999). The graph nodes themselves keep their full CPU
  ranges: only the viewport post pass is budgeted.
- **Scene-linear, unclamped highs.** The passes run on the Rgba16Float
  chain, so they do not clamp to 1 the way the byte CPU node does
  (film grain clamps only the negative overshoot, like sharpen). On
  0..1 input the parity tests quantize the GPU output with the CPU's
  `byte_round` and compare.
- **Film grain keys on viewport pixels** (`clip_position`), so grain is
  screen-locked; the seed replays it exactly frame to frame.

## Tests

- Parity (gpu-gated, `postfx_exec::tests::gpu`, real adapter): each pass
  vs its CPU node on a 23×13 fixture with hot spots and varied alpha,
  3-4 param sets per node (incl. knee 0, decay 0, negative CA strength,
  CA strength 0, negative seed, per-channel grain, 4 px cells).
  **Tolerance: ≤ 1 byte per RGB channel, alpha ≤ 1 byte.** Observed
  worst delta on Intel HD 530 (Vulkan, Mesa): 1 byte for all four (CA
  strength 0.2 case: 0).
- `every_pass_records_and_submits` now covers all nine kinds (pins derive
  from `PostPass::ALL.len()`).
- Model/plan: `post_node_params_clamp_at_construct`,
  `post_node_passes_are_single_stage_and_pack_their_params`;
  `defaults_are_inside_the_clamps` now covers the four.
- Carry: `postfx_carry_round_trips_the_post_node_effects` (core, file
  round-trip + byte-identical resave); panel
  `post_node_effects_cross_the_carry_with_exact_params`,
  `chain_from_carry_clamps_post_node_params`,
  `add_effect_dispatches_each_post_node_to_its_pass`.
- Router: `routing_splits_the_param_dependent_arms` pins the four to
  `GlobalPass`; `routing_covers_every_registered_node_def` covers them.

## Lock deviation (named)

`crates/umber-app/src/graph_panel.rs` was edited although not in the
lock list: the brief's `registry_listing_covers_all_merged_families`
pin (28 → 32) lives there, not in `umber-graph/src/lib.rs`. The edit is
the pin bump plus the four names in its expected list; nothing else.

## Deferred

1. `crates/umber-graph/src/nodes/mod.rs` module doc still says the post
   nodes are "not yet folded into `full_registry`" (outside the lock).
   One-line doc fix for the next kill touching that file.
2. No GPU *graph* dispatch for the four nodes: graph evaluation routes
   them to their CPU nodes (`GlobalPass`). A `GpuOp` port would reuse
   this kill's WGSL; not needed for correctness.

## Battery

Worktree, under `/tmp/umber-cargo.lock`, `UMBER_ASSETS_DIR=<worktree>`:

- `rustfmt --edition 2021 --check` clean on every touched `.rs` (7 files).
- `python3 scripts/check-layers.py`: GREEN (16 crates).
- `python3 scripts/scorecard.py --check`: README.md current.
- `cargo clippy -p umber-gpu -p umber-core -p umber-app -p umber-graph --all-targets -- -D warnings`: clean.
- `cargo clippy -p umber-gpu --features gpu --all-targets -- -D warnings`: clean.
- `cargo test -p umber-gpu -p umber-core -p umber-app -p umber-graph`: 0 failed
  (app lib 315, app bin 791, app integration 39, core 234, gpu 156,
  graph 146 + mtlx pins 10).
- `cargo test -p umber-gpu --features gpu`: 224 passed, 0 failed, **no
  adapter skips** (ran on Intel HD 530 / Mesa Vulkan locally; CI runs the
  same suite on lavapipe).
