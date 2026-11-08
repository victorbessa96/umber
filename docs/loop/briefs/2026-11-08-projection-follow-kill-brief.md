# Kill brief — row 288: follow-camera projection (R-A2 v2b)

You are a claw in the Umber dragon loop. Read
`docs/loop/briefs/2026-11-08-projection-follow-design.md` FIRST — it is the
contract. Implement exactly that design.

## Hard rules

- NEVER commit, push, stash, checkout, reset, or run cargo/rustfmt — the
  dragon owns those. Your only writes are the source edits below.
- Your lock is `claw-projection-follow` over EXACTLY:
  `crates/umber-app/src/projection.rs`,
  `crates/umber-app/src/viewport.rs`,
  `crates/umber-app/src/display_panel.rs`,
  `crates/umber-app/src/main.rs`,
  `assets/i18n/en-US.ftl`,
  `assets/i18n/pt-BR.ftl`.
  Do not touch any other file. Do not call `scripts/kill-locks.sh`.
- No new dependencies. Both .ftl catalogs get the two new keys (completeness
  test scans both). No em-dashes in user-facing strings.

## The change (summary — the design doc is the full contract)

1. `projection.rs`: `ProjectionState.follow_camera: bool` (default false,
   untouched by adopt/arm/disarm/tiling). `arm()` freezes only when follow
   is OFF. Extract the per-texel core into a private
   `rasterize_with(state, mesh, tile, w, h, camera, rect)`;
   `rasterize_projection` keeps its signature and requires `frozen`;
   NEW `rasterize_projection_live(state, mesh, tile, w, h, camera, rect)`.
   Pins 1-5 from the design doc.
2. `viewport.rs`: `Viewport::apply_projection` picks live-vs-frozen by
   `projection.follow_camera && projection.armed` (live camera + the captured
   view rect are already at the call site). Refusal semantics unchanged.
3. `display_panel.rs`: `ProjectionRow.follow_camera: &'a mut bool` + the
   "Follow camera" checkbox next to Arm. Panel pin.
4. `main.rs`: pass `follow_camera` through the projection section.
5. Both .ftl catalogs: `overlays.projection-follow`, `overlays.projection-follow-hint`.

## Out of scope (named cuts)

No per-stroke continuous re-projection, no depth/backface culling, no
persistence, no overlay change, no interaction with the perf hunt.

## When done

Report: files changed, pin names added, deviations named with why. Exit 0.
The dragon runs the gates, commits with the lock, pushes, watches CI.
