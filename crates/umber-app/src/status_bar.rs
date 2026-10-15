//! The bottom status bar (GUI roadmap Phase 0b,
//! docs/specs/gui-audit-and-roadmap.md Part 3): cursor UV | UDIM tile |
//! pen source | fps.
//!
//! The format functions are pure (no `tr()`, no egui) and pinned by the
//! tests below. [`show`] only lays them out next to localized labels.
//!
//! # Judgment calls
//!
//! - **Cursor UV comes from the 2D UV view's hover, not the 3D viewport.**
//!   The UV view maps the pointer to UV with one division
//!   (`uv_view::uv_from_pointer`). A 3D hover readout would need a
//!   per-move ray cast, and `umber_mesh::ray_intersect` is a brute-force
//!   loop over every triangle (no BVH). Per-hover cost on a production
//!   mesh is not v1 material. The 3D readout lands with the
//!   paint-cursor layer (roadmap Phase 2 item 4), which needs the same
//!   hover pick anyway.
//! - **The UV is absolute (UDIM space).** The UV view draws one tile's
//!   0..1 square: the paint session's active tile (`PaintState::
//!   active_tile`). The readout adds that tile's grid origin, so it reads
//!   the same coordinate the mesh stores (tile 1012, local (0.5, 0.25) →
//!   (1.500, 1.250)).
//! - **The tile is the active paint tile**, shown even when the pointer is
//!   off the UV view. Without a paint session there is no tile, and it
//!   reads "—".
//! - **fps is `1 / stable_dt`.** egui repaints reactively (on input or a
//!   requested repaint), so this is the rate while frames are being
//!   drawn, not a free-running counter. The bar never requests continuous
//!   repaints to make the number "live": that would burn a core while the
//!   app idles.
//! - **One frame stale by construction.** The bar is a bottom panel, so it
//!   draws before the dock (egui lays panels out first). It shows the hover
//!   the UV view recorded last frame. main.rs requests one extra repaint
//!   when the hover changed, so the bar catches up within a frame.
//! - **GPU memory is omitted.** wgpu has no portable VRAM query. The
//!   allocator report is backend-specific and not exposed through
//!   `umber-gpu`, and this crate never names wgpu types. Summing our own
//!   texture sizes would be a lower bound passed off as a reading. It
//!   stays off the bar until `umber-gpu` can report a real number.

use egui::{RichText, Ui};
use umber_gui::theme;

use crate::i18n::tr;

/// What the bar shows this frame.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StatusSnapshot {
    /// The absolute (UDIM-space) UV under the cursor, if it is over the
    /// UV view's square.
    pub cursor_uv: Option<[f32; 2]>,
    /// The active paint tile.
    pub tile: Option<u16>,
    /// The tablet backend behind the latest pen event, if any
    /// (`stylus_input::StylusPoller::pen_source` — XI2 on Linux, `None`
    /// on Touch/mouse). The token itself (`x11-xinput2`) is a protocol
    /// id, not a translated string; only the `Pen` label is i18n'd.
    pub pen: Option<stylus::InputSource>,
    /// `egui::InputState::stable_dt`, seconds.
    pub frame_dt: f32,
    /// Which mouse/key hints the left of the bar shows.
    pub hints: HintContext,
    /// The navigation preset: its orbit / pan / zoom chords follow the
    /// paint hint in the [`HintContext::Paint`] context.
    pub nav: crate::navigation::NavPreset,
}

/// The viewport's mouse/key hint set for the left of the bar (GUI plan G1
/// `g1-status-hints`; Blender's status-bar tool hints, Painter's keyboard
/// helper). It changes with the held modifier, so holding Ctrl shows what
/// Ctrl does: the bar teaches the bindings without a cheat sheet.
///
/// Every hint states a binding the viewport ACTUALLY has: Ctrl/Cmd+LMB
/// paints (`viewport.rs::ui`), and the navigation chords come from the
/// active [`crate::navigation::NavPreset`] (`handle_input` routes through
/// the same preset). A hint for a binding that does not exist is a bug.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HintContext {
    /// No mesh: the only action is loading one.
    #[default]
    NoMesh,
    /// A mesh is loaded and a paint tool is active; no modifier held.
    Paint,
    /// Ctrl / Cmd held over the viewport: the paint chord.
    PaintModifier,
    /// Shift held: the pan chord.
    PanModifier,
    /// The eyedropper is armed.
    Pick,
    /// The ID-mask select is armed.
    IdMask,
}

impl HintContext {
    /// The hint context for the shell's state this frame.
    pub fn of(has_mesh: bool, tool: crate::toolbar::ToolKind, mods: egui::Modifiers) -> Self {
        use crate::toolbar::ToolKind;
        if !has_mesh {
            return HintContext::NoMesh;
        }
        match tool {
            ToolKind::Pick => HintContext::Pick,
            ToolKind::IdMask => HintContext::IdMask,
            _ if mods.ctrl || mods.command => HintContext::PaintModifier,
            _ if mods.shift => HintContext::PanModifier,
            _ => HintContext::Paint,
        }
    }

