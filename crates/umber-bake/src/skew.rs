//! Skew correction for the bake engine (§6:57): the paintable skew map
//! plus edge protection.
//!
//! # What "skew" is here
//!
//! A per-texel deviation channel the baker emits alongside the map: the
//! angle (in degrees) between the surface's geometric normal and the
//! averaged unoccluded ray direction (the bent vector). On flat,
//! well-projected areas the rays leave along the normal and the deviation
//! is ~0°; on stretched UVs / grazing projections the hemisphere tilts
//! and the deviation grows. The map is grayscale RGBA8 (`rgb = deviation
//! / 90°`, `a` = coverage) — the same alpha convention every other baker
//! uses (uncovered `(0,0,0,0)`, covered `a = 255`).
//!
//! # The three pieces
//!
//! - Emission ([`skew_deviation_deg`] + [`skew_map_from_deviations`]):
//!   the deviation channel. The GPU pass (`umber_gpu::bake_shaders::
//!   SKEW_BAKE_SHADER`) reuses [`crate::ao`]'s deterministic stratified
//!   hemisphere sample set and Duff-et-al. tangent frame verbatim, so the
//!   skew direction agrees with the AO rays texel-for-texel.
//! - Paintable ([`combine_skew_with_paint`]): the skew map joins the
//!   paintable channel set — the artist brushes a mask over it, and
//!   `max(baked, painted)` wins per texel ("high skew = don't bake
//!   here"). The bridge lives in `umber-app::bake_sources`;
//!   this module owns the pure combine.
//! - Edge protection ([`apply_edge_protection`]): the guard in the bake
//!   pass — where the (combined) skew exceeds the threshold the guarded
//!   map's texel is clamped to its edge-safe neutral and counted. The
//!   count feeds the bake log's warnings column.
//!
//! # Affected maps + debounce (§6:58)
//!
//! [`BakeParam`] names the bake parameters the panel exposes;
//! [`param_dirties_map`] is the fixed table of which parameter change
//! invalidates which map, and [`AutoRebake`] is the 500 ms debounce
//! window over those dirty marks. All pure logic (headless-tested).

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use crate::BakeMap;

/// The skew threshold default: texels deviating more than 15° from the
/// geometric normal are edge-protected (Marmoset-class default range is
/// 10–20°; 15° is the midpoint).
pub const DEFAULT_SKEW_THRESHOLD_DEG: f32 = 15.0;
/// Minimum/maximum skew thresholds the slider allows.
pub const MIN_SKEW_THRESHOLD_DEG: f32 = 0.0;
/// Maximum skew threshold (90° = protection effectively off: no
/// deviation can exceed a right angle against its own normal).
pub const MAX_SKEW_THRESHOLD_DEG: f32 = 90.0;
/// The auto-rebake debounce window (§6:58): 500 ms after the last
/// parameter change the pending dirty set fires through the async queue.
pub const AUTOREBAKE_DEBOUNCE: Duration = Duration::from_millis(500);
/// The edge-safe neutral [`apply_edge_protection`] clamps AO texels to:
/// unoccluded white (never darkens a seam — the artifact skew causes).
pub const EDGE_SAFE_NEUTRAL: u8 = 255;

/// Bake parameters the panel exposes that can dirty a bake (§6:58).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BakeParam {
    /// UV-padding dilation width (the post-pass over every map).
    Dilation,
    /// Hemisphere ray count / ray length (`max_distance`): the raycast
    /// bakers' shared sample budget.
    RayLength,
    /// The skew edge-protection threshold.
    SkewThreshold,
    /// Output resolution (re-rasterizes everything).
    Resolution,
}

impl BakeParam {
    /// Every tunable parameter, for exhaustive tests.
    pub const ALL: [Self; 4] = [
        Self::Dilation,
        Self::RayLength,
        Self::SkewThreshold,
        Self::Resolution,
    ];
}

