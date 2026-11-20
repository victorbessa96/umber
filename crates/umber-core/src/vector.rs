//! The live vector-path data model (K1 of the path-tool vector wave,
//! `razul-vector-model`, 2026-11-18 — brief
//! `docs/loop/briefs/2026-11-18-umber-vector-model.md`).
//!
//! A vector layer (`LayerKind::Vector`) carries ONE [`VectorShape`] in
//! `Layer::vector`: bezier anchors with in/out handles in UV space, a fill
//! color, and an optional stroke. K1 landed the model; K2's [`rasterize`]
//! fills closed paths and strokes open or closed paths into eval-owned
//! baseColor planes through the app's Fill pipeline. K5 owns SVG io.
//!
//! Serialization is additive (`#[serde(default)]` on the `Layer` field),
//! so projects saved before this row load with `vector == None`.

use serde::{Deserialize, Serialize};

/// One bezier anchor: position plus in/out control handles, all in UV
/// space (`0..1` over the active tile frame). A zero handle equals the
/// anchor position (a corner point).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub pos: [f32; 2],
    pub in_handle: [f32; 2],
    pub out_handle: [f32; 2],
}

impl Anchor {
    /// A corner anchor at `pos` (both handles collapsed onto the point).
    pub fn corner(pos: [f32; 2]) -> Self {
        Self {
            pos,
            in_handle: pos,
            out_handle: pos,
        }
    }
}

/// Stroke endpoint style. `Round` preserves the K2 raster exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StrokeCap {
    /// No cap beyond the endpoint.
    Butt,
    /// A semicircular cap centered on the endpoint.
    #[default]
    Round,
    /// A square cap extended by the stroke radius.
    Square,
}

/// Join style at a real direction change. `Round` preserves the K2 raster.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StrokeJoin {
    /// Extend the outer edges to their intersection, bounded by 4× radius.
    Miter,
    /// A semicircular join centered on the turn.
    #[default]
    Round,
    /// A flat bevel between the two outer edges.
    Bevel,
}

/// Maximum number of entries in a dash pattern. Fixed capacity keeps
/// `Stroke` copyable and bounded in serialized projects.
pub const MAX_DASH_LENGTHS: usize = 8;

/// A bounded SVG-style dash pattern in UV units. `len == 0` means solid.
/// Odd-length patterns are repeated by the rasterizer to preserve on/off
/// alternation, matching SVG.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct StrokeDash {
    #[serde(default)]
    pub lengths: [f32; MAX_DASH_LENGTHS],
    #[serde(default)]
    pub len: u8,
    #[serde(default)]
    pub offset: f32,
}

impl StrokeDash {
    pub fn solid() -> Self {
        Self::default()
    }

    pub fn is_solid(&self) -> bool {
        self.len == 0
    }

    pub fn lengths(&self) -> &[f32] {
        &self.lengths[..usize::from(self.len).min(MAX_DASH_LENGTHS)]
    }

    pub fn from_lengths(lengths: &[f32], offset: f32) -> Result<Self, VectorError> {
        if lengths.is_empty() {
            return Ok(Self::solid());
        }
        if lengths.len() > MAX_DASH_LENGTHS
            || lengths.iter().any(|v| !v.is_finite() || *v < 0.0)
            || lengths.iter().all(|v| *v == 0.0)
            || !offset.is_finite()
        {
            return Err(VectorError::InvalidDash);
        }
        let mut dash = Self {
            len: lengths.len() as u8,
            offset,
            ..Self::default()
        };
        dash.lengths[..lengths.len()].copy_from_slice(lengths);
        Ok(dash)
    }

    pub fn validate(&self) -> Result<(), VectorError> {
        if self.is_solid() {
            return Ok(());
        }
        if usize::from(self.len) > MAX_DASH_LENGTHS
            || self.lengths().iter().any(|v| !v.is_finite() || *v < 0.0)
            || self.lengths().iter().all(|v| *v == 0.0)
            || !self.offset.is_finite()
        {
            return Err(VectorError::InvalidDash);
        }
        Ok(())
    }
}

/// An optional stroke around the shape's outline.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    /// Stroke width in UV units.
    pub width: f32,
    /// sRGBA color.
    pub color: [u8; 4],
    /// Endpoint style. Additive: old projects default to Round.
    #[serde(default)]
    pub cap: StrokeCap,
    /// Join style at direction changes. Additive: old projects default to Round.
    #[serde(default)]
    pub join: StrokeJoin,
    /// Dash pattern. Additive: old projects default to solid.
    #[serde(default)]
    pub dash: StrokeDash,
}

/// The shape a vector layer holds. `anchors` in path order; `closed`
/// joins the last anchor back to the first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorShape {
    pub closed: bool,
    /// Fill color, sRGBA.
    pub fill: [u8; 4],
    /// Fill rule for closed paths. Additive: old projects default to nonzero.
    #[serde(default)]
    pub fill_rule: crate::svg::FillRule,
    pub stroke: Option<Stroke>,
    pub anchors: Vec<Anchor>,
}

impl VectorShape {
    /// A degenerate empty shape (no anchors): rasterizes to nothing.
    pub fn empty() -> Self {
        Self {
            closed: false,
            fill: [0, 0, 0, 0],
            fill_rule: crate::svg::FillRule::NonZero,
            stroke: None,
            anchors: Vec::new(),
        }
    }

    /// Whether the shape can cover any texel (K2's rasterizer gate).
    pub fn is_empty(&self) -> bool {
        self.anchors.len() < 2
    }
}

/// The vector layer's parameters, held in `Layer::vector` (`Some` exactly
/// for [`crate::layers::LayerKind::Vector`] layers).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorParams {
    pub shape: VectorShape,
}

impl Default for VectorParams {
    /// Defaults for a new editable path (K4): visible opaque white fill, no
    /// stroke, no anchors. `VectorShape::empty()` deliberately stays transparent
    /// for geometry/raster short-circuits; this default is for editor adoption.
    fn default() -> Self {
        Self {
            shape: VectorShape {
                fill: [255, 255, 255, 255],
                ..VectorShape::empty()
            },
        }
    }
}

fn f32_cache_eq(a: f32, b: f32) -> bool {
    a == b || (a.is_nan() && b.is_nan() && a.to_bits() == b.to_bits())
}

fn point_cache_eq(a: [f32; 2], b: [f32; 2]) -> bool {
    f32_cache_eq(a[0], b[0]) && f32_cache_eq(a[1], b[1])
}

fn anchor_cache_eq(a: &Anchor, b: &Anchor) -> bool {
    point_cache_eq(a.pos, b.pos)
        && point_cache_eq(a.in_handle, b.in_handle)
        && point_cache_eq(a.out_handle, b.out_handle)
}

fn stroke_cache_eq(a: Option<Stroke>, b: Option<Stroke>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            f32_cache_eq(a.width, b.width)
                && a.color == b.color
                && a.cap == b.cap
                && a.join == b.join
                && dash_cache_eq(a.dash, b.dash)
        }
        (None, None) => true,
        _ => false,
    }
}

impl VectorShape {
    /// Equality for evaluation-cache identity. Unlike derived `PartialEq`,
    /// identical NaN bits compare equal so an invalid project cannot look
    /// edited on every frame; ordinary float semantics (including ±0) stay.
    pub fn same_cache_key(&self, other: &Self) -> bool {
        self.closed == other.closed
            && self.fill == other.fill
            && self.fill_rule == other.fill_rule
            && stroke_cache_eq(self.stroke, other.stroke)
            && self.anchors.len() == other.anchors.len()
            && self
                .anchors
                .iter()
                .zip(&other.anchors)
                .all(|(a, b)| anchor_cache_eq(a, b))
    }
}

impl VectorParams {
    /// Cache-key equality for the complete live-vector parameter set.
    pub fn same_cache_key(&self, other: &Self) -> bool {
        self.shape.same_cache_key(&other.shape)
    }
}

/// The live-vector raster ceiling: the app's 8K maximum, inclusive.
pub const MAX_VECTOR_RASTER_PIXELS: usize = 8192 * 8192;
/// The most shapes one multi-path SVG import may produce.
pub const MAX_VECTOR_SHAPES: usize = 256;
/// The most anchors one live shape, or one multi-path import in total, may carry.
pub const MAX_VECTOR_ANCHORS: usize = 65_536;
/// The most flattened centerline points one live shape may carry.
pub const MAX_VECTOR_POINTS: usize = 262_144;
/// Worst-case edge visits across all four subsamples of every output row.
pub const MAX_VECTOR_EDGE_ROW_VISITS: u64 = 100_000_000;

/// Errors a live-vector raster can report before allocation or flattening.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VectorError {
    /// The requested raster exceeds [`MAX_VECTOR_RASTER_PIXELS`].
    #[error("vector raster {width}x{height} exceeds the {max_pixels}-pixel cap")]
    TooLarge {
        width: u32,
        height: u32,
        max_pixels: usize,
    },
    /// A structural budget refused the shape before rasterization.
    #[error("vector shape is too complex: more than {limit} {what}")]
    TooComplex { what: &'static str, limit: usize },
    /// A dash pattern is non-finite, negative, all-zero, or too long.
    #[error("invalid stroke dash pattern")]
    InvalidDash,
    /// The scanline's conservative edge × row × subsample cost is too high.
    #[error("vector raster needs {edges} edges across {rows} subsampled rows (limit {limit})")]
    TooExpensive { edges: usize, rows: u64, limit: u64 },
}

