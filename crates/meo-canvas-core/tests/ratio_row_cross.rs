//! The cross size a ratio box takes in a **row** container, which is the half
//! of the ratio compensation nothing else exercises.
//!
//! # What this pins
//!
//! `derived_cross` in `crates/meo-canvas-core/src/layout.rs` writes the size a
//! ratio'd item should have taken when the solve did not give it one. Its two
//! arms are the parent's direction: a column container derives a **width**
//! from a height, and a row container derives a **height** from a width. The
//! column arm is asserted from several directions -- the conformance table in
//! `crates/meo-canvas/tests/assets/chrome/flex-ratio-cross.tsv` is built on a
//! column container, and every row of it lands there. **The row arm was
//! reached by nothing.**
//!
//! Found by neutering it: returning `None` from the row arm before the write
//! left the whole of `meo-canvas-core` and `meo-canvas` green -- 65 summary
//! lines, no failures. A print inside the arm then showed the stronger fact,
//! that no test in either crate *enters* it at all.
//!
//! # Why a ratio of one hides this branch, and why both rows here avoid it
//!
//! The arm writes `height = width / ratio`. **At `ratio: 1` that is
//! `height = width`, which is also what several unrelated rules produce**, so
//! a row-container case at ratio one agrees with the compensated answer
//! whether or not the compensation ran. The first scene tried here was exactly
//! that -- a bound `min-width: 300` at ratio one -- and it gives `300 x 300`
//! with the arm live and `300 x 300` with it neutered. **The branch fires and
//! changes nothing**, which is indistinguishable from a branch that never
//! fired, and is why a test written around it would have passed in both
//! directions.
//!
//! So both asserted rows sit **off** ratio one, and on opposite sides of it,
//! which also pins the direction of the derivation rather than only its
//! existence:
//!
//! ```text
//! ratio 0.5   arm live 300 x 600   arm neutered 300 x 248
//! ratio 2     arm live 300 x 150   arm neutered 424 x 248
//! ```
//!
//! `248` is the container's own content height, so the neutered answer is the
//! stretch rather than the derivation -- a number that arrives from somewhere
//! else entirely and could not be mistaken for a rounding difference. `600`
//! overflows the container by more than twice its height and `150` falls well
//! inside it, so no single unrelated rule produces both.
//!
//! # What this file does not claim
//!
//! **It is not a browser comparison.** No row here is measured against Chrome;
//! the conformance tables do that, and adding a row to one of them is a
//! separate question from whether this branch is asserted at all. What is
//! pinned is the decision the branch makes, in the terms the branch makes it.
//!
//! **It carries no `[FOUNDATION]` marker**, because it rests on no property of
//! the dependency. The compensation for `l7aromeo/meo-canvas#129` names
//! `crates/meo-canvas-core/tests/taffy_flex_ratio.rs` as its probe, and that
//! is where the marked row belongs. This file asserts what the compensation
//! itself does, which is a different question and needs a different file.

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

/// One empty ratio box in a `424x248` row container, solved.
///
/// **Hand-assembled, so the item is `Display::Block`** -- the scene default,
/// and what a `<div>` is. A scene built through the authoring surface's
/// factories would be a flex container instead, which is a different scene and
/// not the one these numbers were taken from.
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

/// The derived height is the width over the ratio, in both directions.
///
/// **The relationship rather than the two magnitudes**, because the
/// magnitudes are what an unrelated rule can coincide with and the
/// relationship is what the branch decides. Neither assertion substitutes for
/// the other: this one would survive a scene change that moved both numbers
/// together, and the two above would survive a ratio that was read but never
/// divided by.
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

/// A row container taffy already resolves correctly keeps its size.
///
/// **The control, and it is the reason the branch is narrow rather than
/// load-bearing everywhere.** A grown item with no minimum already satisfies
/// `height == width / ratio` after the first solve, so `derived_cross` returns
/// `None` and writes nothing. If this row ever moved, the compensation would
/// have started rewriting sizes that were already right -- which is a
/// different defect from the one it exists for, and the two would be
/// indistinguishable from the rows above alone.
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
