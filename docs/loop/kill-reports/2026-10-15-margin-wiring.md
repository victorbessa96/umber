# Kill report — margin-wiring (2026-10-15)

- **Author:** claw (subagent), dispatched by the dragon.
- **Lock `margin-wiring`:** `crates/umber-cli/src/main.rs`, `crates/umber-cli/src/batch.rs`,
  `crates/umber-cli/tests/margin_cli.rs`, this report.
- **Source:** `crates/umber-bake/src/margin.rs` module docs (Blender "Margin Type";
  "Composition with Extend": Adjacent Faces first, then `dilation::dilate_map`).

## Built

- `main.rs`
  - `parse_margin` (`extend` | `adjacent`, case-sensitive; any other value is an error
    naming `--margin` and the accepted values) and `parse_bake_args` → `(BakeFlags, MarginType)`
    with `--margin` (absent = `MarginType::Extend`). `parse_bake_flags` is kept as its
    margin-less projection for `export`.
  - `bake-all` and `bake-ao` take `--margin`; usage strings and the no-arg help list it.
    The `wrote …` lines append `, adjacent-faces margin` under Adjacent Faces.
  - `pad_map`: the one UV-padding post-pass. `dilate == 0` → untouched; otherwise optional
    `adjacent_faces_margin(&data, size, size, mesh, graph, dilate)` (error wrapped with
    `adjacent-faces margin (WxH, N px)` context) THEN the existing `dilate_map(dilate)`.
    With no seam graph it is exactly the previous `dilate_map` call.
  - `margin_seams`: builds `build_seam_graph(mesh)` once per bake call, only for
    AdjacentFaces with `dilate > 0`.
  - `bake_mesh_maps_with_margin` (takes a `BakeJob { flags, margin }`) is the body;
    `bake_mesh_maps` keeps its old signature and delegates with Extend; `run_bake` gains a
    `margin` parameter.
  - `BakeFlags` derives `Debug, Clone, Copy, PartialEq, Eq` (needed by `BakeJob` and the tests).
  - Tests (+4): `absent_margin_flag_is_extend_with_default_flags`,
    `margin_flag_parses_both_types_alongside_the_others`,
    `bad_margin_values_are_errors_naming_the_flag`,
    `margin_seams_built_only_for_adjacent_with_nonzero_dilate`.
- `batch.rs`
  - `BakeStep.margin: MarginType`, `#[serde(default = "default_margin")]` (Extend) with a
    `deserialize_with` that goes through `parse_margin`, so recipe and flag accept the same
    names. `run_step` passes it to `run_bake`.
  - Tests (+2): `bake_margin_defaults_to_extend_and_parses_adjacent` (also the design
    example), `bad_bake_margin_is_an_error_naming_the_field_and_path`.
- `tests/margin_cli.rs` (new, +2): runs the built binary on a seamed quad OBJ the test
  writes (margin.rs's `seamed_quad` fixture: two triangles sharing one 3D edge that is a UV
  cut, islands at u ≤ 0.375 and u ≥ 0.625) at `--size 64 --dilate 4 --rays 4`:
  - `adjacent_margin_bakes_every_map_and_differs_from_extend`: adjacent writes 6 PNGs;
    absent `--margin` is byte-identical to `--margin extend`; at least one map differs
    between adjacent and extend, and the position map is among them. GPU-gated
    (`skipping:` on the no-adapter messages). Local run: all six maps differed, no skip.
  - `bad_margin_value_fails_before_any_bake`: non-zero exit, stderr names `--margin` and
    the value, no PNG written. Not GPU-dependent.

## Judgment calls

- **Fixture: a written seamed quad, not `cube_b7400.fbx`.** The brief named the cube, and
  the first run used it: every texel of all six maps came back alpha 0 at 64² (decoded from
  the written PNGs), so adjacent and extend wrote identical empty maps and the discriminating
  assertion failed. Zero coverage on that cube is a separate observation (UVs or loader on
  this fixture), not investigated here — it is a candidate hunt. The quad is the same
  geometry margin.rs's unit tests pin, so the CLI test exercises a known-seamed layout.

- **`MarginType` beside `BakeFlags`, not inside it.** The brief asked for a `BakeFlags`
  field, but `agent.rs` builds three full `BakeFlags { size, rays, dilate }` literals and is
  outside the lock — a new field breaks them (E0063). Rejected: `..Default::default()` edits
  in agent.rs (out of lock). The margin travels as a parameter / `BakeJob`, and
  `bake_mesh_maps` keeps its signature (Extend) for the agent. Follow-up: fold the field into
  `BakeFlags` and give the agent `bake` step a `margin` field in a kill that holds agent.rs.
- **`bake-ao` also takes `--margin`.** It shares `parse_bake_args` and has its own dilate
  post-pass; routing it through `pad_map` keeps one padding path. `export` does not: it bakes
  undilated, and its own flag loop rejects `--margin` as unknown.
- **`margin_px = dilate`.** One knob: `--dilate N` is both the Adjacent Faces strip width
  and the Extend iteration count (Blender's single "Margin" size). Adjacent without
  `--dilate` (dilate 0) is a no-op, like Extend.
- **Recipe parse through `parse_margin`** instead of a serde derive on `MarginType` (which
  lives in umber-bake, out of lock).

## Unverified

- The GPU integration test proves adjacent ≠ extend only where an adapter ran it (locally:
  ran, zero `skipping:` lines). It does not pin WHICH texels Adjacent Faces wrote — the
  per-texel values are margin.rs's unit tests' job; this pins the wiring.
- Why `cube_b7400.fbx` bakes to zero coverage through `bake-all` (see Judgment calls).
- Byte-identity of absent-margin vs extend is pinned only through the binary on one adapter,
  which is the relation that matters (same code path by construction).
- Agent-session bakes stay Extend-only.

## Deferred

- `crates/umber-app/src/bakes_panel.rs` (UI margin-type control) — out of scope.
- Agent `bake` step `margin` field + `BakeFlags.margin` folding (needs agent.rs).
- `docs/specs/cli-batch-design.md` recipe-field documentation (out of lock).

## Process note

- Three scratch dirs from the failed first run remain under
  `~/.hermes/cache/scratch/umber-cli-margin-{adjacent,extend,default}-328854` (the cube's
  empty maps — evidence for the zero-coverage observation). Removal was blocked by the
  host's destructive-command guard; safe to delete.
