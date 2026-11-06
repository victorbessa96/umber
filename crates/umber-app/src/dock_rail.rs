//! Collapsible side columns (UI/UX U7, the photocraft icon-rail pattern;
//! spec section 9). A side column of the dock folds into a narrow icon
//! rail and unfolds back to the column it was.
//!
//! This module is the dock-tree half and holds no UI: it finds a side
//! column, lifts its tabs out of the dock into a [`RailColumn`], and puts
//! them back. The rail strip itself is `umber_gui::IconRail`, and the
//! shell decides what to draw.
//!
//! - A **side column** is the outermost child on one side of the main
//!   surface's horizontal splits that holds no protected tab (the 3D and
//!   2D views). In every preset that is the project column on the left
//!   and the Layers/Properties stack on the right.
//! - **Collapse** records each of the column's leaves top to bottom (its
//!   tabs, its active tab, its share of the column height) and the
//!   column's share of the window width, then removes the tabs. egui_dock
//!   rebalances the tree, so the centre takes the width.
//! - **Restore** splits the root on that side at the recorded width and
//!   stacks the leaves back with their recorded heights. A column that
//!   had leaves side by side comes back stacked; no preset has one.
//! - Nothing here moves a protected tab, and a panel the user docked
//!   somewhere else while the column was folded is not restored twice
//!   (it stays where the user put it).
//!
//! Generic over the tab type so the tests run without the shell's
//! `Panel` (which lives in the binary).

use egui_dock::{DockState, Node, NodeIndex, Tree};

/// Which side of the centre a column sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Side {
    Left,
    Right,
}

/// One leaf of a folded column: its tabs in order, the active one, and
/// its share of the column height (the shares of a column sum to 1).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RailLeaf<T> {
    pub tabs: Vec<T>,
    pub active: usize,
    pub height: f32,
}

/// A folded side column: everything [`restore`] needs to put it back.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RailColumn<T> {
    pub side: Side,
    /// The column's share of the window width when it was folded.
    pub width: f32,
    pub leaves: Vec<RailLeaf<T>>,
}

impl<T> RailColumn<T> {
    /// Every tab on the rail, top to bottom (the rail's icon order).
    pub fn tabs(&self) -> impl Iterator<Item = &T> {
        self.leaves.iter().flat_map(|l| l.tabs.iter())
    }
}

/// The narrowest and widest a restored column may be, as a share of the
/// window. A recorded share outside this (a corrupt file, a column that
/// had been dragged almost shut) is clamped so the column comes back
/// usable and the centre keeps most of the room.
pub const WIDTH_SHARE: std::ops::RangeInclusive<f32> = 0.10..=0.45;

fn in_tree<T>(tree: &Tree<T>, node: NodeIndex) -> bool {
    node.0 < tree.len()
}

/// Whether any leaf under `node` holds a tab matching `protected`.
fn subtree_has<T>(tree: &Tree<T>, node: NodeIndex, protected: &impl Fn(&T) -> bool) -> bool {
    if !in_tree(tree, node) {
        return false;
    }
    match &tree[node] {
        Node::Empty => false,
        Node::Leaf(leaf) => leaf.tabs.iter().any(protected),
        Node::Horizontal(_) | Node::Vertical(_) => {
            subtree_has(tree, node.left(), protected) || subtree_has(tree, node.right(), protected)
        }
    }
}

/// The side column on `side`, with its share of the window width: walk
/// the horizontal splits from the root toward `side` until a child holds
/// no protected tab. None when that side has no such column (it is
/// already folded, or the user docked a view there).
pub fn side_column<T>(
    dock: &DockState<T>,
    side: Side,
    protected: impl Fn(&T) -> bool,
) -> Option<(NodeIndex, f32)> {
    let tree = dock.main_surface();
    let mut node = NodeIndex::root();
    let mut share = 1.0f32;
    loop {
        if !in_tree(tree, node) {
            return None;
        }
        let Node::Horizontal(split) = &tree[node] else {
            return None;
        };
        let (child, child_share) = match side {
            Side::Left => (node.left(), split.fraction),
            Side::Right => (node.right(), 1.0 - split.fraction),
        };
        share *= child_share;
        if !subtree_has(tree, child, &protected) {
            return in_tree(tree, child).then_some((child, share));
        }
        node = child;
    }
}

