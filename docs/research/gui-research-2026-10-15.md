# GUI research index (2026-10-15)

The brief (`gui-plan-prompt-2026-10-15.md`) asked for one research report.
It landed as three slices, because a single subagent stalled for ~33
minutes on one model response and died after fetching the corpus. The
corpus survived, and three bounded subagents each wrote one slice from it:

| Slice | File | Items | Basis |
|---|---|---|---|
| Substance 3D Painter | `gui-research-painter-2026-10-15.md` | sections A-F, 45 cited pages | 52 corpus pages (7 are 404 stubs) |
| Blender | `gui-research-blender-2026-10-15.md` | A1-A10, B1-B12, C1-C7, D1-D23, E1-E10, F1-F19 | 86 corpus pages; the HIG index only |
| photocraft | `gui-research-photocraft-2026-10-15.md` | 20 patterns + egui techniques | ~35 `crates/ui-egui` source files read |

Corpus: `~/Projects/.umber-gui-corpus-2026-10-15/` (148 pages, the
fetch lists, photocraft's GitHub tree JSON). It sits outside the repo
because the pages are third-party documentation.

## Where the slices agree (the strongest signals)

- **Brush cursor in the viewport.** Painter C1, Blender C4.1, photocraft #1.
  All three treat it as basic. Umber has none (0 hits in tree).
- **One action table behind menus, shortcuts and search.** Blender A8 + D20
  (every menu item shows its key), photocraft #2-#4 + #12 (one binding
  table, a palette, a keymap editor, disabled reasons), Painter F1
  (editable shortcuts).
- **Status bar as the teacher.** Blender A4.1 (tool mouse/key hints) and
  Painter A9 (keyboard helper while a modifier is held).
- **Layer thumbnails.** Painter B1 and photocraft #6 (content-keyed,
  updated in place).
- **A real colour picker.** Painter D1, Blender D14-D17, photocraft #7. All
  three converge on the same shape: a 2D field + strip, previous/current
  swatches, sRGB hex, an eyedropper, persisted swatches.
- **The field grammar.** Painter D2 (shift precision, click-to-jump,
  arrows), Blender D1-D9, photocraft #8. The contract is drag-scrub with
  fine/coarse modifiers, click-to-type, Ctrl+wheel, reset.

## Where they disagree (decided in the plan)

- **Navigation.** Painter uses Alt+mouse (Maya); Blender uses MMB.
  Decision: Painter's is the default and Blender's is a preset (users come
  from Painter).
- **Modes vs a dock tab for baking.** Painter has a Baking mode; Umber has
  a Bakes tab. Decision: workspaces (G5) give a Bake layout without a
  modal mode.
- **Pie menus.** Blender only. Decision: deferred.

The plan is `docs/specs/gui-plan-2026-10-15.md`.
