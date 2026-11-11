//! umber-core — the Umber document model.
//!
//! Pure Rust, no GPU dependency: texture sets, channel stacks, layers,
//! masks, and the undo journal. Everything here must be testable headless
//! in CI (SPEC.md acceptance criterion: CPU-path round-trip determinism).
//!
//! Wave 2 fills in the layer stack ([`layers`]), the command-pattern undo
//! journal ([`undo`]), and the `.umber` project format ([`project`]) on top
//! of the Wave 1 texture-set/channel skeleton below
//! (docs/specs/requirements.md §2, §8).

// Never-crash policy (clippy.toml + queue row 13, absorbed from the
// photocraft exploration): the engine crates deny unwrap/expect/panic
// in non-test code. The 2026-10-13 census found ZERO non-test uses,
// so this is a pin. Tests are exempt (the cfg_attr): the deny binds
// ONLY non-test builds, so test-code unwrap/expect stays legal while
// a shipped-code unwrap becomes a hard compile error.
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub mod assets;
pub mod bundled;
pub mod control;
pub mod journal;
pub mod layers;
pub mod project;
pub mod smart;
pub mod stencil;
pub mod svg;
pub mod tiles;
pub mod undo;
pub mod uv_dilation;

pub use assets::{AssetError, AssetRef, AssetStore};
pub use journal::{Journal, JournalEntry, JournalOp};

pub use layers::{BlendMode, InstanceError, Layer, LayerCommand, LayerKind, LayerMask, LayerStack};
pub use layers::{MaterialChannel, MaterialParamOverride, MaterialParamValue, MaterialParams};
pub use project::{
    load_from_dir, save_to_dir, ProjectError, ProjectModel, ProjectSettings, TextureSetLayers,
    CURRENT_PROJECT_VERSION, LAYER_PLANES_PROJECT_VERSION, MAX_SUPPORTED_PROJECT_VERSION,
};
pub use smart::{
    AnchorError, AnchorRef, GeneratorBinding, MaskOp, MaskOpKind, MaskOpParams, MaskSource,
    SmartApplyResult, SmartError, SmartLayerKind, SmartLayerTemplate, SmartMask, SmartMaterial,
};
pub use tiles::{AffectedRegion, TileChannels, TileSet};
pub use umber_mesh::{FillParams, FillProjection, FillSource, ProjAxis};
pub use undo::{Command, UndoStack};
pub use uv_dilation::{DilationMap, NONE as UV_PAD_NONE};

/// A named channel of a texture set (e.g. baseColor, roughness, normal).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    pub name: String,
    pub kind: ChannelKind,
}

/// How a channel's values are stored and color-managed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelKind {
    /// sRGB-encoded color data (baseColor, emissive color).
    Color,
    /// Linear data (roughness, metallic, height, normal, AO).
    Data,
}

/// One texture set: a resolution plus a stack of channels.
/// Wave 1: model skeleton; painting + per-channel blending land in Wave 2.
///
/// Wave 5 (UDIM slice 2): `tiles` carries the per-tile channel maps.
/// Empty means "tile 1001 implied by `resolution`/`channels`" — the
/// `serde(default)` keeps pre-tile `.umber` files loading unchanged,
/// and the [`TileSet`] conversions normalize 1001-only sets back to
/// this empty form. `BTreeMap` keeps tile ids sorted on the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextureSet {
    pub name: String,
    pub resolution: u32,
    pub channels: Vec<Channel>,
    #[serde(default)]
    pub tiles: BTreeMap<u16, TileChannels>,
}

