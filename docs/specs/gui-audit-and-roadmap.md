# The Umber GUI Audit + Long-Horizon Plan

**The date**: 2026-10-10, late session. **The auditor**: the dragon, adversarial, fresh-context.
**The constraint honored**: code-only (no cargo) until Bessa lifts it — this plan's Phase 0 is code-only; the rest gates on the machine.

---

## PART 1 — The adversarial audit (what the GUI actually is, not what we tell ourselves)

### 1.1 The honest inventory

What EXISTS and works (verified in tree this session):

- **The dock shell**: egui_dock 0.21, `DockState<Panel>` with 12 panel kinds
  (Viewport, UvView, Bakes, Export, Graph canvas, Layers/History, Brush
  properties, Display, Fill layer, Tile selection, Plugins, Retouch params).
  Tabs re-arrange, split, float — the structural floor is real.
- **The 2D/3D dual views**: the wgpu viewport (OpenPBR mesh pass, IBL, view
  modes, symmetry overlays) + the UV view (paint canvas + wireframe) — the
  CORE identity, and it's the strongest part of the app.
- **i18n**: fluent catalogs en-US + pt-BR, every panel migrated. Rare for a
  hobby-stage DCC; real professional polish.
- **The panels carry REAL state**: these aren't mockups — the brush panel
  drives the actual stroke pipeline, the bakes panel queues real GPU bakes.

### 1.2 The indictment (the nine counts)

**Count 1 — Default visuals = "hobby project" first impression.** The app
boots into stock dark egui: the gray `Visuals::dark()` palette, stock fonts,
stock spacing. Against Substance Painter (the bar), Umber's chrome reads as
a debug tool. The FIRST 10 seconds decide "professional tool" vs "student
project" — and we fail them today. This is the single highest-leverage fix
in the entire project.

**Count 2 — No icon system.** Text-only buttons everywhere. Every pro DCC
uses iconography for tool rows (brush/eraser/smudge/clone/fill) — the eye
scans icons 10× faster than labels. We have zero icons. A "B" and an "E"
lucide-style glyph row would transform the tool strip.

**Count 3 — The panels are farms of undifferentiated sliders.** `slider()`
after `slider()` with no visual hierarchy: no group headers, no read-only
value displays, no compact numeric-with-drag-scrub fields, no collapsible
sub-sections. Pro DCC panels have visual GRAMMAR — a parameter group reads
as a unit. Ours read as a list.

**Num**

**Count 4 — No layout density control.** One density for everything. Pro
tools ship compact/dense modes; artists live in dense. egui can do this
(spacing scale) — we don't touch it.

**Count 5 — Zero animation/motion feedback.** Panel collapse = instant snap.
View-mode switches = hard cuts. No hover transitions on tabs. egui's
`ctx.animate_value_with_time` exists and we never call it once. Motion is
how a UI "feels alive" — we feel dead.

**Count 6 — The theming story is absent.** `Style::from_egui(ui.style())`
for the dock — we consume, never author. No Umber brand palette, no
accent color, no selection/hover/active states tuned to painting (dark
neutral chrome + ONE saturated accent is the DCC convention — Painter/
Blender/Krita all converge on it).

**Count 7 — Color pickers are stock.** For a TEXTURE PAINTING app the color
picker is a flagship surface — SV square + hue ring/slider + hex + swatches
+ harmony hints. egui's stock `color_picker` is functional-but-generic.
Ours must be a custom widget: the SV wheel, recent colors, per-channel
sliders, the eyedropper integration.

**Count 8 — No onboarding/empty states.** Boot with no mesh loaded → the
viewport shows... nothing, plus zero guidance. Pro apps ship a
drop-your-FBX hero screen. First-run experience is unconsidered.

**Count 9 — The toolbar/shortcut layer is thin.** Pro tools have a
persistent top toolbar (new/open/save | undo/redo | the tool row) + a
status bar (mouse position, tile under cursor, fps, GPU memory). We have
neither. The keyboard-shortcut system exists (E for eraser etc.) but is
inconsistently surfaced.

### 1.3 The verdict

The ENGINE under the GUI is genuinely strong — the strongest part of the
project (verified across 57 P0 rows this session). The GUI above it is
structurally sound (the dock, the panels, the dual views) but VISUALLY
BANKRUPT. The gap is not capability; it is the 2–3 week "chrome" layer
that every successful DCC invests in and we skipped. **The good news: none
of the nine counts require architecture changes. They are presentation-
layer work on a healthy body.**

---

## PART 2 — The design language (decided, not discussed)

The "modern dynamic GUI" target, concretely:

