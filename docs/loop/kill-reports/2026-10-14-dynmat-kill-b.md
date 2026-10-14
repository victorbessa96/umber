# Kill report — Dynamic Materials, Kill B (app wiring + UI)

Date: 2026-10-14. Contract: `docs/specs/dynamic-materials-design.md` r2
(READY TO IMPLEMENT), read in full before any write. Lock `dynmat-killb`
(pre-registered; `kill-locks.sh` not called per the brief). NO cargo, NO
commits per the brief — fmt/clippy/tests ride the dragon-side battery.

Kill A is landed (commits `f85cb0a` + `30985d8`, plus `4bd1a4a` lib-surface
and `12413d9` fmt follow-up): the core model, the headless driver
(`material_layer.rs`, unchanged by this kill), and the `material_output`
nodedef. This kill wires it into the shell.

## 1. Built

**`crates/umber-app/src/layers_panel.rs` (NEW, ~640 lines incl. 6 tests)** —
the Layers-panel material surface (doc §4):

- `MaterialPanelContext<'a>`: per-frame context the rows read (session
  `defs`, refresh `badges`, drain `status`, live `states`). Built by the
  shell; `document` never owns any of it.
- `material_section`: badge line (refresh report first; static
  no-params/missing-definition fallbacks for never-refreshed layers) +
  unpaintable hint + disclosure (`CollapsingHeader`, default-open) with the
  `material_id` re-point combo, reset-channels + explicit-refresh buttons,
  and one sub-row per DECLARED channel (name-as-enable-checkbox dimmed when
  off, filtered blend combo, opacity slider). Every widget change is one
  undoable, journaled `Document::run(SetMaterial)` op. Undeclared channels
  are not shown (the ignored line names them); unparseable definitions show
  their error instead of rows. Missing layers return silently.
- Pure headless helpers: `paint_gate_allows` (the stroke gate),
  `channel_blend_options` (never Passthrough, normal-map trio only on
  `normal`, enum order), `declared_channels` (parse + driver discovery —
  panel and refresh read one declaration), plus private `ignored_channel_
  names`, `default_channels_for`, `upsert_channel` (opacity clamps
  0..=1, non-finite → 0.0), `point_at_definition`, `reset_channels`.
- Tests (6): declaration order is node-id sorted; garbage/duplicate
  definitions rejected loudly (`twice` pinned); blend filtering 31 vs 28
  entries with exact membership; upsert update/insert/clamp (1.5→1.0,
  NaN→0.0, −0.5→0.0); ignored-set pin; stroke-gate pin (Material → typed
  `PaintOnMaterial` naming the layer, Paint allowed, Instance + unknown
  rejected) — the §5.8 stroke half's verifiable part.

**`crates/umber-app/src/main.rs`** (only the doc-named sections + the
context plumbing they require):

- `mod layers_panel;` + `mod material_layer;` (the binary names the
  lib-surface driver).
- `AppState`: `material_states`, `material_defs` (session `BTreeMap` copy
  of `material_defs_mtlx`), `material_badges`, `material_status`,
  `material_sizes` (set size + per-channel census). `reset_for_new` clears
  all five (document-scoped — a new document carries no materials).
- `PaintMaterialIo` adapter (`MaterialTileIo` over the live session):
  channel-aware sizes/tiles/readback/upload, mask planes at the tile's set
  size, anchors resolving to the flattened channel with a live-layer guard
  (deleted source → `None` → loud eval error), mesh maps as the Graph
  panel's flat placeholders at set size (white AO, up-Normal, unknown names
  absent).
- Frame drain beside the fill refresh: adopt every doc-side change
  (`ensure` per Material layer with params/definition/set size; `None`
  layers get no state and a static badge), channel-override census with
  `mark_dirty` on change, then `refresh_dirty_materials` bottom-to-top
  (stack order) only when dirty — badges/status pruned against live layers,
  failures and ignored-undeclared logged and surfaced.
- `save_project` commits the session defs map
  (`.with_material_defs_mtlx`); `open_project` restores it and seeds states
  CLEAN via `ensure_open` with the size-census baseline set (no publish on
  open — the sidecars already hold the result).