/// Whether a change to `param` invalidates `map` (§6:58's
/// "affected regions" at map granularity — the fixed table):
///
/// - Dilation / Resolution touch every map (post-pass over all outputs /
///   full re-raster).
/// - RayLength touches the two hemisphere-raycast bakers (AO, thickness)
///   only — curvature/position/ID never cast a ray.
/// - SkewThreshold touches the skew-guarded bakers (AO, normal) plus the
///   skew map itself — curvature/thickness/position/ID ignore skew.
pub fn param_dirties_map(param: BakeParam, map: BakeMap) -> bool {
    match param {
        BakeParam::Dilation | BakeParam::Resolution => true,
        BakeParam::RayLength => matches!(map, BakeMap::AmbientOcclusion | BakeMap::Thickness),
        BakeParam::SkewThreshold => matches!(
            map,
            BakeMap::AmbientOcclusion | BakeMap::Normal | BakeMap::Skew
        ),
    }
}

/// The maps `param` dirties, in [`BakeMap`] declaration order (the
/// per-map/per-set checklist intersects this with what is checked).
pub fn affected_maps_for_param(param: BakeParam) -> Vec<BakeMap> {
    const ORDER: [BakeMap; 9] = [
        BakeMap::AmbientOcclusion,
        BakeMap::Normal,
        BakeMap::Curvature,
        BakeMap::Position,
        BakeMap::Thickness,
        BakeMap::Id,
        BakeMap::Height,
        BakeMap::BentNormal,
        BakeMap::WorldSpaceNormal,
    ];
    // `BakeMap::Skew` is the tenth map (see below); order it after AO so
    // the skew-guarded pair reads together.
    let mut out: Vec<BakeMap> = ORDER
        .into_iter()
        .filter(|m| param_dirties_map(param, *m))
        .collect();
    if param_dirties_map(param, BakeMap::Skew) {
        out.insert(1, BakeMap::Skew);
    }
    out
}

/// The angular deviation between the geometric normal `normal` and the
/// averaged unoccluded ray direction `bent_dir` (both need not be unit —
/// they are normalized here), in degrees 0..=180.
///
/// Returns 0.0 when either vector is degenerate (zero length): no
/// direction exists to deviate from, so the texel reads as unskewed
/// rather than NaN.
pub fn skew_deviation_deg(normal: [f32; 3], bent_dir: [f32; 3]) -> f32 {
    let n_len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    let b_len = (bent_dir[0] * bent_dir[0]
        + bent_dir[1] * bent_dir[1]
        + bent_dir[2] * bent_dir[2])
        .sqrt();
    if n_len <= f32::EPSILON || b_len <= f32::EPSILON {
        return 0.0;
    }
    let dot = (normal[0] * bent_dir[0] + normal[1] * bent_dir[1] + normal[2] * bent_dir[2])
        / (n_len * b_len);
    dot.clamp(-1.0, 1.0).acos().to_degrees()
}

/// Encodes a deviation in degrees as a skew-map grayscale byte
/// (`deviation / 90°`, clamped — a deviation past a right angle saturates,
/// it cannot carry more "don't bake here" than full white).
pub fn skew_byte(deviation_deg: f32) -> u8 {
    ((deviation_deg.clamp(0.0, 90.0) / 90.0) * 255.0).round() as u8
}

/// Decodes a skew-map byte back to degrees (the inverse of [`skew_byte`],
/// within rounding).
pub fn skew_deg_from_byte(byte: u8) -> f32 {
    (byte as f32 / 255.0) * 90.0
}

/// The threshold in degrees as a skew-map byte (same encoding as
/// [`skew_byte`], so the guard compares bytes to bytes).
pub fn skew_threshold_byte(threshold_deg: f32) -> u8 {
    skew_byte(threshold_deg)
}

