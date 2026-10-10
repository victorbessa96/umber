//! The retouch stroke session — the app half of the smudge + clone brushes
//! (docs/specs/smudge-clone-design.md).
//!
//! The paint pipeline composites on the GPU (`PaintCompositor::splat_dabs`,
//! a compute pass); there is no CPU composite step to extend. The retouch
//! tools therefore run their dabs on a per-stroke CPU mirror instead: the
//! first time a stroke touches a tile, [`crate::paint_state::PaintState`]
//! drains that tile's queue and reads the target back
//! (`umber_gpu::paint::PaintTarget::read_back_rgba8` — the same full-target
//! readback the high-to-low transfer and the PNG export use), hands the
//! bytes to [`RetouchStroke::insert_tile`], and after every event writes
//! the dirty rect back (`PaintThreadCommand::WriteRegion`). One readback
//! per tile per stroke; per-event uploads are dirty-rect sized.
//!
//! The session is GPU-free (the [`PaintState`](crate::paint_state) wiring
//! owns the device), so the stroke + undo contract is testable headless.
//!
//! Undo (design test 5, "one undo per stroke"): [`RetouchStroke::finish`]
//! yields ONE [`PaintStrokeCommand`] per stroke — every touched tile's
//! dirty rect, before + after — for an [`UndoStack`](umber_core::undo::UndoStack).
//! Its document is a [`PaintWrites`] queue, so applying/reverting a
//! command needs no GPU either: the caller drains the queue into
//! `WriteRegion` commands.
//!
//! Symmetry (§2's P1 row, `umber_brush::symmetry`): [`RetouchStroke::apply`]
//! takes the PRIMARY dabs and stamps each one's symmetric siblings itself
//! (the texel-space transforms set by [`RetouchStroke::set_symmetry`]).
//! The semantics chosen:
//!
//! * Smudge: every sibling keeps its own previous-dab chain, so a
//!   sibling's motion vector is its own position delta — the primary's
//!   motion transformed with the point (a mirrored smudge drags the
//!   mirrored way). Sharing one chain would measure motion between a dab
//!   and its own mirror.
//! * Clone: a sibling at `T(p)` samples `T(source of p)` — the source
//!   point (and so the offset, `L · offset`) mirrors/rotates with the dab;
//!   fixed mode samples the transformed anchor. The patch is copied by
//!   translation (`clone_dab`), so its CONTENT is not reflected: a
//!   mirrored clone places mirrored-source texels unflipped (v1; a
//!   reflected-sample clone is a named follow-up).
//!
//! All siblings land in the stroke's own tile mirror, so a symmetric
//! stroke is still ONE undo entry; its dirty rect is the bounding box of
//! every sibling (at 4K a full-tile radial stroke's before + after can
//! take half of the history's 256 MB total budget — a per-sibling rect
//! list is a follow-up).

use std::collections::BTreeMap;

use umber_brush::retouch::{clone_dab, smudge_dab, CloneStroke, DirtyRect, RetouchDab};
use umber_brush::symmetry::{sibling_positions, COINCIDENT_EPS_TEXELS};
use umber_brush::{BrushTool, RgbaBuffer, SymTransform};
use umber_core::undo::Command;

/// One tile's CPU mirror for the current stroke.
struct TileMirror {
    /// The live copy every dab edits.
    live: RgbaBuffer,
    /// The stroke-start snapshot: clone's sample source and the undo
    /// record's "before".
    before: RgbaBuffer,
    /// Union of every rect the stroke wrote on this tile.
    dirty: Option<DirtyRect>,
}

/// Clone tracking for the stroke: the source tracker plus the one tile it
/// lives in (v1: same texture set, same layer, same tile — an event routed
/// to another tile is dropped).
struct CloneState {
    tracker: CloneStroke,
    tile: u16,
}

/// The live retouch stroke: the tool, the per-tile mirrors, and the dab
/// motion history the smudge tilt derives from.
pub(crate) struct RetouchStroke {
    tool: BrushTool,
    tiles: BTreeMap<u16, TileMirror>,
    /// The previous dab (tile, texel position) of each symmetry sibling,
    /// indexed like `symmetry`: each smudge dab's motion vector is its
    /// position minus its own sibling's previous one, so the incoming
    /// stroke direction comes straight from the conditioned dab path.
    /// `None` at stroke start (the first dab has no direction → no tilt)
    /// and stale whenever the stroke crosses into another tile (local
    /// coordinates jump there).
    last_dab: Vec<Option<(u16, [f32; 2])>>,
    clone: Option<CloneState>,
    /// The tile-local texel-space symmetry transforms, identity first
    /// (`[IDENTITY]` = symmetry off).
    symmetry: Vec<SymTransform>,
}

