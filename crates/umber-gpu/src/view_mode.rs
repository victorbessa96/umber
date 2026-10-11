//! Viewport view modes (requirements.md §5, P0 row: "lit / unlit
//! single-channel solo (±display transform) / mesh-map preview").
//!
//! # Placement: why this lives in umber-gpu
//!
//! The mode's shader encoding (the `mode`/`channel` u32 ids), the
//! reserved channel-texture bindings, and the LUT-bypass flag are
//! GPU-layout facts — the same class of fact as [`crate::WireColor`]
//! or [`crate::SpecParams`]. umber-app therefore owns only the
//! *selection* (a `ViewMode` value synced into `Viewport`, like the
//! wireframe/grid bools) and never names a wgpu type; everything the
//! mesh pass needs to honor the selection lives here.
//!
//! # The LUT-bypass semantics (§9 rule)
//!
//! Per-channel color management (requirements.md §9: "baseColor yes;
//! roughness/metallic/normal = data") decides the display transform:
//!
//! * through the LUT: [`ViewMode::Lit`], [`ViewMode::Unlit`], and
//!   [`ViewMode::Solo`] of a color channel (baseColor, emissive);
//! * bypassed: [`ViewMode::Solo`] of a data channel (roughness,
//!   metallic, AO, height, normal) and every
//!   [`ViewMode::MeshMapPreview`] (all bake outputs are data).
//!
//! The bypass is wired at bind time: [`ViewMode::select_lut`] returns
//! `None` for bypass modes, so the mesh pass binds the context's
//! identity table — a byte-exact bypass per
//! [`crate::display_lut`]'s contract (entry `i` holds `(i, i, i)`).
//! The shader additionally early-returns before `apply_display_lut`
//! for those modes (defense in depth: a caller that passes the app
//! LUT with a data mode still renders untransformed). The Rust matrix
//! ([`ViewMode::applies_display_lut`]) is canonical; the WGSL rule in
//! [`VIEW_MODE_WGSL`] mirrors it and the tests pin both sides.
//!
//! # Solo sources in v1
//!
//! The paint texture-set's channels are document-model data today
//! (`umber_core::TextureSet`: baseColor/roughness/metallic/normal/
//! height) with no per-channel GPU textures bound in the mesh pass,
//! so Solo renders a *proxy* from already-bound data (normals, the
//! [`crate::SpecParams`] uniform, environment irradiance) — see the
//! `solo_proxy` arms in [`crate::shaders::MESH_SHADER`]. The
//! [`SoloChannel::texture_binding`] ids reserve the future
//! channel-texture group (group 2, one binding per channel): the ids
//! pin the layout contract now so the Wave-2 texture handoff cannot
//! renumber them.

use umber_mesh::MeshMapKind;

use crate::display_lut::DisplayLut;

/// One paint channel shown full-screen by [`ViewMode::Solo`].
///
/// The color/data split ([`Self::is_data`]) mirrors
/// `umber_core::ChannelKind` (`Color` = baseColor/emissive,
/// `Data` = the rest) and requirements.md §9 without taking a
/// dependency on umber-core from the GPU crate — the mapping is
/// pinned by `solo_color_data_split_mirrors_channel_kind` below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum SoloChannel {
    /// Albedo. Color: goes through the display LUT.
    BaseColor = 0,
    /// Emission color. Color: goes through the display LUT.
    Emissive = 1,
    /// Specular roughness. Data: bypasses the display LUT.
    Roughness = 2,
    /// Base metalness. Data: bypasses the display LUT.
    Metallic = 3,
    /// Ambient occlusion. Data: bypasses the display LUT.
    AmbientOcclusion = 4,
    /// Displacement height. Data: bypasses the display LUT.
    Height = 5,
    /// Tangent-space normal. Data: bypasses the display LUT.
    Normal = 6,
}

impl SoloChannel {
    /// Every soloable channel, in texture-binding order.
    pub const ALL: [Self; 7] = [
        Self::BaseColor,
        Self::Emissive,
        Self::Roughness,
        Self::Metallic,
        Self::AmbientOcclusion,
        Self::Height,
        Self::Normal,
    ];

    /// Whether this channel is linear data (and therefore bypasses
    /// the display transform). `false` = sRGB color data.
    pub fn is_data(self) -> bool {
        !matches!(self, Self::BaseColor | Self::Emissive)
    }