/// The leaves under `node` top to bottom (left to right inside a
/// horizontal split), each with its share of `node`'s height.
fn collect_leaves<T: Clone>(
    tree: &Tree<T>,
    node: NodeIndex,
    height: f32,
    out: &mut Vec<RailLeaf<T>>,
) {
    if !in_tree(tree, node) {
        return;
    }
    match &tree[node] {
        Node::Empty => {}
        Node::Leaf(leaf) => {
            if !leaf.tabs.is_empty() {
                out.push(RailLeaf {
                    tabs: leaf.tabs.clone(),
                    active: leaf.active.0.min(leaf.tabs.len() - 1),
                    height,
                });
            }
        }
        Node::Vertical(split) => {
            collect_leaves(tree, node.left(), height * split.fraction, out);
            collect_leaves(tree, node.right(), height * (1.0 - split.fraction), out);
        }
        Node::Horizontal(_) => {
            // Side by side inside a column: stacked on restore, each
            // keeping the full share it spanned.
            let mut inner = Vec::new();
            collect_leaves(tree, node.left(), height, &mut inner);
            collect_leaves(tree, node.right(), height, &mut inner);
            let n = inner.len().max(1) as f32;
            for mut l in inner {
                l.height /= n;
                out.push(l);
            }
        }
    }
}

/// Folds the side column on `side` out of `dock`. None (dock untouched)
/// when that side has no foldable column.
pub fn collapse<T: Clone + PartialEq>(
    dock: &mut DockState<T>,
    side: Side,
    protected: impl Fn(&T) -> bool,
) -> Option<RailColumn<T>> {
    let (node, width) = side_column(dock, side, &protected)?;
    let mut leaves = Vec::new();
    collect_leaves(dock.main_surface(), node, 1.0, &mut leaves);
    if leaves.is_empty() {
        return None;
    }
    for tab in leaves.iter().flat_map(|l| l.tabs.iter()) {
        if let Some(path) = dock.find_tab(tab) {
            dock.remove_tab(path);
        }
    }
    Some(RailColumn {
        side,
        width,
        leaves,
    })
}

/// Unfolds `column` back into `dock` on its side. Tabs the user docked
/// elsewhere meanwhile stay where they are; a leaf left with no tabs is
/// skipped. Returns whether anything was docked.
pub fn restore<T: Clone + PartialEq>(dock: &mut DockState<T>, column: RailColumn<T>) -> bool {
    let leaves: Vec<RailLeaf<T>> = column
        .leaves
        .into_iter()
        .filter_map(|mut l| {
            let keep_active = l.tabs.get(l.active).cloned();
            l.tabs.retain(|t| dock.find_tab(t).is_none());
            if l.tabs.is_empty() {
                return None;
            }
            l.active = keep_active
                .and_then(|a| l.tabs.iter().position(|t| *t == a))
                .unwrap_or(0);
            Some(l)
        })
        .collect();
    if leaves.is_empty() {
        return false;
    }
    let width = column.width.clamp(*WIDTH_SHARE.start(), *WIDTH_SHARE.end());
    let tree = dock.main_surface_mut();
    if tree.is_empty() {
        // Nothing left to sit beside: the column becomes the layout.
        *tree = Tree::new(leaves[0].tabs.clone());
    }
    // egui_dock's split fraction is the FIRST (left/top) child's share,
    // and `split_left` puts the new leaf on the left (`shell_menu::
    // default_dock`'s notes).
    let [_, mut leaf] = match column.side {
        Side::Left => {
            let [old, new] = tree.split_left(NodeIndex::root(), width, leaves[0].tabs.clone());
            [old, new]
        }
        Side::Right => tree.split_right(NodeIndex::root(), 1.0 - width, leaves[0].tabs.clone()),
    };
    let _ = tree.set_active_tab(leaf, egui_dock::TabIndex(leaves[0].active));
    let total: f32 = leaves.iter().map(|l| l.height.max(0.0)).sum();
    let mut remaining = if total > 0.0 {
        total
    } else {
        leaves.len() as f32
    };
    let height = |l: &RailLeaf<T>| if total > 0.0 { l.height.max(0.0) } else { 1.0 };
    for i in 1..leaves.len() {
        let above = height(&leaves[i - 1]);
        let top = if remaining > 0.0 {
            (above / remaining).clamp(0.05, 0.95)
        } else {
            0.5
        };
        remaining -= above;
        let [_, below] = tree.split_below(leaf, top, leaves[i].tabs.clone());
        let _ = tree.set_active_tab(below, egui_dock::TabIndex(leaves[i].active));
        leaf = below;
    }
    true
}

