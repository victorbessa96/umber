//! The quick mask (§2's P1 row: "Quick mask (temporary painted mask,
//! invert)"): a temporary single-channel mask the user paints, which then
//! gates every brush consumer per texel.
//!
//! Placement: the model lives here, in the brush crate, because the gate
//! is applied where per-texel coverage is computed, and both CPU copies of
//! that math live in this crate: the retouch ops' `coverage()`
//! ([`crate::retouch::smudge_dab_masked`], [`crate::retouch::clone_dab_masked`])
//! and the mask stamp below. The GPU copy is the paint shader
//! (`umber_gpu::shaders::PAINT_COMPUTE_SHADER`, a per-dab `mask_gate` flag
//! and an R8 mask texture). The model is pure and GPU-free, so every rule
//! here is testable headless. `umber-app`'s `PaintState` owns the instance,
//! uploads it to each tile's GPU mask, and routes strokes.
//!
//! Convention: `1.0` = paint allowed, `0.0` = blocked. Painting the mask
//! with [`MaskPaint::Allow`] (white) pulls texels toward `1.0`;
//! [`MaskPaint::Block`] (black) pulls them toward `0.0`. Both go through
//! the brush's own coverage (`alpha * falloff`), so a soft brush paints a
//! soft mask edge. The UV view tints blocked texels red (`1 - value`).
//!
//! Two flags, not one (a named deviation from the row's single `active`):
//!
//! * `editing` (mask mode, the X shortcut): strokes paint the MASK; the
//!   layer is untouched and nothing is gated.
//! * `active` (gating): strokes paint the layer, gated by the mask.
//!
//! The spec asks for both "painting in mask mode edits the mask" and "the
//! mask gates painting while active", and those can only hold at the same
//! time for different strokes. So gating applies when `active && !editing`
//! ([`QuickMask::gates`]). Entering mask mode arms `active`, so the usual
//! loop works: X, paint the mask, X, paint the layer through it.
//! Deactivating ([`QuickMask::set_active`]`(false)`) stops gating but keeps
//! the data for the session. Re-entering mask mode or re-activating shows
//! and gates the same mask again.
//!
//! Empty mask: a fresh mask and a cleared one are the same thing, all
//! `0.0`. An active empty mask blocks everything, and the full red overlay
//! shows it. An inverted empty mask allows everything. [`QuickMask::clear`]
//! zeroes the data and resets the inversion.
//!
//! Invert flips the stored values in place (`v -> 1 - v`), so there is
//! one sample path and what the overlay shows is what gates. The
//! `inverted` flag records the parity so that tile planes created later
//! start at the inverted empty value (`1.0`), and so the panel can show
//! the state.
//!
//! Per tile: one plane per UDIM tile, each the paint target's size
//! (`width * height`, row-major, texel rows top-down like the target).
//! Planes are allocated on first touch, so a session that never masks
//! pays nothing; an 8K plane is 256 MB of `f32`. Resizing the targets
//! resets the mask, as it resets their contents.
//!
//! The GPU copy is 8-bit (`round(v * 255)`), so the shader's gate differs
//! from the f32 CPU gate by at most 1/510 at soft mask edges. `0.0` and
//! `1.0` are exact in both. The fill-coverage hook
//! (`symmetry::symmetrize_coverage`) is not gated yet, because no fill op
//! exists to call it (follow-up with the fill layer).
//!
//! Not persisted, not undoable: the mask is temporary by definition. It is
//! session state outside the project file, the document's layer journal
//! (`umber-app`'s `document`), and the retouch stroke history. A mask
//! stroke is not an undo entry, and undoing a gated paint stroke leaves
//! the mask alone.

use std::collections::BTreeMap;

use crate::retouch::{coverage, footprint, DirtyRect, RetouchDab};

/// What a mask-mode stroke paints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MaskPaint {
    /// White: toward `1.0`, paint allowed.
    #[default]
    Allow,
    /// Black: toward `0.0`, paint blocked.
    Block,
}

