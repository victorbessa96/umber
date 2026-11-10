# Umber UI/UX audit — 2026-11-10 (round 3, fable 5.2)

Author: claude-fable-5-2 claw (headless, proc_2350501), dragon-landed. Code-only pass on master @ 38419c55; live screenshots were permission-blocked, so pixel claims are marked (computed). Builds on the 11-05 audit (rounds 1-2, waves U1-U7).


---

# Umber UI/UX audit — 2026-11-10 (third round)

Read-only pass on `master` @ `38419c55`. Builds on the 11-05 audit doc (rounds 1–2, waves U1–U7); nothing below repeats a finding that doc records as fixed.

Anything about rendered pixels is derived from arithmetic on the source and marked **(computed)**; confirm those live before dispatching kills. Contrast ratios are WCAG 2.x, computed by hand with the formula at `theme.rs:287-304`.

## 1. TL;DR

| # | Sev | Finding | Where | Size |
|---|---|---|---|---|
| F1 | P0 | Open Mesh and Open Project are two adjacent, identical `folder-open` icon buttons, and the empty state tells the user to "use Open Mesh… in the toolbar" | `toolbar.rs:586`, `:592`; `en-US.ftl:139` | S |
| F2 | P0 | The home card says "Drop a mesh here" but has no Open Mesh button | `empty_state.rs:415-443` | S |
| F3 | P0 (computed) | Default tab strips cannot show their tabs; the 266 pt project column shows about 1.5 of 4, so Bakes and Export start hidden | `main.rs:268-291`, `:5477-5483`; `shell_menu.rs:46-66` | S–M |
| F4 | P1 | Quick-mask toggle and Invert mask are two identical `pencil` icons; the same glyph is the Properties rail icon | `toolbar.rs:676-690`; `main.rs:376` | S |
| F5 | P1 | Faint text carries readable content but its floor is 3:1 (Dark 3.16:1, Light 3.87:1; Dark 2D-canvas hint 3.29:1, and the test skips Dark) | `theme.rs:27`, `:161`, `:260`, `:949`, `:971` | S |
| F6 | P1 | Toolbar, contextual strip and status bar reset the style to text scale 1.0, so the text-scale preference never reaches the shell chrome | `toolbar.rs:563`, `:751`; `status_bar.rs:219` | S |
| F7 | P1 | The loaded-mesh viewport clears to a hard-coded `(28,28,30)`, not the theme's viewport token | `viewport.rs:300`, `:1196` | S |
| F8 | P1 | Eight glyph-only buttons (`+`, `✕`, `▸`) have no tooltip and no accessible name | `assets_panel.rs:697,709,789,858,916`; `document.rs:852,1005,1592` | S |
| F9 | P1 | Three import sections open with long always-on paragraphs under a heading | `mesh_map_import.rs:723`, `psd_import.rs:1310`, `spp_import.rs:209` | S |
| F10 | P1 | Blend-mode combos show Rust `Debug` names; 8 UI strings bypass `tr()` | `layers_panel.rs:398`, `document.rs:1738` | S |

## 2. Full findings

Severity: **P0** blocks or confuses a first-time user; **P1** visible polish gap; **P2** nice to have.

### 2.1 Icon-only buttons

**F1 — P0 — two identical Open buttons.**
- **Where:** `toolbar.rs:586` (OpenMesh) and `:592` (OpenProject), both `icon_button(ui, atlas, "folder-open", …)`, side by side.
- **Root cause:** U1-3 gave the view toggles distinct glyphs (`toolbar.rs:639-642`) but not the file pair.
- **Why P0:** the empty state's only non-drag route is `shell.hero-hint` (`en-US.ftl:139`). A new user finds two identical folders and must hover each for 0.35 s (`theme.rs:321`).
- **Also:** the accessible name is the atlas id (`umber-gui/src/icons.rs:789`), so a screen reader hears "folder-open" twice.
- **Class:** 2 pairs (with F4).
- **Fix:** give Open Mesh its own glyph, or make it a text button as New already is (`:578`). Add a test that no two toolbar buttons with different `ShellAction`s share an icon id.

