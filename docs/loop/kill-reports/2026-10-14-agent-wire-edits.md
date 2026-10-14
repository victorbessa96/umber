# KILL — agent wire EDIT steps (queue row 14's named follow-up)

Date: 2026-10-14. Lock: `agent-wire-edits` (pre-registered; `kill-locks.sh` not called).

## Built

`crates/umber-cli/src/agent.rs` (+~280):

- `AgentStep`: `LayerAdd`, `LayerRemove`, `LayerSetOpacity`, `LayerReorder`,
  `LayerSetMaterial` (snake_case rename maps them to `layer_add` /
  `layer_remove` / `layer_set_opacity` / `layer_reorder` /
  `layer_set_material`). All bodies carry `#[serde(deny_unknown_fields)]`
  (the photocraft rule); unknown ids, missing projects and non-finite
  input are typed errors, and a failed step never ends the session.
- Step structs: `LayerAddStep` (`name`, `kind: paint|fill|material`,
  optional `opacity`), `LayerRemoveStep`, `LayerSetOpacityStep`,
  `LayerReorderStep` (`id`, `to`, 0 = bottom), `MaterialChannelStep`
  (`name`, defaulted `blend`/`opacity`/`enabled`) and
  `LayerSetMaterialStep` (`id`, `material_id`, optional `channels`).
  `LayerAddKind` admits only paint/fill/material (folders need members
  the flat stack cannot name; instances need a source the step does not
  take).
- `Session::apply_edit`: the one `Document::run` chokepoint — push through
  the active set's per-set history, then journal the applied op read after
  the push (failed appends are stderr notes, never fatal). Every edit step
  pre-validates so a pushed command always lands on a live layer.
- Replies mirror the undo shape (`journal_seq` / `can_undo` / `can_redo`)
  plus the affected row: add carries the new id + row + post count;
  remove carries id + name + former index + post count; set-opacity
  carries the stored (post-clamp) value; reorder carries `from` + the
  landed `to`; set-material carries the stored `material_id` + channels.
- `layer_census_row` shared by `texture_set_census` and the add reply, so
  the two can never disagree (the open/state one-builder precedent).

`crates/umber-cli/tests/agent_wire_edits.rs` (new file, 10 black-box
tests over the real binary, CPU-only): add happy-path (id + row + delta +
two-entry `journal_seq`); fill/material kinds with the 1.0 default;
add strict-typing (unknown field, unknown kind, 1e38-opacity) with
survival; remove row + unknown-id error; opacity store/clamp/reject;
reorder by id + past-the-end clamp + unknown id; set-material sparse and
explicit channels (defaults + clamp) + bad blend + unknown id; the
undo/redo round trip through the census (property and structural);
journal-records + save-stamps-high-water with fresh-process reopen;
edits-without-open-project typed errors with survival.

Kill report: this file.

## Judgment calls

1. **No `layer_rename` — the one deviation.** `LayerCommand` carries no
   rename op and `JournalOp` has no name-only entry, and the brief's "every
   op you need" claim is wrong on this point (verified: the full variant
   list in `layers.rs`, plus the app itself never renames — `layers_ui`
   renders names as labels, fixed at add time). A wire rename could be
   neither undone nor recovered, so shipping one would violate the kill's
   own undoable+journaled rule and the no-new-variants hard rule at once.
   It waits on a core `Rename` command plus a journal entry (both
   umber-core, out of surface). Recorded in the module docs; no rename
   test exists to pin (an unknown `layer_rename` line fails as an unknown
   variant like any other unknown step).
2. **Sparse channel storage on absent `channels`.** umber-cli cannot
   resolve a definition's declared set (that needs `umber-graph`'s mtlx
   parse plus the app-side output discovery — both out of surface, and no
   new dep was added). Storing `[]` is still "defaults per the declared
   set": `MaterialParams`' docs make the definition win at eval time
   (declared-but-unlisted channels read as `MaterialChannel::named`
   defaults), so the sparse form evaluates identically to the app's
   eagerly-materialized list. Pinned by test (`channels == []`).
3. **`layer_add` + `opacity` is two undo entries.** `Add` carries no
   opacity, and folding the value in by direct mutation would journal
   nothing while diverging recovery. Two commands (add, set-opacity),
   each journaled; the reply's `journal_seq` (2 on a fresh fixture) pins
   the granularity. One wire step, two undos — stated in the module docs.
