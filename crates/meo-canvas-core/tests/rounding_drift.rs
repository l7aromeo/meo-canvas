//! Whether whole-pixel layout rounding accumulates down a stack.
//!
//! # The question
//!
//! taffy rounds every box to whole pixels; **Chrome rounds none.** It works in
//! sixty-fourths of a pixel, floors each line box into that grid and sums, and
//! never rounds the total -- `16px x 1.4` over three lines is `67.171875`
//! against our `67`. So any box with a fractional computed size is a whole
//! pixel here and a sixty-fourth there.
//!
//! The worry that follows is accumulation: **a column of ten fractional boxes
//! drifting a little further from Chrome at every step**, until the bottom of
//! the stack is somewhere visibly wrong.
//!
//! # It does not accumulate, and the reason is the formula
//!
//! `round_layout` (`taffy-0.13.0/src/compute/mod.rs:219`) does not round a
//! box's *size*. It rounds the **cumulative absolute coordinate** of each edge
//! and takes the difference:
//!
//! ```text
//! size.height = round(cumulative_y + height) - round(cumulative_y)
//! ```
//!
//! So every edge in the tree lands on `round(its exact position)`, **and an
//! edge that is the rounding of an exact value is within half a pixel of it by
//! definition** -- at depth one and at depth a thousand. The per-box heights
//! wobble between `floor` and `ceil` of the exact height, and the wobble is
//! what *keeps* the edges true rather than what accumulates.
//!
//! **The individual boxes are what look wrong; the stack is what stays
//! right.** That is the opposite of the intuition the question started from,
//! and it is why this file measures the bottom edge rather than the heights.

use meo_canvas_core::{
    layout,
    measure::SceneMeasurer,
    resolve::{Fonts, Resolved},
};
use meo_canvas_scene::{
    Scene, Size,
    node::{Node, NodeId, NodeKind},
    style::{Dimension, layout::FlexDirection},
};

/// A height with no exact binary form and a fraction clear of `.5`, so each
/// box rounds down and a naive sum of rounded heights would fall behind fast.
const STEP: f32 = 10.3;

/// The bottom edge of a stack of `count` boxes each `STEP` tall.
fn stack_bottom(count: usize) -> f32 {
    let mut scene = Scene::new(Size::new(50.0, 4000.0));
    if let Some(page) = scene.get_mut(NodeId::ROOT) {
        page.layout.flex_direction = FlexDirection::Column;
    }
    let mut last = NodeId::ROOT;
    for _ in 0..count {
        last = scene
            .push(NodeId::ROOT, Node::new(NodeKind::Box))
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(last) {
            node.layout.size =
                (Dimension::Points(50.0), Dimension::Points(STEP));
        }
    }

    let fonts = Fonts::new();
    let resolved = Resolved::new(&scene, &fonts)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let solved = layout::solve(&scene, NodeId::ROOT, &mut measurer)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let rect = solved
        .get(last)
        .unwrap_or_else(|| unreachable!("no rectangle for the last box"));
    rect.origin.y + rect.size.height
}

#[test]
fn a_stack_of_fractional_boxes_does_not_drift() {
    // The claim, at four depths: the bottom of the stack is the rounding of
    // where it exactly belongs, so the error is bounded by half a pixel and
    // does not grow with the count.
    for count in [1_usize, 2, 10, 100] {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a hundred boxes is exactly representable"
        )]
        let exact = STEP * count as f32;
        let ours = stack_bottom(count);
        assert!(
            (ours - exact).abs() <= 0.5,
            "at depth {count} the stack ends at {ours} where exactly {exact} \
             is right -- the rounding is accumulating"
        );
    }
}

#[test]
fn the_boxes_wobble_even_though_the_stack_does_not() {
    // The other half, and the reason the heights look wrong when the edges
    // are right: a 10.3 box is drawn 10 or 11 tall depending on where it
    // starts, because both of its edges are rounded from absolute positions.
    // **A test asserting every box is the same height would fail here and be
    // wrong to.**
    let mut heights = std::collections::BTreeSet::new();
    let mut scene = Scene::new(Size::new(50.0, 4000.0));
    if let Some(page) = scene.get_mut(NodeId::ROOT) {
        page.layout.flex_direction = FlexDirection::Column;
    }
    let mut boxes = Vec::new();
    for _ in 0..10 {
        let id = scene
            .push(NodeId::ROOT, Node::new(NodeKind::Box))
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(id) {
            node.layout.size =
                (Dimension::Points(50.0), Dimension::Points(STEP));
        }
        boxes.push(id);
    }
    let fonts = Fonts::new();
    let resolved = Resolved::new(&scene, &fonts)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let solved = layout::solve(&scene, NodeId::ROOT, &mut measurer)
        .unwrap_or_else(|error| unreachable!("{error}"));
    for id in boxes {
        let rect = solved
            .get(id)
            .unwrap_or_else(|| unreachable!("no rectangle"));
        heights.insert(rect.size.height.to_bits());
    }
    assert!(
        heights.len() > 1,
        "every box came out the same height, so the edges cannot all be \
         rounded from absolute positions"
    );
}