    /// The hints as `(input key, action key)` i18n pairs, in display order.
    pub fn hints(self) -> &'static [(&'static str, &'static str)] {
        match self {
            HintContext::NoMesh => &[("hint.drop", "hint.load-mesh")],
            // The navigation chords follow from the preset (`show`).
            HintContext::Paint => &[
                ("hint.ctrl-lmb", "hint.paint"),
                ("hint.brackets", "hint.size"),
            ],
            HintContext::PaintModifier => &[
                ("hint.lmb-drag", "hint.paint-stroke"),
                ("hint.rmb-drag", "hint.resize"),
                ("hint.ctrl-z", "hint.undo"),
            ],
            // Shift alone binds nothing the bar should claim under the
            // Painter / Blender presets; the preset's own chords (which
            // include Shift+LMB pan under the classic preset) follow.
            HintContext::PanModifier => &[
                ("hint.lmb", "hint.straight-line"),
                ("hint.ctrl-shift", "hint.snap-angle"),
            ],
            HintContext::Pick => &[("hint.lmb", "hint.sample-color")],
            HintContext::IdMask => &[("hint.lmb", "hint.select-region")],
        }
    }
}

/// The placeholder for a value that has no reading.
pub const NO_VALUE: &str = "—";

/// The grid origin `(u, v)` of a UDIM tile (`1001 + u + 10 v`). Tiles
/// below 1001 clamp to the first tile, like `umber_mesh::tile_of_uv`.
pub fn tile_origin(tile: u16) -> [f32; 2] {
    let index = tile.saturating_sub(umber_mesh::FIRST_TILE);
    [f32::from(index % 10), f32::from(index / 10)]
}

/// The absolute UV of a tile-local UV in `tile`.
pub fn absolute_uv(tile: u16, local: [f32; 2]) -> [f32; 2] {
    let [u0, v0] = tile_origin(tile);
    [u0 + local[0], v0 + local[1]]
}

/// `"u, v"` at 3 decimals, or [`NO_VALUE`] for no reading or a
/// non-finite component. Three decimals resolve a texel at 1K, and the
/// fixed width keeps the bar from jittering.
pub fn format_uv(uv: Option<[f32; 2]>) -> String {
    match uv {
        Some([u, v]) if u.is_finite() && v.is_finite() => format!("{u:.3}, {v:.3}"),
        _ => NO_VALUE.to_owned(),
    }
}

/// The tile number, or [`NO_VALUE`].
pub fn format_tile(tile: Option<u16>) -> String {
    tile.map_or_else(|| NO_VALUE.to_owned(), |t| t.to_string())
}

/// The pen backend token (`x11-xinput2`), or [`NO_VALUE`] with no pen
/// event yet this session.
pub fn format_pen(source: Option<stylus::InputSource>) -> String {
    source.map_or_else(|| NO_VALUE.to_owned(), |s| s.to_string())
}

/// Frames per second from a frame delta. `None` for a zero, negative or
/// non-finite delta (the first frame, or a clock hiccup).
pub fn fps_from_dt(dt: f32) -> Option<f32> {
    if !dt.is_finite() || dt <= 0.0 {
        return None;
    }
    Some(1.0 / dt)
}

/// Whole frames per second, or [`NO_VALUE`].
pub fn format_fps(dt: f32) -> String {
    fps_from_dt(dt).map_or_else(|| NO_VALUE.to_owned(), |fps| format!("{fps:.0}"))
}

