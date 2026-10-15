//! Brush-level cavity mask (absorbed 2026-10-14 from Blender's texture-paint
//! cavity mask): restrict a stroke to the mesh's crevices — or to its ridges —
//! without first building a mask layer, by reading the BAKED curvature map.
//!
//! The curvature bake (`umber_bake::curvature`) encodes signed mean
//! curvature as `g = (curv + 1) / 2` per texel (MeshLab's convention:
//! convex = negative = darker, so ridges sit below 0.5 and cavities above).
//! [`CavityMask::gate`] turns one such texel into a `0..=1` paint weight:
//!
//! * [`CavityTarget::Cavities`]: full weight at and past `threshold + width`
//!   above flat, fading linearly to zero at `threshold` — paint pools in
//!   crevices (grime, dirt, ambient dust);
//! * [`CavityTarget::Ridges`]: the mirror — paint lands on convex edges
//!   (edge wear, chipped paint).
//!
//! `threshold` and `width` are in the signed-curvature domain (`0..=1`):
//! `threshold 0` with a small width gates "anything not flat". The output
//! is plain coverage, so it composes with the quick mask by multiplication
//! (the paint session's per-texel gate). Pure CPU and texture-agnostic: the
//! caller supplies curvature bytes; nothing here touches the GPU.

use serde::{Deserialize, Serialize};

/// Which side of flat the mask lets paint through.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CavityTarget {
    /// Concave texels (curvature above flat): crevices, creases, folds.
    #[default]
    Cavities,
    /// Convex texels (curvature below flat): edges, ridges, bevels.
    Ridges,
}

/// Cavity-mask parameters.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CavityMask {
    /// Which side of flat paints.
    pub target: CavityTarget,
    /// Signed curvature (`0..=1`) below which the target side paints
    /// nothing.
    pub threshold: f32,
    /// The fade band above `threshold` (`> 0`); full weight past
    /// `threshold + width`.
    pub width: f32,
    /// Flip the result (paint everywhere EXCEPT the target).
    #[serde(default)]
    pub invert: bool,
}

impl Default for CavityMask {
    /// Cavities, a soft gate starting just off flat.
    fn default() -> Self {
        Self {
            target: CavityTarget::Cavities,
            threshold: 0.05,
            width: 0.15,
            invert: false,
        }
    }
}

/// A rejected parameter set.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CavityMaskError {
    /// `threshold` must be finite and in `0..=1`.
    #[error("cavity threshold must be finite and in 0..=1, got {0}")]
    Threshold(f32),
    /// `width` must be finite and `> 0`.
    #[error("cavity width must be finite and > 0, got {0}")]
    Width(f32),
    /// The curvature buffer does not match `width * height` RGBA8 texels.
    #[error("curvature map is {actual} bytes, expected {expected} ({width}x{height} RGBA8)")]
    SizeMismatch {
        /// Byte length received.
        actual: usize,
        /// `width * height * 4`.
        expected: usize,
        /// Map width, texels.
        width: u32,
        /// Map height, texels.
        height: u32,
    },
}

impl CavityMask {
    /// Checks the parameters (call once when the brush changes).
    ///
    /// # Errors
    ///
    /// [`CavityMaskError::Threshold`] / [`CavityMaskError::Width`] for an
    /// out-of-range or non-finite value.
    pub fn validate(&self) -> Result<(), CavityMaskError> {
        if !self.threshold.is_finite() || !(0.0..=1.0).contains(&self.threshold) {
            return Err(CavityMaskError::Threshold(self.threshold));
        }
        if !self.width.is_finite() || self.width <= 0.0 {
            return Err(CavityMaskError::Width(self.width));
        }
        Ok(())
    }

    /// The paint weight for one curvature byte (`g` as baked: 128 ≈ flat).
    /// Assumes validated parameters; a non-finite intermediate gates 0
    /// (blocking is the safe failure — a mask never leaks paint).
    pub fn gate(&self, curvature_byte: u8) -> f32 {
        // Decode g -> signed curvature in -1..=1, then the target side's
        // magnitude (concave = positive).
        let signed = (f32::from(curvature_byte) / 255.0) * 2.0 - 1.0;
        let side = match self.target {
            CavityTarget::Cavities => signed,
            CavityTarget::Ridges => -signed,
        };
        let w = ((side - self.threshold) / self.width).clamp(0.0, 1.0);
        let w = if w.is_finite() { w } else { 0.0 };
        if self.invert {
            1.0 - w
        } else {
            w
        }
    }

