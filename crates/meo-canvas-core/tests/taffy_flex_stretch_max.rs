//! Pins an inherited taffy defect, taffy 0.15.1 `src/compute/flexbox.rs`
//! `determine_flex_base_size`: in a flex row of indefinite height, a stretched
//! item with both sizes `auto` is measured at its maximum height, or at the
//! one its maximum width transfers through its ratio, as though stretched to
//! it. 60x40 art is 90x60 under `max-height: 60` and under `max-width: 90`,
//! where Chrome gives the natural 60x40. Asserts taffy's numbers, so a fix
//! fails here and the `[WORKAROUND]` `base_on_natural_sizes` in `layout.rs`
//! can go. `l7aromeo/meo-canvas#197`.

use taffy::prelude::{
    AlignItems, AvailableSpace, Dimension, Display, LengthPercentageAuto, Size,
    Style, TaffyTree, length,
};

/// The art, whose ratio is 1.5.
const ART: (f32, f32) = (60.0, 40.0);

/// The flex row's width, wider than the art.
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

/// How the item is set up: its alignment, its minimum and maximum, and the
/// flex basis it is given.
struct Item {
    align: AlignItems,
    min: Size<LengthPercentageAuto>,
    max: Size<LengthPercentageAuto>,
    basis: Dimension,
}

/// A stretched item with both sizes `auto` under `max`, nothing else stated.
const fn stretched(max: Size<LengthPercentageAuto>) -> Item {
    Item {
        align: AlignItems::STRETCH,
        min: Size::auto(),
        max,
        basis: Dimension::auto(),
    }
}

/// The item's laid-out size in a flex row 200 wide of `auto` height.
fn laid_out(item: &Item) -> (f32, f32) {
    let mut tree: TaffyTree<bool> = TaffyTree::new();
    let leaf = tree
        .new_leaf_with_context(
            Style {
                align_self: Some(item.align),
                flex_basis: item.basis,
                min_size: item.min,
                max_size: item.max,
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
                display: Display::Flex,
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
fn a_stretched_item_is_measured_at_its_maximum() {
    let (auto, px) =
        (LengthPercentageAuto::auto(), LengthPercentageAuto::length);
    for (bound, max) in [
        (
            "max-height: 60",
            Size {
                width: auto,
                height: px(60.0),
            },
        ),
        (
            "max-width: 90",
            Size {
                width: px(90.0),
                height: auto,
            },
        ),
    ] {
        assert_eq!(
            laid_out(&stretched(max)),
            (90.0, 60.0),
            "{bound}: taffy keeps a stretched item's natural size in a row of \
             indefinite height now, Chrome's 60x40: delete the [WORKAROUND] \
             `base_on_natural_sizes` in layout.rs"
        );
    }
}

#[test]
fn an_unstretched_item_keeps_its_natural_size() {
    // The control: the defect is the stretch, not the maximum.
    let item = Item {
        align: AlignItems::FLEX_START,
        ..stretched(Size {
            width: LengthPercentageAuto::auto(),
            height: LengthPercentageAuto::length(60.0),
        })
    };
    assert_eq!(laid_out(&item), ART);
}

// [FOUNDATION] the property `base_on_natural_sizes` rests on: given a definite
// basis and a minimum width, taffy sizes the stretched item from them, the
// height following through the ratio, rather than from its maximum. If it
// stopped, the basis would no longer keep the natural size.
#[test]
fn a_definite_basis_and_minimum_size_the_item() {
    let (auto, px) =
        (LengthPercentageAuto::auto(), LengthPercentageAuto::length);
    for (minimum, min_height, size) in
        [(60.0, auto, ART), (75.0, px(50.0), (75.0, 50.0))]
    {
        let item = Item {
            min: Size {
                width: px(minimum),
                height: min_height,
            },
            basis: length(ART.0),
            ..stretched(Size {
                width: auto,
                height: px(60.0),
            })
        };
        assert_eq!(laid_out(&item), size, "minimum {minimum}");
    }
}
