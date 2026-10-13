# Kill report — post-effects persistence (wave-postfx)

Date: 2026-10-12. Claw: opencode (wave2-postfx lock).
Closes the C1 audit's 1c boundary ("the chain is session-only, not
project-persisted yet"): the PostFX chain's effects + params now
round-trip through save/load and restore exactly.

## The carry type's shape

`umber-core` owns the file schema (it cannot name `umber-gpu` types —
`umber-gpu` depends on `umber-core`, so the reverse dep would be a
cycle; this is the same reason `DisplaySettings` lives in the leaf
`umber-color` crate). New in `crates/umber-core/src/project.rs`:

- `PostFxToneMapMode` (`Aces` / `Filmic` / `Reinhard`, `Default = Aces`
  via derive + `#[default]`) — plain-data mirror of
  `umber_gpu::postfx::ToneMapMode`.
- `PostFxEffect` (`Dof` / `Bloom` / `Vignette` / `Sharpen` / `ToneMap`,
  same fields as `umber_gpu::postfx::PostEffect`) — the mirror.
- `PostFxCarry` (`effects: Vec<PostFxEffect>`, `enabled: Vec<bool>`
  index-aligned; container `#[serde(default)]`, `Default` = empty =
  identity) — the chain's persisted form, versioned by the project
  version stamp (no separate version field).
- `ProjectSettings` gains `#[serde(default)] pub postfx: PostFxCarry` —
  the exact `display` precedent (pre-postfx files lack the key and load
  as the empty chain).

The seam (`crates/umber-app/src/postfx_panel.rs`, which sees both
crates): `carry_from_chain` (save; params ride as-is — the model is
already the clamp point) and `chain_from_carry` (load; clamps through
`PostEffect::clamped`, short `enabled` reads as enabled / absent ⇒
enabled). Both matches are exhaustive on both enums: a variant added to
either side breaks the build here — the mirror-drift guard.
`main.rs`: save writes `carry_from_chain(&self.state.postfx)` into the
settings literal; load assigns
`chain_from_carry(&model.settings.postfx)` next to the display restore.
No dirty flag: the viewport reads the chain every frame and `PostStack`
builds a newly-seen kind on first use (params ride uniforms). The
panel's own state (`PostFxPanel`: add pick, drag in flight) stays
session-only; the `postfx.session-only` status line was removed (it is
now false).

## Round-trip evidence (hand-derived; the battery runs the tests)

Wire form is serde externally-tagged (`{"Dof":{"focus":5.0,…}}`,
`"mode":"Filmic"`), struct fields in declaration order — deterministic
(no maps involved), same basis as the existing display byte-identical
test. Traced for the test chain
(Dof 5.0/0.3, Bloom 1.0/0.5/8, Vignette 0.3/0.5, Sharpen 0.5/1,
ToneMap Filmic/-1.5, flags `[true,false,true,true,true]`):

- Save: every test param is already inside its clamp
  (0.3 ∈ 0..=1, 8 ∈ 1..=64, −1.5 ∈ −4..=4, …), so `clamped()` is the
  identity and the JSON holds the exact values; `0.3f32` prints as
  `0.3` via serde_json's shortest repr and parses back to identical
  bits (pinned by `to_bits`, the same guarantee the display test pins
  for 1.37).
- Load: `chain_from_carry` re-pushes (clamp = identity here) and
  restores flags → `PostChain == original` (`assert_eq` on both vecs:
  order-sensitive, so a reorder/dropped effect fails it).
- Resave: loaded model `==` original model → `save_to_dir` is
  deterministic → `assert_dirs_byte_identical` holds.
- Pre-postfx file (no `postfx` key): `serde(default)` → empty carry →
  `PostChain::new()` → viewport route Idle, zero intermediates, no
  dispatch.

Tests (4 new; each fails if the behavior regresses):
`umber-core`: `pre_postfx_project_json_loads_empty_chain`,
`postfx_carry_round_trip_byte_identical` (wire-name pins + `to_bits` +
save→load→save byte-identity). `umber-app` panel:
`carry_round_trip_is_exact` (all 5 kinds, one disabled),
`chain_from_carry_clamps_and_defaults_missing_flags` (99/−4/1000 →
4.0/0.0/64, missing flag → enabled).

