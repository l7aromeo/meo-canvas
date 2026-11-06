//! Pins an inherited taffy defect, taffy 0.15.1: a maximum height transfers
//! through an `aspect_ratio` into a maximum width even where the width is
//! declared, 90x60 for a declared 200 under `max-height: 60` at 3:2, as a flex
//! or grid item, out of flow, and in block flow unless replaced, where Chrome
//! gives 200x60. Asserts taffy's
//! numbers, so a fix fails here and the `[WORKAROUND]`
//! `unbound_declared_width` in `layout.rs` can go. `l7aromeo/meo-canvas#193`.

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
             `unbound_declared_width` in layout.rs"
        );
    }
}

// [FOUNDATION] the property `unbound_declared_width` rests on: a maximum width
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