impl MaskPaint {
    /// The mask value this paint pulls texels toward.
    pub fn value(self) -> f32 {
        match self {
            Self::Allow => 1.0,
            Self::Block => 0.0,
        }
    }
}

/// A borrowed view of one tile's mask plane: what the gated ops multiply
/// their per-texel coverage by.
#[derive(Debug, Clone, Copy)]
pub struct MaskView<'a> {
    width: u32,
    height: u32,
    coverage: &'a [f32],
}

impl<'a> MaskView<'a> {
    /// Wraps `coverage`; `None` when its length is not `width * height`.
    pub fn new(width: u32, height: u32, coverage: &'a [f32]) -> Option<Self> {
        (coverage.len() == width as usize * height as usize).then_some(Self {
            width,
            height,
            coverage,
        })
    }

    /// The mask at `(x, y)`; `0.0` (blocked) off the plane, so a size
    /// mismatch can only block paint, never leak it.
    pub fn at(&self, x: u32, y: u32) -> f32 {
        if x < self.width && y < self.height {
            self.coverage[y as usize * self.width as usize + x as usize]
        } else {
            0.0
        }
    }
}

/// The session's quick mask: per-tile coverage planes plus the mode flags
/// (see the module docs for the semantics).
#[derive(Debug, Clone)]
pub struct QuickMask {
    /// Plane size, texels (the paint targets' size).
    width: u32,
    height: u32,
    /// One `width * height` plane per touched UDIM tile, `1.0` = allowed.
    coverage: BTreeMap<u16, Vec<f32>>,
    /// Gating armed (applies while not `editing`).
    active: bool,
    /// Invert parity: fresh planes start at `1.0` while set.
    inverted: bool,
    /// Mask mode: strokes paint the mask.
    editing: bool,
    /// What mask-mode strokes paint.
    paint: MaskPaint,
    /// Bumped on every change a viewer might draw (overlay cache key).
    generation: u64,
}

