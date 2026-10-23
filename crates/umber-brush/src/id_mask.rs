//! ID-mask selection (C1 worklist #11): click a mesh region, keep only its
//! baked ID paintable.
//!
//! The hard-surface gesture: the artist clicks a mesh region; the baked ID
//! map (`umber_bake::id` — `bake_id_mesh`, FNV-1a region colors as RGBA8) resolves which region was
//! clicked; [`select_id`] then fills the active set's [`QuickMask`] plane
//! with `1.0` where the bake's ID equals the clicked texel's ID and `0.0`
//! elsewhere, and arms gating. The existing mask plumbing (the
//! `stroke_mask_gate` snapshot, `WriteMask` sync, mask-gated dabs — see
//! `umber-app`'s `paint_state` module docs) does the actual gating; this
//! module only fills the coverage and activates. `LayerMask`
//! (`umber-core`'s `layers`) stays metadata-only — untouched by design.
//!
//! # Judgment calls (v1 boundaries)
//!
//! * **Bytes in, not a bake call.** [`select_id`] takes the baked RGBA8
//!   bytes as a parameter rather than baking itself: the brush crate is
//!   GPU-free and headless-testable, and no persistent ID-bake store exists
//!   in the app yet (nothing calls `bake_id_mesh` outside its own tests).
//!   The shell bakes on click (`PaintState::bake_and_select_id`) and hands
//!   the bytes down. A cached bake (bake on tool-entry, re-bake on mesh
//!   change) is the named follow-up — today's path re-bakes the full mask
//!   size on every click.
//! * **Bake texel convention, not the eyedropper's.** The clicked texel is
//!   resolved with the bake's own texel-center, V-flipped mapping (`u =
//!   (x + 0.5) / width`, `v = 1 - (y + 0.5) / height` — see
//!   `umber_bake::position`'s docs, which the ID pass shares): `x =
//!   round(u * w - 0.5)`, `y = round((1 - v) * h - 0.5)`, clamped into the
//!   frame. The eyedropper (`viewport.rs`) instead samples `uv * (w - 1)`
//!   corner-anchored; reusing it here would pick the neighbor texel along
//!   every region edge. Non-finite UV components read as `0.0` (a click can
//!   never produce a NaN, but the signature takes raw floats, so the floor
//!   is explicit rather than a saturating-cast accident).
//! * **Whole-ID equality, alpha included.** A texel is kept iff all four of
//!   its bytes equal the clicked texel's. Covered texels always carry alpha
//!   `255` and uncovered ones `(0, 0, 0, 0)`, so the alpha byte doubles as
//!   the coverage test — an uncovered click is refused
//!   ([`SelectIdError::ClickedUncovered`]) instead of selecting "the
//!   background", which would read as selecting nothing while arming the
//!   gate (an active all-zero mask blocks ALL paint).
//! * **Absolute write, parity reset.** The fill goes through
//!   [`QuickMask::replace_plane`]: exact `1.0`/`0.0` (no falloff — region
//!   edges are texel-exact by construction), replacing any previous mask
//!   content, and resetting the invert parity like `clear`/`resize` do, so
//!   a later-created tile plane agrees with the selected one.
//! * **Single tile.** Only `tile`'s plane is filled. The shell routes the
//!   click and, on a multi-tile mesh, supplies a per-tile bake — that tile's
//!   triangles with UVs rebased onto `0..1`
//!   (`umber_bake::ao::filter_mesh_for_tile`) — plus a click UV rebased by
//!   the same integer tile offset, so this module stays a pure
//!   `0..1`-frame mapping. An out-of-frame UV still clamps to the frame edge
//!   here ([`select_id`]'s contract); the shell never hands one down.
//! * **Refused mid-stroke is the caller's rule.** This module knows no
//!   strokes; `PaintState::select_id_mask` refuses while `stroking` (the
//!   live stroke snapshotted its gate at `begin_stroke`).

use crate::quick_mask::QuickMask;

/// Errors from [`select_id`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SelectIdError {
    /// The bake byte slice is empty — there is no texel to click.
    #[error("id bake is empty")]
    EmptyBake,
    /// `bake.len()` is not `width * height * 4`.
    #[error("id bake length {got} is not {width}x{height} RGBA8")]
    SizeMismatch {
        /// The declared bake width.
        width: u32,
        /// The declared bake height.
        height: u32,
        /// The actual byte length.
        got: usize,
    },
    /// The bake frame is not the mask's size — a fill needs exact texel
    /// correspondence, never a resample (resampling would blur region
    /// edges; the shell bakes at the mask size instead).
    #[error("id bake {bake_width}x{bake_height} does not match mask {mask_width}x{mask_height}")]
    DimensionsMismatch {
        /// The bake frame width.
        bake_width: u32,
        /// The bake frame height.
        bake_height: u32,
        /// The mask plane width.
        mask_width: u32,
        /// The mask plane height.
        mask_height: u32,
    },
    /// The clicked texel is uncovered (alpha `0`) — there is no region to
    /// select there.
    #[error("clicked texel ({x}, {y}) is uncovered (alpha 0)")]
    ClickedUncovered {
        /// Clicked texel column.
        x: u32,
        /// Clicked texel row.
        y: u32,
    },
}

