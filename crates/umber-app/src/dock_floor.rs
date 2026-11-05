//! Dock layout floors (UI/UX pass U1-1, requirement R-LAYOUT-1 in
//! `docs/specs/uiux-2026-11-05-audit-requirements-plan.md`).
//!
//! egui_dock 0.21.1 lays a split out from its `fraction` alone: each child
//! gets `fraction x parent` whatever the window size, and nothing stops a
//! leaf from shrinking to a sliver. At 1423 x 729 the default Paint layout
//! gave Properties 162 px and the 2D UV view 97 px, so every side panel
//! truncated its text.
//!
//! [`apply_floors`] runs once per frame BEFORE `DockArea::show`. For every
//! split on the main surface it computes each child subtree's minimum size
//! along the split axis (the sum over same-axis splits, the max over
//! cross-axis splits, a leaf's own floor at the bottom) and clamps the
//! split's `fraction` so both children keep their floor. When the parent is
//! too small for both floors, the space is shared in proportion to the
//! floors, so nothing collapses to zero.
//!
//! The pass only ever moves a fraction that violates a floor; a layout
//! that already fits is left untouched, bit for bit, so a user's divider
//! drag inside the allowed range is never fought. It is idempotent.
//!
//! Floating windows are not touched: egui keeps their size.

use egui::{Rect, Vec2};
use egui_dock::{DockState, Node, NodeIndex, Tree};

/// Gap egui_dock leaves for a separator between two children. Counted
/// once per same-axis split so the floors are met after the separator
/// takes its share.
pub const SEPARATOR_ALLOWANCE: f32 = 2.0;

/// Clamp every split fraction on `dock`'s main surface so each leaf keeps
/// `min_size(tab)` (the largest floor among the leaf's tabs) inside
/// `area`, the rect the dock area will occupy. Returns whether any
/// fraction changed.
pub fn apply_floors<Tab>(
    dock: &mut DockState<Tab>,
    area: Rect,
    min_size: impl Fn(&Tab) -> Vec2,
) -> bool {
    if !area.is_finite() || area.width() <= 0.0 || area.height() <= 0.0 {
        return false;
    }
    let tree = dock.main_surface_mut();
    if tree.is_empty() {
        return false;
    }
    let mut changed = false;
    clamp_node(
        tree,
        NodeIndex::root(),
        area.size(),
        &min_size,
        &mut changed,
    );
    changed
}

/// The minimum size of the subtree at `index`.
pub fn subtree_floor<Tab>(
    tree: &Tree<Tab>,
    index: NodeIndex,
    min_size: &impl Fn(&Tab) -> Vec2,
) -> Vec2 {
    if index.0 >= tree.len() {
        return Vec2::ZERO;
    }
    match &tree[index] {
        Node::Empty => Vec2::ZERO,
        Node::Leaf(leaf) => leaf
            .tabs()
            .iter()
            .map(min_size)
            .fold(Vec2::ZERO, |a, b| a.max(b)),
        Node::Horizontal(_) => {
            let l = subtree_floor(tree, index.left(), min_size);
            let r = subtree_floor(tree, index.right(), min_size);
            Vec2::new(l.x + r.x + SEPARATOR_ALLOWANCE, l.y.max(r.y))
        }
        Node::Vertical(_) => {
            let l = subtree_floor(tree, index.left(), min_size);
            let r = subtree_floor(tree, index.right(), min_size);
            Vec2::new(l.x.max(r.x), l.y + r.y + SEPARATOR_ALLOWANCE)
        }
    }
}

/// The fraction for a split of `total` points whose children need
/// `first_min` and `second_min`: `current` when it already satisfies both,
/// else the nearest fraction that does, else the floors' proportion.
pub fn floored_fraction(current: f32, total: f32, first_min: f32, second_min: f32) -> f32 {
    if total <= 0.0 || !total.is_finite() {
        return current;
    }
    let need = first_min + second_min + SEPARATOR_ALLOWANCE;
    if need > total {
        let sum = first_min + second_min;
        return if sum > 0.0 {
            (first_min / sum).clamp(0.0, 1.0)
        } else {
            current
        };
    }
    let lo = (first_min + SEPARATOR_ALLOWANCE * 0.5) / total;
    let hi = 1.0 - (second_min + SEPARATOR_ALLOWANCE * 0.5) / total;
    current.clamp(lo, hi.max(lo))
}

