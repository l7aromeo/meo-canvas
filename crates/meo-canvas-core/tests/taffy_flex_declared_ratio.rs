//! Pins two inherited taffy defects, taffy 0.15.1 `src/compute/flexbox.rs`,
//! for a flex item with a ratio, a declared width and an `auto` height in a
//! row too narrow for it. 60x40 art declared 300 wide in a 200 row is 300x200:
//! its automatic minimum is measured at the height 300 gives, so it is 300.
//! With that minimum removed it shrinks to 200 and keeps the height of 300,
//! 200x200. Chrome gives 200x133.33. Asserts taffy's numbers, so a fix fails
//! here and the `[WORKAROUND]`s `floor_flexed_minimums` and
//! `derive_flexed_cross_sizes` in `layout.rs` can go.
//! `l7aromeo/meo-canvas#196`.

use taffy::prelude::{
    AvailableSpace, Dimension, Display, LengthPercentageAuto, Size, Style,
    TaffyTree, length,
};

/// The art, whose ratio is 1.5.
const ART: (f32, f32) = (60.0, 40.0);

/// The flex row's width, narrower than the declared width.
const ROW: f32 = 200.0;

/// The width the item declares.
const DECLARED: f32 = 300.0;

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

/// The item's laid-out size in a flex row 200 wide of `auto` height, given
/// its style.
fn laid_out(style: Style) -> (f32, f32) {
    let mut tree: TaffyTree<bool> = TaffyTree::new();
    let leaf = tree
        .new_leaf_with_context(style, true)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let container = tree
        .new_with_children(
            Style {
                display: Display::Flex,
                size: Size {
                    width: length(ROW),
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
            width: AvailableSpace::Definite(ROW),
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

/// The item declared 300 wide with the art's ratio, under `min_width`.
fn declared(min_width: LengthPercentageAuto) -> Style {
    Style {
        size: Size {
            width: length(DECLARED),
            height: Dimension::auto(),
        },
        min_size: Size {
            width: min_width,
            height: LengthPercentageAuto::auto(),
        },
        aspect_ratio: Some(ART.0 / ART.1),
        item_is_replaced: true,
        ..Style::default()
    }
}

#[test]
fn the_automatic_minimum_is_the_declared_width() {
    assert_eq!(
        laid_out(declared(LengthPercentageAuto::auto())),
        (DECLARED, 200.0),
        "taffy shrinks an item with a ratio below its declared width now, \
         Chrome's 200x133.33: delete the [WORKAROUND] \
         `floor_flexed_minimums` in layout.rs"
    );
}

#[test]
fn a_shrunk_item_keeps_the_height_of_its_declared_width() {
    assert_eq!(
        laid_out(declared(LengthPercentageAuto::length(0.0))),
        (ROW, 200.0),
        "taffy derives a flexed item's height from its flexed width now, \
         Chrome's 133.33: delete the [WORKAROUND] \
         `derive_flexed_cross_sizes` in layout.rs"
    );
}

// [FOUNDATION] the property both compensations rest on: an explicit minimum
// width lets the item shrink to the line, and an item given both sizes, no
// ratio, and no flexing keeps them. If either stopped, the pins would no
// longer hold the sizes Chrome gives.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "taffy lays a whole length out as that length, exactly"
)]
fn a_minimum_and_two_pinned_sizes_hold() {
    assert_eq!(
        laid_out(declared(LengthPercentageAuto::length(ART.0))).0,
        ROW,
        "the minimum"
    );
    // Whole lengths, since the probe solves with taffy's rounding on.
    let pinned = Style {
        size: Size {
            width: length(ROW),
            height: length(ART.1),
        },
        flex_grow: 0.0,
        flex_shrink: 0.0,
        item_is_replaced: true,
        ..Style::default()
    };
    assert_eq!(laid_out(pinned), (ROW, ART.1), "the pin");
}