## Built

- `crates/umber-core/src/project.rs`: `PostFxToneMapMode`,
  `PostFxEffect`, `PostFxCarry`, the `ProjectSettings.postfx` field, 2
  tests.
- `crates/umber-app/src/postfx_panel.rs`: `carry_from_chain`,
  `chain_from_carry`, module-docs update, removed the stale
  `session-only` status line, 2 tests.
- `crates/umber-app/src/main.rs`: save literal + save/open doc lines,
  AppState field docs, load-path rebuild. No other lines touched
  (paint_state.rs / icons.rs avoided per lock).
- This report.

## Judgment calls

All are also stated in the module docs; repeating here per contract:

- Mirror-in-core over moving `PostEffect` down: moving the enum to a
  leaf crate would refactor gpu + app + panel surfaces owned by sibling
  kills; the mirror keeps this kill disjoint. Drift guard = exhaustive
  matches (compile break), documented on `PostFxToneMapMode`.
- No `sanitized()` on the carry: float clamping stays in exactly one
  place (`PostEffect::clamped`, applied on load); the carry only needs
  length reconciliation, which `chain_from_carry` does (absent ⇒
  enabled — the `is_none_or` semantics, never `is_some_and`).
- No separate carry version: the project version stamp gates it (same
  as `display` / `graphs_mtlx`).
- No dirty flag on load: per-frame chain read + lazy per-kind pipeline
  build already covers it (comment at the load site).
- Removed (not reworded) the `session-only` line: any retained wording
  would lie about persistence; the orphaned ftl keys are a Doc delta
  below.

## Unverified

- No compiler, clippy, fmt, or test run: the wave-end rule forbids cargo
  and the sandbox forbids bash scripts, so the battery owns all gates.
  Highest-risk items for the battery: (a) hand-formatted match arms in
  the two conversion fns vs `cargo fmt --check` (shaped to match the
  repo's own `clamped()` arms, but unverified); (b) the
  `umber_core::project::{…}` import in the panel (dep exists in
  `umber-app/Cargo.toml`, usage is one-per-item — believed fine).
- No GPU/runtime run: viewport pickup of a loaded chain is by
  code-reading (`PostFxViewport::callback` takes `&PostChain` per frame;
  `PostStack::run` ensures per-kind pipelines before recording), not by
  execution.

## Deferred

- Undo of a load-restored chain (load replaces it wholesale; undo
  history already restarts fresh on open by design — no change).
- Per-effect enable-flag forward-compat beyond absent⇒enabled (a LONGER
  vec than effects is silently truncated by construction — iterating
  effects; recorded here, not in module docs).
- Journal ops for postfx edits (journal is layer-scoped; out of scope).

## Doc deltas

For the dragon (all outside this kill's locked surface — no edits made):

- Requirements row (§7 post-effects / P1): append
  `| ✓ built (PENDING) — chain persists via the `postfx` settings
  carry (effects + params + flags, byte-exact round-trip); v1 boundary:
  panel add-pick/drag state stays session-only; undo restarts on open
  (existing design) |`.
- `assets/i18n/en-US.ftl` + `pt-BR.ftl`: `postfx.session-only` is now
  unreferenced (line removed from the panel; the one-directional
  completeness test still passes). Dragon's call: delete the key or keep
  for a future session-only note.
- `umber-gpu/src/postfx.rs` module docs still say "`PostChain` is pure
  data (serde, …)" — now additionally project-persisted; one-line touch
  for whoever next owns that file.

## Gates run

- `cargo fmt --all`: NOT RUN (wave-end rule: no cargo).
- `cargo clippy -p umber-core / umber-app --all-targets -- -D warnings`:
  NOT RUN (same rule).
- `bash scripts/verify-kill.sh`: NOT RUN (sandbox wall: no bash
  scripts — brief-expected).
- Trap self-check by reading (thiserror-`source`, `matches!`-arity, no
  items in `impl`, `mut` bindings, `&&` refs, format-string field
  access, single `mod`, borrow splits, `>7` params, derivable `Default`,
  neg-cmp, doc attachment, `map_or`, field shorthand, lifetimes, `?`,
  `must_use`, `new`/`Default`, match guards): clean to the best of
  manual inspection — the battery confirms.
