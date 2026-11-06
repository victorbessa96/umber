# Umber UI/UX pass (2026-11-05): audit, requirements, plan

Author: Razul (session local-8dc7062e). Commissioned by Bessa 2026-11-05: "prioritize the UI/UX. Get it complete, use the blender and photocraft projects as reference, get it all to the best next-gen quality, start with the audit, requirements, planning and then implement it all."

This document supersedes nothing. It builds on `docs/specs/gui-plan-2026-10-15.md` (waves G0-G7, 36/43 kills landed per `docs/loop/audit-2026-10-24-lens-uiux.md`) and audits what a user actually SEES today, because the G-plan census counted landed code, not the rendered result.

## 1. Method

1. **Live render audit.** The release binary (`target/release/umber`, built 15:42 today) ran in a scratch profile (`XDG_*` under `~/.hermes/cache/scratch/uiux-audit`) with `--control 8391`. `ui.inspect` gave every dock leaf's rect; `viewport.screenshot` gave the pixels (1920x984 physical, 1423x729 logical at 1.35 scale, the minihost display). Screenshots were reviewed region by region.
2. **Code audit.** Each visible defect was traced to its code cause and widened to its class with `rg` counts.
3. **Font coverage audit.** Every non-ASCII character in `assets/i18n/en-US.ftl` and in UI string literals was checked against the glyph maps of the bundled fonts (Inter, JetBrains Mono) and egui's default fallbacks.
4. **Reference survey.** photocraft at `3317aede3` (origin/main, 2026-11-05 19:27 UTC, verified 0 behind), its new shared UI library craft-libs at `a8b996d` (origin/main), and Blender 4.3 to 5.0 release notes and manual.

## 2. What the references changed since 2026-10-15

### 2.1 photocraft and craft-libs

- photocraft landed 5,810 commits since the 10-15 survey. Most are Photoshop-parity features; the UI-relevant ones are a 2-column Home screen with recent-file cards that hides the tools, options bar, status bar and docks while Home is up (`aa08b4007`, `51131b82c`), live preview in dialog color pickers, Layers panel filters, and Reset buttons in option bars.
- **Reusable UI moved into `craft-ui`** (github.com/storytold/craft-libs, `crates/ui`, about 41k lines, MIT OR Apache-2.0). It now owns: a docking model and renderer (`docs/ui-docking.md`, 816 lines), tab fitting with "…" overflow, split geometry with keyboard-resizable handles, accessible icon buttons, SVG icon catalogs, the font stack, locale-aware number fields and screen-test helpers. Patterns that matter for Umber:
  - **Docks have a floor width.** photocraft `crates/ui-egui/src/panels/dock.rs`: `DOCK_MIN_WIDTH = 250.0`, and the dock never takes more than `window - 400` (`DOCK_LEAVES`), so the canvas always keeps room.
  - **Tabs shrink to a floor, then overflow.** `min_tab_width` is about three characters plus "…" (54 pt at 12 pt). Tabs that do not fit go to a "…" menu; the active tab always shows.
  - **The canvas is a protected panel** (`Permissions::PROTECTED`): it cannot be closed, floated or tabbed over.
  - **Collapsible icon rail** for dock columns, with flyouts.
  - **Theme tokens are a struct per theme** (`theme.rs` `Tokens`: chrome, canvas, canvas_dot, dock, card, card_border, field, field_border, hover, pressed, text, text_dim, text_faint, icon, accent, accent_soft, primary_bg, primary_text, danger, warning, radius_sm/radius/radius_lg, scrim). 14 theme kinds. The canvas has a dot-grid colour token.
- **photocraft moved to egui 0.37 on 2026-10-25** (`5c9192bbe`), on a patched egui fork (storytold/egui `6d518fc`). craft-ui requires egui 0.37.

### 2.2 Blender 4.3 to 5.0

- **Brushes are assets** (4.3): an asset shelf along the bottom of the viewport in paint modes, a brush popup on `Shift+Space`, catalogs instead of tool grouping, consistent thumbnails with a published style guide.
- **Per-property pressure curves** for size, strength and jitter, with presets (5.0).
- **Brush size is a diameter** (5.0). Unified brush settings are per mode, not per scene.
- **UI (5.0):** "Reduce Motion" accessibility option; regions show overflow with a line or gradient; area padding at window edges increased; sidebar snapping; "Focus Mode" (formerly Full Screen Area); menus stay open until a click closes them; disabled toggles grey out when their target is absent; a custom "not defined" glyph instead of tofu for missing characters.
- Sources: developer.blender.org/docs/release_notes/4.3/sculpt, /5.0/sculpt, /5.0/user_interface; docs.blender.org/manual/en/5.0/sculpt_paint/brush/introduction.html.

