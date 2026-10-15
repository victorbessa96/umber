# Kill report: prop-menu-sweep (every app value field gets the menu)

Date: 2026-10-15. Author: dragon. Lock: `prop-menu-sweep` over the eight
panel files with `ValueField`s, `crates/umber-app/src/i18n.rs`,
`crates/umber-gui/src/widgets/value_field.rs`, both ftl catalogs and this
report. It follows g4-property-context-menu (ce21db8), whose report named
this sweep as its deferred half.

## Built

- `MenuLabels` holds `Cow<str>`s instead of `&str`s, so a caller hands
  over fresh `tr()` Strings with no lifetime to keep alive. Tests use
  `"Reset".into()`.
- `i18n::prop_menu_labels()`: one app helper over three new catalog
  keys (`propmenu.reset`, `.copy`, `.paste`) in both locales.
- **All 48 `ValueField::new(...)` sites** in umber-app get
  `.context_menu(prop_menu_labels())`, through one regex pass that
  asserted every call matched before writing. Counts: bakes 3, brush
  21, display 2, fill 6, layers 1, main (Preferences) 1, path tool 4,
  postfx 10.
- **Reset to Default works where a panel has a named default:** Bakes
  rays / dilation / skew threshold (`DEFAULT_*` constants), and Display
  exposure / gamma (`DisplaySettings::default()`). The other fields offer
  Copy and Paste.

## Verification

- Gate 1 on the build host: umber-gui 72 + umber-app 462 tests, clippy
  and fmt clean. The remote copy asserted that the sibling's three dirty
  files (camera.rs, widgets/mod.rs, gui lib.rs) equal HEAD, so the
  verdict is this kill's alone.
- The two committed panel snapshots still pass: the menu draws only on
  right-click.

## Deferred

- Defaults for the remaining 43 fields. Brush, fill and postfx defaults
  live in their own `Default` impls. Wiring each through
  `default_value` is per-panel work, and a wrong default is worse than
  none.
