//! Pins an inherited taffy defect, taffy 0.15.1: a minimum height transfers
//! through an `aspect_ratio` into a minimum width even where the width is
//! declared, 240x160 for a declared 200 under `min-height: 160` at 3:2, in
//! block flow, as a flex item and out of flow, where Chrome gives 200x160.
//! Asserts taffy's numbers, so a fix fails here and the `[WORKAROUND]`
//! `keep_declared_widths` in `layout.rs` can go. `l7aromeo/meo-canvas#192`.

use taffy::prelude::{
    AvailableSpace, Dimension, Display, LengthPercentageAuto, Position, Size,
    Style, TaffyTree, length,
};

/// The declared width, and the minimum height whose ratio width exceeds it.
const DECLARED: (f32, f32) = (200.0, 160.0);

/// The ratio, which transfers that minimum into a width of 240.
const RATIO: f32 = 1.5;

/// The leaf's laid-out size in a 200-wide container of `display`, its own
/// `position` and minimum width as given.
fn leaf(
    display: Display,
    position: Position,
    min_width: LengthPercentageAuto,
) -> (f32, f32) {
    let mut tree: TaffyTree<()> = TaffyTree::new();
    let leaf = tree
        .new_leaf(Style {
            position,
            size: Size {
                width: length(DECLARED.0),
                height: Dimension::auto(),
            },
            min_size: Size {
                width: min_width,
                height: LengthPercentageAuto::length(DECLARED.1),
            },
            aspect_ratio: Some(RATIO),
            item_is_replaced: true,
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

#[test]
fn a_minimum_height_transfers_into_a_declared_width() {
    let auto = LengthPercentageAuto::auto();
    for (display, position) in [
        (Display::Block, Position::Relative),
        (Display::Flex, Position::Relative),
        (Display::Block, Position::Absolute),
    ] {
        assert_eq!(
            leaf(display, position, auto),
            (240.0, 160.0),
            "{display:?} {position:?}: taffy keeps a declared width under a \
             transferred minimum now: delete the [WORKAROUND] \
             `keep_declared_widths` in layout.rs"
        );
    }
}

// [FOUNDATION] the property `keep_declared_widths` rests on: a minimum width
// that is not `auto` stops the transfer, so the declared width as the minimum
// leaves the declared width. If it stopped, the pin would no longer hold it.
#[test]
fn a_stated_minimum_width_stops_the_transfer() {
    let declared = LengthPercentageAuto::length(DECLARED.0);
    for (display, position) in [
        (Display::Block, Position::Relative),
        (Display::Flex, Position::Relative),
        (Display::Block, Position::Absolute),
    ] {
        assert_eq!(
            leaf(display, position, declared),
            DECLARED,
            "{display:?} {position:?}"
        );
    }
}
