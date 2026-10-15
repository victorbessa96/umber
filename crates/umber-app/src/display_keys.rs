//! The display-mode keys (docs/specs/gui-plan-2026-10-15.md kill
//! `g6-display-mode-keys`, P:C4, P:B10): Painter's M / C / B.
//!
//! - **M** returns to the material (the Lit view).
//! - **C** / **Shift+C** step to the next / previous paint channel soloed
//!   (`SoloChannel::ALL` order). From a non-solo mode, C starts at the
//!   first channel and Shift+C at the last.
//! - **B** / **Shift+B** do the same over the baked mesh maps
//!   (`ViewMode::ALL_PREVIEW_KINDS` order).
//!
//! Both cycles wrap. The new mode's name is posted as a toast, so the
//! user sees what is soloed without opening the View menu (the plan's
//! "soloed name shown in the viewport").

use umber_gpu::{SoloChannel, ViewMode};
use umber_mesh::MeshMapKind;

/// Steps `list` from `current` (`None` = not in this cycle), forward or
/// back, wrapping. Entering from outside starts at the first item going
/// forward and the last going back.
fn step<T: Copy + PartialEq>(list: &[T], current: Option<T>, forward: bool) -> T {
    let n = list.len();
    match current.and_then(|c| list.iter().position(|x| *x == c)) {
        Some(i) if forward => list[(i + 1) % n],
        Some(i) => list[(i + n - 1) % n],
        None if forward => list[0],
        None => list[n - 1],
    }
}

/// C / Shift+C: the next or previous soloed channel.
pub fn cycle_channel(mode: ViewMode, forward: bool) -> ViewMode {
    let current = match mode {
        ViewMode::Solo(c) => Some(c),
        _ => None,
    };
    ViewMode::Solo(step(&SoloChannel::ALL, current, forward))
}

/// B / Shift+B: the next or previous previewed mesh map.
pub fn cycle_mesh_map(mode: ViewMode, forward: bool) -> ViewMode {
    let current = match mode {
        ViewMode::MeshMapPreview(k) => Some(k),
        _ => None,
    };
    ViewMode::MeshMapPreview(step(&ViewMode::ALL_PREVIEW_KINDS, current, forward))
}

/// The i18n key naming a soloed channel (the View menu's own keys).
pub fn channel_key(channel: SoloChannel) -> &'static str {
    match channel {
        SoloChannel::BaseColor => "viewmode.basecolor",
        SoloChannel::Emissive => "viewmode.emissive",
        SoloChannel::Roughness => "viewmode.roughness",
        SoloChannel::Metallic => "viewmode.metallic",
        SoloChannel::AmbientOcclusion => "viewmode.ao",
        SoloChannel::Height => "viewmode.height",
        SoloChannel::Normal => "viewmode.normal",
    }
}

/// The toast text for `mode`: the channel's localized name, or the mesh
/// map's file suffix (the same text the View menu's map combo shows).
pub fn mode_name(mode: ViewMode, tr: impl Fn(&str) -> String) -> String {
    match mode {
        ViewMode::Lit => tr("viewkeys.toast-material"),
        ViewMode::Unlit => tr("viewkeys.toast-unlit"),
        ViewMode::Solo(c) => format!("{} {}", tr("viewkeys.toast-channel"), tr(channel_key(c))),
        ViewMode::MeshMapPreview(k) => {
            format!("{} {}", tr("viewkeys.toast-map"), map_name(k))
        }
    }
}

fn map_name(kind: MeshMapKind) -> &'static str {
    kind.suffix()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_cycle_enters_steps_and_wraps() {
        let first = ViewMode::Solo(SoloChannel::ALL[0]);
        let last = ViewMode::Solo(*SoloChannel::ALL.last().unwrap());
        // From Lit: C enters at the first channel, Shift+C at the last.
        assert_eq!(cycle_channel(ViewMode::Lit, true), first);
        assert_eq!(cycle_channel(ViewMode::Lit, false), last);
        // Steps follow SoloChannel::ALL and wrap both ways.
        assert_eq!(
            cycle_channel(first, true),
            ViewMode::Solo(SoloChannel::ALL[1])
        );
        assert_eq!(cycle_channel(last, true), first);
        assert_eq!(cycle_channel(first, false), last);
        // A full forward lap returns to the start.
        let mut m = first;
        for _ in 0..SoloChannel::ALL.len() {
            m = cycle_channel(m, true);
        }
        assert_eq!(m, first);
        // From a mesh-map preview, C switches cycles (enters channels).
        let preview = ViewMode::MeshMapPreview(MeshMapKind::Curvature);
        assert_eq!(cycle_channel(preview, true), first);
    }

    #[test]
    fn mesh_map_cycle_enters_steps_and_wraps() {
        let kinds = ViewMode::ALL_PREVIEW_KINDS;
        let first = ViewMode::MeshMapPreview(kinds[0]);
        let last = ViewMode::MeshMapPreview(kinds[kinds.len() - 1]);
        assert_eq!(cycle_mesh_map(ViewMode::Unlit, true), first);
        assert_eq!(cycle_mesh_map(ViewMode::Unlit, false), last);
        assert_eq!(
            cycle_mesh_map(first, true),
            ViewMode::MeshMapPreview(kinds[1])
        );
        assert_eq!(cycle_mesh_map(last, true), first);
        assert_eq!(cycle_mesh_map(first, false), last);
        assert_eq!(
            cycle_mesh_map(ViewMode::Solo(SoloChannel::Height), true),
            first
        );
    }

    #[test]
    fn names_use_the_view_menus_keys() {
        let tr = |k: &str| format!("<{k}>");
        assert_eq!(mode_name(ViewMode::Lit, tr), "<viewkeys.toast-material>");
        assert_eq!(
            mode_name(ViewMode::Solo(SoloChannel::Roughness), tr),
            "<viewkeys.toast-channel> <viewmode.roughness>"
        );
        assert_eq!(
            mode_name(ViewMode::MeshMapPreview(MeshMapKind::Curvature), tr),
            "<viewkeys.toast-map> curvature"
        );
        let i18n = crate::i18n::I18n::builtin();
        let toast = [
            "viewkeys.toast-material",
            "viewkeys.toast-unlit",
            "viewkeys.toast-channel",
            "viewkeys.toast-map",
        ];
        for key in SoloChannel::ALL.map(channel_key).into_iter().chain(toast) {
            for locale in ["en-US", "pt-BR"] {
                assert!(i18n.has_key(locale, key), "{locale} missing {key}");
            }
        }
    }
}
