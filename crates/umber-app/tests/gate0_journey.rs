//! Gate 0 — the core-loop journey test
//! (docs/specs/2026-10-12-strategy-review.md §2, the release-checklist
//! function gate).
//!
//! One test drives the six core-loop criticals JOINED, in user order:
//!
//! 1. **FBX**: `umber_mesh::load_fbx` loads the real binary cube fixture
//!    (`umber-mesh/tests/assets/cube_b7400.fbx`, the same file the
//!    `loads_fbx_cube_fixture` test pins).
//! 2. **4K pressure paint**: `PaintState::with_target_size(4096)` plus the
//!    CRITICAL-5 pair
//!    (`begin_screen_point_pressured`/`extend_screen_point_pressured`) at
//!    pressure 0.5, driven across the loaded mesh's UVs (`set_mesh` first,
//!    so the seam/UDIM routing sees the real cube). The stroke paints pure
//!    green (`set_brush_color`) so the painted texel is color-identifiable
//!    in every downstream leg.
//! 3. **See lit**: the CORE-LOOP bind is the paint tile feeding the mesh
//!    pass; its headless form is the COMPOSITOR's output — the painted
//!    tile composites (single `Normal` layer over transparent) and the
//!    painted texel differs from the unpainted state.
//! 4. **Undo/redo**: `undo_stroke` readback equals the pre-stroke bytes,
//!    `redo_stroke` equals the post-stroke bytes (the CRITICAL-3 pins).
//! 5. **Save/reopen**: the CRITICAL-4 sidecar round-trip at the
//!    `PaintState` level — readback → `write_png(Linear)` +
//!    `target_size.txt` → fresh session → `read_png_rgba8` + row-flip →
//!    `publish_fill` → byte-identical readback. Same helpers
//!    (`umber_export::png::{write_png, read_png_rgba8}`) and same session
//!    calls (`publish_fill`, the `WriteRegion` queue path) that
//!    `save_paint_tiles`/`load_paint_tiles` use.
//! 6. **ORM export decode-verified**: `ExportPreset::unreal_orm()` driven
//!    via `driver::run_preset`, then the written `Gate0_OpenglORM.png`
//!    DECODED (`read_png_rgba8`) with the packed channels asserted at the
//!    painted texel (R=AO, G=rough, B=metal).
//!
//! Judgment calls (v1 boundaries):
//!
//! * **Headless-GPU vs shell.** Paint, composite, undo, sidecar bytes and
//!   ORM all execute headless (lavapipe/WARP cover CI). The literal "see
//!   lit" viewport frame — the mesh pass sampling the tile under the
//!   eframe render loop — is shell-only; the compositor assertion is its
//!   headless equivalent (the compositor IS the paint→mesh bind's
//!   consumer-side contract: the tile's texels reach a downstream pass).
//! * **Shell-only remainder.** The rfd dialogs and the `project.json`
//!   commit around the sidecar (`save_project`/`open_project`) need the
//!   eframe shell and are NOT driven here; the filename codec, the flip
//!   inverse and the `save_paint_tiles`/`load_paint_tiles` byte round-trip
//!   are pinned by `main.rs`'s inline `paint_persistence_tests`, and this
//!   test replicates their file shape (`paint/tile-<N>.png`,
//!   `paint/target_size.txt` body `"{w}x{h}"`) so the two agree.
//! * **Pressure magnitude is not pinned.** The test drives the pressured
//!   entry points (a pressure-ignoring regression still stages through
//!   them, but the CRITICAL-5 wiring — pressure reaching the session — is
//!   what this gate owns); asserting an exact alpha would pin GPU
//!   coverage rounding, which is implementation-defined.
//! * **Memory.** 4096² RGBA8 is 64 MiB per buffer. The peak (post +
//!   BaseColor clone + four solid maps + the driver's packed outputs) is
//!   a few hundred MiB transient; earlier full-size buffers are dropped
//!   as soon as their leg ends.
//! * **4K is the point.** A smaller target would make every leg cheaper,
//!   but leg 2's directive is the 4K live target — shrinking it here
//!   would un-test the gate.

use std::path::PathBuf;

