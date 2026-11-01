# Kill report: UMBER_THEME_FILE live token reload (g0-style-both-branches tail)

Date: 2026-11-01 ~11:0x (dragon local). Lock: `g0-theme-live-reload`.
Envelope: code only, no cargo/tests (host envelope); rustfmt sanctioned.

## Scope

GUI plan G0 `g0-style-both-branches` row's open parenthetical: the boot
wiring was already done (`theme::apply` writes both `Theme::Dark` and
`Theme::Light`, main.rs installs once); the open half was the debug-only
live token reload from `UMBER_THEME_FILE` (polled every 400 ms). The env
var existed nowhere in code before this kill.

## What landed (`1e9155d`)

`crates/umber-gui/src/theme.rs`:

- `pub type TokenOverrides = HashMap<String, Color32>` + `parse_token_overrides`
  (`name = #rrggbb` lines, keyed by lowercase `palette::TOKENS` names; unknown
  names / bad hex / non-`=` lines skipped; `#` comments only at line start),
  `set_token_overrides` / `token_overrides` (persistent ctx data under
  `Id::new("umber-theme-overrides")`).
- `umber_visuals` now runs through `umber_visuals_impl(Option<&TokenOverrides>)`;
  `apply` uses the new `umber_style_scaled_with(ctx, …)`, so a reload recolors
  the visuals (fills, strokes, selection, hyperlink/warn/error) on BOTH OS
  branches. The compiled `umber_style_scaled` is unchanged for existing tests.
  Text-color helper fns (`accent()`, `panel_bg()`, …) stay compiled-token —
  documented scope of the debug reload.
- 2 pinned unit tests: `token_overrides_parse_skips_junk`,
  `token_overrides_recolor_the_visuals`.

`crates/umber-app/src/main.rs`:

- `AppState` gains `theme_file` / `theme_file_mtime` / `theme_last_poll`, all
  `#[cfg(debug_assertions)]` (fields and inits), env read at construction.
- `poll_theme_file` (debug-only), called at the top of `fn ui`: at most every
  400 ms, on mtime change reads the file, stores the parsed overrides, and
  re-applies the style. Unreadable mid-write files retry next poll.

## Verification (static, per envelope)

- `rustfmt --edition 2021 --check` CLEAN on both touched files.
- `Color32::from_hex` confirmed in vendored ecolor 0.36.2
  (hex_color_runtime.rs:143); `ctx.data` persistent get/insert confirmed
  against egui 0.36.2; `#[cfg]` on struct-literal fields is stable.
- NOT run: cargo/tests (envelope) — the two new tests run at the next
  battery; release builds compile the fields out entirely.

## Row status

`g0-style-both-branches` parenthetical fully closed (boot wiring pre-existing
+ this live reload).
