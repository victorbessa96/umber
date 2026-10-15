//! Layer thumbnails (GUI plan G2 `g2-layer-thumbnails`,
//! `docs/specs/gui-plan-2026-10-15.md`; sources: Painter B1 ("Thumbnails:
//! real thumbnails by default"), photocraft #6 (content-keyed, updated in
//! place with `tex.set`, pruned on delete)).
//!
//! One small texture per layer, showing its tile-1001 baseColor plane
//! (Painter's stack shows the color channel too). The cache key is the
//! paint session's per-owner content revision
//! (`PaintState::owner_revision`): painting one layer rebuilds only that
//! layer's thumbnail. An unchanged layer costs a map lookup per frame and
//! no GPU readback. Rebuilds are rate-limited to [`MAX_REBUILDS_PER_FRAME`],
//! so a session load with many layers never stalls one frame on readbacks,
//! and never run while a stroke is in flight (the stroke writes its layer
//! every frame; the thumbnail catches up when the stroke ends).
//!
//! The pure part is [`downsample`]: a box filter from the plane to
//! [`THUMB_PX`] square, composited over a checker so alpha reads as
//! transparency.

use std::collections::BTreeMap;

use egui::{ColorImage, TextureHandle, TextureOptions};

use crate::paint_state::{PaintState, PlaneOwner, BASE_COLOR_CHANNEL};

/// Thumbnail edge, px (Painter's stack rows are ~40 px tall).
pub const THUMB_PX: usize = 32;
/// At most this many thumbnails rebuild per frame (each costs one plane
/// readback).
pub const MAX_REBUILDS_PER_FRAME: usize = 2;
/// The tile thumbnails show.
const THUMB_TILE: u16 = umber_mesh::FIRST_TILE;

/// Box-filters an RGBA8 plane down to `out` × `out`, over a two-tone
/// checker so transparent paint reads as transparent. Rows are flipped:
/// the paint target's row 0 is V = 0 (the bottom of the UV square), while
/// an image's row 0 is the top. Returns `out * out * 4` bytes.
pub fn downsample(rgba: &[u8], width: usize, height: usize, out: usize) -> Vec<u8> {
    let mut img = vec![0u8; out * out * 4];
    if width == 0 || height == 0 || rgba.len() < width * height * 4 || out == 0 {
        return img;
    }
    for oy in 0..out {
        for ox in 0..out {
            let (x0, x1) = (
                ox * width / out,
                ((ox + 1) * width / out).max(ox * width / out + 1),
            );
            let (y0, y1) = (
                oy * height / out,
                ((oy + 1) * height / out).max(oy * height / out + 1),
            );
            let mut acc = [0u64; 4];
            let mut n = 0u64;
            for y in y0..y1.min(height) {
                for x in x0..x1.min(width) {
                    let i = (y * width + x) * 4;
                    for c in 0..4 {
                        acc[c] += u64::from(rgba[i + c]);
                    }
                    n += 1;
                }
            }
            let n = n.max(1);
            let [r, g, b, a] = acc.map(|v| (v / n) as u32);
            // Checker: 4 px cells, light / dark grey.
            let bg: u32 = if (ox / 4 + oy / 4) % 2 == 0 {
                0x5a
            } else {
                0x3c
            };
            let over = |c: u32| ((c * a + bg * (255 - a)) / 255) as u8;
            // Flip vertically: plane row 0 is the bottom of the image.
            let o = ((out - 1 - oy) * out + ox) * 4;
            img[o..o + 4].copy_from_slice(&[over(r), over(g), over(b), 255]);
        }
    }
    img
}

/// The per-layer thumbnail textures, keyed by layer id and content
/// revision.
#[derive(Default)]
pub struct Thumbnails {
    cache: BTreeMap<u64, (u64, TextureHandle)>,
}