    /// The gate over a whole baked curvature map (RGBA8, the R channel is
    /// read; the bake writes grayscale), as a `width * height` coverage
    /// plane in the quick mask's format (`1.0` = paint allowed). Uncovered
    /// texels (`a == 0`, outside every UV island) gate 0.
    ///
    /// # Errors
    ///
    /// The [`Self::validate`] errors, or [`CavityMaskError::SizeMismatch`].
    pub fn coverage(
        &self,
        rgba: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Vec<f32>, CavityMaskError> {
        self.validate()?;
        let expected = width as usize * height as usize * 4;
        if rgba.len() != expected {
            return Err(CavityMaskError::SizeMismatch {
                actual: rgba.len(),
                expected,
                width,
                height,
            });
        }
        Ok(rgba
            .chunks_exact(4)
            .map(|px| if px[3] == 0 { 0.0 } else { self.gate(px[0]) })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-5;

    /// The byte a signed curvature bakes to (`round((c + 1) / 2 * 255)`).
    fn byte(curv: f32) -> u8 {
        (((curv + 1.0) * 0.5) * 255.0).round() as u8
    }

    #[test]
    fn cavities_gate_concave_and_block_flat_and_convex() {
        let m = CavityMask {
            target: CavityTarget::Cavities,
            threshold: 0.2,
            width: 0.2,
            invert: false,
        };
        // Flat (128 ≈ +0.0039) and convex: below threshold -> 0.
        assert_eq!(m.gate(byte(0.0)), 0.0);
        assert_eq!(m.gate(byte(-0.8)), 0.0);
        // Past threshold + width (0.4): full weight.
        assert_eq!(m.gate(byte(0.8)), 1.0);
        assert_eq!(m.gate(255), 1.0);
        // Halfway through the band (curv 0.3 -> (0.3-0.2)/0.2 = 0.5), within
        // the byte quantization of the decode.
        let mid = m.gate(byte(0.3));
        assert!((mid - 0.5).abs() < 0.02, "{mid}");
    }

    #[test]
    fn ridges_mirror_cavities_exactly() {
        let cav = CavityMask::default();
        let ridge = CavityMask {
            target: CavityTarget::Ridges,
            ..cav
        };
        // g and 255-g are mirror images around flat: the decode maps them to
        // +c and -c exactly (1 - 2b/255 vs 2b/255 - 1), so the two targets
        // agree bit-for-bit on mirrored bytes.
        for b in 0u8..=255 {
            assert!(
                (cav.gate(b) - ridge.gate(255 - b)).abs() < EPS,
                "byte {b}: {} vs {}",
                cav.gate(b),
                ridge.gate(255 - b)
            );
        }
        // And they never both paint the same texel past the flat band.
        for b in 0u8..=255 {
            assert!(cav.gate(b) == 0.0 || ridge.gate(b) == 0.0, "byte {b}");
        }
    }

    #[test]
    fn invert_is_the_complement() {
        let m = CavityMask::default();
        let inv = CavityMask { invert: true, ..m };
        for b in [0u8, 64, 128, 170, 200, 255] {
            assert!((m.gate(b) + inv.gate(b) - 1.0).abs() < EPS);
        }
    }

    #[test]
    fn coverage_reads_r_skips_uncovered_and_checks_size() {
        let m = CavityMask {
            target: CavityTarget::Cavities,
            threshold: 0.0,
            width: 0.5,
            invert: false,
        };
        // Three texels: deep cavity (covered), deep cavity but UNCOVERED,
        // convex (covered).
        let rgba = [255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0, 255];
        let cov = m.coverage(&rgba, 3, 1).expect("valid");
        assert_eq!(cov, vec![1.0, 0.0, 0.0]);
        assert_eq!(
            m.coverage(&rgba[..8], 3, 1),
            Err(CavityMaskError::SizeMismatch {
                actual: 8,
                expected: 12,
                width: 3,
                height: 1
            })
        );
    }

    #[test]
    fn validation_rejects_bad_parameters() {
        let ok = CavityMask::default();
        assert!(ok.validate().is_ok());
        for t in [-0.1, 1.1, f32::NAN, f32::INFINITY] {
            let m = CavityMask { threshold: t, ..ok };
            assert!(
                matches!(m.validate(), Err(CavityMaskError::Threshold(_))),
                "{t}"
            );
        }
        for w in [0.0, -1.0, f32::NAN] {
            let m = CavityMask { width: w, ..ok };
            assert!(
                matches!(m.validate(), Err(CavityMaskError::Width(_))),
                "{w}"
            );
        }
        // coverage() refuses bad params before touching the buffer.
        let bad = CavityMask { width: 0.0, ..ok };
        assert!(bad.coverage(&[0; 4], 1, 1).is_err());
    }

    #[test]
    fn serde_round_trips_and_invert_defaults_false() {
        let m = CavityMask {
            target: CavityTarget::Ridges,
            threshold: 0.3,
            width: 0.1,
            invert: true,
        };
        let json = serde_json::to_string(&m).expect("serializes");
        assert!(json.contains("\"ridges\""), "{json}");
        let back: CavityMask = serde_json::from_str(&json).expect("parses");
        assert_eq!(back, m);
        let no_invert: CavityMask =
            serde_json::from_str(r#"{"target":"cavities","threshold":0.1,"width":0.2}"#)
                .expect("invert is optional");
        assert!(!no_invert.invert);
    }
}
