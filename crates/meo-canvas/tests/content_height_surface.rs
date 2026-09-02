//! The Rust surface's own half of the content-height contract.
//!
//! `crates/meo-canvas-core/tests/content_height.rs` proves the renderer does
//! it; this proves the surface asks for it, and that the default is the one a
//! caller gets by writing the least. They are different failures: a surface
//! that never sets the flag renders a stated height correctly for ever and
//! never content-sizes anything.

use meo_canvas::{Box as BoxNode, Root, Styled as _, px};

/// The width every case states, since a width is never derived.
const WIDTH: f32 = 200.0;

/// A scene from a root, or the error that stopped it.
fn scene_of(root: Root) -> meo_canvas::scene::Scene {
    root.into_scene()
        .unwrap_or_else(|error| unreachable!("the root did not build: {error}"))
}

#[test]
fn a_root_takes_its_height_from_its_content_by_default() {
    let scene = scene_of(Root::new(WIDTH));

    assert!(
        scene.content_height,
        "`Root::new` alone must ask for a content height, or the shortest \
         thing a caller can write is the one behaviour they cannot reach"
    );
    assert!(
        (scene.size.height - 0.0).abs() < f32::EPSILON,
        "with no floor stated the floor is nothing"
    );
}

#[test]
fn a_stated_height_is_a_stated_height() {
    // The control. Without it a surface that set the flag unconditionally
    // would pass the test above and silently ignore every height ever given.
    let scene = scene_of(Root::new(WIDTH).height(120.0));

    assert!(!scene.content_height, "a stated height is not derived");
    assert!((scene.size.height - 120.0).abs() < f32::EPSILON);
}

#[test]
fn a_floor_travels_as_the_height_and_only_while_the_content_decides() {
    let floored = scene_of(Root::new(WIDTH).min_height(90.0));
    assert!(floored.content_height);
    assert!((floored.size.height - 90.0).abs() < f32::EPSILON);

    // A floor after a height has nothing to raise, and must not turn a stated
    // height back into a derived one -- which is the ordering mistake that
    // would make `.height(120).min_height(90)` render a 90-tall page.
    let stated = scene_of(Root::new(WIDTH).height(120.0).min_height(90.0));
    assert!(
        !stated.content_height,
        "a floor must not undo a stated height"
    );
    assert!((stated.size.height - 120.0).abs() < f32::EPSILON);
}

#[test]
fn the_content_still_reaches_the_scene() {
    // The flag is not the whole claim: a root that content-sizes and drops its
    // children would satisfy every assertion above.
    let scene = scene_of(
        Root::new(WIDTH).children(BoxNode::new().size(px(10.0), px(10.0))),
    );

    assert!(scene.content_height);
    assert!(scene.nodes.len() > 1, "the child reached the scene");
}