4. **Unknown ids are checked before the push, never pushed as no-ops.**
   `Document::run` pushes unconditionally, but a no-op push still occupies
   undo depth and journals nothing; the wire reports the typed error
   (`unknown layer {id}`, the `InstanceError::UnknownLayer` wording)
   without touching the stacks.
5. **Non-finite opacity is rejected; out-of-range is clamped.**
   `LayerStack::set_opacity` silently ignores non-finite input (a push
   that journals nothing), so the wire refuses it; finite-but-wild input
   (7.0) clamps per the stack rule and the reply carries the stored
   value. Channel-entry opacity instead mirrors the app panel's
   `upsert_channel` (non-finite maps to 0.0) — the step the brief's reject
   rule names is `layer_set_opacity` only.
6. **Unknown `material_id`s still point.** The app's `point_at_definition`
   never refuses an edit (the refresh badges `MissingDefinition`); the
   wire matches it. Pinned by test with `umber.test-steel`.
7. **No `requirements.md` row marked.** No open row covers the agent edit
   steps; the §10 scripting row already carries ✓ built.

## Unverified (no cargo per the amendment — nothing below was compiled or run)

- `cargo fmt --all --check`, `cargo clippy -p umber-cli --all-targets --
  -D warnings`, `cargo test -p umber-cli`: NOT run. Every type and
  signature was read from source (`LayerCommand::{add,remove,reorder,
  set_opacity,set_material}` + `Command::apply` in layers.rs,
  `UndoStack::{new,push,peek_undo,can_undo,can_redo}` in undo.rs,
  `JournalOp::applied` + `Journal::{record,last_seq}` in journal.rs,
  `LayerStack::{layer,index_of,next_layer_id,add_layer}` shapes,
  `MaterialChannel`/`MaterialParams`/`Layer` root re-exports in lib.rs —
  all confirmed in-tree), but a signature could have drifted under
  concurrent edits.
- The `apply_edit` borrow-split shape was reviewed against the E0502
  precedent (`project_undo`'s disjoint field borrows) but never compiled;
  likewise the pre/post shared reads around the `&mut` push.
- The 1e38-opacity tests assume serde_json casts a JSON float to f32 with
  overflow-to-inf (so the step parses and the explicit finite check
  fires). If serde instead range-errors the parse, those two assertions
  fail on the message (`not finite`) while the `ok: false` half still
  holds — the failure mode is loud, not silent.
- The `journal_seq == 2` pins assume a fresh fixture opens at mark 0 with
  an attached writable journal (the v0 attach path); a read-only scratch
  would degrade to `journal: None` and pin mark 0 instead.
- Behavior needs a runtime run regardless: the full battery below.

## Deferred

- `layer_rename` (needs core `Rename` + journal entry — the follow-up this
  kill names).
- Edit-step coverage for kinds beyond add's three (folder/instance
  constructors), blend/visibility/fill/instance-source/mask steps (the
  command vocabulary already carries them; only the five brief-named steps
  are wired).
- `requirements.md` row (see judgment 7).

## Gates run

- `bash scripts/verify-kill.sh crates/umber-cli/src/agent.rs crates/umber-cli/tests/agent_wire_edits.rs docs/loop/kill-reports/2026-10-14-agent-wire-edits.md`:
  braces PASS (435/435, 207/207, 5/5), fmt PASS after in-place reflows of
  my own test-file lines only (long `agent.call` / assert chains reflowed
  to canonical form, no semantic change; `agent.rs` had no drift),
  matches!-arity PASS, doc-attachment PASS. No thiserror WARNs. Lock
  audit: all three files locked by `agent-wire-edits.lock`.
- `cargo fmt / clippy / test`: NOT run (amendment forbids cargo). CI is the gate.
- No commits; `batch.rs`, `main.rs`, and every other crate untouched.

## Battery the dragon must run (CI + local)

- `cargo fmt --all --check` (verify-kill already reformats in place; this
  is the confirmation).
- `cargo clippy -p umber-cli --all-targets -- -D warnings` (touched
  crate; per AGENTS.md this is what CI uses).
- `cargo test -p umber-cli` — runs the inline `agent.rs` unit suite plus
  all three integration targets (`agent.rs`, `project.rs`, and the new
  `agent_wire_edits.rs`; the last two are CPU-only, the first needs a GPU
  adapter or accepts its documented no-adapter skip).
- Watch for: the two `not finite` assertions (see Unverified), the
  `journal_seq == 2` pins, and any E0502/E0507 the borrow review missed.
