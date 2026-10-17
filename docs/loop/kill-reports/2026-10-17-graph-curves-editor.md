# Kill report: graph-curves-editor (the g4 census CurveEdit consumer)

- Authorship: claw (Claude Code), from the dragon's brief.
- Kill-lock `graph-curves-editor`: `crates/umber-app/src/graph_panel.rs`
  ONLY. No other file touched apart from this report.
- Closes the g4 census finding: umber-gui's `CurveEdit` had landed, but
  the graph's `curves` node still edited its control points as eight raw
  DragValues (four `Vec2` rows).

## Built

- `selected_editor`'s param loop: when `curve_points(def, params)` is
  `Some`, the four `curve_p*` rows are replaced by ONE `CurveEdit`, drawn
  at the first curve param's slot. Every other param still goes through
  `param_editor`, so the loop keeps its shape. A write-back is four
  `set_param` calls (`curve_p0..p3` as `Vec2`), made only when the curve
  changed.
- New private helpers (all in graph_panel.rs):
  - `CURVE_PARAMS`: the four names.
  - `CURVE_IDENTITY`: the thirds identity that umber-graph's `CurvesNode`
    docs pin as byte-exact.
  - `curve_points`: the gate. `def == "curves"` AND all four params
    present as `Vec2`, using the same first-match lookup as graph_eval's
    `find_param`.
  - `curves_editor`: the brush_panel `falloff_custom_editor` shape.
    `push_id(("graph_curves_curve", id))`, then
    `CurveEdit::new(..).size(160.0).fixed_count(true)`, with the model
    rebuilt every frame and a `response.changed()` gate.
  - `curve_edit_model`: brush's `falloff_edit_model` approach verbatim:
    clamp into `[0,1]²`, then nudge x to `MIN_GAP + 1e-6`. It returns
    `(Curve, fallback)`; on refusal it shows the identity with
    `fallback = true`.
  - `sanitize_curve`: the eval contract (finite, x strictly increasing),
    nothing more.
  - `curve_write_back`: does the merge, sanitize and change detection.
- `default_params("curves")` = the identity curve. Without it, a curves
  node added from the menu has no params, so the editor (which keys on
  the four params) could never appear.
- New module-docs section "The curves editor (g4 census consumer)", with
  every judgment call below.
- Tests (5 new, in the existing `mod tests`, plus one helper
  `curve_params`):
  1. `curves_model_round_trips_and_writes_back_the_moved_point` (a). The
     fresh node is the identity, the model shows a valid curve exactly,
     and moving p1 writes back exactly `[[0,0],[0.3,0.5],[0.6,0.3],[1,1]]`.
     The four set_param calls land and mark the node dirty.
  2. `curves_write_back_keeps_the_eval_contract` (b):
     - A stored y of -1 / 2 is shown clamped and survives an edit to p1.
     - A display-nudged p2 writes back its STORED x when p0 is edited.
     - A stored non-monotone curve whose merge is refused writes the
       whole edited curve.
     - The crowded-at-x=1 curve shows the identity and its first edit
       replaces the whole curve.
     - The gate refuses NaN and non-increasing x, and accepts
       out-of-range x/y.
  3. `only_a_complete_curves_node_builds_the_editor` (c). `levels` and
     `gradient` with all four curve params → None. A curves node with
     p3 as a Float, or only three params → None. A `center` Vec2 → None.
     A fresh node of every registry def except `curves` → None.
  4. `curves_write_back_fires_only_on_change` (d). An untouched
     re-render writes nothing for each of four curves: a valid one, a
     clamped one, a nudged one, and the identity fallback. A drag back
     onto the stored point also writes nothing.
  5. `curves_node_replaces_the_raw_point_rows` (kittest). A selected
     curves node has no `curve_p0` label. The same params on a `levels`
     node keep the label; this is the positive control showing the
     query reaches the editor section.

## Judgment calls

- **Write-back follows the eval contract, not the widget's.** graph_eval
  (`GpuOp::Curves`, graph_eval.rs:688-714) and umber-graph's
  `Curve4::try_new` require only finite coordinates and strictly
  increasing x. Neither range-limits x or y, so the sanitize step invents
  no y clamp, as the brief asked.
- **Per-point merge instead of brush's wholesale write.** The brush writes
  the whole clamped model back, because `ControlCurve` is its own source
  of truth. Here that would silently clamp a stored `y = 2.0`, and bake
  the `1e-6` display nudge into the graph, whenever ANY point is dragged.
  So only the point(s) whose edited value differs from the shown value
  take the editor's value; the rest keep their stored value. The point
  actually being dragged is clamped into `[0,1]²`, because the widget
  can't represent anything else; the module docs say so.
- **Fallback when the merge is refused.** This happens when the stored
  curve already broke the contract, e.g. tied x. The whole edited curve
  is written instead, and it is always valid because the editor keeps
  `MIN_GAP`.
- **The identity fallback writes back whole.** Without the `fallback`
  flag, a merge against the identity model would keep the crowded stored
  points, so the editor would show the identity forever.
- **`edited == shown` returns None before anything else.** Otherwise an
  untouched re-render under the identity fallback would "write" the
  identity. The UI is also gated by `response.changed()`, but the helper
  is idempotent on its own.
- **Four set_param calls, all four points**, whenever the result differs
  from the stored curve, even if only one point moved. This matches the
  brief's "four set_param calls".
- **The editor sits at the first `curve_p*` slot**, drawn once even if a
  hand-edited graph carries duplicate names (`curve_drawn` flag).
  Duplicates are hidden behind the editor; both evaluators use the first
  match anyway.
- **No label of its own**: the brush precedent has none, and the node
  title already names the def. No i18n keys were added, so neither
  catalog changed.