impl RetouchStroke {
    /// A fresh stroke for `tool` (a non-retouch tool yields a stroke that
    /// never writes), symmetry off.
    pub(crate) fn new(tool: BrushTool) -> Self {
        Self {
            tool,
            tiles: BTreeMap::new(),
            last_dab: vec![None],
            clone: None,
            symmetry: vec![SymTransform::IDENTITY],
        }
    }

    /// Arms symmetry for the stroke: `texel_transforms` from
    /// `umber_brush::symmetry::texel_transforms` (identity first). Call
    /// before the first [`Self::apply`]; an empty list keeps the identity.
    pub(crate) fn set_symmetry(&mut self, texel_transforms: Vec<SymTransform>) {
        if texel_transforms.is_empty() {
            return;
        }
        self.last_dab = vec![None; texel_transforms.len()];
        self.symmetry = texel_transforms;
    }

    /// Arms clone tracking: the stroke starts at `start` (texels, local to
    /// `tile`) with the source `offset` texels away.
    pub(crate) fn begin_clone(
        &mut self,
        tile: u16,
        start: [f32; 2],
        offset: [f32; 2],
        follow: bool,
    ) {
        self.clone = Some(CloneState {
            tracker: CloneStroke::begin(start, offset, follow),
            tile,
        });
    }

    /// Whether events routed to `tile` take part in this stroke (clone:
    /// only its own tile; smudge: every tile).
    pub(crate) fn accepts(&self, tile: u16) -> bool {
        match (&self.tool, &self.clone) {
            (BrushTool::Paint, _) => false,
            (BrushTool::Clone(_), Some(clone)) => clone.tile == tile,
            (BrushTool::Clone(_), None) => false,
            (BrushTool::Smudge(_), _) => true,
        }
    }

    /// Whether `tile` already has its mirror for this stroke.
    pub(crate) fn has_tile(&self, tile: u16) -> bool {
        self.tiles.contains_key(&tile)
    }

    /// Installs `tile`'s mirror from its stroke-start readback.
    pub(crate) fn insert_tile(&mut self, tile: u16, image: RgbaBuffer) {
        self.tiles.insert(
            tile,
            TileMirror {
                before: image.clone(),
                live: image,
                dirty: None,
            },
        );
    }

    /// Runs one event's dabs on `tile`'s mirror (in stroke order, each
    /// primary dab followed by its symmetry siblings) and returns the rect
    /// they wrote — what the caller uploads. `None` when nothing changed
    /// (strength 0, an off-tile clone source, a tile with no mirror, ...).
    pub(crate) fn apply(&mut self, tile: u16, dabs: &[RetouchDab]) -> Option<DirtyRect> {
        if !self.accepts(tile) {
            return None;
        }
        let mirror = self.tiles.get_mut(&tile)?;
        let mut event_dirty = None;
        for primary in dabs {
            // The primary's clone source; siblings sample its images.
            let primary_src = self
                .clone
                .as_ref()
                .map(|clone| clone.tracker.source_for(primary.pos));
            for (k, pos) in sibling_positions(primary.pos, &self.symmetry, COINCIDENT_EPS_TEXELS) {
                let dab = RetouchDab { pos, ..*primary };
                let wrote = match self.tool {
                    BrushTool::Paint => None,
                    BrushTool::Smudge(params) => {
                        let motion = match self.last_dab[k] {
                            Some((last_tile, last)) if last_tile == tile => {
                                [pos[0] - last[0], pos[1] - last[1]]
                            }
                            _ => [0.0, 0.0],
                        };
                        smudge_dab(&mut mirror.live, &dab, params, motion)
                    }
                    BrushTool::Clone(_) => primary_src.and_then(|src| {
                        let src = self.symmetry[k].apply(src);
                        clone_dab(&mut mirror.live, &mirror.before, &dab, src)
                    }),
                };
                self.last_dab[k] = Some((tile, pos));
                event_dirty = DirtyRect::union_opt(event_dirty, wrote);
            }
        }
        mirror.dirty = DirtyRect::union_opt(mirror.dirty, event_dirty);
        event_dirty
    }

    /// `rect`'s current (live) bytes on `tile` — the upload payload.
    pub(crate) fn region(&self, tile: u16, rect: DirtyRect) -> Vec<u8> {
        self.tiles
            .get(&tile)
            .map(|mirror| mirror.live.region(rect))
            .unwrap_or_default()
    }