fn clamp_node<Tab>(
    tree: &mut Tree<Tab>,
    index: NodeIndex,
    size: Vec2,
    min_size: &impl Fn(&Tab) -> Vec2,
    changed: &mut bool,
) {
    if index.0 >= tree.len() {
        return;
    }
    let horizontal = match &tree[index] {
        Node::Horizontal(_) => true,
        Node::Vertical(_) => false,
        Node::Empty | Node::Leaf(_) => return,
    };
    let first = subtree_floor(tree, index.left(), min_size);
    let second = subtree_floor(tree, index.right(), min_size);
    let (total, a, b) = if horizontal {
        (size.x, first.x, second.x)
    } else {
        (size.y, first.y, second.y)
    };
    let fraction = match &mut tree[index] {
        Node::Horizontal(split) | Node::Vertical(split) => {
            let next = floored_fraction(split.fraction, total, a, b);
            if next != split.fraction {
                split.fraction = next;
                *changed = true;
            }
            next
        }
        Node::Empty | Node::Leaf(_) => return,
    };
    let (first_size, second_size) = if horizontal {
        (
            Vec2::new(size.x * fraction, size.y),
            Vec2::new(size.x * (1.0 - fraction), size.y),
        )
    } else {
        (
            Vec2::new(size.x, size.y * fraction),
            Vec2::new(size.x, size.y * (1.0 - fraction)),
        )
    };
    clamp_node(tree, index.left(), first_size, min_size, changed);
    clamp_node(tree, index.right(), second_size, min_size, changed);
}

/// Each leaf's laid-out size under `area`, by the fractions alone (the
/// arithmetic egui_dock's layout pass does, minus separators): the
/// preset tests' census, `(first tab, size)` per leaf.
#[cfg(test)]
pub fn leaf_sizes<Tab: Clone>(dock: &DockState<Tab>, area: Rect) -> Vec<(Tab, Vec2)> {
    let mut out = Vec::new();
    let tree = dock.main_surface();
    if !tree.is_empty() {
        collect(tree, NodeIndex::root(), area.size(), &mut out);
    }
    out
}