    /// Whether a solo of this channel runs through the display LUT
    /// (the §9 rule: color yes, data no).
    pub fn uses_display_transform(self) -> bool {
        !self.is_data()
    }

    /// The reserved channel-texture binding (the future group-2 slot).
    /// No group 2 exists in v1 — these ids pin the layout contract
    /// for the Wave-2 texture handoff; the order matches
    /// [`Self::ALL`] and the `VIEW_MODE_SOLO_*` WGSL constants.
    pub fn texture_binding(self) -> u32 {
        self as u32
    }

    /// The export-style token for this channel (matches
    /// `umber_export::MapKind::token` where the vocabularies overlap:
    /// `baseColor`, `emissive`, `roughness`, `metallic`,
    /// `ambient_occlusion`, `height`, `normal`).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BaseColor => "baseColor",
            Self::Emissive => "emissive",
            Self::Roughness => "roughness",
            Self::Metallic => "metallic",
            Self::AmbientOcclusion => "ambient_occlusion",
            Self::Height => "height",
            Self::Normal => "normal",
        }
    }
}

/// Where a [`ViewMode`]'s pixels come from: the shaded/paint
/// composite path, or the bake-output path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelSource {
    /// Lit / Unlit / Solo: the mesh pass over paint + material state.
    Composite,
    /// Mesh-map preview: the bake outputs, not the paint.
    Bake,
}

/// The viewport's shading mode (requirements.md §5 P0 row).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewMode {
    /// Today's render: the full PBR math through the display LUT.
    #[default]
    Lit,
    /// Albedo/base-color only, no lighting math, through the LUT
    /// (unlit is still a color view of a color channel — §9).
    Unlit,
    /// One channel displayed raw (grayscale or remapped). Color
    /// channels go through the LUT, data channels bypass it.
    Solo(SoloChannel),
    /// The baked mesh map displayed instead of the paint (always
    /// data, always bypasses the LUT).
    MeshMapPreview(MeshMapKind),
}

impl ViewMode {
    /// Every previewable bake kind, in preview-id order (matches
    /// [`preview_id`](Self::preview_id) and the `VIEW_MODE_PREVIEW_*`
    /// WGSL constants).
    pub const ALL_PREVIEW_KINDS: [MeshMapKind; 9] = [
        MeshMapKind::AmbientOcclusion,
        MeshMapKind::Curvature,
        MeshMapKind::NormalBase,
        MeshMapKind::Position,
        MeshMapKind::Thickness,
        MeshMapKind::Id,
        MeshMapKind::Height,
        MeshMapKind::BentNormal,
        MeshMapKind::WorldSpaceNormal,
    ];

    /// The `V` shortcut: cycles lit ↔ unlit. From Solo or
    /// MeshMapPreview, `V` returns to Lit (the shortcut only ever
    /// toggles the two full-frame modes).
    pub fn cycle_v(self) -> Self {
        match self {
            Self::Lit => Self::Unlit,
            _ => Self::Lit,
        }
    }

    /// Whether this mode's output runs through the display LUT (the
    /// §9 rule — see the module docs). The mesh pass binds the
    /// identity table when this is `false` (see [`Self::select_lut`]).
    pub fn applies_display_lut(self) -> bool {
        match self {
            Self::Lit | Self::Unlit => true,
            Self::Solo(channel) => channel.uses_display_transform(),
            Self::MeshMapPreview(_) => false,
        }
    }

    /// Where this mode's pixels come from (test (d)'s hook: preview
    /// routes the bake output, everything else the composite).
    pub fn pixel_source(self) -> PixelSource {
        match self {
            Self::MeshMapPreview(_) => PixelSource::Bake,
            _ => PixelSource::Composite,
        }
    }

    /// The shader `mode` id (`VIEW_MODE_*` in [`VIEW_MODE_WGSL`]).
    pub fn mode_id(self) -> u32 {
        match self {
            Self::Lit => 0,
            Self::Unlit => 1,
            Self::Solo(_) => 2,
            Self::MeshMapPreview(_) => 3,
        }
    }

    /// The shader `channel` id: the solo channel's discriminant, or
    /// the preview kind's id for [`Self::MeshMapPreview`]; 0 for the
    /// full-frame modes.
    pub fn channel_id(self) -> u32 {
        match self {
            Self::Lit | Self::Unlit => 0,
            Self::Solo(channel) => channel as u32,
            Self::MeshMapPreview(kind) => Self::preview_id(kind),
        }
    }

