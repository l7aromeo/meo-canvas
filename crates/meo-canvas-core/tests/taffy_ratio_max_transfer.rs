//! Pins taffy 0.15.1 carrying a maximum through `aspect_ratio` onto a declared
//! other axis, 90x60 and 50x33.33 where Chrome gives 200x60 and 50x60. A fix
//! fails here and the [WORKAROUND] `unbound_declared_sizes` can go.
//! `l7aromeo/meo-canvas#193`, `l7aromeo/meo-canvas#129`.

use taffy::prelude::{
    AvailableSpace, Dimension, Display, LengthPercentageAuto, Position, Size,
    Style, TaffyTree, length,
};

/// The declared width, and the maximum height whose ratio width is below it.
const DECLARED: (f32, f32) = (200.0, 60.0);

/// The ratio, which transfers that maximum into a width of 90.
const RATIO: f32 = 1.5;

/// A maximum width as large as `FINITE_CEILING` in `meo-canvas-core`, which
/// the compensation writes and which is crate-private.
const CEILING: f32 = 3.355_443_2e7;

/// The leaf's laid-out size in a 200-wide container of `display`, its own
/// `position`, whether it is replaced, and its maximum width as given.
fn leaf(
    (display, position, replaced): (Display, Position, bool),
    max_width: LengthPercentageAuto,
) -> (f32, f32) {
    let mut tree: TaffyTree<()> = TaffyTree::new();
    let leaf = tree
        .new_leaf(Style {
            position,
            size: Size {
                width: length(DECLARED.0),
                height: Dimension::auto(),
            },
            max_size: Size {
                width: max_width,
                height: LengthPercentageAuto::length(DECLARED.1),
            },
            aspect_ratio: Some(RATIO),
            item_is_replaced: replaced,
            ..Style::default()
        })
        .unwrap_or_else(|error| unreachable!("{error}"));
    let container = tree
        .new_with_children(
            Style {
                display,
                size: Size {
                    width: length(DECLARED.0),
                    height: Dimension::auto(),
                },
                ..Style::default()
            },
            &[leaf],
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    tree.compute_layout(
        container,
        Size {
            width: AvailableSpace::Definite(DECLARED.0),
            height: AvailableSpace::MaxContent,
        },
    )
    .unwrap_or_else(|error| unreachable!("{error}"));
    let layout = tree
        .layout(leaf)
        .unwrap_or_else(|error| unreachable!("{error}"));
    (layout.size.width, layout.size.height)
}

/// The containers the transfer is seen in, and whether the leaf is replaced: a
/// replaced one in block flow is floored at 200x133 instead, which
/// `taffy_leaf_ratio_floor.rs` pins.
const WHERE: [(Display, Position, bool); 4] = [
    (Display::Flex, Position::Relative, true),
    (Display::Grid, Position::Relative, true),
    (Display::Block, Position::Absolute, true),
    (Display::Block, Position::Relative, false),
];

#[test]
fn a_maximum_height_transfers_into_a_declared_width() {
    for case in WHERE {
        assert_eq!(
            leaf(case, LengthPercentageAuto::auto()),
            (90.0, DECLARED.1),
            "{case:?}: taffy keeps a declared width under a \
             transferred maximum now: delete the [WORKAROUND] \
             `unbound_declared_sizes` in layout.rs"
        );
    }
}

// [FOUNDATION] the property `unbound_declared_sizes` rests on: a maximum width
// that is not `auto` stops the transfer, so the largest one leaves the declared
// width; if it stopped, the width would narrow again. Width alone: a replaced
// leaf's height is floored at 133 here, as `taffy_leaf_ratio_floor.rs` pins.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "taffy lays a declared length out as that length, exactly"
)]
fn a_stated_maximum_width_stops_the_transfer() {
    for case in WHERE {
        assert_eq!(
            leaf(case, LengthPercentageAuto::length(CEILING)).0,
            DECLARED.0,
            "{case:?}"
        );
    }
}

/// The transpose: a declared height of 60, 90 wide through the ratio, and a
/// maximum width of 50 below that.
const TALL: (f32, f32) = (60.0, 50.0);

/// A ratio leaf with the declared height of [`TALL`] under its maximum width,
/// in a 200-wide container of `display`, with its own `position` and maximum
/// height as given. Not replaced: a replaced leaf in block flow keeps it
/// already.
fn tall_leaf(
    (display, position): (Display, Position),
    max_height: LengthPercentageAuto,
) -> (f32, f32) {
    let mut tree: TaffyTree<()> = TaffyTree::new();
    tree.disable_rounding();
    let leaf = tree
        .new_leaf(Style {
            position,
            size: Size {
                width: Dimension::auto(),
                height: length(TALL.0),
            },
            max_size: Size {
                width: LengthPercentageAuto::length(TALL.1),
                height: max_height,
            },
            aspect_ratio: Some(RATIO),
            ..Style::default()
        })
        .unwrap_or_else(|error| unreachable!("{error}"));
    let container = tree
        .new_with_children(
            Style {
                display,
                size: Size {
                    width: length(DECLARED.0),
                    height: Dimension::auto(),
                },
                ..Style::default()
            },
            &[leaf],
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    tree.compute_layout(
        container,
        Size {
            width: AvailableSpace::Definite(DECLARED.0),
            height: AvailableSpace::MaxContent,
        },
    )
    .unwrap_or_else(|error| unreachable!("{error}"));
    let layout = tree
        .layout(leaf)
        .unwrap_or_else(|error| unreachable!("{error}"));
    (layout.size.width, layout.size.height)
}

/// The containers the transposed transfer is seen in.
const TALL_WHERE: [(Display, Position); 4] = [
    (Display::Flex, Position::Relative),
    (Display::Grid, Position::Relative),
    (Display::Block, Position::Absolute),
    (Display::Block, Position::Relative),
];

#[test]
fn a_maximum_width_transfers_into_a_declared_height() {
    for case in TALL_WHERE {
        let (width, height) = tall_leaf(case, LengthPercentageAuto::auto());
        assert!(
            (width - TALL.1).abs() < 0.01
                && (height - TALL.1 / RATIO).abs() < 0.01,
            "{case:?}: taffy gives {width}x{height} where it gave 50x33.33: \
             if the height is now the declared 60, delete the height half of \
             the [WORKAROUND] `unbound_declared_sizes` in layout.rs"
        );
    }
}

// [FOUNDATION] the property the height half of `unbound_declared_sizes` rests
// on: a maximum height that is not `auto` stops the transfer, so the largest
// one leaves the declared height; if it stopped, the height would shrink again.
#[test]
#[expect(
    clippy::float_cmp,
    reason = "taffy lays a declared length out as that length, exactly"
)]
fn a_stated_maximum_height_stops_the_transfer() {
    for case in TALL_WHERE {
        assert_eq!(
            tall_leaf(case, LengthPercentageAuto::length(CEILING)).1,
            TALL.0,
            "{case:?}"
        );
    }
}
