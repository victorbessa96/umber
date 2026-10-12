//! The retouch stroke session — the app half of the smudge + clone + eraser
//! brushes (docs/specs/smudge-clone-design.md; the eraser is §2's P0 row).
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
//! Undo (design test 5, "one undo per stroke"): [`RetouchStroke::record`]
//! records ONE [`PaintStrokeCommand`] per stroke — every touched tile's
//! dirty rect, before + after — for an [`UndoStack`] (an overlapping
//! successor may later coalesce into it — the cumulative merge below).
//! Its document is a [`PaintWrites`] queue, so applying/reverting a
//! command needs no GPU either: the caller drains the queue into
//! `WriteRegion` commands.
//!
//! Cumulative merge (audit 2026-10-12-c1 Pass 4 item 5, §12's undo-RAM
//! row): strokes reach the history through [`record_stroke`] (the retouch
//! path via [`RetouchStroke::record`], the paint path via `PaintState`'s
//! capture), which COALESCES a stroke into the top entry when they overlap
//! instead of stacking a second snapshot of the same texels. The merged
//! entry stores the oldest before + the newest after, never the
//! intermediate pair. Judgment calls:
//!
//! * Merge happens at PUSH time, against the TOP undo entry only (older
//!   entries are never rewritten). It needs the new stroke's FULL-tile
//!   start and end images ([`StrokeTile`]), not just its rect bytes: the
//!   merged rect is the bounding box `U` of the two rects, and `U`'s
//!   corners lie in neither rect, so neither entry holds their bytes. With
//!   the start image they are exact: `before(U)` = the stroke-start image
//!   over `U` with the old rect overwritten by the old entry's before;
//!   `after(U)` = the stroke-end image over `U`. Texels outside both rects
//!   were untouched by either stroke, so start = end there. Undoing the
//!   merged entry equals undoing both strokes in turn, byte for byte.
//! * Eligibility, per tile the stroke and the top entry SHARE: the rects
//!   must intersect (half-open, so edge-adjacent rects do NOT merge — the
//!   merge never eats a distinct region), and `|U| ≤ |old| + |new|` (bytes)
//!   — the size guard: a thin horizontal stroke crossing a thin vertical
//!   one has a bounding box far larger than the pair, so merging would
//!   GROW memory; those stack. Every shared tile must pass, and at least
//!   one tile must be shared; tiles on only one side carry over unchanged.
//!   An entry keeps at most one region per tile (the merge preserves it).
//! * Undo/redo granularity: a merged entry undoes AND redoes all of its
//!   strokes as one unit — repainting the same spot N times is one undo
//!   step. Accepted and named (the brief's call); split redo under merged
//!   storage is the follow-up, not v1.
//! * Redo replays the merged entry's after over all of `U`, including the
//!   old rect's texels outside the new rect. Those equal the old entry's
//!   after unless something outside the history wrote the tile between the
//!   strokes (a fill-layer upload) — the same last-write-wins property
//!   linear undo already has.
//! * The envelope is unchanged: 64 entries / 256 MiB (`PaintState`'s
//!   caps). The merge raises the EFFECTIVE stroke count inside it (N
//!   overlapping 4K full-tile strokes hold 128 MiB, not N × 128 MiB); it
//!   does not raise the caps.
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
//! * Erase: motion-independent — every sibling erases at its own texels
//!   with the same channel flags, so the mirror image is exact by
//!   construction (no per-sibling state to transform).
//!
//! All siblings land in the stroke's own tile mirror, so a symmetric
//! stroke is still ONE undo entry; its dirty rect is the bounding box of
//! every sibling (at 4K a full-tile radial stroke's before + after can
//! take half of the history's 256 MB total budget — a per-sibling rect
//! list is a follow-up).
//!
//! Quick mask (§2's P1 row, `umber_brush::quick_mask`): while the mask
//! gates, [`RetouchStroke::apply_masked`] multiplies every dab's per-texel
//! coverage by the tile's mask plane, for the primary and each sibling at
//! its own texels. Smudge, clone and eraser are gated the way the paint
//! shader gates a paint dab.

use std::collections::BTreeMap;