/// Emits a skew map from per-texel deviations: `deviations_deg` holds one
/// deviation per texel (row-major, `width × height`), `covered` one
/// coverage flag per texel. Covered texels encode [`skew_byte`] in rgb
/// with `a = 255`; uncovered texels are `(0,0,0,0)` — the bakers' shared
/// alpha convention.
pub fn skew_map_from_deviations(
    deviations_deg: &[f32],
    covered: &[bool],
    width: u32,
    height: u32,
) -> Vec<u8> {
    let texels = (width as usize) * (height as usize);
    debug_assert_eq!(deviations_deg.len(), texels);
    debug_assert_eq!(covered.len(), texels);
    let mut out = vec![0u8; texels * 4];
    for t in 0..texels {
        let Some(cov) = covered.get(t).copied() else {
            continue;
        };
        if !cov {
            continue;
        }
        let byte = skew_byte(deviations_deg.get(t).copied().unwrap_or(0.0));
        out[t * 4] = byte;
        out[t * 4 + 1] = byte;
        out[t * 4 + 2] = byte;
        out[t * 4 + 3] = 255;
    }
    out
}

/// Paintable skew (§6:57): the artist's brushed mask joins the baked map
/// — per texel, `max(baked, painted)` wins ("high skew = don't bake
/// here": painting white blocks the bake even where the rays measured
/// flat). Both maps are RGBA8 grayscale-with-coverage; uncovered texels
/// (either side `a == 0`) stay uncovered. Mismatched lengths fall back to
/// the baked map alone (never a panic, never a short read).
pub fn combine_skew_with_paint(baked: &[u8], painted: Option<&[u8]>) -> Vec<u8> {
    let Some(painted) = painted else {
        return baked.to_vec();
    };
    if painted.len() != baked.len() {
        return baked.to_vec();
    }
    let mut out = baked.to_vec();
    for (texel_out, texel_paint) in out.chunks_exact_mut(4).zip(painted.chunks_exact(4)) {
        if texel_out[3] == 0 || texel_paint[3] == 0 {
            continue;
        }
        let peak = texel_out[0].max(texel_paint[0]);
        texel_out[0] = peak;
        texel_out[1] = peak;
        texel_out[2] = peak;
    }
    out
}

/// Edge protection (§6:57): the guard in the bake pass. Where the
/// (paint-combined) `skew_map`'s grayscale exceeds `threshold_byte`, the
/// guarded `map`'s texel rgb is clamped to `neutral` (default
/// [`EDGE_SAFE_NEUTRAL`); alpha/coverage is preserved. Uncovered texels
/// on either side are skipped. Returns the clamped texel count — the bake
/// log's warnings column.
///
/// Mismatched lengths clamp nothing (returns 0): a corrupt skew map must
/// degrade to "no protection", never to a misaligned guard.
pub fn apply_edge_protection(
    map: &mut [u8],
    skew_map: &[u8],
    threshold_byte: u8,
    neutral: u8,
) -> usize {
    if map.len() != skew_map.len() {
        return 0;
    }
    let mut clamped = 0usize;
    for (texel, skew) in map.chunks_exact_mut(4).zip(skew_map.chunks_exact(4)) {
        if texel[3] == 0 || skew[3] == 0 {
            continue;
        }
        if skew[0] > threshold_byte {
            texel[0] = neutral;
            texel[1] = neutral;
            texel[2] = neutral;
            clamped += 1;
        }
    }
    clamped
}

/// The auto-rebake debounce (§6:58): collects dirty maps from parameter
/// changes and reports when the 500 ms window after the LAST change has
/// elapsed. Pure logic over injected `Instant`s — the panel drives it once
/// per frame and hands the fired set to the existing async bake queue.
#[derive(Debug, Clone, Default)]
pub struct AutoRebake {
    dirty: BTreeSet<BakeMap>,
    last_change: Option<Instant>,
}

impl AutoRebake {
    /// Empty tracker (nothing dirty, no pending fire).
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks the maps `param` dirties (the [`param_dirties_map`] table)
    /// at `now`. Every call re-arms the debounce window.
    pub fn mark_param_change(&mut self, param: BakeParam, now: Instant) {
        for map in affected_maps_for_param(param) {
            self.dirty.insert(map);
        }
        self.last_change = Some(now);
    }