    /// The preview id of a bake kind (`VIEW_MODE_PREVIEW_*` in
    /// [`VIEW_MODE_WGSL`], in [`Self::ALL_PREVIEW_KINDS`] order).
    pub fn preview_id(kind: MeshMapKind) -> u32 {
        match kind {
            MeshMapKind::AmbientOcclusion => 0,
            MeshMapKind::Curvature => 1,
            MeshMapKind::NormalBase => 2,
            MeshMapKind::Position => 3,
            MeshMapKind::Thickness => 4,
            MeshMapKind::Id => 5,
            MeshMapKind::Height => 6,
            MeshMapKind::BentNormal => 7,
            MeshMapKind::WorldSpaceNormal => 8,
        }
    }

    /// The uniform bytes for this mode (see [`ViewModeParams`]).
    pub fn uniform(self) -> ViewModeParams {
        ViewModeParams {
            mode: self.mode_id(),
            channel: self.channel_id(),
            _pad: [0, 0],
        }
    }

    /// The display LUT the mesh pass should bind for this mode: the
    /// app's LUT when the mode runs through the display transform,
    /// `None` (the context's identity table — a byte-exact bypass)
    /// when it bypasses. Callers pass the result straight to
    /// [`crate::MeshBuffers::paint_callback_with_mode`].
    pub fn select_lut<'a>(self, app_lut: Option<&'a DisplayLut>) -> Option<&'a DisplayLut> {
        if self.applies_display_lut() {
            app_lut
        } else {
            None
        }
    }
}

/// The mode-selector uniform (group 0, binding 8 of the mesh pass):
/// two u32s + padding to the 16-byte uniform multiple. Zeroable so
/// fresh buffers start as Lit (mode 0, channel 0).
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ViewModeParams {
    /// [`ViewMode::mode_id`].
    pub mode: u32,
    /// [`ViewMode::channel_id`].
    pub channel: u32,
    /// Padding to 16 bytes (uniform alignment).
    pub _pad: [u32; 2],
}

/// WGSL for the mode selector — pasted verbatim into
/// [`crate::shaders::MESH_SHADER`] (WGSL has no includes here; the
/// verbatim test below pins the copy so the ids cannot drift from
/// [`ViewMode::mode_id`], the [`SoloChannel`] discriminants, or
/// [`ViewMode::preview_id`].
///
/// Self-contained by construction: only the ids, the uniform
/// declaration, and the bypass predicate. The solo/preview shading
/// arms live in `MESH_SHADER` itself (they read that shader's
/// `BASE_COLOR`, `spec_params`, and irradiance fns).
pub const VIEW_MODE_WGSL: &str = r#"// ---- view mode (crate::view_mode — keep verbatim) ----
struct ViewModeParams {
    mode: u32,
    channel: u32,
    _pad0: u32,
    _pad1: u32,
};
@group(0) @binding(8) var<uniform> view_mode: ViewModeParams;

const VIEW_MODE_LIT: u32 = 0u;
const VIEW_MODE_UNLIT: u32 = 1u;
const VIEW_MODE_SOLO: u32 = 2u;
const VIEW_MODE_MESH_PREVIEW: u32 = 3u;

const VIEW_MODE_SOLO_BASE_COLOR: u32 = 0u;
const VIEW_MODE_SOLO_EMISSIVE: u32 = 1u;
const VIEW_MODE_SOLO_ROUGHNESS: u32 = 2u;
const VIEW_MODE_SOLO_METALLIC: u32 = 3u;
const VIEW_MODE_SOLO_AO: u32 = 4u;
const VIEW_MODE_SOLO_HEIGHT: u32 = 5u;
const VIEW_MODE_SOLO_NORMAL: u32 = 6u;

const VIEW_MODE_PREVIEW_AO: u32 = 0u;
const VIEW_MODE_PREVIEW_CURVATURE: u32 = 1u;
const VIEW_MODE_PREVIEW_NORMAL_BASE: u32 = 2u;
const VIEW_MODE_PREVIEW_POSITION: u32 = 3u;
const VIEW_MODE_PREVIEW_THICKNESS: u32 = 4u;
const VIEW_MODE_PREVIEW_ID: u32 = 5u;
const VIEW_MODE_PREVIEW_HEIGHT: u32 = 6u;
const VIEW_MODE_PREVIEW_BENT_NORMAL: u32 = 7u;
const VIEW_MODE_PREVIEW_WORLD_SPACE_NORMAL: u32 = 8u;

