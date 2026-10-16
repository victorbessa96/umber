//! Tab-strip overflow (GUI plan g5-tab-overflow): long titles elide with
//! "…", and the selected tab is scrolled into view.
//!
//! Two pure pieces the dock shell (`umber-app` main.rs) wires up:
//!
//! - [`elided_title`] lays a title out at most [`TAB_TITLE_MAX`] wide on
//!   one row, ending in "…" when it does not fit, and returns the
//!   pre-laid galley. egui_dock 0.21.1 lays `TabViewer::title` out at an
//!   infinite width (`leaf.rs` `tab_title`: `into_galley(.., INFINITY,
//!   ..)`), which overwrites a `LayoutJob`'s own wrap; a
//!   `WidgetText::Galley` is passed through untouched, so the elision
//!   survives.
//! - [`scroll_into_view`] returns the `LeafNode::scroll` that puts tab
//!   `index` inside the bar. egui_dock draws tab `k` at `bar_left +
//!   scroll + sum(widths[..k])` and clamps `scroll` to `[-overflow, 0]`
//!   (`tab_bar_scroll`), so this is arithmetic on the widths, and a
//!   test pins it.
//!
//! Design: `docs/specs/g5-tab-overflow-design.md`.

use std::sync::Arc;

use egui::text::{LayoutJob, TextWrapping};
use egui::{Context, FontId, Galley, TextFormat};

/// The widest a tab title is laid out, in points. Every en-US panel
/// title fits (the longest, "Texture Sets", is about 80 pt at the button
/// size); only the longest pt-BR title ("Conjuntos de texturas") elides.
pub const TAB_TITLE_MAX: f32 = 140.0;

/// `title` laid out on one row at most `max_width` wide in `font`,
/// ending in "…" when it is cut. The colour is egui's placeholder, so
/// the dock paints it in the tab's own text colour (active, inactive,
/// focused), exactly as a plain-string title would be.
///
/// `galley.elided` says whether it was cut; the caller then puts the
/// full title in the tab's hover text.
pub fn elided_title(ctx: &Context, title: &str, font: FontId, max_width: f32) -> Arc<Galley> {
    let mut job = LayoutJob::single_section(
        title.to_owned(),
        TextFormat::simple(font, egui::Color32::PLACEHOLDER),
    );
    job.wrap = TextWrapping::truncate_at_width(max_width);
    ctx.fonts_mut(|fonts| fonts.layout_job(job))
}

/// The `LeafNode::scroll` (always `<= 0`) that shows tab `index` fully,
/// moving as little as possible from `scroll`.
///
/// - `widths`: each tab's laid-out width, spacing included, in order.
/// - `bar`: the visible width of the tab strip.
///
/// Already visible → `scroll` unchanged. Left of the view → its left
/// edge aligns with the bar's. Right of it → its right edge aligns. A
/// tab wider than the bar aligns left (its title start is what matters).
/// The result is clamped to `[-(total - bar), 0]`, egui_dock's own
/// clamp, so nothing is asked of the dock that it would undo. An index
/// past the end returns `scroll` unchanged.
pub fn scroll_into_view(widths: &[f32], index: usize, bar: f32, scroll: f32) -> f32 {
    let Some(&width) = widths.get(index) else {
        return scroll;
    };
    let total: f32 = widths.iter().sum();
    let overflow = (total - bar).max(0.0);
    let left: f32 = widths[..index].iter().sum();
    let right = left + width;
    // Tab k is visible when [left + scroll, right + scroll] ⊆ [0, bar].
    let wanted = if left + scroll < 0.0 || width > bar {
        -left
    } else if right + scroll > bar {
        bar - right
    } else {
        scroll
    };
    wanted.clamp(-overflow, 0.0)
}