use umber_app::paint_state::PaintState;
use umber_core::BlendMode;
use umber_export::driver::{run_preset, MapSet, TokenSources};
use umber_export::png::{read_png_rgba8, write_png, Transfer};
use umber_export::presets::{ExportPreset, MapKind};
use umber_gpu::compositor::{GpuStackLayer, StackCompositor};
use umber_gpu::paint::PaintTarget;
use umber_mesh::{load_fbx, FIRST_TILE};

/// Paint-target edge length: the 4K live default (CORE-LOOP CRITICAL 2).
const SIZE: u32 = 4096;

/// Stroke pressure for the driven stroke (CRITICAL 5: pressure reaches
/// the session through the pressured entry points).
const PRESSURE: f32 = 0.5;

/// Distinct solid-map values for the ORM leg (mirror the driver's own
/// DirectX test values where they overlap: AO 230 there, distinct here
/// so a channel swap fails instead of hiding).
const AO_VALUE: u8 = 204;
const ROUGH_VALUE: u8 = 128;
const METAL_VALUE: u8 = 77;

/// Requests a paint-capable device (graceful skip where no wgpu adapter
/// exists — the same pattern as `paint_state::tests::try_request_device`;
/// lavapipe/WARP cover CI). Returns `None` when the test must be skipped.
fn try_request_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let Ok(adapter) =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
    else {
        eprintln!("skipping: no wgpu adapter available");
        return None;
    };
    if !adapter
        .features()
        .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
    {
        eprintln!("skipping: adapter lacks TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES");
        return None;
    }
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
        ..Default::default()
    }))
    .ok()
}

/// A solid `size`×`size` RGBA8 map with every byte `value` (so R=G=B=A=
/// `value` per texel — the `Gray` wiring slot's Rec.709 luminance of a
/// gray texel is exactly the input value, which the ORM leg depends on).
fn solid(size: u32, value: u8) -> Vec<u8> {
    vec![value; size as usize * size as usize * 4]
}

/// Vertical row flip over tightly-packed RGBA8 bytes (mirrors
/// `main.rs::flip_rows_vertical`: `read_png_rgba8` returns GPU row order,
/// so the load path flips back before upload — without it the round trip
/// is a vertical mirror).
fn flip_rows_vertical(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let stride = width as usize * 4;
    let rows = height as usize;
    let mut out = vec![0u8; rgba.len()];
    for row in 0..rows {
        let src = row * stride;
        let dst = (rows - 1 - row) * stride;
        out[dst..dst + stride].copy_from_slice(&rgba[src..src + stride]);
    }
    out
}