impl TextureSet {
    /// The default OpenPBR-style starting set for a new project.
    pub fn new_default(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            resolution: 2048,
            channels: vec![
                Channel {
                    name: "baseColor".into(),
                    kind: ChannelKind::Color,
                },
                Channel {
                    name: "roughness".into(),
                    kind: ChannelKind::Data,
                },
                Channel {
                    name: "metallic".into(),
                    kind: ChannelKind::Data,
                },
                Channel {
                    name: "normal".into(),
                    kind: ChannelKind::Data,
                },
                Channel {
                    name: "height".into(),
                    kind: ChannelKind::Data,
                },
            ],
            tiles: BTreeMap::new(),
        }
    }

    /// Declares a new channel (the Texture Sets panel's add row, R-B4).
    /// The name is trimmed; an empty name, a name another channel already
    /// carries (case-insensitively: per-channel paint planes save as one
    /// directory each, and Windows folds case), or one that is unsafe as a
    /// single directory name is refused. Tiled sets get the channel on
    /// every tile too, so the per-tile lists stay in step with `channels`.
    pub fn add_channel(&mut self, name: &str, kind: ChannelKind) -> Result<(), ChannelError> {
        let name = self.check_new_name(name, None)?;
        let channel = Channel { name, kind };
        for tile in self.tiles.values_mut() {
            // A tile that already lists the name (a hand-edited project,
            // where the tile lists and the declaration drifted) keeps its
            // entry: pushing a second one would double-bind the plane.
            if tile.channels.iter().any(|c| c.name == channel.name) {
                continue;
            }
            tile.channels.push(channel.clone());
        }
        self.channels.push(channel);
        Ok(())
    }

    /// Removes a channel from the set's DECLARATION and returns it. The
    /// paint planes stay: an undeclared channel still edits (the panel's
    /// materialized-but-unlisted rule), so removal never deletes pixels.
    /// [`BASE_COLOR_CHANNEL`] is protected.
    pub fn remove_channel(&mut self, name: &str) -> Result<Channel, ChannelError> {
        if name == BASE_COLOR_CHANNEL {
            return Err(ChannelError::BaseColorProtected);
        }
        let at = self
            .channels
            .iter()
            .position(|c| c.name == name)
            .ok_or_else(|| ChannelError::Unknown(name.to_string()))?;
        for tile in self.tiles.values_mut() {
            tile.channels.retain(|c| c.name != name);
        }
        Ok(self.channels.remove(at))
    }

    /// Renames a declared channel in place (order and kind kept). The new
    /// name follows [`Self::add_channel`]'s rules (a case-only rename of
    /// the same channel is allowed); [`BASE_COLOR_CHANNEL`] can be neither
    /// renamed nor taken as a new name. Declaration only: planes keyed by
    /// the old name stay under it (the kill report's strand rule).
    pub fn rename_channel(&mut self, old: &str, new: &str) -> Result<(), ChannelError> {
        if old == BASE_COLOR_CHANNEL || new.trim() == BASE_COLOR_CHANNEL {
            return Err(ChannelError::BaseColorProtected);
        }
        if !self.channels.iter().any(|c| c.name == old) {
            return Err(ChannelError::Unknown(old.to_string()));
        }
        let new = self.check_new_name(new, Some(old))?;
        for channel in self
            .channels
            .iter_mut()
            .chain(self.tiles.values_mut().flat_map(|t| t.channels.iter_mut()))
        {
            if channel.name == old {
                channel.name = new.clone();
            }
        }
        Ok(())
    }

    /// The trimmed name, or why it cannot name a channel of this set
    /// (`except` = the channel being renamed, ignored by the dup check).
    fn check_new_name(&self, name: &str, except: Option<&str>) -> Result<String, ChannelError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ChannelError::EmptyName);
        }
        if !is_valid_channel_name(name) {
            return Err(ChannelError::InvalidName(name.to_string()));
        }
        let folded = name.to_lowercase();
        let taken = self
            .channels
            .iter()
            .filter(|c| Some(c.name.as_str()) != except)
            .any(|c| c.name.to_lowercase() == folded);
        if taken {
            return Err(ChannelError::Duplicate(name.to_string()));
        }
        Ok(name.to_string())
    }
}

/// The reserved base channel every set paints into (the paint session's
/// `BASE_COLOR_CHANNEL` names the same string). It IS the set's size and
/// its legacy flat sidecar, so the channel commands never remove or
/// rename it.
pub const BASE_COLOR_CHANNEL: &str = "baseColor";

