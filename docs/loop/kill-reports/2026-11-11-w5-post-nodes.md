# Wave-5 post nodes: glare, lens flare, chromatic aberration, film grain (2026-11-11)

**Kill:** `razul-post-nodes` (census gap R-E5,
`docs/research/spp-parity-census-2026-10-16.md` §16; brief
`docs/loop/briefs/2026-11-10-post-nodes.md`). Dispatched subagent (claw).

## Landed

New module `crates/umber-graph/src/nodes/post.rs`, exported from
`nodes/mod.rs` with its own `register_post_nodes` (the pattern `synth` set):

| node_def | params (defaults) | CPU eval |
|---|---|---|
| `glare` | `threshold` 0.8, `knee` 0.1, `intensity` 1.0, `radius` 4 (Int px) | soft-knee bright pass on Rec.709 luma, separable box blur, added back |
| `lens_flare` | `threshold` 0.9, `intensity` 1.0, `decay` 0.95, `tint` Color [0.6, 0.8, 1.0] | bright pass smeared along each row with a symmetric exponential falloff (O(n) two-pass recursion), normalized, tinted, added back |
| `chromatic_aberration` | `strength` 0.005, `center` Vec2 [0.5, 0.5] | lateral CA: R samples at `c + d(1+s)`, B at `c + d(1-s)`, bilinear edge-clamped; G and alpha untouched; `s == 0` is byte-identity |
| `film_grain` | `amount` 0.05, `seed` 0 (Int), `size` 1 (Int px), `monochrome` true | triangular noise from a `lowbias32` integer hash of `(seed, x/size, y/size, channel)`; same seed + same input gives identical bytes on any host; `amount == 0` is byte-identity |

All four consume exactly one Image, keep its size, keep alpha, clamp
out-of-range params, and refuse mistyped or non-finite params with
`BadParam`. No existing node changed.

Tests (14, `nodes::post::tests`): soft-knee curve values; glare golden
(radius 0 doubles), symmetric spread, dim-image identity, intensity-0
identity, monotone add; streak recursion equals the direct sum; flare is
horizontal, mirrored, falls off, tinted blue, plus a hand-computed golden
(0.6 grey → 168/173/179); CA golden radial shift on a 5×1 ramp, centre pixel
fixed, G/alpha invariant; grain seed replay byte-exact, seed-sensitive,
literal hash pins and byte pins, zero mean on 4096 samples, bounded delta,
monochrome vs colour, flat 4×4 cells at `size` 4; error paths
(mistyped, NaN/Inf, missing input, Uniform input).

## Deferred

1. **Panel slots in `postfx_panel.rs` + the `postfx.*` ftl keys.** The
   scout put `PostPass` in `postfx_panel.rs`, but it is defined in
   `crates/umber-gpu/src/postfx.rs` (with `PostEffect`, its clamps and
   defaults). The panel's exhaustive matches also cover
   `umber_core::project::PostFxEffect` (the save carry), and every
   `PostPass` needs a GPU pass in `crates/umber-gpu/src/postfx_exec.rs`
   (`PostPass::ALL` feeds the pipeline-kind table). A panel row therefore
   needs `umber-gpu` (model + WGSL pass) and `umber-core` (carry variant +
   serde round-trip), both outside this lock. Adding ftl keys that no code
   reads was skipped, because the locale test names unread keys as stale.
   Follow-up kill: lock `umber-gpu/src/postfx.rs`,
   `umber-gpu/src/postfx_exec.rs` (+ shader), `umber-core/src/project.rs`,
   `umber-app/src/postfx_panel.rs`, both ftl. The four CPU evals here are the
   reference for the GPU passes' parity tests.
2. **`full_registry` wiring.** `crates/umber-graph/src/lib.rs` is outside
   the lock, and `umber-app`'s `registry_listing_covers_all_merged_families`
   pins the registry at exactly 28 defs. Wiring is one line in lib.rs plus
   that pin (28 → 32), and graph_eval's router would send the four to
   `CpuPath::PluginGuest` by default, so it also needs a
   `"glare" | ... => Self::Cpu(CpuPath::GlobalPass)` arm in
   `umber-gpu/src/graph_eval.rs`. Until then the nodes are reachable through
   `register_post_nodes` only.

## Battery

rustfmt --check clean on touched .rs; check-layers GREEN (16 crates); scorecard current; clippy -p umber-graph -p umber-app --all-targets -D warnings clean; cargo test -p umber-graph -p umber-app 0 failed (graph lib 146 incl. 14 new, app lib 315, app bin 788, integration suites all ok).
