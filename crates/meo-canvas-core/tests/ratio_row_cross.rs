//! The cross size a ratio box takes in a row container: `derived_cross`'s row
//! arm, `height = width / ratio`, which nothing else reaches. Both rows sit off
//! ratio one -- 0.5 gives 300 x 600 and 2 gives 300 x 150 -- since at one,
//! unrelated rules coincide with the answer. Not a browser comparison.

use meo_canvas_core::{Available, Measure, MeasuredLeaf, layout::solve};
use meo_canvas_scene::{
    Scene, Size,
    node::{Node, NodeId, NodeKind},
    style::{
        Dimension,
        layout::{Display, FlexDirection},
    },
};

/// Every scene here is boxes, so nothing needs measuring.
struct NoLeaves;
impl Measure for NoLeaves {
    fn measure(
        &mut self,
        _: NodeId,
        _: (Option<f32>, Option<f32>),
        _: (Available, Available),
    ) -> MeasuredLeaf {
        MeasuredLeaf::EMPTY
    }
}

/// The container both scenes use: a `424x248` flex row.
const WIDTH: f32 = 424.0;
/// Its content height, which is also what a stretched item takes -- the number
/// the neutered arm leaves behind.
const HEIGHT: f32 = 248.0;
/// A minimum wide enough to bind against an empty item's fit-content width,
/// which is what declines the pin and reaches the derivation.
const MINIMUM: f32 = 300.0;

/// One empty ratio box in a `424x248` row container, solved. Hand-assembled, so
/// the item is `Display::Block`, which is what a `<div>` is and what these
/// numbers were taken from.
fn item(ratio: f32, grow: f32, minimum: Option<f32>) -> (f32, f32) {
    let mut scene = Scene::new(Size::new(WIDTH, HEIGHT));
    if let Some(page) = scene.get_mut(NodeId::ROOT) {
        page.layout.display = Display::Flex;
        page.layout.flex_direction = FlexDirection::Row;
        page.layout.size =
            (Dimension::Points(WIDTH), Dimension::Points(HEIGHT));
    }
    let item = scene
        .push(NodeId::ROOT, Node::new(NodeKind::Box))
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(item) {
        node.layout.aspect_ratio = Some(ratio);
        node.layout.flex_grow = grow;
        node.layout.size = (Dimension::Auto, Dimension::Auto);
        if let Some(minimum) = minimum {
            node.layout.min_size.0 = Dimension::Points(minimum);
        }
    }

    let solved = solve(&scene, NodeId::ROOT, &mut NoLeaves)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let rect = solved
        .get(item)
        .unwrap_or_else(|| unreachable!("no rectangle for the ratio box"));
    (rect.size.width, rect.size.height)
}

/// A ratio below one derives a height taller than the container it sits in.
#[test]
fn a_bound_minimum_in_a_row_derives_a_height_above_the_container() {
    let (width, height) = item(0.5, 0.0, Some(MINIMUM));
    assert!(
        (width - 300.0).abs() < 0.01 && (height - 600.0).abs() < 0.01,
        "a `min-width: 300` item at ratio 0.5 in a 424x248 row solves to \
         {width} x {height}, where it solves to 300 x 600. 600 is 300 / 0.5, \
         the height the row arm of `derived_cross` writes. {HEIGHT} here \
         means the item took the container's height by stretch and the \
         derivation never ran"
    );
}

/// A ratio above one derives a height shorter than the container, and a
/// narrower main size with it.
#[test]
fn a_bound_minimum_in_a_row_derives_a_height_below_the_container() {
    let (width, height) = item(2.0, 0.0, Some(MINIMUM));
    assert!(
        (width - 300.0).abs() < 0.01 && (height - 150.0).abs() < 0.01,
        "a `min-width: 300` item at ratio 2 in a 424x248 row solves to \
         {width} x {height}, where it solves to 300 x 150. 150 is 300 / 2. \
         {WIDTH} x {HEIGHT} means the derivation never ran and the item was \
         left grown and stretched"
    );
}

/// The derived height is the width over the ratio, in both directions: the
/// relationship survives a scene change that moves both magnitudes, and the
/// magnitudes catch a ratio that is read but never divided by.
#[test]
fn the_derived_height_is_the_width_over_the_ratio() {
    for ratio in [0.5_f32, 2.0] {
        let (width, height) = item(ratio, 0.0, Some(MINIMUM));
        let want = width / ratio;
        assert!(
            (height - want).abs() < 0.01,
            "at ratio {ratio} the item solves to {width} x {height}, where \
             the row arm of `derived_cross` makes the height {want}"
        );
    }
}

/// A row the solve already gets right keeps its size: `derived_cross` returns
/// `None` for a grown item with no minimum. If this moved, the compensation
/// would be rewriting sizes that were already right.
#[test]
fn a_row_the_solve_already_gets_right_is_left_alone() {
    let (width, height) = item(2.0, 1.0, None);
    assert!(
        (width - 496.0).abs() < 0.01 && (height - HEIGHT).abs() < 0.01,
        "a grown item at ratio 2 with no minimum solves to {width} x \
         {height}, where it solves to 496 x {HEIGHT}. 496 is {HEIGHT} x 2, so \
         the cross size is already the ratio's answer and nothing is written"
    );
}
