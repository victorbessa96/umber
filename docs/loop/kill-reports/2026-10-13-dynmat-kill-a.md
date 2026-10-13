# Kill report — Dynamic Materials, Kill A (headless implementation)

Date: 2026-10-13. Contract: `docs/specs/dynamic-materials-design.md` r2
(READY TO IMPLEMENT), read in full before any write. Lock `dynmat-killa`
(pre-registered; `kill-locks.sh` not called). NO cargo, NO commits per the
brief — the battery owns fmt/clippy/tests.

## 1. Built

**`umber-core`**
- `src/layers.rs`: `LayerKind::Material` plain tag (payload never rides the
  kind); `MaterialParams` (`material_id` + `channels` + `params`),
  `MaterialChannel` (`name`/`blend`/`opacity`/`enabled` + `named()` defaults
  Normal/1.0/enabled), `MaterialParamOverride` (`node_id`/`param`/`value`),
  `MaterialParamValue` six-variant serde mirror (Float/Vec2/Vec3/Color/Int/
  Bool); `Layer::material` (`#[serde(default, skip_serializing_if =
  "Option::is_none")]` — re-saved non-material layers carry no key, unlike
  the `fill: null` precedent); `LayerStack::set_material` (inert-on-
  wrong-kind, the `set_fill` convention); `LayerCommand::SetMaterial` +
  builder + apply/revert (the `SetFill` shape, incl. `None`-before-means-
  absent-before); `check_paintable` Material arm →
  `InstanceError::PaintOnMaterial { layer }` (single field, never `source`);
  `memory_bytes` honesty (+ `material_bytes` helper: id + channel/param
  names + vec storage; definition mtlx and eval caches explicitly excluded).
  `add_layer`/`add_layer_with_id`/`add_instance` init `material: None`.
  Tests (4): `set_material_round_trips_through_undo_and_json` (undo/redo,
  JSON round-trip, pre-material legacy load, no-`material`-key pin, stray-
  write-inert), `material_channel_named_carries_the_definition_wins_defaults`
  (incl. the can-fail `opacity != 0.0` anti-Default pin),
  `check_paintable_rejects_material_layers_with_a_typed_error` (§5.8 unit
  half), `material_memory_bytes_counts_heap_without_double_counting_tiles`.
- `src/journal.rs`: `JournalOp::SetMaterial { id, material }` + the three
  exhaustive arms (`applied` reads post-apply state, `reverting` carries the
  captured old, `apply_to` sets or clears) + the `Set*` doc-list line.
  Test (1): `set_material_journals_absolute_state_and_replays` (Add + set +
  undo-clear + redo through a live session; committed-line shape; crash
  recovery reaches the live end state).
- `src/project.rs`: `ProjectModel::material_defs_mtlx: BTreeMap<String,
  String>` + `with_material_defs_mtlx` builder + `ProjectFile` carry
  (`#[serde(default)]`, version STAYS 1). Tests (2):
  `pre_material_project_loads_load_identical_at_version_1` (hand-written old
  file: same layers/kinds/ids, `material: None`, empty map, version 1,
  re-save writes no `material` key) and
  `material_layer_and_definition_round_trip_exactly` (params + definition
  exact; save→load→save byte-identical).
- `src/lib.rs`: re-export of the four new types.

**`umber-graph`**
- `src/material.rs` (NEW): `MaterialOutputNode` (passthrough impl — echoes
  its single input UNCHANGED, Image or Uniform alike; `channel` must be
  `Asset` → `BadParam`, 0 inputs → `MissingInput`, n ≠ 1 → `BadParam`) +
  `register_material_nodes`. Tests (3): image echo byte-exact, Uniform echo
  without failing, channel/arity rejections.
- `src/lib.rs`: `pub mod material;` + public `full_registry()` (seed +
  generator + filter/color + mask/spatial + anchor + synth + material — the
  one shared registry for panel and driver). Test (1):
  `full_registry_holds_every_family_including_material_output` (all nine
  representatives dispatch + a material chain evaluates green).
- `src/mtlx.rs`: `material_output` declared nodedef beside `anchor`
  (`input: color3`, `channel: string`) + module-docs table row. NOT added to
  `KNOWN_STANDARD_NODES` (declared customs aren't listed — the anchor
  precedent). Test (1): `material_output_declares_and_round_trips`
  (nodedef-before-nodes, channel-as-string, parse-back equality,
  byte-identical re-emit).

**`umber-app`**
- `src/document.rs`: `LayerKind::Material` arm in the kind-label match
  (`tr("layers.kind-material")`, renders as a normal row).
- `src/history_panel.rs`: `HistoryKind::AddMaterial` (+ `from_command` arm)
  and `HistoryKind::SetMaterial` (+ `from_command` + `label()` arms);
  `from_command_labels_every_variant` extended (Material add + SetMaterial
  label pins).
- `src/graph_panel.rs`: private `full_registry()` now delegates to
  `umber_graph::full_registry()` (sharing contract documented). Test (1):
  `panel_registry_shares_the_material_nodes` (registry lists
  `material_output`; a uniform→material_output chain evaluates in-panel to
  exact blue bytes).