- **Size 160 px** (brush's is 120), because here the curve is the node's
  whole body, not a row beside a combo.
- **`default_params("curves")` = identity.** This changes a fresh curves
  node from "empty, eval reports the missing-param error" to "identity,
  evaluates". Checked for conflicts first: no `add_node("curves")`
  anywhere in umber-app or umber-cli, and psd_import's `"curves"`
  (psd_import.rs:1622) is a PSD layer name, not a graph node.
- **A local `CURVE_IDENTITY` const, not `umber_brush::ControlCurve::identity`.**
  The graph panel shouldn't take a brush coupling for one constant. Its
  points are the same thirds that umber-graph's filter.rs uses.

## Blast radius (rung 4)

Registered built-in defs: anchor, blur, brick_pattern, checkerboard,
color_correct, curves, direction_warp, dots, edge_detect, expect_image,
flood_fill, gradient, histogram_match, hsv_adjust, invert, levels,
material_output, mesh_map_generator, mix, noise_perlin, noise_value,
noise_worley, sharpen, synth, triplanar_blend (plus passthrough /
image_asset from the engine seed).

Vec2 params in use today:
- `scale`: pattern.rs (checkerboard/dots/brick)
- `offset`: noise.rs
- `vector`: spatial.rs (direction_warp)
- `center`: gradient.rs, and the vignette plugin
- `resolution`: the generic eval.rs param

Only `curves` carries `curve_p*`, and the special case needs
`def == "curves"`, so none of those can trigger it. Plugins can't take
the name either: plugins.rs:314-322 refuses any already-registered name
("plugins never shadow"), and test `a_plugin_never_shadows_a_built_in`
covers this. So `def == "curves"` is always the built-in.

## Unverified

- **Rung 1 (rustfmt --check): NOT RUN.** This session's sandbox denied
  `~/.cargo/bin/rustfmt` and `scripts/verify-kill.sh` (both "requires
  approval"). I formatted by hand against rustfmt's default widths
  (max_width 100, chain/fn_call/array width 60) and rewrote every
  construct that sat within ~2 columns of a limit, using let-bindings or
  a `unit` closure. The advisor review caught one I had miscounted: the
  `curves_editor` signature was exactly 100 columns on one line. I
  renamed `node_id` to `id`, so it is now a 95-column one-liner. There
  is no `rustfmt.toml`, so rustfmt runs on defaults. The dragon's
  battery fmt pass is the real check.
- **Rung 2 (API resolution): done from in-repo sources.**
  - umber-gui: `Curve::new`, `points`, `move_point`, `Default`,
    `x_window`, and `MIN_GAP` at `umber_gui::widgets::curve` (curve.rs:35-166).
  - `CurveEdit::new/size/fixed_count/show -> egui::Response` (curve.rs:283-306).
  - The crate-root re-exports `Curve` and `CurveEdit` (lib.rs:43-47).
  - `ui.push_id(..).inner` and kittest `query_by_label(..).is_some()` /
    `build_ui_state` follow existing graph_panel/brush_panel uses.
  - I couldn't read the kittest registry source (the sandbox blocks
    `~/.cargo/registry`). Whether `query_by_label` panics on multiple
    matches is unconfirmed; each case here has at most one `curve_p0`
    label. Matching a plain `ui.label` text has in-repo precedent
    (export_dialog.rs:970 `get_by_label` on a reason line).
  - `default_params("curves")` conflict check: no `node_defs()` loop
    (graph_panel.rs:1857/1971, graph_canvas.rs:869) expects a fresh
    curves node to be empty or to fail eval.
- **Rung 3 (Python port + mutation): NOT RUN.** python3 was also denied.
  Instead I traced each mutation by hand through the pinned values:
  - Wholesale write-back (drop the merge): (b) first case gets
    `[0,0]…[1,1]` instead of keeping `-1` / `2.0`. RED.
  - Drop the `fallback` flag: (b) crowded case keeps stored
    `(0.9995, 0.3)` at p2 instead of `CURVE_IDENTITY[2]`. RED.
  - Drop the `edited == shown` early return: (d) fallback re-render
    returns `Some(identity)`. RED.
  - Drop the strictly-increasing check in `sanitize_curve`: (b)
    non-monotone case writes the tie `0.3` instead of the nudged x, and
    the direct gate assert fails. RED.
  - Add a y clamp to `sanitize_curve`: the `wide` assert and (b) first
    case fail. RED.
  - Drop the nudge: `0.51 - 0.5` in f32 is below `MIN_GAP`, so the
    model falls back. `assert!(!fallback)` fails, and so does
    "display nudged". RED.
  - Drop the `def == "curves"` gate: (c) `levels` / `gradient` return
    Some. RED.
- **Runtime/GUI**: the drag feel and egui id stability across node
  switches (the id includes the node id) need a live run. The kittest
  only proves the raw rows are gone; it doesn't drag the curve.
- No cargo of any kind (claw rule 1). Clippy reviewed by hand against
  the CLAUDE.md trap list:
  - `curve_edit_model`'s `for i in 1..len` is the same shape as brush's,
    which already passes the battery.
  - `drop(harness)` avoids `let_and_return` and the tail-expression
    borrow.

## Deferred

- Nothing from the brief. A label row or an i18n key could be added
  later if UX wants one; nothing is recorded for it.
- The requirements row isn't marked: the brief named no row, and this is
  a census consumer, not a requirements row.

## Gates run

- verify-kill: NOT RUN (sandbox denied).
- rustfmt --check: NOT RUN (sandbox denied).
- clippy / cargo: NOT RUN (forbidden for claws; the dragon's battery
  owns it: `bash scripts/wave-battery.sh umber-app`).