impl QuickMask {
    /// An empty, inactive mask with `width`x`height` planes.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            coverage: BTreeMap::new(),
            active: false,
            inverted: false,
            editing: false,
            paint: MaskPaint::default(),
            generation: 0,
        }
    }

    /// Plane size, texels.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Whether gating is armed.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Arms or disarms gating. Disarming keeps the data (the row's
    /// "exit": the mask stops gating, re-activating gates the same mask).
    pub fn set_active(&mut self, active: bool) {
        if self.active != active {
            self.active = active;
            self.generation += 1;
        }
    }

    /// Whether mask mode is on (strokes paint the mask).
    pub fn is_editing(&self) -> bool {
        self.editing
    }

    /// Enters or leaves mask mode. Entering arms gating too, so leaving
    /// mask mode paints through what was just painted.
    pub fn set_editing(&mut self, editing: bool) {
        if self.editing != editing {
            self.editing = editing;
            self.active |= editing;
            self.generation += 1;
        }
    }

    /// Flips mask mode (the X shortcut); returns the new state.
    pub fn toggle_editing(&mut self) -> bool {
        self.set_editing(!self.editing);
        self.editing
    }

    /// Whether layer strokes are gated right now: armed and not in mask
    /// mode.
    pub fn gates(&self) -> bool {
        self.active && !self.editing
    }

    /// Whether the UV view draws the overlay (mask mode, or gating armed).
    pub fn shows_overlay(&self) -> bool {
        self.active || self.editing
    }

    /// The invert parity (see the module docs).
    pub fn is_inverted(&self) -> bool {
        self.inverted
    }

    /// What mask-mode strokes paint.
    pub fn paint(&self) -> MaskPaint {
        self.paint
    }

    /// Sets what mask-mode strokes paint.
    pub fn set_paint(&mut self, paint: MaskPaint) {
        self.paint = paint;
    }

    /// The change counter (viewers rebuild their overlay when it moves).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The value of a texel no stroke has touched: `0.0`, or `1.0` while
    /// inverted.
    pub fn empty_value(&self) -> f32 {
        if self.inverted {
            1.0
        } else {
            0.0
        }
    }

    /// `tile`'s plane, or `None` while untouched (it reads as
    /// [`Self::empty_value`] everywhere).
    pub fn coverage(&self, tile: u16) -> Option<&[f32]> {
        self.coverage.get(&tile).map(Vec::as_slice)
    }

    /// The mask at `(x, y)` on `tile` (untouched planes read empty; off
    /// the plane reads blocked, like [`MaskView::at`]).
    pub fn sample(&self, tile: u16, x: u32, y: u32) -> f32 {
        match self.view(tile) {
            Some(view) => view.at(x, y),
            None if x < self.width && y < self.height => self.empty_value(),
            None => 0.0,
        }
    }

    /// Allocates `tile`'s plane (at the empty value) if it has none yet,
    /// so [`Self::view`] returns it.
    pub fn ensure_plane(&mut self, tile: u16) {
        self.plane_mut(tile);
    }

    /// `tile`'s plane as a gate view; `None` while untouched (call
    /// [`Self::ensure_plane`] first to gate an untouched tile).
    pub fn view(&self, tile: u16) -> Option<MaskView<'_>> {
        self.coverage
            .get(&tile)
            .and_then(|plane| MaskView::new(self.width, self.height, plane))
    }

    /// Paints one dab into `tile`'s mask: every covered texel moves toward
    /// the [`MaskPaint`] value by the dab's coverage (`alpha * falloff`,
    /// the paint shader's footprint and falloff), so a hard full-alpha dab
    /// sets its disc exactly. Returns the rect it touched, or `None` when
    /// the dab covers nothing.
    pub fn stamp(&mut self, tile: u16, dab: &RetouchDab) -> Option<DirtyRect> {
        let target = self.paint.value();
        let width = self.width as usize;
        let rect = footprint(dab.pos, dab.radius, self.width, self.height)?;
        let plane = self.plane_mut(tile);
        let mut wrote = false;
        for y in rect.y0..rect.y1 {
            for x in rect.x0..rect.x1 {
                let Some(cov) = coverage(dab, x, y) else {
                    continue;
                };
                let cov = cov.clamp(0.0, 1.0);
                if cov <= 0.0 {
                    continue;
                }
                let m = &mut plane[y as usize * width + x as usize];
                *m += (target - *m) * cov;
                wrote = true;
            }
        }
        if wrote {
            self.generation += 1;
        }
        wrote.then_some(rect)
    }

    /// Flips the mask (`v -> 1 - v` on every plane, and the parity that
    /// untouched planes read): what was blocked paints, and vice versa.
    pub fn invert(&mut self) {
        for plane in self.coverage.values_mut() {
            for v in plane.iter_mut() {
                *v = 1.0 - *v;
            }
        }
        self.inverted = !self.inverted;
        self.generation += 1;
    }

    /// Zeroes the mask: every plane to `0.0` and the invert parity reset,
    /// back to the fresh (empty) state. The mode flags are kept.
    pub fn clear(&mut self) {
        for plane in self.coverage.values_mut() {
            plane.fill(0.0);
        }
        self.inverted = false;
        self.generation += 1;
    }

    /// Re-sizes the planes to `width`x`height`, dropping their contents
    /// (the paint targets' resize drops theirs too) and the invert parity.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.coverage.clear();
        self.inverted = false;
        self.generation += 1;
    }

    /// `rect` of `tile`'s mask quantized to 8 bits (`255` = allowed),
    /// tightly packed row-major: the GPU mask upload payload. Untouched
    /// planes read empty.
    pub fn gpu_bytes(&self, tile: u16, rect: DirtyRect) -> Vec<u8> {
        let view = self.view(tile);
        let empty = self.empty_value();
        let mut out = Vec::with_capacity(rect.width() as usize * rect.height() as usize);
        for y in rect.y0..rect.y1 {
            for x in rect.x0..rect.x1 {
                let v = view.map_or(empty, |view| view.at(x, y)).clamp(0.0, 1.0);
                out.push((v * 255.0).round() as u8);
            }
        }
        out
    }

    /// `tile`'s plane, allocated at the empty value on first touch.
    fn plane_mut(&mut self, tile: u16) -> &mut Vec<f32> {
        let len = self.width as usize * self.height as usize;
        let empty = self.empty_value();
        self.coverage
            .entry(tile)
            .or_insert_with(|| vec![empty; len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: u32 = 64;
    const TILE: u16 = 1001;

    fn hard_dab(pos: [f32; 2], radius: f32) -> RetouchDab {
        RetouchDab {
            pos,
            radius,
            alpha: 1.0,
            hardness: 1.0,
        }
    }

    #[test]
    fn fresh_mask_is_empty_inactive_and_unallocated() {
        let mask = QuickMask::new(SIZE, SIZE);
        assert!(!mask.is_active() && !mask.is_editing() && !mask.gates());
        assert!(!mask.shows_overlay());
        assert!(mask.coverage(TILE).is_none(), "no plane until touched");
        assert!(mask.view(TILE).is_none());
        assert_eq!(mask.sample(TILE, 10, 10), 0.0, "empty reads blocked");
        assert_eq!(mask.empty_value(), 0.0);
    }

    #[test]
    fn mask_mode_arms_gating_which_applies_after_exit() {
        let mut mask = QuickMask::new(SIZE, SIZE);
        assert!(mask.toggle_editing());
        assert!(mask.is_active(), "entering mask mode arms gating");
        assert!(!mask.gates(), "mask-mode strokes edit the mask, ungated");
        assert!(mask.shows_overlay());
        assert!(!mask.toggle_editing());
        assert!(mask.gates(), "leaving mask mode paints through the mask");
        mask.set_active(false);
        assert!(!mask.gates() && !mask.shows_overlay());
    }

    /// Test (c), model half: mask-mode painting moves the mask toward the
    /// paint's value by the brush coverage. White sets a hard disc to 1,
    /// black takes it back, and texels outside the disc never move.
    #[test]
    fn stamping_paints_white_up_and_black_down_inside_the_disc_only() {
        let mut mask = QuickMask::new(SIZE, SIZE);
        let g0 = mask.generation();
        let rect = mask
            .stamp(TILE, &hard_dab([32.0, 32.0], 8.0))
            .expect("covers texels");
        assert!(rect.x0 <= 24 && rect.x1 >= 40, "{rect:?}");
        assert!(mask.generation() > g0);
        assert_eq!(mask.sample(TILE, 32, 32), 1.0, "white allows");
        assert_eq!(mask.sample(TILE, 2, 2), 0.0, "outside untouched");
        assert_eq!(mask.sample(TILE, 32, 45), 0.0, "just past the radius");

        mask.set_paint(MaskPaint::Block);
        mask.stamp(TILE, &hard_dab([32.0, 32.0], 4.0));
        assert_eq!(mask.sample(TILE, 32, 32), 0.0, "black blocks again");
        assert_eq!(
            mask.sample(TILE, 32, 38),
            1.0,
            "outside the smaller disc stays"
        );

        // A half-alpha white dab moves halfway.
        mask.set_paint(MaskPaint::Allow);
        let half = RetouchDab {
            alpha: 0.5,
            ..hard_dab([32.0, 32.0], 2.0)
        };
        mask.stamp(TILE, &half);
        assert!((mask.sample(TILE, 32, 32) - 0.5).abs() < 1e-6);

        // A dab entirely off the plane touches nothing.
        assert_eq!(mask.stamp(TILE, &hard_dab([-50.0, 32.0], 4.0)), None);
    }

    /// Test (b), model half: invert flips every value, and a tile touched
    /// for the first time after the flip starts allowed.
    #[test]
    fn invert_flips_existing_and_fresh_planes() {
        let mut mask = QuickMask::new(SIZE, SIZE);
        mask.stamp(TILE, &hard_dab([16.0, 16.0], 4.0));
        assert_eq!(mask.sample(TILE, 16, 16), 1.0);
        assert_eq!(mask.sample(TILE, 48, 48), 0.0);
        mask.invert();
        assert!(mask.is_inverted());
        assert_eq!(mask.sample(TILE, 16, 16), 0.0);
        assert_eq!(mask.sample(TILE, 48, 48), 1.0);
        assert_eq!(
            mask.sample(1002, 5, 5),
            1.0,
            "untouched tile reads inverted-empty"
        );
        mask.ensure_plane(1002);
        assert_eq!(mask.coverage(1002).unwrap()[0], 1.0);
        mask.invert();
        assert!(!mask.is_inverted());
        assert_eq!(mask.sample(TILE, 16, 16), 1.0, "double invert is identity");
        assert_eq!(mask.sample(1002, 5, 5), 0.0);
    }

    /// Test (e): leaving mask mode and deactivating keep the data;
    /// re-entering shows and gates the same mask.
    #[test]
    fn mask_survives_exit_and_re_enter() {
        let mut mask = QuickMask::new(SIZE, SIZE);
        mask.set_editing(true);
        mask.stamp(TILE, &hard_dab([20.0, 20.0], 6.0));
        let painted = mask.coverage(TILE).unwrap().to_vec();
        mask.set_editing(false);
        mask.set_active(false);
        assert!(!mask.gates());
        assert_eq!(mask.coverage(TILE).unwrap(), painted.as_slice());
        mask.set_editing(true);
        assert_eq!(mask.coverage(TILE).unwrap(), painted.as_slice());
        mask.set_editing(false);
        assert!(mask.gates(), "re-entering re-arms the same mask");
        assert_eq!(mask.coverage(TILE).unwrap(), painted.as_slice());
    }

    /// Test (f): clear zeroes every plane and the invert parity; the
    /// modes stay.
    #[test]
    fn clear_zeroes() {
        let mut mask = QuickMask::new(SIZE, SIZE);
        mask.set_editing(true);
        mask.stamp(TILE, &hard_dab([20.0, 20.0], 6.0));
        mask.invert();
        mask.clear();
        assert!(mask.coverage(TILE).unwrap().iter().all(|&v| v == 0.0));
        assert!(!mask.is_inverted());
        assert_eq!(mask.sample(1002, 1, 1), 0.0, "fresh planes are empty again");
        assert!(mask.is_editing() && mask.is_active());
    }

    #[test]
    fn resize_drops_planes_and_gpu_bytes_quantize() {
        let mut mask = QuickMask::new(SIZE, SIZE);
        mask.stamp(TILE, &hard_dab([8.0, 8.0], 3.0));
        let rect = DirtyRect {
            x0: 7,
            y0: 8,
            x1: 10,
            y1: 9,
        };
        let half = RetouchDab {
            alpha: 0.5,
            ..hard_dab([30.0, 30.0], 1.0)
        };
        mask.stamp(TILE, &half);
        assert_eq!(mask.gpu_bytes(TILE, rect), vec![255, 255, 255]);
        let one = DirtyRect {
            x0: 30,
            y0: 30,
            x1: 31,
            y1: 31,
        };
        assert_eq!(mask.gpu_bytes(TILE, one), vec![128]);
        // Untouched tiles upload the empty value.
        assert_eq!(mask.gpu_bytes(1002, one), vec![0]);

        mask.resize(32, 16);
        assert_eq!(mask.dimensions(), (32, 16));
        assert!(mask.coverage(TILE).is_none());
        mask.ensure_plane(TILE);
        assert_eq!(mask.coverage(TILE).unwrap().len(), 32 * 16);
    }

    #[test]
    fn view_checks_length_and_blocks_off_plane() {
        assert!(MaskView::new(4, 4, &[1.0; 15]).is_none());
        let plane = [1.0; 16];
        let view = MaskView::new(4, 4, &plane).expect("16 = 4 x 4");
        assert_eq!(view.at(3, 3), 1.0);
        assert_eq!(view.at(4, 0), 0.0, "off the plane blocks");
    }
}