use umber_brush::retouch::{
    clone_dab_masked, erase_dab_masked, smudge_dab_masked, CloneStroke, DirtyRect, RetouchDab,
};
use umber_brush::symmetry::{sibling_positions, COINCIDENT_EPS_TEXELS};
use umber_brush::{BrushTool, MaskView, RgbaBuffer, SymTransform};
use umber_core::undo::{Command, UndoStack};

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
    /// only its own tile; smudge and eraser: every tile).
    pub(crate) fn accepts(&self, tile: u16) -> bool {
        match (&self.tool, &self.clone) {
            (BrushTool::Paint, _) => false,
            (BrushTool::Clone(_), Some(clone)) => clone.tile == tile,
            (BrushTool::Clone(_), None) => false,
            (BrushTool::Smudge(_), _) => true,
            (BrushTool::Erase(_), _) => true,
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
    #[cfg(test)]
    pub(crate) fn apply(&mut self, tile: u16, dabs: &[RetouchDab]) -> Option<DirtyRect> {
        self.apply_masked(tile, dabs, None)
    }

    /// [`Self::apply`] gated by the quick mask: `mask` is `tile`'s plane
    /// (`None` = ungated). It gates every sibling at the sibling's own
    /// texels, so the mask applies AFTER symmetry expansion.
    pub(crate) fn apply_masked(
        &mut self,
        tile: u16,
        dabs: &[RetouchDab],
        mask: Option<MaskView<'_>>,
    ) -> Option<DirtyRect> {
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
                        smudge_dab_masked(&mut mirror.live, &dab, params, motion, mask)
                    }
                    BrushTool::Clone(_) => primary_src.and_then(|src| {
                        let src = self.symmetry[k].apply(src);
                        clone_dab_masked(&mut mirror.live, &mirror.before, &dab, src, mask)
                    }),
                    // The eraser is motion-independent (like clone, unlike
                    // smudge): every sibling erases at its own texels with
                    // the same channel flags, so the mirror image is exact.
                    BrushTool::Erase(params) => {
                        erase_dab_masked(&mut mirror.live, &dab, params, mask)
                    }
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

    /// Every tile the stroke wrote, with its full stroke-start (`before`)
    /// and stroke-end (`live`) mirrors.
    fn stroke_tiles(&self) -> Vec<StrokeTile<'_>> {
        self.tiles
            .iter()
            .filter_map(|(&tile, mirror)| {
                Some(StrokeTile {
                    tile,
                    // Retouch tools (smudge/clone/erase) stay baseColor-only
                    // in v1 — a deliberate boundary (see `PaintState`'s
                    // module docs): they never consult the session's
                    // active channel.
                    channel: crate::paint_state::BASE_COLOR_CHANNEL.to_string(),
                    rect: mirror.dirty?,
                    start: &mirror.before,
                    end: &mirror.live,
                })
            })
            .collect()
    }

    /// Ends the stroke: ONE undo record covering every tile it wrote
    /// (each tile's dirty union, before + after), or `None` when the
    /// stroke changed nothing. Unmerged — the history path is
    /// [`Self::record`].
    #[cfg(test)]
    pub(crate) fn finish(self) -> Option<PaintStrokeCommand> {
        stroke_command(&self.stroke_tiles())
    }

    /// Ends the stroke onto `history` through [`record_stroke`] (merged
    /// into the top entry when they overlap). `false` when the stroke
    /// changed nothing.
    pub(crate) fn record(
        self,
        history: &mut UndoStack<PaintStrokeCommand>,
        writes: &mut PaintWrites,
    ) -> bool {
        record_stroke(history, writes, &self.stroke_tiles())
    }
}

/// One tile of a finished stroke, before it becomes an undo record: the
/// dirty rect plus the FULL-tile stroke-start and stroke-end images (the
/// merge reads texels outside the rect — see the module docs). `channel`
/// is the per-channel tile pool's undo key extension
/// (docs/specs/per-channel-tile-pool-design.md): a stroke only ever
/// touches ONE channel, so every `StrokeTile` a single stroke produces
/// shares it.
pub(crate) struct StrokeTile<'a> {
    pub tile: u16,
    pub channel: String,
    pub rect: DirtyRect,
    pub start: &'a RgbaBuffer,
    pub end: &'a RgbaBuffer,
}

/// Whether the half-open rects share at least one texel.
fn intersects(a: DirtyRect, b: DirtyRect) -> bool {
    a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1
}

/// Whether `rect` lies inside `image`.
fn fits(rect: DirtyRect, image: &RgbaBuffer) -> bool {
    rect.x1 <= image.width && rect.y1 <= image.height
}

