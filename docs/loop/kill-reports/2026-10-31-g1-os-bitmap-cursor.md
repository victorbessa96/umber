# Kill report: OS-bitmap brush cursor (g1-brush-cursor open tail)

Date: 2026-10-31 ~23:3x (dragon local, same code-only session).
Lock: `g1-os-bitmap-cursor`. Envelope: code only, no cargo/tests;
rustfmt write+check sanctioned. Battery/clippy/new tests at the next
sanctioned wave push.

## Scope

GUI plan G1 `g1-brush-cursor` row's open tail: "the OS-bitmap cursor path
(`set_cursor_image`) behind a plugin, with the painted fallback on Wayland
≠1×, on web and for oversize tips" (photocraft #1, research doc §1).

## What landed

- `brush_cursor.rs`:
  - `bitmap_supported(pixels_per_point)` — false on web
    (`cfg!(target_arch = "wasm32")`) and at any scale ≠ 1x (Wayland
    fractional / HiDPI hotspot misplacement). Conservative.
  - `bitmap(shape, ppp) -> Option<egui::CustomCursorImage>` — rasterizes
    the ring (or crosshair) into an RGBA buffer with the hotspot at the
    bitmap centre (the dab position); `None` when unsupported or the side
    would exceed `MAX_BITMAP_SIDE_PX = 128` (Linux HW-cursor planes cap
    near 255 — oversize tips fall back to the painted ring).
  - Pure rasterizers `rasterize_ring` / `rasterize_crosshair`: same
    two-pass look as `paint()` (3 px dark outline, 1 px light rim line,
    faint hardness-core ring, centre dot) at device scale.
  - 3 pinned unit tests (support gate, side cap + hotspot + pixel checks,
    crosshair corners).
  - Module doc's judgment call updated (bitmap path landed, painted =
    fallback).
- `viewport.rs`: `set_brush_cursor(ctx, painter, at, shape)` — bitmap via
  `ctx.set_cursor_image` when supported, else `CursorIcon::None` +
  `paint()`. All three cursor sites (resize drag, flow drag, hover) route
  through it.
- `main.rs`: `fn ui` clears `set_cursor_image(None)` at pass start — the
  `on_begin_pass` half of photocraft's plugin lifecycle, so the normal
  cursor returns everywhere except where the viewport set the bitmap.

## Verification (static, per envelope)

- `rustfmt --edition 2021 --check` CLEAN on all 3 touched .rs files.
- egui 0.36.2 API verified from the vendored source:
  `Context::set_cursor_image(Option<CustomCursorImage>)` and
  `CustomCursorImage { rgba: Arc<[u8]>, size: [u16; 2], hotspot: [u16; 2] }`
  (all pub) — the constructor literal matches.
- Greps: all three former `set_cursor_icon(None)+paint` sites now call
  `set_brush_cursor`; the only remaining `CursorIcon::None` in viewport.rs
  is the fallback branch inside the helper; the frame-start clear is in
  place at main.rs.
- NOT run: cargo build/clippy/tests (envelope). The 3 new unit tests run
  at the next battery. Commits local until the wave gate.

## Known semantics / deferred tails

- HiDPI (ppp > 1) intentionally uses the painted ring — the bitmap path
  requires scale exactly 1x (the plan's Wayland-≠1× rule, generalized).
- The bitmap shows ring + core + dot but no numeric HUD; the resize/flow
  drag HUDs still paint as before (they sit beside the cursor, not in it).
- CursorShape stays Copy; no per-frame allocation beyond the bitmap itself
  (one small Vec per frame while hovering the mesh — acceptable; a
  ctx.data cache like photocraft's is the named follow-up if profiling
  complains).
