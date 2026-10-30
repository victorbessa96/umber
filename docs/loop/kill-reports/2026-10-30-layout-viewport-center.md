# Kill report: 2026-10-30 layout-viewport-center — Painter-faithful default dock

**Status: Gate 1 battery running — committing on green.**

- **Kill**: `layout-viewport-center`. Bessa's report 2026-10-30: "the
  layout is currently wrong, the viewport should be the largest section
  and on the centre of the screen, right now it's on the corner. Check
  how Substance Painter, photocraft and others do their layout, check
  the current one we have and fix/improve what's needed."
- **Reference research** (web this date): Adobe's own Experience League
  docs for Substance 3D Painter — `interface/viewport/viewport`: "The
  default layout puts the 3D view on the left and the 2D view on the
  right", both inside the viewport region, with a contextual toolbar on
  top of the viewport and a progress bar at its bottom; Viewport Mode
  3D/2D | 3D only | 2D only | Swap (which umber already maps to F1/F2/F4/
  F5). `interface/toolbars`: the Tools toolbar docks vertically at the
  left edge, the Docks toolbar at the right edge, and all panels (Assets,
  Properties, Layer stack, History, Texture Set, Display settings…)
  dock on the edges. Blender/Photopea-class editors follow the same
  pattern: dominant centre canvas/viewport, narrow rails on the edges.
  (web_search was dry this session; the layout facts above are from the
  Adobe docs pages fetched directly, plus the repo's own gui-audit bar
  quote: "Against Substance Painter (the bar)".)
- **What was wrong**: `shell_menu::default_dock` (the Paint preset)
  put [Viewport, UvView] as TABS in the centre leaf with a 0.25-wide
  left column and 0.2 right column — and users' persisted layouts
  (LAYOUTS_VERSION 1) kept whatever arrangement they had saved, which is
  why the viewport could end up in a corner on a real machine.
- **Kill-lock**: `layout-viewport-center` — shell_menu.rs,
  workspaces.rs, this report.

## What landed

1. `default_dock()` rebuilt Painter-faithful:
   `[left rail 0.15 | 3D view | 2D UV view | right rail 0.24]` —
   right rail = Properties on top (62% of the rail) with
   LayerStack/History/Graph/Display tabbed below (Painter: properties
   upper right, layers/history lower right); left rail =
   TextureSets/Assets/Bakes/Export; the centre split is 65/35 with the
   3D viewport left and the 2D UV view right (Painter's default 3D/2D
   mode). The viewport is now the single largest section, dead centre.
   F2/F4 still collapse the pair to one view, which then fills the whole
   centre (the viewport-preset machinery is unchanged).
2. `workspaces::side_layout` (Bake/Export presets) gets the same centre
   split (rails narrowed 0.28/0.22 → 0.18/0.24).
3. `LAYOUTS_VERSION` 1 → 2: existing saved layouts fall back to the new
   presets on load (the documented mechanism), so the fix is visible at
   boot without Window ▸ Reset UI. Custom layouts made after this
   version persist as before.
4. Tests: new `paint_preset_splits_the_views_side_by_side_in_the_centre`
   (the two views are separate leaves, not tabs; every panel docked
   exactly once); the one-leaf Swap test now builds its own merged leaf
   since the Paint preset no longer has one. One battery red caught and
   fixed during the kill: `split_*` ADDS tabs without removing them
   from the split node, so the first draft duplicated Viewport
   (`[Viewport, Viewport, …]`) — the dock now starts as `[UvView]` and
   the centre split adds Viewport as its left leaf.

## Not done here (named honestly)

- Painter's *contextual toolbar* floating on top of the viewport (brush
  params when painting) is a separate widget kill; the top strip stays.
- Visual confirmation needs the live desktop — desktop-checks 1-3 stay
  deferred (Bessa at the desktop per the standing note).
