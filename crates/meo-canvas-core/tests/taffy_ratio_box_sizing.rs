//! Pins two inherited taffy defects, taffy 0.15.1, for a picture with
//! padding, whose own ratio CSS applies to its content box. `src/compute/
//! leaf.rs` hands the measure function a known size as the border box, so
//! 60x40 art with 10px of padding stretched in a flex row is measured 80 wide
//! and comes out 80x73, where Chrome gives 80x60. And an `aspect_ratio` applies
//! to the box `box_sizing` names, so under `border-box` the same art declared
//! 100 wide is 100x67, where Chrome gives 100x73.33. Asserts taffy's numbers,
//! so a fix fails here and the `[WORKAROUND]`s in `measure_leaf` and
//! `size_pictures_by_content` in `layout.rs` can go. `l7aromeo/meo-canvas#217`.

use taffy::prelude::{
    AvailableSpace, BoxSizing, Dimension, Display, LengthPercentage, Rect,
    Size, Style, TaffyTree, length,
};

/// The art, whose ratio is 1.5.
const ART: (f32, f32) = (60.0, 40.0);

/// The padding on every side.
const PADDING: f32 = 10.0;

/// The container's width, wider than any box here.
const CONTAINER: f32 = 200.0;

/// The art's size given what is known of it, as `fit_intrinsic` in
/// `meo-canvas-core` answers, reading a known size as the content box.
fn picture(known: Size<Option<f32>>) -> Size<f32> {
    let ratio = ART.0 / ART.1;
    match (known.width, known.height) {
        (Some(width), Some(height)) => Size { width, height },
        (Some(width), None) => Size {
            width,
            height: width / ratio,
        },
        (None, Some(height)) => Size {
            width: height * ratio,
            height,
        },
        (None, None) => Size {
            width: ART.0,
            height: ART.1,
        },
    }
}

/// The padded picture's laid-out size in a container of `display` 200 wide,
/// at `width`, under `box_sizing`, with the art's ratio given to taffy.
fn laid_out(
    display: Display,
    width: Dimension,
    box_sizing: BoxSizing,
) -> (f32, f32) {
    let padding = LengthPercentage::length(PADDING);
    let mut tree: TaffyTree<bool> = TaffyTree::new();
    let leaf = tree
        .new_leaf_with_context(
            Style {
                size: Size {
                    width,
                    height: Dimension::auto(),
                },
                padding: Rect {
                    left: padding,
                    right: padding,
                    top: padding,
                    bottom: padding,
                },
                box_sizing,
                aspect_ratio: Some(ART.0 / ART.1),
                item_is_replaced: true,
                ..Style::default()
            },
            true,
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    let container = tree
        .new_with_children(
            Style {
                display,
                size: Size {
                    width: length(CONTAINER),
                    height: Dimension::auto(),
                },
                ..Style::default()
            },
            &[leaf],
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    tree.compute_layout_with_measure(
        container,
        Size {
            width: AvailableSpace::Definite(CONTAINER),
            height: AvailableSpace::MaxContent,
        },
        |inputs, _, context, style| {
            let art = matches!(context, Some(true));
            taffy::compute_leaf_layout(
                inputs,
                style,
                |_, _| 0.0,
                |known, _| if art { picture(known) } else { Size::ZERO },
            )
        },
    )
    .unwrap_or_else(|error| unreachable!("{error}"));
    let layout = tree
        .layout(leaf)
        .unwrap_or_else(|error| unreachable!("{error}"));
    (layout.size.width, layout.size.height)
}

#[test]
fn a_known_size_reaches_the_measure_function_as_the_border_box() {
    assert_eq!(
        laid_out(Display::Flex, Dimension::auto(), BoxSizing::ContentBox),
        (80.0, 73.0),
        "taffy measures a padded leaf in its content box now, Chrome's \
         80x60: delete the [WORKAROUND] narrowing in `measure_leaf` in \
         layout.rs"
    );
}

#[test]
fn a_border_box_takes_the_ratio() {
    assert_eq!(
        laid_out(Display::Block, length(100.0), BoxSizing::BorderBox),
        (100.0, 67.0),
        "taffy applies a picture's ratio to its content box under \
         border-box now, Chrome's 100x73.33: delete the [WORKAROUND] \
         `size_pictures_by_content` in layout.rs"
    );
}

// [FOUNDATION] the properties both compensations rest on: with no size known,
// taffy adds the padding to what the measure function answers, so an answer
// in the content box is right; and under `content-box` the ratio applies to
// the content box. If either stopped, the compensations would be off by it.
#[test]
fn the_content_box_is_measured_and_takes_the_ratio() {
    assert_eq!(
        laid_out(Display::Block, Dimension::auto(), BoxSizing::ContentBox),
        (80.0, 60.0),
        "the measured size"
    );
    assert_eq!(
        laid_out(Display::Block, length(80.0), BoxSizing::ContentBox),
        (100.0, 73.0),
        "the ratio in the content box"
    );
}
