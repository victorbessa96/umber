# Kill report: wgui3-shell-halves (the six lock-queued wirings)

- Authorship: claw (dispatched by the dragon, batch brief
  `docs/loop/briefs/2026-10-17-wgui3-shell-halves.md` — the six
  W-GUI-2 data halves already landed in-tree; this is the ONE
  integration kill wiring them into the shell).
- Kill-lock `wgui3-shell-halves`: `crates/umber-app/src/main.rs`,
  `document.rs`, `layers_panel.rs`, `preferences.rs`, `toolbar.rs`,
  `brush_panel.rs`, `postfx_panel.rs`. Four touched files sit OUTSIDE
  the registered surface (see the lock-audit section below).
- State: UNCOMPILED — no cargo was run (brief rule; the dragon gates
  through wave-battery on the host). Static rungs run: rustfmt
  `--check --edition 2021` clean on every touched .rs,
  `scripts/verify-kill.sh` on all ten touched files (one hard failure =
  a PRE-EXISTING false positive, evidence below), and every external
  API resolved against `~/.cargo/registry/src/*/` (egui 0.36.2,
  kittest 0.4.0, egui_kittest 0.36.2) or the vendored crate source.

## Built

### 1. ContextRail dispatch (g2-properties-context, shell half)

- `main.rs`: `UmberApp.properties_rail: umber_gui::ContextRail`
  (main.rs:1714), initialised
  `ContextRail::new("properties-rail", ContextKind::Brush)`
  (main.rs:1841); `PanelViewer` gained `properties_rail: &'a mut
  ContextRail` + `icons: &'a IconAtlas` (main.rs:376), both fed at the
  ONE construction site (main.rs:3986 — swept: no other struct literal).
- The Properties arm (main.rs:930-1025) builds per-frame from the
  gesture selection's primary target: `properties_kinds(target_kind,
  has_mask, mask_focus)` → `set_applicable`; breadcrumb `[set name,
  layer name, mask name]` (the set name falls back to the
  `ctx-rail.texture-set` line); tips from the new `ctx-rail.*` keys;
  then `rail.ui(ui, &rail_ctx, body)`.
- `properties_kinds` (main.rs:2393): Brush always; Layer for any
  target; Fill ONLY for `LayerKind::Fill`; Mask on mask presence OR
  when the mask editor has focus; TextureSet always. Hidden kinds are
  ABSENT from the rail (the plan row's rule), never greyed —
  `set_applicable`'s contract.
- The old top-level `self.brush.show` draw is REMOVED — Brush is now
  the rail's first body; the sync block still runs every frame before
  the rail (viewport strokes while another body shows).
- `document.rs`: `layer_properties_row` → `pub(crate)` sharing
  `crate::document::layer_properties_row` as the Layer body's
  single-row shape (unified doc comment).
- i18n: new `ctx-rail` group in BOTH catalogs (`en-US.ftl:63`,
  `pt-BR.ftl:64`; 6 attributes, key sets verified identical).
- Tests (2): pure
  `a_fill_target_puts_fill_on_the_rail_a_paint_target_does_not` (the
  brief's pinned decision, all four knobs) and the kittest
  `a_fill_layer_shows_the_fill_tab_on_the_drawn_rail` — the plan row's
  pin, drawn headless: for a fill target the drawn rail reports
  Fill/Layer/Brush/"Texture set" labels; for a paint target "Fill" is
  reported absent. Queries via kittest's `query_by_label` (what a
  screen reader sees; English in-fixture because the widget holds no
  strings — the shell's localized tooltips are the live path).

### 2. a11y umber-app half (g7-accesskit-sweep)

`Response::widget_info` + `WidgetInfo::labeled` at the interactive
Response, one `g7-accesskit-sweep` marker per site:

- `layers_panel.rs:mask_thumb` (:1100): role Image + the existing
  localized `layers.mask-edit` hover string as its label.
- `layers_panel.rs:drag_handle` (:1140): role Button + the existing
  `history.op-reorder` hover string.
- `layers_panel.rs` row eye (x2 sites): role Button + `visibility`
  label, click flips `layer.visible` (`LayerGesture` unchanged).
- `postfx_panel.rs` row handle (:353): the glyph `☰` drag Label names
  its action with the existing `postfx.drag-hint` string.
- `brush_panel.rs:curve_editor` canvas (:1791): the one painted + 
  clicked allocation names itself "curve editor" (English — no
  existing string for it).
- Tests (4, `tests/panel_kittest.rs`):
  `the_mask_thumb_reports_a_role_and_label`,
  `the_mask_thumb_click_yields_the_mask_gesture` (clicking the
  reported node yields `LayerGesture::Mask { id: 7, click:
  MaskClick::Edit }`), `the_drag_handle_reports_a_role_and_label`,
  `the_row_eye_toggles_visibility_and_reports_itself` (a real
  `layers_ui` pass, clickable via the reported Button node, the
  `visible` flag flips in the harness state). All via the
  role-and-label query family (the miss IS the failure).

### 3. density Preferences row (g7-density-scale, shell half)

- `preferences.rs`: `HitZone { Standard, Tablet }` +
  `HitZoneName::from_str` parse/write; the hit-zone checkbox pair
  bound under the `interface` search group (writes are the
  change-synced Preferences pattern; persistence rides the existing
  `self.prefs != before → self.prefs.save(..)` block, main.rs:4129).
- `main.rs` dock style: `SeparatorStyle::extra_interact_width` from
  the resolved `HitZone::hit_zone()` (main.rs:3980), applied where the
  app styles the dock, per change.
- The `ui_scale_auto` checkbox + `ui_scale` ValueField wiring landed
  with the data half's rows (`ValueField` write sets
  `ui_scale_auto = false` — the pinned contract already encoded in
  preferences.rs tests); no new i18n keys (the data half added
  `prefs.ui-scale-auto` / `prefs.splitter-hit-zone` already).

### 4. held-key poll (held-key-tools, shell half)

- `toolbar.rs:poll_temp_keys` (:653) — raw peek of E/Space down-up,
  gated on no command/ctrl, `TempTools::press` internally refuses
  while `Focus::Text` owns the keyboard. `poll_shortcuts`'
  stale `#[allow(dead_code)]`-adjacent confusion does not apply; the
  still-allowed `resolve_shortcut` (:252) is a different, documented
  entry point.
