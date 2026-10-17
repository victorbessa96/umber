# Kill report: g6-overlays-popover (GUI plan G6)

- Authorship: claw (wgui2 batch, KILL C; parallel to g5-tab-overflow on
  main.rs and g2-layer-gestures on layers_panel.rs).
- Kill-lock `wgui2-overlays-popover`: `crates/umber-app/src/viewport.rs`,
  `crates/umber-app/src/display_panel.rs` (+ the two i18n catalogs, which
  the brief's shared rules require for every new string).
- Implements: `docs/specs/gui-plan-2026-10-15.md` row
  `g6-overlays-popover` (B:C3.1-C3.4, P:B10): one Overlays toggle in the
  viewport header plus a popover with wireframe (+ opacity), grid, UV
  seams, face orientation and the channel-name readout.
- State: UNCOMMITTED, no cargo run (brief: the dragon gates on the build
  host). Static checks only: rustfmt `--check` clean on both files;
  every egui/emath API resolved against the pinned egui 0.36.2 registry
  source (`Popup::from_toggle_button_response`, `close_behavior`,
  `align(RectAlign::BOTTOM_END)`, `Button::selected`, `Ui::put`,
  `Painter::layout_no_wrap`/`galley`, `Rect::contains_rect`).

## Built

- `display_panel.rs`
  - `OverlaySettings { wireframe, wireframe_opacity, grid, uv_seams,
    face_orientation, channel_readout }`: manual `Default` (every
    overlay off, opacity `DEFAULT_WIRE_OPACITY = 0.4` = the wire pass's
    default alpha, readout on), `any_on()` (the header highlight; the
    readout does not count).
  - `overlay_rows(ui, &mut OverlaySettings) -> bool`: the popover body —
    checkboxes, the opacity `ValueField` (0..=1, step 0.05, enabled only
    with the wireframe on), face orientation disabled + a visible reason
    while `FACE_ORIENTATION_RENDERS` is false.
  - Module doc section citing the plan row.
- `viewport.rs`
  - `Viewport` gains `overlays`, `overlay_edited`, `seam_cache`;
    `load_mesh` drops the seam cache.
  - `set_show_wireframe`/`set_show_grid` also write into `overlays`
    (the shell's W/G state stays the source of truth); new
    `overlays()`, `set_overlays()` (clamps opacity), `take_overlay_edit()`,
    `set_face_orientation()`/`face_orientation()`.
  - Header: `overlays_header` puts an "Overlays" button at the top-right
    (8 px inset; hidden without a mesh or when the viewport is too small)
    and a `Popup` (close on click outside, bottom-end aligned) running
    `overlay_rows`; an edit updates the draw flags and marks
    `overlay_edited`.
  - Wireframe opacity: `wire_color_for(opacity)` keeps the default rgb,
    replaces alpha (default opacity == `WireColor::default()` exactly;
    NaN = default).
  - UV seams: `uv_seam_segments(&MeshData) -> Vec<SeamSegment>` (pure)
    over `umber_mesh::seam::build_seam_graph`, recovering each cut's
    endpoints from `tri_a`'s corners by bit-exact midpoint match, plus
    both faces' winding normals. `paint_seams` projects
    (`project_to_screen`, `None` behind the eye / outside depth 0..1) and
    draws cyan 1.5 px lines AFTER the mesh, BEFORE the wireframe
    callback; a seam whose two faces both point away is skipped.
  - Face orientation: `FaceSide` + `face_side(normal, to_eye)` (edge-on
    and degenerate = Back).
  - Channel readout: `readout_text(mode, on, tr)` = `display_keys::mode_name`
    for Solo / MeshMapPreview, `None` for Lit/Unlit or readout off;
    `paint_channel_readout` draws it top-left on a dark backing.
- Catalogs: `overlays` message with 9 attributes in en-US and pt-BR:
  `button, tooltip, wireframe, wireframe-opacity, grid, uv-seams,
  face-orientation, face-orientation-pending, channel-readout`.

Tests (10):
- display_panel.rs: `overlay_defaults_are_off_with_the_readout_on`,
  `overlay_popover_rows_round_trip_the_toggles` (kittest: each row flips
  its field and restores, the disabled face-orientation row does not
  flip), `overlay_keys_exist_in_both_catalogs`.
- viewport.rs: `seam_extraction_finds_exactly_the_uv_cut` (two-island
  strip fixture: one seam with exact endpoints; single-island quad, a
  welded variant and an empty mesh give none),
  `face_side_decides_front_back_and_edge_on`,
  `seams_hide_when_both_faces_point_away` (perspective + ortho),
  `seam_endpoints_project_into_the_rect`,
  `wire_opacity_keeps_the_default_color_at_the_default`,
  `overlay_state_round_trips_through_the_shell_setters`,
  `readout_names_only_soloed_modes`.

## Judgment calls

- Plan says the Display panel's view toggles "move here". The tree has
  none in display_panel.rs (wireframe/grid are the shell's
  `toolbar::ViewToggles`, W/G + View menu). So the panel module gains
  the shared `OverlaySettings` + `overlay_rows` (the popover's source of
  truth, reusable by any other surface) instead of losing a section.
  Rejected: a duplicate Display-panel section — two editors of the same
  struct with nothing gained.
- Wireframe/grid ownership stays with the shell; the popover is a
  second editor, reporting back via `take_overlay_edit`. Rejected:
  moving ownership into the viewport, which would need the
  `ViewToggles`/actions.rs path (outside the lock) to change.
- Seam overlay is CPU screen-space lines, no depth test (seams behind
  front-facing geometry still show; whole-back-facing seams are
  culled). Rejected for this kill: a depth-tested seam line pass, which
  is new pipeline work in umber-gpu (outside the lock).
- Face orientation ships the decision fn + flag + a disabled row with a
  visible reason. The mesh pipeline culls back faces
  (`cull_mode: Some(Face::Back)`, renderer.rs), so a tint needs renderer
  work. A toggle that does nothing on screen would mislead the user.
- Readout defaults ON: it draws only while something is soloed, which
  is the plan's "soloed name shown in the viewport" (the open half of
  g6-display-mode-keys' a420e07 note).
- The header button is a corner overlay inside the viewport rect, not
  a new egui panel strip. The contextual strip already sits above the
  viewport in main.rs, and this lock does not hold main.rs.

## Unverified

- Ad-hoc verified (NOT suite-verified): a line-for-line Python port of
  the pure fns (`build_seam_graph` → `uv_seam_segments`, `tri_normal`,
  `face_side`, `SeamSegment::visible`, `wire_color_for`, `readout_text`)
  passes every pinned case of the Rust tests. 9 mutants (keep
  island-internal edges, take the first triangle edge, `>=` / flipped
  side test, ortho using the eye, all-faces visibility, no NaN guard,
  readout ignoring its flag, readout in Lit) all die. The all-faces
  mutant died only on a mixed-facing case that was missing from the
  Rust test; that case is now added to
  `seams_hide_when_both_faces_point_away`. Static: every `tr("overlays.*")`
  key the two files call exists in both catalogs.
- No compile, clippy or test run yet (brief rule). The kittest
  label queries assume `get_by_label` finds egui checkboxes by their
  text, the same as the existing `graph_panel.rs` button test does.
- The popover's live behaviour (open/close, alignment, clicks not
  leaking into orbit) has no automated check; the kittest drives
  `overlay_rows` directly, not the header button over a GPU viewport.
- The seam overlay's on-screen look on a real mesh.

## Deferred / dragon's wiring (outside this lock)

- main.rs (held by g5-tab-overflow): after the viewport draws, apply
  `if let Some(o) = viewport.take_overlay_edit() { self.view.wireframe =
  o.wireframe; self.view.grid = o.grid; }`. Without this, the shell's
  per-frame `set_show_wireframe(self.view.wireframe)` sync reverts a
  popover wireframe/grid toggle on the next frame (the seams/readout/
  opacity rows already work, since only the viewport owns them). When
  this lands, remove the five `#[allow(dead_code)]` markers on the new
  accessors and on `overlay_edited`.
- Persistence of `OverlaySettings` (project or preference): not in this
  plan row.
- umber-gpu: (a) face-orientation tint, meaning a mesh-pass variant
  without back-face culling plus a front/back two-colour mode on
  `front_facing`, then flip `display_panel::FACE_ORIENTATION_RENDERS`;
  (b) a depth-tested seam line pass to replace the CPU lines.
- A snapshot (kittest+wgpu) of the open popover: the plan names kittest;
  the widget-tree kittest above covers the rows; a pixel snapshot waits
  for the wiring.

## Process note

- main.rs showed dirty during this kill (the g5-tab-overflow sibling);
  it was not touched. The catalogs were edited by appending one
  `overlays` block to each file. The sibling kills' briefs do not list
  catalog keys, so a merge conflict is unlikely; check this when
  committing the batch.
