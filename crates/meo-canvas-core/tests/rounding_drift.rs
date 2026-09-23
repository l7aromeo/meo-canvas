//! Our layout edges against Chrome's, down a stack of fractional boxes.
//! Chrome truncates each length to sixty-fourths of a pixel before adding;
//! `a_length_is_truncated_into_sixty_fourths_rather_than_rounded` in
//! `layout.rs` pins that grid, and this file pins the painted edges it yields.

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
    // Values, not a tolerance: at an exact edge of 51.5, a half-pixel bound
    // passes both the unsnapped 52 and the right 51. Chrome snaps 10.3 to
    // 10.296875, so five reach 51.484375, which rounds to 51.
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
    // Snap once, then add: our painted edge is Chrome's fraction rounded, and
    // a second rounding inside the accumulation would show as a whole pixel.
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
    // A 10.3 box is drawn 10 or 11 tall depending on where it starts, since
    // both edges round from absolute positions -- so equal heights would be
    // the wrong thing to assert.
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
