# Kill report: a painted stroke makes the document dirty

Date: 2026-10-15. Author: dragon. Lock: `stroke-dirty` over
`crates/umber-app/src/{history_panel,document}.rs` and this report. Found
while scoping C2-3, right after the C2-1 quit guard landed.

## The bug

`Document::dirty` was set only by `Document::run` (layer commands). Paint
strokes never pass through `run`; they ride the paint session's stroke
journal. A session that painted and never touched a layer read CLEAN, so
the new quit guard, File ▸ New and File ▸ Open all let it go without a
prompt, and the paint was lost. The doc comment claimed strokes were
"covered by the journal's existence check". No such check exists in the
gates, and the autosave journal records layer ops only.

## Fix

`history_panel::apply_stroke_commits`, the one chokepoint every committed
stroke passes (the per-frame drain), now sets `doc.dirty` when any commit
arrived. The doc comment on `Document::dirty` is corrected.

## Test

`a_committed_stroke_makes_the_document_dirty`: no commits stays clean;
one commit makes it dirty. Gate 1: umber-app 513.

## Note

The quit guard reads `is_dirty()` this frame. The drain runs every frame
before the shell UI (see `drain_stroke_commits`' docs), so a stroke
painted this frame is dirty before a close in the same frame is judged.
