//! Where our layout edges land, against Chrome's, down a stack.
//!
//! # What this used to be, and why it changed
//!
//! It asked whether whole-pixel rounding **accumulates**. It does not:
//! `round_layout` (`taffy-0.13.0/src/compute/mod.rs:219`) rounds the
//! **cumulative absolute coordinate** of each edge and takes the difference,
//!
//! ```text
//! size.height = round(cumulative_y + height) - round(cumulative_y)
//! ```
//!
//! so every edge is `round(its exact position)` and is within half a pixel of
//! it by definition, at depth one and at depth a thousand. **The per-box
//! heights wobble and the wobble is what keeps the edges true.** That answer
//! still holds and the last test here still asserts it.
//!
//! **What changed is that half a pixel was not good enough.** Chrome's layout
//! is fractional: it snaps a used length into sixty-fourths of a pixel and
//! accumulates the snapped values exactly. We accumulated the exact values, so
//! wherever a running coordinate landed on a half we rounded up and Chrome --
//! having already been nudged below the half by the snap -- rounded down. One
//! whole CSS pixel, at that edge and nowhere else.
//!
//! `layout::snapped` now puts every length into the same grid before anything
//! accumulates, **so the tie never forms**, and this file pins the result
//! against the browser rather than against a tolerance.
//!
//! # Chrome floors, and one row proves it is truncation
//!
//! Measured by MC Main through Playwright: `getBoundingClientRect().height` on
//! a single box of the stated height and on a column of five, `box-sizing:
//! border-box`, margins and padding zeroed.
//!
//! | height | floor | round | Chrome | x5 | |
//! |---|---|---|---|---|---|
//! | `10.008` | `10` | `10.015625` | `10` | `50` | floor |
//! | `10.0234375` | `10.015625` | `10.03125` | `10.015625` | `50.078125` | floor |
//! | `7.999` | `7.984375` | `8` | `7.984375` | `39.921875` | floor |
//! | `10.02` | `10.015625` | `10.015625` | `10.015625` | `50.078125` | agree |
//! | `3.3` | `3.296875` | `3.296875` | `3.296875` | `16.484375` | agree |
//! | `10.3` | `10.296875` | `10.296875` | `10.296875` | `51.484375` | agree |
//!
//! **`10.0234375` is the row that settles it**, and it settles more than the
//! question asked. It is exactly `641.5` sixty-fourths -- a tie -- and Chrome
//! takes `641`. So it is not floor against round-half-up, and not banker's
//! either: **it is truncation, and no rounding mode reproduces it.**
//!
//! Those rows are asserted in `layout.rs`'s own test module rather than here,
//! **because the snap is the only place the grid is observable and it is not
//! public.** taffy rounds the solved tree to whole pixels, so nothing this
//! file can reach carries a sixty-fourth: a box of `10.0234375` is `10` here
//! and `10.015625` in `getBoundingClientRect`, and **those are not the same
//! measurement** -- a painted edge against a layout rect. This file asserts
//! the painted edges, which is the comparison that holds.
//!
//! The `x5` column is not decoration: it shows the snapped value accumulating
//! five times with no second rounding, which is the property the fix rests on.

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

/// The bottom edge of a stack of `count` boxes each `step` tall.
fn stack_bottom(step: f32, count: usize) -> f32 {
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
                (Dimension::Points(50.0), Dimension::Points(step));
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
fn every_edge_of_a_fractional_stack_is_chromes() {
    // **Values, not a tolerance.** The tolerance this file used to assert --
    // half a pixel -- was satisfied exactly by the one edge that was wrong,
    // and satisfied identically by the right answer once it was fixed: with
    // an exact position of 51.5, both 52 and 51 are half a pixel away. **An
    // assertion on a magnitude cannot tell a defect from its fix when the two
    // sit the same distance from the truth.**
    //
    // Chrome, from the table above: 10.3 snaps to 10.296875, and five of them
    // reach 51.484375, which rounds to 51. Before the snap we summed 51.5
    // exactly and a tie rounds up, giving 52.
    let chrome = [10, 21, 31, 41, 51, 62, 72, 82];
    for (index, want) in chrome.into_iter().enumerate() {
        let count = index + 1;
        let ours = stack_bottom(STEP, count);
        assert!(
            (ours - f64::from(want) as f32).abs() < f32::EPSILON,
            "edge {count} is at {ours} where Chrome puts it at {want}"
        );
    }
}

#[test]
fn five_snapped_boxes_accumulate_without_a_second_rounding() {
    // The `x5` column, and the property the fix rests on: snap once, then
    // add. **Our edge is Chrome's fraction rounded**, because taffy rounds the
    // tree and Chrome's paint rounds each edge too -- the comparison is
    // painted edge against painted edge, which is the one place the two are
    // the same measurement.
    //
    // A second rounding anywhere inside the accumulation would show here as a
    // whole pixel.
    for (height, chrome_five) in [
        (10.008_f32, 50.0_f32),
        (10.023_437_5, 50.078_125),
        (7.999, 39.921_875),
        (3.3, 16.484_375),
        (STEP, 51.484_375),
    ] {
        let ours = stack_bottom(height, 5);
        let rounded = chrome_five.round();
        assert!(
            (ours - rounded).abs() < f32::EPSILON,
            "five boxes of {height} reach {ours} where Chrome's \
             {chrome_five} rounds to {rounded}"
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