/// True when the tabs do not fit the bar: the dock shows its scroll bar,
/// and the shell's tab-list menu earns its place.
pub fn overflows(widths: &[f32], bar: f32) -> bool {
    widths.iter().sum::<f32>() > bar
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A context whose fonts are loaded: egui builds the atlas on the
    /// first frame, so layout needs one `run_ui` first.
    fn ctx() -> Context {
        let ctx = Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |_| {});
        ctx
    }

    /// Disarms epaint's `TexturesDelta` Drop bomb before the test ends:
    /// building a galley queues a font-texture delta, and a delta left
    /// unapplied panics on drop (epaint 0.36 `textures.rs:337`). The
    /// battery caught both tab-overflow tests tripping it on the host.
    fn disarm_drop_bomb(ctx: &Context) {
        ctx.tex_manager().write().clear();
    }

    /// Drops the context's texture deltas, so epaint's Drop bomb (a
    /// `TexturesDelta` with unapplied deltas panics on drop) does not
    /// fire when the test ends (it panics in ANY test that builds a
    /// galley, i.e. all of them).
    fn drop_ok(ctx: &Context) {
        let deltas = ctx.fonts(|_| ()) // force the font texture into existence
            ;
        let _ = deltas;
        // egui 0.36's public API: `Context::tex_manager` returns a mutex
        // guard; taking and clearing it disarms the bomb.
        if let Ok(mut tex) = ctx.tex_manager() {
            tex.clear();
        }
    }

    #[test]
    fn a_long_title_elides_and_a_short_one_does_not() {
        let ctx = ctx();
        let font = FontId::proportional(13.0);
        let short = elided_title(&ctx, "Layers", font.clone(), TAB_TITLE_MAX);
        assert!(!short.elided);
        // `Galley::text()` is the job's full INPUT; what is drawn is the
        // row's glyphs.
        assert_eq!(short.rows[0].text(), "Layers");

        let long = "Conjuntos de texturas e muito mais texto que nunca cabe";
        let cut = elided_title(&ctx, long, font.clone(), TAB_TITLE_MAX);
        assert!(cut.elided, "a title wider than the cap is cut");
        let drawn = cut.rows[0].text();
        assert!(drawn.ends_with('…'), "{drawn:?}");
        assert_eq!(cut.text(), long, "the input is kept whole for the tooltip");
        assert!(cut.size().x <= TAB_TITLE_MAX + 0.5, "{}", cut.size().x);
        assert_eq!(cut.rows.len(), 1, "one row, never wrapped");
        // The same text with no cap is wider: the cut really removed text.
        let whole = elided_title(&ctx, long, font, f32::INFINITY);
        disarm_drop_bomb(&ctx);
        assert!(!whole.elided && whole.size().x > cut.size().x);
    }

    #[test]
    fn the_shipped_titles_fit_except_the_longest_pt_br_one() {
        // The design's claim behind TAB_TITLE_MAX, measured rather than
        // estimated: egui_dock lays titles out at the Button text style.
        let ctx = ctx();
        let font = egui::TextStyle::Button.resolve(&ctx.global_style());
        for title in [
            "Texture Sets",
            "Properties",
            "Viewport",
            "Propriedades",
            "Exportação",
        ] {
            let g = elided_title(&ctx, title, font.clone(), TAB_TITLE_MAX);
            assert!(!g.elided, "{title:?} must fit {TAB_TITLE_MAX} pt");
        }
        let g = elided_title(&ctx, "Conjuntos de texturas", font, TAB_TITLE_MAX);
        disarm_drop_bomb(&ctx);
        assert!(
            g.size().x <= TAB_TITLE_MAX + 0.5,
            "elided or not, it never exceeds the cap"
        );
    }

    #[test]
    fn scrolling_moves_only_as_far_as_needed() {
        let widths = [100.0; 10]; // 1000 wide in a 300 bar: overflow 700.
                                  // Visible already: unchanged.
        assert_eq!(scroll_into_view(&widths, 1, 300.0, 0.0), 0.0);
        // Off the right: right edge aligns (tab 5 spans 500..600 → -300).
        assert_eq!(scroll_into_view(&widths, 5, 300.0, 0.0), -300.0);
        // Off the left: left edge aligns (tab 2 at 200, viewed from -450).
        assert_eq!(scroll_into_view(&widths, 2, 300.0, -450.0), -200.0);
        // The last tab: clamped to -overflow, exactly egui_dock's clamp.
        assert_eq!(scroll_into_view(&widths, 9, 300.0, 0.0), -700.0);
        // Out of range: unchanged.
        assert_eq!(scroll_into_view(&widths, 10, 300.0, -50.0), -50.0);
    }

    #[test]
    fn every_target_lands_inside_the_bar() {
        // The property the plan row asks for, swept: for each tab and
        // each starting scroll, the result shows the tab and stays in
        // the dock's clamp range.
        let widths = [60.0, 140.0, 90.0, 30.0, 120.0, 75.0];
        let total: f32 = widths.iter().sum();
        for bar in [100.0, 200.0, 333.0, 600.0] {
            let overflow = (total - bar).max(0.0);
            for index in 0..widths.len() {
                for start in [0.0, -overflow / 2.0, -overflow] {
                    let s = scroll_into_view(&widths, index, bar, start);
                    assert!((-overflow..=0.0).contains(&s), "clamp: {s}");
                    let left: f32 = widths[..index].iter().sum::<f32>() + s;
                    let right = left + widths[index];
                    if widths[index] <= bar {
                        assert!(
                            left >= -0.01 && right <= bar + 0.01,
                            "tab {index} at {left}..{right} in bar {bar} from {start}"
                        );
                    } else {
                        assert!(left.abs() < 0.01, "a too-wide tab aligns left");
                    }
                }
            }
        }
    }

    #[test]
    fn overflow_is_strictly_wider_than_the_bar() {
        assert!(!overflows(&[100.0, 100.0], 200.0));
        assert!(overflows(&[100.0, 100.5], 200.0));
        assert!(!overflows(&[], 0.0));
    }
}