impl Thumbnails {
    /// The thumbnail for `layer`, rebuilt first if its pixels changed
    /// (and this frame's rebuild budget allows). `None` for a layer that
    /// never got a plane.
    pub fn get(
        &mut self,
        ctx: &egui::Context,
        paint: &mut PaintState,
        layer: u64,
        budget: &mut usize,
    ) -> Option<TextureHandle> {
        let owner = PlaneOwner::Layer(layer);
        let rev = paint.owner_revision(owner);
        let stale = self.cache.get(&layer).is_none_or(|(r, _)| *r != rev);
        // Never mid-stroke: a stroke writes its layer every frame, and a
        // 4K plane readback per frame would cost the paint loop its
        // budget. The thumbnail catches up on the first frame after the
        // stroke ends (the revision is still newer than the cached one).
        if stale && *budget > 0 && rev > 0 && !paint.is_stroking() {
            *budget -= 1;
            if let Some(plane) = paint.read_plane(owner, THUMB_TILE, BASE_COLOR_CHANNEL) {
                let bytes = downsample(
                    &plane.data,
                    plane.width as usize,
                    plane.height as usize,
                    THUMB_PX,
                );
                let image = ColorImage::from_rgba_unmultiplied([THUMB_PX, THUMB_PX], &bytes);
                match self.cache.get_mut(&layer) {
                    // Update in place: no texture re-allocation per stroke.
                    Some((r, tex)) => {
                        tex.set(image, TextureOptions::LINEAR);
                        *r = rev;
                    }
                    None => {
                        let tex = ctx.load_texture(
                            format!("umber-layer-thumb-{layer}"),
                            image,
                            TextureOptions::LINEAR,
                        );
                        self.cache.insert(layer, (rev, tex));
                    }
                }
            }
        }
        self.cache.get(&layer).map(|(_, t)| t.clone())
    }

    /// Drops thumbnails of layers that no longer exist.
    pub fn prune(&mut self, live: &[u64]) {
        self.cache.retain(|id, _| live.contains(id));
    }

    /// How many thumbnails are cached (tests).
    pub fn len(&self) -> usize {
        self.cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downsample_averages_blocks_and_flips_rows() {
        // 4x4 plane: bottom two rows (plane rows 0-1) opaque red, top two
        // (rows 2-3) opaque blue.
        let mut plane = Vec::new();
        for y in 0..4 {
            for _ in 0..4 {
                plane.extend_from_slice(if y < 2 {
                    &[255, 0, 0, 255]
                } else {
                    &[0, 0, 255, 255]
                });
            }
        }
        let img = downsample(&plane, 4, 4, 2);
        assert_eq!(img.len(), 2 * 2 * 4);
        // Image row 0 is the TOP = plane rows 2-3 = blue.
        assert_eq!(&img[0..4], &[0, 0, 255, 255]);
        // Image row 1 is the bottom = red.
        assert_eq!(&img[8..12], &[255, 0, 0, 255]);
    }

    #[test]
    fn transparent_paint_shows_the_checker() {
        let plane = vec![0u8; 8 * 8 * 4]; // fully transparent
        let img = downsample(&plane, 8, 8, 8);
        let px = |x: usize, y: usize| img[(y * 8 + x) * 4];
        // Two checker tones, both opaque greys.
        assert_ne!(px(0, 0), px(4, 0), "cells alternate");
        assert!(img
            .chunks_exact(4)
            .all(|p| p[3] == 255 && p[0] == p[1] && p[1] == p[2]));
    }

    #[test]
    fn degenerate_input_is_blank_not_a_panic() {
        assert_eq!(downsample(&[], 0, 0, 4), vec![0; 64]);
        assert_eq!(downsample(&[1, 2, 3], 4, 4, 2), vec![0; 16], "short buffer");
        assert!(downsample(&[0; 16], 2, 2, 0).is_empty());
    }

    #[test]
    fn prune_keeps_only_live_layers() {
        let ctx = egui::Context::default();
        let mut t = Thumbnails::default();
        let img = ColorImage::new([1, 1], vec![egui::Color32::WHITE]);
        for id in [1, 2, 3] {
            let tex = ctx.load_texture(format!("t{id}"), img.clone(), TextureOptions::LINEAR);
            t.cache.insert(id, (1, tex));
        }
        t.prune(&[1, 3]);
        assert_eq!(t.cache.keys().copied().collect::<Vec<_>>(), vec![1, 3]);
        assert_eq!(t.len(), 2);
    }
}