#[test]
fn gate0_journey_fbx_to_decode_verified_orm() {
    // ---- Leg 1: FBX (CPU; always runs, even where the GPU legs skip). ----
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../umber-mesh/tests/assets/cube_b7400.fbx");
    let mesh = load_fbx(&fixture).expect("cube fbx fixture must load");
    assert!(
        mesh.vertex_count() >= 8,
        "cube needs >= 8 verts, got {}",
        mesh.vertex_count()
    );
    assert!(
        mesh.triangle_count() >= 12,
        "cube needs >= 12 tris, got {}",
        mesh.triangle_count()
    );
    assert_eq!(mesh.positions.len(), mesh.normals.len());
    assert_eq!(mesh.positions.len(), mesh.uvs.len());

    // ---- Legs 2-6 need a headless GPU; skip (passing) without one. ----
    let Some((device, queue)) = try_request_device() else {
        return;
    };

    // ---- Leg 2: 4K pressure paint on the loaded mesh's UVs. ----
    let mut paint =
        PaintState::with_target_size(device.clone(), queue.clone(), SIZE).expect("paint builds");
    assert_eq!(paint.target_size(), (SIZE, SIZE));
    // Pure-green stroke: R and B stay exactly 0 under any coverage (0 ×
    // anything is exact — no rounding), so the painted texel pins color,
    // not just alpha.
    paint.set_brush_color([0.0, 1.0, 0.0, 1.0]);
    paint.set_mesh(&mesh);
    paint.process_pending().expect("pre-stroke drain succeeds");
    let pre = paint
        .paint_target()
        .read_back_rgba8(&device, &queue)
        .expect("pre-stroke readback succeeds");
    assert_eq!(pre.len(), SIZE as usize * SIZE as usize * 4);

    paint.begin_screen_point_pressured(Some(egui::pos2(0.30, 0.30)), PRESSURE);
    for i in 1..=16 {
        let t = i as f32 * 0.02;
        paint.extend_screen_point_pressured(Some(egui::pos2(0.30 + t, 0.30 + t)), PRESSURE);
    }
    paint.end_stroke();
    assert!(
        paint.can_undo_stroke(),
        "the driven stroke must record one undo entry"
    );
    paint.process_pending().expect("post-stroke drain succeeds");
    let post = paint
        .paint_target()
        .read_back_rgba8(&device, &queue)
        .expect("post-stroke readback succeeds");
    assert_eq!(post.len(), pre.len());
    assert_eq!(paint.tiles_present(), vec![FIRST_TILE]);

    // The painted texel: strongest alpha wins (deterministic — the same
    // stroke always conditions identically, so the max is stable).
    let mut painted_count = 0usize;
    let mut painted_texel = 0usize;
    let mut best_alpha = 0u8;
    for (t, px) in post.chunks_exact(4).enumerate() {
        if px[3] > 0 {
            painted_count += 1;
            if px[3] > best_alpha {
                best_alpha = px[3];
                painted_texel = t;
            }
        }
    }
    // Range, not exact: GPU dab-edge coverage rounding is
    // implementation-defined, but one r=24-texel dab covers ~1800 texels,
    // so 100 is far below any real stroke and a vacuous (unstaged) stroke
    // fails here instead of passing silently.
    assert!(
        painted_count >= 100,
        "the stroke must paint texels (got {painted_count})"
    );
    let painted_x = (painted_texel % SIZE as usize) as u32;
    let painted_y = (painted_texel / SIZE as usize) as u32;
    let painted_base = painted_texel * 4;
    assert_eq!(
        pre[painted_base..painted_base + 4],
        [0, 0, 0, 0],
        "the painted texel starts transparent"
    );
    assert_ne!(
        post[painted_base..painted_base + 4],
        [0, 0, 0, 0],
        "the stroke must change the painted texel"
    );
    // Color identity: the stroke is pure green, so R and B are exactly 0
    // and G is positive at the painted texel (a default-color or
    // color-dropping regression fails here).
    assert_eq!(post[painted_base], 0, "painted R must be 0");
    assert_eq!(post[painted_base + 2], 0, "painted B must be 0");
    assert!(
        post[painted_base + 1] > 0,
        "painted G must be positive, got {}",
        post[painted_base + 1]
    );

    // ---- Leg 3: see lit — the painted tile composites downstream. ----
    let compositor =
        StackCompositor::new(device.clone()).expect("compositor builds on the paint device");
    let output = PaintTarget::new(&device, SIZE, SIZE);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("umber_gate0_composite"),
    });
    let layers = [GpuStackLayer {
        blend_mode: BlendMode::Normal,
        opacity: 1.0,
        visible: true,
        texture: Some(paint.paint_target()),
        mask: None,
    }];
    compositor
        .composite_stack(&mut encoder, &layers, &output)
        .expect("composite records");
    queue.submit(Some(encoder.finish()));
    let comp = output
        .read_back_rgba8(&device, &queue)
        .expect("composite readback succeeds");
    assert_eq!(comp.len(), post.len());
    // Exact inequality (not bytes): UNORM round-trip/FMA make GPU bytes
    // ±1 LSB vs any CPU prediction (see compositor module docs), but a
    // dropped layer composites to transparent — which fails here.
    assert_ne!(
        comp[painted_base..painted_base + 4],
        [0, 0, 0, 0],
        "the composite must carry the paint at the painted texel"
    );

    // ---- Leg 4: undo/redo (byte-exact — WriteRegion restores slices). ----
    assert!(paint.undo_stroke(), "one entry must be undoable");
    assert!(paint.can_redo_stroke(), "redo must arm after undo");
    paint.process_pending().expect("undo drain succeeds");
    let undone = paint
        .paint_target()
        .read_back_rgba8(&device, &queue)
        .expect("undo readback succeeds");
    assert_eq!(
        undone, pre,
        "undo must restore the pre-stroke bytes exactly"
    );
    assert!(paint.redo_stroke(), "the undone stroke must redo");
    paint.process_pending().expect("redo drain succeeds");
    let redone = paint
        .paint_target()
        .read_back_rgba8(&device, &queue)
        .expect("redo readback succeeds");
    assert_eq!(
        redone, post,
        "redo must restore the post-stroke bytes exactly"
    );
    drop(pre);
    drop(undone);
    drop(redone);

    // ---- Leg 5: save/reopen — the sidecar round-trip at session level. ----
    let base = std::env::temp_dir().join(format!("umber-gate0-{}", std::process::id()));
    let paint_dir = base.join("project").join("paint");
    let sidecar = paint_dir.join("tile-1001.png");
    write_png(&sidecar, SIZE, SIZE, &post, Transfer::Linear).expect("sidecar PNG writes");
    std::fs::write(paint_dir.join("target_size.txt"), "4096x4096").expect("size file writes");
    assert!(sidecar.exists(), "the sidecar file must exist after save");
    let mut fresh =
        PaintState::with_target_size(device.clone(), queue.clone(), SIZE).expect("fresh builds");
    let (rw, rh, rbytes) = read_png_rgba8(&sidecar).expect("sidecar PNG decodes");
    assert_eq!((rw, rh), (SIZE, SIZE));
    assert!(
        fresh.publish_fill(FIRST_TILE, rw, rh, flip_rows_vertical(&rbytes, rw, rh)),
        "the sidecar upload must land"
    );
    fresh.process_pending().expect("restore drain succeeds");
    assert_eq!(fresh.tiles_present(), vec![FIRST_TILE]);
    let restored = fresh
        .paint_target()
        .read_back_rgba8(&device, &queue)
        .expect("restored readback succeeds");
    assert_eq!(
        restored, post,
        "save→reopen must restore byte-identical texels"
    );
    drop(restored);
    drop(fresh);

    // ---- Leg 6: ORM export, decode-verified at the painted texel. ----
    let mut set = MapSet::new(SIZE);
    set.set(MapKind::BaseColor, post.clone());
    set.set(MapKind::AmbientOcclusion, solid(SIZE, AO_VALUE));
    set.set(MapKind::Roughness, solid(SIZE, ROUGH_VALUE));
    set.set(MapKind::Metallic, solid(SIZE, METAL_VALUE));
    set.set(MapKind::Normal, solid(SIZE, 128));
    drop(post);
    let out_dir = base.join("export");
    let written = run_preset(
        &ExportPreset::unreal_orm(),
        &set,
        &TokenSources::new("Gate0"),
        &out_dir,
    )
    .expect("ORM preset runs");
    assert_eq!(written.len(), 3, "Unreal ORM writes 3 outputs");
    // NOTE: `Path::ends_with` matches whole path components, not string
    // suffixes — match on the file name string (the driver's own tests do
    // the same).
    let orm_path = written
        .iter()
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("OpenglORM.png"))
        })
        .expect("ORM output written");
    let orm_name = orm_path
        .file_name()
        .and_then(|n| n.to_str())
        .expect("ORM path has a file name");
    assert_eq!(orm_name, "Gate0_OpenglORM.png");
    let (ow, oh, obytes) = read_png_rgba8(orm_path).expect("ORM PNG decodes");
    assert_eq!((ow, oh), (SIZE, SIZE));
    // The decoder returns GPU row order (output row 0 = the file's last
    // stored row) while the export packed top-down: flip the painted row.
    let flipped_y = oh - 1 - painted_y;
    let o = (flipped_y * ow + painted_x) as usize * 4;
    // ORM packing is R=AO / G=rough / B=metal over a Linear transfer (no
    // sRGB curve on data maps), and solid-gray luminance is exact — so
    // every channel pins exactly, alpha included (A = AO alpha).
    assert_eq!(obytes[o], AO_VALUE, "ORM R must be the AO value");
    assert_eq!(
        obytes[o + 1],
        ROUGH_VALUE,
        "ORM G must be the roughness value"
    );
    assert_eq!(
        obytes[o + 2],
        METAL_VALUE,
        "ORM B must be the metallic value"
    );
    assert_eq!(obytes[o + 3], AO_VALUE, "ORM A must be the AO alpha");

    let _ = std::fs::remove_dir_all(&base);
}
