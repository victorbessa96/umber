# Kill brief: wave-replay-exact opt-in toggle (next-steps item 21, decided 2026-11-16)

- Status: **DESIGN ONLY**. Written 2026-11-17 by a Razul-dispatched subagent under kill-lock `razul-replay-toggle` (docs only, no code touched).
- Decision source: `docs/loop/next-steps-2026-11-16.md:70-73` — item 21: "wave-replay-exact — **DECIDED 2026-11-16 (alignment form): default-off stands**; the remaining work is the opt-in toggle kill, moved to section B (after D5/displacement). Prep brief: `docs/loop/briefs/2026-11-14-replay-exact-decision-prep.md`."
- Prep brief: `docs/loop/briefs/2026-11-14-replay-exact-decision-prep.md` (trade table, derivation sketch, recommendation = default OFF for v1).
- Wave plan: `docs/loop/waves/2026-11-09-wave-plan.md:189` — "`wave-replay-exact` | path replays paint exactly | **a UX call** (default-off until Bessa decides)".
- ROADMAP: `docs/ROADMAP.md:244` — "wave-replay-exact | path replays paint exactly | a UX call (default-off until decided) — **decision prep brief written 2026-11-14**".
- Additive persistence pattern to mirror: `docs/loop/briefs/2026-11-11-rd1-baker-sync.md` (ProjectSettings additive fields, `..Default::default()` on load, no project version bump), `docs/loop/briefs/2026-11-16-d2-auto-cage.md` (D2 auto-cage, `HighPoly` additive fields via `BakeSync`).
- Sibling brief structure to mirror: `docs/loop/briefs/2026-11-16-d2-auto-cage.md` (landed brief format), `docs/loop/briefs/2026-11-16-d5-ground-plane.md` (newest D2-D5 brief).

## What exists (measured on live tree)

Project settings model (`umber-core/src/project.rs`):
- `ProjectSettings` at `project.rs:263-290` carries project-wide settings: `active_texture_set`, `display` (`DisplaySettings`), `postfx` (`PostFxCarry`), `symmetry` (`ProjectSymmetry`). All are additive carries via `#[serde(default)]` — old files load with defaults, empty carries are not written (`skip_serializing_if` on `BakeSyncCarry`, implicit for `Vec`/`BTreeMap`).
- `ProjectModel::new` at `project.rs:459-474` stamps `CURRENT_PROJECT_VERSION` (1), `MAX_SUPPORTED_PROJECT_VERSION` is 2 (for layer planes). No version bump for additive fields — the `serde(default)` pattern is the versioning.
- `ProjectFile` at `project.rs:513-536` is the on-disk shape; new additive fields go here with `#[serde(default)]`.

Replay code paths (read-only scout):
- `PaintState::replay_path` at `crates/umber-paint/src/paint_state.rs:5014` — replays a `PaintPath` through the stroke pipeline (`PathOp::Paint`/`Erase`/`Smudge`).
- `PaintState::replay_filled_path` at `paint_state.rs:5061` — replays polygon fill interior through the same pipeline.
- Journal replay at `crates/umber-core/src/journal.rs:538` — `replay(model, entries)` applies `JournalEntry::op` to each texture set's stack past `journal_seq`. Used for crash recovery (autosave journal).
- Undo/redo replay at `crates/umber-core/src/undo.rs:230` — `UndoStack::redo` reapplies commands; merged redo entries split into per-stroke replays (retouch precedent at `retouch.rs:1995-2031`).
- Existing replay tests at `paint_state.rs:12310-12400`:
  - `replayed_path_stages_exactly_like_the_manual_stroke` — asserts replay matches manual stroke dab-for-dab, batch-for-batch.
  - `replay_refuses_live_strokes_and_short_paths` — guards against invalid replays.
  - `retouch_replay_routes_to_the_cpu_mirror` — smudge/erase replays route to CPU mirror, not GPU.

What "replay-exact ON" means (from prep brief):
- Every paint primitive deterministic under replay: seeded RNG per stroke, fixed iteration order, no time/hashmap-order dependence, float-op-order stability.
- Audit surface: `umber-paint` stroke eval + `umber-core` journal replay.
- The toggle is a **project-format flag** (`replay_exact: bool` in `ProjectSettings`), default `false` (status quo), persisted additively.

No existing `replay_exact` field in `ProjectSettings` or `ProjectFile`. No UI for it. No wiring in replay paths.

## Dispatch lock list

Lock name: **`razul-replay-toggle`**

