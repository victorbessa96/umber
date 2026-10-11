//! The post-effects section of the Display panel (requirements.md §7 —
//! the post-effects stack): edits the ordered
//! [`umber_gpu::postfx::PostChain`] on `AppState::postfx` — add an effect,
//! enable/disable it, tune its params, drag (or ▲/▼) to reorder, remove.
//!
//! The panel writes data only: every param edit is written back through
//! [`PostEffect::clamped`] (the model is the single clamp point; the
//! slider ranges below match those clamps so the write-back is a no-op in
//! practice), and structural edits (move, remove) are collected during
//! the row loop and applied after it, so no row ever indexes a chain that
//! changed under it.
//!
//! The status line says whether the chain is live in the viewport
//! ([`PostFxStatus`]: idle with nothing enabled, live, or bypassed under
//! a data view mode — see `umber_gpu::postfx_viewport`). v1 limit (also
//! said there): the chain is session state, not saved in the `.umber`
//! project yet.

use crate::i18n;
use egui::Ui;
use fluent::FluentValue;
use umber_gpu::postfx::{PostChain, PostEffect, PostPass, ToneMapMode};
use umber_gpu::{PostFxStatus, ViewMode};

/// The panel's session state (never serialized): the "Add effect" pick
/// and the row being dragged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PostFxPanel {
    add_kind: PostPass,
    drag: Option<RowDrag>,
}

impl Default for PostFxPanel {
    fn default() -> Self {
        Self {
            add_kind: PostPass::ToneMap,
            drag: None,
        }
    }
}

/// A drag in flight: the row it started on and where it would land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RowDrag {
    from: usize,
    to: usize,
}

/// A structural edit, applied after the row loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowEdit {
    Move { from: usize, to: usize },
    Remove(usize),
}

fn effect_label(pass: PostPass) -> String {
    match pass {
        PostPass::Dof => i18n::tr("postfx.dof"),
        PostPass::Bloom => i18n::tr("postfx.bloom"),
        PostPass::Vignette => i18n::tr("postfx.vignette"),
        PostPass::Sharpen => i18n::tr("postfx.sharpen"),
        PostPass::ToneMap => i18n::tr("postfx.tonemap"),
    }
}

fn mode_label(mode: ToneMapMode) -> String {
    match mode {
        ToneMapMode::Aces => i18n::tr("postfx.mode-aces"),
        ToneMapMode::Filmic => i18n::tr("postfx.mode-filmic"),
        ToneMapMode::Reinhard => i18n::tr("postfx.mode-reinhard"),
    }
}

/// The index a dragged row ends at ([`PostChain::reorder`]'s `to`): the
/// number of OTHER rows whose vertical center lies above the pointer.
fn drop_index(centers: &[f32], from: usize, pointer_y: f32) -> usize {
    centers
        .iter()
        .enumerate()
        .filter(|&(j, &c)| j != from && c < pointer_y)
        .count()
}

/// The status line's count: how many effects run, in what order.
fn summary_label(chain: &PostChain) -> String {
    let args = [
        (
            "active",
            FluentValue::from(chain.active().count().to_string()),
        ),
        ("total", FluentValue::from(chain.effects.len().to_string())),
    ];
    i18n::tr_args("postfx.summary", args)
}

/// The status line's route: whether the viewport is running the chain.
fn status_label(status: PostFxStatus) -> String {
    match status {
        PostFxStatus::Idle => i18n::tr("postfx.status-idle"),
        PostFxStatus::Live => i18n::tr("postfx.status-live"),
        PostFxStatus::BypassedDataMode => i18n::tr("postfx.status-bypassed"),
    }
}