/// Selects the baked ID under `uv`: fills `tile`'s plane of `mask` with
/// `1.0` where `bake`'s ID equals the clicked texel's ID and `0.0`
/// elsewhere, then arms gating (`set_active(true)`, leaving mask mode so
/// [`QuickMask::gates`] holds).
///
/// `bake` is the ID bake's `width * height * 4` RGBA8 bytes in row-major
/// order (what `umber_bake::id::bake_id_mesh` returns); `width`/`height`
/// the mask's dimensions. Returns the clicked texel's four bytes (the
/// selected region's color).
///
/// # Errors
///
/// Returns [`SelectIdError::EmptyBake`], [`SelectIdError::SizeMismatch`],
/// [`SelectIdError::DimensionsMismatch`], or
/// [`SelectIdError::ClickedUncovered`]; on any error the mask is untouched
/// (the clicked texel is resolved before any plane is written).
pub fn select_id(
    mask: &mut QuickMask,
    tile: u16,
    bake: &[u8],
    width: u32,
    height: u32,
    uv: [f32; 2],
) -> Result<[u8; 4], SelectIdError> {
    if bake.is_empty() {
        return Err(SelectIdError::EmptyBake);
    }
    if bake.len() != width as usize * height as usize * 4 {
        return Err(SelectIdError::SizeMismatch {
            width,
            height,
            got: bake.len(),
        });
    }
    let (mask_width, mask_height) = mask.dimensions();
    if (mask_width, mask_height) != (width, height) {
        return Err(SelectIdError::DimensionsMismatch {
            bake_width: width,
            bake_height: height,
            mask_width,
            mask_height,
        });
    }
    // The bake's texel-center, V-flipped mapping (see the module docs).
    // Width/height are nonzero here (an empty bake already returned, and a
    // zero width or height with a non-empty slice fails the length check).
    let u = finite_or_zero(uv[0]).clamp(0.0, 1.0);
    let v = finite_or_zero(uv[1]).clamp(0.0, 1.0);
    let x = ((u * width as f32 - 0.5).round()).clamp(0.0, width as f32 - 1.0) as u32;
    let y = (((1.0 - v) * height as f32 - 0.5).round()).clamp(0.0, height as f32 - 1.0) as u32;
    let i = (y as usize * width as usize + x as usize) * 4;
    let clicked = [bake[i], bake[i + 1], bake[i + 2], bake[i + 3]];
    if clicked[3] == 0 {
        return Err(SelectIdError::ClickedUncovered { x, y });
    }
    let mut plane = Vec::with_capacity(width as usize * height as usize);
    for px in bake.chunks_exact(4) {
        let same = px[0] == clicked[0]
            && px[1] == clicked[1]
            && px[2] == clicked[2]
            && px[3] == clicked[3];
        plane.push(f32::from(same));
    }
    // `replace_plane` only refuses on a length mismatch, and `plane` was
    // built at exactly `width * height` == the mask size (checked above).
    if !mask.replace_plane(tile, plane) {
        return Err(SelectIdError::DimensionsMismatch {
            bake_width: width,
            bake_height: height,
            mask_width,
            mask_height,
        });
    }
    mask.set_active(true);
    mask.set_editing(false);
    Ok(clicked)
}

