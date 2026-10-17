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
//! a data view mode — see `umber_gpu::postfx_viewport`). The chain
//! persists in the `.umber` project through the `postfx` settings carry
//! (`umber_core::project::PostFxCarry`, converted at the save/load seam
//! by `carry_from_chain`/`chain_from_carry` below); only the panel's own
//! state ([`PostFxPanel`]: the add pick, the drag in flight) stays
//! session-only.

use crate::i18n;
use egui::Ui;
use fluent::FluentValue;
use umber_core::project::{PostFxCarry, PostFxEffect, PostFxToneMapMode};
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

/// Builds the project carry from the live chain (the save path):
/// params ride as-is — the model is already the clamp point (every panel
/// edit writes back through `PostEffect::clamped`), so a save is exact.
#[must_use]
pub fn carry_from_chain(chain: &PostChain) -> PostFxCarry {
    PostFxCarry {
        effects: chain
            .effects
            .iter()
            .map(|effect| match *effect {
                PostEffect::Dof { focus, aperture } => PostFxEffect::Dof { focus, aperture },
                PostEffect::Bloom {
                    threshold,
                    intensity,
                    radius,
                } => PostFxEffect::Bloom {
                    threshold,
                    intensity,
                    radius,
                },
                PostEffect::Vignette { strength, softness } => {
                    PostFxEffect::Vignette { strength, softness }
                }
                PostEffect::Sharpen { amount, radius } => PostFxEffect::Sharpen { amount, radius },
                PostEffect::ToneMap { mode, exposure } => PostFxEffect::ToneMap {
                    mode: match mode {
                        ToneMapMode::Aces => PostFxToneMapMode::Aces,
                        ToneMapMode::Filmic => PostFxToneMapMode::Filmic,
                        ToneMapMode::Reinhard => PostFxToneMapMode::Reinhard,
                    },
                    exposure,
                },
            })
            .collect(),
        enabled: chain.enabled.clone(),
    }
}

/// Rebuilds a chain from a loaded carry (the load path): params clamp
/// through `PostEffect::clamped` (hand-edited files can carry
/// out-of-range values — the same rule as the display `sanitized`
/// restore), and a short `enabled` vec reads as enabled (absent ⇒
/// enabled). The matches are exhaustive on both enums: a variant added
/// to either the carry or the model breaks the build here — the
/// mirror-drift guard (see `umber_core::project::PostFxToneMapMode`).
#[must_use]
pub fn chain_from_carry(carry: &PostFxCarry) -> PostChain {
    let mut chain = PostChain::new();
    for (index, effect) in carry.effects.iter().enumerate() {
        let mapped = match *effect {
            PostFxEffect::Dof { focus, aperture } => PostEffect::Dof { focus, aperture },
            PostFxEffect::Bloom {
                threshold,
                intensity,
                radius,
            } => PostEffect::Bloom {
                threshold,
                intensity,
                radius,
            },
            PostFxEffect::Vignette { strength, softness } => {
                PostEffect::Vignette { strength, softness }
            }
            PostFxEffect::Sharpen { amount, radius } => PostEffect::Sharpen { amount, radius },
            PostFxEffect::ToneMap { mode, exposure } => PostEffect::ToneMap {
                mode: match mode {
                    PostFxToneMapMode::Aces => ToneMapMode::Aces,
                    PostFxToneMapMode::Filmic => ToneMapMode::Filmic,
                    PostFxToneMapMode::Reinhard => ToneMapMode::Reinhard,
                },
                exposure,
            },
        };
        // `push` clamps and appends enabled; the index is the one just
        // pushed, so the flag write cannot miss.
        chain.push(mapped);
        let on = carry.enabled.get(index).copied().unwrap_or(true);
        chain.set_enabled(index, on);
    }
    chain
}

/// One labelled numeric row in the house grammar (muted label + a
/// [`umber_gui::ValueField`]), the same shape the brush panel uses.
fn field_row<N: egui::emath::Numeric>(
    ui: &mut Ui,
    label: &str,
    field: umber_gui::ValueField<'_, N>,
) -> bool {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).color(umber_gui::muted()));
        ui.add(field).changed()
    })
    .inner
}

