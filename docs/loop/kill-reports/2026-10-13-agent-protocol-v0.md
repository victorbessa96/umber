# KILL — control protocol v0 (queue row 14, absorbed from photocraft)

Date: 2026-10-13. Lock: `agent-protocol` (pre-registered; `kill-locks.sh` not called).

## Built

`crates/umber-cli/src/agent.rs` (+914/−2):

- `AgentStep`: `ProjectOpen`, `ProjectState`, `ProjectSave`, `SetActive`, `Undo`, `Redo`
  (snake_case rename maps them to `project_open`/`project_state`/`project_save`/
  `set_active`/`undo`/`redo`). `ProjectOpenStep`/`ProjectSaveStep`/`SetActiveStep`
  all carry `#[serde(deny_unknown_fields)]` (the photocraft rule); the three
  no-field steps reuse `NoFields`, so the bare/`null`/`{}` shorthands work.
- `ProjectSession` (`model` + `dir` + `active` + per-set `histories` +
  attached `journal`) held beside the mesh; `Session` gains
  `project: Option<ProjectSession>`.
- Census shapes (`ChannelCensus`/`LayerCensus`/`TextureSetCensus`/`ProjectCensus`/
  `ProjectStateBody`, all `Serialize`): version, active set, journal_seq,
  per-set names + resolution + channels (with paint sizes) + tiles + layer
  stack summary (id, name, kind, opacity, visible, blend). One builder feeds
  both `project_open` and `project_state`, so the two can never disagree.
- Sidecar mirror (`SidecarSizes` + `parse_sidecar_size`/`parse_channel_override`/
  `parse_sidecar_sizes`/`read_sidecar_sizes`): the app's `target_sizes.txt`
  size/channel arms, kept here because umber-cli must not depend on umber-app.
- `resolve_active` (saved active set, else first set; typed errors on empty
  projects, dangling saved names, and a missing layers entry for the active set).
- `project_open` (typed load failure verbatim; journal attach with read-only
  degrade), `project_state` (`project: null` when empty), `project_save`
  (stamps live mark + working set, Save-As follow, `painted: 0`), `set_active`
  (unknown names error with the set list), `project_undo`/`project_redo`
  (model-level, Document-mirrored journaling, empty reads as false).
- `bake` names outputs after the active set when a project is open, else the
  mesh's set as before (one line in `start_bake`).
- Tests (6): `project_open_reads_back_the_saved_census_exactly` (2 sets, exact
  channels/sizes/layers + open/state set-equality); `project_state_with_no_project_open_is_null_not_an_error`;
  `project_steps_reject_unknown_fields_and_names` (bogus field, unknown set,
  bad state field, missing dir → `I/O error`); `set_active_switches_and_save_round_trips`
  (stamp + `painted: 0` + on-disk files + no `paint/` + reopen layers-equal with
  model-fallback sizes pinned); `undo_and_redo_drive_the_held_model` (white-box:
  push through the journal, undo/redo steps flip the stack, floor reads false);
  `project_steps_need_an_open_project` (undo/redo/save/set_active without open).

`crates/umber-cli/tests/project.rs` (new file, 8 black-box tests over the real
binary, CPU-only): census-exact open; null empty state; missing-dir typed error;
set_active ok + unknown names (nothing switched); deny-unknown-fields;
save (files on disk, `painted: 0`, fresh-process reopen); empty undo/redo flags;
mesh/project independence (the ruling, both directions).

Kill report: this file.

## Judgment calls

1. **Bake retargets, export does not.** The brief names the active set as "what
   bake/export target". Bake's set name is one in-surface line (`start_bake`);
   export's naming is computed inside `main.rs::export_mesh` (out of surface —
   `texture_set_name(mesh_path, mesh)`, no override param), so unifying it
   needs a `main.rs` change this kill must not make. A half-retarget is
   documented in the module docs + the bake arm; export unification is deferred.
2. **Empty undo/redo is `ok: true` with `undone/redone: false`.** Absence is
   state (the census convention), and v0 has no edit steps, so empty is the
   common case — erroring would make every fresh session's first undo a failure.
3. **Undo/redo/save/set_active without an open project ARE errors** (`no project
   open`): unlike the census there is no model to drive, mirroring the
   `no mesh loaded` rule. Pinned by test.
4. **Per-set undo journals, not one.** A single history replayed across a
   `set_active` switch is exactly the stale-journal corruption
   `Document::load_stack` guards against (ids collide across stacks). The map
   costs three lines.
5. **Journal attached on open (app mirror).** A read-only `project_open` still
   succeeds with `journal: None` + a stderr note (the journal is an
   optimization, never a gate). Side effect named honestly: opening creates an
   empty `journal.jsonl` when none exists — the app behaves identically.