/// The unmerged record of a stroke: one region per tile, its rect's
/// start + end bytes. `None` when no tile was written.
pub(crate) fn stroke_command(tiles: &[StrokeTile<'_>]) -> Option<PaintStrokeCommand> {
    let regions: Vec<TileRegion> = tiles
        .iter()
        .map(|t| TileRegion {
            tile: t.tile,
            channel: t.channel.clone(),
            rect: t.rect,
            before: t.start.region(t.rect),
            after: t.end.region(t.rect),
        })
        .collect();
    (!regions.is_empty()).then_some(PaintStrokeCommand { regions })
}

/// Whether `tiles` coalesce into `top` (the module docs' eligibility
/// rule): at least one shared (tile, channel), and every shared one's
/// rects intersect, pass the size guard, and fit the stroke's images. A
/// height stroke never coalesces with a baseColor entry on the same
/// tile — the channel is part of the undo key, not just the tile.
fn can_merge(top: &PaintStrokeCommand, tiles: &[StrokeTile<'_>]) -> bool {
    let mut shared = 0;
    for t in tiles {
        let Some(old) = top
            .regions
            .iter()
            .find(|r| r.tile == t.tile && r.channel == t.channel)
        else {
            continue;
        };
        let union = old.rect.union(t.rect);
        if !intersects(old.rect, t.rect)
            || union.byte_len() > old.rect.byte_len() + t.rect.byte_len()
            || !fits(union, t.start)
            || !fits(union, t.end)
        {
            return false;
        }
        shared += 1;
    }
    shared > 0
}

/// Folds the stroke into `top` (eligibility already checked by
/// [`can_merge`]): shared (tile, channel) pairs grow to the union with
/// the oldest before and the newest after, the rest carry over.
fn merge_stroke(top: PaintStrokeCommand, tiles: &[StrokeTile<'_>]) -> PaintStrokeCommand {
    let mut regions: BTreeMap<(u16, String), TileRegion> = top
        .regions
        .into_iter()
        .map(|r| ((r.tile, r.channel.clone()), r))
        .collect();
    for t in tiles {
        let key = (t.tile, t.channel.clone());
        let region = match regions.remove(&key) {
            Some(old) => {
                let union = old.rect.union(t.rect);
                let mut before = RgbaBuffer {
                    width: union.width(),
                    height: union.height(),
                    data: t.start.region(union),
                };
                // The old rect, in the union's local coordinates.
                let local = DirtyRect {
                    x0: old.rect.x0 - union.x0,
                    y0: old.rect.y0 - union.y0,
                    x1: old.rect.x1 - union.x0,
                    y1: old.rect.y1 - union.y0,
                };
                before.write_region(local, &old.before);
                TileRegion {
                    tile: t.tile,
                    channel: t.channel.clone(),
                    rect: union,
                    before: before.data,
                    after: t.end.region(union),
                }
            }
            None => TileRegion {
                tile: t.tile,
                channel: t.channel.clone(),
                rect: t.rect,
                before: t.start.region(t.rect),
                after: t.end.region(t.rect),
            },
        };
        regions.insert(key, region);
    }
    PaintStrokeCommand {
        regions: regions.into_values().collect(),
    }
}

/// Records a finished stroke on `history`: merged into the top entry when
/// [`can_merge`] allows, else pushed as its own entry. The push's queued
/// re-apply is dropped from `writes` (the stroke already uploaded its
/// bytes). `false` (history untouched) when no tile was written.
pub(crate) fn record_stroke(
    history: &mut UndoStack<PaintStrokeCommand>,
    writes: &mut PaintWrites,
    tiles: &[StrokeTile<'_>],
) -> bool {
    let merge = history.peek_undo().is_some_and(|top| can_merge(top, tiles));
    let command = if merge {
        history
            .take_top_unreverted()
            .map(|top| merge_stroke(top, tiles))
    } else {
        stroke_command(tiles)
    };
    let Some(command) = command else {
        return false;
    };
    history.push(command, writes);
    writes.writes.clear();
    true
}

/// One tile's slice of a stroke's undo record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TileRegion {
    pub tile: u16,
    /// The undo key's channel component (see [`StrokeTile::channel`]).
    pub channel: String,
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
    /// `(tile, channel, rect, bytes)`, in order.
    pub writes: Vec<(u16, String, DirtyRect, Vec<u8>)>,
}

impl Command for PaintStrokeCommand {
    type Doc = PaintWrites;

    fn apply(&mut self, doc: &mut PaintWrites) {
        for region in &self.regions {
            doc.writes.push((
                region.tile,
                region.channel.clone(),
                region.rect,
                region.after.clone(),
            ));
        }
    }

    fn revert(&mut self, doc: &mut PaintWrites) {
        for region in &self.regions {
            doc.writes.push((
                region.tile,
                region.channel.clone(),
                region.rect,
                region.before.clone(),
            ));
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
    use umber_brush::{
        CloneParams, EraseChannels, EraseParams, MirrorAxis, MirrorSettings, QuickMask,
        SymmetrySettings,
    };

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
        run_stroke_masked(image, tool, events, symmetry, None)
    }

    /// [`run_stroke_with`] gated by `mask` (the tile's quick-mask plane),
    /// the way `PaintState` gates a stroke while the quick mask is active.
    fn run_stroke_masked(
        image: &mut RgbaBuffer,
        tool: BrushTool,
        events: &[[f32; 2]],
        symmetry: &SymmetrySettings,
        mask: Option<MaskView<'_>>,
    ) -> Option<PaintStrokeCommand> {
        drive_stroke(image, tool, events, symmetry, mask).finish()
    }

    /// The stroke [`run_stroke_masked`] drives, left unfinished so a test
    /// can [`RetouchStroke::record`] it instead.
    fn drive_stroke(
        image: &mut RgbaBuffer,
        tool: BrushTool,
        events: &[[f32; 2]],
        symmetry: &SymmetrySettings,
        mask: Option<MaskView<'_>>,
    ) -> RetouchStroke {
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
            if let Some(rect) = stroke.apply_masked(TILE, &[dab(pos)], mask) {
                image.write_region(rect, &stroke.region(TILE, rect));
            }
        }
        stroke
    }

    fn drain(writes: &mut PaintWrites, image: &mut RgbaBuffer) {
        for (tile, channel, rect, bytes) in writes.writes.drain(..) {
            assert_eq!(tile, TILE);
            assert_eq!(channel, crate::paint_state::BASE_COLOR_CHANNEL);
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

    // --- Quick mask ---

    /// A mask allowing only a small hard disc at (32, 36), r 4: the
    /// strokes below cross far more than that.
    fn small_window_mask() -> QuickMask {
        let mut mask = QuickMask::new(SIZE, SIZE);
        mask.stamp(TILE, &hard_dab_r([32.0, 36.0], 4.0));
        mask
    }

    fn hard_dab_r(pos: [f32; 2], radius: f32) -> RetouchDab {
        RetouchDab {
            pos,
            radius,
            alpha: 1.0,
            hardness: 1.0,
        }
    }

    /// Texels that differ between `a` and `b` where `mask` reads
    /// `blocked` (exactly 0) or allowed (anything else).
    fn changed_where(a: &RgbaBuffer, b: &RgbaBuffer, mask: &QuickMask, blocked: bool) -> usize {
        let mut n = 0;
        for y in 0..SIZE {
            for x in 0..SIZE {
                let is_blocked = mask.sample(TILE, x, y) == 0.0;
                if is_blocked == blocked && a.texel(x, y) != b.texel(x, y) {
                    n += 1;
                }
            }
        }
        n
    }

    /// The two retouch strokes the gating tests run: a smudge across the
    /// hard edge, and a clone pulling the white half over the black.
    fn gated_strokes() -> Vec<(BrushTool, Vec<[f32; 2]>)> {
        let clone = BrushTool::Clone(CloneParams {
            source_offset: (8.0, 0.0),
            follow: true,
        });
        vec![
            (
                smudge(),
                (0..9).map(|i| [16.0 + 4.0 * i as f32, 34.0]).collect(),
            ),
            (clone, vec![[24.0, 36.0], [28.0, 36.0], [32.0, 36.0]]),
        ]
    }

    /// Test (a): a stroke through a blocked region leaves the layer
    /// unchanged there (before/after compare, texel for texel) and still
    /// paints the allowed window, for smudge and clone alike. The
    /// ungated control proves the stroke would have changed the blocked
    /// texels.
    #[test]
    fn quick_mask_gates_retouch_strokes_per_texel() {
        let mask = small_window_mask();
        let view = mask.view(TILE).expect("the stamp allocated the plane");
        for (tool, events) in gated_strokes() {
            let before = hard_edge();
            let mut plain = before.clone();
            run_stroke(&mut plain, tool, &events);
            assert!(
                changed_where(&before, &plain, &mask, true) > 0,
                "{tool:?}: the ungated stroke reaches the blocked region"
            );

            let mut gated = before.clone();
            run_stroke_masked(
                &mut gated,
                tool,
                &events,
                &SymmetrySettings::default(),
                Some(view),
            );
            assert_eq!(
                changed_where(&before, &gated, &mask, true),
                0,
                "{tool:?}: blocked texels must be byte-identical"
            );
            assert!(
                changed_where(&before, &gated, &mask, false) > 0,
                "{tool:?}: the allowed window still paints"
            );
        }
    }

    /// Test (b): inverting flips the gating: the window that painted is
    /// now the only place that does not, and what was blocked paints.
    #[test]
    fn inverted_quick_mask_flips_the_gating() {
        let mut mask = small_window_mask();
        mask.invert();
        let view = mask.view(TILE).expect("plane exists");
        for (tool, events) in gated_strokes() {
            let before = hard_edge();
            let mut gated = before.clone();
            run_stroke_masked(
                &mut gated,
                tool,
                &events,
                &SymmetrySettings::default(),
                Some(view),
            );
            assert_eq!(
                changed_where(&before, &gated, &mask, true),
                0,
                "{tool:?}: the inverted window is blocked"
            );
            assert!(
                changed_where(&before, &gated, &mask, false) > 0,
                "{tool:?}: the formerly blocked region paints"
            );
            // The window itself (allowed before the invert) is untouched.
            assert_eq!(gated.texel(31, 36), before.texel(31, 36));
        }
    }

    /// Opaque mid-gray (every channel erasable): the eraser tests'
    /// equivalent of `hard_edge`.
    fn gray_opaque() -> RgbaBuffer {
        let mut buf = RgbaBuffer::transparent(SIZE, SIZE);
        for y in 0..SIZE {
            for x in 0..SIZE {
                buf.set_texel(x, y, [200, 120, 60, 255]);
            }
        }
        buf
    }

    fn eraser() -> BrushTool {
        BrushTool::Erase(EraseParams::default())
    }

    /// Test (d): one eraser stroke is ONE undo entry and undo restores the
    /// exact pre-stroke bytes (the non-destructive contract: the erase
    /// touches layer channels only, and the before-state comes back whole).
    #[test]
    fn eraser_stroke_is_one_undo_entry_restoring_before_state() {
        let original = gray_opaque();
        let mut image = original.clone();
        let mut history: UndoStack<PaintStrokeCommand> = UndoStack::new(16);
        let mut writes = PaintWrites::default();

        let events: Vec<[f32; 2]> = (0..6).map(|i| [16.0 + 4.0 * i as f32, 32.0]).collect();
        let cmd = run_stroke(&mut image, eraser(), &events).expect("erase changed texels");
        assert_eq!(cmd.regions.len(), 1, "one tile touched, one region");
        history.push(cmd, &mut writes);
        writes.writes.clear(); // the stroke itself already uploaded these
        assert_eq!(history.len(), 1, "six events, one stroke, one entry");
        let after_erase = image.clone();
        assert_ne!(after_erase, original);
        assert_eq!(image.texel(24, 32)[3], 0, "the stroke's path is erased");
        assert_eq!(image.texel(24, 32)[0], 200, "a-only erase keeps color");
        assert_eq!(
            image.texel(56, 56),
            [200, 120, 60, 255],
            "far texels untouched"
        );

        assert!(history.undo(&mut writes));
        drain(&mut writes, &mut image);
        assert_eq!(image, original, "undo restores the exact before-state");
        assert!(!history.can_undo());

        assert!(history.redo(&mut writes));
        drain(&mut writes, &mut image);
        assert_eq!(image, after_erase);
    }

    /// Test (e): the eraser mirrors like paint. On a mirror-symmetric
    /// image, a stroke kept clear of the axis leaves the image
    /// mirror-symmetric byte for byte; the control without symmetry breaks
    /// it. The undo contract holds with symmetry: one entry, both sides
    /// restored.
    #[test]
    fn mirrored_eraser_erases_both_sides_symmetrically() {
        // Mirror-symmetric gray with a distinct left/right-readable alpha:
        // symmetric_bands is black/white; erase its alpha instead.
        let before = symmetric_bands();
        let mut image = before.clone();
        let events: Vec<[f32; 2]> = (0..3).map(|i| [12.0 + 4.0 * i as f32, 32.0]).collect();
        let cmd = run_stroke_with(&mut image, eraser(), &events, &mirror_x()).expect("erase wrote");
        assert_eq!(cmd.regions.len(), 1, "both sides live in one tile region");
        assert_ne!(image, before, "the eraser must change texels");
        for y in 0..SIZE {
            for x in 0..SIZE {
                assert_eq!(
                    image.texel(x, y),
                    image.texel(SIZE - 1 - x, y),
                    "texel ({x},{y}) vs its mirror"
                );
            }
        }
        assert!(
            (38..58).any(|x| image.texel(x, 32) != before.texel(x, 32)),
            "the mirror band's edge really was erased"
        );

        // Control: the same stroke without symmetry breaks the symmetry.
        let mut plain = before.clone();
        run_stroke(&mut plain, eraser(), &events).expect("erase wrote");
        assert!((0..SIZE).any(|x| plain.texel(x, 32) != plain.texel(SIZE - 1 - x, 32)));

        // Undo restores both sides in one entry.
        let mut history: UndoStack<PaintStrokeCommand> = UndoStack::new(16);
        let mut writes = PaintWrites::default();
        let mut image2 = before.clone();
        let cmd =
            run_stroke_with(&mut image2, eraser(), &events, &mirror_x()).expect("erase wrote");
        history.push(cmd, &mut writes);
        writes.writes.clear();
        assert_eq!(history.len(), 1);
        assert!(history.undo(&mut writes));
        drain(&mut writes, &mut image2);
        assert_eq!(image2, before, "undo restores both sides");
    }

    /// Test (f): the quick mask gates the eraser per texel — blocked
    /// texels stay byte-identical while the allowed window erases. The
    /// ungated control proves the stroke would have erased the blocked
    /// texels.
    #[test]
    fn quick_mask_gates_the_eraser_per_texel() {
        let mask = small_window_mask();
        let view = mask.view(TILE).expect("the stamp allocated the plane");
        // A stroke across the hard edge's middle: the window (32,36,r4)
        // sits inside its footprint.
        let events: Vec<[f32; 2]> = (0..9).map(|i| [16.0 + 4.0 * i as f32, 34.0]).collect();

        let before = gray_opaque();
        let mut plain = before.clone();
        run_stroke(&mut plain, eraser(), &events).expect("erase wrote");
        assert!(
            changed_where(&before, &plain, &mask, true) > 0,
            "the ungated stroke reaches the blocked region"
        );

        let mut gated = before.clone();
        run_stroke_masked(
            &mut gated,
            eraser(),
            &events,
            &SymmetrySettings::default(),
            Some(view),
        )
        .expect("the allowed window still erases");
        assert_eq!(
            changed_where(&before, &gated, &mask, true),
            0,
            "blocked texels must be byte-identical"
        );
        assert!(
            changed_where(&before, &gated, &mask, false) > 0,
            "the allowed window still erases"
        );
        // The eraser's own invariant under the gate: allowed texels lost
        // alpha only (default a-only channels).
        assert_eq!(gated.texel(32, 36)[3], 0);
        assert_eq!(&gated.texel(32, 36)[..3], &[200, 120, 60]);
    }

    /// The eraser's per-channel flags survive the stroke session: an
    /// RGB-erase stroke zeroes color leaving alpha, through the same
    /// mirror/undo path as the default.
    #[test]
    fn eraser_channel_flags_apply_through_the_stroke_session() {
        let rgb_erase = BrushTool::Erase(EraseParams {
            channels: EraseChannels {
                r: true,
                g: true,
                b: true,
                a: false,
            },
        });
        let original = gray_opaque();
        let mut image = original.clone();
        let mut history: UndoStack<PaintStrokeCommand> = UndoStack::new(16);
        let mut writes = PaintWrites::default();
        let cmd =
            run_stroke(&mut image, rgb_erase, &[[32.0, 32.0]]).expect("rgb erase changed texels");
        history.push(cmd, &mut writes);
        writes.writes.clear();
        assert_eq!(image.texel(32, 32), [0, 0, 0, 255]);
        assert!(history.undo(&mut writes));
        drain(&mut writes, &mut image);
        assert_eq!(image, original);
    }

    /// Test (d): the mask applies AFTER symmetry expansion. Every sibling
    /// is gated at its own texels: with the right half blocked, a left-side
    /// mirrored smudge leaves the right half (where its sibling lands)
    /// untouched, while the ungated control smudges it.
    #[test]
    fn quick_mask_gates_symmetry_siblings() {
        let before = symmetric_bands();
        let events: Vec<[f32; 2]> = (0..3).map(|i| [12.0 + 4.0 * i as f32, 32.0]).collect();
        // Allow a disc on the left half only (x in 2..30): the whole right
        // half reads blocked.
        let mut mask = QuickMask::new(SIZE, SIZE);
        mask.stamp(TILE, &hard_dab_r([16.0, 32.0], 14.0));
        assert!((32..SIZE).all(|x| mask.sample(TILE, x, 32) == 0.0));

        let mut plain = before.clone();
        run_stroke_with(&mut plain, smudge(), &events, &mirror_x()).expect("smudge wrote");
        let right_changed = |img: &RgbaBuffer| {
            (32..SIZE).any(|x| (0..SIZE).any(|y| img.texel(x, y) != before.texel(x, y)))
        };
        assert!(
            right_changed(&plain),
            "control: the sibling smudges the right half"
        );

        let mut gated = before.clone();
        run_stroke_masked(&mut gated, smudge(), &events, &mirror_x(), mask.view(TILE))
            .expect("the primary still smudges the allowed left side");
        assert!(!right_changed(&gated), "the blocked sibling writes nothing");
        assert_eq!(changed_where(&before, &gated, &mask, true), 0);
        assert!(changed_where(&before, &gated, &mask, false) > 0);
    }

    // --- Cumulative merge ---

    fn rect(x0: u32, y0: u32, x1: u32, y1: u32) -> DirtyRect {
        DirtyRect { x0, y0, x1, y1 }
    }

    /// A synthetic stroke on `image`: fills `r` with opaque gray `value`
    /// and returns the stroke's full-tile `(start, end)` images.
    fn fill_stroke(image: &mut RgbaBuffer, r: DirtyRect, value: u8) -> (RgbaBuffer, RgbaBuffer) {
        let start = image.clone();
        for y in r.y0..r.y1 {
            for x in r.x0..r.x1 {
                image.set_texel(x, y, [value, value, value, 255]);
            }
        }
        (start, image.clone())
    }

    /// Paints `r` on `image` and records it on `history` the way
    /// `end_stroke` does.
    fn record_fill(
        history: &mut UndoStack<PaintStrokeCommand>,
        writes: &mut PaintWrites,
        image: &mut RgbaBuffer,
        r: DirtyRect,
        value: u8,
    ) {
        let (start, end) = fill_stroke(image, r, value);
        let tiles = [StrokeTile {
            tile: TILE,
            channel: crate::paint_state::BASE_COLOR_CHANNEL.to_string(),
            rect: r,
            start: &start,
            end: &end,
        }];
        assert!(record_stroke(history, writes, &tiles));
        assert!(writes.writes.is_empty(), "the push's re-apply is dropped");
    }

    /// The soak: N = 8 strokes, each a 16×16 rect shifted 4 texels right
    /// of the last (`R_k = [8+4k, 24+4k) × [8, 24)`), every one
    /// overlapping its predecessor. Pins (a) count + bytes, (b) undo of
    /// the merged entry restores the pre-first-stroke image, (d) the
    /// delta against the unmerged baseline.
    #[test]
    fn overlapping_strokes_merge_into_one_entry_with_exact_bytes() {
        const N: u32 = 8;
        let original = hard_edge();
        let mut image = original.clone();
        let mut history: UndoStack<PaintStrokeCommand> = UndoStack::new(64);
        let mut writes = PaintWrites::default();
        // After stroke k the entry's rect is the running union
        // [8, 24+4k) × [8, 24): (16 + 4k) × 16 texels × 4 bytes × 2 sides.
        let expected_bytes = [2048, 2560, 3072, 3584, 4096, 4608, 5120, 5632];
        for k in 0..N {
            let r = rect(8 + 4 * k, 8, 24 + 4 * k, 24);
            record_fill(&mut history, &mut writes, &mut image, r, 10 * (k as u8 + 1));
            assert_eq!(history.len(), 1, "stroke {k} coalesces into the one entry");
            assert_eq!(
                history.bytes_held(),
                expected_bytes[k as usize],
                "stroke {k}"
            );
        }
        let top = history.peek_undo().expect("one entry");
        assert_eq!(top.regions.len(), 1);
        assert_eq!(top.regions[0].rect, rect(8, 8, 52, 24));
        let painted = image.clone();

        // (d) The unmerged baseline: the same strokes as plain entries,
        // 8 × (16·16·4·2 = 2048) = 16384 bytes; merged 44·16·4·2 = 5632.
        let mut baseline: UndoStack<PaintStrokeCommand> = UndoStack::new(64);
        let mut scratch = original.clone();
        for k in 0..N {
            let r = rect(8 + 4 * k, 8, 24 + 4 * k, 24);
            let (start, end) = fill_stroke(&mut scratch, r, 10 * (k as u8 + 1));
            let cmd = stroke_command(&[StrokeTile {
                tile: TILE,
                channel: crate::paint_state::BASE_COLOR_CHANNEL.to_string(),
                rect: r,
                start: &start,
                end: &end,
            }])
            .expect("the fill writes");
            baseline.push(cmd, &mut writes);
        }
        writes.writes.clear();
        assert_eq!(baseline.len(), 8);
        assert_eq!(baseline.bytes_held(), 16384);
        assert_eq!(baseline.bytes_held() - history.bytes_held(), 10752);

        // (b) One undo restores the ORIGINAL pre-first-stroke bytes over
        // the whole image (the union's corners included), and redo the
        // final state.
        assert!(history.undo(&mut writes));
        drain(&mut writes, &mut image);
        assert_eq!(image, original);
        assert!(!history.can_undo(), "the eight strokes were one entry");
        assert!(history.redo(&mut writes));
        drain(&mut writes, &mut image);
        assert_eq!(image, painted, "the merged unit redoes as one");
    }

    /// (c) Strokes that share no texel stack one entry each — including
    /// an edge-adjacent pair (half-open rects touch but do not intersect,
    /// though their union would pass the size guard). 4 × 2048 bytes.
    #[test]
    fn non_overlapping_strokes_stack_one_entry_each() {
        let original = hard_edge();
        let mut image = original.clone();
        let mut history: UndoStack<PaintStrokeCommand> = UndoStack::new(64);
        let mut writes = PaintWrites::default();
        let rects = [
            rect(0, 0, 16, 16),
            rect(16, 0, 32, 16), // shares the x = 16 edge with the first
            rect(40, 0, 56, 16),
            rect(0, 40, 16, 56),
        ];
        let mut states = vec![image.clone()];
        for (k, &r) in rects.iter().enumerate() {
            record_fill(&mut history, &mut writes, &mut image, r, 50 + k as u8);
            assert_eq!(history.len(), k + 1);
            states.push(image.clone());
        }
        assert_eq!(history.bytes_held(), 4 * 2048);

        // Each undo walks back exactly one stroke.
        for k in (0..rects.len()).rev() {
            assert!(history.undo(&mut writes));
            drain(&mut writes, &mut image);
            assert_eq!(image, states[k], "undo of stroke {k}");
        }
        assert_eq!(image, original);
        assert!(!history.can_undo());
    }

    /// The size guard: a thin horizontal stroke crossing a thin vertical
    /// one intersects, but the union (64·64 = 4096 texels) dwarfs the pair
    /// (64·4 + 4·64 = 512) — merging would GROW memory, so they stack:
    /// 2 entries, 2 × (256·4·2 = 2048) = 4096 bytes (merged would be
    /// 4096·4·2 = 32768).
    #[test]
    fn crossing_strokes_whose_union_outgrows_the_pair_stack() {
        let original = hard_edge();
        let mut image = original.clone();
        let mut history: UndoStack<PaintStrokeCommand> = UndoStack::new(64);
        let mut writes = PaintWrites::default();
        record_fill(
            &mut history,
            &mut writes,
            &mut image,
            rect(0, 30, 64, 34),
            90,
        );
        let first = image.clone();
        record_fill(
            &mut history,
            &mut writes,
            &mut image,
            rect(30, 0, 34, 64),
            180,
        );
        assert_eq!(history.len(), 2);
        assert_eq!(history.bytes_held(), 4096);

        assert!(history.undo(&mut writes));
        drain(&mut writes, &mut image);
        assert_eq!(image, first, "the crossing stroke undoes alone");
        assert!(history.undo(&mut writes));
        drain(&mut writes, &mut image);
        assert_eq!(image, original);
    }

    /// Multi-tile: a stroke overlapping the top entry on one tile and
    /// touching a fresh tile merges, carrying the fresh tile's region in;
    /// a later stroke whose only SHARED tile is disjoint stacks.
    #[test]
    fn merge_carries_unshared_tiles_and_disjoint_shared_tiles_stack() {
        const OTHER: u16 = 1002;
        let mut images = BTreeMap::from([(TILE, hard_edge()), (OTHER, gray_opaque())]);
        let originals = images.clone();
        let mut history: UndoStack<PaintStrokeCommand> = UndoStack::new(64);
        let mut writes = PaintWrites::default();

        // Stroke 1: TILE only, 16×16 → 2048 bytes.
        let r1 = rect(8, 8, 24, 24);
        let (s1, e1) = fill_stroke(images.get_mut(&TILE).unwrap(), r1, 30);
        let tiles = [StrokeTile {
            tile: TILE,
            channel: crate::paint_state::BASE_COLOR_CHANNEL.to_string(),
            rect: r1,
            start: &s1,
            end: &e1,
        }];
        assert!(record_stroke(&mut history, &mut writes, &tiles));

        // Stroke 2: overlaps on TILE ([16,32)×[8,24) → union [8,32)×[8,24),
        // 24·16·4·2 = 3072) and writes OTHER fresh (16·16·4·2 = 2048).
        let r2 = rect(16, 8, 32, 24);
        let r2_other = rect(0, 0, 16, 16);
        let (s2, e2) = fill_stroke(images.get_mut(&TILE).unwrap(), r2, 60);
        let (s2o, e2o) = fill_stroke(images.get_mut(&OTHER).unwrap(), r2_other, 120);
        let tiles = [
            StrokeTile {
                tile: TILE,
                channel: crate::paint_state::BASE_COLOR_CHANNEL.to_string(),
                rect: r2,
                start: &s2,
                end: &e2,
            },
            StrokeTile {
                tile: OTHER,
                channel: crate::paint_state::BASE_COLOR_CHANNEL.to_string(),
                rect: r2_other,
                start: &s2o,
                end: &e2o,
            },
        ];
        assert!(record_stroke(&mut history, &mut writes, &tiles));
        assert_eq!(history.len(), 1);
        assert_eq!(history.peek_undo().unwrap().regions.len(), 2);
        assert_eq!(history.bytes_held(), 3072 + 2048);
        let after_two = images.clone();

        // Stroke 3: TILE only, disjoint from the entry's TILE rect → stacks.
        let r3 = rect(40, 40, 48, 48);
        let (s3, e3) = fill_stroke(images.get_mut(&TILE).unwrap(), r3, 200);
        let tiles = [StrokeTile {
            tile: TILE,
            channel: crate::paint_state::BASE_COLOR_CHANNEL.to_string(),
            rect: r3,
            start: &s3,
            end: &e3,
        }];
        assert!(record_stroke(&mut history, &mut writes, &tiles));
        assert_eq!(history.len(), 2);
        assert_eq!(history.bytes_held(), 3072 + 2048 + 512);

        assert!(history.undo(&mut writes));
        drain_tiles(&mut writes, &mut images);
        assert!(history.undo(&mut writes));
        drain_tiles(&mut writes, &mut images);
        assert_eq!(images, originals, "both tiles back to their originals");
        assert!(history.redo(&mut writes));
        drain_tiles(&mut writes, &mut images);
        assert_eq!(images, after_two, "the merged unit redoes both tiles");
    }

    /// [`drain`] across several tiles.
    fn drain_tiles(writes: &mut PaintWrites, images: &mut BTreeMap<u16, RgbaBuffer>) {
        for (tile, channel, r, bytes) in writes.writes.drain(..) {
            assert_eq!(channel, crate::paint_state::BASE_COLOR_CHANNEL);
            images
                .get_mut(&tile)
                .expect("a tile the test owns")
                .write_region(r, &bytes);
        }
    }

    /// The retouch wiring: two overlapping smudge strokes recorded through
    /// [`RetouchStroke::record`] land as ONE entry, and its undo restores
    /// the pre-first-stroke image.
    #[test]
    fn overlapping_retouch_strokes_record_as_one_entry() {
        let original = hard_edge();
        let mut image = original.clone();
        let mut history: UndoStack<PaintStrokeCommand> = UndoStack::new(64);
        let mut writes = PaintWrites::default();
        let none = SymmetrySettings::default();
        let down: Vec<[f32; 2]> = (0..6).map(|i| [32.0, 16.0 + 4.0 * i as f32]).collect();
        // Back up the same edge: the same footprint column, so the rects
        // nearly coincide (the union passes the size guard by a wide margin).
        let up: Vec<[f32; 2]> = (0..6).map(|i| [33.0, 36.0 - 4.0 * i as f32]).collect();

        let stroke = drive_stroke(&mut image, smudge(), &down, &none, None);
        assert!(stroke.record(&mut history, &mut writes));
        let after_first = image.clone();
        assert_ne!(after_first, original);
        let stroke = drive_stroke(&mut image, smudge(), &up, &none, None);
        assert!(stroke.record(&mut history, &mut writes));
        assert_ne!(image, after_first, "the second smudge moves texels too");
        assert_eq!(history.len(), 1, "the overlapping smudge coalesced");

        assert!(history.undo(&mut writes));
        drain(&mut writes, &mut image);
        assert_eq!(image, original);
        assert!(!history.can_undo());
    }
}
