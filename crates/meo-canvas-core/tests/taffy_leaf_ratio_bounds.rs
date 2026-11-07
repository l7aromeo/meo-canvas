//! Pins an inherited taffy defect, taffy 0.15.1 `src/compute/leaf.rs`: a
//! replaced leaf with both sizes `auto` and a ratio takes a minimum through the
//! ratio but not a maximum, and derives no height from a clamped width. 60x40
//! art in block flow is 60x40 under `max-height: 20` and 30x40 under
//! `max-width: 30`, where Chrome gives 30x20 for both. Asserts taffy's numbers,
//! so a fix fails here and the `[WORKAROUND]` `bound_natural_sizes` in
//! `layout.rs` can go. `l7aromeo/meo-canvas#198`.

use taffy::prelude::{
    AvailableSpace, Dimension, Display, LengthPercentageAuto, Size, Style,
    TaffyTree, length,
};

/// The art, whose ratio is 1.5.
const ART: (f32, f32) = (60.0, 40.0);

/// The block container's width, wider than the art.
const CONTAINER: f32 = 200.0;

/// The art's size given what is known of it, as `fit_intrinsic` in
/// `meo-canvas-core` answers: the other axis through the ratio, or the art.
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

/// The leaf's laid-out size in block flow, both sizes `auto` and the art's
/// ratio, under `min` and `max`.
fn leaf(
    min: Size<LengthPercentageAuto>,
    max: Size<LengthPercentageAuto>,
) -> (f32, f32) {
    sized(Size::auto(), Some(ART.0 / ART.1), (min, max))
}

/// The leaf's laid-out size in block flow at `size`, with `ratio`, under
/// `bounds`, the minimum then the maximum.
fn sized(
    size: Size<Dimension>,
    ratio: Option<f32>,
    (min, max): (Size<LengthPercentageAuto>, Size<LengthPercentageAuto>),
) -> (f32, f32) {
    let mut tree: TaffyTree<bool> = TaffyTree::new();
    let leaf = tree
        .new_leaf_with_context(
            Style {
                size,
                min_size: min,
                max_size: max,
                aspect_ratio: ratio,
                item_is_replaced: true,
                ..Style::default()
            },
            true,
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    let container = tree
        .new_with_children(
            Style {
                display: Display::Block,
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
fn a_maximum_does_not_reach_the_other_axis_through_the_ratio() {
    let auto = LengthPercentageAuto::auto();
    let px = LengthPercentageAuto::length;
    for (bound, max, taffy) in [
        (
            "max-height: 20",
            Size {
                width: auto,
                height: px(20.0),
            },
            (60.0, 40.0),
        ),
        (
            "max-width: 30",
            Size {
                width: px(30.0),
                height: auto,
            },
            (30.0, 40.0),
        ),
    ] {
        assert_eq!(
            leaf(Size::auto(), max),
            taffy,
            "{bound}: taffy bounds an auto-sized replaced leaf through its \
             ratio now, Chrome's 30x20: delete the [WORKAROUND] \
             `bound_natural_sizes` in layout.rs once the leaf reaches taffy \
             with an auto width"
        );
    }
}

#[test]
fn a_minimum_height_does_reach_the_width() {
    // The control: the transfer the maximum lacks, 240x160 as in Chrome.
    let min = Size {
        width: LengthPercentageAuto::auto(),
        height: LengthPercentageAuto::length(160.0),
    };
    assert_eq!(leaf(min, Size::auto()), (240.0, 160.0));
}

// [FOUNDATION] the property `bound_natural_sizes` rests on: given both sizes
// and no ratio, the leaf keeps them under the bounds it was sized by, the
// minimum winning where the maximum is below it. If it stopped, a pin from the
// table would be clamped again on the axis the table gave way on.
#[test]
fn both_sizes_and_no_ratio_hold_under_their_bounds() {
    let auto = LengthPercentageAuto::auto();
    let px = LengthPercentageAuto::length;
    for (size, bounds) in [
        (
            (30.0, 20.0),
            (
                Size::auto(),
                Size {
                    width: auto,
                    height: px(20.0),
                },
            ),
        ),
        (
            (45.0, 30.0),
            (
                Size {
                    width: auto,
                    height: px(30.0),
                },
                Size {
                    width: auto,
                    height: px(20.0),
                },
            ),
        ),
        (
            (30.0, 30.0),
            (
                Size {
                    width: auto,
                    height: px(30.0),
                },
                Size {
                    width: px(30.0),
                    height: auto,
                },
            ),
        ),
    ] {
        let declared = Size {
            width: length(size.0),
            height: length(size.1),
        };
        assert_eq!(sized(declared, None, bounds), size);
    }
}
