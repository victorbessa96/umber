# Design brief: 8K COW vs dirty-tile undo A/B (2026-11-25)

Planned kill lock: `razul-cow-ab`.
Base: theme-matrix status tip `b772f8191d3942c85fb5ecd33fa1bffc80346e6f`, clean and origin-equal.

## What exists

`record_stroke` stores each region's before + after bytes. A full 8192² RGBA8 tile costs 256 MiB per side, 512 MiB per unmerged entry. Overlapping strokes merge to the oldest before + newest after plus the split-redo journal, so three full-tile strokes cost 1 GiB. The existing 64-entry / 256 MiB envelope evicts oldest entries. There is no COW rewrite to adopt by default.

## Target

- Add a temporary test-only A/B probe to `retouch.rs` that compares the real `record_stroke` path against a sparse 64×64 COW prototype using `Arc<Vec<u8>>` and block-local clones.
- Measure three cases at 8192²: one full-tile stroke, three overlapping full-tile strokes, and three overlapping 64×64 strokes. Record baseline bytes, COW bytes, and median elapsed microseconds for each.
- Assert final bytes match between the two models, then remove the temporary probe before committing only the report/status surfaces. No production COW implementation is retained.
- **Decision rule (pre-registered):** adopt COW only when its history bytes are at least **1.5× smaller** than the real dirty-journal baseline at no worse than **1.25× median latency**. Otherwise defer. Report the exact ratios, not a generic win.

## Failure handling

Do not rewrite `UndoStack`, `record_stroke`, or the tile pool. Do not report modeled numbers as measurements. If allocation fails or the host cannot finish, report that blocker instead of fabricating results. The measurement must be actual 8192² data, not scaled-down evidence.

## Verification and acceptance

- `cargo test -p umber-paint cow_ab_probe -- --nocapture` emits actual baseline/COW bytes and elapsed medians, with final-byte equality assertions.
- Strict clippy on `umber-paint`, fmt, layers, scorecard, parity, and `git diff --check`.
- Independent read-only review of the measurement and adopt-or-defer conclusion, report, safe-push, and `git ls-remote origin master`.

## Non-goals

No production COW rewrite, no new undo feature, and no claim that a prototype is a performance improvement without measured evidence. This is the last code-ready measurement before the final origin check.