1. **The Umber palette** (authored, committed as a theme module):
   - Chrome: near-black neutral (‹#16181D› bg, ‹#1E2128› panels, ‹#262A33›
     raised, ‹#333842› hover) — slightly WARM dark, not the cold blue-gray
     of stock egui.
   - Text: ‹#E8E9EC› primary, ‹#9CA0AB› muted, ‹#6A6E78› faint.
   - Accent: a single saturated ember-orange ‹#FF7A1A› (Umber = amber +
     umber earth pigment — the brand) for selection/active/values. Hover
     deepens, never shifts hue.
   - Semantic: success ‹#3FB972›, warn ‹#E5A13D›, error ‹#E5534B›.
   - Viewport chrome: even darker (‹#111317›) so the 3D view dominates.
2. **Typography**: Inter (UI) + JetBrains Mono (numeric fields/terminal
   vibes) — embedded TTFs, no system-font roulette. Sizes: 13px UI, 11px
   labels, 10px status bar; the density scale ±20%.
3. **Icons**: lucide static SVGs, pre-rasterized per density → an egui
   `TextureHandle` atlas per family (tool icons 20px, panel icons 16px).
   The tool row: paint/eraser/smudge/clone/fill/path/polygon/mask/symmetry
   + the top-strip family (file/edit/undo/redo/view/panel toggles).
4. **Motion**: 120ms ease-out as the house curve; animated panel-collapse
   (height lerp), view-mode crossfades, accent pulse on record-arm, the
   dock tab's underline slide. `animate_value_with_time` everywhere.
5. **Density**: a compact toggle (settings) scaling `spacing`/`spacing/2`,
   plus 4K-aware DPI scaling (already half-there via egui).

**The reference bar** (the three screenshots taped to the wall):
Substance 3D Painter's 2023+ dark theme (the density + icon grammar),
Blender's 4.x property-editor grouping (the collapsible sections), Krita's
tool-options strip (the always-visible per-tool rows).

---

## PART 2.5 — The question Bessa actually asked: HOW BEST to do this

The honest engineering answer, adversarial to my own plan:

**The trap** would be a "big bang GUI overhaul" branch: 4 weeks, 200+
files touched, merge hell with the four hunting P1 kills, and no working
app at the end until ALL of it lands. REJECTED.

**The right way** — the stranglehold pattern that worked all session:

1. **Establish the theme foundation as ONE small vertical slice first**
   (Phase 0a: the `umber_gui` presentation crate — theme tokens, fonts,
   icon atlas, the 4 foundational widgets). Land it. Commit. Use it in ONE
   panel (the Brush panel). Verify the pattern. THEN fan out.
2. **Every subsequent phase = a panel-by-panel migration**, each one a
   kill-sized unit (one claw, one panel, one commit), so the app stays
   shippable at every commit and the kills never collide (each claw owns
   ONE panel file — zero overlap with the parallel P1 kills).
3. **The engine-facing contracts stay frozen**: the panels keep their
   current public state structs; the migration is Ui-code-only. No
   refactor of paint_state/bakes/gpu — the GUI layer reads, it doesn't
   reorganize.
4. **The verification is visual + static**: cargo-free checks during the
   machine-off era (the tree's parse gate, the widget unit tests), then
   the CI runtime gate, then screenshots on the machine's return.

Why this beats the alternatives:
- **vs. forking the dock lib**: no — egui_dock 0.21 is fine; it takes a
  `Style` struct; we author one, we don't fork.
- **vs. an immediate rewrite to a retained-mode UI (e.g. react-like)**:
  absolutely not — egui's immediate mode is what makes 12 panels cheap
  and the GPU-callback views trivial. The DCC convention is immediate
  layers everywhere; we're on the right stack.
- **vs. CSS-like theming libraries (egui-theme etc.)**: they buy little;
  our palette is 12 tokens and a Style authoring fn — own it in-tree.
- **vs. hiring the "design first, code later" order**: reversed — we
  commit the palette + widget shapes as code and iterate against the
  live app, because DCC GUIs are only judgeable live.

**The one process rule**: no panel gets migrated twice. Each migration
lands the full grammar for that panel (palette + icons + grouping +
density + motion where earned) — half-migrations are how GUI work
sprawls.

---

## PART 3 — The roadmap (4 phases, kill-sized, CI-green at every step)

### Phase 0 — Foundation (the umber_gui crate) — ~3 sessions, code-only

**0a. The theme crate** (`crates/umber-gui/`): 
- `theme.rs`: the token table (§2's palette) → `egui::Style` constructor
  `umber_style(density: Density) -> Style`; the semantic helpers
  (`accent()`, `panel_bg()`, `hover()`).
- `fonts.rs`: Inter + JetBrains Mono embedded (include bytes), the
  FontDefinitions builder, tabular-numerals on for value fields.
- `icons.rs`: the lucide SVG set → the per-density atlas; `icon(ui, name)`
  widget fn + `icon_button` with hover/active states.
- `widgets/`: the four foundational widgets:
  - `param_group` — the labeled section (collapsible, the Blender-style
    group header, counts its children).
  - `value_field` — numeric with drag-scrub + units + wheel-step +
    tabular font + copy on right-click (the DCC workhorse).
  - `tool_row` — the icon-button strip with the active-tool accent
    underline (the animated one).
    - `swatch_grid` — recent colors + the custom SV/hue picker (Phase 2's
    seed).
- `main.rs` wires: `umber_app` applies the style + fonts at boot; ONE
  panel (Brush) migrates as the proof.
- **Gate**: the Brush panel screenshot-matches the design language; the
  tree parses; CI green.

**0b. The shell chrome**: top toolbar (file ops | undo/redo | the tool
row) + bottom status bar (coords/tile/fps/GPU-mem) + the drop-zone hero
empty state for no-mesh. `toolbar.rs` + `status_bar.rs` in umber-app,
built FROM umber_gui parts. Wire the 9 shortcut actions.
- **Gate**: the app reads as a chrome'd app, not a panel dump.

### Phase 1 — Panel migration wave (12 panels, kill-sized, parallelizable) — ~5 sessions

Each kill = one panel, full grammar (palette + icons + param groups +
value fields + density + motion-where-earned). The order by user-value:
1. **Layers/History** (the highest-traffic surface; also lands the
   layer-thumbnail infrastructure — texture readback → egui texture).
2. **Bakes** (the checklist + progress rows — the bake log pattern).
3. **Export dialog** (the presets grid → icon cards).
4. **Graph canvas** (node rendering with the accent wires + the grid
   backdrop; node-visual polish is its own sub-phase — node headers,
   port dots, bezier wire hit-testing).
5. **Display** (the LUT chain rows).
6. **Fill layer** + **Tile selection** (the small panels, fast wins).
7. **Brush properties** (the curve editor gets the custom-plot chrome —
   the biggest single-widget investment; the input→curve editor is the
   most-watched widget after the viewport).
8. **Retouch params** + **Plugins** (the tail).
- **Gate**: zero stock-egui-looking panels remain; every panel
  screen-shots consistent.

### Phase 2 — The flagship widgets (the differentiation layer) — ~4 sessions

1. **The color picker** (`widgets/color_picker.rs`): the SV square + hue
   ring (custom-drawn), hex/rgb/hsv rows, recent swatches, eyedropper
   (screen-space via egui's screenshot+pick), alpha row. THE painting
   app's signature widget.
2. **The curve editor** (brush dynamics): multi-point cubic spline
   editor with the accent, drag handles, presets row. (Upgrades the
   existing one, not a rewrite — read the current impl first.)
3. **The node canvas polish**: zoom/pan momentum, node auto-layout
   (layered, deterministic), minimap, port hover states.
4. **The paint-cursor layer**: brush outline circle in-viewport with
   hardness preview alpha, the symmetry ghost, quick-mask tint border.
   (Reads viewport.rs's existing overlay passes — extension, not
   rebuild.)
5. **Tooltips**: rich (icon + title + shortcut + one-line help), i18n'd,
   delayed 400ms — the discoverability layer.

### Phase 3 — Motion + delight + the long tail — ~3 sessions

1. **Motion pass**: the collapse animations, the tab underline slide,
   view-mode crossfade, dock-splitter hover, value-scrub tooltip follows
   the cursor, busy spinners on bake/export.
2. **Empty/loading states**: the drop-zone hero (mesh drop → import), the
   texture-set picker for first paint, the "no GPU adapter" fatal screen
   (branded).
3. **First-run experience**: a light onboarding overlay (3 cards:
   drop a mesh / paint / bake+export), skippable, remembered.
4. **Settings panel**: theme density, UI scale, language picker (live
   catalog switch), the shortcut map view (read-only v1).
5. **Accessibility**: the tab-2 piloting, focus visible, tooltips as
   the accessible descriptions.

### The tail beyond Phase 3 (P2 tier, recorded not scheduled)

- QML-style .umbergui layout files for user panel skins (P2 §7 row).
- User-editable themes as TOML (P2 row) + theme import/export.
- The ribbon? No. Never. The dock+toolbar shape is correct.
- HDR surface output + post-effects (P1 engine rows) get their viewport
  widgets when the engine rows land.

---

## PART 4 — The sequencing answer (what runs when, given the era)

- **Now (machine-off, code-only)**: Phase 0a — the umber_gui crate is
  PURE code (theme tokens, fonts, icons, widget fns): perfect
  claw-shaped work, no GPU needed, no cargo needed. **Dispatch 0a as
  the next kill.** The four P1 kills run to completion first; umber_gui
  starts the moment they land (avoid tree collisions on main.rs /
  paint_state).
- **Machine's return**: the local gates (fmt, clippy, the test suite)
  + the CI verdicts, then Phase 0b's live tuning.
- **The one-week cadence**: one phase per ~week of evenings; every kill
  shippable; no long-lived branches.

**The success test**: put Umber next to Substance Painter 2026 on a
4K screen. If the chrome doesn't embarrass us — if the first-impression
scan reads "professional tool" — Phase 3 is done.
