# Kill report (NOT STARTED, design only): C2-32 imported colour PNGs land linear

Date: 2026-10-15. Author: dragon. Audit row C2-32.

## Why not written yet

Both files the fix needs are under live locks:
- `mesh_map_import.rs` (where the pure helper belongs, beside
  `channel_kind_for`) is held by `sec-import-channel`;
- `main.rs` (the one call site, `publish_import_plane`) is held by
  `g5-workspace-persist`.

A first draft was written into `mesh_map_import.rs` before the lock
check, which is a process slip. It was removed the same minute, and the
file was proven back to `sec-import-channel`'s hunks only by a test-
section diff against HEAD. That diff also caught a neighbouring helper
(`tmp_store`) the removal had taken with it, and it was restored
verbatim from HEAD.

## The fix, ready to write when both locks free

1. `mesh_map_import.rs`: `pub fn png_to_plane_space(channel: &str,
   rgba: &mut [u8])`.
   - When `channel_kind_for(channel) == ChannelKind::Color`, map RGB
     through the sRGB→linear table (the `curvature_png_to_linear` curve),
     leaving alpha untouched. Data channels pass through.
   - Test: `[188,188,188,77, 0,255,128,200]` as baseColor →
     `[128,128,128,77, 0,255,55,200]`. Emissive red 188 → 128.
     Roughness, normal, height, AO and skew are unchanged.
2. `main.rs` `publish_import_plane`: after the decode and the size check,
   `mesh_map_import::png_to_plane_space(&applied.channel, &mut rgba);`
   before `straight_to_premultiplied`. (`rgba` is bound immutably today;
   make it `mut`.)
3. `spp_import::write_channels`: write colour channels with
   `Transfer::Srgb` (the CLI's `transfer_for` rule) in the SAME commit.
   It writes linear today only to survive this gap.

The expected values were checked in a Python port of the table: sRGB
188 → 128, 128 → 55.