- `PanelViewer` carries defs/badges/status/states; the LayerStack arm builds
  the context for `layers_ui`; the Graph arm draws `show_material_section`
  after `show`.

**`crates/umber-app/src/document.rs`** — `layers_ui` takes the material
context: an "Add material" button (plain-tag layer, `material: None` until
pointed — the badge explains), the per-row `material_section` hook under
each Material row (re-fetched by id; badges stay visible collapsed), and the
drain status line under the rows.

**`crates/umber-app/src/graph_panel.rs`** — `save_as_material` (copies the
open graph's `.mtlx` into the defs map under `umber.material-<n>`,
smallest-unused, never an overwrite; empty graph → `None`) +
`show_material_section` (button + status line). Tests (2): copy semantics
(stored bytes frozen while the source keeps editing; occupied ids skipped,
stale entries preserved) and the empty-graph no-op.

**`assets/i18n/en-US.ftl` + `pt-BR.ftl`** (additive only, 16 keys × 2):
`layers` `.add-material`, `.default-material-name`, `.material-channels`,
`.material-id`, `.material-id-none`, `.material-no-defs`,
`.material-reset-channels`, `.material-refresh`,
`.material-channel-opacity`, `.material-badge-no-params`,
`.material-badge-missing-def`, `.material-ignored`,
`.material-unpaintable`; `graph` `.save-material`, `.saved-material`,
`.save-material-empty`. Every key is used via a `tr`/`tr_args` literal.

Test count: 6 (layers_panel) + 2 (graph_panel) = 8 new inline test fns.
No integration tests touched. No exhaustive matches needed — Kill A armed
`document.rs` and `history_panel.rs`; the grep census holds (no other
`LayerKind`/`LayerCommand` matches in the app).

## 2. Judgment calls (all also in module docs)

- **Brief file `layers_panel.rs` did not exist** — the Layers panel lives in
  `document.rs::layers_ui`. Created the named file for the material rows
  (honoring the lock) with a 4-line hook + signature extension in
  `document.rs`, rather than stuffing ~400 lines into it.
- **`material_id` is a combo, not free text**: free text emits one undo op
  per keystroke; a pick is one clean op. Unknown ids still arise (project
  files, hand edits) and badge honestly.
