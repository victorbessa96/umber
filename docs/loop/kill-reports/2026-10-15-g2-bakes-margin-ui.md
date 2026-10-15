# Kill report: g2-bakes-margin-ui (the Bakes panel Margin Type control)

Date: 2026-10-15. Author: dragon. Lock: `g2-bakes-margin-ui` over
`crates/umber-app/src/bakes_panel.rs` and this report. The four `bakes.*`
catalog keys ride inside the live `cavity-wiring` lock's ftl surface: they
are appended to the `bakes =` block only, the cavity kill's hunks sit in
`brush =`, and the commit stages the bakes hunks alone (see Process note).
Plan: `docs/specs/gui-plan-2026-10-15.md` row `g2-bakes-margin-ui`; ROADMAP
6a `wave-margin-ui`, the open panel half re-queued by the margin-ui CLI kill
(`2026-10-15-margin-ui.md`, Deferred).

## Built

- `bakes_panel.rs`:
  - `BakesPanel.margin: MarginType` (default `Extend`, the pre-margin
    behavior) with `margin()` / `set_margin()`; `BakeRequest.margin`
    carries it onto the worker.
  - A Margin type row in the Settings group, right under Dilation: a combo
    over Extend / Adjacent faces with a hover hint. Its width is the
    dilation width, the same coupling the CLI's `--margin` + `--dilate`
    has.
  - `Padding { margin, iterations, size }`: `seams(mesh)` builds the seam
    graph only for Adjacent Faces with a non-zero width; `fill(mesh,
    seams, raw)` runs `umber_bake::margin::adjacent_faces_margin` or
    returns the map untouched. `run_bake` runs it BEFORE every map's GPU
    dilation (the CLI `pad_map` order: adjacency first, or Extend covers
    the strip and leaves nothing to fill).
  - Seam graphs are built at most once per bake: one for the whole mesh,
    one per AO tile. Tile AO bakes a filtered, UV-rebased mesh
    (`filter_mesh_for_tile`), and the margin must read the mesh the map
    was rasterized from, so the tile path builds its graph from that same
    filtered mesh.
  - Auto-rebake: a margin change marks `BakeParam::Dilation`, so it
    dirties exactly what a dilation change dirties (every map).
- `assets/i18n/{en-US,pt-BR}.ftl`: `bakes.margin`, `.margin-extend`,
  `.margin-adjacent`, `.margin-hint`, both catalogs.

## Tests (5 new, in `bakes_panel.rs`)

- `margin_defaults_to_extend_and_round_trips`: default + setter + keys.
- `margin_change_dirties_what_a_dilation_change_dirties`: the fired map
  set equals the dilation change's (and is all four maps); an unchanged
  margin fires nothing. Fails if the tracking block is deleted.
- `extend_padding_is_byte_identical_and_builds_no_seams`: Extend (and
  Adjacent at width 0) builds no graph and returns the input bytes.
- `adjacent_padding_copies_the_partner_island_across_the_seam`: on the
  CLI test's 64 px seamed quad, column 24 takes island B's column-40
  texel and column 39 takes island A's column-23 texel (exact bytes,
  three rows); beyond the 4 px width stays uncovered; covered texels are
  untouched. Extend cannot produce the partner's colour there, so the
  test tells the two strategies apart.

## Judgment calls

- No separate margin width: the dilation width is the margin width, like
  the CLI. Blender has one Margin size too. A second field would let the
  two drift for no gain.
- The margin row is not a `BakeSettingId` jump target. No log entry can
  blame it yet (the margin adds no warnings), so a jump id would be dead.
- `Padding` is a panel-local twin of the CLI's `pad_map` CPU half, not a
  shared function. umber-app does not depend on umber-cli; hoisting it
  into umber-bake is the follow-up if a third caller appears.

## Unverified

- The `show` combo draw itself (egui drawing is untested in this panel by
  construction, see the struct doc). The worker path with Adjacent Faces
  on a GPU is covered for the CLI by `margin_cli.rs`; the panel's
  `run_bake` with `AdjacentFaces` has no GPU test of its own. It is the
  same `fill` the unit tests pin, ahead of the same `dilate_map` call.

## Deferred

- Margin type persisted per project (the panel settings are session
  values today, like rays and dilation).

## Process note

The cavity-wiring kill (a sibling session) holds both ftl files and was
writing its `brush.cavity*` keys while this kill landed. Gate 1 ran in a
detached worktree (`umber-verify-margin`, off 51d9047) carrying only this
kill's bakes_panel.rs and its four `bakes.*` lines, so the verdict covers
this kill alone and not the sibling's in-flight files.