Locked files (exact surface at dispatch):
- `crates/umber-core/src/project.rs` (add `replay_exact: bool` to `ProjectSettings` + `ProjectFile`, `Default::default()` = `false`, additive `#[serde(default)]`)
- `crates/umber-paint/src/paint_state.rs` (wire `replay_exact` into `replay_path` / `replay_filled_path` — seeded RNG, fixed iteration order when `true`)
- `crates/umber-core/src/journal.rs` (wire `replay_exact` into `replay` — pass flag through `JournalEntry::op.apply_to` if needed for deterministic replay)
- `crates/umber-core/src/undo.rs` (wire `replay_exact` into `UndoStack::redo` / merged redo split replay — same deterministic path)
- `crates/umber-app/src/main.rs` (load/save the flag via `ProjectModel::settings.replay_exact`; no new UI in v1 — the opt-in is a project.json hand-edit or future Preferences page)
- `assets/i18n/en-US.ftl` + `assets/i18n/pt-BR.ftl` (new keys: `settings.replay-exact`, `settings.replay-exact-hint`)
- `docs/loop/queue.md` + this brief's kill report

## Build (dispatch-time lock: `razul-replay-toggle`)

### 1. Project settings flag (`project.rs`)

Add to `ProjectSettings` (after `symmetry`, before closing brace):
```rust
/// Replay-exact mode: when enabled, stroke replay (undo/redo, journal recovery)
/// uses seeded RNG, fixed iteration order, and stable float-op order so the
/// replay paints pixel-identically to the original stroke. Default OFF (status
/// quo). Additive carry: absent in file = false, so pre-toggle projects load
/// unchanged and re-save byte-identically when false.
#[serde(default)]
pub replay_exact: bool,
```

Add to `ProjectFile` (after `symmetry`, before closing brace):
```rust
/// See `ProjectSettings::replay_exact`. Additive: `default` loads pre-toggle
/// files as `false`.
#[serde(default)]
replay_exact: bool,
```

Wire in `ProjectModel::new` → `settings.replay_exact: false` (already covered by `Default`).
Wire in `load_from_dir` → `file_settings.replay_exact` maps to model.
Wire in `save_to_dir` → `model.settings.replay_exact` written to `ProjectFile`.

### 2. Replay path wiring (`paint_state.rs`)

`replay_path` and `replay_filled_path` receive `&mut self` — `self` has access to the document's `ProjectSettings` via the app's `PaintState` construction. The flag must be threaded from the app shell:

- `PaintState` needs a `replay_exact: bool` field (set at construction from the project's `settings.replay_exact`).
- When `replay_exact == true`:
  - Stroke RNG seeded from stroke ID (stable hash of path points + op + timestamp quantized to stroke granularity).
  - Dab iteration order fixed (already deterministic by path point order; verify no `HashMap`/`HashSet` iteration in hot path).
  - Float accumulation order stabilized (no parallel reduction; `f32` sum order is the path point order).
  - Smudge/retouch CPU mirror already deterministic — verify no `Instant::now` or thread-local RNG.
- When `false`: current behavior (unseeded RNG, any iteration order).

### 3. Journal replay wiring (`journal.rs`)

`replay(model, entries)` is called from `open_project_at` (crash recovery). The `ProjectModel` carries `settings.replay_exact`. Two options:
- A) Pass `replay_exact` into `JournalEntry::op.apply_to(&mut stack, replay_exact)` — requires `JournalOp::apply_to` signature change (cascade to `LayerCommand::apply`).
- B) Thread through `ProjectModel` → the `LayerStack` apply path reads `model.settings.replay_exact` (but `JournalOp` doesn't have model access).

Option A is cleaner: add `replay_exact: bool` parameter to `JournalOp::apply_to` and `LayerCommand::apply` (default `false` for callers without context). The `replay` function reads `model.settings.replay_exact` and passes it.

### 4. Undo/redo wiring (`undo.rs` + `retouch.rs`)

`UndoStack::redo` reapplies commands. The merged redo split (retouch) at `retouch.rs:1995-2031` replays each stroke alone via `replay_path`. Wire `replay_exact` from the document's project settings into the `UndoStack` redo path — either via a `redo_with_replay_exact` method or by storing the flag in the command (preferred: `LayerCommand` gets `replay_exact: bool` captured at push time from the document's current settings, so a stroke recorded with `replay_exact=true` replays exactly even if the setting is later toggled).

### 5. Persistence (additive, no version bump)

- `replay_exact: bool` in `ProjectSettings` + `ProjectFile` with `#[serde(default)]`.
- Old projects (no key) load as `false` → status quo behavior.
- Saving a project with `replay_exact=false` writes no key (or writes `false` — either is fine; `skip_serializing_if = "std::ops::Not::not"` on the field keeps the file clean). Precedent: `BakeSyncCarry::is_empty` + `skip_serializing_if`. For a `bool`, `#[serde(default, skip_serializing_if = "std::ops::Not::not")]` omits `false`.
- `MAX_SUPPORTED_PROJECT_VERSION` stays 2. `CURRENT_PROJECT_VERSION` stays 1.