6. **`project_save` stamps the live mark and the working set** (the app's rule),
   follows a Save As (re-attach + truncate-old, the `attach_journal` rule), and
   always reports `painted: 0` — the CLI holds no paint tiles, the save is
   `project.json` + layers only, an existing sidecar is left intact, a fresh
   directory has none. The reopen test pins the consequence (sizes fall back to
   the model resolution).
7. **Census tolerates missing layers entries (reads as empty), but open requires
   one for the ACTIVE set** — lenient census, strict working set (the app's
   `open_project` parity).
8. **Sidecar mirror lives in `agent.rs`** with a drift guard in its docs;
   `tile-<N>` assignments are parsed-and-skipped (reserved prefix, sizes only —
   carrying them would be dead state). Absent mapping degrades per-field to the
   model, never an error.
9. **Mask presence is not in the census** (v1 boundary, stated in
   `LayerCensus`'s docs): the census is a stack summary, not a mask inventory.
10. **"Undo/redo on a journaled edit" is tested white-box** (`undo_and_redo_drive_the_held_model`):
    v0 has no edit step that could push through the wire, so the mechanism is
    pinned in-process (real model + real `UndoStack` + real step replies) while
    the wire tests pin the empty behavior. The first edit step wires into the
    existing `histories` + autosave path with no session changes.
11. **No `requirements.md` row marked.** No open row covers the agent protocol;
    the §10 scripting row already carries ✓ built. Marking a P2 row this kill
    does not complete would be dishonest.

## Unverified (no cargo per the amendment — nothing below was compiled or run)

- `cargo fmt --all --check`, `cargo clippy -p umber-cli --all-targets -- -D warnings`,
  `cargo test -p umber-cli`: NOT run. All types/APIs were read from source
  (`load_from_dir`/`save_to_dir`/`truncate_journal`/`Journal::open+record+last_seq`
  in umber-core journal.rs/project.rs, `UndoStack::{new,push,undo,redo,peek_undo,can_undo,can_redo}`
  in undo.rs, `LayerCommand::add` + `JournalOp::{applied,reverting}` in
  layers.rs/journal.rs, `TileSet::from(&TextureSet)` + `tiles_present` in
  tiles.rs, `LayerStack::{add_layer,set_opacity,set_blend_mode,set_visible}` field
  shapes, `TextureSet`/`Channel`/`ProjectSettings`/`TextureSetLayers` re-exports
  in umber-core lib.rs — all confirmed in-tree), but a signature could have
  drifted under concurrent edits.
- The borrow-split shapes (`project_undo`/`project_redo` disjoint field borrows,
  the `held.journal_seq()` inline dodge, `json!` moves) were reviewed twice
  against the E0502/E0507 traps but never compiled.
- The bake-retarget line needs a GPU run (no test here bakes); the wire tests
  never start a bake.
- Sidecar parsing is pinned against hand-written `target_sizes.txt`, not against
  a file the real app wrote; the mirror's drift guard names what must match.
- `set_active`'s `{:?}` quoting in the unknown-set error is pinned (`"Boots"`,
  both suites); the exact `I/O error` prefix is ours, the OS tail is not —
  pinned as `contains` only.

## Deferred

- Export naming unification with the active set (needs an `ExportAsk`/`export_mesh`
  override in main.rs — out of surface, named in the module docs).
- Edit steps over the wire (layer add/remove/reorder/property): the session side
  (per-set histories + autosave recording + save stamping) is already in place.
- Per-tile channel census for multi-tile sets (set-level channels + tile list today).
- Mask inventory in the census (see judgment 9).
- `requirements.md` row (see judgment 11).

## Gates run

- `bash scripts/verify-kill.sh crates/umber-cli/src/agent.rs crates/umber-cli/tests/project.rs`:
  braces PASS (382/382, 135/135), fmt PASS after in-place reflows of my own lines
  only (no semantic change — long asserts and the `set_active` guard reflowed to
  canonical form), matches!-arity PASS, doc-attachment PASS. No thiserror WARNs.
  Lock audit: `agent.rs` is covered by `agent-protocol.lock`; `tests/project.rs`
  (new file, in-surface per the brief) shows as unlocked-but-noted — the brief
  forbids calling `kill-locks.sh`, so the dragon resolves it. The other NOTEs
  (umber-core journal/layers/lib/project, umber-graph) are concurrent kills'
  files, untouched here.
- `cargo fmt / clippy / test`: NOT run (amendment forbids cargo). CI is the gate.
- No commits; `batch.rs`, `main.rs`, and every other crate untouched.
