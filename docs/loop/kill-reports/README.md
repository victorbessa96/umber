# Kill reports — durable home (contract §4)

One JSON per claw kill, dated, copied from the /tmp working copies the
loop's claws write. The /tmp files are the live channel (the loop-status
script reads their sizes+mtimes for liveness); these copies are the
crash-surviving record — "kill reports and the constraint file survive
compaction and crashes; your context does not."

Naming: `YYYY-MM-DD-<kill>.json`, matching the /tmp `umber-<kill>-*-out.json`
pattern. Copy on landing (or at run-end for in-flight kills, as tonight's
sweep did); the gate0 copy at 04:59 predates the claw's exit — refresh it
when the kill is consumed and committed.

## The core-loop night (2026-10-12)

| kill | report | landed as |
|---|---|---|
| strategy-review | 2026-10-12-strategy-review.json | d3516c6 (the review doc + disposition) |
| viewporttex (criticals 1+6) | 2026-10-12-viewporttex.json | e802f92 |
| appwire (the viewport bind) | 2026-10-12-appwire.json | 9455885 |
| tilepool (pool + compositor) | 2026-10-12-tilepool.json | e802f92 (pool) / 133dd12 (4K default) |
| strokeundo (critical 3) | 2026-10-12-strokeundo.json | f78e6c1 + bed9e0a (smudge stacking) |
| pixelsave (critical 4) | 2026-10-12-pixelsave.json | d834d77 + c999da6 |
| target4k (critical 2) | 2026-10-12-target4k.json | 133dd12 + 0c66840 |
| pressure (critical 5) | 2026-10-12-pressure.json | f867215 + 6430623 (source fix) |
| gate0 (critical 6 / B6) | 2026-10-12-gate0.json | in flight at copy time — refresh on landing |

The dragon-authored kills of the night (bed9e0a smudge stacking contract,
6430623 the Event::Touch pressure source) have no claw reports by design;
their receipts live in the Obsidian vault:
- projects/umber/2026-10-12-smudge-stacking-cross-review-receipt.md (sibling's receipt of bed9e0a)
- projects/umber/2026-10-12-pressure-kill-cross-review-receipt.md (this session's receipt of f867215, resolved same-night by 6430623)
