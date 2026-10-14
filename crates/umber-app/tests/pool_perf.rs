//! The per-layer-pool stage-4 perf harnesses ([M8], docs/perf/protocol.md's
//! stage-4 gate). Nightly-gated shape (NOT per-commit assertions): each
//! prints its P95 verdict line without failing on the budget — the
//! baselines table + the >10% regression comparison are the gate.
//!
//! Both harnesses drive the REAL paths on a real device (lavapipe in CI,
//! the dev GPU locally — separate baselines, recorded with the row):
//! - `pool_drag_session` (the metadata-drag scenario): one synthetic
//!   opacity change per frame through the adoption → refresh →
//!   end-of-update flush cycle of a 5-channel MASKED material at 4K
//!   (all channels enabled, every entry masked). Gate: P95 frame
//!   < 16.7 ms.
//! - `pool_mirror_session` (the X-symmetry scenario): one stroke with
//!   X symmetry straddling the tile edge (a mirrored pair at x≈100 and
//!   x≈3996), per-frame flush wall time + the dirt census asserting the
//!   pair stays TWO rects (the unit pin `distant_footprints_stay_two_rects`
//!   in paint_state.rs carries the census; this harness carries the wall
//!   time). Gate: P95 frame < 16.7 ms.
//!
//! The stage-4 kill consumes these: the numbers land in the baselines
//! table (docs/perf/protocol.md) beside the pre-stage-4 i2p row, and the
//! verdict is recorded in the kill report. `UMBER_POOL_DRAG_FRAMES`
//! (default 120) and `UMBER_POOL_MIRROR_FRAMES` size the sessions.

use std::time::{Duration, Instant};

use egui::Pos2;
use umber_app::paint_state::{
    CompositeSnapshot, PlaneOwner, SnapshotLayer, SnapshotLayerKind, SnapshotMaterialChannel,
};
use umber_core::BlendMode;

/// The adapter request (perf_soak's proven shape, inlined: dev-deps
/// are reachable from integration tests, lib modules are not).
fn try_request_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let Ok(adapter) =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
    else {
        eprintln!("pool-perf: no wgpu adapter available");
        return None;
    };
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
        ..Default::default()
    }))
    .ok()
}

