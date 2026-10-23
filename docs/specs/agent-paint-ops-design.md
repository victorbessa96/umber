# Agent paint ops — design (wave 7 row `wave-agent-paint-ops`)

Status: BUILT 2026-10-14 (CI 38653177537 green both platforms; Gate 1 battery green), dragon-authored — `crates/umber-cli/src/agent.rs` paint steps + `tests/agent_paint_ops.rs` (6 test functions covering the 7 cases below: case 3 rides the stroke/read test, case 6 the validation test). Built on the umber-paint
extraction (`dbc3d1f` engine, `989ba7f` persistence — both uncompiled
until Bessa lifts the cargo hold).

## Goal

An agent driving `umber-cli agent` can paint: stroke a layer, read the
result back, undo it, and save a project whose paint survives a reopen in
the GUI — through the SAME engine and persistence the shell uses. No
second paint implementation.

## The session gains a paint half

`ProjectSession` grows one optional field:

```rust
paint: Option<PaintHalf>,

struct PaintHalf {
    state: umber_paint::paint_state::PaintState,
    /// The §8 snapshot the end-of-step flush composites against (the
    /// shell's `build_composite_snapshot` shape; see "Snapshot" below).
    prev_snapshot: Option<CompositeSnapshot>,
}
```

Created lazily on the first paint step (a project that is only censused
never touches the GPU). Device: the paint compositor needs
`TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES` (read_write storage textures),
which `bake_context` does not request — a separate `paint_context()` asks
for it and fails typed ("adapter lacks read-write storage textures") on
adapters without it. Lavapipe has it (CI).

`project_open` with paint PNGs in the sidecar (`list_paint_tile_names`
non-empty — a sizes-only `paint/` costs no device) loads it immediately into
a fresh `PaintHalf` (so `paint_read` after open sees the saved pixels),
mirroring `open_project`: `set_active_set`, `clear_layer_planes`,
`load_paint_tiles_reported(paint, dir, live)`, `prune_layer_masks(live)`.
Dormancy seeding is NOT replicated in v1: the CLI evaluates no fills or
materials (see Boundaries), so dormant/dirty only matters to the GUI,
which re-derives it on its own open from the same header.

## Steps