    /// Ends the stroke: ONE undo record covering every tile it wrote
    /// (each tile's dirty union, before + after), or `None` when the
    /// stroke changed nothing.
    pub(crate) fn finish(self) -> Option<PaintStrokeCommand> {
        let regions: Vec<TileRegion> = self
            .tiles
            .into_iter()
            .filter_map(|(tile, mirror)| {
                let rect = mirror.dirty?;
                Some(TileRegion {
                    tile,
                    rect,
                    before: mirror.before.region(rect),
                    after: mirror.live.region(rect),
                })
            })
            .collect();
        (!regions.is_empty()).then_some(PaintStrokeCommand { regions })
    }
}

/// One tile's slice of a stroke's undo record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TileRegion {
    pub tile: u16,
    pub rect: DirtyRect,
    /// The rect's bytes at stroke start.
    pub before: Vec<u8>,
    /// The rect's bytes at stroke end.
    pub after: Vec<u8>,
}

/// One retouch stroke as an undoable edit (one entry per stroke).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PaintStrokeCommand {
    pub regions: Vec<TileRegion>,
}

/// The pending region writes undo/redo produce: the [`Command::Doc`] of
/// [`PaintStrokeCommand`]. The owner drains `writes` into the GPU targets.
#[derive(Debug, Default)]
pub(crate) struct PaintWrites {
    /// `(tile, rect, bytes)`, in order.
    pub writes: Vec<(u16, DirtyRect, Vec<u8>)>,
}

impl Command for PaintStrokeCommand {
    type Doc = PaintWrites;

    fn apply(&mut self, doc: &mut PaintWrites) {
        for region in &self.regions {
            doc.writes
                .push((region.tile, region.rect, region.after.clone()));
        }
    }

    fn revert(&mut self, doc: &mut PaintWrites) {
        for region in &self.regions {
            doc.writes
                .push((region.tile, region.rect, region.before.clone()));
        }
    }