/// One effect's param sliders; true when a value changed. Ranges mirror
/// [`PostEffect::clamped`].
fn param_rows(ui: &mut Ui, effect: &mut PostEffect) -> bool {
    let mut changed = false;
    match effect {
        PostEffect::Dof { focus, aperture } => {
            changed |= ui
                .add(
                    egui::Slider::new(focus, 0.01..=100.0)
                        .logarithmic(true)
                        .text(i18n::tr("postfx.focus")),
                )
                .changed();
            changed |= ui
                .add(egui::Slider::new(aperture, 0.0..=1.0).text(i18n::tr("postfx.aperture")))
                .changed();
        }
        PostEffect::Bloom {
            threshold,
            intensity,
            radius,
        } => {
            changed |= ui
                .add(egui::Slider::new(threshold, 0.0..=4.0).text(i18n::tr("postfx.threshold")))
                .changed();
            changed |= ui
                .add(egui::Slider::new(intensity, 0.0..=4.0).text(i18n::tr("postfx.intensity")))
                .changed();
            changed |= ui
                .add(egui::Slider::new(radius, 1..=64).text(i18n::tr("postfx.radius")))
                .changed();
        }
        PostEffect::Vignette { strength, softness } => {
            changed |= ui
                .add(egui::Slider::new(strength, 0.0..=1.0).text(i18n::tr("postfx.strength")))
                .changed();
            changed |= ui
                .add(egui::Slider::new(softness, 0.0..=1.0).text(i18n::tr("postfx.softness")))
                .changed();
        }
        PostEffect::Sharpen { amount, radius } => {
            changed |= ui
                .add(egui::Slider::new(amount, 0.0..=4.0).text(i18n::tr("postfx.amount")))
                .changed();
            changed |= ui
                .add(egui::Slider::new(radius, 1..=8).text(i18n::tr("postfx.radius")))
                .changed();
        }
        PostEffect::ToneMap { mode, exposure } => {
            // `from_label` ids are scoped by the row's `push_id`, so two
            // tone-map rows do not share a combo.
            egui::ComboBox::from_label(i18n::tr("postfx.mode"))
                .selected_text(mode_label(*mode))
                .show_ui(ui, |ui| {
                    for candidate in ToneMapMode::ALL {
                        changed |= ui
                            .selectable_value(mode, candidate, mode_label(candidate))
                            .changed();
                    }
                });
            changed |= ui
                .add(egui::Slider::new(exposure, -4.0..=4.0).text(i18n::tr("postfx.exposure")))
                .changed();
        }
    }
    changed
}

/// Draws the section and edits `chain` in place. `mode` is the viewport's
/// view mode (the status line: a data mode bypasses the chain). Returns
/// whether the chain changed this frame.
pub fn show(ui: &mut Ui, chain: &mut PostChain, panel: &mut PostFxPanel, mode: ViewMode) -> bool {
    let mut changed = false;
    egui::CollapsingHeader::new(i18n::tr("postfx.title"))
        .default_open(true)
        .show(ui, |ui| {
            changed = show_body(ui, chain, panel, mode);
        });
    changed
}

