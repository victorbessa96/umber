# Kill report 2026-11-10 razul-uiux-f9-import-intros (dragon)

Authorship: dragon (Razul, subagent). Kill lock `razul-uiux-f9-import-intros`
over `crates/umber-app/src/{mesh_map_import,psd_import,spp_import,path_tool,fill_layer,display_panel,brush_panel,i18n}.rs`,
both `.ftl` catalogs, `docs/loop/queue.md`, this report, the brief
`docs/loop/briefs/2026-11-10-f9-import-intros.md`, `README.md` and
`docs/SCORECARD.md` (scorecard regen only if stale; it was current).
Implements finding F9 / requirement R3-9 of
`docs/loop/audit-2026-11-10-uiux-code.md` (section 2.5).

## Built

- `i18n.rs`: new `hint_line(ui, short, more)`. It draws the short hint as one
  weak label and, when `more` is given, puts the long text on hover with the
  Help cursor. Callers pass literal `tr()` keys, so the completeness scan sees
  both keys.
- All 12 audit sites now call `hint_line`: `mesh_map_import.rs` (import.hint),
  `psd_import.rs`, `spp_import.rs`, `path_tool.rs`, `fill_layer.rs`,
  `display_panel.rs` (stencil N hint), and the six Brush panel hints
  (clone, erase, quick mask, symmetry, line constraint, projection). The three
  import intros were full-colour `ui.label`; all hints are now weak text.
- Catalogs: the 8 keys over 80 characters got a new short line (55 to 69
  characters en-US) and a `<key>-more` sibling carrying the old full text
  word for word. The same 8 + 8 changes are in pt-BR.
- Test `i18n::tests::inline_hint_keys_fit_one_line`: every en-US key ending
  `.hint` or `-hint` is at most 80 characters with no newline, except a named
  allow-list of six tooltip-only keys (`bakes.margin-hint`,
  `layers.base-paint-hint`, four `overlays.projection-*-hint`). The test also
  checks that each allow-listed key is only passed to `on_hover_text(` (or
  sits in a key list), so the list cannot hide an inline paragraph.
- Kittest `mesh_map_import::tests::import_sections_show_one_line_hints_at_260pt`:
  it renders the Import window body (mesh-map, PSD and .spp sections, in the
  order `main.rs` stacks them) at 260 pt wide. Each short hint is present and
  at most two wrapped lines tall. Each long text is absent until hovered.
  Hovering the mesh-map hint shows its full text.

## Judgment calls

- Hover tooltip, not a collapsed section: one helper, no extra click, and the
  pattern R-COPY-1 already uses for Texture Sets. A collapsing header per
  hint would add 12 more toggle states. Follow-up: none, unless keyboard-only
  users need the long text. Tooltips are not keyboard reachable today; that
  is a class issue for F11 / a11y.
- Kept the existing key for the short line and added `-more` for the long
  one, instead of renaming to `-short`. Call sites and other locales keep
  working, and the cap test reads the natural key name.
- The 80-character cap is a source-length check. At 260 pt an 80-character
  line wraps to two lines, so the kittest's height bound is 2.5 body lines.
  The old paragraphs wrapped to 5 to 7 lines at that width.
- The four sub-80 hints (clone, symmetry, fill poly, stencil N) keep their
  text and have no `-more`. They moved to `hint_line` so every inline hint
  has one shape (weak text).

## Unverified

- No live-render screenshot of the Import window; the kittest covers layout
  and hover in a headless harness only.
- pt-BR short lines are not length-capped by a test (R3-9 caps en-US only).
  All eight are 61 to 72 characters.

## Deferred

- F10 (untranslated literals in `brush_panel.rs`, `spp_import.rs` and others)
  is a separate kill.
- Keyboard access to hover-only help text.

## Process note

- A sibling F10 battery (`f10-battery.sh`, cargo test -p umber-app -p
  umber-gui -p umber-core) held `/tmp/umber-cargo.lock` at battery time. This
  kill queued behind it on the same flock and never ran cargo in parallel.
- The sibling's F10 edits were uncommitted in the shared tree and overlap
  `brush_panel.rs`, `spp_import.rs`, `i18n.rs` and both catalogs, with no
  kill lock registered. This kill ran in an isolated worktree and did not
  touch the shared tree. Whichever kill lands second rebases onto the other.
