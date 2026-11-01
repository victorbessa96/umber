# Kill report: `gradient_map` graph node — the g4-gradient-ramp widget's first consumer

Date: 2026-11-01 ~12:0x (dragon local, loop wakeup #7). Lock: `g4-gradient-map`.
Envelope: code only, no cargo/tests (Bessa-confirmed this session); rustfmt
(edition 2021) sanctioned and clean on all 8 touched files.

## Scope

The GUI plan's g4-gradient-ramp row named its open tail: "For the
gradient-map filter and generators". This kill lands the gradient-map
filter: a new built-in graph node whose `stops` param is edited by the
existing `umber_gui::RampEdit` widget (the g4-gradient-ramp kill `9fb1066`).

## What landed

`crates/umber-graph/src/lib.rs`:

- `ParamValue::Stops(Vec<(f32, [f32; 4])>)` — `(position, linear RGBA)`
  ramp stops. Host-side only, like `NodeRef`/`Anchor` (see wire notes).
- `eval_ramp(stops, t) -> [f32; 4]` — sorts stops by position, clamps `t`
  to `[0, 1]` (NaN → 0), lerps the surrounding segment per channel in
  linear light (matching the gui widget's Linear mode), holds the nearest
  end stop outside the ends, empty ramp = opaque mid gray.

`crates/umber-graph/src/nodes/gradient.rs`:

- `GradientMapNode` ("gradient_map"): Rec.709 luma of each input pixel's
  RGB bytes → `eval_ramp` → RGBA8 out, alpha passed through untouched,
  output image = input size. Input port `in` (panel convention; first
  input accepted). `stops` absent → default opaque black→white; mistyped
  → honest `BadParam`.
- Registered next to `gradient` in `register_gradient_nodes`; exported
  from `nodes/mod.rs`.

`crates/umber-app/src/graph_panel.rs`:

- `default_params("gradient_map")` = black→white two-stop ramp, so a
  menu-added node evaluates immediately.
- `editor_section` special case mirroring `curves`: the `stops` row draws
  ONE `umber_gui::RampEdit` (per-node push_id) and writes the edited
  stops back through `set_param`. Helpers `ramp_stops` / `ramp_editor`.
- `param_editor` gains a read-only `Stops` arm (exhaustive match).
- Registry-size test 27 → 28, `gradient_map` added to the expected list.

`crates/umber-gpu/src/graph_eval.rs`: `"gradient_map"` routes
`Cpu(CpuPath::GlobalPass)` — the `Stops` param is host-only and the whole
image is the input, so no GPU op in v1.

`crates/umber-wasm/src/wire.rs` + `tests/sdk_mirror.rs`: `Stops` joins
`NodeRef`/`Anchor` in `wire_params`'s host-only filter; encode rejects it
explicitly; the sdk mirror test marks it unreachable on the wire. Built-in
nodes never run as guests, so no SDK `Param` variant is needed.

`crates/umber-graph/src/mtlx.rs`: export arm `Stops(_) => {}` — no
MaterialX equivalent; stops are dropped on mtlx export (the in-app graph
keeps them). Known limit, named here.

## Verification (envelope-limited, honest)

- `rustfmt --edition 2021 --check` clean on all 8 touched files — rustfmt
  parses the whole module trees it enters, so syntax is proven; type
  checking is NOT (no cargo this session — battery + clippy + pinned
  tests owed at the next sanctioned wave push).
- `eval_ramp` algorithm mirrored in Python and exercised on 9 edge cases
  (empty, single stop, unsorted stops, out-of-range t, equal positions,
  NaN t): 9/9 pass.
- Exhaustive-match audit: every `match` over `ParamValue` in the tree
  (wire encode, sdk mirror, mtlx export, panel `param_editor`) carries a
  `Stops` arm; the rest match on `Option`/`MaterialParamValue` and are
  unaffected.

## Open tails (named, not parked)

- mtlx export drops `stops` (no mtlx ramp type) — round-trip loss if a
  gradient_map node is exported to .mtlx.
- No GPU op for gradient_map (CPU GlobalPass) — fine at bake sizes, a
  WGSL ramp op is the follow-up if perf-nightly says so.
- Generators consuming ramps (the row's other half) stay open.