- **Blend modes render as `BlendMode::{:?}` identifiers** (the Graph
  panel's `node_def` precedent — technical identifiers, not prose), so no
  32-keys-per-locale explosion. Filtering matches the driver exactly.
- **Declaration re-parsed on expanded draw** (no cache beside the
  definition — definition and declaration can never drift, doc §1.2).
- **Session defs are `BTreeMap`** (project-model order, deterministic save,
  sorted combo); the driver takes `HashMap`, so the drain clones on dirty
  frames only (KB-scale, documented at the call site).
- **Channel-override census in the shell**: `MaterialLayerState` tracks set
  size but not per-channel overrides, so the frame fingerprints
  set-size + per-channel sizes and `mark_dirty`s on change. Mesh-map
  revisions need no census (flat placeholders never bump).
- **Save writes the whole session map** (superset of "every definition a
  layer references"): a save-as-material entry not yet layered survives the
  save. Referenced defs are always present, so the doc's portability clause
  holds.
- **Status line = Layers-panel line + log**: there is a `status_bar.rs` but
  it is outside this kill's surface; failures/ignored-channels land on
  `log::warn` (once per drain) plus the row badge plus the panel status
  line. Wiring the real status bar is a follow-up for its owner.
- **Stale material states are left in place** (the `fill_states` precedent:
  session caches keyed by layer id sit unused after delete). A reused id
  self-heals through `sync` (params/definition compare → dirty). Badges ARE
  pruned against live layers every drain.

## 3. Deviations from the doc/brief (honest degradations, none silent)

- **D1 — the stroke gate is not IN stroke begin.** The doc assumes stroke
  begin lives in `main.rs`; it does not — strokes begin in `viewport.rs:355`
  (`begin_screen_point_pressured`) and `uv_view.rs:320` (`begin_stroke`)
  against the layer-agnostic pool session, all outside this kill's 6 locked
  files, and v1 has no selected-layer seam to gate on. Forcing a `main.rs`
  gate would be dead code (house rule 6) or a surface violation. Delivered
  instead: `paint_gate_allows` (the `check_paintable` truth, Kill A armed
  it) + the row's unpaintable hint derived from it + the §5.8 gate test.
  Follow-up: wire `paint_gate_allows` into the two stroke-begin sites when a
  kill owning that seam adds layer-targeted strokes (the helper is the seam;
  the pixel-unchanged end-to-end half of §5.8 belongs there — it needs a GPU
  session and cannot run headless).
- **D2 — no layer-opacity slider on material rows.** The doc's "layer
  opacity" is listed among *existing* widgets; no opacity widget exists on
  any row today, and the sub-row spec names only per-channel controls.
  Layer `opacity` IS live (the driver multiplies it into every channel).
- **D3 — `material: None` layers are creatable but definition-less.**
  The Add button makes a plain-tag layer the badge guides toward a
  definition. There is no "clear material" affordance (no command for it;
  `SetMaterial` needs params).

## 4. Unverified (battery must cover; nothing was compiled here)

- Compilation of all four touched source files (no cargo per the brief).
  Highest-risk spots, all checked by reading against in-tree precedent:
  `RichText` into `WidgetText` for `checkbox` labels; `ComboBox::
  from_id_salt(String)` (same call as `fill_layer.rs:918`); unused-
  `Response` statements (same as `fill_layer`/`graph_panel` precedents);
  implicit `&mut` reborrows of `PanelViewer` fields at call sites (same as
  the existing `self.doc` pattern); `matches!` on `&BlendMode` (match
  ergonomics); `assert_eq!` on `f32` (precedent `graph_panel.rs:1389`).
- Runtime behavior of the drain (eval → resample → composite → publish
  through the real session), the panel drawing, and save/open round-trips
  with material layers.
- `cargo fmt --check` cleanliness: splits were hand-normalized to the
  100-col style (several multi-line statements collapsed where they fit);
  residual diffs are the battery's to normalize.

## 5. Deferred (recorded follow-ups, not cut silently)

- Viewport/UV-view stroke-begin wiring of `paint_gate_allows` + the
  pixel-unchanged §5.8 test (needs the paint/viewport seam + GPU).
- Real status-bar delivery of drain messages (owner: `status_bar.rs`).
- Free-text `material_id` entry; per-layer material library curation
  (pruning unreferenced defs); channel-size census inside
  `MaterialLayerState` (today shell-side); live in-memory bake externals
  replacing the flat mesh-map placeholders; per-layer pool storage and
  every v1 boundary in `material_layer.rs`'s docs (unchanged).

## 6. Gates run

NONE — per the brief ("NO cargo (fmt/clippy/test all ride the dragon-side
battery after)"), `cargo fmt`, `cargo clippy`, `cargo test`, and
`scripts/verify-kill.sh` were deliberately not run. Files touched (exact):

1. `crates/umber-app/src/layers_panel.rs` (NEW)
2. `crates/umber-app/src/main.rs`
3. `crates/umber-app/src/document.rs`
4. `crates/umber-app/src/graph_panel.rs`
5. `assets/i18n/en-US.ftl` (additive)
6. `assets/i18n/pt-BR.ftl` (additive)
7. `docs/loop/kill-reports/2026-10-14-dynmat-kill-b.md` (this report)

Untouched as ordered: `material_layer.rs`, `umber-core/**`,
`umber-graph/**`, `crates/umber-app/tests/`, `crates/umber-app/src/lib.rs`,
every other panel, `requirements.md`, `CHANGELOG`. No commits.

**Battery must run, in order:** `cargo fmt --all --check` (expect possible
reflow-only diffs in the new/edited regions); `cargo clippy -p umber-app
--all-targets -- -D warnings` (touched crate; umber-core/umber-graph
untouched); `cargo test -p umber-app` (8 new fns: 6 `layers_panel::tests`
+ 2 `graph_panel::tests::save_as_material_*`) plus the i18n completeness
test (16 new keys, all referenced by literals); workspace build for the
exhaustive-match census (expected: no new arms needed).