/// A rasterized live vector: straight RGB with alpha 255 wherever the
/// coverage is nonzero, and total source-over alpha per texel. The alpha
/// lives in `coverage` so the app's existing quick-mask and premultiply
/// pipeline performs the final quantization exactly once.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorRaster {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    coverage: Vec<f32>,
}

impl VectorRaster {
    fn empty(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            rgba: Vec::new(),
            coverage: Vec::new(),
        }
    }

    /// Raster width in texels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Raster height in texels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Total source-over alpha, row-major, `0.0..=1.0` per texel.
    pub fn coverage(&self) -> &[f32] {
        &self.coverage
    }

    /// Straight RGB with alpha 255 wherever `coverage` is nonzero.
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    /// Consumes the raster into the fill pipeline's straight-RGB + coverage pair.
    pub fn into_rgba_coverage(self) -> (Vec<u8>, Vec<f32>) {
        (self.rgba, self.coverage)
    }
}

fn device_point(p: [f32; 2], width: u32, height: u32) -> [f64; 2] {
    [
        f64::from(p[0]) * f64::from(width),
        f64::from(p[1]) * f64::from(height),
    ]
}

/// The path's flattened centerline in device px, shared by fill and stroke.
/// K3's UV overlay reads this exact geometry for its drag preview; it does not
/// allocate a second flattener. Any non-finite control coordinate invalidates
/// this shape's geometry: it contributes no coverage instead of becoming a
/// straight segment by accident.
pub fn centerline(
    shape: &VectorShape,
    width: u32,
    height: u32,
) -> Result<Vec<[f64; 2]>, VectorError> {
    let Some(first) = shape.anchors.first() else {
        return Ok(Vec::new());
    };
    if shape.anchors.iter().any(|anchor| {
        anchor
            .pos
            .iter()
            .chain(anchor.in_handle.iter())
            .chain(anchor.out_handle.iter())
            .any(|value| !value.is_finite())
    }) {
        return Ok(Vec::new());
    }
    let mut points = vec![device_point(first.pos, width, height)];
    let segments = if shape.closed {
        shape.anchors.len()
    } else {
        shape.anchors.len().saturating_sub(1)
    };
    for i in 0..segments {
        let a = &shape.anchors[i];
        let b = &shape.anchors[(i + 1) % shape.anchors.len()];
        let (p0, p1, p2, p3) = (
            device_point(a.pos, width, height),
            device_point(a.out_handle, width, height),
            device_point(b.in_handle, width, height),
            device_point(b.pos, width, height),
        );
        let added = crate::svg::cubic_device_point_count(p0, p1, p2, p3).saturating_sub(1);
        let next_len = points.len().saturating_add(added);
        if next_len > MAX_VECTOR_POINTS {
            return Err(VectorError::TooComplex {
                what: "path points",
                limit: MAX_VECTOR_POINTS,
            });
        }
        crate::svg::cubic_device_points_into(p0, p1, p2, p3, &mut points);
    }
    Ok(points)
}

fn stroke_polys(
    shape: &VectorShape,
    centerline: &[[f64; 2]],
    width: u32,
    height: u32,
) -> Result<Vec<Vec<[f64; 2]>>, VectorError> {
    let Some(stroke) = shape.stroke.filter(|s| {
        s.color[3] > 0 && s.width.is_finite() && s.width > 0.0 && shape.anchors.len() >= 2
    }) else {
        return Ok(Vec::new());
    };
    if !stroke.dash.is_solid() {
        return dashed_stroke_polys(shape, centerline, width, height, stroke);
    }
    let radius = f64::from(stroke.width) * f64::from(width.min(height)) * 0.5;
    let ellipse_cost = crate::svg::ellipse_point_count(radius, radius);
    let mut builder = crate::svg::PolyBuilder::new_device();
    let mut points = 0usize;
    for pair in centerline.windows(2) {
        if points.saturating_add(4) > MAX_VECTOR_POINTS {
            return Err(VectorError::TooComplex {
                what: "path points",
                limit: MAX_VECTOR_POINTS,
            });
        }
        let [a, b] = [pair[0], pair[1]];
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let length = dx.hypot(dy);
        if !length.is_finite() || length <= 0.0 {
            continue;
        }
        let (nx, ny) = (-dy / length * radius, dx / length * radius);
        let quad = [
            [a[0] + nx, a[1] + ny],
            [b[0] + nx, b[1] + ny],
            [b[0] - nx, b[1] - ny],
            [a[0] - nx, a[1] - ny],
        ];
        builder.move_to(quad[0]);
        for i in 1..quad.len() {
            builder.line_to(quad[i - 1], quad[i]);
        }
        points += 4;
    }
    // Cap style affects open endpoints only. A closed path has no endpoints,
    // so every cap keeps Round's join behavior, including the closure corner.
    let cap = if shape.closed {
        StrokeCap::Round
    } else {
        shape.stroke.map(|s| s.cap).unwrap_or_default()
    };
    let join = shape.stroke.map(|s| s.join).unwrap_or_default();
    for (index, &point) in centerline.iter().enumerate() {
        let turn = join_at(centerline, index, shape.closed);
        let endpoint = if shape.closed {
            false
        } else {
            index == 0 || index + 1 == centerline.len()
        };
        let ellipse = match (cap, join) {
            (_, StrokeJoin::Round) => cap == StrokeCap::Round || turn,
            (StrokeCap::Round, _) => endpoint && !turn,
            _ => false,
        };
        if ellipse {
            if points.saturating_add(ellipse_cost) > MAX_VECTOR_POINTS {
                return Err(VectorError::TooComplex {
                    what: "path points",
                    limit: MAX_VECTOR_POINTS,
                });
            }
            builder.ellipse(point, radius, radius);
            points += ellipse_cost;
        } else if turn && join != StrokeJoin::Round {
            let polygon = join_polygon(centerline, index, radius, join, shape.closed)?;
            if polygon.is_empty() {
                continue;
            }
            if points.saturating_add(polygon.len()) > MAX_VECTOR_POINTS {
                return Err(VectorError::TooComplex {
                    what: "path points",
                    limit: MAX_VECTOR_POINTS,
                });
            }
            builder.move_to(polygon[0]);
            for i in 1..polygon.len() {
                builder.line_to(polygon[i - 1], polygon[i]);
            }
            points += polygon.len();
        }
    }
    if cap == StrokeCap::Square && !shape.closed && centerline.len() >= 2 {
        let last = centerline.len() - 1;
        for (point, direction) in [
            (
                centerline[0],
                [
                    centerline[0][0] - centerline[1][0],
                    centerline[0][1] - centerline[1][1],
                ],
            ),
            (
                centerline[last],
                [
                    centerline[last][0] - centerline[last - 1][0],
                    centerline[last][1] - centerline[last - 1][1],
                ],
            ),
        ] {
            let length = direction[0].hypot(direction[1]);
            if length <= 0.0 || !length.is_finite() {
                continue;
            }
            let direction = [direction[0] / length, direction[1] / length];
            let normal = [-direction[1] * radius, direction[0] * radius];
            let extension = [direction[0] * radius, direction[1] * radius];
            let quad = [
                [point[0] + normal[0], point[1] + normal[1]],
                [
                    point[0] + extension[0] + normal[0],
                    point[1] + extension[1] + normal[1],
                ],
                [
                    point[0] + extension[0] - normal[0],
                    point[1] + extension[1] - normal[1],
                ],
                [point[0] - normal[0], point[1] - normal[1]],
            ];
            if points.saturating_add(4) > MAX_VECTOR_POINTS {
                return Err(VectorError::TooComplex {
                    what: "path points",
                    limit: MAX_VECTOR_POINTS,
                });
            }
            builder.move_to(quad[0]);
            for i in 1..quad.len() {
                builder.line_to(quad[i - 1], quad[i]);
            }
            points += 4;
        }
    }
    Ok(builder.finish())
}

#[derive(Debug, Clone)]
struct DashPattern {
    entries: Vec<(f64, bool)>,
    period: f64,
    offset: f64,
}

impl DashPattern {
    fn new(dash: StrokeDash, scale: f64) -> Result<Self, VectorError> {
        dash.validate()?;
        let lengths = dash.lengths();
        let repeats = if lengths.len() % 2 == 1 { 2 } else { 1 };
        let mut entries = Vec::with_capacity(lengths.len() * repeats);
        let mut period = 0.0;
        for index in 0..lengths.len() * repeats {
            let length = f64::from(lengths[index % lengths.len()]) * scale;
            if length > 0.0 {
                entries.push((length, index % 2 == 0));
                period += length;
            }
        }
        if entries.is_empty() || !period.is_finite() || period <= 0.0 {
            return Err(VectorError::InvalidDash);
        }
        Ok(Self {
            entries,
            period,
            offset: f64::from(dash.offset) * scale,
        })
    }

