# Kill report: C2-5 paint-channel targeting (the Brush panel picker)

Date: 2026-10-15. Author: dragon. Lock: `c2-5-channel-picker` over
`crates/umber-app/src/brush_panel.rs` and this report. Source:
`docs/loop/audit-2026-10-15-c2.md` C2-5. Before this, painting only
reached Base Color: `PaintState::set_active_channel` existed with no app
caller.

## Built

- A **Channel** group at the top of the Brush panel's paint-session
  rows. It is a combo over `pickable_channels`: the default texture set's
  channels in order (`TextureSet::new_default`: baseColor, roughness,
  metallic, normal, height, …), then any extra channel the paint session
  already holds on tile 1001 (an imported user channel), deduplicated.
- Choosing a channel calls `set_active_channel`. The session snapshots
  the channel at stroke begin (`stroke_channel`, paint_state.rs:1696),
  so a change during a stroke applies from the next stroke. The engine
  routes to that channel's plane, its undo entry and its save sidecar.
  That path was already proven by umber-paint's own tests (a height
  stroke lands in the height plane and is one undo entry,
  paint_state.rs:9457), and this kill only adds the app-side control.
- Labels reuse the View menu's channel keys (`viewmode.roughness` …).
  User channels show their own name.

## Tests

`channel_picker_lists_the_default_set_then_session_extras`: baseColor
first, the four PBR data channels offered, a user channel appended once
with no duplicate of a present standard channel, label keys in both
catalogs. Gate 1 (umber-app): fmt, clippy, 463 tests.

## Judgment calls

- **No new strings.** Both ftl catalogs are under the sibling session's
  live g1-axis-gizmo lock. The group title reuses `viewmode.channel`
  ("Channel"), and a "you are painting a data channel" hint waits for
  the lock.
- **What the user SEES while painting a data channel** is the viewport's
  current view mode. Lit shows height through the normal perturbation,
  but roughness and metallic need Solo (C, the g6-display-mode-keys
  cycle) to be visible. Auto-switching the view on a channel change is a
  UX call, so it is deferred and named, not done silently.

## What this does NOT close

Export still writes Base Color only from paint (C2-6). A painted
roughness channel saves and reloads with the project but does not reach
the exported textures yet. C2-6 is the next kill in the same tier.

## Deferred

- The data-channel hint string (after the lock).
- Optionally, auto-Solo the picked channel in the viewport.