/// Whether `name` is safe as ONE directory name under the project's
/// `paint/` sidecar on every OS: no separators, no Windows-reserved
/// characters, no control characters, not dot-only. Mirrors the paint
/// persistence's manifest trust check, plus `/`, so a declared channel
/// can always save.
///
/// The set-name rule's extra clauses come with it: a trailing dot or
/// space (Windows strips both, so "ao" and "ao." would be ONE sidecar
/// directory) and the Windows reserved device stems ("CON", "CON.txt" —
/// invalid on Windows whatever the extension). A channel name becomes a
/// directory name exactly like a set name, so it answers to the same
/// rule ([`crate::project::validate_set_name`]'s).
fn is_valid_channel_name(name: &str) -> bool {
    let stem = name.split_once('.').map_or(name, |(before, _)| before);
    !name.chars().all(|c| c == '.')
        && !name.contains(['/', '\\', ':', '<', '>', '"', '|', '?', '*'])
        && !name.chars().any(char::is_control)
        && !name.ends_with('.')
        && !name.ends_with(' ')
        && !crate::project::WINDOWS_RESERVED_STEMS.contains(&stem.to_ascii_lowercase().as_str())
}

/// Why a channel add/remove/rename was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChannelError {
    /// The name was empty (or whitespace only).
    #[error("a channel name cannot be empty")]
    EmptyName,
    /// Another channel of the set already carries the name.
    #[error("channel {0:?} already exists")]
    Duplicate(String),
    /// The name is not safe as a sidecar directory name.
    #[error("{0:?} is not a valid channel name")]
    InvalidName(String),
    /// No declared channel has this name.
    #[error("no channel {0:?}")]
    Unknown(String),
    /// baseColor cannot be removed, renamed, or renamed onto.
    #[error("the baseColor channel cannot be removed or renamed")]
    BaseColorProtected,
}