    /// Returns whether `distance` is on a dash and the next pattern boundary.
    fn state(&self, distance: f64) -> (bool, f64) {
        let mut position = (distance + self.offset).rem_euclid(self.period);
        for (length, on) in &self.entries {
            if position < *length {
                return (*on, distance + (*length - position));
            }
            position -= *length;
        }
        // Floating-point roundoff can only reach this fallback at a period
        // boundary. Refuse to invent coverage there and advance one period.
        (false, distance + self.period)
    }

    fn intervals(&self, start: f64, end: f64) -> Result<Vec<(f64, f64)>, VectorError> {
        let mut intervals: Vec<(f64, f64)> = Vec::new();
        let mut cursor = start;
        let mut steps = 0usize;
        while cursor < end {
            steps += 1;
            if steps > MAX_VECTOR_POINTS {
                return Err(VectorError::TooComplex {
                    what: "dash steps",
                    limit: MAX_VECTOR_POINTS,
                });
            }
            let (on, next) = self.state(cursor);
            let stop = next.min(end);
            if !stop.is_finite() || stop <= cursor {
                break;
            }
            if on {
                let local_start = cursor - start;
                let local_end = stop - start;
                if let Some(last) = intervals.last_mut() {
                    if (last.1 - local_start).abs() <= 1e-12 {
                        // A zero off-length is not a dash break. Merge the
                        // adjacent active intervals so no false cap appears.
                        last.1 = local_end;
                        cursor = stop;
                        continue;
                    }
                }
                if intervals.len() >= MAX_VECTOR_POINTS / 4 {
                    return Err(VectorError::TooComplex {
                        what: "dash segments",
                        limit: MAX_VECTOR_POINTS / 4,
                    });
                }
                intervals.push((local_start, local_end));
            }
            cursor = stop;
        }
        Ok(intervals)
    }
}

#[derive(Debug, Clone, Copy)]
struct DashPart {
    start: f64,
    end: f64,
    a: [f64; 2],
    b: [f64; 2],
    direction: [f64; 2],
}

fn push_stroke_polygon(
    builder: &mut crate::svg::PolyBuilder,
    points: &mut usize,
    polygon: &[[f64; 2]],
) -> Result<(), VectorError> {
    if polygon.is_empty() {
        return Ok(());
    }
    if points.saturating_add(polygon.len()) > MAX_VECTOR_POINTS {
        return Err(VectorError::TooComplex {
            what: "path points",
            limit: MAX_VECTOR_POINTS,
        });
    }
    builder.move_to(polygon[0]);
    for index in 1..polygon.len() {
        builder.line_to(polygon[index - 1], polygon[index]);
    }
    *points += polygon.len();
    Ok(())
}

fn push_stroke_ellipse(
    builder: &mut crate::svg::PolyBuilder,
    points: &mut usize,
    center: [f64; 2],
    radius: f64,
) -> Result<(), VectorError> {
    let count = crate::svg::ellipse_point_count(radius, radius);
    if points.saturating_add(count) > MAX_VECTOR_POINTS {
        return Err(VectorError::TooComplex {
            what: "path points",
            limit: MAX_VECTOR_POINTS,
        });
    }
    builder.ellipse(center, radius, radius);
    *points += count;
    Ok(())
}

/// Rasterizes a dashed stroke as bounded active intervals along the exact
/// flattened centerline. Each interval gets caps at actual dash breaks;
/// continuous turns get the selected join. Solid strokes keep the K6c path.
fn dashed_stroke_polys(
    shape: &VectorShape,
    centerline: &[[f64; 2]],
    width: u32,
    height: u32,
    stroke: Stroke,
) -> Result<Vec<Vec<[f64; 2]>>, VectorError> {
    let scale = f64::from(width.min(height));
    let pattern = DashPattern::new(stroke.dash, scale)?;
    let radius = f64::from(stroke.width) * scale * 0.5;
    let mut segments: Vec<Vec<DashPart>> = Vec::with_capacity(centerline.len().saturating_sub(1));
    let mut cumulative = Vec::with_capacity(centerline.len());
    cumulative.push(0.0);
    for pair in centerline.windows(2) {
        let [a, b] = [pair[0], pair[1]];
        let delta = [b[0] - a[0], b[1] - a[1]];
        let length = delta[0].hypot(delta[1]);
        let start = *cumulative.last().unwrap_or(&0.0);
        if !length.is_finite() || length <= 0.0 {
            segments.push(Vec::new());
            cumulative.push(start);
            continue;
        }
        let direction = [delta[0] / length, delta[1] / length];
        let mut parts = Vec::new();
        for (local_start, local_end) in pattern.intervals(start, start + length)? {
            let at = |distance: f64| {
                let t = distance / length;
                [a[0] + delta[0] * t, a[1] + delta[1] * t]
            };
            parts.push(DashPart {
                start: start + local_start,
                end: start + local_end,
                a: at(local_start),
                b: at(local_end),
                direction,
            });
        }
        segments.push(parts);
        cumulative.push(start + length);
    }

    let total = *cumulative.last().unwrap_or(&0.0);
    let continuous = |vertex: usize| {
        let count = segments.len();
        if count == 0 {
            return false;
        }
        let vertex = if shape.closed && vertex == count {
            0
        } else {
            vertex
        };
        if !shape.closed && (vertex == 0 || vertex >= count) {
            return false;
        }
        let previous = if vertex == 0 { count - 1 } else { vertex - 1 };
        let next = if vertex == count { 0 } else { vertex };
        let Some(previous) = segments[previous].last() else {
            return false;
        };
        let Some(next) = segments[next].first() else {
            return false;
        };
        if shape.closed && vertex == 0 {
            (previous.end - total).abs() <= 1e-9 && next.start.abs() <= 1e-9
        } else {
            let boundary = cumulative[vertex];
            (previous.end - boundary).abs() <= 1e-9 && (next.start - boundary).abs() <= 1e-9
        }
    };

    let mut builder = crate::svg::PolyBuilder::new_device();
    let mut points = 0usize;
    for (index, parts) in segments.iter().enumerate() {
        let segment_start = cumulative[index];
        let segment_end = cumulative[index + 1];
        for part in parts {
            let start_continuous = (part.start - segment_start).abs() <= 1e-9 && continuous(index);
            let end_continuous = (part.end - segment_end).abs() <= 1e-9 && continuous(index + 1);
            let mut a = part.a;
            let mut b = part.b;
            if stroke.cap == StrokeCap::Square {
                if !start_continuous {
                    a[0] -= part.direction[0] * radius;
                    a[1] -= part.direction[1] * radius;
                }
                if !end_continuous {
                    b[0] += part.direction[0] * radius;
                    b[1] += part.direction[1] * radius;
                }
            }
            let normal = [-part.direction[1] * radius, part.direction[0] * radius];
            let quad = [
                [a[0] + normal[0], a[1] + normal[1]],
                [b[0] + normal[0], b[1] + normal[1]],
                [b[0] - normal[0], b[1] - normal[1]],
                [a[0] - normal[0], a[1] - normal[1]],
            ];
            push_stroke_polygon(&mut builder, &mut points, &quad)?;
            if stroke.cap == StrokeCap::Round {
                if !start_continuous {
                    push_stroke_ellipse(&mut builder, &mut points, part.a, radius)?;
                }
                if !end_continuous {
                    push_stroke_ellipse(&mut builder, &mut points, part.b, radius)?;
                }
            }
        }
    }

    for (index, &point) in centerline.iter().enumerate() {
        if !continuous(index) || !join_at(centerline, index, shape.closed) {
            continue;
        }
        if stroke.join == StrokeJoin::Round {
            push_stroke_ellipse(&mut builder, &mut points, point, radius)?;
        } else {
            let polygon = join_polygon(centerline, index, radius, stroke.join, shape.closed)?;
            push_stroke_polygon(&mut builder, &mut points, &polygon)?;
        }
    }
    Ok(builder.finish())
}

fn dash_cache_eq(a: StrokeDash, b: StrokeDash) -> bool {
    a.len == b.len
        && f32_cache_eq(a.offset, b.offset)
        && a.lengths
            .iter()
            .zip(b.lengths.iter())
            .all(|(x, y)| f32_cache_eq(*x, *y))
}

/// One join polygon at `index`. Bevel uses a flat outer triangle; Miter
/// extends the outer edges to their intersection, falling back to bevel
/// beyond a 4×radius miter limit. Closed paths use the last real segment
/// as the previous segment for index 0, so the closure join is retained.
fn join_polygon(
    centerline: &[[f64; 2]],
    index: usize,
    radius: f64,
    join: StrokeJoin,
    closed: bool,
) -> Result<Vec<[f64; 2]>, VectorError> {
    if centerline.len() < 2 || index + 1 >= centerline.len() {
        return Ok(Vec::new());
    }
    let a = if index == 0 {
        if !closed {
            return Ok(Vec::new());
        }
        centerline[centerline.len() - 2]
    } else {
        centerline[index - 1]
    };
    let b = centerline[index];
    let c = centerline[index + 1];
    let prev = [b[0] - a[0], b[1] - a[1]];
    let next = [c[0] - b[0], c[1] - b[1]];
    let prev_len = prev[0].hypot(prev[1]);
    let next_len = next[0].hypot(next[1]);
    if prev_len <= 0.0 || next_len <= 0.0 {
        return Ok(Vec::new());
    }
    let prev_normal = [-prev[1] / prev_len, prev[0] / prev_len];
    let next_normal = [-next[1] / next_len, next[0] / next_len];
    let cross = prev[0] * next[1] - prev[1] * next[0];
    let outer_sign = if cross > 0.0 { -1.0 } else { 1.0 };
    let prev_outer = [
        b[0] + outer_sign * prev_normal[0] * radius,
        b[1] + outer_sign * prev_normal[1] * radius,
    ];
    let next_outer = [
        b[0] + outer_sign * next_normal[0] * radius,
        b[1] + outer_sign * next_normal[1] * radius,
    ];
    if join == StrokeJoin::Miter {
        let prev_start = [
            a[0] + outer_sign * prev_normal[0] * radius,
            a[1] + outer_sign * prev_normal[1] * radius,
        ];
        let next_end = [
            c[0] + outer_sign * next_normal[0] * radius,
            c[1] + outer_sign * next_normal[1] * radius,
        ];
        if let Some(miter) = line_intersection(prev_start, prev_outer, next_outer, next_end) {
            let distance = (miter[0] - b[0]).hypot(miter[1] - b[1]);
            if distance.is_finite() && distance <= 4.0 * radius {
                return Ok(vec![prev_outer, miter, next_outer, b]);
            }
        }
    }
    Ok(vec![prev_outer, next_outer, b])
}