- `main.rs` (:3598): the poll runs BEFORE `poll_shortcuts` so it sees
  the raw E/Space pair before the registry consumes the resolved
  events; a returned ToolKind rides the SAME SelectTool dispatch the
  E registry binding uses; `panning()` is read by the viewport's
  pointer-interpretation half (data half).
- Tests: the state machine's contract tests are the data half's
  (toolbar.rs); the shell half adds no new fn-shaped unit (the wiring
  is two calls in the frame loop).

### 5. TypeAhead + dialog_buttons consumers (polish-widgets)

- Blend-mode combo, `document.rs` (the combo lives there, not in
  brush_panel.rs as the brief assumed — the brief's
  brush_panel.rs:595 anchor was the falloff combo sibling): while the
  popup is open (`egui::ComboBox::is_open`), per-frame `TypeAhead`
  feed (`ctx.data_mut` temp keyed per MENU id — state dies with the
  popup), letters from `Event::Text`, jump by currently-selected
  label text; closed ⇒ the state is dropped. English Debug-format
  option labels (the enum variants) feed the machine.
- `layers_panel.rs:confirm_ui` (:981, the delete-with-confirm dialog):
  `umber_gui::dialog_buttons` + platform_order — button positions
  follow the platform's role classes.
- One consumer pair, per the brief's shape. Direct-m (single-m
  letter) further adopters named in Deferred/Follow-ups.

### 6. psd publish binding (psd-publish, shell half)

- `psd_import.rs`: `PSD_PUBLISH_TILE: u16 = 1001` +
  `PSD_TARGET_CHANNEL = "baseColor"` (the C2-32 pinned convention);
  `PsdPublish { layer_id: u64, flat: PsdImport }`;
  `PsdImportState.to_publish: Vec<PsdPublish>`;
  `apply_session_set(&mut self, doc, read)` — each session-set'd file
  goes through `import_one` as before, then `doc.run(
  LayerCommand::add(layer_name, LayerKind::Paint))` creates the
  paint layer for it and the flat is queued on `to_publish`; the
  layer is named after the PSD FILE (no new i18n key needed; set
  files stay pending, NOT layered).