#[cfg(test)]
fn collect<Tab: Clone>(tree: &Tree<Tab>, index: NodeIndex, size: Vec2, out: &mut Vec<(Tab, Vec2)>) {
    if index.0 >= tree.len() {
        return;
    }
    match &tree[index] {
        Node::Empty => {}
        Node::Leaf(leaf) => {
            if let Some(tab) = leaf.tabs().first() {
                out.push((tab.clone(), size));
            }
        }
        Node::Horizontal(split) => {
            let f = split.fraction;
            collect(tree, index.left(), Vec2::new(size.x * f, size.y), out);
            collect(
                tree,
                index.right(),
                Vec2::new(size.x * (1.0 - f), size.y),
                out,
            );
        }
        Node::Vertical(split) => {
            let f = split.fraction;
            collect(tree, index.left(), Vec2::new(size.x, size.y * f), out);
            collect(
                tree,
                index.right(),
                Vec2::new(size.x, size.y * (1.0 - f)),
                out,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2};

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    enum T {
        Big,
        Side,
        Other,
    }

    fn floor(t: &T) -> Vec2 {
        match t {
            T::Big => vec2(360.0, 240.0),
            T::Side | T::Other => vec2(260.0, 140.0),
        }
    }

    fn area(w: f32, h: f32) -> Rect {
        Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h))
    }

    /// `Big` centre, `Side` split off to the right at a sliver fraction.
    fn sliver() -> DockState<T> {
        let mut dock = DockState::new(vec![T::Big]);
        dock.main_surface_mut()
            .split_right(NodeIndex::root(), 0.95, vec![T::Side]);
        dock
    }

    fn size_of(sizes: &[(T, Vec2)], t: T) -> Vec2 {
        sizes.iter().find(|(x, _)| *x == t).unwrap().1
    }

    #[test]
    fn a_sliver_leaf_is_widened_to_its_floor() {
        let mut dock = sliver();
        let a = area(1400.0, 900.0);
        assert!(size_of(&leaf_sizes(&dock, a), T::Side).x < 100.0);
        assert!(apply_floors(&mut dock, a, floor));
        let side = size_of(&leaf_sizes(&dock, a), T::Side);
        assert!(side.x >= 260.0, "side {side:?}");
        assert!(size_of(&leaf_sizes(&dock, a), T::Big).x >= 360.0);
    }

    #[test]
    fn a_layout_that_fits_is_untouched_bit_for_bit() {
        let mut dock = DockState::new(vec![T::Big]);
        dock.main_surface_mut()
            .split_right(NodeIndex::root(), 0.7, vec![T::Side]);
        let before = format!("{:?}", leaf_sizes(&dock, area(1400.0, 900.0)));
        assert!(!apply_floors(&mut dock, area(1400.0, 900.0), floor));
        assert_eq!(
            before,
            format!("{:?}", leaf_sizes(&dock, area(1400.0, 900.0)))
        );
    }

    #[test]
    fn floors_are_idempotent() {
        let mut dock = sliver();
        let a = area(1400.0, 900.0);
        apply_floors(&mut dock, a, floor);
        assert!(!apply_floors(&mut dock, a, floor), "second pass is a no-op");
    }

    #[test]
    fn nested_same_axis_floors_add_up() {
        // [Side | [Big | Other]] at sliver fractions both ways.
        let mut dock = DockState::new(vec![T::Big]);
        let tree = dock.main_surface_mut();
        tree.split_right(NodeIndex::root(), 0.98, vec![T::Other]);
        tree.split_left(NodeIndex::root(), 0.02, vec![T::Side]);
        let a = area(1400.0, 900.0);
        apply_floors(&mut dock, a, floor);
        let s = leaf_sizes(&dock, a);
        assert!(size_of(&s, T::Side).x >= 260.0, "{s:?}");
        assert!(size_of(&s, T::Other).x >= 260.0, "{s:?}");
        assert!(size_of(&s, T::Big).x >= 360.0, "{s:?}");
    }

    #[test]
    fn vertical_splits_keep_height_floors() {
        let mut dock = DockState::new(vec![T::Side]);
        dock.main_surface_mut()
            .split_below(NodeIndex::root(), 0.97, vec![T::Other]);
        let a = area(300.0, 700.0);
        apply_floors(&mut dock, a, floor);
        assert!(size_of(&leaf_sizes(&dock, a), T::Other).y >= 140.0);
    }

    #[test]
    fn too_small_shares_in_proportion_to_the_floors() {
        // 400 px cannot hold 360 + 260: the split follows 360:260.
        let mut dock = sliver();
        let a = area(400.0, 900.0);
        apply_floors(&mut dock, a, floor);
        let s = leaf_sizes(&dock, a);
        let big = size_of(&s, T::Big).x;
        let side = size_of(&s, T::Side).x;
        assert!((big / side - 360.0 / 260.0).abs() < 0.01, "{big} {side}");
    }

    #[test]
    fn the_floored_fraction_edges() {
        assert_eq!(floored_fraction(0.5, 1000.0, 100.0, 100.0), 0.5);
        assert!(floored_fraction(0.01, 1000.0, 100.0, 100.0) >= 0.1);
        assert!(floored_fraction(0.99, 1000.0, 100.0, 100.0) <= 0.9);
        assert_eq!(floored_fraction(0.3, 0.0, 100.0, 100.0), 0.3);
        assert_eq!(floored_fraction(0.3, f32::NAN, 100.0, 100.0), 0.3);
        assert_eq!(floored_fraction(0.3, 100.0, 0.0, 0.0), 0.3);
    }

    #[test]
    fn a_degenerate_area_changes_nothing() {
        let mut dock = sliver();
        assert!(!apply_floors(&mut dock, area(0.0, 0.0), floor));
        let nan = Rect::from_min_size(pos2(0.0, 0.0), vec2(f32::NAN, 10.0));
        assert!(!apply_floors(&mut dock, nan, floor));
    }
}