fn show_body(ui: &mut Ui, chain: &mut PostChain, panel: &mut PostFxPanel, mode: ViewMode) -> bool {
    let mut changed = false;
    let mut edit: Option<RowEdit> = None;
    let n = chain.effects.len();
    let mut rows: Vec<egui::Rect> = Vec::with_capacity(n);
    let mut drag_pointer: Option<f32> = None;
    let mut drag_released = false;

    if n == 0 {
        ui.label(i18n::tr("postfx.empty"));
    }
    for i in 0..n {
        let row = ui.push_id(i, |ui| {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    let handle = ui
                        .add(egui::Label::new("☰").sense(egui::Sense::drag()))
                        .on_hover_text(i18n::tr("postfx.drag-hint"));
                    if handle.drag_started() {
                        panel.drag = Some(RowDrag { from: i, to: i });
                    }
                    if handle.dragged() {
                        drag_pointer = handle.interact_pointer_pos().map(|p| p.y);
                    }
                    if handle.drag_stopped() {
                        drag_released = true;
                    }

                    let mut on = chain.enabled[i];
                    let label = effect_label(chain.effects[i].pass());
                    if ui.checkbox(&mut on, label).changed() {
                        changed |= chain.set_enabled(i, on);
                    }
                    if ui
                        .add_enabled(i > 0, egui::Button::new("▲"))
                        .on_hover_text(i18n::tr("postfx.move-up"))
                        .clicked()
                    {
                        edit = Some(RowEdit::Move { from: i, to: i - 1 });
                    }
                    if ui
                        .add_enabled(i + 1 < n, egui::Button::new("▼"))
                        .on_hover_text(i18n::tr("postfx.move-down"))
                        .clicked()
                    {
                        edit = Some(RowEdit::Move { from: i, to: i + 1 });
                    }
                    if ui
                        .small_button("✕")
                        .on_hover_text(i18n::tr("postfx.remove"))
                        .clicked()
                    {
                        edit = Some(RowEdit::Remove(i));
                    }
                });
                // Edit a copy; write back clamped (the model's clamp point).
                let mut effect = chain.effects[i];
                if param_rows(ui, &mut effect) {
                    chain.effects[i] = effect.clamped();
                    changed = true;
                }
            })
            .response
        });
        rows.push(row.inner.rect);
    }

    // The drag: retarget while held (with a drop-line indicator), move
    // on release.
    let centers: Vec<f32> = rows.iter().map(|r| r.center().y).collect();
    if let (Some(drag), Some(y)) = (panel.drag.as_mut(), drag_pointer) {
        drag.to = drop_index(&centers, drag.from, y);
    }
    if let Some(drag) = panel.drag {
        if let Some(target) = rows.get(drag.to) {
            if drag.to != drag.from {
                let y = if drag.to > drag.from {
                    target.bottom()
                } else {
                    target.top()
                };
                let stroke = egui::Stroke::new(2.0, ui.visuals().selection.stroke.color);
                ui.painter().hline(target.x_range(), y, stroke);
            }
        }
    }
    if drag_released {
        if let Some(drag) = panel.drag.take() {
            if drag.from != drag.to {
                edit = Some(RowEdit::Move {
                    from: drag.from,
                    to: drag.to,
                });
            }
        }
    }

    match edit {
        Some(RowEdit::Move { from, to }) => changed |= chain.reorder(from, to),
        Some(RowEdit::Remove(i)) => {
            changed |= chain.remove(i);
            panel.drag = None;
        }
        None => {}
    }

    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("postfx-add-kind")
            .selected_text(effect_label(panel.add_kind))
            .show_ui(ui, |ui| {
                for pass in PostPass::ALL {
                    ui.selectable_value(&mut panel.add_kind, pass, effect_label(pass));
                }
            });
        if ui.button(i18n::tr("postfx.add")).clicked() {
            chain.push(PostEffect::default_for(panel.add_kind));
            changed = true;
        }
    });

    ui.label(summary_label(chain));
    // After this frame's edits, so a toggle reads right immediately.
    ui.weak(status_label(PostFxStatus::of(chain, mode)));
    ui.weak(i18n::tr("postfx.session-only"));
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_index_lands_where_the_pointer_is() {
        // Four rows, centers 10/30/50/70.
        let centers = [10.0, 30.0, 50.0, 70.0];
        // Row 0 dragged below everything → last index (the move the old
        // reorder could not express).
        assert_eq!(drop_index(&centers, 0, 90.0), 3);
        // Row 3 dragged above everything → first.
        assert_eq!(drop_index(&centers, 3, 0.0), 0);
        // Row 0 dragged between rows 1 and 2 → index 1.
        assert_eq!(drop_index(&centers, 0, 40.0), 1);
        // Row 2 nudged but still between rows 1 and 3 → stays at 2.
        assert_eq!(drop_index(&centers, 2, 45.0), 2);
    }

    #[test]
    fn drop_index_drives_reorder_end_to_end() {
        let mut chain = PostChain::new();
        for pass in [PostPass::Sharpen, PostPass::Bloom, PostPass::Vignette] {
            chain.push(PostEffect::default_for(pass));
        }
        let to = drop_index(&[10.0, 30.0, 50.0], 0, 99.0);
        assert!(chain.reorder(0, to));
        assert_eq!(chain.effects[2].pass(), PostPass::Sharpen);
    }

    #[test]
    fn summary_counts_enabled_and_total() {
        let mut chain = PostChain::new();
        chain.push(PostEffect::default_for(PostPass::Bloom));
        chain.push(PostEffect::default_for(PostPass::ToneMap));
        assert!(chain.set_enabled(0, false));
        let label = summary_label(&chain);
        assert!(label.contains("1 of 2"), "{label}");
    }

    #[test]
    fn status_line_flips_with_the_route() {
        let mut chain = PostChain::new();
        let idle = status_label(PostFxStatus::of(&chain, ViewMode::Lit));
        chain.push(PostEffect::default_for(PostPass::Vignette));
        let live = status_label(PostFxStatus::of(&chain, ViewMode::Lit));
        let data = ViewMode::Solo(umber_gpu::SoloChannel::Roughness);
        let bypassed = status_label(PostFxStatus::of(&chain, data));
        for label in [&idle, &live, &bypassed] {
            // Not the raw key: the en-US catalog defines each.
            assert!(!label.starts_with("postfx."), "{label}");
        }
        assert_ne!(idle, live);
        assert_ne!(live, bypassed);
        assert_ne!(idle, bypassed);
    }

    #[test]
    fn every_effect_and_mode_has_a_label() {
        // Not the raw key: the en-US catalog defines each.
        for pass in PostPass::ALL {
            let label = effect_label(pass);
            assert!(!label.starts_with("postfx."), "{pass:?} → {label}");
        }
        for mode in ToneMapMode::ALL {
            let label = mode_label(mode);
            assert!(!label.starts_with("postfx."), "{mode:?} → {label}");
        }
    }
}
