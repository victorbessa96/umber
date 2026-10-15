# Kill report — g3-rich-tooltips (GUI plan G3)

Date: 2026-10-15. Author: dragon. Lock: `g3-rich-tooltips` over
`crates/umber-app/src/rich_tooltip.rs` (new), `toolbar.rs`, `main.rs` (one
`mod` line), `assets/i18n/*.ftl` and this report. It lives in umber-app,
not umber-gui: the sibling session's `g1-axis-gizmo` lock holds
`umber-gui/src/widgets/mod.rs`, and the tooltip needs the registry, which
is umber-app's anyway. Plan: `docs/specs/gui-plan-2026-10-15.md` row
`g3-rich-tooltips`. Sources: Blender E4 (the control's explanation plus
its hotkey), Blender A5.2 (a short hover shows the name), photocraft #12
(a disabled control says why).

## Found first

Every toolbar tooltip had its shortcut baked into the TRANSLATED
string, for example "Open a .umber project (Ctrl+O)", in both catalogs.
Nothing tied those strings to the binding table. The first keymap edit
(G3 keymap editor) would have made every one of them wrong, in every
locale, silently.

## Built

- **`rich_tooltip.rs`:**
  - `tooltip_keys(action)` maps each of the 21 shell actions to a title
    key and an optional description key (an exhaustive match, so a new
    action fails to compile until it has a tooltip).
  - `tooltip_parts(...)` assembles:
    - the translated title and description;
    - the LIVE shortcut via `actions::shortcut_label` (platform names:
      Ctrl+O, or ⌘O on a Mac);
    - the unavailable reason via `actions::availability`, the same key
      the toast and the greyed menu row use.
  - `draw`: title strong with the shortcut right-aligned and monospace,
    the description below, the reason in the warning colour.
  - `Response::action_tip(action, state)` (the `RichTooltip` trait)
    attaches it to hover AND disabled-hover.
  - 4 tests:
    - the shortcut comes from the registry, and none shows for an
      unbound action;
    - a greyed action states the registry's reason;
    - every tooltip key is in both catalogs;
    - a guard that no `tip.*` description hard-codes a shortcut.
- **The toolbar:** all 11 `on_hover_text` calls became `action_tip`. The
  view-cycle button leads with the current mode (the button's state) in
  the accent colour, then the action's tooltip.
- **i18n:**
  - 12 new `tip.*` keys (descriptions without keys), en-US and pt-BR;
  - the 8 dead `shell.tip-*` keys with baked shortcuts removed from both
    catalogs;
  - `shell.tip-new`, `shell.tip-open-mesh` and `shell.tip-view-cycle`
    stay: they never carried a key, and other surfaces still read them.

## Judgment calls

1. **Only shell actions get rich tooltips here.** Panel controls (a
   bake setting, a layer button) are not registry actions. Their
   tooltips stay plain text until they become actions (the palette kill's
   same boundary).
2. **No developer line (Blender E5).** The `ActionId` as a dev line needs
   the stable string ids the keymap editor introduces. It lands with
   that kill.

## Unverified

- The live look, and the two-stage delay (the theme's 0.35 s, off via
  Preferences): manual.