/// Intersects two finite line segments' supporting lines. `None` for
/// parallel or non-finite intersections.
fn line_intersection(a0: [f64; 2], a1: [f64; 2], b0: [f64; 2], b1: [f64; 2]) -> Option<[f64; 2]> {
    let a = [a1[0] - a0[0], a1[1] - a0[1]];
    let b = [b1[0] - b0[0], b1[1] - b0[1]];
    let denominator = a[0] * b[1] - a[1] * b[0];
    if denominator.abs() <= 1e-12 {
        return None;
    }
    let t = ((b0[0] - a0[0]) * b[1] - (b0[1] - a0[1]) * b[0]) / denominator;
    let point = [a0[0] + t * a[0], a0[1] + t * a[1]];
    point.iter().all(|v| v.is_finite()).then_some(point)
}

/// Whether `index` is a genuine join rather than a collinear flattening
/// point. Non-round caps still get round joins at real turns, but never
/// get accidental caps from a collinear point near an endpoint.
fn join_at(centerline: &[[f64; 2]], index: usize, closed: bool) -> bool {
    if index + 1 >= centerline.len() {
        return false;
    }
    let a = if index == 0 {
        if !closed || centerline.len() < 2 {
            return false;
        }
        centerline[centerline.len() - 2]
    } else {
        centerline[index - 1]
    };
    let b = centerline[index];
    let c = centerline[index + 1];
    let prev = [b[0] - a[0], b[1] - a[1]];
    let next = [c[0] - b[0], c[1] - b[1]];
    let prev_len = prev[0].hypot(prev[1]);
    let next_len = next[0].hypot(next[1]);
    if prev_len <= 0.0 || next_len <= 0.0 {
        return false;
    }
    let cross = prev[0] * next[1] - prev[1] * next[0];
    cross.abs() > 1e-12 || prev[0] * next[0] + prev[1] * next[1] < 0.0
}

