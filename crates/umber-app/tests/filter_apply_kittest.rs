//! The app-side smart-filter apply path (the Assets panel's Filters
//! category): `filter_apply::build_filter_op` over the live paint session
//! plus the single-undo-step stack, driven exactly the way the shell's
//! `apply_filter_file` and the Undo/Redo arms drive it.
use umber_app::filter_apply::{build_filter_op, FilterUndoStack};
use umber_app::history_panel::{HistoryKind, SessionHistory};
use umber_app::paint_state::{PaintState, PlaneOwner, BASE_COLOR_CHANNEL};
use umber_core::smart::SmartFilter;

fn try_request_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::default();
    let Ok(adapter) =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
    else {
        return None;
    };
    if !adapter
        .features()
        .contains(wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES)
    {
        return None;
    }
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
        ..Default::default()
    }))
    .ok()
}

fn levels_filter() -> SmartFilter {
    SmartFilter::from_json(
        r#"{"name":"Crunch","version":1,"ops":[{"type":"levels","low":0.1,"high":0.9,"gamma":0.8}]}"#,
    )
    .unwrap()
}

fn read_layer(paint: &mut PaintState, layer: u64) -> Vec<u8> {
    let tile = paint.tiles_present()[0];
    paint
        .read_plane(PlaneOwner::Layer(layer), tile, BASE_COLOR_CHANNEL)
        .expect("the plane materialized")
        .data
}

#[test]
fn filter_apply_is_one_undo_step_and_reverts_exactly() {
    let Some((device, queue)) = try_request_device() else {
        eprintln!("no gpu; skipping");
        return;
    };
    let size = 8u32;
    let mut paint = PaintState::with_target_size(device, queue, size).expect("paint builds");
    let tile = paint.tiles_present()[0];
    let (w, h) = paint.tile_size(tile);
    let before = vec![100u8; (size * size * 4) as usize];
    paint
        .publish_plane(
            PlaneOwner::Layer(7),
            tile,
            BASE_COLOR_CHANNEL,
            w,
            h,
            before.clone(),
        )
        .expect("publish");

    // The stroke journal does not see plane publishes — the filter step
    // must be the shell's own single undo entry, not a stroke row.
    assert_eq!(paint.stroke_len(), 0, "a publish is not a stroke");

    let filter = levels_filter();
    let op = build_filter_op(&mut paint, 7, &filter)
        .expect("the filter applies")
        .expect("the layer has a materialized plane");
    assert_eq!(op.planes.len(), 1);
    assert_eq!(op.planes[0].before, before, "the op pins the before bytes");
    let after = op.planes[0].after.clone();
    assert_ne!(after, before, "levels moved the bytes");
    // build_filter_op does NOT publish: the plane still holds `before`.
    assert_eq!(read_layer(&mut paint, 7), before, "build only reads");

    // The shell's apply: one context row + one stack push (which publishes).
    let mut history = SessionHistory::new();
    history.record_context(HistoryKind::FilterApply);
    let op_id = history.entries().last().unwrap().op_id;
    let mut stack = FilterUndoStack::default();
    stack.push(op_id, op, &mut paint);
    assert_eq!(read_layer(&mut paint, 7), after, "the push published once");
    assert_eq!(stack.undo_len(), 1, "exactly one undo step");
    assert!(
        stack.newest_is_ours(history.entries().last()),
        "newest row is ours"
    );
    assert!(
        !stack.redo_is_next(history.next_op_id()),
        "nothing to redo yet"
    );

    // Undo arm: newest row is the filter's → revert + pop the row.
    let newest = history.entries().last().cloned();
    assert!(stack.newest_is_ours(newest.as_ref()));
    let popped = stack.undo(&mut paint).expect("one step to undo");
    assert_eq!(popped, op_id);
    history.pop_last();
    assert_eq!(
        read_layer(&mut paint, 7),
        before,
        "undo restored the before bytes"
    );
    assert_eq!(paint.stroke_len(), 0, "still no stroke entry");

    // Redo arm: the undone row's id + 1 is the next op id → redo first.
    assert!(
        stack.redo_is_next(history.next_op_id()),
        "the filter is next to redo"
    );
    history.record_context(HistoryKind::FilterApply);
    let new_id = history.entries().last().unwrap().op_id;
    assert!(stack.redo(&mut paint, new_id));
    assert_eq!(read_layer(&mut paint, 7), after, "redo re-published");
    assert!(stack.newest_is_ours(history.entries().last()));
    assert_eq!(stack.undo_len(), 1, "redo re-filed the step");
}

#[test]
fn filter_on_a_layer_without_pixels_is_a_no_op_and_empty_filter_errors() {
    let Some((device, queue)) = try_request_device() else {
        eprintln!("no gpu; skipping");
        return;
    };
    let mut paint = PaintState::with_target_size(device, queue, 8).expect("paint builds");
    // Layer 9 was never written: no materialized plane.
    let filter = levels_filter();
    assert!(
        build_filter_op(&mut paint, 9, &filter).unwrap().is_none(),
        "nothing to filter -> Ok(None), nothing pushed"
    );
    // An ops-less preset refuses at apply time (load-time validation
    // passed it): the shell surfaces the error and pushes nothing.
    let empty = SmartFilter::from_json(r#"{"name":"Empty","version":1,"ops":[]}"#).unwrap();
    let tile = paint.tiles_present()[0];
    let (w, h) = paint.tile_size(tile);
    paint
        .publish_plane(
            PlaneOwner::Layer(7),
            tile,
            BASE_COLOR_CHANNEL,
            w,
            h,
            vec![7u8; (w * h * 4) as usize],
        )
        .unwrap();
    assert!(
        build_filter_op(&mut paint, 7, &empty).is_err(),
        "empty filter refuses"
    );
}