**F4 — P1 — two identical mask buttons, and rail icons that collide with toolbar actions.**
- **Where:** `toolbar.rs:676-683` (ToggleMaskMode) and `:688-690` (InvertMask), both `"pencil"`.
- **Rail collisions** (`main.rs:373-386`): Properties → `pencil`, History → `undo-2` (the Undo glyph), Export → `save` (the Save glyph). The comment at `:371-372` says the reuse was deliberate to avoid new atlas cells.
- **Also:** the rail's expand button is `"panel-left"` on both sides (`main.rs:5373`).
- **Class:** 1 toolbar pair, 3 rail glyphs shared with toolbar actions, 1 mirrored glyph.
- **Fix:** bake distinct glyphs; extend the F1 uniqueness test over `rail_icon`.

**F8 — P1 — glyph-only buttons without tooltip or name.**
- **Where:**
  - `assets_panel.rs:697` `+` (add directory), `:709` `✕` (remove directory), `:789` `▸` (use brush), `:858` `+` (add material), `:916` `+` (attach mask)
  - `document.rs:852` `+` (attach smart mask), `:1005` `✕` (delete layer, destructive), `:1592` `+` (add smart material)
- **Root cause:** drift, not design. Sibling sites do carry tooltips (`assets_panel.rs:645`, `:813`; `postfx_panel.rs:381-396`; `empty_state.rs:500`).
- **Class:** 8 bare of 17 glyph-button sites.
- **Fix:** a `umber_gui` helper `glyph_button(ui, glyph, tooltip)` with a required tooltip that also sets `WidgetInfo::labeled`; migrate all 17.

**F11 — P2 — icon accessible names are atlas ids, not localized** (`icons.rs:785-789`). Fix: `icon_button` takes an optional label, as `tool_row.rs:114` already does for tools.

### 2.2 Empty state and first run

**F2 — P0 — the home card has no Open Mesh action.**
- **Where:** `empty_state.rs:415-443`. Buttons are `home.new-project`, `home.open-project` (opens a saved `.umber` project), and `home.browse-assets`. `OpenMesh` does not appear in the file. Double-click on empty space also opens a project (`:302-307`).
- **Possible aggravation:** the local photocraft README (line 291) notes winit 0.30 has no Wayland drag-and-drop. If Umber has the same limit, the card's headline action does not work on Wayland. Unverified for Umber.
- **Fix:** add a primary "Open Mesh…" button first on the card (`primary_button`, already used at `first_run.rs:241`) plus `HomeCardAction::OpenMesh`.

**F12 — P1 — panel empty states are inconsistent and carry no action.**
- **Where:** 20 sites in four renderings:
  - plain `ui.label`: `bakes_panel.rs:1426,1428`, `export_dialog.rs:642,644`, `main.rs:3510`, `postfx_panel.rs:346`, `assets_panel.rs:803,870,930,998,1057`, `mesh_map_import.rs:747`, `psd_import.rs:1313`, `spp_import.rs:212`, `path_tool.rs:540`
  - `ui.weak`: `history_panel.rs:1025`, `graph_panel.rs:1082`, `toolbar.rs:802,820`
  - custom RichText: `document.rs:1846`
  - painter text: `uv_view.rs:568-574`
- **Example:** "No mesh loaded — open a mesh to enable baking." (`en-US.ftl:213`) names an action but offers no button.
- **Fix:** one `umber_gui::widgets::empty_state(ui, text, Option<action>)`; no-mesh states pass "Open Mesh…".

**F13 — P2 — Quick Setup's language picker lists raw locale codes** (`first_run.rs:153-158`). Fix: a `locale-name` message per `.ftl`.

### 2.3 Dock layout, split fractions, tab strips

