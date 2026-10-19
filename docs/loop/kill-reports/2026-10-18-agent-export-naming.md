# Kill report: agent export naming (2026-10-18)

Dragon-authored host-free by the Umber /loop session "Work on Umber
project coordination #3" (local-d3deea1d3c7631cc80f8782e73c37fe384c4c50d1248d89def19e4801b4c9a45, wakeup #1, under Bessa's live steer
"do all the work that you can locally, keep developing the project").
Kill-lock `agent-export-naming`, holding
`crates/umber-cli/src/main.rs` and
`crates/umber-cli/tests/agent_export_naming.rs`;
`crates/umber-cli/src/agent.rs` is held under this session's own
`agent-brush-radius` lock (the registry refuses overlapping locks —
both kills are one session's, one recovery row; see Reconciliation).

Context: agent.rs's module docs carried the named deferral —
"`export` still names from the mesh (its naming lives in main.rs's
`export_mesh`, out of this surface — unifying it is deferred)" —
while `bake` already names after the active set when a project is
open (control-protocol v0's contract). This kill unifies export to
bake's rule.

## Built

- `main.rs`: `ExportAsk` gains `set_override: Option<&'a str>` (the
  tunables bundle, staying under clippy's 7-argument floor).
  `export_mesh` reads it first:
  `ask.set_override.unwrap_or_else(|| texture_set_name(mesh_path, mesh))`
  — an explicit override names every output; none keeps the
  mesh-derived name. The batch path (`run_export`) passes `None`:
  byte-identical behavior, enforced by the compiler (the field has no
  default — both constructors had to name it).
- `agent.rs`: the `Export` step passes
  `set_override: self.project.as_ref().map(|held| held.active.as_str())`
  — an open project's ACTIVE set names the export, exactly bake's v0
  rule; no project = the mesh's set, as before. No new wire field: the
  rule is implicit in project state, symmetric with `bake`. The `$mesh`
  token stays mesh-derived (only `texture_set` changes). Module docs
  and `AgentExportStep` docs updated (the "unifying it is deferred"
  sentence is now the landed rule, dated).
- `tests/agent_export_naming.rs` (new): the first CLI export
  integration test — spawn → `load quad.obj` → `export` (no project)
  → every written filename `quad_*` (the OBJ carries no materials, so
  the stem names it); `project_open` (a `Body` 512² fixture) →
  `export` → every filename `Body_*` and never `quad*` (the override
  REPLACES the name, not appends). Preset `unity`: its Normal output
  survives the headless satisfiability filter (AO + baked tangent
  normal are exactly the maps the pipeline bakes). Export deadline is
  the bake class (600 s); `is_gpu_skip` guards the no-adapter machine
  (`bake_context` and `paint_context` share the "no suitable GPU
  adapter" phrasing).

## Judgment calls

1. `set_override` on `ExportAsk`, not a new `export_mesh` parameter —
   the bundle exists precisely to stay under the 7-arg floor; the
   request's naming IS a request tunable.
2. No wire field — bake's rule is implicit in project state; a
   per-step `set` field would let a driver name outputs after a set
   the project does not hold. The typed-refusal path for that
   (validate `set_override` against the project) does not exist in
   bake either; symmetry wins, and `set_active` is the validated way
   to change the active set.
3. Batch stays `None` forever — the recipe surface's naming contract
   is the mesh's own set; only the agent session (which can hold a
   project beside a mesh) has anything to unify.

## Reconciliation (agent.rs carries two kills)

`agent.rs` holds this kill's hunks AND `agent-brush-radius`'s (same
session, sequential kills, interleaved regions — the Export arm and
the PaintStrokeStep arm). One file's bytes cannot split across commit
rows: both kills land as ONE combined recovery row. The
`agent-brush-radius` manifest's agent.rs pin was re-pinned to the
combined bytes with this note; this kill's manifest lists its own
files and names agent.rs as riding the same row. The recovery session
commits once, releases both locks at that commit.

## Unverified

- No compile run (host down since 10-17 21:48; the outage contract
  parks the battery). Compile risk swept statically: both `ExportAsk`
  constructors updated (the field has no default — the compiler
  enforces completeness); `held.active` is `String` → `.as_str()`
  matches `Option<&'a str>`; the `Export` arm's borrows are all
  immutable (`self.loaded()` earlier, `self.project.as_ref()` —
  disjoint fields); `unwrap_or_else` over `Option<&str>` matches
  `texture_set_name`'s `String` return via closure coercion.
- The test's GPU legs run under the battery (lavapipe/CI adapters);
  the skip guard stands.

## Gates run (host-free, this session)

- `rustfmt --check --edition 2021`: CLEAN on all five touched files
  (write-mode pass applied its own wrap to the `set_override` chain
  and two test-file lines — the parked-bytes fmt-clean rule).
- `scripts/verify-kill.sh`: 3/3 PASS on this kill's files
  (brace census, trap greps, doc-attachment, lock audit).
- Interface coherence: `ExportAsk` is `pub` with private fields,
  constructed only at the two in-crate sites (grep-verified); the
  naming flows through the existing `TokenSources` path untouched.

State: STATICALLY VERIFIED, IN FLIGHT — battery (scope: umber-cli) +
commit + push parked on the build host's return, landing in the same
recovery session as the stencil wave, e0063, and agent-brush-radius
(one combined row with agent-brush-radius; see Reconciliation).