/// Frames per session (the protocol's default; baselines use ≥ 1000).
fn frames_from_env(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// The P50/P95/P99 wall-time percentiles of `samples`, in ms.
fn percentiles(mut samples: Vec<f64>) -> (f64, f64, f64) {
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let at = |q: f64| -> f64 {
        if samples.is_empty() {
            return 0.0;
        }
        let idx = ((samples.len() - 1) as f64 * q).round() as usize;
        samples[idx.min(samples.len() - 1)]
    };
    (at(0.50), at(0.95), at(0.99))
}

fn print_verdict(name: &str, label: &str, p50: f64, p95: f64, p99: f64, budget_ms: f64) {
    // One human row + one machine row (the protocol's two-line convention).
    println!(
        "pool-{name} baseline: p50 {p50:.2}ms p95 {p95:.2}ms p99 {p99:.2}ms (budget {budget_ms}ms: {})",
        if p95 < budget_ms { "PASS" } else { "FAIL" }
    );
    println!(
        "pool-{name} json: {{\"p50_ms\":{p50},\"p95_ms\":{p95},\"p99_ms\":{p99},\"budget_ms\":{budget_ms},\"scenario\":\"{label}\"}}"
    );
}

/// The real snapshot shape the flush consumes: one Paint layer over
/// Base (the harness's minimal stack), Base visible, the pair displayed.
/// The opacity row edits through this snapshot — each frame builds the
/// next one, exactly as the shell does.
fn harness_snapshot(opacity: f32, displayed: &[(u16, String)]) -> CompositeSnapshot {
    CompositeSnapshot {
        layers: vec![
            SnapshotLayer {
                owner: PlaneOwner::Base,
                layer_id: None,
                kind: SnapshotLayerKind::Paint,
                blend: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                mask_enabled: false,
                material_channels: None,
            },
            SnapshotLayer {
                owner: PlaneOwner::Layer(1),
                layer_id: Some(1),
                kind: SnapshotLayerKind::Paint,
                blend: BlendMode::Normal,
                opacity,
                visible: true,
                mask_enabled: false,
                material_channels: None,
            },
        ],
        base_visible: true,
        displayed: displayed.to_vec(),
    }
}

fn wait_gpu(device: &wgpu::Device) {
    let _ = device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
}

/// The five material channels the drag scenario composites (§8 [M8]:
/// "a 5-channel masked material").
const DRAG_CHANNELS: [&str; 5] = ["baseColor", "roughness", "metallic", "normal", "height"];

/// The drag scenario's stack: Base + one 5-channel MASKED material
/// (`Layer(1)`), every channel Normal at `opacity` (the shell folds layer
/// opacity into each channel entry at snapshot time — main.rs's
/// `build_composite_snapshot`).
fn drag_snapshot(opacity: f32, displayed: &[(u16, String)]) -> CompositeSnapshot {
    let channels = DRAG_CHANNELS
        .iter()
        .map(|channel| SnapshotMaterialChannel {
            channel: (*channel).to_string(),
            blend: BlendMode::Normal,
            opacity,
        })
        .collect();
    CompositeSnapshot {
        layers: vec![
            SnapshotLayer {
                owner: PlaneOwner::Base,
                layer_id: None,
                kind: SnapshotLayerKind::Paint,
                blend: BlendMode::Normal,
                opacity: 1.0,
                visible: true,
                mask_enabled: false,
                material_channels: None,
            },
            SnapshotLayer {
                owner: PlaneOwner::Layer(1),
                layer_id: Some(1),
                kind: SnapshotLayerKind::Material,
                blend: BlendMode::Normal,
                opacity,
                visible: true,
                mask_enabled: std::env::var("UMBER_POOL_DRAG_NOMASK").is_err(),
                material_channels: Some(channels),
            },
        ],
        base_visible: true,
        displayed: displayed.to_vec(),
    }
}

/// [M8] scenario 1 — the metadata drag: one opacity edit per frame on a
/// 5-channel masked material at 4K, through the shell's real per-frame
/// cycle: snapshot diff (`CompositeSnapshot::full_owners`) → owners marked
/// `Full` → the end-of-update flush. The shell displays the active tile's
/// baseColor + height (main.rs), so those two recomposite per frame and
/// the other three coalesce per [M8] — exactly what the frame pays.
///
/// History: the first version of this harness (stage-4 kill) published no
/// layer planes and never marked dirt, so every frame flushed nothing and
/// it reported a meaningless ~0.003 ms. The sanity pin after the session
/// makes that failure mode impossible to repeat silently.
#[test]
fn pool_drag_session() {
    const TARGET: u32 = 4096; // 4K per the protocol
    const BUDGET_MS: f64 = 16.7;
    let frames = frames_from_env("UMBER_POOL_DRAG_FRAMES", 120);

    let Some((device, queue)) = try_request_device() else {
        eprintln!("pool-drag: no device, skipping (headless env without lavapipe)");
        return;
    };
    let poll_device = device.clone();
    let mut paint =
        umber_app::paint_state::PaintState::new(device, queue).expect("paint state builds");
    paint.resize_target(TARGET, TARGET);
    paint.process_pending().expect("resize applies");

    // The material's five planes (premultiplied, opaque mid-grey) and its
    // mask (a left-half-open gradient: every entry is MASKED for real, not
    // a white no-op plane).
    let texels = TARGET as usize * TARGET as usize;
    let plane: Vec<u8> = [96u8, 96, 96, 255].repeat(texels);
    for channel in DRAG_CHANNELS {
        paint
            .publish_plane(
                PlaneOwner::Layer(1),
                1001,
                channel,
                TARGET,
                TARGET,
                plane.clone(),
            )
            .expect("material plane publishes");
    }
    let width = TARGET as usize;
    let mask: Vec<u8> = (0..texels)
        .map(|i| if i % width < width / 2 { 255 } else { 64 })
        .collect();
    assert!(paint.publish_layer_mask(1, 1001, mask), "mask publishes");
    // The displayed caches exist (the viewport's binds read them every
    // frame — materialize them the way a reader does).
    let displayed: Vec<(u16, String)> = vec![
        (1001, "baseColor".to_string()),
        (1001, "height".to_string()),
    ];
    let mut prev = drag_snapshot(0.0, &displayed);
    paint.flush_composite(&prev);
    for (tile, channel) in &displayed {
        paint
            .read_composite(*tile, channel)
            .expect("displayed cache reads");
    }
    paint.process_pending().expect("drain succeeds");
    wait_gpu(&poll_device);

    // The drag: opacity 0 → 1 → 0, one step per frame (egui's per-frame
    // slider emission shape).
    let steps = frames.max(2);
    let mut samples = Vec::with_capacity(steps);
    for i in 0..steps {
        let t = (i + 1) as f64 / (steps + 1) as f64;
        let opacity = (t * std::f64::consts::PI).sin().abs() as f32;
        let start = Instant::now();
        let next = drag_snapshot(opacity, &displayed);
        for owner in CompositeSnapshot::full_owners(&prev, &next) {
            paint.mark_owner_full(owner);
        }
        paint.flush_composite(&next);
        paint.process_pending().expect("drain succeeds");
        wait_gpu(&poll_device);
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
        prev = next;
    }

    // Sanity pin: the session really composited. Opacity 1.0 over a
    // transparent Base gives the material's 96 in the mask-open half; the
    // opacity-0 frame gives 0. A harness that flushes nothing fails here.
    for (opacity, want) in [(1.0f32, 96u8), (0.0, 0)] {
        let next = drag_snapshot(opacity, &displayed);
        for owner in CompositeSnapshot::full_owners(&prev, &next) {
            paint.mark_owner_full(owner);
        }
        paint.flush_composite(&next);
        prev = next;
        let cache = paint
            .read_composite(1001, "baseColor")
            .expect("cache reads");
        let got = cache.data[0];
        assert!(
            (got as i16 - want as i16).abs() <= 2,
            "pool-drag sanity: opacity {opacity} must composite {want}, got {got}"
        );
    }

    let (p50, p95, p99) = percentiles(samples);
    print_verdict(
        "drag",
        "metadata-drag-4k-masked-material",
        p50,
        p95,
        p99,
        BUDGET_MS,
    );
}

/// [M8] scenario 2 — the X-symmetry stroke near the tile edge: the
/// mirrored pair's per-frame flush cost.
#[test]
fn pool_mirror_session() {
    const TARGET: u32 = 4096;
    const BUDGET_MS: f64 = 16.7;
    let frames = frames_from_env("UMBER_POOL_MIRROR_FRAMES", 120);

    let Some((device, queue)) = try_request_device() else {
        eprintln!("pool-mirror: no device, skipping (headless env without lavapipe)");
        return;
    };
    let poll_device = device.clone();
    let mut paint =
        umber_app::paint_state::PaintState::new(device, queue).expect("paint state builds");
    paint.resize_target(TARGET, TARGET);
    paint.process_pending().expect("resize applies");
    wait_gpu(&poll_device);
    // X-symmetry on (the panel's shape: sync_symmetry adopts between
    // strokes, the paint_state.rs:1722 contract).
    let mut sym = umber_brush::SymmetrySettings::default();
    sym.mirror.enabled = true;
    sym.mirror.axis = umber_brush::MirrorAxis::X; // reflect U (the vertical line)
    paint.sync_symmetry(&mut sym);

    // The stroke: begin near the left edge, drag across the mirror line
    // (x≈0.5 in UV = the tile's middle), end — one event per frame, the
    // per-frame flush measured after each.
    paint.begin_stroke(Pos2::new(0.02, 0.5));
    let mut samples = Vec::with_capacity(frames);
    for i in 0..frames {
        let t = (i + 1) as f32 / (frames + 1) as f32;
        let start = Instant::now();
        paint.extend_stroke(Pos2::new(0.02 + 0.96 * t, 0.5 + 0.02 * t.sin()));
        let displayed = vec![(1001u16, "baseColor".to_string())];
        let snapshot = harness_snapshot(1.0, &displayed);
        paint.flush_composite(&snapshot);
        paint.process_pending().expect("drain succeeds");
        wait_gpu(&poll_device);
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    paint.end_stroke();
    let displayed = vec![(1001u16, "baseColor".to_string())];
    let snapshot = harness_snapshot(1.0, &displayed);
    paint.flush_composite(&snapshot);
    paint.process_pending().expect("drain succeeds");
    wait_gpu(&poll_device);

    // Sanity pin: the stroke really landed and composited. Both mirrored
    // footprints sit on row y = 0.5: the cache must be non-transparent at
    // the stroke's left start AND at its X-mirror (a harness whose flush
    // composited nothing, or whose symmetry was off, fails here).
    let cache = paint
        .read_composite(1001, "baseColor")
        .expect("cache reads");
    let row = (TARGET / 2) as usize;
    let alpha_at = |x: usize| cache.data[(row * TARGET as usize + x) * 4 + 3];
    let left = (0.03 * TARGET as f32) as usize;
    let mirror = TARGET as usize - 1 - left;
    assert!(
        alpha_at(left) > 0 && alpha_at(mirror) > 0,
        "pool-mirror sanity: stroke + X mirror must composite (alpha {} / {})",
        alpha_at(left),
        alpha_at(mirror)
    );

    let (p50, p95, p99) = percentiles(samples);
    print_verdict(
        "mirror",
        "x-symmetry-stroke-near-edge",
        p50,
        p95,
        p99,
        BUDGET_MS,
    );
    // The census pin lives in paint_state.rs's unit test
    // (distant_footprints_stay_two_rects) — this harness carries only
    // the wall time; the protocol names both as the gate's evidence.
    let _ = Duration::ZERO; // keep Duration in scope for future rows
}