**F3 — P0 (computed, confirm live) — default tab strips overflow.**
- **Where:** the shell's own restatement of egui_dock's layout: `dock_tab_widths` (`main.rs:268-283`) gives each tab `text + 16 + 24` for its close button; `dock_bar_width` (`:289-291`) subtracts `3 × 24` for the end buttons. `DockArea` is built at `:5477-5483` with nothing hiding the close, close-all or collapse buttons.
- **Arithmetic at 1400 × 900:**
  - Project column: 0.19 × 1400 = 266 pt (`shell_menu.rs:46-48`), leaving about 190 pt of strip. "Texture Sets" is about 80 pt of text (`tab_overflow.rs:26-28`) + 40 = 120 pt; the other three are about 80 pt each by my estimate. Strip needs about 360 pt.
  - Right column: 0.27 × 1134 = 306 pt, about 226 pt of strip for about 330 pt of tabs. The fourth tab is hidden and the third cut.
- **Presets are worse:** Bake and Export put 6–7 tabs in one leaf (`workspaces.rs:69-91`, `:125`); Graph puts 8 (`:97-110`).
- **Root cause:** U2-3 made overflow reachable (scroll-into-view, the group menu) rather than removing its cause. 40 pt of every tab and 72 pt of every strip are chrome.
- **Fix:**
  1. Hide per-tab close buttons and drop close-all and collapse, keeping the group menu. The egui_dock builder names are unverified; read `egui_dock-0.21.1` first (rule 4). Update `dock_tab_widths` / `dock_bar_width` and their pinned tests in the same kill.
  2. Rebalance presets so no leaf holds more than 4 tabs.

**F14 — P1 — the 3D and 2D views can be closed from their own tab.**
- **Where:** `Panel::is_protected` (`main.rs:367-369`) is read only by the rail; nothing in `impl TabViewer` (`:551-601`) makes a tab non-closable.
- **Fix:** non-closable for protected tabs (hook name unverified) and skip them in close-all.

**F15 — P1 — R-LAYOUT-1's "no leaf below 120 px" clause is not implemented.**
- **Where:** `dock_floor.rs:91-103` shares space in proportion to the floors with no absolute minimum.
- **Effect:** user layouts, which `compact_for_width` never touches (`workspaces.rs:449-451`), scale every leaf below its floor in a narrow window.
- **Fix:** implement the clause, then auto-fold the right column (R-RAIL-5, recorded as not done).

**F16 — P2 — fold state does not persist; flyout does not close on outside click.** Both are recorded deferrals (11-05 doc §9.3); listed so they are not re-found.

### 2.4 Contrast and theme tokens

**F5 — P1 — the faint token is below the text floor but carries text.**
- **Values:**
  - Dark `#6A6E78` on panel `#1E2128` = 3.16:1; on raised `#262A33` = 2.81:1.
  - Light `#76787F` on `#F2F0EC` = 3.87:1.
- **Floor:** `theme.rs:949` asserts faint ≥ 3.0, the non-text floor.
- **Where it carries text:**
  - missing recent rows (`empty_state.rs:287-292`);
  - the Dark canvas label `(110,118,130)` on `(38,38,42)` = 3.29:1, used for the 2D view's no-mesh hint (`uv_view.rs:568-574`) and tile numbers. The canvas test checks 4.5 only for Light and High Contrast (`theme.rs:971`).
- **Class:** 3 theme values; 13 `faint()` call sites.
- **Fix:** raise Dark and Light `text_faint` and the Dark canvas label to ≥ 4.5:1, and move both tests to 4.5 for every theme. Keep a separate non-text token for hairlines (`icon_rail.rs:83`, `empty_state.rs:366`). `palette_hex_values_pinned` (`theme.rs:815-837`) must move with it, so this needs a DECISIONS.md line.
- **Unverified:** egui's default `weak` colour is not overridden for Dark or Light (`theme.rs:764` sets it only for High Contrast). My estimate for Light is about 4.2:1; measure it in a test.