## 3. Audit findings

Severity: **P0** confuses or blocks a first-time user; **P1** visible polish gap; **P2** nice to have.

### 3.1 Layout

| # | Sev | Finding | Code cause | Class |
|---|---|---|---|---|
| L1 | P0 | At first launch the side panels are too narrow to read: Properties is 162 px, the UV view 97 px, while 5 leaves share the window. Text truncates everywhere ("Search proper…", "Histo…", "Inherit documen…"), rows clip at the panel edge. | `shell_menu.rs:31` `default_dock()` builds the layout from fractions alone, and egui_dock 0.21.1 has no minimum leaf size: a split's children get `fraction x parent` whatever the window width. Nothing in the app clamps a leaf to a readable floor. | Every preset in `workspaces.rs` (Paint, Bake, Export, Graph) and every user-dragged layout. |
| L2 | P0 | The 2D UV view is a 97 px sliver beside the 3D view at first launch, so neither view is usable as a 2D view. | Same fractions. The Painter-style 3D/2D split needs the 2D half to be a real fraction of the centre. | Paint preset. |
| L3 | P1 | Layers sit below Properties in a short bottom-right leaf, and the first layer row ("Base paint") is clipped by the window edge at 729 px tall. Painter puts Layers top-right with room. | `default_dock()` gives Properties 0.62 of the right column. | Paint preset. |
| L4 | P1 | The Properties search field is a fixed 160 px, so in a 162 px panel it truncates its own hint text and pushes the header out. | `umber-gui/src/widgets/context_rail.rs:41` `SEARCH_WIDTH = 160.0`. | One site. |

### 3.2 Missing glyphs (tofu)

| # | Sev | Finding | Code cause | Class |
|---|---|---|---|---|
| G1 | P0 | Five of the top toolbar's buttons are empty squares. | `toolbar.rs` draws the view toggles and quick mask with Panel-family icon ids (`eye`, `pencil`, `folder-open`, `save`, `undo-2`, `redo-2`), but `main.rs:2097` loads ONLY the Tool family atlas (its comment: "the Panel family's consumers arrive with the panel-icon wave"). They arrived; the loader did not. Every Panel id misses and draws the placeholder box. | 8 Panel ids; also `ContextKind::icon()` asks for `layers` and `layout-grid`, which are in NO atlas (`context_rail.rs:86`). |
| G2 | P1 | Wireframe, grid and view-mode toggles all use the same `eye` glyph, so even with the atlas loaded they are indistinguishable. | `toolbar.rs:562-590`. | 3 buttons. |
| G3 | P1 | `＋` (U+FF0B), `⟳` (U+27F3) and `⧉` (U+29C9) are used as button labels and in catalog strings but are NOT in Inter or JetBrains Mono. `⟳` resolves only through egui's emoji fallback; `＋` and `⧉` resolve nowhere and render as tofu. | `assets/i18n/*.ftl` (`layers.add-*`), `assets_panel.rs:175,238,281,329`, `document.rs:756,819,983,1498,1511`. | 10 code sites + 4 catalog keys per locale. No test guards glyph coverage. |

### 3.3 Content and copy

| # | Sev | Finding | Code cause |
|---|---|---|---|
| C1 | P1 | The Texture Sets panel prints two always-on warning paragraphs ("Changing the selected set's size clears that set's canvas and the stroke undo history, and resets its quick mask. This is intentional." and "Resizing a channel clears its tiles") even when nothing is being resized. | `main.rs` `texture_sets_panel_ui` (`texturesets.resize-clears`, `texturesets.channel-resize-clears`). The first already shows in the pending-resize confirmation, which is where it belongs. |
| C2 | P1 | The per-channel size combo's selected text is "Inherit document resolution", which cannot fit any side panel. | `texturesets.inherit` used as the combo's `selected_text`. |
| C3 | P1 | Quick Setup's Finish and Skip look alike; neither reads as the primary action. | `first_run.rs:220` plain `ui.button`. |
| C4 | P1 | Quick Setup opens at egui's default top-left position over the empty-state card, hiding its headline ("rop a mesh here…"). | `first_run.rs` `Window::new` without an anchor. |

### 3.4 Agent and automation access

| # | Sev | Finding | Code cause |
|---|---|---|---|
| A1 | P0 (for agents) | `command.run file.open_mesh` over the control channel opens a native file dialog that blocks the UI thread; every later control request times out after 30 s. There is no way to load a mesh by path, so an agent cannot get past the empty state. | `ShellAction::OpenMesh` runs `rfd_pick_mesh()` synchronously; `control.rs` has no path-taking load method; the app takes no positional mesh argument. |