| Step | Fields | Effect | Reply |
|---|---|---|---|
| `paint_stroke` | `layer` (id, or absent = Base), `points` [[u,v], ...] ≥ 1, `color` [r,g,b,a] 0..1, `radius_scale` (optional, default 1.0 — multiplies the session brush's radius via `begin_stroke_styled`) | `sync_active_target(target, verdict = stack.check_paintable(id), index)` then `set_active_target(target)` (explicit, so a refusing layer refuses rather than auto-resolving to Base); `begin_stroke_styled(p0, 1.0, radius_scale)`, `extend_stroke_styled(p.., 1.0, radius_scale)`, `end_stroke`, drain; end-of-step flush. A refusal returns `stroke_refusal` as the typed error | `{ "layer", "tiles": [touched], "refused": null, "can_undo_stroke", "can_redo_stroke" }` |
| `paint_read` | `layer` (absent = composite), `tile`, `channel` (default baseColor), `texels` [[x,y], ...] (1..=64) | `read_plane` for a layer, `read_composite` for the stack (materials contribute nothing — see Snapshot) | `{ "width", "height", "texels": [[r,g,b,a], ...] }` — sampled texels only, never a 64 MiB dump |
| `paint_undo` / `paint_redo` | — | `undo_stroke` / `redo_stroke` + flush | `{ "undone"/"redone", "can_undo_stroke", "can_redo_stroke" }` |
| `project_save` | (unchanged) | + when `paint` is live: `paint_save_version` stamps the model version BEFORE `save_to_dir`, then `prune_layer_masks(live)` + `save_paint_tiles(paint, dir, live, &[])` | `painted: <staged>` (was always 0) |

Points are UV (the paint session's native space; `begin_stroke` takes UV).
UDIM: a point's integer part routes it to its tile (`tile_of_uv`), so
`tile` only matters for `paint_read`. Radius: `PaintState` has no public
radius setter (`set_brush_params` is a test hook); v1 uses the
per-stroke `radius_scale` that `begin_stroke_styled` already takes, so
no engine API changes. An absolute-radius field needs a `set_brush_radius`
pub seam — follow-up.

### Why stroke undo is separate from `undo`

The model's `undo` drives `LayerCommand` histories (per set, journaled).
Paint strokes live on the paint session's own retouch history — that is
how the app works too (two stacks; the History panel interleaves them by
`StrokeCommit` depth). Unifying them over the wire would invent a
contract the GUI doesn't have. v1 exposes both honestly; the reply names
which stack moved.

### Snapshot

The flush needs the stack's rows. `build_composite_snapshot` +
`resolve_instance_source` moved into `umber_paint::snapshot` (pure over
`LayerStack` + `MaterialLayerStates`), so the agent builds the SAME rows
the shell does: it passes an empty `MaterialLayerStates` (it evaluates no
materials, so material rows contribute nothing — true of that session),
`base_visible = true`, and `displayed = []` (no viewport: every composite
happens on read via `read_composite`'s flush-first path). Paint, Fill
(when its plane loaded from the sidecar), and Instance rows composite
exactly as in the shell.

## Boundaries (v1)

- No fill or material EVALUATION in the CLI (the refresh drivers need the
  shell's frame loop; their outputs reload from the sidecar fine).
- No layer masks over the wire (mask strokes are a `mask_edit_layer`
  mode; a `paint_mask_stroke` step is the follow-up).
- No pressure/tilt: `begin_stroke` (pressure 1.0). `begin_stroke_styled`
  is one field away when an agent needs it.
- One texture set's tiles at a time (the active set).
- The paint undo stack does not journal (same as the app: stroke undo is
  session-scoped; the sidecar is the durable record).

## Tests (headless; GPU-gated skip like every paint test)

1. stroke → read: a green stroke (`color` [0,1,0,1]) on Base through
   (0.5,0.5) → `paint_read` at the texel under the stroke centre has
   R = 0 and B = 0 EXACTLY (0 × any coverage is exact) and A > 0; a far
   texel is (0,0,0,0) exactly. Not "== (0,255,0,255)": dab-edge coverage
   is implementation-defined (the Gate-0 journey test's rule), and the
   centre texel's alpha depends on spacing/flow.
2. stroke on a paint layer → `paint_read layer` shows it, Base doesn't.
3. paint_undo restores the sampled texels byte-exact (vs a pre-stroke
   `paint_read` — undo IS exact: it writes the captured before-bytes);
   paint_redo restores the post-stroke sample byte-exact.
4. save → fresh session `project_open` → `paint_read` shows the stroke
   (the sidecar round trip through the shared persistence).
5. stroke on a Fill layer → typed refusal (`PaintOnFill`), session lives.
6. `paint_read` with > 64 texels → typed error.
7. a session that never paints: `project_save` still reports `painted: 0`
   and creates no `paint/` (no GPU device requested).

## Exact stroke input (2026-10-15)

`paint_stroke` points are painted exactly as sent: the session switches the
conditioner to exact mode (`StrokeConditioner::set_exact`), which skips the
one-euro and lazy-mouse smoothing the GUI's pointer input needs. Dab
spacing still follows the brush. Before this, a short wire stroke could
paint nothing (a 2-point stroke never travelled one dab spacing under the
1 Hz filter at the synthetic 8 ms event clock).

## The snapshot arm (2026-10-14, ROADMAP `wave-harness-script`)

`paint_snapshot { out, layer?, composite?, tile?, channel? }` writes the
WHOLE selected plane to a PNG at `out` and replies with its `blake3` (plus
`exists`, `composite`, `tile`, `channel`, `width`, `height`, `out`). Plane
selection is `paint_read`'s exactly, so the two share one implementation
(`Session::read_selected_plane`). A never-written plane writes no file and
replies `exists: false` — that is a never-painted LAYER: Base is allocated
when the paint half initializes, so it always exists (all zero before a
stroke). The PNG is raw (linear transfer, premultiplied as
stored), so the file decodes to exactly the hashed bytes.

This is the harness's `--script` arm in protocol form: a script is just a
JSONL file of steps piped into `umber-cli agent`, with a
`paint_snapshot` after each step it wants to pin. **Pins are relational,
not golden hex** — dab-edge coverage differs across lavapipe, WARP and
hardware, so a hard-coded hash is platform-flaky. The test pins: file ==
hashed bytes, same state → same hash, a stroke changes it, undo restores
it byte-for-byte. A per-platform golden set (one hash file per adapter
class) is the follow-up if cross-machine pins are needed.

### `out_dir` (2026-10-19, ROADMAP `snapshot-dir`) — the deferral tick

The 2026-10-14 report deferred "a `snapshot_dir` convenience (auto-named
files per step index)". It lands wire-shaped as `paint_snapshot`'s
`out_dir`, the second destination arm:

- `paint_snapshot { out | out_dir, layer?, composite?, tile?, channel? }`
  takes EXACTLY ONE of `out` (the file, verbatim — the v1 behavior) and
  `out_dir` (a directory). Both, or neither, is a typed refusal emitted
  BEFORE `read_selected_plane`, so a malformed step requests no GPU
  device and never reports a device/project error instead.
- `out_dir` writes `<out_dir>/<step:05>.png`, where `step` is the
  session's **0-based step index** — the same `index` every reply carries,
  stored on `Session` by `reply` and read here (one counter, two readers;
  no second one added). The rule is the pure `snapshot_out_path`, tested
  directly. The reply's `out` is the resolved path, so a driver reads
  where the file went without recomputing the name.
- `out` became `Option<PathBuf>` (the pair is optional for exclusivity,
  not a default: an absent one is the other arm). `deny_unknown_fields`
  keeps every existing script valid — an absent `out_dir` changes nothing
  — and `PROTOCOL_VERSION` stays 1: 0.1.0 is untagged, so the Beta-freeze
  bump rule has not started (the 2026-10-14 ruling, restated).

## As written — deltas from the draft

- `paint_read` gained `composite` (absent → composite iff `layer` is
  absent); `composite: true` + `layer` is a typed error. A never-written
  plane replies `exists: false` instead of inventing zeros.
- The census's sidecar-size mirror (`SidecarSizes`, a hand-kept copy of
  the app parser with a "drift guard" comment) is GONE: the census reads
  `target_sizes.txt` through `umber_paint::persistence::parse_paint_set_sizes`
  — the parser the app's open path uses.
- The brush color arrives straight and is premultiplied on the way in
  (`BrushParams::color` is premultiplied).

## Order of work (once cargo is allowed)

1. Gate 1 on `dbc3d1f` + `989ba7f` (the extraction): fmt + clippy +
   umber-paint/umber-app suites. Fix forward.
2. Write the CLI half against the compiled engine (this doc's steps),
   tests 1–7, Gate 1.
3. One push + one CI run (the extraction is a big diff; CI on both OSes
   is the real proof).

## The absolute-radius field (2026-10-18)

The named follow-up ("an absolute-radius field needs a `set_brush_radius`
pub seam") closed, wire-shaped: the engine seam already existed as
`PaintState::set_brush_size` (the multiplier, `sanitize_size`-clamped
`0.05..=8.0`), so the kill adds the absolute form on top of it and the
wire field that drives it.

- `PaintState::set_brush_radius_texels(radius)` — the pub seam:
  `set_brush_size(radius / BRUSH_RADIUS_TEXELS)`, so a finite radius
  lands in `1.2..=192.0` texels and a non-finite radius resets to the
  historical 24. A stroke in flight keeps the size it began with (the
  `set_brush_size` rule).
- `paint_stroke.radius` (optional, target texels) — applied before the
  stroke begins. It SETS THE SESSION BRUSH: later strokes inherit it
  until another `radius` arrives (the GUI's brush-size rule — the size
  is state, not per-stroke; `paint_undo` restores pixels, never the
  brush). Absent keeps the current size.
- Validation: non-finite or `<= 0` is a typed error BEFORE any GPU
  work (wire input can only express the `<= 0` half — JSON carries no
  NaN/Inf tokens; the non-finite arm guards internal callers).
- Reply: `paint_stroke` now carries `radius` — the radius actually in
  force for the stroke, the clamped value included (a driver that sends
  1000 learns 192).
- `PROTOCOL_VERSION` stays 1: the bump rule is post-0.1.0-tag only
  (pre-tag, the protocol is fluid by contract).