**F7 — P1 — the loaded viewport ignores the viewport token.**
- **Where:** `viewport.rs:300` `EMPTY_VIEWPORT_COLOR = (28,28,30)` is used at `:1196` when a mesh is loaded. The empty state uses `theme::viewport_bg()` (`empty_state.rs:168`): `#111317` Dark, `#3A3C40` Light, black High Contrast.
- **Effect:** the backdrop changes colour at mesh load and is not black in High Contrast.
- **Fix:** use `theme::viewport_bg()` and delete the constant. Check whether any GPU golden samples the backdrop first.

**F17 — P1 — hard-coded `Color32` that breaks under Light / High Contrast.**
Most of the ~100 literal sites outside `theme.rs` are data (hue ramps, picker cursors, tests) or content overlays. The chrome ones:

| Site | Problem |
|---|---|
| `brush_panel.rs:142-169` (`curve_palette`, 9 consts, 9 uses) | Copies of Dark tokens; the curve editor stays Dark in every theme. The comment at `:138-141` says it duplicates because the theme module was in flight; it has landed. |
| `graph_canvas.rs:62` `OUTPUT_ACCENT (232,160,64)` (3 uses) | 1.93:1 on the Light panel (computed); also a second orange beside the accent. |
| `umber-gui/src/widgets/curve.rs:405` | White handle fill on Light's near-white surface. |
| `palette.rs:407` | `from_black_alpha(90)` scrim; no scrim token. |
| `uv_view.rs:800-801,823,830` vs `axis_gizmo.rs:96-98` | Two definitions of the axis colours with different values. |

Fix: route these through `theme::*`, add a shared `theme::overlay` for axis and overlay colours, and add a source-grep test with an allow-list.

**F6 — P1 — shell chrome ignores the text-scale and density preferences.**
- **Where:** `toolbar.rs:563`, `:751` and `status_bar.rs:219` call `ui.set_style(theme::umber_style(Density::…))`, which is scale 1.0 (`theme.rs:524-526`). The status bar also sizes every run at 10 px (`status_bar.rs:220-236`).
- **Literal font sizes:** `viewport.rs:1636,1755,1782`; `uv_view.rs:528,572,822,829,899`; `axis_gizmo.rs:235`; `graph_canvas.rs:860`; `color_picker.rs:532,539`.
- **Effect:** at text scale 1.2 the panels grow but the toolbar, strip and 10 px status hints do not.
- **Fix:** derive from `ui.style()` and override only spacing; status text uses `TextStyle::Name(STATUS_STYLE)`, already defined scaled at `theme.rs:587-590`.

### 2.5 Copy

**F9 — P1 — always-on paragraphs in panels.**
- **Where:** `mesh_map_import.rs:722-723` (`en-US.ftl:699`), `psd_import.rs:1309-1310` (`:734`), `spp_import.rs:208-209` (`:747`): heading plus a multi-sentence full-colour label, stacked in the Assets tab.
- **Also:** `path_tool.rs:426`, `fill_layer.rs:706`, `display_panel.rs:404`, and six `weak` hints in the Brush panel (`brush_panel.rs:421,431,1649,1698,1762,1784`).
- **Root cause:** R-COPY-1 was applied to Texture Sets only, not as a class (12 sites).
- **Fix:** one line of 80 characters or fewer, with the full text behind a hover or a collapsed section.

**F10 — P1 — raw identifiers and untranslated strings.**
- **Blend modes via `Debug`:** `layers_panel.rs:398`, `document.rs:1738` render "ColorDodge", "LinearBurn".
- **Hard-coded English (rule 7):**
  - `tile_selection.rs:115,118,125`
  - `brush_panel.rs:330` (a `no-preset` key already exists at `en-US.ftl:411`), `:635`, `:866`, `:878`
  - `spp_import.rs:250`
- **File-extension headings:** `document.rs:836`, `:851`.
- **Status bar pen readout** shows the token `x11-xinput2` (`status_bar.rs:197-200`).
- **Fix:** `BlendMode::label_key()`, move the literals to both catalogs, and add a source grep to the i18n completeness test.

### 2.6 Defect classes with nothing new

