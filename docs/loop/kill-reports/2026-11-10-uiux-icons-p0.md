# Kill report 2026-11-10 razul-uiux-icons-p0 (dragon, battery+land of the audit icon kill)

Authorship: dragon (Razul). Kill lock `razul-uiux-icons-p0` held over
`crates/umber-app/src/toolbar.rs`, `crates/umber-app/src/empty_state.rs`,
`crates/umber-app/src/main.rs`, `crates/umber-gui/src/icons.rs`,
`assets/i18n/en-US.ftl`, `assets/i18n/pt-BR.ftl`, the baked atlas
(`assets/icons/atlas/{manifest.json,panel.png,panel@2x.png}`) and the 7 new
vendored SVGs under `assets/icons/lucide/`. Implements findings F1, F2, F4 of
`docs/loop/audit-2026-11-10-uiux-code.md` (queue row 356). F8 (glyph_button
helper sweep) is NOT in this kill; it stays queued.

## Built

- **F1**: Open Mesh toolbar button now paints `boxes` instead of `folder-open`
  (`toolbar.rs`), so it no longer duplicates Open Project's icon. New
  `top_strip_glyphs_are_unique_per_action` test asserts the top strip's glyph
  list has no duplicates.
- **F2**: the home card gained an "Open Mesh…" button next to Open…
  (`empty_state.rs` `HomeCardAction::OpenMesh`, dispatched to
  `ShellAction::OpenMesh` in `main.rs`), with `home.open-mesh` /
  `home.open-mesh-tip` keys in BOTH `.ftl` catalogs. The drop-zone hero text
  and the button now agree.
- **F4**: mask pair split — Toggle Mask Mode paints `pencil-line`, Invert
  Mask paints `contrast`. Rail icons de-collided from toolbar actions:
  Properties `sliders-horizontal`, Assets `library`, History `history`,
  Export `upload` (`main.rs` `rail_icon`).
- **Atlas**: 7 new lucide SVGs vendored (`boxes`, `pencil-line`, `contrast`,
  `sliders-horizontal`, `library`, `history`, `upload`; source lucide-static,
  provenance: fetched 2026-11-10), `PANEL_ICONS` 13 -> 20, atlas re-baked via
  `tools/icon-raster` (panel family now 80x64 / 160x128 @2x). The census pins
  in `icons.rs` (`baked_manifest_dims_match_packer_math`) were updated
  deliberately per their own comment.

## Judgment calls

- New glyph SVGs come from lucide-static `latest` (v1.71.0) because the pinned
  release fetch for `history` failed; the 6 others came from lucide-icons main.
  Visually consistent with the vendored set; if the project re-pins lucide
  later, re-bake from the pinned tag.
- `tools/icon-raster/Cargo.lock` drifted when the raster tool ran (dependency
  resolution); reverted, not part of this kill.
- Home card reuses a third button in the existing row (150px + 8px gap each);
  no new ftl section needed beyond the two `home.*` keys.

## Unverified

- Live-render check of the new glyphs in the actual window (headless control
  smoke covers command surface, not pixels). The atlas decode tests
  (`baked_pngs_decode_to_manifest_dims_with_real_pixels`,
  `baked_manifest_covers_every_declared_id`) prove the PNGs carry real
  non-empty cells; visual confirmation is a follow-up smoke.

## Deferred

- F3 (dock strip showing ~1.5/4 tabs), F5-F10, F8 glyph_button helper sweep:
  separate kills, queued.
- `pencil` glyph is still in PANEL_ICONS (Properties context rail uses
  `ContextKind::icon`, not `Panel::rail_icon`); left in the family so the
  merged loader keeps resolving it.

## Verification

- `cargo run --manifest-path tools/icon-raster/Cargo.toml` exit 0 (atlas
  re-baked, `missing` empty).
- rustfmt --check clean on all 4 touched .rs files; check-layers GREEN;
  scorecard --check current; clippy `-p umber-app -p umber-gui --all-targets
  -D warnings` clean; `cargo test -p umber-gui -p umber-app` all ok
  (312 + 777 + doc/bin tests, 0 failed), including the new uniqueness test
  and the i18n key-completeness test over both catalogs.
