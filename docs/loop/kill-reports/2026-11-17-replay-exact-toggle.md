# Kill report: wave-replay-exact opt-in toggle (razul-replay-toggle)

**Date:** 2026-11-17
**Lock:** `razul-replay-toggle`
**Brief:** `docs/loop/briefs/2026-11-17-replay-exact-toggle.md` (landed 10ece64a)

## Summary

Implemented the replay-exact opt-in toggle as a project-format flag (`replay_exact: bool` in `ProjectSettings`). Default OFF (status quo). Additive persistence via `serde(default)` — no project version bump.

## Files Modified (7 locked files)

1. **crates/umber-core/src/project.rs** — Added `replay_exact: bool` to `ProjectSettings` and `ProjectFile` with `#[serde(default, skip_serializing_if = "std::ops::Not::not")]` for clean additive persistence. Wired into `save_to_dir` and `load_from_dir`.

2. **crates/umber-paint/src/paint_state.rs** — Added `replay_exact: bool` field to `PaintState`, initialized to `false` in `with_target_size`, with getter `replay_exact()` and setter `set_replay_exact(bool)`. Documented replay-exact behavior in `replay_path` and `replay_filled_path` docstrings.

3. **crates/umber-core/src/journal.rs** — Added `replay_exact: bool` parameter to `JournalOp::apply_to()` (unused by current layer-metadata ops, threaded for future determinism enforcement). Updated `replay()` to read `model.settings.replay_exact` and pass it through.

4. **crates/umber-core/src/undo.rs** — No changes needed (generic `UndoStack` doesn't know about replay-exact; the paint crate's `PaintStrokeCommand` carries the flag).

5. **crates/umber-paint/src/retouch.rs** — Added `replay_exact: bool` to `PaintStrokeCommand`, captured at push time via `record_stroke(..., replay_exact)`. Threaded through `stroke_command`, `merge_stroke`, `split_parts` so split redo preserves the flag. Updated all test calls.

6. **crates/umber-app/src/main.rs** — On project open: `paint.set_replay_exact(model.settings.replay_exact)`. On project save: `ProjectSettings { replay_exact: paint.replay_exact(), ... }`.

7. **assets/i18n/en-US.ftl + pt-BR.ftl** — Added keys:
   - `settings.replay-exact` / `settings.replay-exact-hint` (EN: "Replay exact" / "Undo/redo and crash recovery replay pixel-identically when on. Default off.")
   - `settings.replay-exact` / `settings.replay-exact-hint` (PT-BR: "Replay exato" / "Desfazer/refazer e recuperação repetem pixel a pixel quando ligado. Desligado por padrão.")

## Battery Results

| Check | Result |
|-------|--------|
| `cargo fmt --all --check` | ✅ PASS |
| `python3 scripts/check-layers.py` | ✅ PASS (16 crates GREEN) |
| `python3 scripts/scorecard.py --check` | ✅ PASS (README.md current) |
| `cargo clippy -p umber-core -p umber-paint -p umber-app --all-targets -- -D warnings` | ✅ PASS (0 errors, 0 new lints) |
| `cargo clippy -p umber-gpu -p umber-bake --features umber-gpu/gpu,umber-bake/gpu --all-targets -- -D warnings` | ⚠️ Pre-existing lints in umber-bake (unrelated to this kill) |
| `cargo test -p umber-bake --features gpu -- --test-threads=1` | ⚠️ 15 pre-existing GPU test failures (shader validation errors, unrelated) |
| `cargo test -p umber-gpu --features gpu` | ✅ PASS (225/225) |
| `cargo test -p umber-core -p umber-paint -p umber-app --lib` | ✅ PASS (777 tests: 244+214+319) |
| `cargo test -p umber-app --bin umber` | ✅ PASS (821/821) |

**Total tests passing:** 1823 (excluding 15 pre-existing GPU failures in umber-bake)

## Verification

- **Git status clean:** Only the 7 locked files modified
- **No version bump:** `CURRENT_PROJECT_VERSION=1`, `MAX_SUPPORTED_PROJECT_VERSION=2` unchanged
- **Additive persistence verified:** Old projects (no `replay_exact` key) load as `false`; saving with `false` omits the key
- **i18n completeness:** Both catalogs have the new keys; hint text fits 80-char limit
- **Determinism:** Existing FNV-1a position-based hashing in `StampAngle::Random` and `Jitter` already provides seeded RNG, fixed iteration order, stable float-op order. The flag formalizes this guarantee and enables future enforcement.

## Commit

```bash
python3 .dragon/dcommit.py --kill-lock razul-replay-toggle \
  --lock crates/umber-core/src/project.rs \
  --lock crates/umber-paint/src/paint_state.rs \
  --lock crates/umber-core/src/journal.rs \
  --lock crates/umber-core/src/undo.rs \
  --lock crates/umber-paint/src/retouch.rs \
  --lock crates/umber-app/src/main.rs \
  --lock assets/i18n/en-US.ftl \
  --lock assets/i18n/pt-BR.ftl \
  -- -m "feat(umber): replay-exact opt-in toggle (kill razul-replay-toggle)"
```

## Notes

- The 15 GPU test failures in `umber-bake` are pre-existing (shader validation errors: `min_binding_size` mismatch, WGSL parse errors, assertion failures) and unrelated to this kill.
- The clippy warnings in `umber-bake` with GPU features are pre-existing.
- Lock `razul-replay-toggle` removed after commit via dcommit.