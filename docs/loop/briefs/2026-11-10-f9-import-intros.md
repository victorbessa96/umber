# F9 inline hints: one short line, full text on hover

Audit: `docs/loop/audit-2026-11-10-uiux-code.md` section 2.5 (F9), requirement R3-9.
Kill lock: `razul-uiux-f9-import-intros`.

## What exists

Twelve panel sites print help text inline, always on:

| Site | Key | en-US length |
|---|---|---|
| `mesh_map_import.rs:723` | `import.hint` | 241 |
| `psd_import.rs:1310` | `psd-import.hint` | 250 |
| `spp_import.rs:209` | `spp-import.hint` | 206 |
| `path_tool.rs:426` | `path.hint` | 151 |
| `fill_layer.rs:706` | `fill.poly-hint` | 77 |
| `display_panel.rs:404` | `overlays.stencil-n-hint` | 65 |
| `brush_panel.rs:421` | `brush.clone-hint` | 46 |
| `brush_panel.rs:431` | `brush.erase-hint` | 139 |
| `brush_panel.rs:1649` | `brush.quick-mask-hint` | 117 |
| `brush_panel.rs:1698` | `brush.symmetry-hint` | 50 |
| `brush_panel.rs:1762` | `brush.line-constraint-hint` | 92 |
| `brush_panel.rs:1784` | `brush.projection-hint` | 94 |

The three import sections stack in the Assets tab's Import window as a
heading plus a multi-sentence full-colour paragraph each. R-COPY-1 (UI/UX
pass U1-5) moved long copy to hover text for Texture Sets only.

## Target

- One shared helper, `i18n::hint_line(ui, short, more)`: a weak label with
  the short line, and the long text (when there is one) as its hover text
  with a Help cursor.
- Every site calls it. The eight over-80 keys keep their key for the short
  line (<= 80 chars en-US) and gain a `<key>-more` sibling carrying the old
  full text, so no information is lost.
- Both catalogs (en-US, pt-BR) change in the same kill.

## Verification

- `i18n.rs`: `inline_hint_keys_fit_one_line` asserts every en-US key named
  `hint` or ending `-hint` is <= 80 chars, except a named allow-list of
  hover-only keys (`bakes.margin-hint`, `layers.base-paint-hint`, four
  `overlays.projection-*-hint`), and pins that each allow-listed key is only
  ever called inside `on_hover_text(`.
- `mesh_map_import.rs`: kittest renders the Import window body (mesh-map,
  PSD, .spp sections) at 260 pt wide; the short lines are present, the long
  text is absent until the short line is hovered, and then shows.
- Existing catalog completeness tests cover the new `-more` keys in pt-BR.

## Non-goals

- The hover-only `-hint` keys that are already tooltips stay as they are.
- F10 untranslated literals in the same files (separate kill).
- No change to what the import sections do.