/// The shell's fold state: at most one folded column per side, and the
/// rail item whose flyout is open.
#[derive(Debug, Clone, PartialEq)]
pub struct Rails<T> {
    left: Option<RailColumn<T>>,
    right: Option<RailColumn<T>>,
    /// The open flyout: the side and the item index on that rail.
    pub flyout: Option<(Side, usize)>,
}

impl<T> Default for Rails<T> {
    fn default() -> Self {
        Self {
            left: None,
            right: None,
            flyout: None,
        }
    }
}

impl<T: Clone + PartialEq> Rails<T> {
    fn slot(&mut self, side: Side) -> &mut Option<RailColumn<T>> {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        }
    }

    /// The folded column on `side`, if any.
    pub fn column(&self, side: Side) -> Option<&RailColumn<T>> {
        match side {
            Side::Left => self.left.as_ref(),
            Side::Right => self.right.as_ref(),
        }
    }

    /// The sides currently folded, left first (what the layouts file
    /// records).
    pub fn folded_sides(&self) -> Vec<Side> {
        [Side::Left, Side::Right]
            .into_iter()
            .filter(|s| self.column(*s).is_some())
            .collect()
    }

    /// Folds `side` when it is open, unfolds it when it is folded.
    /// Returns whether the dock changed.
    pub fn toggle(
        &mut self,
        dock: &mut DockState<T>,
        side: Side,
        protected: impl Fn(&T) -> bool,
    ) -> bool {
        if self.flyout.is_some_and(|(s, _)| s == side) {
            self.flyout = None;
        }
        match self.slot(side).take() {
            Some(column) => restore(dock, column),
            None => match collapse(dock, side, protected) {
                Some(column) => {
                    *self.slot(side) = Some(column);
                    true
                }
                None => false,
            },
        }
    }

    /// Unfolds the column holding `tab` (the Show-panel commands reveal a
    /// folded panel by unfolding its column). Returns whether it did.
    pub fn unfold_holding(&mut self, dock: &mut DockState<T>, tab: &T) -> bool {
        for side in [Side::Left, Side::Right] {
            if self
                .column(side)
                .is_some_and(|c| c.tabs().any(|t| t == tab))
            {
                self.flyout = None;
                let column = self.slot(side).take().expect("checked");
                return restore(dock, column);
            }
        }
        false
    }

    /// Unfolds every column into `dock` (a workspace switch stores the
    /// complete layout).
    pub fn restore_all(&mut self, dock: &mut DockState<T>) {
        self.flyout = None;
        for side in [Side::Left, Side::Right] {
            if let Some(column) = self.slot(side).take() {
                restore(dock, column);
            }
        }
    }

    /// `dock` with every folded column put back, leaving `self` as is:
    /// the layout a save writes, so a folded panel is never lost.
    pub fn unfolded(&self, dock: &DockState<T>) -> DockState<T> {
        let mut full = dock.clone();
        for column in [&self.left, &self.right].into_iter().flatten() {
            restore(&mut full, column.clone());
        }
        full
    }

    /// Drops rail items the user docked elsewhere (Window > Panels) and
    /// a rail left empty; closes a flyout whose item went away.
    pub fn prune(&mut self, dock: &DockState<T>) {
        for side in [Side::Left, Side::Right] {
            let slot = self.slot(side);
            if let Some(column) = slot {
                for leaf in &mut column.leaves {
                    leaf.tabs.retain(|t| dock.find_tab(t).is_none());
                    leaf.active = leaf.active.min(leaf.tabs.len().saturating_sub(1));
                }
                column.leaves.retain(|l| !l.tabs.is_empty());
                if column.leaves.is_empty() {
                    *slot = None;
                }
            }
        }
        if let Some((side, i)) = self.flyout {
            if self.column(side).is_none_or(|c| i >= c.tabs().count()) {
                self.flyout = None;
            }
        }
    }

    /// The tab behind the open flyout.
    pub fn flyout_tab(&self) -> Option<(Side, &T)> {
        let (side, i) = self.flyout?;
        self.column(side)?.tabs().nth(i).map(|t| (side, t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum P {
        View3d,
        View2d,
        Sets,
        Assets,
        Bakes,
        Layers,
        History,
        Props,
    }

    fn protected(p: &P) -> bool {
        matches!(p, P::View3d | P::View2d)
    }

    /// The Paint preset's shape (`shell_menu::default_dock`):
    /// [Sets/Assets/Bakes | 3D | 2D | Layers/History over Props].
    fn paint() -> DockState<P> {
        let mut dock = DockState::new(vec![P::View3d]);
        let tree = dock.main_surface_mut();
        let [rest, _] =
            tree.split_left(NodeIndex::root(), 0.19, vec![P::Sets, P::Assets, P::Bakes]);
        let [centre, right] = tree.split_right(rest, 0.73, vec![P::Layers, P::History]);
        tree.split_below(right, 0.45, vec![P::Props]);
        tree.split_right(centre, 0.62, vec![P::View2d]);
        dock
    }

    fn all(dock: &DockState<P>) -> Vec<P> {
        dock.iter_all_tabs().map(|(_, t)| t.clone()).collect()
    }

    fn leaf_of(dock: &DockState<P>, p: &P) -> (NodeIndex, usize, Vec<P>) {
        let path = dock.find_tab(p).expect("docked");
        let leaf = dock.main_surface()[path.node].get_leaf().unwrap();
        (path.node, leaf.active.0, leaf.tabs.clone())
    }

    #[test]
    fn finds_both_side_columns_of_the_paint_preset() {
        let dock = paint();
        let (l, lw) = side_column(&dock, Side::Left, protected).unwrap();
        assert_eq!(dock.main_surface()[l].get_leaf().unwrap().tabs[0], P::Sets);
        assert!((lw - 0.19).abs() < 1e-6, "{lw}");
        let (_, rw) = side_column(&dock, Side::Right, protected).unwrap();
        assert!((rw - 0.81 * 0.27).abs() < 1e-5, "{rw}");
    }

    #[test]
    fn collapse_lifts_the_column_and_keeps_the_views() {
        let mut dock = paint();
        let col = collapse(&mut dock, Side::Right, protected).unwrap();
        assert_eq!(col.leaves.len(), 2);
        assert_eq!(col.leaves[0].tabs, vec![P::Layers, P::History]);
        assert!((col.leaves[0].height - 0.45).abs() < 1e-6);
        assert_eq!(col.tabs().count(), 3);
        let left = all(&dock);
        assert!(left.contains(&P::View3d) && left.contains(&P::View2d));
        assert!(!left.contains(&P::Layers) && !left.contains(&P::Props));
        // The left column is still there, and the right is now absent.
        assert!(side_column(&dock, Side::Left, protected).is_some());
        assert!(side_column(&dock, Side::Right, protected).is_none());
    }

    #[test]
    fn restore_puts_every_tab_back_with_its_leaf_active_tab_and_side() {
        for side in [Side::Left, Side::Right] {
            let mut dock = paint();
            // A non-first active tab must survive the round trip.
            let p = if side == Side::Left {
                P::Assets
            } else {
                P::History
            };
            let at = dock.find_tab(&p).unwrap();
            dock.set_active_tab(at).unwrap();
            let before = all(&dock).len();
            let col = collapse(&mut dock, side, protected).unwrap();
            let width = col.width;
            assert!(restore(&mut dock, col));
            assert_eq!(
                all(&dock).len(),
                before,
                "{side:?}: nothing lost or doubled"
            );
            let (_, active, tabs) = leaf_of(&dock, &p);
            assert_eq!(tabs[active], p, "{side:?}: active tab kept");
            let (_, w) = side_column(&dock, side, protected).expect("column back on its side");
            let want = width.clamp(*WIDTH_SHARE.start(), *WIDTH_SHARE.end());
            assert!((w - want).abs() < 1e-5, "{side:?}: width {w} vs {want}");
        }
    }

    #[test]
    fn restore_keeps_the_leaf_heights() {
        let mut dock = paint();
        let col = collapse(&mut dock, Side::Right, protected).unwrap();
        restore(&mut dock, col);
        let (node, _) = side_column(&dock, Side::Right, protected).unwrap();
        let Node::Vertical(split) = &dock.main_surface()[node] else {
            panic!("the right column is a vertical stack again");
        };
        assert!((split.fraction - 0.45).abs() < 1e-5, "{}", split.fraction);
    }

    #[test]
    fn a_tab_docked_elsewhere_while_folded_is_not_restored_twice() {
        let mut dock = paint();
        let col = collapse(&mut dock, Side::Left, protected).unwrap();
        dock.main_surface_mut().push_to_first_leaf(P::Bakes);
        restore(&mut dock, col);
        let n = all(&dock).iter().filter(|p| **p == P::Bakes).count();
        assert_eq!(n, 1);
    }

    #[test]
    fn no_column_no_change() {
        let mut dock = DockState::new(vec![P::View3d, P::View2d]);
        assert!(collapse(&mut dock, Side::Left, protected).is_none());
        assert_eq!(all(&dock), vec![P::View3d, P::View2d]);
    }

    #[test]
    fn a_corrupt_width_restores_clamped() {
        let mut dock = paint();
        let mut col = collapse(&mut dock, Side::Left, protected).unwrap();
        col.width = 0.0;
        restore(&mut dock, col);
        let (_, w) = side_column(&dock, Side::Left, protected).unwrap();
        assert!((w - WIDTH_SHARE.start()).abs() < 1e-6, "{w}");
    }

    #[test]
    fn rails_toggle_folds_then_unfolds_and_closes_its_flyout() {
        let mut dock = paint();
        let before = all(&dock).len();
        let mut rails = Rails::default();
        assert!(rails.toggle(&mut dock, Side::Right, protected));
        assert_eq!(rails.folded_sides(), vec![Side::Right]);
        rails.flyout = Some((Side::Right, 1));
        assert_eq!(rails.flyout_tab(), Some((Side::Right, &P::History)));
        assert!(rails.toggle(&mut dock, Side::Right, protected));
        assert!(rails.flyout.is_none());
        assert!(rails.folded_sides().is_empty());
        assert_eq!(all(&dock).len(), before);
    }

    #[test]
    fn rails_unfolded_saves_every_panel_without_changing_the_fold() {
        let mut dock = paint();
        let before = all(&dock).len();
        let mut rails = Rails::default();
        rails.toggle(&mut dock, Side::Left, protected);
        let full = rails.unfolded(&dock);
        assert_eq!(all(&full).len(), before);
        assert!(rails.column(Side::Left).is_some());
        assert!(dock.find_tab(&P::Sets).is_none());
    }

    #[test]
    fn rails_prune_drops_redocked_items_and_empty_rails() {
        let mut dock = paint();
        let mut rails = Rails::default();
        rails.toggle(&mut dock, Side::Left, protected);
        rails.flyout = Some((Side::Left, 2));
        dock.main_surface_mut().push_to_first_leaf(P::Bakes);
        rails.prune(&dock);
        assert_eq!(rails.column(Side::Left).unwrap().tabs().count(), 2);
        assert!(rails.flyout.is_none(), "the flyout's item left the rail");
        dock.main_surface_mut().push_to_first_leaf(P::Sets);
        dock.main_surface_mut().push_to_first_leaf(P::Assets);
        rails.prune(&dock);
        assert!(rails.column(Side::Left).is_none());
    }

    #[test]
    fn rails_unfold_holding_reveals_a_folded_panel() {
        let mut dock = paint();
        let mut rails = Rails::default();
        rails.toggle(&mut dock, Side::Right, protected);
        assert!(rails.unfold_holding(&mut dock, &P::Props));
        assert!(dock.find_tab(&P::Props).is_some());
        assert!(!rails.unfold_holding(&mut dock, &P::Props));
    }

    #[test]
    fn serde_round_trip() {
        let col = RailColumn {
            side: Side::Right,
            width: 0.2,
            leaves: vec![RailLeaf {
                tabs: vec![1u8, 2],
                active: 1,
                height: 1.0,
            }],
        };
        let json = serde_json::to_string(&col).unwrap();
        let back: RailColumn<u8> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, col);
    }
}