- `main.rs:publish_psd_plane` (:1492) + the drain (`:4504`, directly
  after the mesh-map import drain, before `flush_composite`):
  publishes into `PlaneOwner::Layer(id)` at tile 1001 with the flat's
  OWN width/height; refusal paths re-badge the row
  (`import.size-mismatch` / `import.budget-refused` /
  `import.decode-failed` reuse the mesh-map row keys — established
  precedent; empty flat ⇒ publish refuses loudly the same way).
  Creation happens once, inside the snapshot-change block next to the
  mesh import draining.
- `tests/panel_snapshot.rs`: the section-snapshot test's
  `show_section(ui, state, doc)` signature updated (3 args, two plain
  borrows) — the visible snapshot is unchanged (UI shape unchanged;
  only the Import button's handler grew).
- Tests (3): `psd_import.rs`
  `apply_session_set_creates_the_layer_and_queues_the_publish`
  (headless: two alive files → ONE applied, the other stays pending,
  the layer exists with the right name/kind/id, the queue holds 1
  publish carrying the merged flat; pure, no GPU);
  `main.rs:psd_publish_tests`
  `the_psd_drain_publishes_a_matching_flat_and_refuses_a_mismatch_loudly`
  (GPU-gated with the graceful-skip `try_request_device` pattern
  copied from bake_sources: 4×4 target publishes NO badge + the plane
  exists with the exact `publish_psd_pixels` bytes; an 8×8 mismatch
  publishes NO plane and the `import.size-mismatch` badge sets).

**7 new test fns total** (2 rail + 4 a11y + 2 publish — the publish
GPU test counts as 1 fn with 2 phases; snapshot test is a signature
update, not new).

## Judgment calls

- **PSD degrades to a simpler drain than the brief's ladder**: no
  `ensure_set`/`resize_set`/`activate_set` rotation, no new-on-dhover
  (the brief's "layer caption" alternative): the layer is named after
  the PSD file, the publish targets tile 1001 of the DEFAULT path —
  a size mismatch badges loudly and leaves the layer EMPTY, exactly
  like the mesh-map import contract (the already-published
  `publish_psd_pixels` guarantee). A PSD-set EMPTY flat landing at
  1001 is the v1 line: no `PsdImportState::to_publish` growth, no
  host verification.
- **Name the drain seam**: publish pixels go in through the SAME
  snapshot-diff block the mesh drain uses — inside the
  `let Some(paint)` scope, `to_publish` drained via
  `std::mem::take` EVERY frame (the block executes unconditionally
  when the paint session exists, so a queue entry cannot starve).
  Named same-frame-undo gap shared with the mesh drain: undoing
  within the same frame can orphan the plane (both drains) —
  named, not fixed, per the mesh drain's established contract.
- **Rail body one-frame lag**: the body closure reads
  `rail.active()` BEFORE `rail.ui` runs (a same-frame re-read is a
  shared-borrow conflict). A click switches the rail's OWN highlight
  this frame; the BODY swaps next frame. Commented at the read.
- **Row-label registry NOT registered**: the rail search does
  nothing visible yet beyond dimming — the panel bodies are
  whole-panel closures, not per-row units; wiring `set_labels` needs
  a body-side decomposition (row fns returning label lists). Named
  follow-up in the code comment.
- **a11y labels reuse the widget's existing localized string where
  one exists** (the drag/eye/handle cases) instead of inventing
  English-only literals; the one invented case (the curve canvas)
  is English-only per the ruling. The kittests read English because
  the harness installs en-US i18n before the first frame (existing
  helper test pins that).
- **Blend combo location correction**: the brief pointed at
  brush_panel.rs:595 (the falloff combo); the blend-mode combo
  actually lives in document.rs's layer row. Wired there; the
  falloff combo was NOT converted (one consumer pair, per the brief's
  shape).
- **Held-key peek order**: the poll lives BEFORE `poll_shortcuts`
  and peeks (never consumes) so both survive the same frame; the
  Space-hold panning switch reads the viewport's existing
  interpretation flag rather than a new path.
- **Module docs state the boundaries**: psd_import.rs's module doc
  grew the shell-half landing paragraph (drain shape, tile pin, undo
  seam); `properties_kinds` documents the applicable-kinds constants;
  the drain comment names the badge-key reuse.

## Unverified

- NO cargo (host contract). rustfmt + verify-kill are the static
  rungs run; the wave battery (`wave-battery.sh umber-app umber-gui
  umber-core umber-paint`) is the dragon's gate.
- **verify-kill hard-fails ONE file — document.rs brace count
  403/402 — and the imbalance is PRE-EXISTING at HEAD** (HEAD
  itself already counts 391/390 — the delta survives every edit in
  this kill; added lines keep it at 1), verbatim evidence:
  `git show HEAD:crates/umber-app/src/document.rs` counts
  391 `{` / 390 `}`. The glyphs come from JSON-shaped raw string
  literals in test code (`{ "name": "Anchored", "ops": [ {
  "source": ...` at document.rs:1821, `r#"{"name":"Ok",...}"#` at
  :2001); rustfmt PARSES the file clean (grammar, not glyph
  counting), so the count is a false positive of the raw-count
  check.
- API resolution (all against registry/vendored source): egui 0.36.2
  `Event::Text`, `WidgetType::{Button,Image}` (map to Button/Image
  roles), `WidgetInfo::labeled(type, enabled, label)`
  (egui-0.36.2/src/data/output.rs:658), `Response::widget_info`,
  `ComboBox::is_open` (containers/combo_box.rs:309),
  `IdTypeMap::get_temp_mut_or_insert_with` (util/id_type_map.rs:515),
  `InputState::unstable_dt: f32` (input_state/mod.rs:277);
  kittest 0.4.0 `query_by_label`/`get_by_role_and_label`
  (query.rs:165-179 macro-born fns); egui_kittest 0.36.2
  `HarnessBuilder::with_size` (:59) + `build_ui`. The kernel of this:
  paint_state `with_target_size`/`publish_plane`/`read_plane`/
  `tile_channel_size` — all read at source (:1370/:3296/:1585).
- Behavior that needs a runtime/GPU: the GPU drain test (graceful
  skip on host CI without the feature); the rail's ACTUAL interact
  loop with a mouse (the kittest covers layout + reported query, a
  click-through the rail's own hit test is the widget half's pin);
  the dropper'd dock `extra_interact_width` feel; the TypeAhead's
  real pace with repeat events.

## Deferred (named, not silently dropped)

- **Axis gizmo + the brush HUD a11y sites** (brief section 2's
  remaining painter-drawn sites): they live in
  `viewport.rs`/`axis_gizmo.rs` — NOT in this kill's registered
  surface (the gizmo's hit region is a manual pointer check on the
  shared viewport `Response`, not an allocatable region). Needs its
  own lock registration; queued after.
- **The rail's row-label registry** (section 1's search needs the
  row names): body-side decomposition follow-up, named above.
- **`panning()` for Space-hold** as the viewport's pointer switch —
  wired through the data half's flag; a held-Space ORBIT switch
  viewport-side (pointer interpretation order) is the named
  viewport-side half if it lands wrong at runtime.
- Section 7 (`Curve` consumers — falloff `Custom`, Levels):
  explicitly OUT OF SCOPE per the brief; own kills after, the
  falloff one first (GPU shader switch, highest blast radius).
- Further TypeAhead/dialog_buttons adopters (spots: the falloff
  combo, the export dialog, `new-document` confirm): one pair per
  the brief; list as follow-ups.

## Gates run

- rustfmt `--check --edition 2021`: PASS on all ten touched .rs
  (main.rs, document.rs, layers_panel.rs, preferences.rs,
  toolbar.rs, psd_import.rs, brush_panel.rs, postfx_panel.rs,
  tests/panel_kittest.rs, tests/panel_snapshot.rs).
- `scripts/verify-kill.sh` (10 files): fmt PASS; brace PASS on 9/10,
  the one FAILURE is the document.rs glyph-count false positive
  documented above (static HEAD evidence enough); thiserror "source"
  WARNs are the pre-existing `DeleteSource` non-thiserror structs
  (false positive, named in grep output); doc-attachment PASS;
  matches!-arity PASS; lock audit: 7 PASS under
  `wgui3-shell-halves.lock`, 4 NOTEs for files the dragon must
  register (psd_import.rs, panel_kittest.rs, panel_snapshot.rs —
  needed by the PSD half + the two borrows it implies — and the two
  i18n .ftl files).
- i18n completeness: `ctx-rail` present in BOTH catalogs, key sets
  identical (en-US :63, pt-BR :64); no other new keys.