- **Vertical strip labels:** none found; the rail is icon-only. The only route to per-word wrapping is F15.
- **Index-only tab titles:** all 11 dock tabs are named via `panel_title` (`main.rs:209-223`). The only index-only labels are the UDIM "Tile {n}" rows (F10).
- **Black viewport before mesh load:** fixed in U2-1; the residual is F7.
- **Truncated prose:** 2 explicit sites, both with hover text. The live risk is F3 and F15.

## 3. Reference patterns Umber lacks

Nothing here was fetched today. "Local" means the README copies under `docs/references/`. "Second-hand" means the 11-05 doc §2, whose author read photocraft at `3317aede3` and craft-libs at `a8b996d`; re-check those paths before a kill cites them.

| # | Pattern | Source | Umber status |
|---|---|---|---|
| P1 | Control channel drives pointer events and renders offscreen screenshots | photocraft README, local copy `:249` | **Partial.** `control.rs:41-49` has no pointer method and the screenshot needs a live window. |
| P2 | Every menu item, tool and dialog is a command in one registry ("500+ commands") | photocraft README, local copy `:230`, `:213` | **Partial.** 43 commands per the 11-05 doc §9.3. |
| P3 | Protected canvas panel; dock width floors with a canvas reserve (`DOCK_MIN_WIDTH`, `Permissions::PROTECTED`) | second-hand: photocraft `crates/ui-egui/src/panels/dock.rs`, craft-libs | **Partial.** Floors exist (`dock_floor.rs`); views are fold-protected only (F14). |
| P4 | Tabs shrink to a floor, then a "…" menu; the active tab always shows | second-hand: craft-libs `crates/ui`, `docs/ui-docking.md` | **Partial.** Elision cap and scroll-into-view (`tab_overflow.rs:29`); tabs never shrink (F3). |
| P5 | Accessible icon buttons as a library primitive | second-hand: craft-ui (signature not read) | **Missing.** `icons.rs:762-789` takes no label (F8, F11). |
| P6 | Richer theme tokens (scrim, icon, separate faint levels); user-selectable accent | second-hand: photocraft `theme.rs` `Tokens`; texturecraft README local copy `:91` | **Partial.** 12 tokens, 3 themes, one accent (`theme.rs:105-130`). |
| P7 | Blender UI changes: region overflow shown by a gradient; a "not defined" glyph instead of tofu | Blender 5.0 release notes, as cited in the 11-05 doc §2.2 (not re-fetched) | **Partial.** Edge fades exist only on the brush shelf (A2-5). |

## 4. Requirements (ranked by impact ÷ effort)

