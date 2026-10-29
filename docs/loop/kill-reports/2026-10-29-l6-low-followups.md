# Kill report: 2026-10-29 l6-low-followups — the pi review's two LOWs, closed

**Status: code landed + rustfmt clean; Gate 1 battery running at write
time. Report completes at the commit step.**

- **Kill**: `l6-low-followups` — both LOW follow-ups of the 2026-10-27
  pi cross-review of the L6 unified timeline, as dispositioned in
  `.dragon/COORDINATION-next-wave.md` lines 1895-1903 ("TWO NAMED
  FOLLOW-UPS from the review").
- **Wave**: local-4e9bde4c's 2026-10-29 wave, wave-lock
  `wave-10-29-code`.
- **Authorship**: this session, directly.

## What landed

1. **LOW-1 — the History panel's Undo/Redo buttons now see same-frame
   graph commits.** `main.rs`, `Panel::History` arm of the dock
   viewer: before `history_ui_with_graph` runs, the viewer drains the
   graph queue itself (`take_graph_commits` +
   `history_panel::apply_graph_commits(doc, epoch, commits)`). The
   frame-top drain still runs; this take is the between-panels
   remainder — the Graph panel may have drawn earlier in the same dock
   pass and queued a commit the frame-top drain could not see. The
   viewer is the one call site holding both journals, which is why the
   fix lives there and not in the panel.
2. **LOW-2 — the load test now pins the undrained-pending death.**
   `graph_panel.rs` `load_graph_queues_one_row_and_bumps_the_epoch`:
   the pre-load `take_graph_commits()` is REMOVED, so the pending
   `add_node` commit is deliberately undrained across `load_graph`;
   the post-load take must return exactly ONE row (`GraphLoad`,
   cursor 0, depth 0), proving `bump_epoch`'s clear-pending killed the
   stale add. Before this change the test drained first, so the
   clear-pending path was asserted only by its comment.

## Gates

- rustfmt: `cargo fmt --all -- --check` clean after apply.
- Battery: `wave-battery.sh umber-app` (Gate 1) — see wave claim entry
  for the run; verdict consumed before commit.

## Not done

- The review's remaining items were MEDIUM/deferred by design (the
  compound one-step flatten belongs to the follow-up queue, not this
  kill); nothing silently dropped.