/// Draws the bar into `ui`.
pub fn show(ui: &mut Ui, status: &StatusSnapshot) {
    ui.set_style(theme::umber_style(theme::Density::Compact));
    let label = |text: String| {
        RichText::new(text)
            .size(theme::TEXT_STATUS)
            .color(theme::muted())
    };
    let value = |text: String| {
        RichText::new(text)
            .size(theme::TEXT_STATUS)
            .monospace()
            .color(theme::text())
    };
    let key = |text: String| {
        RichText::new(text)
            .size(theme::TEXT_STATUS)
            .strong()
            .color(theme::text())
    };
    ui.horizontal(|ui| {
        let nav = match status.hints {
            HintContext::NoMesh | HintContext::PaintModifier => &[][..],
            _ => status.nav.hints(),
        };
        for (input, action) in status.hints.hints().iter().chain(nav) {
            ui.label(key(tr(input)));
            ui.label(label(tr(action)));
            ui.add_space(6.0);
        }
        ui.separator();
        ui.label(label(tr("status.uv")));
        ui.label(value(format_uv(status.cursor_uv)));
        ui.separator();
        ui.label(label(tr("status.tile")));
        ui.label(value(format_tile(status.tile)));
        ui.separator();
        ui.label(label(tr("status.pen")));
        ui.label(value(format_pen(status.pen)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(label(tr("status.fps")));
            ui.label(value(format_fps(status.frame_dt)));
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_origins_follow_the_udim_formula() {
        assert_eq!(tile_origin(1001), [0.0, 0.0]);
        assert_eq!(tile_origin(1002), [1.0, 0.0]);
        assert_eq!(tile_origin(1010), [9.0, 0.0]);
        assert_eq!(tile_origin(1011), [0.0, 1.0]);
        assert_eq!(tile_origin(1012), [1.0, 1.0]);
        assert_eq!(tile_origin(1100), [9.0, 9.0]);
        // Below the grid: clamps to the first tile.
        assert_eq!(tile_origin(0), [0.0, 0.0]);
    }

    #[test]
    fn absolute_uv_round_trips_through_tile_of_uv() {
        assert_eq!(absolute_uv(1012, [0.5, 0.25]), [1.5, 1.25]);
        assert_eq!(absolute_uv(1001, [0.5, 0.25]), [0.5, 0.25]);
        for tile in [1001u16, 1002, 1010, 1011, 1055, 1100] {
            let uv = absolute_uv(tile, [0.5, 0.5]);
            assert_eq!(umber_mesh::tile_of_uv(uv), tile, "tile {tile}");
        }
    }

    #[test]
    fn uv_formats_at_three_decimals() {
        assert_eq!(format_uv(Some([0.5, 0.25])), "0.500, 0.250");
        assert_eq!(format_uv(Some([1.5, 1.25])), "1.500, 1.250");
        // Rounds to nearest at the third decimal, carrying into the units.
        assert_eq!(format_uv(Some([0.12345, 0.9996])), "0.123, 1.000");
        assert_eq!(format_uv(None), "—");
        assert_eq!(format_uv(Some([f32::NAN, 0.5])), "—");
        assert_eq!(format_uv(Some([0.5, f32::INFINITY])), "—");
    }

    #[test]
    fn tile_formats_or_dashes() {
        assert_eq!(format_tile(Some(1001)), "1001");
        assert_eq!(format_tile(Some(1100)), "1100");
        assert_eq!(format_tile(None), "—");
    }

    #[test]
    fn pen_formats_the_backend_token_or_dashes() {
        // The token is the protocol id (untranslated); the label is.
        assert_eq!(
            format_pen(Some(stylus::InputSource::X11Xinput2)),
            "x11-xinput2"
        );
        assert_eq!(
            format_pen(Some(stylus::InputSource::MouseFallback)),
            "mouse-fallback"
        );
        assert_eq!(format_pen(None), "—");
    }

    #[test]
    fn fps_is_the_reciprocal_of_a_positive_finite_dt() {
        assert_eq!(fps_from_dt(0.5), Some(2.0));
        assert_eq!(fps_from_dt(0.25), Some(4.0));
        assert_eq!(fps_from_dt(0.0), None);
        assert_eq!(fps_from_dt(-0.016), None);
        assert_eq!(fps_from_dt(f32::NAN), None);
        assert_eq!(fps_from_dt(f32::INFINITY), None);
    }

    #[test]
    fn hint_context_follows_tool_and_modifier() {
        use crate::toolbar::ToolKind;
        let none = egui::Modifiers::NONE;
        let ctrl = egui::Modifiers {
            ctrl: true,
            command: true,
            ..none
        };
        let shift = egui::Modifiers {
            shift: true,
            ..none
        };
        assert_eq!(
            HintContext::of(false, ToolKind::Paint, ctrl),
            HintContext::NoMesh
        );
        assert_eq!(
            HintContext::of(true, ToolKind::Paint, none),
            HintContext::Paint
        );
        assert_eq!(
            HintContext::of(true, ToolKind::Smudge, ctrl),
            HintContext::PaintModifier
        );
        assert_eq!(
            HintContext::of(true, ToolKind::Paint, shift),
            HintContext::PanModifier
        );
        // The strip tools win over modifiers: they never paint.
        assert_eq!(
            HintContext::of(true, ToolKind::Pick, ctrl),
            HintContext::Pick
        );
        assert_eq!(
            HintContext::of(true, ToolKind::IdMask, none),
            HintContext::IdMask
        );
    }

    #[test]
    fn every_hint_key_is_in_both_catalogs() {
        let en = include_str!("../../../assets/i18n/en-US.ftl");
        let pt = include_str!("../../../assets/i18n/pt-BR.ftl");
        for ctx in [
            HintContext::NoMesh,
            HintContext::Paint,
            HintContext::PaintModifier,
            HintContext::PanModifier,
            HintContext::Pick,
            HintContext::IdMask,
        ] {
            assert!(!ctx.hints().is_empty(), "{ctx:?}");
            for (input, action) in ctx.hints() {
                for k in [input, action] {
                    let attr = format!(".{} =", k.trim_start_matches("hint."));
                    assert!(en.contains(&attr), "en-US missing {k}");
                    assert!(pt.contains(&attr), "pt-BR missing {k}");
                }
            }
        }
    }

    #[test]
    fn fps_formats_whole_frames() {
        assert_eq!(format_fps(0.5), "2");
        assert_eq!(format_fps(0.25), "4");
        // f32(1/60) and f32(1/144) aren't exact; their reciprocals land
        // within 1e-4 of the whole number, which `.0` rounds onto.
        assert_eq!(format_fps(1.0 / 60.0), "60");
        assert_eq!(format_fps(1.0 / 144.0), "144");
        assert_eq!(format_fps(0.3), "3");
        assert_eq!(format_fps(0.0), "—");
    }
}