/// One effect's param rows; true when a value changed. Ranges mirror
/// [`PostEffect::clamped`].
///
/// DoF focus uses the field's log mode (2026-10-15): its 0.01..=100 range
/// spans four decades, and a linear drag-scrub would make the near range
/// — where focus lives on a small mesh — unreachable.
fn param_rows(ui: &mut Ui, effect: &mut PostEffect) -> bool {
    use umber_gui::ValueField;
    let mut changed = false;
    match effect {
        PostEffect::Dof { focus, aperture } => {
            changed |= field_row(
                ui,
                &i18n::tr("postfx.focus"),
                ValueField::new(focus)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(0.01..=100.0)
                    .logarithmic(true)
                    .step(0.0)
                    .decimals(3),
            );
            changed |= field_row(
                ui,
                &i18n::tr("postfx.aperture"),
                ValueField::new(aperture)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(0.0..=1.0),
            );
        }
        PostEffect::Bloom {
            threshold,
            intensity,
            radius,
        } => {
            changed |= field_row(
                ui,
                &i18n::tr("postfx.threshold"),
                ValueField::new(threshold)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(0.0..=4.0)
                    .step(0.05),
            );
            changed |= field_row(
                ui,
                &i18n::tr("postfx.intensity"),
                ValueField::new(intensity)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(0.0..=4.0)
                    .step(0.05),
            );
            changed |= field_row(
                ui,
                &i18n::tr("postfx.radius"),
                ValueField::new(radius)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(1..=64)
                    .unit("px"),
            );
        }
        PostEffect::Vignette { strength, softness } => {
            changed |= field_row(
                ui,
                &i18n::tr("postfx.strength"),
                ValueField::new(strength)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(0.0..=1.0),
            );
            changed |= field_row(
                ui,
                &i18n::tr("postfx.softness"),
                ValueField::new(softness)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(0.0..=1.0),
            );
        }
        PostEffect::Sharpen { amount, radius } => {
            changed |= field_row(
                ui,
                &i18n::tr("postfx.amount"),
                ValueField::new(amount)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(0.0..=4.0)
                    .step(0.05),
            );
            changed |= field_row(
                ui,
                &i18n::tr("postfx.radius"),
                ValueField::new(radius)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(1..=8)
                    .unit("px"),
            );
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
            changed |= field_row(
                ui,
                &i18n::tr("postfx.exposure"),
                ValueField::new(exposure)
                    .context_menu(crate::i18n::prop_menu_labels())
                    .range(-4.0..=4.0)
                    .step(0.1)
                    .unit("EV"),
            );
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
                    // g7-accesskit-sweep (the painter-drawn half): the
                    // glyph handle names its action (English-only at the
                    // widget layer; the localized hint is the tooltip).
                    handle.widget_info(|| {
                        egui::WidgetInfo::labeled(
                            egui::WidgetType::Button,
                            true,
                            i18n::tr("postfx.drag-hint"),
                        )
                    });
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
    fn carry_round_trip_is_exact() {
        // One effect of each kind (the bloom row disabled): across the
        // save/load seam the chain comes back equal — order, params and
        // flags (in-memory; the byte-exact file leg lives in
        // `umber_core::project`'s `postfx_carry_round_trip_byte_identical`).
        let mut chain = PostChain::new();
        for pass in PostPass::ALL {
            chain.push(PostEffect::default_for(pass));
        }
        assert!(chain.set_enabled(1, false));
        let carry = carry_from_chain(&chain);
        assert_eq!(carry.effects.len(), PostPass::ALL.len());
        assert_eq!(carry.enabled, vec![true, false, true, true, true]);
        assert_eq!(chain_from_carry(&carry), chain);
    }

    #[test]
    fn chain_from_carry_clamps_and_defaults_missing_flags() {
        // Hand-edited values clamp through the model (the pins mirror
        // `umber_gpu::postfx`'s `params_clamp_at_construct`); a missing
        // flag reads as enabled (absent ⇒ enabled).
        let carry = PostFxCarry {
            effects: vec![PostFxEffect::Bloom {
                threshold: 99.0,
                intensity: -4.0,
                radius: 1000,
            }],
            enabled: vec![],
        };
        let chain = chain_from_carry(&carry);
        assert_eq!(
            chain.effects,
            vec![PostEffect::Bloom {
                threshold: 4.0,
                intensity: 0.0,
                radius: 64,
            }]
        );
        assert_eq!(chain.enabled, vec![true]);
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