| # | Requirement | Test | Evidence | Impact / effort |
|---|---|---|---|---|
| R3-1 | No two toolbar buttons with different actions, and no rail icon and toolbar button, share an icon id. | Unit test over `(action, icon)` pairs; fails today on `folder-open`×2, `pencil`×3, `undo-2`, `save`. | F1 `toolbar.rs:586,592`; F4 `toolbar.rs:679,689`, `main.rs:373-386` | High / S |
| R3-2 | With no mesh loaded, the home card's first button is an accent "Open Mesh…" that yields `HomeCardAction::OpenMesh`. | kittest by label; `resolve_action` unit test. | F2 `empty_state.rs:415-443` | High / S |
| R3-3 | For every workspace preset at 1400 × 900, every leaf's tabs fit its strip. | Pure test: `dock_floor` rects + `dock_tab_widths` + `dock_bar_width` → `!overflows` per leaf. | F3 `main.rs:268-291`, `workspaces.rs:69-126` | High / S–M |
| R3-4 | Viewport and UvView tabs have no close affordance and survive close-all. | Unit test on the closability hook; dock test on close-all. | F14 `main.rs:367-369` | Med / S |
| R3-5 | Every glyph-only button has a localized tooltip and accessible name. | kittest per panel; source grep for bare `small_button("<1–2 chars>")`. | F8 (8 sites) | Med / S |
| R3-6 | `text_faint` on `panel`, `canvas().label` on `square`, and `weak_text_color()` are ≥ 4.5:1 in all three themes; hairlines use a separate token ≥ 3:1. | `theme.rs:949` → 4.5; `:971` iterates `Theme::ALL`. Fails today at 3.16, 3.87, 3.29. | F5 | Med / S |
| R3-7 | At text scale 1.2, toolbar, strip and status-bar text is 1.2× its 1.0 size. | kittest on galley height; source grep for `set_style(theme::umber_style(`. | F6 | Med / S |
| R3-8 | The viewport backdrop is the viewport token with and without a mesh, in all themes. | Unit on the fill colour; `EMPTY_VIEWPORT_COLOR` gone. Goldens need a GPU run. | F7 `viewport.rs:300,1196` | Med / S |
| R3-9 | A panel's inline hint is ≤ 80 characters in en-US; longer help is behind a hover or collapsed section. | Catalog length test; kittest on the Assets tab at 260 pt. | F9 (12 sites) | Med / S |
| R3-10 | Blend modes render through `tr()`; the 8 literals are in both catalogs. | i18n completeness plus source grep; every `BlendMode` has a key. | F10 | Med / S |
| R3-11 | `curve_palette`, `OUTPUT_ACCENT`, the curve handle and the scrim read `theme::*`; axis colours have one definition. | Source grep with allow-list; output outline ≥ 3:1 on `panel` in every theme. | F17 | Med / M |
| R3-12 | Every empty state uses one shared widget; no-mesh states offer "Open Mesh…". | kittest per panel. | F12 (20 sites) | Med / M |
| R3-13 | No leaf below 120 pt on its split axis; the right column folds before any leaf drops below its floor. | `dock_floor` tests at 800 / 960 / 1100 pt. | F15 `dock_floor.rs:91-103` | Med / M |
| R3-14 | Control channel gains pointer input and an offscreen screenshot. | `control.rs` unit tests; e2e smoke opens and screenshots the rail flyout. | P1; `control.rs:41-49` | High / L |
| R3-15 | Icon buttons take a localized label; Quick Setup lists language names. | kittest by localized label. | F11, F13 | Low / S |

Suggested order: R3-1 to R3-4 as one first-launch set, then R3-5 to R3-10 as small kills, then R3-11 to R3-13; R3-14 needs its own design. Note that R3-3, R3-4 and the rail half of R3-1 all touch `main.rs`, so they cannot run as concurrent kills under the lock rule.

## 5. Unverified

1. No pixel was seen. F3 uses my estimate (about 40 pt) for the Assets, Bakes and Export title widths; only "Texture Sets ≈ 80 pt" is from source.
2. egui_dock 0.21.1 names for hiding close buttons and marking tabs non-closable (`~/.cargo/registry` was not readable).
3. egui 0.36's default weak-text colour (F5).
4. Whether file drag-and-drop works on this Wayland session (F2's severity depends on it).
5. Whether any `umber-gpu` golden samples the viewport backdrop (F7).
6. Every second-hand row in section 3, and the Blender source.
7. The skill section itself.

---

Top 5 findings:

1. F1 (P0): Open Mesh and Open Project are two identical folder icons side by side in the toolbar (`toolbar.rs:586`, `:592`), and the empty state sends new users to exactly that spot.
2. F2 (P0): the home card says "Drop a mesh here" but has no Open Mesh button; its Open button opens a project (`empty_state.rs:415-443`).
3. F3 (P0, computed, needs a live check): every tab carries a 24 pt close button and every strip reserves 72 pt for end buttons, so the default project column shows about 1.5 of its 4 tabs and Bakes/Export start hidden (`main.rs:268-291`).
4. F5 (P1): faint text sits at 3.16:1 in Dark and 3.87:1 in Light while carrying readable content, and the Dark canvas hint at 3.29:1 is exempt from its own contrast test (`theme.rs:949`, `:971`).
5. F6 and F7 (P1): the toolbar, contextual strip and status bar ignore the text-scale preference (`toolbar.rs:563`, `status_bar.rs:219`), and the loaded viewport clears to a hard-coded colour instead of the theme token (`viewport.rs:300`).