### 6. i18n keys (both catalogs)

**en-US.ftl** (append to `settings` section at line ~106):
```ftl
settings =
    .language = Language
    .replay-exact = Replay exact
    .replay-exact-hint = When enabled, undo/redo and crash recovery replay strokes pixel-identically (seeded RNG, fixed iteration order). Default off.
```

**pt-BR.ftl** (append to `settings` section at line ~110):
```ftl
settings =
    .language = Idioma
    .replay-exact = Replay exato
    .replay-exact-hint = Quando ativado, desfazer/refazer e recuperação de falha repetem traços pixel a pixel (RNG semeado, ordem de iteração fixa). Desligado por padrão.
```

### 7. UI (v1 scope: project.json hand-edit only)

No Preferences page or panel UI in v1. The opt-in is a project-format flag — users who want it hand-edit `project.json` `"replay_exact": true` or a future wave adds a Preferences row. The brief only delivers the flag, the wiring, and the tests.

## Kittest-first definition of done

- `replay_exact_false_is_byte_identical_to_today`: `replay_exact=false` + a 15-point paint stroke + undo + redo + journal save/load/replay → output pixels byte-identical to tree without the toggle code (proves zero regression when off).
- `replay_exact_true_replay_matches_manual_stroke`: `replay_exact=true` + `replayed_path_stages_exactly_like_the_manual_stroke` conditions → replay pixels byte-identical to manual stroke (same test, flag flipped).
- `replay_exact_true_redo_matches_original`: `replay_exact=true` + paint stroke → undo → redo → redo output byte-identical to original stroke.
- `replay_exact_true_journal_recovery_matches`: `replay_exact=true` + paint stroke → save → kill process → load → journal replay → recovered state byte-identical to pre-crash state (extends `journal::tests::replay_reaches_the_live_session_end_state` with flag).
- `replay_exact_true_merged_redo_split_replays_each`: `replay_exact=true` + merged stroke (retouch) → undo → split redo → each split stroke replay byte-identical to its original (extends `retouch::split_redo_replays_merged_parts_with_exact_bytes` with flag).
- `replay_exact_persists_round_trip`: project with `replay_exact=true` → save → load → `model.settings.replay_exact == true`; project without key → load → `false`; re-save without key → file lacks `replay_exact` key (byte-identical to pre-toggle file).
- `replay_exact_false_old_project_loads`: load a pre-toggle `project.json` (no `replay_exact` key) → `settings.replay_exact == false`; re-save → file still lacks the key.

## Rules

- Battery before commit (all under `flock /tmp/umber-cargo.lock` + `UMBER_ASSETS_DIR=<repo root>`):
  - `rustfmt --edition 2021 --check` on touched `.rs`
  - `python3 scripts/check-layers.py`
  - `python3 scripts/scorecard.py --check`
  - `cargo clippy -p umber-core -p umber-paint -p umber-app --all-targets -- -D warnings`
  - `cargo test -p umber-core -p umber-paint -p umber-app` (0 failed)
- Both `.ftl` catalogs if keys added (ftl completeness is a battery gate).
- No weakened tests; new behavior gets new tests (kittest-first above).
- Edit only the locked surface at dispatch.
- Commit via `.dragon/dcommit.py --kill-lock razul-replay-toggle --lock <paths> -- -m "docs(umber): replay-exact opt-in toggle design brief"`; push via `scripts/safe-push.py` bare (refusal on sibling ledger 28f6c467 is FINE — project-watch lane owns push).

## Rejected alternatives

| Alternative | Reason |
|-------------|--------|
| App-level preference (not project) | Replay exactness is a document fidelity property — a project opened on another machine must replay identically. Project setting is correct. |
| Separate project version bump | Additive `serde(default)` pattern (symmetry, postfx, bake_sync) proves no bump needed. Version bumps are for breaking schema changes. |
| UI in Preferences panel v1 | The decision prep brief explicitly scopes the remaining work as "the toggle kill" — UI is a follow-up wave if adoption warrants. |
| Per-stroke toggle | Replay exactness is a global document policy (crash recovery, undo/redo consistency). Per-stroke would create mixed-determinism journals. |
| RNG seed in journal entry | The seed is derivable from stroke ID (path points + op). Storing it bloats the journal; re-derivation is the deterministic path. |

## Dragon verification

- Cross-review diff vs this brief, full battery on the landed commit, `ls-remote origin` hash check.
- Kill report at `docs/loop/kill-reports/2026-11-17-replay-exact-toggle.md` with measured numbers: replay time overhead (ms) with `replay_exact=true` vs `false` on a 15-point stroke, byte-identity proof for `false`, byte-identity proof for `true` vs manual stroke.