/// Rasterizes a live vector shape at the target tile size. Closed paths
/// fill under the shape's nonzero or even-odd rule; open paths contribute
/// only their stroke.
/// Strokes composite source-over fill per texel in sRGB-encoded space,
/// matching the app's byte-space premultiply boundary; linear-light blending
/// is a separate wave.
pub fn rasterize(
    shape: &VectorShape,
    width: u32,
    height: u32,
) -> Result<VectorRaster, VectorError> {
    if u64::from(width) * u64::from(height) > MAX_VECTOR_RASTER_PIXELS as u64 {
        return Err(VectorError::TooLarge {
            width,
            height,
            max_pixels: MAX_VECTOR_RASTER_PIXELS,
        });
    }
    if shape.anchors.len() > MAX_VECTOR_ANCHORS {
        return Err(VectorError::TooComplex {
            what: "anchors",
            limit: MAX_VECTOR_ANCHORS,
        });
    }
    let fills = shape.closed && shape.fill[3] > 0;
    let strokes = shape.stroke.is_some_and(|s| {
        s.color[3] > 0 && s.width.is_finite() && s.width > 0.0 && shape.anchors.len() >= 2
    });
    if width == 0 || height == 0 || shape.anchors.len() < 2 || (!fills && !strokes) {
        return Ok(VectorRaster::empty(width, height));
    }
    let centerline = centerline(shape, width, height)?;
    let fill_polys = if shape.closed && shape.fill[3] > 0 && centerline.len() >= 2 {
        vec![centerline.clone()]
    } else {
        Vec::new()
    };
    let stroke_geometry = stroke_polys(shape, &centerline, width, height)?;
    let edges: usize = fill_polys
        .iter()
        .chain(&stroke_geometry)
        .map(Vec::len)
        .sum();
    let rows = u64::from(height) * 4;
    if (edges as u64).saturating_mul(rows) > MAX_VECTOR_EDGE_ROW_VISITS {
        return Err(VectorError::TooExpensive {
            edges,
            rows,
            limit: MAX_VECTOR_EDGE_ROW_VISITS,
        });
    }
    let fill = crate::svg::rasterize_polys_rule(width, height, &fill_polys, shape.fill_rule);
    let stroke = crate::svg::rasterize_polys(width, height, &stroke_geometry);
    let pixels = (width as usize).saturating_mul(height as usize);
    let mut rgba = vec![0u8; pixels.saturating_mul(4)];
    let mut coverage = vec![0.0f32; pixels];
    for i in 0..pixels {
        let stroke_color = shape.stroke.map(|v| v.color).unwrap_or([0; 4]);
        let fill_alpha = f32::from(fill.get(i).copied().unwrap_or(0)) / 255.0
            * (f32::from(shape.fill[3]) / 255.0);
        let stroke_alpha = f32::from(stroke.get(i).copied().unwrap_or(0)) / 255.0
            * (f32::from(stroke_color[3]) / 255.0);
        let alpha = stroke_alpha + fill_alpha * (1.0 - stroke_alpha);
        if alpha <= 0.0 {
            continue;
        }
        let fill_weight = fill_alpha * (1.0 - stroke_alpha);
        for channel in 0..3 {
            let value = (f32::from(stroke_color[channel]) * stroke_alpha
                + f32::from(shape.fill[channel]) * fill_weight)
                / alpha;
            rgba[i * 4 + channel] = (value.clamp(0.0, 255.0) + 0.5).floor() as u8;
        }
        rgba[i * 4 + 3] = 255;
        coverage[i] = alpha;
    }
    Ok(VectorRaster {
        width,
        height,
        rgba,
        coverage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_corner_collapses_handles() {
        let a = Anchor::corner([0.25, 0.75]);
        assert_eq!(a.in_handle, a.pos);
        assert_eq!(a.out_handle, a.pos);
    }

    #[test]
    fn shape_round_trips_through_json_deterministically() {
        let shape = VectorShape {
            closed: true,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [200, 30, 40, 255],
            stroke: Some(Stroke {
                width: 0.01,
                cap: crate::vector::StrokeCap::Round,

                join: crate::vector::StrokeJoin::Round,
                dash: StrokeDash::solid(),
                color: [10, 10, 10, 255],
            }),
            anchors: vec![Anchor::corner([0.0, 0.0]), Anchor::corner([1.0, 1.0])],
        };
        let s = serde_json::to_string(&shape).expect("serialize");
        let back: VectorShape = serde_json::from_str(&s).expect("deserialize");
        assert_eq!(shape, back);
        let s2 = serde_json::to_string(&shape).expect("serialize again");
        assert_eq!(s, s2, "serialization is deterministic");
    }

    #[test]
    fn cache_identity_is_stable_for_nan_and_signed_zero() {
        let shape = VectorShape {
            closed: false,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [255, 0, 0, 255],
            stroke: Some(Stroke {
                width: f32::NAN,
                cap: crate::vector::StrokeCap::Round,

                join: crate::vector::StrokeJoin::Round,
                dash: StrokeDash::solid(),
                color: [0, 0, 255, 255],
            }),
            anchors: vec![
                Anchor {
                    pos: [f32::NAN, -0.0],
                    in_handle: [f32::INFINITY, 0.25],
                    out_handle: [0.5, f32::NAN],
                },
                Anchor::corner([0.75, 0.5]),
            ],
        };
        assert!(shape.same_cache_key(&shape.clone()));
        let params = VectorParams {
            shape: shape.clone(),
        };
        assert!(params.same_cache_key(&params.clone()));
        let mut signed_zero = shape.clone();
        signed_zero.anchors[0].pos[1] = 0.0;
        assert!(shape.same_cache_key(&signed_zero));
        let mut changed = shape.clone();
        changed.anchors[0].pos[0] = 0.0;
        assert!(!shape.same_cache_key(&changed));
        let mut changed_nan = shape.clone();
        changed_nan.stroke.as_mut().unwrap().width = f32::from_bits(f32::NAN.to_bits() ^ 1);
        assert!(!shape.same_cache_key(&changed_nan));
    }

    #[test]
    fn new_path_default_is_visible_but_geometry_empty() {
        let params = VectorParams::default();
        assert!(params.shape.is_empty());
        assert!(!params.shape.closed);
        assert_eq!(params.shape.fill, [255, 255, 255, 255]);
        assert_eq!(params.shape.stroke, None);
        assert!(params.shape.anchors.is_empty());
        assert_eq!(VectorShape::empty().fill, [0, 0, 0, 0]);
    }

    #[test]
    fn empty_shape_reports_empty() {
        assert!(VectorShape::empty().is_empty());
        let one = VectorShape {
            anchors: vec![Anchor::corner([0.5, 0.5])],
            ..VectorShape::empty()
        };
        assert!(one.is_empty(), "one anchor cannot cover anything");
    }

    #[test]
    fn cubic_into_appends_without_duplicate_start() {
        let (p0, p1, p2, p3) = ([0.0, 0.0], [1.0, 2.0], [3.0, 2.0], [4.0, 0.0]);
        let expected = crate::svg::cubic_device_points(p0, p1, p2, p3);
        let mut actual = vec![p0];
        crate::svg::cubic_device_points_into(p0, p1, p2, p3, &mut actual);
        assert_eq!(actual, expected);
        assert_eq!(
            crate::svg::cubic_device_point_count(p0, p1, p2, p3),
            expected.len()
        );
    }

    #[test]
    fn centerline_matches_individually_flattened_cubics() {
        let shape = VectorShape {
            closed: true,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [255, 255, 255, 255],
            stroke: None,
            anchors: vec![
                Anchor {
                    pos: [0.1, 0.2],
                    in_handle: [0.0, 0.1],
                    out_handle: [0.3, 0.0],
                },
                Anchor {
                    pos: [0.8, 0.3],
                    in_handle: [0.6, 0.0],
                    out_handle: [1.0, 0.6],
                },
                Anchor {
                    pos: [0.4, 0.9],
                    in_handle: [0.8, 0.8],
                    out_handle: [0.2, 1.0],
                },
            ],
        };
        let mut expected = vec![device_point(shape.anchors[0].pos, 64, 32)];
        for i in 0..shape.anchors.len() {
            let a = &shape.anchors[i];
            let b = &shape.anchors[(i + 1) % shape.anchors.len()];
            expected.extend(
                crate::svg::cubic_device_points(
                    device_point(a.pos, 64, 32),
                    device_point(a.out_handle, 64, 32),
                    device_point(b.in_handle, 64, 32),
                    device_point(b.pos, 64, 32),
                )
                .into_iter()
                .skip(1),
            );
        }
        assert_eq!(centerline(&shape, 64, 32).unwrap(), expected);
    }

    #[test]
    fn closed_shape_matches_svg_coverage_exactly() {
        let shape = VectorShape {
            closed: true,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [200, 30, 40, 255],
            stroke: None,
            anchors: vec![
                Anchor {
                    pos: [2.0 / 32.0, 20.0 / 32.0],
                    in_handle: [2.0 / 32.0, 20.0 / 32.0],
                    out_handle: [2.0 / 32.0, 12.0 / 32.0],
                },
                Anchor {
                    pos: [14.0 / 32.0, 20.0 / 32.0],
                    in_handle: [14.0 / 32.0, 12.0 / 32.0],
                    out_handle: [14.0 / 32.0, 20.0 / 32.0],
                },
            ],
        };
        let actual = rasterize(&shape, 32, 32).expect("closed cubic rasterizes");
        let actual_coverage: Vec<u8> = actual
            .coverage()
            .iter()
            .map(|v| (v * 255.0 + 0.5).floor() as u8)
            .collect();
        let expected = crate::svg::rasterize_svg_default(
            br#"<svg width="32" height="32"><path d="M 2 20 C 2 12, 14 12, 14 20 Z"/></svg>"#,
        )
        .expect("reference SVG rasterizes");
        assert_eq!(actual_coverage, expected.coverage());
    }

    #[test]
    fn closed_shape_with_curved_closure_matches_svg() {
        let shape = VectorShape {
            closed: true,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [40, 180, 220, 255],
            stroke: None,
            anchors: vec![
                Anchor {
                    pos: [0.2, 0.2],
                    in_handle: [0.1, 0.1],
                    out_handle: [0.4, 0.1],
                },
                Anchor {
                    pos: [0.8, 0.2],
                    in_handle: [0.6, 0.1],
                    out_handle: [0.9, 0.4],
                },
                Anchor {
                    pos: [0.5, 0.8],
                    in_handle: [0.9, 0.6],
                    out_handle: [0.1, 0.6],
                },
            ],
        };
        let actual = rasterize(&shape, 64, 64).expect("vector raster");
        let svg = crate::svg::rasterize_svg_default(
            br#"<svg width="64" height="64"><path d="M 12.8 12.8 C 25.6 6.4 38.4 6.4 51.2 12.8 C 57.6 25.6 57.6 38.4 32 51.2 C 6.4 38.4 6.4 6.4 12.8 12.8 Z"/></svg>"#,
        )
        .expect("SVG raster");
        let actual_coverage: Vec<u8> = actual
            .coverage()
            .iter()
            .map(|alpha| (alpha * 255.0 + 0.5).floor() as u8)
            .collect();
        assert_eq!(actual_coverage, svg.coverage());
    }

    #[test]
    fn open_shape_stroke_has_round_caps() {
        let shape = VectorShape {
            closed: false,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [200, 30, 40, 255],
            stroke: Some(Stroke {
                width: 0.25,
                cap: crate::vector::StrokeCap::Round,

                join: crate::vector::StrokeJoin::Round,
                dash: StrokeDash::solid(),
                color: [10, 20, 30, 255],
            }),
            anchors: vec![Anchor::corner([0.25, 0.5]), Anchor::corner([0.75, 0.5])],
        };
        let actual = rasterize(&shape, 16, 16).expect("open stroke rasterizes");
        let at = |x: u32, y: u32| {
            let i = (y * 16 + x) as usize;
            (actual.coverage()[i], &actual.rgba()[i * 4..i * 4 + 4])
        };
        assert_eq!(at(7, 7), (1.0, &[10, 20, 30, 255][..]));
        assert_eq!(
            at(5, 7),
            (1.0, &[10, 20, 30, 255][..]),
            "the left cap and its quad overlap without cancelling"
        );
        assert_eq!(
            at(10, 7),
            (1.0, &[10, 20, 30, 255][..]),
            "the right cap and its quad overlap without cancelling"
        );
        assert_eq!(
            at(3, 7),
            (1.0, &[10, 20, 30, 255][..]),
            "round cap reaches one pixel left of the endpoint"
        );
        assert_eq!(
            at(1, 7).0,
            0.0,
            "the cap stays circular instead of becoming a square"
        );
        assert_eq!(at(7, 1).0, 0.0, "outside the stroke stays transparent");
        assert_eq!(at(0, 0).0, 0.0);
    }

    #[test]
    fn stroke_composites_source_over_fill() {
        let shape = VectorShape {
            closed: true,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [255, 0, 0, 128],
            stroke: Some(Stroke {
                width: 0.5,
                cap: crate::vector::StrokeCap::Round,

                join: crate::vector::StrokeJoin::Round,
                dash: StrokeDash::solid(),
                color: [0, 0, 255, 128],
            }),
            anchors: vec![
                Anchor::corner([0.0, 0.0]),
                Anchor::corner([1.0, 0.0]),
                Anchor::corner([1.0, 1.0]),
                Anchor::corner([0.0, 1.0]),
            ],
        };
        let actual = rasterize(&shape, 16, 16).expect("alpha shape rasterizes");
        let at = |x: u32, y: u32| {
            let i = (y * 16 + x) as usize;
            (actual.coverage()[i], &actual.rgba()[i * 4..i * 4 + 4])
        };
        let (stroke_alpha, stroke_rgb) = at(7, 0);
        assert!((stroke_alpha - 48_896.0 / 65_025.0).abs() < 1e-6);
        assert_eq!(stroke_rgb, &[85, 0, 170, 255]);
        let (fill_alpha, fill_rgb) = at(7, 7);
        assert!((fill_alpha - 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(fill_rgb, &[255, 0, 0, 255]);
    }

    #[test]
    fn zero_alpha_stroke_is_identical_to_no_stroke() {
        let shape = VectorShape {
            closed: true,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [255, 0, 0, 128],
            stroke: Some(Stroke {
                width: 0.5,
                cap: crate::vector::StrokeCap::Round,

                join: crate::vector::StrokeJoin::Round,
                dash: StrokeDash::solid(),
                color: [0, 0, 255, 0],
            }),
            anchors: vec![
                Anchor::corner([0.25, 0.25]),
                Anchor::corner([0.75, 0.25]),
                Anchor::corner([0.75, 0.75]),
                Anchor::corner([0.25, 0.75]),
            ],
        };
        let invisible = rasterize(&shape, 16, 16).expect("zero-alpha stroke");
        let absent = rasterize(
            &VectorShape {
                stroke: None,
                ..shape
            },
            16,
            16,
        )
        .expect("no stroke");
        assert_eq!(invisible, absent);
    }

    #[test]
    fn dimension_budget_is_a_typed_error() {
        let shape = VectorShape {
            closed: true,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [255, 255, 255, 255],
            stroke: None,
            anchors: vec![
                Anchor::corner([0.0, 0.0]),
                Anchor::corner([1.0, 0.0]),
                Anchor::corner([1.0, 1.0]),
                Anchor::corner([0.0, 1.0]),
            ],
        };
        assert!(rasterize(&shape, 8192, 1).is_ok(), "8K is inclusive");
        assert_eq!(
            rasterize(&shape, 8193, 8193),
            Err(VectorError::TooLarge {
                width: 8193,
                height: 8193,
                max_pixels: MAX_VECTOR_RASTER_PIXELS,
            })
        );
    }

    #[test]
    fn structure_budgets_are_typed_errors() {
        let base = VectorShape {
            closed: false,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [0, 0, 0, 0],
            stroke: None,
            anchors: Vec::new(),
        };
        let too_many = VectorShape {
            anchors: vec![Anchor::corner([0.5, 0.5]); MAX_VECTOR_ANCHORS + 1],
            ..base.clone()
        };
        assert_eq!(
            rasterize(&too_many, 16, 16),
            Err(VectorError::TooComplex {
                what: "anchors",
                limit: MAX_VECTOR_ANCHORS,
            })
        );
        let curved = VectorShape {
            closed: true,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [255, 255, 255, 255],
            stroke: None,
            anchors: (0..300)
                .map(|_| Anchor {
                    pos: [0.5, 0.5],
                    in_handle: [-1.0e6, -1.0e6],
                    out_handle: [1.0e6, 1.0e6],
                })
                .collect(),
        };
        assert_eq!(
            rasterize(&curved, 8192, 8192),
            Err(VectorError::TooComplex {
                what: "path points",
                limit: MAX_VECTOR_POINTS,
            })
        );
    }

    #[test]
    fn invalid_and_empty_geometry_is_transparent() {
        let cases = [
            VectorShape::empty(),
            VectorShape {
                closed: false,
                fill_rule: crate::svg::FillRule::NonZero,
                fill: [255, 0, 0, 255],
                stroke: Some(Stroke {
                    width: 0.25,
                    cap: crate::vector::StrokeCap::Round,

                    join: crate::vector::StrokeJoin::Round,
                    dash: StrokeDash::solid(),
                    color: [0, 0, 255, 255],
                }),
                anchors: vec![Anchor::corner([0.5, 0.5])],
            },
            VectorShape {
                closed: false,
                fill_rule: crate::svg::FillRule::NonZero,
                fill: [255, 0, 0, 255],
                stroke: None,
                anchors: vec![Anchor::corner([0.25, 0.5]), Anchor::corner([0.75, 0.5])],
            },
            VectorShape {
                closed: true,
                fill_rule: crate::svg::FillRule::NonZero,
                fill: [255, 0, 0, 255],
                stroke: Some(Stroke {
                    width: 0.25,
                    cap: crate::vector::StrokeCap::Round,

                    join: crate::vector::StrokeJoin::Round,
                    dash: StrokeDash::solid(),
                    color: [0, 0, 255, 255],
                }),
                anchors: vec![
                    Anchor::corner([f32::NAN, 0.5]),
                    Anchor::corner([f32::NAN, f32::NAN]),
                ],
            },
            VectorShape {
                closed: false,
                fill_rule: crate::svg::FillRule::NonZero,
                fill: [255, 0, 0, 255],
                stroke: Some(Stroke {
                    width: 0.25,
                    cap: crate::vector::StrokeCap::Round,

                    join: crate::vector::StrokeJoin::Round,
                    dash: StrokeDash::solid(),
                    color: [0, 0, 255, 255],
                }),
                anchors: vec![
                    Anchor {
                        pos: [0.25, 0.5],
                        in_handle: [0.25, 0.5],
                        out_handle: [f32::NAN, 0.5],
                    },
                    Anchor::corner([0.75, 0.5]),
                ],
            },
        ];
        for shape in &cases {
            let actual = rasterize(shape, 16, 16).expect("degenerate input is not an error");
            assert!(actual.coverage().iter().all(|v| *v == 0.0));
            assert!(actual.rgba().iter().all(|v| *v == 0));
        }
        let empty = rasterize(&cases[1], 0, 16).expect("zero-sized output is empty");
        assert!(empty.coverage().is_empty());
        assert!(empty.rgba().is_empty());
    }

    #[test]
    fn raster_is_deterministic_and_non_square() {
        let shape = VectorShape {
            closed: false,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [0, 0, 0, 0],
            stroke: Some(Stroke {
                width: 0.25,
                cap: crate::vector::StrokeCap::Round,

                join: crate::vector::StrokeJoin::Round,
                dash: StrokeDash::solid(),
                color: [10, 20, 30, 255],
            }),
            anchors: vec![Anchor::corner([0.25, 0.5]), Anchor::corner([0.75, 0.5])],
        };
        let first = rasterize(&shape, 32, 16).expect("non-square stroke rasterizes");
        let second = rasterize(&shape, 32, 16).expect("repeat rasterization");
        assert_eq!(first, second);
        let at = |x: u32, y: u32| first.coverage()[(y * 32 + x) as usize];
        assert_eq!(at(15, 7), 1.0);
        assert_eq!(at(7, 7), 1.0, "the shorter edge sets the same 2px radius");
        assert_eq!(at(5, 7), 0.0);
        assert_eq!(at(15, 1), 0.0);
    }

    #[test]
    fn empty_inputs_short_circuit_before_geometry() {
        let shape = VectorShape {
            closed: false,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [255, 0, 0, 255],
            stroke: None,
            anchors: (0..300)
                .map(|_| Anchor {
                    pos: [0.5, 0.5],
                    in_handle: [-1.0e6, -1.0e6],
                    out_handle: [1.0e6, 1.0e6],
                })
                .collect(),
        };
        let empty = rasterize(&shape, 4096, 4096).expect("no painted primitive is empty");
        assert!(empty.coverage().is_empty());
        assert!(empty.rgba().is_empty());
        let zero = rasterize(&shape, 0, 4096).expect("zero width is empty");
        assert!(zero.coverage().is_empty());
        assert!(zero.rgba().is_empty());
        assert_eq!((zero.width(), zero.height()), (0, 4096));
    }

    #[test]
    fn edge_row_budget_is_a_typed_error() {
        let shape = VectorShape {
            closed: true,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [255, 255, 255, 255],
            stroke: None,
            anchors: (0..1_000)
                .map(|i| Anchor::corner([0.25 + 0.5 * (i % 2) as f32, 0.5]))
                .collect(),
        };
        let result = rasterize(&shape, 8192, 8192);
        let Err(VectorError::TooExpensive { edges, rows, limit }) = result else {
            panic!("edge-row budget must refuse this shape: {result:?}");
        };
        assert_eq!(rows, 8192 * 4);
        assert_eq!(limit, MAX_VECTOR_EDGE_ROW_VISITS);
        assert!((edges as u64) * rows > limit);
    }

    #[test]
    fn evenodd_star_changes_only_the_inner_pentagon() {
        let star = VectorShape {
            closed: true,
            fill: [255, 255, 255, 255],
            fill_rule: crate::svg::FillRule::EvenOdd,
            stroke: None,
            anchors: vec![
                Anchor::corner([0.50, 0.05]),
                Anchor::corner([0.79, 0.95]),
                Anchor::corner([0.02, 0.39]),
                Anchor::corner([0.98, 0.39]),
                Anchor::corner([0.21, 0.95]),
            ],
        };
        let evenodd = rasterize(&star, 100, 100).expect("evenodd star");
        let nonzero = rasterize(
            &VectorShape {
                fill_rule: crate::svg::FillRule::NonZero,
                ..star.clone()
            },
            100,
            100,
        )
        .expect("nonzero star");
        let at = |raster: &VectorRaster, x: u32, y: u32| raster.coverage()[(y * 100 + x) as usize];
        assert_eq!(
            at(&evenodd, 50, 50),
            0.0,
            "inner pentagon is outside evenodd"
        );
        assert_eq!(
            at(&nonzero, 50, 50),
            1.0,
            "inner pentagon is inside nonzero"
        );
        assert_eq!(at(&evenodd, 50, 10), 1.0, "star tip is inside both");
        assert_eq!(at(&nonzero, 50, 10), 1.0);

        let svg_evenodd = crate::svg::rasterize_svg_default(
            br#"<svg width="100" height="100"><path fill-rule="evenodd" d="M 50 5 L 79 95 L 2 39 L 98 39 L 21 95 Z"/></svg>"#,
        )
        .expect("SVG evenodd oracle");
        let svg_nonzero = crate::svg::rasterize_svg_default(
            br#"<svg width="100" height="100"><path fill-rule="nonzero" d="M 50 5 L 79 95 L 2 39 L 98 39 L 21 95 Z"/></svg>"#,
        )
        .expect("SVG nonzero oracle");
        let evenodd_bytes: Vec<u8> = evenodd
            .coverage()
            .iter()
            .map(|value| (value * 255.0 + 0.5).floor() as u8)
            .collect();
        let nonzero_bytes: Vec<u8> = nonzero
            .coverage()
            .iter()
            .map(|value| (value * 255.0 + 0.5).floor() as u8)
            .collect();
        assert_eq!(evenodd_bytes, svg_evenodd.coverage());
        assert_eq!(nonzero_bytes, svg_nonzero.coverage());
        let different: Vec<usize> = (0..100 * 100)
            .filter(|&index| evenodd_bytes[index] != nonzero_bytes[index])
            .collect();
        assert!(!different.is_empty());
        for index in different {
            assert!(
                evenodd_bytes[index] < nonzero_bytes[index],
                "evenodd must not add coverage outside the inner pentagon"
            );
        }

        let json = serde_json::to_string(&star).expect("serialize evenodd");
        let back: VectorShape = serde_json::from_str(&json).expect("deserialize evenodd");
        assert_eq!(back.fill_rule, crate::svg::FillRule::EvenOdd);
        let old: VectorShape = serde_json::from_str(
            r#"{"closed":true,"fill":[255,255,255,255],"stroke":null,"anchors":[]}"#,
        )
        .expect("pre-fill-rule project loads");
        assert_eq!(old.fill_rule, crate::svg::FillRule::NonZero);
        let mut changed = star.clone();
        changed.fill_rule = crate::svg::FillRule::NonZero;
        assert!(
            !star.same_cache_key(&changed),
            "fill rule is cache identity"
        );
    }

    #[test]
    fn butt_and_square_caps_follow_the_endpoints() {
        let shape = |cap| VectorShape {
            closed: false,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [0, 0, 0, 0],
            stroke: Some(Stroke {
                width: 0.25,
                color: [10, 20, 30, 255],
                cap,
                join: StrokeJoin::Round,
                dash: StrokeDash::solid(),
            }),
            anchors: vec![Anchor::corner([0.25, 0.5]), Anchor::corner([0.75, 0.5])],
        };
        let at = |raster: &VectorRaster, x: u32, y: u32| raster.coverage()[(y * 16 + x) as usize];
        let butt = rasterize(&shape(StrokeCap::Butt), 16, 16).expect("butt cap");
        assert_eq!(at(&butt, 3, 7), 0.0, "butt stops before the endpoint");
        assert_eq!(at(&butt, 5, 7), 1.0);
        assert_eq!(at(&butt, 10, 7), 1.0);
        assert_eq!(at(&butt, 13, 7), 0.0, "butt stops after the last endpoint");

        let square = rasterize(&shape(StrokeCap::Square), 16, 16).expect("square cap");
        assert_eq!(at(&square, 2, 7), 1.0, "square extends by the radius");
        assert_eq!(at(&square, 1, 7), 0.0, "square still stops at its cap edge");
        assert_eq!(at(&square, 13, 7), 1.0, "the last square cap extends too");
        assert_eq!(at(&square, 14, 7), 0.0);

        let closed = VectorShape {
            closed: true,
            anchors: vec![
                Anchor::corner([0.0, 0.0]),
                Anchor::corner([1.0, 0.0]),
                Anchor::corner([1.0, 1.0]),
                Anchor::corner([0.0, 1.0]),
            ],
            ..shape(StrokeCap::Butt)
        };
        let round = rasterize(
            &VectorShape {
                stroke: Some(Stroke {
                    cap: StrokeCap::Round,

                    join: crate::vector::StrokeJoin::Round,
                    dash: StrokeDash::solid(),
                    ..closed.stroke.expect("closed stroke")
                }),
                ..closed.clone()
            },
            16,
            16,
        )
        .expect("closed round");
        let butt = rasterize(
            &VectorShape {
                stroke: Some(Stroke {
                    cap: StrokeCap::Butt,

                    join: crate::vector::StrokeJoin::Round,
                    dash: StrokeDash::solid(),
                    ..closed.stroke.expect("closed stroke")
                }),
                ..closed
            },
            16,
            16,
        )
        .expect("closed butt");
        assert_eq!(round, butt, "closed paths have no endpoints to cap");

        for (x, y) in [(0, 0), (0, 15), (15, 0), (15, 15)] {
            let index = (y * 16 + x) as usize;
            assert_eq!(
                round.coverage()[index],
                1.0,
                "Round keeps the closure join at ({x},{y})"
            );
            assert_eq!(
                butt.coverage()[index],
                1.0,
                "cap choice must not remove the closure join at ({x},{y})"
            );
        }

        let json = serde_json::to_string(&shape(StrokeCap::Square)).expect("serialize cap");
        let back: VectorShape = serde_json::from_str(&json).expect("deserialize cap");
        assert_eq!(back.stroke.expect("cap survives").cap, StrokeCap::Square);
        let old: VectorShape = serde_json::from_str(
            r#"{"closed":false,"fill":[0,0,0,0],"stroke":{"width":0.25,"color":[10,20,30,255]},"anchors":[{"pos":[0.25,0.5],"in_handle":[0.25,0.5],"out_handle":[0.25,0.5]},{"pos":[0.75,0.5],"in_handle":[0.75,0.5],"out_handle":[0.75,0.5]}]}"#,
        )
        .expect("pre-cap project loads");
        assert_eq!(old.stroke.expect("old stroke").cap, StrokeCap::Round);
        let mut changed = shape(StrokeCap::Square);
        changed.stroke.as_mut().unwrap().cap = StrokeCap::Butt;
        assert!(!shape(StrokeCap::Square).same_cache_key(&changed));
    }

    #[test]
    fn bevel_and_miter_joins_emit_the_expected_geometry() {
        let shape = |join| VectorShape {
            closed: false,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [0, 0, 0, 0],
            stroke: Some(Stroke {
                width: 0.25,
                color: [10, 20, 30, 255],
                cap: StrokeCap::Butt,
                join,
                dash: StrokeDash::solid(),
            }),
            anchors: vec![
                Anchor::corner([0.25, 0.75]),
                Anchor::corner([0.50, 0.75]),
                Anchor::corner([0.50, 0.25]),
            ],
        };
        let centerline = |join| centerline(&shape(join), 16, 16).expect("centerline");
        let bevel = stroke_polys(
            &shape(StrokeJoin::Bevel),
            &centerline(StrokeJoin::Bevel),
            16,
            16,
        )
        .expect("bevel join");
        assert!(bevel.iter().any(|polygon| polygon.len() == 3));
        let line = centerline(StrokeJoin::Bevel);
        let join_index = (0..line.len())
            .find(|&index| join_at(&line, index, false))
            .expect("an actual turn exists");
        let actual =
            join_polygon(&line, join_index, 2.0, StrokeJoin::Bevel, false).expect("bevel polygon");
        let expected = [[8.0, 14.0], [10.0, 12.0], [8.0, 12.0]];
        for (actual, expected) in actual.iter().zip(expected.iter()) {
            assert!((actual[0] - expected[0]).abs() < 1e-6);
            assert!((actual[1] - expected[1]).abs() < 1e-6);
        }
        let miter = stroke_polys(
            &shape(StrokeJoin::Miter),
            &centerline(StrokeJoin::Miter),
            16,
            16,
        )
        .expect("miter join");
        assert!(miter.iter().any(|polygon| polygon.len() == 4));
        let actual =
            join_polygon(&line, join_index, 2.0, StrokeJoin::Miter, false).expect("miter polygon");
        let expected = [[8.0, 14.0], [10.0, 14.0], [10.0, 12.0], [8.0, 12.0]];
        for (actual, expected) in actual.iter().zip(expected.iter()) {
            assert!((actual[0] - expected[0]).abs() < 1e-6);
            assert!((actual[1] - expected[1]).abs() < 1e-6);
        }

        let closed = VectorShape {
            closed: true,
            anchors: vec![
                Anchor::corner([0.0, 0.0]),
                Anchor::corner([1.0, 0.0]),
                Anchor::corner([1.0, 1.0]),
                Anchor::corner([0.0, 1.0]),
            ],
            ..shape(StrokeJoin::Bevel)
        };
        let closed_line = super::centerline(&closed, 16, 16).expect("closed centerline");
        let closed_geometry = stroke_polys(&closed, &closed_line, 16, 16).expect("closed join");
        assert_eq!(
            join_polygon(&closed_line, 0, 2.0, StrokeJoin::Bevel, true)
                .expect("closure join polygon"),
            vec![[-2.0, 0.0], [0.0, -2.0], [0.0, 0.0]],
            "the closure join is a real join, not an endpoint cap"
        );
        assert!(closed_geometry.iter().any(|polygon| {
            polygon.len() == 3
                && polygon
                    .iter()
                    .zip([[-2.0, 0.0], [0.0, -2.0], [0.0, 0.0]])
                    .all(|(a, e)| (a[0] - e[0]).abs() < 1e-6 && (a[1] - e[1]).abs() < 1e-6)
        }));
        let ellipse_points = crate::svg::ellipse_point_count(2.0, 2.0);
        assert!(
            !closed_geometry
                .iter()
                .any(|polygon| polygon.len() == ellipse_points),
            "closed Miter/Bevel joins must not emit a round endpoint cap"
        );

        let json = serde_json::to_string(&shape(StrokeJoin::Miter)).expect("serialize join");
        let back: VectorShape = serde_json::from_str(&json).expect("deserialize join");
        assert_eq!(back.stroke.expect("join survives").join, StrokeJoin::Miter);
        let old: VectorShape = serde_json::from_str(
            r#"{"closed":false,"fill":[0,0,0,0],"stroke":{"width":0.25,"color":[10,20,30,255]},"anchors":[]}"#,
        )
        .expect("pre-join project loads");
        assert_eq!(old.stroke.expect("old stroke").join, StrokeJoin::Round);
        let mut changed = shape(StrokeJoin::Miter);
        changed.stroke.as_mut().unwrap().join = StrokeJoin::Bevel;
        assert!(!shape(StrokeJoin::Miter).same_cache_key(&changed));
    }

    #[test]
    fn dash_pattern_alternates_offsets_and_repeats_odd_patterns() {
        let dash = StrokeDash::from_lengths(&[0.25, 0.125], 0.0).expect("valid dash");
        let pattern = DashPattern::new(dash, 1.0).expect("pattern");
        let intervals = pattern.intervals(0.0, 1.0).expect("intervals");
        assert_eq!(intervals, vec![(0.0, 0.25), (0.375, 0.625), (0.75, 1.0)]);

        let offset = StrokeDash::from_lengths(&[0.25, 0.125], 0.125).expect("offset dash");
        let pattern = DashPattern::new(offset, 1.0).expect("offset pattern");
        let intervals = pattern.intervals(0.0, 0.75).expect("offset intervals");
        assert_eq!(intervals, vec![(0.0, 0.125), (0.25, 0.5), (0.625, 0.75)]);

        let odd = StrokeDash::from_lengths(&[0.25], 0.0).expect("odd dash");
        let pattern = DashPattern::new(odd, 1.0).expect("odd pattern");
        assert_eq!(
            pattern.intervals(0.0, 1.0).expect("odd intervals"),
            vec![(0.0, 0.25), (0.5, 0.75)]
        );
        let zero_off =
            StrokeDash::from_lengths(&[0.25, 0.0, 0.25, 0.25], 0.0).expect("zero off length");
        let pattern = DashPattern::new(zero_off, 1.0).expect("zero-off pattern");
        assert_eq!(
            pattern.intervals(0.0, 1.0).expect("zero-off intervals"),
            vec![(0.0, 0.5), (0.75, 1.0)],
            "a zero off-length merges contiguous dashes instead of adding caps"
        );
        let zero_on =
            StrokeDash::from_lengths(&[0.0, 0.25, 0.25, 0.25], 0.0).expect("zero on length");
        let pattern = DashPattern::new(zero_on, 1.0).expect("zero-on pattern");
        assert_eq!(
            pattern.intervals(0.0, 1.0).expect("zero-on intervals"),
            vec![(0.25, 0.5)]
        );
        assert!(StrokeDash::from_lengths(&[0.0, 0.0], 0.0).is_err());
        assert!(StrokeDash::from_lengths(&[-0.1, 0.2], 0.0).is_err());
        assert!(StrokeDash::from_lengths(&[0.1; 9], 0.0).is_err());
    }

    #[test]
    fn dashed_strokes_emit_caps_joins_and_scaled_intervals() {
        let shape = |cap, join, dash| VectorShape {
            closed: false,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [0, 0, 0, 0],
            stroke: Some(Stroke {
                width: 0.25,
                color: [10, 20, 30, 255],
                cap,
                join,
                dash,
            }),
            anchors: vec![Anchor::corner([0.0, 0.5]), Anchor::corner([1.0, 0.5])],
        };
        let dash = StrokeDash::from_lengths(&[0.25, 0.25], 0.0).expect("dash");
        let butt = stroke_polys(
            &shape(StrokeCap::Butt, StrokeJoin::Round, dash),
            &centerline(&shape(StrokeCap::Butt, StrokeJoin::Round, dash), 16, 16).unwrap(),
            16,
            16,
        )
        .expect("butt dashes");
        assert!(butt.iter().all(|polygon| polygon.len() == 4));
        let runs = butt.windows(2).fold(1usize, |runs, pair| {
            let right = pair[0]
                .iter()
                .map(|p| p[0])
                .fold(f64::NEG_INFINITY, f64::max);
            let left = pair[1].iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
            runs + usize::from(left > right + 1e-6)
        });
        assert_eq!(runs, 2, "16px line, 4px on / 4px off");

        let round = stroke_polys(
            &shape(StrokeCap::Round, StrokeJoin::Round, dash),
            &centerline(&shape(StrokeCap::Round, StrokeJoin::Round, dash), 16, 16).unwrap(),
            16,
            16,
        )
        .expect("round dash caps");
        assert_eq!(round.len(), butt.len() + 4, "four round dash caps added");

        let square = stroke_polys(
            &shape(StrokeCap::Square, StrokeJoin::Round, dash),
            &centerline(&shape(StrokeCap::Square, StrokeJoin::Round, dash), 16, 16).unwrap(),
            16,
            16,
        )
        .expect("square dash caps");
        assert_eq!(square.len(), butt.len());
        let min_x = square[0].iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        let max_x = square
            .last()
            .expect("second dash run")
            .iter()
            .map(|p| p[0])
            .fold(f64::NEG_INFINITY, f64::max);
        assert!((min_x + 2.0).abs() < 1e-6 && (max_x - 14.0).abs() < 1e-6);

        let zero_off =
            StrokeDash::from_lengths(&[0.25, 0.0, 0.25, 0.25], 0.0).expect("zero off length");
        let zero_butt = stroke_polys(
            &shape(StrokeCap::Butt, StrokeJoin::Round, zero_off),
            &centerline(&shape(StrokeCap::Butt, StrokeJoin::Round, zero_off), 16, 16).unwrap(),
            16,
            16,
        )
        .expect("zero-gap butt dashes");
        let zero_round = stroke_polys(
            &shape(StrokeCap::Round, StrokeJoin::Round, zero_off),
            &centerline(
                &shape(StrokeCap::Round, StrokeJoin::Round, zero_off),
                16,
                16,
            )
            .unwrap(),
            16,
            16,
        )
        .expect("zero-gap round caps");
        assert_eq!(
            zero_round.len(),
            zero_butt.len() + 4,
            "two merged dash runs get four caps, not a false cap at the zero gap"
        );

        let turn_shape = |dash| VectorShape {
            anchors: vec![
                Anchor::corner([0.25, 0.75]),
                Anchor::corner([0.50, 0.75]),
                Anchor::corner([0.50, 0.25]),
            ],
            ..shape(StrokeCap::Butt, StrokeJoin::Bevel, dash)
        };
        let continuous = StrokeDash::from_lengths(&[2.0, 1.0], 0.0).expect("continuous dash");
        let joined = stroke_polys(
            &turn_shape(continuous),
            &centerline(&turn_shape(continuous), 16, 16).unwrap(),
            16,
            16,
        )
        .expect("continuous join");
        assert!(joined.iter().any(|polygon| polygon.len() == 3));
        let gap = StrokeDash::from_lengths(&[0.25, 0.5], 0.0).expect("gap dash");
        let broken = stroke_polys(
            &turn_shape(gap),
            &centerline(&turn_shape(gap), 16, 16).unwrap(),
            16,
            16,
        )
        .expect("gap at join");
        assert!(!broken.iter().any(|polygon| polygon.len() == 3));
    }

    #[test]
    fn dash_json_cache_and_invalid_values_are_safe() {
        let dash = StrokeDash::from_lengths(&[0.25, 0.125], 0.0625).expect("dash");
        let shape = VectorShape {
            closed: false,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [0, 0, 0, 0],
            stroke: Some(Stroke {
                width: 0.25,
                color: [10, 20, 30, 255],
                cap: StrokeCap::Round,
                join: StrokeJoin::Round,
                dash,
            }),
            anchors: vec![Anchor::corner([0.0, 0.5]), Anchor::corner([1.0, 0.5])],
        };
        let json = serde_json::to_string(&shape).expect("serialize dash");
        let back: VectorShape = serde_json::from_str(&json).expect("deserialize dash");
        assert_eq!(back.stroke.expect("stroke").dash, dash);
        let old: VectorShape = serde_json::from_str(
            r#"{"closed":false,"fill":[0,0,0,0],"stroke":{"width":0.25,"color":[10,20,30,255]},"anchors":[]}"#,
        )
        .expect("pre-dash project loads");
        assert!(old.stroke.expect("old stroke").dash.is_solid());
        let mut changed = shape.clone();
        changed.stroke.as_mut().unwrap().dash.offset = 0.125;
        assert!(!shape.same_cache_key(&changed));
        let mut invalid = shape;
        invalid.stroke.as_mut().unwrap().dash.len = 9;
        let line = centerline(&invalid, 16, 16).expect("centerline");
        assert_eq!(
            stroke_polys(&invalid, &line, 16, 16),
            Err(VectorError::InvalidDash)
        );
    }

    #[test]
    fn stroke_geometry_budget_is_typed_error() {
        let shape = VectorShape {
            closed: false,
            fill_rule: crate::svg::FillRule::NonZero,
            fill: [0, 0, 0, 0],
            stroke: Some(Stroke {
                width: 1.0e6,
                cap: crate::vector::StrokeCap::Round,

                join: crate::vector::StrokeJoin::Round,
                dash: StrokeDash::solid(),
                color: [255, 255, 255, 255],
            }),
            anchors: vec![Anchor::corner([0.5, 0.5]); 300],
        };
        assert_eq!(
            rasterize(&shape, 16, 16),
            Err(VectorError::TooComplex {
                what: "path points",
                limit: MAX_VECTOR_POINTS,
            })
        );
    }
}