    fn memory_bytes(&self) -> usize {
        self.regions
            .iter()
            .map(|r| r.before.len() + r.after.len())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umber_brush::retouch::SmudgeParams;
    use umber_brush::symmetry::texel_transforms;
    use umber_brush::{CloneParams, MirrorAxis, MirrorSettings, SymmetrySettings};
    use umber_core::undo::UndoStack;

    const SIZE: u32 = 64;
    const TILE: u16 = 1001;

    fn hard_edge() -> RgbaBuffer {
        let mut buf = RgbaBuffer::transparent(SIZE, SIZE);
        for y in 0..SIZE {
            for x in 0..SIZE {
                let v = if x < SIZE / 2 { 0 } else { 255 };
                buf.set_texel(x, y, [v, v, v, 255]);
            }
        }
        buf
    }

    fn dab(pos: [f32; 2]) -> RetouchDab {
        RetouchDab {
            pos,
            radius: 8.0,
            alpha: 1.0,
            hardness: 1.0,
        }
    }

    fn smudge() -> BrushTool {
        BrushTool::Smudge(SmudgeParams {
            strength: 1.0,
            smear: 0.25,
        })
    }

    /// Drives one stroke over `image` the way `PaintState` does — mirror on
    /// first touch, apply per event, write the event rect back — and
    /// returns its undo record.
    fn run_stroke(
        image: &mut RgbaBuffer,
        tool: BrushTool,
        events: &[[f32; 2]],
    ) -> Option<PaintStrokeCommand> {
        run_stroke_with(image, tool, events, &SymmetrySettings::default())
    }

    /// [`run_stroke`] with `symmetry` armed the way `PaintState` arms it
    /// (texel-space transforms at the tile's texel density).
    fn run_stroke_with(
        image: &mut RgbaBuffer,
        tool: BrushTool,
        events: &[[f32; 2]],
        symmetry: &SymmetrySettings,
    ) -> Option<PaintStrokeCommand> {
        let mut stroke = RetouchStroke::new(tool);
        stroke.set_symmetry(texel_transforms(symmetry, SIZE as f32));
        if let BrushTool::Clone(params) = tool {
            let offset = [params.source_offset.0, params.source_offset.1];
            stroke.begin_clone(TILE, events[0], offset, params.follow);
        }
        for &pos in events {
            if !stroke.has_tile(TILE) {
                stroke.insert_tile(TILE, image.clone());
            }
            if let Some(rect) = stroke.apply(TILE, &[dab(pos)]) {
                image.write_region(rect, &stroke.region(TILE, rect));
            }
        }
        stroke.finish()
    }

    fn drain(writes: &mut PaintWrites, image: &mut RgbaBuffer) {
        for (tile, rect, bytes) in writes.writes.drain(..) {
            assert_eq!(tile, TILE);
            image.write_region(rect, &bytes);
        }
    }

    /// Design test 5: each retouch stroke is ONE undo entry (smudge and
    /// clone alike), undo restores the exact pre-stroke bytes, redo the
    /// exact post-stroke bytes.
    #[test]
    fn one_undo_entry_per_retouch_stroke() {
        let original = hard_edge();
        let mut image = original.clone();
        let mut history: UndoStack<PaintStrokeCommand> = UndoStack::new(16);
        let mut writes = PaintWrites::default();

        // Stroke 1: a multi-event smudge down the edge.
        let events: Vec<[f32; 2]> = (0..6).map(|i| [32.0, 16.0 + 4.0 * i as f32]).collect();
        let cmd = run_stroke(&mut image, smudge(), &events).expect("smudge changed texels");
        assert_eq!(cmd.regions.len(), 1, "one tile touched, one region");
        history.push(cmd, &mut writes);
        writes.writes.clear(); // the stroke itself already uploaded these
        assert_eq!(history.len(), 1, "six events, one stroke, one entry");
        let after_smudge = image.clone();
        assert_ne!(after_smudge, original);

        // Stroke 2: a clone stroke — its own single entry.
        let clone = BrushTool::Clone(CloneParams {
            source_offset: (20.0, 0.0),
            follow: true,
        });
        let cmd = run_stroke(&mut image, clone, &[[12.0, 50.0], [16.0, 50.0]])
            .expect("clone changed texels");
        history.push(cmd, &mut writes);
        writes.writes.clear();
        assert_eq!(history.len(), 2, "two strokes, two entries");
        let after_clone = image.clone();
        assert_ne!(after_clone, after_smudge);

        // Undo walks back one STROKE at a time, byte-exact.
        assert!(history.undo(&mut writes));
        drain(&mut writes, &mut image);
        assert_eq!(image, after_smudge);
        assert!(history.undo(&mut writes));
        drain(&mut writes, &mut image);
        assert_eq!(image, original);
        assert!(!history.undo(&mut writes));

        // Redo replays them.
        assert!(history.redo(&mut writes));
        drain(&mut writes, &mut image);
        assert_eq!(image, after_smudge);
        assert!(history.redo(&mut writes));
        drain(&mut writes, &mut image);
        assert_eq!(image, after_clone);
    }

    #[test]
    fn identity_stroke_records_nothing() {
        let mut image = hard_edge();
        let zero = BrushTool::Smudge(SmudgeParams {
            strength: 0.0,
            smear: 0.5,
        });
        assert!(run_stroke(&mut image, zero, &[[32.0, 32.0], [32.0, 40.0]]).is_none());
        assert_eq!(image, hard_edge());
    }

    #[test]
    fn clone_stays_on_its_own_tile_and_memory_is_counted() {
        let mut stroke = RetouchStroke::new(BrushTool::Clone(CloneParams::default()));
        stroke.begin_clone(TILE, [10.0, 10.0], [0.0, 0.0], true);
        assert!(stroke.accepts(TILE));
        assert!(!stroke.accepts(1002), "v1: clone never leaves its tile");
        stroke.insert_tile(1002, hard_edge());
        assert_eq!(stroke.apply(1002, &[dab([32.0, 32.0])]), None);

        let mut image = hard_edge();
        let cmd = run_stroke(&mut image, smudge(), &[[32.0, 30.0], [32.0, 34.0]])
            .expect("smudge changed texels");
        let r = &cmd.regions[0];
        assert_eq!(cmd.memory_bytes(), 2 * r.rect.byte_len());
        // Paint is never a retouch stroke.
        assert!(!RetouchStroke::new(BrushTool::Paint).accepts(TILE));
    }

    // --- Symmetry ---

    fn mirror_x() -> SymmetrySettings {
        SymmetrySettings {
            mirror: MirrorSettings {
                enabled: true,
                axis: MirrorAxis::X,
                offset: 0.5,
            },
            ..SymmetrySettings::default()
        }
    }

    /// White with two black bands mirror-symmetric about the tile's
    /// center line (`x ↦ 63 - x`): [12, 20) and [44, 52).
    fn symmetric_bands() -> RgbaBuffer {
        let mut buf = RgbaBuffer::transparent(SIZE, SIZE);
        for y in 0..SIZE {
            for x in 0..SIZE {
                let band = (12..20).contains(&x) || (44..52).contains(&x);
                let v = if band { 0 } else { 255 };
                buf.set_texel(x, y, [v, v, v, 255]);
            }
        }
        buf
    }

    /// `r = x * 4`: every column distinct, so a clone sample's column is
    /// readable from the bytes.
    fn column_gradient() -> RgbaBuffer {
        let mut buf = RgbaBuffer::transparent(SIZE, SIZE);
        for y in 0..SIZE {
            for x in 0..SIZE {
                buf.set_texel(x, y, [(x * 4) as u8, 0, 0, 255]);
            }
        }
        buf
    }

    /// Test (d): a mirrored smudge drags the mirrored way. On a
    /// mirror-symmetric image, a +x stroke kept clear of the axis (its
    /// sample windows and its mirror's never meet) leaves the image
    /// mirror-symmetric byte for byte — which holds only if the sibling's
    /// motion vector (hence its tilt) is the mirrored motion: an
    /// unmirrored +x tilt on the right side would drag the band the
    /// other way and break the equality.
    #[test]
    fn mirrored_smudge_smears_in_the_mirrored_direction() {
        let before = symmetric_bands();
        let mut image = before.clone();
        let events: Vec<[f32; 2]> = (0..3).map(|i| [12.0 + 4.0 * i as f32, 32.0]).collect();
        run_stroke_with(&mut image, smudge(), &events, &mirror_x()).expect("smudge wrote");
        assert_ne!(image, before, "the smudge must change texels");
        for y in 0..SIZE {
            for x in 0..SIZE {
                assert_eq!(
                    image.texel(x, y),
                    image.texel(SIZE - 1 - x, y),
                    "texel ({x},{y}) vs its mirror"
                );
            }
        }
        // And the right side really was smudged (not just left alone on a
        // symmetric image): the mirror band's edge moved.
        assert!((38..58).any(|x| image.texel(x, 32) != before.texel(x, 32)));

        // Control: the same stroke without symmetry breaks the symmetry.
        let mut plain = before.clone();
        run_stroke(&mut plain, smudge(), &events).expect("smudge wrote");
        assert!((0..SIZE).any(|x| plain.texel(x, 32) != plain.texel(SIZE - 1 - x, 32)));
    }

    /// The clone semantics: a sibling at `T(p)` samples `T(source)` — the
    /// offset mirrors with the point; the patch copies by translation.
    #[test]
    fn mirrored_clone_samples_the_mirrored_source() {
        let source = column_gradient();
        let mut image = source.clone();
        let clone = BrushTool::Clone(CloneParams {
            source_offset: (8.0, 0.0),
            follow: true,
        });
        run_stroke_with(&mut image, clone, &[[12.0, 32.0]], &mirror_x()).expect("clone wrote");
        // Primary: (12,32) samples (20,32).
        assert_eq!(image.texel(12, 32), source.texel(20, 32));
        // Sibling: (52,32) = T(12,32) samples T(20,32) = (44,32) — an
        // offset of -8, the mirrored +8.
        assert_eq!(image.texel(52, 32), source.texel(44, 32));
    }

    /// Test (e): the undo contract holds with symmetry — a multi-event
    /// mirrored stroke is ONE entry covering both sides, and undo restores
    /// the exact pre-stroke bytes on both.
    #[test]
    fn symmetric_stroke_is_one_undo_entry() {
        let original = symmetric_bands();
        let mut image = original.clone();
        let mut history: UndoStack<PaintStrokeCommand> = UndoStack::new(16);
        let mut writes = PaintWrites::default();
        let events: Vec<[f32; 2]> = (0..4).map(|i| [12.0 + 4.0 * i as f32, 30.0]).collect();
        let cmd = run_stroke_with(&mut image, smudge(), &events, &mirror_x())
            .expect("smudge changed texels");
        assert_eq!(cmd.regions.len(), 1, "both sides live in one tile region");
        let rect = cmd.regions[0].rect;
        assert!(
            rect.x0 < 32 && rect.x1 > 32,
            "the region spans both sides: {rect:?}"
        );
        history.push(cmd, &mut writes);
        writes.writes.clear();
        assert_eq!(history.len(), 1, "four events x two siblings, one entry");
        let after = image.clone();

        assert!(history.undo(&mut writes));
        drain(&mut writes, &mut image);
        assert_eq!(image, original, "undo restores both sides");
        assert!(!history.can_undo());
        assert!(history.redo(&mut writes));
        drain(&mut writes, &mut image);
        assert_eq!(image, after);
    }
}