/// A flat 3D bounding box, in the mesh's own coordinate space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_texture_set_has_openpbr_channels() {
        let ts = TextureSet::new_default("Body");
        assert_eq!(ts.channels.len(), 5);
        assert_eq!(ts.channels[0].name, "baseColor");
        assert_eq!(ts.channels[0].kind, ChannelKind::Color);
        assert_eq!(ts.channels[2].name, "metallic");
        assert_eq!(ts.channels[2].kind, ChannelKind::Data);
    }

    fn names(ts: &TextureSet) -> Vec<&str> {
        ts.channels.iter().map(|c| c.name.as_str()).collect()
    }

    #[test]
    fn channel_names_refuse_the_windows_device_stems_and_trailing_dots() {
        let mut ts = TextureSet::new_default("Body");
        // The set-name rule's clauses, for a channel name too: a channel
        // becomes a `paint/<channel>/` directory.
        for name in [
            "CON", "con", "PRN.txt", "aux", "NUL", "COM1", "lpt9", "com1.log",
        ] {
            assert_eq!(
                ts.add_channel(name, ChannelKind::Data),
                Err(ChannelError::InvalidName(name.to_string())),
                "{name} must be refused",
            );
        }
        for name in ["ao.", "ao.."] {
            assert_eq!(
                ts.add_channel(name, ChannelKind::Data),
                Err(ChannelError::InvalidName(name.to_string())),
                "{name} must be refused",
            );
        }
        // A trailing SPACE is not refused: the name is trimmed first, so
        // "ao " lands on "ao" — the same directory Windows would keep.
        ts.add_channel("ao ", ChannelKind::Data).unwrap();
        assert!(ts.channels.iter().any(|c| c.name == "ao"));
        assert!(!ts.channels.iter().any(|c| c.name == "ao "));
        // The ordinary names still pass.
        ts.add_channel("dirt_mask", ChannelKind::Data).unwrap();
        ts.add_channel("ao2", ChannelKind::Data).unwrap();
        assert!(ts.channels.iter().any(|c| c.name == "dirt_mask"));
    }

    #[test]
    fn add_channel_never_doubles_a_tile_entry() {
        let mut ts = TextureSet::new_default("Body");
        // A tile that already lists the name (a drifted project.json):
        // the declaration gains the channel, the tile keeps ONE entry.
        ts.tiles.insert(
            1001,
            TileChannels::new(
                2048,
                vec![Channel {
                    name: "ao".into(),
                    kind: ChannelKind::Data,
                }],
            ),
        );
        ts.add_channel("ao", ChannelKind::Data).unwrap();
        assert_eq!(ts.channels.iter().filter(|c| c.name == "ao").count(), 1);
        assert_eq!(
            ts.tiles[&1001]
                .channels
                .iter()
                .filter(|c| c.name == "ao")
                .count(),
            1
        );
    }

    #[test]
    fn add_channel_appends_trimmed_and_refuses_empty_dup_and_unsafe() {
        let mut ts = TextureSet::new_default("Body");
        ts.add_channel("  dirt_mask ", ChannelKind::Data).unwrap();
        assert_eq!(ts.channels.last().unwrap().name, "dirt_mask");
        assert_eq!(ts.channels.len(), 6);
        ts.add_channel("emissive", ChannelKind::Color).unwrap();
        assert_eq!(ts.channels.last().unwrap().kind, ChannelKind::Color);
        let before = ts.clone();
        assert_eq!(
            ts.add_channel("   ", ChannelKind::Data),
            Err(ChannelError::EmptyName)
        );
        assert_eq!(
            ts.add_channel("Roughness", ChannelKind::Data),
            Err(ChannelError::Duplicate("Roughness".into()))
        );
        for bad in ["a/b", "..", "c:d", "x\\y", "tab\there"] {
            assert_eq!(
                ts.add_channel(bad, ChannelKind::Data),
                Err(ChannelError::InvalidName(bad.into())),
                "{bad:?}"
            );
        }
        assert_eq!(ts, before, "a refused add changes nothing");
    }

    #[test]
    fn remove_channel_protects_base_color_and_reports_unknown() {
        let mut ts = TextureSet::new_default("Body");
        let removed = ts.remove_channel("metallic").unwrap();
        assert_eq!(removed.name, "metallic");
        assert_eq!(names(&ts), ["baseColor", "roughness", "normal", "height"]);
        assert_eq!(
            ts.remove_channel(BASE_COLOR_CHANNEL),
            Err(ChannelError::BaseColorProtected)
        );
        assert_eq!(
            ts.remove_channel("metallic"),
            Err(ChannelError::Unknown("metallic".into()))
        );
        assert_eq!(ts.channels.len(), 4);
    }

    #[test]
    fn rename_channel_keeps_order_and_kind_and_refuses_dup_and_base_color() {
        let mut ts = TextureSet::new_default("Body");
        ts.rename_channel("height", " displacement ").unwrap();
        assert_eq!(
            names(&ts),
            [
                "baseColor",
                "roughness",
                "metallic",
                "normal",
                "displacement"
            ]
        );
        assert_eq!(ts.channels[4].kind, ChannelKind::Data);
        // A case-only rename of the same channel is not a duplicate.
        ts.rename_channel("normal", "Normal").unwrap();
        assert_eq!(ts.channels[3].name, "Normal");
        let before = ts.clone();
        assert_eq!(
            ts.rename_channel("roughness", "METALLIC"),
            Err(ChannelError::Duplicate("METALLIC".into()))
        );
        assert_eq!(
            ts.rename_channel(BASE_COLOR_CHANNEL, "albedo"),
            Err(ChannelError::BaseColorProtected)
        );
        assert_eq!(
            ts.rename_channel("roughness", BASE_COLOR_CHANNEL),
            Err(ChannelError::BaseColorProtected)
        );
        assert_eq!(
            ts.rename_channel("nope", "x"),
            Err(ChannelError::Unknown("nope".into()))
        );
        assert_eq!(
            ts.rename_channel("roughness", ""),
            Err(ChannelError::EmptyName)
        );
        assert_eq!(ts, before, "a refused rename changes nothing");
    }

    #[test]
    fn channel_commands_keep_tiled_sets_in_step() {
        let mut ts = TextureSet::new_default("Body");
        ts.tiles
            .insert(1001, TileChannels::new(ts.resolution, ts.channels.clone()));
        ts.tiles
            .insert(1002, TileChannels::new(1024, ts.channels.clone()));
        ts.add_channel("dirt", ChannelKind::Data).unwrap();
        ts.rename_channel("roughness", "gloss").unwrap();
        ts.remove_channel("metallic").unwrap();
        for tile in ts.tiles.values() {
            assert_eq!(tile.channels, ts.channels);
        }
    }

    #[test]
    fn base_color_constant_names_the_default_first_channel() {
        assert_eq!(
            TextureSet::new_default("Body").channels[0].name,
            BASE_COLOR_CHANNEL
        );
    }
}
