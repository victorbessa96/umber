# Batch brief: W-GUI-3 shell halves — the six lock-queued wirings

Prepared 2026-10-17 ~06:35 by the loop session (host-back iteration),
pre-written against the W-GUI-2 data halves' landed APIs so dispatch
costs one command after the main.rs lock releases at the W-GUI-2
commit. Every API below was verified against the tree (file:line),
NOT from memory.

You are wiring SIX already-built data halves into the Umber shell, in
the Rust workspace at ~/Projects/umber (dispatch HEAD = whatever the
W-GUI-2 push landed; re-verify line anchors before editing). All six
queue on the SAME surface: `crates/umber-app/src/main.rs` (held with
document.rs + layers_panel.rs by the `wgui2-gesture-integration`
lock; preferences.rs + toolbar.rs by their own locks). That means ONE
integration kill, not six — dispatch as a single claw, one kill report
`docs/loop/kill-reports/<date>-wgui3-shell-halves.md`.

VERIFICATION LADDER (mandatory before claiming done): (1)
~/.cargo/bin/rustfmt --check --edition 2021 on every .rs you touched;
(2) resolve EVERY external API against the pinned registry source — a
guessed API is a fabricated API; (3) for arithmetic-only logic, port
to Python + mutate one branch; (4) for any new struct field or enum
variant, sweep every construction site and exhaustive match. State
which rungs you ran in the report's Unverified section. NO cargo
build/test (the dragon gates through wave-battery on the host).

## 1. ContextRail dispatch (g2-properties-context, the shell half)

Kill report: docs/loop/kill-reports/2026-10-16-g2-context-rail.md
(the widget + its API). The wiring:

- `ContextRail::ui<R>(&mut self, ui, ctx: ContextRailCtx, body)` ->
  `(ContextRailOutcome, R)` — context_rail.rs:369. The ctx
  (context_rail.rs:145) carries `atlas: &IconAtlas`, `tooltips:
  [&str; ContextKind::COUNT]` (5 kinds: Brush/Layer/Fill/Mask/
  TextureSet — context_rail.rs:45), `breadcrumb: &[String]`
  (outermost first), `search_hint: &str`.