/// `v` itself when finite, else `0.0` (see the module docs — NaN never
/// indexes a saturating cast into a surprise texel).
fn finite_or_zero(v: f32) -> f32 {
    if v.is_finite() {
        v
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: u32 = 8;
    const TILE: u16 = 1001;
    const ID_A: [u8; 4] = [10, 20, 30, 255];
    const ID_B: [u8; 4] = [40, 50, 60, 255];

    /// Two-ID fixture: the left half (`x < 4`) is region A, the right half
    /// region B — the headless mirror of the GPU bake's split-halves mesh
    /// (`umber_bake::id`'s `split_halves_regions_hash_to_their_triangle_index`).
    fn two_id_bake() -> Vec<u8> {
        let mut bake = vec![0u8; SIZE as usize * SIZE as usize * 4];
        for y in 0..SIZE {
            for x in 0..SIZE {
                let id = if x < SIZE / 2 { ID_A } else { ID_B };
                let i = (y * SIZE + x) as usize * 4;
                bake[i..i + 4].copy_from_slice(&id);
            }
        }
        bake
    }

    fn mask_at() -> QuickMask {
        QuickMask::new(SIZE, SIZE)
    }

    #[test]
    fn select_builds_exact_single_region_coverage_and_arms_gating() {
        let mut mask = mask_at();
        let bake = two_id_bake();
        // u = 0.25 -> x = round(0.25 * 8 - 0.5) = round(1.5) = 2, in A.
        let clicked = select_id(&mut mask, TILE, &bake, SIZE, SIZE, [0.25, 0.5])
            .expect("region A is covered");
        assert_eq!(clicked, ID_A);
        let plane = mask.coverage(TILE).expect("selection allocates the plane");
        assert_eq!(plane.len(), SIZE as usize * SIZE as usize);
        for y in 0..SIZE {
            for x in 0..SIZE {
                let want = if x < SIZE / 2 { 1.0 } else { 0.0 };
                assert_eq!(
                    plane[y as usize * SIZE as usize + x as usize],
                    want,
                    "texel ({x}, {y}): A-in / B-out exactly, no falloff"
                );
            }
        }
        assert!(mask.is_active());
        assert!(!mask.is_editing());
        assert!(mask.gates(), "the mask gates right after selection");
    }

    #[test]
    fn second_selection_replaces_the_first_exactly() {
        let mut mask = mask_at();
        let bake = two_id_bake();
        select_id(&mut mask, TILE, &bake, SIZE, SIZE, [0.25, 0.5]).expect("select A");
        let clicked = select_id(&mut mask, TILE, &bake, SIZE, SIZE, [0.75, 0.5]).expect("select B");
        assert_eq!(clicked, ID_B);
        let plane = mask.coverage(TILE).expect("plane");
        for y in 0..SIZE {
            for x in 0..SIZE {
                let want = if x < SIZE / 2 { 0.0 } else { 1.0 };
                assert_eq!(
                    plane[y as usize * SIZE as usize + x as usize],
                    want,
                    "texel ({x}, {y})"
                );
            }
        }
        assert!(mask.gates());
    }

    #[test]
    fn clicking_uncovered_is_an_error_and_leaves_the_mask_untouched() {
        let mut mask = mask_at();
        let mut bake = two_id_bake();
        // Punch an uncovered 2x2 hole at the top-left: u,v in [0, 0.25).
        for y in 0..2 {
            for x in 0..2 {
                let i = (y * SIZE + x) as usize * 4;
                bake[i..i + 4].copy_from_slice(&[0, 0, 0, 0]);
            }
        }
        // Can-fail guard: the fixture really has a hole where we click.
        assert_eq!(&bake[0..4], &[0, 0, 0, 0]);
        let err = select_id(&mut mask, TILE, &bake, SIZE, SIZE, [0.1, 0.9]).unwrap_err();
        assert_eq!(
            err,
            SelectIdError::ClickedUncovered { x: 0, y: 0 },
            "u=0.1 -> x=round(0.3)=0; v=0.9 -> y=round(0.3)=0"
        );
        assert!(mask.coverage(TILE).is_none(), "no plane on a refused click");
        assert!(!mask.is_active() && !mask.gates());
    }

    #[test]
    fn size_and_dimension_mismatches_are_errors() {
        let mut mask = mask_at();
        let bake = two_id_bake();
        let short = &bake[..bake.len() - 4];
        assert_eq!(
            select_id(&mut mask, TILE, short, SIZE, SIZE, [0.25, 0.5]).unwrap_err(),
            SelectIdError::SizeMismatch {
                width: SIZE,
                height: SIZE,
                got: short.len(),
            }
        );
        assert_eq!(
            select_id(&mut mask, TILE, &[], SIZE, SIZE, [0.25, 0.5]).unwrap_err(),
            SelectIdError::EmptyBake
        );
        // A well-formed bake at the wrong frame size: no resample, an error.
        let mut small = QuickMask::new(4, 4);
        assert_eq!(
            select_id(&mut small, TILE, &bake, SIZE, SIZE, [0.25, 0.5]).unwrap_err(),
            SelectIdError::DimensionsMismatch {
                bake_width: SIZE,
                bake_height: SIZE,
                mask_width: 4,
                mask_height: 4,
            }
        );
        assert!(mask.coverage(TILE).is_none() && small.coverage(TILE).is_none());
    }

    #[test]
    fn selection_resets_invert_parity_and_non_finite_uv_reads_texel_zero() {
        let mut mask = mask_at();
        mask.invert();
        assert!(mask.is_inverted());
        assert_eq!(mask.sample(TILE, 0, 0), 1.0, "inverted empty reads allowed");
        let bake = two_id_bake();
        // NaN floors to 0.0 (texel (0, 7): u=0 -> x=round(-0.5)=-1 -> 0;
        // v=0 -> y=round(7.5)=8 -> clamped to 7), region A.
        let clicked = select_id(&mut mask, TILE, &bake, SIZE, SIZE, [f32::NAN, f32::NAN])
            .expect("non-finite reads texel zero, covered here");
        assert_eq!(clicked, ID_A);
        assert!(
            !mask.is_inverted(),
            "selection resets the parity like clear"
        );
        assert_eq!(mask.sample(TILE, 0, 7), 1.0);
        assert_eq!(mask.sample(TILE, 7, 7), 0.0);
        assert!(mask.gates());
    }
}