### 3.5 Visual quality (beyond defects)

| # | Sev | Finding |
|---|---|---|
| V1 | P1 | The empty viewport is a flat colour with a bordered card. Painter and Blender show a ground grid or a lit backdrop, photocraft a dot grid (`canvas_dot` token). The empty state reads as unfinished. |
| V2 | P2 | The Layers panel stacks four full-width "＋ Add … layer" buttons above the stack. Painter and Blender put creation in one compact row (icon buttons or one "+" menu). |
| V3 | P2 | The theme has one palette (`umber-gui/src/theme.rs`). No light theme, no high-contrast theme, while Quick Setup offers "Light" (it only flips egui's base, which the Umber palette then overwrites on both branches). |

## 4. Requirements

Each requirement is testable. `R-*` ids are referenced by the plan and by the kill reports.

- **R-LAYOUT-1 (floors).** No docked leaf renders narrower than its panel's minimum width, nor shorter than its minimum height, while the window can hold the floors. Minimums: Viewport 360 x 240, UvView 240 x 200, every side panel 260 x 140. When the window cannot hold all floors, the viewport keeps its floor first, then side panels shrink proportionally (no leaf below 120 px).
- **R-LAYOUT-2 (default).** The Paint preset at 1400 x 900 gives: left column (Texture Sets, Assets, Bakes, Export) about 18% of width; 3D viewport the largest leaf; UV view a real 2D view (at least 240 px); right column with Layers on top and Properties below, about 22% of width. Every preset keeps all eleven panels docked (existing test).
- **R-LAYOUT-3 (search).** The Properties search field fills the header's free width (no fixed 160 px).
- **R-GLYPH-1 (atlas).** Every icon id the app paints resolves in the loaded atlas. A test enumerates the ids the code asks for (Tool family, Panel family, `ContextKind::icon`) and asserts each is baked.
- **R-GLYPH-2 (distinct).** Wireframe, grid and view-mode toggles use distinct glyphs.
- **R-GLYPH-3 (fonts).** Every character in every `assets/i18n/*.ftl` value is covered by the installed font stack. A test fails on any uncovered character. Hardcoded glyph labels use covered characters.
- **R-COPY-1.** Destructive-resize warnings appear only where the destructive action is confirmed, or as a hover on the control that triggers it.
- **R-COPY-2.** Combo selected text fits a 260 px panel; long forms move to hover text.
- **R-ONBOARD-1.** Quick Setup opens centred, does not hide the empty-state headline at the default window size, and its Finish button is visually primary (accent fill).
- **R-AGENT-1.** `umber <mesh-path>` opens that mesh at startup. The control channel offers `mesh.load {path}`. Commands that open a native modal dialog refuse over the control channel with a clear error instead of hanging the UI thread.
- **R-VISUAL-1.** The empty viewport draws a subtle backdrop (radial falloff plus dot grid from theme tokens) behind the home card.

## 5. Decision: egui 0.37 and craft-ui

craft-ui would bring most of R-LAYOUT for free, but it requires egui 0.37, and Umber's docking is egui_dock. **egui_dock has no egui 0.37 release** (crates.io: latest 0.21.1, 2026-08-06, built on egui 0.36). Upgrading egui therefore means replacing egui_dock with craft-ui's docking in the same move: every preset, `workspaces.rs` persistence (saved layouts in users' prefs), the tab-overflow code in `main.rs`, and the tab viewer, plus a dependency on a patched egui fork.

| Option | Cost | Risk | Reversible |
|---|---|---|---|
| A. Port the patterns into Umber on egui 0.36 + egui_dock (this pass) | Small, bounded kills | Low; each kill is testable alone | Yes |
| B. Upgrade to egui 0.37 + migrate docking to craft-ui now | Large; touches the shell, persistence, every panel | High; forked egui, saved-layout migration | Hard |

**Chosen: A now; B as a separate later wave (U5)** once egui_dock ships 0.37 support or the craft-ui migration is planned on its own merits. The layout floor (R-LAYOUT-1) is written as a small module with its own tests so a later docking swap can keep the same contract.

## 6. Plan

### Wave U1: first-launch correctness (P0 + cheap P1) — this session

| Kill | Requirement | Files | Test |
|---|---|---|---|
| U1-1 layout floors | R-LAYOUT-1 | new `crates/umber-app/src/dock_floor.rs`; `main.rs` (one call before `DockArea::show`) | unit tests on synthetic trees: floors honoured, viewport-first shrink, no change when everything fits, idempotent |
| U1-2 default preset | R-LAYOUT-2 | `shell_menu.rs` `default_dock`, `workspaces.rs` `side_layout` | leaf-width test at 1400 x 900 through the floor module; existing all-panels-docked tests |
| U1-3 icon atlas | R-GLYPH-1, R-GLYPH-2 | `umber-gui/src/icons.rs` (merged Tool + Panel atlas loader, new ids), `assets/icons/lucide/*.svg` (layers, layout-grid, grid-3x3, box, sun), `assets/icons/atlas/*` (re-baked by `tools/icon-raster`), `toolbar.rs`, `main.rs` loader call | every-requested-id-is-baked test |
| U1-4 glyph coverage | R-GLYPH-3 | `assets/i18n/*.ftl`, `assets_panel.rs`, `document.rs`, new font-coverage test in `umber-gui` | test iterates catalog chars against the installed fonts |
| U1-5 copy and search | R-COPY-1, R-COPY-2, R-LAYOUT-3 | `main.rs` `texture_sets_panel_ui`, `assets/i18n/*.ftl`, `context_rail.rs` | kittest: no always-on warning when nothing is pending |
| U1-6 Quick Setup | R-ONBOARD-1 | `first_run.rs` | kittest: Finish carries the accent fill; window is anchored centre |
| U1-7 agent load path | R-AGENT-1 | `main.rs` (argv), `control.rs` (`mesh.load`, refuse modal commands) | control unit test; live check via `scripts/control_call.py` |
| U1-8 2D view regression (found live) | R-LAYOUT-2 | `uv_view.rs` (zero-width canvas), `umber-gpu/src/texture_display.rs` + `renderer.rs` (depth state) | canvas-width test; GPU depth-pass test |

Verification per kill: rustfmt, clippy `-D warnings`, the touched crates' tests, `check-layers.py` GREEN. Wave close: a fresh release build, the same scripted screenshot at 1423 x 729, and a before/after table in the kill report.

### Wave U2: polish (P1/P2)

- V1 empty-viewport backdrop (R-VISUAL-1).
- V2 compact layer-creation row: one "+" menu button plus icon buttons, Painter order.
- Tab strip floor and overflow menu per craft-ui's contract (tabs shrink to a floor, then a "…" menu).
- Collapsible side columns to an icon rail (photocraft rail pattern).
- Status-bar polish: hints group visually, fps hidden by default (debug toggle).

### Wave U3: brush assets (Blender 4.3 pattern)

- Brush asset shelf along the viewport bottom in paint context, `Shift+Space` popup, catalogs, generated thumbnails.
- Per-property pressure curves (size, flow, hardness) with presets.

### Wave U4: themes and accessibility

- Theme tokens as a struct per theme (photocraft `Tokens` pattern): Umber Dark (current), Umber Light, High Contrast; Quick Setup's Light choice becomes real.
- Reduce Motion already exists in prefs; audit every animation site honours it.

### Wave U5: docking migration (deferred, see §5)

## 7. Status

- 2026-11-05: audit, requirements and plan written.
- 2026-11-05: **Wave U1 LANDED** (U1-1..U1-7 as planned, plus U1-8: the 2D UV view had drawn nothing since 2026-10-19 and its first real draw aborted on a depth-format mismatch; both fixed). U2-1 (empty-viewport backdrop) landed with it. Report: `docs/loop/kill-reports/2026-11-05-uiux-u1.md`, queue row 182.
- 2026-11-05: **U2-2, U2-3, U2-5 LANDED** (queue row 183). U2-4 parked to U5 (egui_dock's leaf collapse already ships; a real icon rail needs the docking migration).
- 2026-11-05: **U3 part 1 LANDED** (queue row 184): brush swatches + asset shelf under the 3D view; Assets swatch-cache texture-lifetime bug fixed.
- 2026-11-05: **U3-3 Shift+Space brush popup LANDED** (queue row 186).
- 2026-11-05: **U3-4 catalogs LANDED** (queue row 189). Found U3-5: the preset alpha/radius pressure curves are dead controls (never read by the paint path).
- 2026-11-05: **U3-5 LANDED** (queue row 191): the preset pressure curves drive flow and size. **Wave U3 complete.**
- 2026-11-05: **Wave U4 LANDED** (queue row 192): Dark / Light / High Contrast themes, the real theme preference (Quick Setup's Light was a no-op), Reduce Motion honoured everywhere.
- Next: U5 (egui 0.37 / docking migration) stays parked until egui_dock ships for egui 0.37. Follow-ups: theme the UV-view constants and the picker internals.