    /// Whether any map is waiting for rebake.
    pub fn is_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }

    /// The currently-dirty maps, sorted (the `BTreeSet` order).
    pub fn dirty_maps(&self) -> Vec<BakeMap> {
        self.dirty.iter().copied().collect()
    }

    /// Whether the debounce window has elapsed at `now`
    /// (`last_change + 500ms <= now`) with dirty maps pending. The window
    /// re-arms on every change, so rapid slider drags fire once, 500 ms
    /// after the drag ends.
    pub fn rebake_due(&self, now: Instant) -> bool {
        match self.last_change {
            Some(last) => self.is_dirty() && now.duration_since(last) >= AUTOREBAKE_DEBOUNCE,
            None => false,
        }
    }

    /// Takes the pending dirty set (clears it and disarms the timer).
    /// Returns `None` when nothing is pending.
    pub fn take_dirty(&mut self) -> Option<BTreeSet<BakeMap>> {
        if self.dirty.is_empty() {
            return None;
        }
        self.last_change = None;
        Some(std::mem::take(&mut self.dirty))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_tilt_fixture_exceeds_the_threshold() {
        // A 30° tilt between the geometric normal (+Z) and the averaged
        // ray direction reads ~30° of skew — past the 15° default — and
        // encodes past the threshold byte.
        let tilt = 30.0f32.to_radians();
        let bent = [tilt.sin(), 0.0, tilt.cos()];
        let dev = skew_deviation_deg([0.0, 0.0, 1.0], bent);
        assert!((dev - 30.0).abs() < 1e-3, "a 30° tilt must read ~30° of skew, got {dev}");
        assert!(dev > DEFAULT_SKEW_THRESHOLD_DEG, "the fixture must exceed the default threshold");
        assert!(skew_byte(dev) > skew_threshold_byte(DEFAULT_SKEW_THRESHOLD_DEG));
        // Aligned rays read ~0° (no skew on flat projections).
        assert!(skew_deviation_deg([0.0, 0.0, 1.0], [0.0, 0.0, 1.0]) < 1e-3);
        // Degenerate inputs read 0, never NaN.
        assert_eq!(skew_deviation_deg([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]), 0.0);
        assert_eq!(skew_deviation_deg([0.0, 0.0, 1.0], [0.0, 0.0, 0.0]), 0.0);
    }

    #[test]
    fn skew_map_emission_follows_the_coverage_convention() {
        // 2x2: one flat texel, one tilted past threshold, one uncovered
        // with garbage deviation, one covered at exactly the threshold.
        let deviations = [0.0, 45.0, 90.0, DEFAULT_SKEW_THRESHOLD_DEG];
        let covered = [true, true, false, true];
        let map = skew_map_from_deviations(&deviations, &covered, 2, 2);
        assert_eq!(map.len(), 16);
        assert_eq!(&map[0..4], &[0, 0, 0, 255]);
        let tilted = skew_byte(45.0);
        assert_eq!(&map[4..8], &[tilted, tilted, tilted, 255]);
        assert!(tilted > skew_threshold_byte(DEFAULT_SKEW_THRESHOLD_DEG));
        assert_eq!(&map[8..12], &[0, 0, 0, 0]);
        let at_threshold = skew_threshold_byte(DEFAULT_SKEW_THRESHOLD_DEG);
        assert_eq!(&map[12..16], &[at_threshold, at_threshold, at_threshold, 255]);
    }

    #[test]
    fn edge_clamp_hits_only_texels_past_the_threshold() {
        let threshold = skew_threshold_byte(DEFAULT_SKEW_THRESHOLD_DEG);
        // AO map: dark occluded texels everywhere, all covered.
        let mut ao = vec![30u8, 30, 30, 255, 10, 10, 10, 255, 200, 200, 200, 255];
        let skew = vec![
            0, 0, 0, 255, // flat: keep
            200, 200, 200, 255, // past threshold: clamp
            0, 0, 0, 0, // uncovered skew: skip (map texel is covered but the guard has no reading)
        ];
        let clamped = apply_edge_protection(&mut ao, &skew, threshold, EDGE_SAFE_NEUTRAL);
        assert_eq!(clamped, 1);
        assert_eq!(&ao[0..4], &[30, 30, 30, 255]);
        assert_eq!(&ao[4..8], &[255, 255, 255, 255]);
        assert_eq!(&ao[8..12], &[200, 200, 200, 255]);
        // At-threshold is NOT past-threshold (strict `>`): no clamp.
        let mut single = vec![10u8, 10, 10, 255];
        let at = vec![threshold, threshold, threshold, 255];
        assert_eq!(apply_edge_protection(&mut single, &at, threshold, 255), 0);
        // Mismatched maps clamp nothing.
        assert_eq!(apply_edge_protection(&mut single, &[1u8; 3], threshold, 255), 0);
    }

    #[test]
    fn paint_combines_by_max_and_keeps_coverage() {
        let baked = vec![10u8, 10, 10, 255, 200, 200, 200, 255, 50, 50, 50, 0];
        let painted = vec![100u8, 100, 100, 255, 5, 5, 5, 255, 255, 255, 255, 255];
        let combined = combine_skew_with_paint(&baked, Some(&painted));
        // Painted higher wins; baked higher survives; uncovered stays.
        assert_eq!(&combined[0..4], &[100, 100, 100, 255]);
        assert_eq!(&combined[4..8], &[200, 200, 200, 255]);
        assert_eq!(&combined[8..12], &[50, 50, 50, 0]);
        // No paint (or a corrupt-length mask) passes the bake through.
        assert_eq!(combine_skew_with_paint(&baked, None), baked);
        assert_eq!(combine_skew_with_paint(&baked, Some(&[1u8; 4])), baked);
    }

    #[test]
    fn ray_length_dirties_raycast_maps_only() {
        // Param X (= RayLength) dirties {AO, Thickness}, NOT curvature /
        // position / skew — the affected-map set probe.
        let affected = affected_maps_for_param(BakeParam::RayLength);
        assert!(affected.contains(&BakeMap::AmbientOcclusion));
        assert!(affected.contains(&BakeMap::Thickness));
        for untouched in [
            BakeMap::Curvature,
            BakeMap::Position,
            BakeMap::Normal,
            BakeMap::Id,
            BakeMap::Skew,
        ] {
            assert!(
                !affected.contains(&untouched),
                "{untouched:?} must survive a ray-length change"
            );
        }
        // Dilation is the post-pass: it dirties everything.
        let all = affected_maps_for_param(BakeParam::Dilation);
        assert!(all.contains(&BakeMap::AmbientOcclusion));
        assert!(all.contains(&BakeMap::Curvature));
        assert!(all.contains(&BakeMap::Thickness));
        assert!(all.contains(&BakeMap::Position));
        // Skew threshold reaches the guarded bakers + the skew map, not
        // the unguarded ones.
        let skew = affected_maps_for_param(BakeParam::SkewThreshold);
        assert!(skew.contains(&BakeMap::AmbientOcclusion));
        assert!(skew.contains(&BakeMap::Normal));
        assert!(skew.contains(&BakeMap::Skew));
        assert!(!skew.contains(&BakeMap::Curvature));
        assert!(!skew.contains(&BakeMap::Thickness));
    }

    #[test]
    fn dirty_marking_and_the_debounce_window() {
        let start = Instant::now();
        let mut auto = AutoRebake::new();
        assert!(!auto.is_dirty());
        assert!(!auto.rebake_due(start));
        auto.mark_param_change(BakeParam::RayLength, start);
        assert!(auto.is_dirty());
        // Inside the 500 ms window: pending but not due.
        assert!(!auto.rebake_due(start + Duration::from_millis(499)));
        // At the window edge: due.
        assert!(auto.rebake_due(start + AUTOREBAKE_DEBOUNCE));
        // A second change re-arms the window from the LAST change.
        auto.mark_param_change(BakeParam::Dilation, start + Duration::from_millis(400));
        assert!(!auto.rebake_due(start + Duration::from_millis(500)));
        assert!(auto.rebake_due(start + Duration::from_millis(900)));
        // Taking clears the set and disarms the timer.
        let fired = auto.take_dirty().expect("dirty set fires");
        assert!(fired.contains(&BakeMap::AmbientOcclusion));
        assert!(!auto.is_dirty());
        assert!(!auto.rebake_due(start + Duration::from_secs(60)));
        assert!(auto.take_dirty().is_none());
    }
}