- `src/material_layer.rs` (NEW, ~1150 lines + ~1500 lines of tests):
  `MaterialError` (10 typed variants, whole-material no-op semantics);
  `material_param_to_graph` bridge; `discover_material_outputs` (id-sorted,
  duplicate/bad-param checks); `validate_material_channels`;
  `apply_material_overrides`; `resample_image_rgba8` (bilinear texel-centre
  + `normal`-name renormalize-before-quantize + `length < 1e-6` guard);
  `resample_mask_nearest`; `composite_material_channel` (one composite per
  channel over `composite_pixel_masked`, opacity product formed first);
  `MaterialLayerState(s)` (`ensure` runtime-dirty / `ensure_open`
  project-open-clean / `mark_dirty` explicit-refresh hook);
  `MaterialTileIo` headless seam trait; `refresh_dirty_materials` driver
  (call-order refresh, ghost-publish guard, invisible-keeps-dirty, per-tile
  vs shared caches, whole-material-`Err`/per-channel-skip split).
  Tests (24): bridge (6 variants); §5.1 declaration golden (red + gray-128
  exact, metallic byte-identical + never read/published); §5.2a (170 +
  ≠97/≠139 sentinels); §5.2b (171 bottom-to-top, 187 reversed); §5.2c (144
  re-eval compounding); §5.3 Saturation exact [128,0,0] + NormalCombine
  whiteout [248,128,170] + forbidden-blend skips with publishing sibling;
  resampler identity/2×2→4×4-table/guard-rows/mask-nearest + the
  channel-override end-to-end (170 through a 1×1→2×2 resample + mask
  resample) + gated-mask no-op; discovery/validation units; override
  apply/missing-target units; 7 never-crash cases (truncated mtlx, missing
  id, unknown node_def, unknown channel, duplicate channel, bad override,
  `material: None`) each with byte-identical tiles incl. good-result-stays;
  Uniform-at-one-output per-channel exception; override-reruns-only-dirty
  (run-counter [1,1]→[1,1]→[1,2]→[2,3] with byte pins); channel-only-edit
  cache reuse; other-layer-no-dirty; size-change invalidation; open-clean
  vs runtime-dirty; anchor per-tile (distinct bytes per tile, 2 caches);
  shared path (no anchor reads); mesh-map per-tile + missing-map whole
  no-op; invisible-keeps-dirty-then-publishes; deleted-drops-silently;
  ignored-undeclared reported.
