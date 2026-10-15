# Kill report — g3-command-palette (GUI plan G3)

Date: 2026-10-15. Author: dragon. Lock: `g3-command-palette` over
`crates/umber-app/src/palette.rs` (new), `actions.rs`, `toolbar.rs`,
`main.rs`, `i18n.rs`, `assets/i18n/*.ftl` and this report. `main.rs` and
the catalogs were added once the texture-set kill released them. Plan:
`docs/specs/gui-plan-2026-10-15.md` row `g3-command-palette`. Sources:
photocraft #2 (subsequence scorer, +8 contiguous, +5 word start, +1000
exact, translated AND English label, recents on an empty query, focus
requested only when missing), Blender A8.1-A8.3 (F3 menu search shows
the menu path and shortcut; recent first).

## Built

- **`palette.rs`**, pure core + overlay:
  - `CATALOG`: all 19 shell actions with their label and menu-path i18n
    keys. Labels REUSE the menu / shell / tool keys, so the palette and the
    menus never name an action two ways.
  - `score(query, text)`: a case-insensitive subsequence (`None`
    otherwise). +1 per matched char, +8 when contiguous with the previous
    match, +5 at a word start, +1000 for an exact name, minus one per 4
    unmatched chars.
  - `search(query, recents, text)` takes the best of the translated
    label, the English label, and "path label", so "view grid" finds Grid
    under View. Ties keep catalog order; at most 12 hits. An empty query
    lists the recents newest-first, then the catalog.
  - `Recents`: dedup, cap 8, newest first.
  - `Palette::show`:
    - a scrim (click closes) plus a card at `Order::Tooltip`, 110 px below
      the top;
    - ↑ / ↓ / Enter / Esc consumed so nothing behind the overlay reacts;
    - `request_focus` only when focus is missing (IME);
    - each row shows label, dim menu path and the LIVE shortcut from the
      registry;
    - the chosen action returns to the caller.
  - 6 tests: the subsequence rule; exact > prefix > scattered; word starts
    beat mid-word; ranking + path matching; empty-query recents order with
    no duplicates; the recents cap / dedup / order; every catalog key in
    both catalogs + every action listed once.
- **Registry:** `ShellAction::CommandPalette` bound to Ctrl+Shift+P
  (sorted with the other Cmd+Shift chords) and F3. Both are in the poll
  list. The two toolbar invariant tests are updated to the new counts:
  9 bare + 5 chords, and a key may now be bound only under Cmd+Shift (P).
  The focus test (from the sibling g0-focus-dispatch kill) now names the
  palette among the bindings that pass a focused text field. Ctrl+Shift+P
  and F3 are global commands, never text input, so `Focus::allows` lets
  them through by its existing rule (non-bare chord or F-key), and the
  test's expectation list was the part that predated the palette.
- **`i18n::tr_en(key)`**: the en-US text whatever the locale, so a pt-BR
  user who knows the English command name still finds it.
- **`main.rs`:** dispatch opens the palette; the overlay draws after the
  toasts; a pick runs through the same `dispatch` as every key and menu
  row (so availability reasons and recents apply uniformly).
- i18n: `palette.*` (placeholder, three menu paths, the two brush-size
  labels), en-US and pt-BR.

## Judgment calls

1. **The catalog is the shell's actions, not every panel control.** The
   registry holds what has a `ShellAction`. Panel-local controls (a
   bake's Run, an export preset) join as they become actions. The plan's
   F5 "every setting bindable" is the G3 keymap kill's generalization.
2. **The test's catalog parser follows the `.ftl` format.** The first
   version split message blocks on a blank line, but the `shell` block
   contains one (between `new-confirm-cancel` and `open`). It now ends a
   block at the first unindented, non-blank line, the Fluent rule. This
   was checked offline against both catalogs (19/19 entries) before the
   battery ran.

## Unverified

- The overlay's look and keyboard flow on hardware: manual (needs a
  window).