- The shell decides APPLICABLE kinds per selection target: a fill
  layer shows Fill; a mask target shows Mask; every target shows
  Layer + TextureSet. Hidden tabs are ABSENT from the rail, not
  greyed (the plan row's rule).
- Breadcrumb: "Set ▸ Layer 3 ▸ Mask" from the active texture set +
  selected layer + mask focus. Build the Vec<String> per frame.
- Dispatch on `ContextRailOutcome { switched, search_focus,
  query_changed, jumped_to, cleared }` — switched drives which
  property body the closure draws; search_focus/query_changed/cleared
  are consumed flags (no shell state needed beyond focus).
- KNOWN GAP (named in the widget kill's report): atlas ids `layers`
  and `layout-grid` do not exist — reuse existing ids; if the rail
  needs one, NAME it in the report, do not invent an atlas entry.
- Kittest pin (the plan row's test): selecting a fill layer shows
  the fill tab.

## 2. a11y umber-app half (g7-accesskit-sweep)

Kill report: docs/loop/kill-reports/2026-10-16-g7-accesskit-sweep.md
(the umber-gui half: 6 widgets done). The umber-app half = the
painter-drawn widgets the plan row lists and the gui half could not
reach: LAYER ROWS (layers_panel.rs — thumbnail cells, the visibility
eye), the AXIS GIZMO (axis_gizmo.rs — the hit region), the brush HUD
readout if painter-drawn. Pattern to copy verbatim from the gui half:
`Response::widget_info` with `WidgetInfo::labeled` (egui 0.36.2
`data/output.rs:658`) at the interactive Response's allocation, one
comment marker `g7-accesskit-sweep` per site, one kittest per widget
(the `get_by_role_and_label` family panics on miss — the query IS the
assertion). English-only labels at the widget layer is the
established ruling; localization is the panel's job.

## 3. density Preferences row (g7-density-scale, the shell half)

Kill report: docs/loop/kill-reports/2026-10-16-g7-density-scale.md.
The data half landed: `ui_scale_auto: bool` (default ON = native
pixels_per_point; no style_delta zoom while on) + `ui_scale: f32`
manual value that SURVIVES a toggle back; `HitZone { Standard,
Tablet }` resolving to SPLITTER_HIT_ZONE 4.0 / 8.0 pt (theme.rs).
The wiring:

- Preferences row: an Auto checkbox bound to `ui_scale_auto`; a
  ValueField on `ui_scale` whose write sets `ui_scale_auto = false`
  (the pinned contract: writing a number IS leaving Auto — the
  re-pinned `style_delta_is_empty...` test already encodes it).
- Apply on change, not per frame: the existing sync-once-per-change
  Preferences pattern (g3-preferences-window's ruling).
- The dock style application: egui_dock 0.21.1
  `SeparatorStyle::extra_interact_width` set from the resolved
  SPLITTER_HIT_ZONE per HitZone — the dock's style write happens
  where the app styles the dock, once per change.
- i18n keys ALREADY EXIST: `prefs.ui-scale-auto`,
  `prefs.splitter-hit-zone` (both catalogs, landed with the data
  half) — do not add new keys.

## 4. held-key poll (held-key-tools, the shell half)

Kill report: docs/loop/kill-reports/2026-10-16-held-key-tools.md.
The state machine is `TempTools` (toolbar.rs:617): `press(key,
current_tool, focus) -> Option<ToolKind>` (text focus swallows; one
hold at a time; hold-E = Erase + remembers previous), `release(key)
-> Option<ToolKind>` (restores once; stray release no-op),
`panning() -> bool` (the viewport's pointer switch). The wiring:

- RAW key poll, BELOW the action registry: Key::E and Key::Space
  down/up read raw (the registry binds Cmd+Space to ToggleMaximize;
  the hold poll must not fight it — gate on no modifiers, or at
  least not COMMAND).
- press/release feed TempTools; a returned ToolKind goes through the
  EXISTING SelectTool action path (do not invent a second dispatch).
- `panning()` switches the viewport's pointer interpretation
  (orbit/pan) for the held frames; Space never touches the strip.
- Focus::Text gating is already inside TempTools — the shell only
  passes the current focus; find the one canonical focus source the
  other input paths use and use THAT, not a new one.

## 5. TypeAhead + dialog_buttons consumers (polish-widgets)

Kill report: docs/loop/kill-reports/2026-10-16-polish-widgets.md
(widgets registered + re-exported at wave end:
`TypeAhead`, `ButtonRole`, `DialogButton`, `dialog_buttons`).
APIs: type_ahead.rs:24 `pub struct TypeAhead`, :41 `pub fn frame`,
:100 `pub fn prefix`; dialog_buttons.rs:23 `ButtonRole`, :55
`DialogButton::new(role, label)`, :69 `platform_order(os,
&[(usize, ButtonRole)])`, :97 `dialog_buttons(...)`. The wiring:

- The blend-mode combo in brush_panel.rs (the falloff combo at
  brush_panel.rs:595 is the same pattern — the blend-mode combo is
  its sibling) becomes a TypeAhead combo for long lists.
- The ✕-with-confirm pattern (delete-with-confirm rows) adopts
  `dialog_buttons` + `platform_order` so button order follows the
  platform convention (Windows/macOS/Linux classes from
  `ButtonRole::order_class`).
- ONE consumer pair is enough for the kill's shape (one combo + one
  confirm dialog); list further adopters in the report as follow-ups
  rather than touching every site.

## 6. psd publish binding (psd-publish, the shell half)

Kill report: docs/loop/kill-reports/2026-10-17-psd-publish.md. The
pure half: psd_import.rs:1025 `publish_psd_pixels(flat: &PsdImport)
-> Vec<u8>` (Photoshop gamma-space composite → the paint planes' RGBA,
the C2-32 rule). The binding, from the kill report verbatim:

```rust
publish_plane(PlaneOwner::Layer(id), 1001, "baseColor", w, h,
              publish_psd_pixels(&flat))
```

- `publish_plane` signature (umber-paint/src/paint_state.rs:3275):
  `(owner: PlaneOwner, tile: u16, channel: &str, width: u32,
  height: u32, rgba: Vec<u8>) -> Result<(), PaintRefusal>`. It
  DROPS (warns + Ok) on a size mismatch — pass the flat import's own
  w/h, never a recomputed size.
- `PlaneOwner::{Base, Layer(u64)}` (paint_state.rs:592).
- Tile 1001 + channel "baseColor" is the C2-32 pinned convention
  (the same landing the sRGB linearize fix verified, 188 -> 128).
- The call site: the PSD drop's apply path in main.rs, once per
  imported PSD layer, in the same transaction as the layer creation.

## 7. g4 curve consumers (census finding, NOT lock-queued — separate)

curve.rs's own doc (curve.rs:4) promises "pressure, falloff Custom
and Levels share one editor" — the editor landed (Curve: insert/
move/remove/evaluate/sampled/to_text/parse, curve.rs:72-221) but the
two consumers were NEVER built. This is NOT part of the shell-halves
kill (different surfaces): falloff Custom needs a FalloffCurve::Custom
variant (umber-brush/src/falloff.rs:48 — the enum is
serde-lowercase, ids are the umber_gpu::paint::FALLOFF_* constants,
NEVER renumber; the GPU `radial_falloff` switch + its Smooth
fallback must carry the new id) and Levels means the GPU graph's
`GpuOp::Levels` (umber-gpu/src/graph_eval.rs:157) growing a
CurveEdit param. Do them as their own kills AFTER the shell halves —
falloff Custom touches a GPU shader switch (highest blast radius in
this set), Levels touches the graph evaluator. Both need the host.

## Order of operations for the consuming session

1. Confirm the W-GUI-2 commit landed (locks released: `bash
   scripts/kill-locks.sh list` shows none of wgui2-*).
2. Dispatch the shell-halves kill (sections 1-6, ONE claw, one
   report). Battery: `bash scripts/wave-battery.sh umber-app
   umber-gui umber-core umber-paint` on the host.
3. Sections 7 kills queue after, each with its own report.
4. Docs tick (gui-plan rows, STATE, queue wave log, CHANGELOG)
   rides W-GUI-3's push per the one-push rule.