fn view_mode_bypasses_lut(mode: u32, channel: u32) -> bool {
    return (mode == VIEW_MODE_SOLO && channel > VIEW_MODE_SOLO_EMISSIVE) || (mode == VIEW_MODE_MESH_PREVIEW);
}
// ---- end view mode ----"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// (a) The data-channel-bypass rule: every data solo (and only
    /// those solos) drops the display transform; Lit, Unlit, and the
    /// color solos keep it.
    #[test]
    fn data_channels_bypass_the_display_lut() {
        // The LUT flag is off for the roughness solo (the named case).
        assert!(!ViewMode::Solo(SoloChannel::Roughness).applies_display_lut());
        // And for every other data channel.
        for channel in SoloChannel::ALL {
            assert_eq!(
                ViewMode::Solo(channel).applies_display_lut(),
                channel.uses_display_transform(),
                "{channel:?} solo must agree with its channel flag"
            );
            assert_eq!(
                ViewMode::Solo(channel).applies_display_lut(),
                !channel.is_data(),
                "{channel:?}"
            );
        }
        assert!(!ViewMode::Solo(SoloChannel::Metallic).applies_display_lut());
        assert!(!ViewMode::Solo(SoloChannel::AmbientOcclusion).applies_display_lut());
        assert!(!ViewMode::Solo(SoloChannel::Height).applies_display_lut());
        assert!(!ViewMode::Solo(SoloChannel::Normal).applies_display_lut());
        // Color solos + the full-frame modes go through.
        assert!(ViewMode::Solo(SoloChannel::BaseColor).applies_display_lut());
        assert!(ViewMode::Solo(SoloChannel::Emissive).applies_display_lut());
        assert!(ViewMode::Lit.applies_display_lut());
        assert!(ViewMode::Unlit.applies_display_lut());
        // Every mesh-map preview bypasses (all bake outputs are data).
        for kind in ViewMode::ALL_PREVIEW_KINDS {
            assert!(
                !ViewMode::MeshMapPreview(kind).applies_display_lut(),
                "{kind:?} preview must bypass the LUT"
            );
        }
    }

    /// The bypass wiring: a bypass mode maps any app LUT to `None`
    /// (the identity-table bind), while a through-mode passes it
    /// along. `None` in stays `None` out either way.
    #[test]
    fn select_lut_binds_identity_for_bypass_modes() {
        assert_eq!(ViewMode::Lit.select_lut(None), None);
        assert_eq!(
            ViewMode::Solo(SoloChannel::Roughness).select_lut(None),
            None
        );
        // Without a device there is no `DisplayLut` to hand in, so the
        // through/bypass split under `Some` is pinned by the matrix
        // above plus the type contract: `select_lut` returns its input
        // unchanged iff `applies_display_lut`.
    }

    /// (b) The `V` cycle order: lit → unlit → lit, and Solo/preview
    /// return to Lit.
    #[test]
    fn v_cycles_lit_unlit_lit() {
        assert_eq!(ViewMode::Lit.cycle_v(), ViewMode::Unlit);
        assert_eq!(ViewMode::Unlit.cycle_v(), ViewMode::Lit);
        assert_eq!(ViewMode::Lit.cycle_v().cycle_v(), ViewMode::Lit);
        assert_eq!(ViewMode::Solo(SoloChannel::Normal).cycle_v(), ViewMode::Lit);
        assert_eq!(
            ViewMode::MeshMapPreview(MeshMapKind::AmbientOcclusion).cycle_v(),
            ViewMode::Lit
        );
    }

    /// (c) Each solo channel's source mapping: distinct reserved
    /// texture bindings, in `ALL` order, equal to the discriminants
    /// (which the WGSL `VIEW_MODE_SOLO_*` constants mirror).
    #[test]
    fn every_solo_channel_has_a_distinct_reserved_binding() {
        let bindings: Vec<u32> = SoloChannel::ALL
            .iter()
            .map(|c| c.texture_binding())
            .collect();
        assert_eq!(bindings, vec![0, 1, 2, 3, 4, 5, 6]);
        for (i, channel) in SoloChannel::ALL.iter().enumerate() {
            assert_eq!(*channel as u32, i as u32, "{channel:?} discriminant");
            assert_eq!(channel.texture_binding(), *channel as u32);
        }
        assert_eq!(SoloChannel::ALL.len(), 7);
    }

    /// (d) The mesh-map preview routes the bake output: every
    /// previewable kind reports [`PixelSource::Bake`], while Lit,
    /// Unlit, and every solo stay on the composite.
    #[test]
    fn mesh_map_preview_routes_the_bake_output() {
        assert_eq!(ViewMode::ALL_PREVIEW_KINDS.len(), 9);
        for kind in ViewMode::ALL_PREVIEW_KINDS {
            assert_eq!(
                ViewMode::MeshMapPreview(kind).pixel_source(),
                PixelSource::Bake,
                "{kind:?}"
            );
        }
        assert_eq!(ViewMode::Lit.pixel_source(), PixelSource::Composite);
        assert_eq!(ViewMode::Unlit.pixel_source(), PixelSource::Composite);
        for channel in SoloChannel::ALL {
            assert_eq!(
                ViewMode::Solo(channel).pixel_source(),
                PixelSource::Composite,
                "{channel:?}"
            );
        }
    }

    /// The uniform encoding: mode/channel ids land in the first two
    /// u32 slots, Lit is the zero value.
    #[test]
    fn uniform_encoding_carries_mode_then_channel() {
        let lit = ViewMode::Lit.uniform();
        assert_eq!([lit.mode, lit.channel], [0, 0]);
        assert_eq!(bytemuck::bytes_of(&lit), &[0u8; 16]);

        let solo = ViewMode::Solo(SoloChannel::Metallic).uniform();
        assert_eq!([solo.mode, solo.channel], [2, 3]);

        let preview = ViewMode::MeshMapPreview(MeshMapKind::Position).uniform();
        assert_eq!([preview.mode, preview.channel], [3, 3]);

        assert_eq!(std::mem::size_of::<ViewModeParams>(), 16);
        // Preview ids cover every bake kind exactly once, in order.
        let ids: Vec<u32> = ViewMode::ALL_PREVIEW_KINDS
            .iter()
            .map(|k| ViewMode::preview_id(*k))
            .collect();
        assert_eq!(ids, vec![0, 1, 2, 3, 4, 5, 6, 7, 8]);
    }

    /// The shader-side contract: `MESH_SHADER` embeds the snippet
    /// verbatim (the display-LUT pattern), so the u32 ids cannot
    /// drift between Rust and WGSL.
    #[test]
    fn mesh_shader_embeds_the_view_mode_snippet_verbatim() {
        assert!(crate::shaders::MESH_SHADER.contains(VIEW_MODE_WGSL));
    }

    /// The WGSL bypass rule mirrors the Rust matrix: solo channels
    /// past emissive (= the data ids) bypass, as does every preview.
    /// Pinned textually (WGSL cannot run headless) plus the numeric
    /// fact it rests on: emissive is id 1, every data id exceeds it.
    #[test]
    fn wgsl_bypass_rule_matches_the_rust_matrix() {
        assert!(VIEW_MODE_WGSL.contains(
            "(mode == VIEW_MODE_SOLO && channel > VIEW_MODE_SOLO_EMISSIVE) \
             || (mode == VIEW_MODE_MESH_PREVIEW)"
        ));
        assert_eq!(SoloChannel::Emissive as u32, 1);
        for channel in SoloChannel::ALL {
            assert_eq!(
                channel.is_data(),
                (channel as u32) > (SoloChannel::Emissive as u32),
                "{channel:?}: data ⟺ id past emissive"
            );
        }
    }

    /// The color/data vocabulary this mode rests on: baseColor and
    /// emissive are color (§9's "yes" set), roughness / metallic /
    /// normal are data (§9's named set), AO and height are data with
    /// it. Mirrors `umber_core::ChannelKind` without depending on it.
    #[test]
    fn solo_color_data_split_mirrors_channel_kind() {
        assert!(!SoloChannel::BaseColor.is_data());
        assert!(!SoloChannel::Emissive.is_data());
        assert!(SoloChannel::Roughness.is_data());
        assert!(SoloChannel::Metallic.is_data());
        assert!(SoloChannel::AmbientOcclusion.is_data());
        assert!(SoloChannel::Height.is_data());
        assert!(SoloChannel::Normal.is_data());
        // Tokens match the export vocabulary where they overlap.
        assert_eq!(SoloChannel::BaseColor.as_str(), "baseColor");
        assert_eq!(SoloChannel::AmbientOcclusion.as_str(), "ambient_occlusion");
        assert_eq!(SoloChannel::Normal.as_str(), "normal");
    }
}