- `src/lib.rs`: `pub mod material_layer;` (paint_state/retouch precedent —
  reachable/testable without the shell; `main.rs` `mod` line is Kill B's).
- `assets/i18n/en-US.ftl` + `pt-BR.ftl` (additive only): `layers`
  `.kind-material` (material/material), `history` `.op-add-material` (Add
  material layer/Adicionar camada de material) + `.op-set-material` (Set
  material/Definir material).

Test count: 4 + 1 + 2 (core) + 3 + 1 + 1 (graph) + 1 (graph_panel) + 24
(material_layer) + history_panel extension = 36 new/changed test fns (the
never-crash-good-pixels case folds 3 failure modes into one test).

## 2. Judgment calls (all also in module docs)

1. Plain-tag `Material` + params-only-in-`MaterialParams` + six-variant
   mirror + definition-wins-on-drift + overrides-replace-any-kind: per doc
   §1.1, kept verbatim.
2. `PaintOnMaterial { layer }` single field: no redirect target exists;
   `source` name avoided for the thiserror trap.
3. Keyed `material_defs_mtlx` sibling map over `graphs_mtlx` (panel-owned,
   unkeyed); version stays 1; `skip_serializing_if` (no `fill: null`
   repeat).
4. `material_output` passthrough echoes ANY single input regardless of port
   name (the `passthrough` leniency precedent); unwired node →
   `MissingInput` (whole-material no-op downstream).
5. `full_registry()` in `umber-graph` (not the app): panel + driver cannot
   drift.
6. Driver-against-`MaterialTileIo` (not `PaintState`): the headless seam.
7. Per-tile cache on `anchor`/`mesh_map_generator` NODES **or any `Anchor`
   param value** (doc names the node case; params ride the same per-tile
   externals — sharing would serve tile A's anchor bytes to tile B).
8. Invisible-dirty layers KEEP dirty (visibility never flows through
   `ensure`); failed refreshes clear layer-dirty but KEEP engine-dirty;
   channel-only edits reuse warm caches; `publish_channel == false` not
   modeled in the report (Kill B follow-up if needed).
9. `mark_dirty` public hook = the doc's Kill-B "explicit refresh material"
   action, built now (the re-eval test needs it).
10. Uniform-absent internal error → per-channel `Internal` failure rather
    than panic (allegedly unreachable, kept loud).

## 3. Deviations from the doc (READY-doc deviations need reasons)

1. **Third ftl key pair `history.op-add-material`.** The doc's Kill A
   surface says "the 2 ftl keys" (`layers.kind-material`,
   `history.op-set-material`), but the `Add { kind }` exhaustive arm needs
   a new `HistoryKind::AddMaterial` variant whose `label()` arm must return
   a real string — mapping Material→an existing kind would lie in the log,
   and a hardcoded literal violates house rule 7. Added en-US "Add material
   layer" + pt-BR "Adicionar camada de material"; Kill B still owns all
   panel strings.
2. **`set_channels` thread-through.** The doc's driver sketch names the
   inputs but not the exact signature; `refresh_dirty_materials` takes
   `set_channels: &[String]` for build-time validation (no TextureSet dep —
   umber-app already depends on umber-core; taking names keeps the driver
   decoupled from the document model).
3. **No `requirements.md` row mark.** AGENTS.md rule 9 marks rows on
   completion; the §2 P2 row needs Kill B (frame wiring + UI). Marking now
   would claim a closed row with no publish path. Left for the dragon on
   Kill B completion.
4. **`umber-gpu` untouched (considered, deferred).** `GpuRoute::route`
   has a `_` catch-all, so `material_output` needs no new arm to compile;
   on the GPU panel path it routes `Cpu(PluginGuest)` and fails honestly.
   Routing it as a CPU echo belongs to the GPU-eval follow-up the doc
   already names ("routing material eval through the GPU tile evaluator is
   a follow-up"), and umber-gpu is outside the Kill A surface.
5. **Resample quantizer spelling.** The doc says "each lane `round`-
   quantized"; the code routes through `umber_gpu::f32_to_u8` (=
   `(v*255).round()` + saturate), the compositor's quantizer the doc itself
   cites (`compositor.rs:645`) — identical for in-range values, fail-closed
   out of range.

## 4. Unverified (no cargo per the brief — battery owns all execution)

1. **Everything compile-level**: all new code is hand-checked (exhaustive-
   match census re-run: only `layers.rs`, `journal.rs`, `document.rs`,
   `history_panel.rs` match; constructors elsewhere), but `cargo check`,
   `clippy -D warnings` and every test listed above are UNRUN. Highest-risk
   spots: `material_layer.rs` borrow choreography in
   `refresh_dirty_materials`/`refresh_one` (disjoint-field + NLL reasoning
   in §1 of this report's review), the `let-else` + `matches!` + struct-
   update syntax in tests, `vec![pixel; n].concat()` (`Vec<[u8;4]>` →
   `Vec<u8>`), `(0..48).collect::<Vec<u8>>()` literal inference.
2. **Golden bytes**: derived with an f32-exact Python replica of the Rust op
   order (`dynmat_goldens.py`, scratch dir): 170/139/97/171/187/144,
   Saturation [128,0,0], whiteout [248,128,170] (margins 0.16/0.34/0.36 —
   the R margin is the thinnest; association-order f32 noise is ~1e-7 so it
   holds iff the Rust whiteout op order matches `blend_rgb`'s, which the
   driver calls rather than reimplements), the 4×4 R table (all ≥ 0.25 from
   boundaries), guard rows, mask blocks. If any pin is off by 1 LSB the
   arithmetic — not the model — needs a look.
3. **GPU/runtime behavior**: all pool seams (`read_channel_tile` None
   semantics, `publish_fill_channel` materialization, set/channel sizes)
   are trait-abstracted and scripted in `FakeTileIo`; the real `PaintState`
   adaptation is Kill B's and untested here by construction.
4. **`graph_canvas.rs`**: reads `painter_nodedefs()` dynamically — the new
   entry flows into its customs automatically; canvas rendering of a
   `material_output` node is untested (egui drawing untested by
   construction).

## 5. Deferred (recorded here + module docs; Kill B owns per the doc)

Frame wiring (`main.rs` mod line, refresh beside the fill refresh at
`size_of_set`, project-open clean seeding, `check_paintable` at stroke
begin + stroke rejection test); Layers-panel expandable sub-rows; Graph-
panel "Save as material"; error badges + status lines (consume
`MaterialRefreshReport`); remaining ftl keys; per-layer pool storage,
multi-entry sub-stacks, pixel-exact undo, live-during-stroke params,
instancing/inheritance, linked definitions, asset-store library, cross-set
+ nested materials, per-channel masks, custom-WGSL authorship, texel-exact
per-size eval, fill compound-on-open bug (all §6 follow-ups, untouched).

## 6. Gates run

- `cargo fmt` / `cargo clippy` / `cargo test`: NOT RUN (brief: battery
  owns them; NO cargo). Long-line scan by hand (2 code lines wrapped;
  remaining >100-col lines are string/assert payloads rustfmt leaves).
- `bash scripts/verify-kill.sh` on all touched files: PENDING at report
  time — run record appended below before finishing.
- `git status` surface check: 14 files (6 locked + resampler module +
  graph/lib/mtlx/core-lib/app-lib/ftl ×2) + this report; NO commits.

### verify-kill record

(pre-finish append)
