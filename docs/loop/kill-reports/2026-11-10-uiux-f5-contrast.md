# Kill report 2026-11-10 razul-uiux-f5-contrast (dragon)

Authorship: dragon (Razul). Kill lock `razul-uiux-f5-contrast` (adopted from
a stale lock left by an earlier attempt) over `crates/umber-gui/src/theme.rs`,
`DECISIONS.md`, `docs/loop/queue.md`. Implements finding F5 of
`docs/loop/audit-2026-11-10-uiux-code.md`.

## Built

- Dark `text_faint` `#6A6E78` -> `#8E9298`: 5.15:1 on panel `#1E2128`, 4.59:1
  on raised `#262A33` (was 3.16 / 2.82).
- Light `text_faint` `#76787F` -> `#606167`: 5.42:1 on panel, 4.82:1 on
  raised (was 3.87).
- Dark 2D-canvas label `(110,118,130)` -> `#8C949E`: 4.91:1 on the square
  (was 3.29) — this is the uv-view no-mesh hint and tile numbers.
- Tests moved to the text floor: faint >= 4.5:1 on panel AND raised for
  every theme; canvas label >= 4.5:1 in every theme (Dark grid stays a
  >= 1.0:1 0.5px guide, split out of the 1.5 loop); egui's
  `weak_text_color` is now MEASURED per theme at >= 4.5 (the audit's
  unverified ~4.2 Light estimate: measured, passes).
- `palette_hex_values_pinned` moved with the token; DECISIONS.md entry
  records the palette change (the roadmap names DECISIONS.md as the
  amendment path).

## Judgment calls

- No separate non-text hairline token: adding one breaks the
  roadmap-§2.1-verbatim token-count pin. The two hairline sites
  (`icon_rail.rs:83`, `empty_state.rs:366`) ride the brighter faint;
  accepted tradeoff, recorded in DECISIONS.md.
- Chose values with margin above 4.5 on BOTH panel and raised so the
  token is safe on elevated surfaces too.

## Unverified

- Pixel-level snapshot diffs of panels using faint (no snapshot baselines
  cover faint text at these sizes; the contrast math is the contract).

## Deferred

- F9/F10/F11 remain queued.

## Verification

rustfmt --check clean; umber-gui suite 148 passed / 0 failed (incl. the
three moved contrast tests); full battery (layers, scorecard, clippy
-D warnings, umber-app tests) green before push.
