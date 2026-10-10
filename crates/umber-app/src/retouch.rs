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

use std::collections::BTreeMap;

use umber_brush::retouch::{clone_dab, smudge_dab, CloneStroke, DirtyRect, RetouchDab};
use umber_brush::{BrushTool, RgbaBuffer};
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
    /// The previous dab (tile, texel position): each smudge dab's motion
    /// vector is its position minus this one's, so the incoming stroke
    /// direction comes straight from the conditioned dab path. `None` at
    /// stroke start (the first dab has no direction → no tilt) and reset
    /// whenever the stroke crosses into another tile (local coordinates
    /// jump there).
    last_dab: Option<(u16, [f32; 2])>,
    clone: Option<CloneState>,
}

impl RetouchStroke {
    /// A fresh stroke for `tool` (a non-retouch tool yields a stroke that
    /// never writes).
    pub(crate) fn new(tool: BrushTool) -> Self {
        Self {
            tool,
            tiles: BTreeMap::new(),
            last_dab: None,
            clone: None,
        }
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

    /// Runs one event's dabs on `tile`'s mirror (in stroke order) and
    /// returns the rect they wrote — what the caller uploads. `None` when
    /// nothing changed (strength 0, an off-tile clone source, a tile with
    /// no mirror, ...).
    pub(crate) fn apply(&mut self, tile: u16, dabs: &[RetouchDab]) -> Option<DirtyRect> {
        if !self.accepts(tile) {
            return None;
        }
        let mirror = self.tiles.get_mut(&tile)?;
        let mut event_dirty = None;
        for dab in dabs {
            let wrote = match self.tool {
                BrushTool::Paint => None,
                BrushTool::Smudge(params) => {
                    let motion = match self.last_dab {
                        Some((last_tile, last)) if last_tile == tile => {
                            [dab.pos[0] - last[0], dab.pos[1] - last[1]]
                        }
                        _ => [0.0, 0.0],
                    };
                    smudge_dab(&mut mirror.live, dab, params, motion)
                }
                BrushTool::Clone(_) => self.clone.as_ref().and_then(|clone| {
                    let src = clone.tracker.source_for(dab.pos);
                    clone_dab(&mut mirror.live, &mirror.before, dab, src)
                }),
            };
            self.last_dab = Some((tile, dab.pos));
            event_dirty = DirtyRect::union_opt(event_dirty, wrote);
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
    use umber_brush::CloneParams;
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
        let mut stroke = RetouchStroke::new(tool);
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
}
