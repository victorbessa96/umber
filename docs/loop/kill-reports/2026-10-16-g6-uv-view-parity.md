# Kill report: g6-uv-view-parity (GUI plan G6)

- Authorship: dragon (solo; two claws ran G3/G5 kills in parallel on
  disjoint surfaces).
- Kill-lock `g6-uv-view-parity`:
  `crates/umber-app/src/uv_view.rs`,
  `crates/umber-gpu/src/texture_display.rs`.
- Implements: `docs/specs/gui-plan-2026-10-15.md` row `g6-uv-view-parity`
  (P:C5, B:C7.1-C7.2): the 2D view reaches parity — UDIM grid with tile
  numbers, per-channel display (R/G/B/A/Gray), a hold-to-show pixel
  inspector (XY, RGBA, HSV).

## Built

- `umber-gpu/src/texture_display.rs`:
  - `ChannelIsolate` (All/Red/Green/Blue/Alpha/Gray) + `UvPresent`
    (per-frame presentation options; Default = All = the identity
    presentation).
  - The WGSL gains `isolate: u32` on `ScreenUniform`; `fs_main` applies
    the isolation BEFORE the display LUT (the LUT transforms the
    presented grey, so isolate stays inside the same colour pipeline as
    All; a wrong-side application would double-encode). 0 = All is the
    byte-identical pre-existing path.
  - `ScreenUniformData` (CPU mirror) carries the field; `callback` /
    `callback_for_view` take `UvPresent`; the callback stores the
    uniform value and `write_uniform` re-applies it every frame.
  - Golden `isolate_picks_its_channel` (the can-fail companion): each
    isolate presents ITS source channel as grey through the identity
    LUT, byte-exact against the CPU expectation; All stays the identity
    render (the regression arm). A uniform the shader ignores makes
    every isolate equal All and the test red.
- `umber-app/src/uv_view.rs`:
  - A footer strip (`FOOTER_H`) below the canvas: the `uv.isolate`
    combo (i18n'd, both catalogs) on the left; the inspector readout
    right-aligned.
  - The UDIM grid: when the mesh spans >1 tile, the 10×10 grid draws
    one cell per PRESENT tile (`present_tiles`), the tile number
    top-left, per-cell borders, and the ACTIVE tile's accent border.
    The active cell IS the paint square (all draw + interaction code
    works in tile-local UV unchanged — the single-tile contract holds
    by shadowing the `square` to the active cell). Triangles whose UVs
    live in another tile draw offset into their own cell (local UV =
    uv − active-tile origin), so multi-tile geometry is visible.
    Single-tile meshes: the grid never draws, layout byte-identical.
  - The shift-hold pixel inspector: while Shift is held and the pointer
    is on the active tile, ONE full-tile composite readback per gesture
    (cached by tile — a 4K readback per frame is too heavy),
    CPU-sampled at the pointer every frame; `inspector_line` renders
    "x, y  R.. G.. B.. A..  H..° S.. V.." (HSV from linear rgb — a
    display-space readout would double-encode). Release drops the cache.
  - The raw "no mesh loaded" literal now rides the existing
    `uv.no-mesh` catalog key (a small honesty fix inside the surface).

## Judgment calls

- Channel isolation in the SHADER, not a CPU resample: zero per-frame
  cost, one uniform write; the alternative (a CPU grey copy per combo
  change) would burn a 4K readback + upload per switch. Rejected for
  the presentation path; the INSPECTOR still reads back because it
  needs CPU texel values anyway.
- Grid cell CLICK = deferred: switching the active tile needs a
  `PaintState::set_active_tile` API (umber-paint, outside this lock);
  today the active tile follows stroke routing (`route_tile`). The
  named follow-up: `wave-uv-tile-click` (umber-paint setter + the click
  handler + the accent moves).
- Inspector y-mirroring: the readback's row 0 is the texture's TOP row;
  UV y grows upward, so the sample mirrors. Pinned by construction
  (read_tile/paint target share storage order); not separately tested
  on GPU here — the golden covers the shader arm, the pure tests cover
  the line math.
- The luma arm dots the RAW normalized samples (Rgba8Unorm, no sRGB
  decode) — the paint planes are linear, so Rec.709 over raw values is
  the correct linear luminance; my first draft gamma-decoded in the
  CPU expectation and was wrong (caught while writing the golden).

## Unverified

- Compiled by construction (no cargo locally; the battery is the wave
  gate on the build host). Every API resolved against source:
  `RgbaBuffer::texel` (umber-brush retouch.rs:167), `read_tile`
  (paint_state.rs:5254), `present_tiles` (udim.rs:96), egui 0.36
  `ComboBox::from_id_salt`/`show_ui`, `child_ui`.
- The inspector's GPU-tile interaction (read while the texture is
  mid-queue) rides `read_tile`'s flush-first contract.

## Deferred

- The tile-cell click (needs the umber-paint setter — named above).
- UDIM zoom/pan (the module's own named follow-up, unchanged).
- The isolate combo remembering per-session persistence (a preference
  row when the keymap prefs file gains it — G3's surface).

## Process note

- Two claws held disjoint surfaces in the same tree while this kill
  ran (G3 keymap in flight at this write, G5 home-card landed);
  the battery for the merged wave runs once all three land.
