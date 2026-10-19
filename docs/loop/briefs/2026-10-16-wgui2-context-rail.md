# Kill brief: wgui2-context-rail — the g2-properties-context widget (umber-gui only)

You are building a UI widget for Umber, in the Rust workspace at
~/Projects/umber (HEAD 9acdc1c). NO main.rs, NO document.rs, NO
layers_panel.rs — another claw holds those. Your surface:
`crates/umber-gui/` ONLY (new file `src/widgets/context_rail.rs`,
plus `src/widgets/mod.rs` + `src/lib.rs` re-exports). No cargo (the
dragon gates on the build host). No commits. Kill report at
`docs/loop/kill-reports/2026-10-16-g2-context-rail.md`.

## The plan row (docs/specs/gui-plan-2026-10-15.md:100)

g2-properties-context: "Properties switches by context (Brush / Layer /
Fill / Mask / Texture set) on a vertical icon rail; tabs that don't
apply are hidden. Breadcrumb header ('Set ▸ Layer 3 ▸ Mask'). Ctrl+F
property search that greys non-matches and jumps to the first hit."

## What to build (read tab_overflow.rs + tool_row.rs first — the house style)

1. `ContextKind { Brush, Layer, Fill, Mask, TextureSet }` — one
   variant per context; carries its icon id (use the icons atlas's
   existing ids — check `crates/umber-gui/src/icons/` for what
   exists; if an id is missing, name it in the report, do not invent
   a new atlas entry).
2. `ContextRail` state: `active: ContextKind`, the set of APPLICABLE
   kinds (the caller decides which tabs exist per target — hidden
   tabs are absent from the rail, per the row), selection state, the
   search field (Ctrl+F focuses it; Escape clears), and the first-hit
   jump.
3. `Breadcrumb` — "Set ▸ Layer 3 ▸ Mask" from a `Vec<String>`; greys
   out when the search is active.
4. Search semantics: a case-insensitive substring over the property
   LABELS the caller registers; non-matching rows grey out (alpha),
   the first match scrolls into view (egui 0.36: scroll_to_id or the
   row response's scroll_to_me — check which the pinned egui carries
   and name it), and Enter cycles matches.
5. Draw API: `ContextRail::ui(ui, ctx)` returning
   `ContextRailOutcome { switched: Option<ContextKind>, search_focus:
   bool, ... }` — a returned-action shape like TabOverflow
   (tab_overflow.rs) — so the shell dispatches after the closure.
6. Tests (pure, in-file, following tab_overflow.rs's test shape):
   - applicable-kinds filter (hidden tabs absent),
   - search matches + grey-out + first-hit ordering,
   - Enter cycles matches,
   - the breadcrumb string building,
   - search focus consumed exactly once per Ctrl+F.
7. i18n: the WIDGET holds no strings of its own — labels come from
   the caller; the search placeholder comes as a parameter. No .ftl
   edits in this kill.

## Rules

- egui 0.36.2 is the pinned version — resolve every API against
  `~/.cargo/registry/src/*/egui-0.36.2/` before writing (the
  kittest-traps list: no Modal, `Event::ModifiersChanged` for held
  mods, `ctx.data`, `new_child`+`UiBuilder`).
- egui_dock is NOT in this crate — do not import it.
- Rust 2021, clippy -D warnings clean, rustfmt clean
  (`~/.cargo/bin/rustfmt`).
- Kill report shape: Built / Judgment calls / Unverified / Deferred /
  Process note. Reply with: files changed, test names, the egui APIs
  used (so the dragon can spot version traps), anything still owed.
