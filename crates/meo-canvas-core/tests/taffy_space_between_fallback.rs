//! taffy alone on an overflowing `space-between` in a reversed axis, which it
//! packs at `start` and Chrome at `flex-start` (`flex-overflow.tsv`). Pins
//! taffy's numbers: a change fails here, and the [WORKAROUND] in `layout.rs`
//! can go. Deliberate upstream: DioxusLabs/taffy#1363.

use taffy::prelude::{
    AlignContent, AvailableSpace, Display, FlexDirection, FlexWrap,
    JustifyContent, Size, Style, TaffyTree, length,
};

/// The container's edge: every case is a 40x40 flex container.
const BOX: f32 = 40.0;

/// Each child's `(x, y)` relative to a 40x40 container of `style`, children
/// sized as given with `flex-shrink: 0`. Unrounded, so no edge is a rounding.
fn placed(style: Style, children: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut tree: TaffyTree<()> = TaffyTree::new();
    tree.disable_rounding();
    let nodes: Vec<_> = children
        .iter()
        .map(|(width, height)| {
            tree.new_leaf(Style {
                size: Size {
                    width: length(*width),
                    height: length(*height),
                },
                flex_shrink: 0.0,
                ..Style::default()
            })
            .unwrap_or_else(|error| unreachable!("{error}"))
        })
        .collect();
    let container = tree
        .new_with_children(
            Style {
                display: Display::Flex,
                size: Size {
                    width: length(BOX),
                    height: length(BOX),
                },
                ..style
            },
            &nodes,
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    tree.compute_layout(
        container,
        Size {
            width: AvailableSpace::MaxContent,
            height: AvailableSpace::MaxContent,
        },
    )
    .unwrap_or_else(|error| unreachable!("{error}"));
    nodes
        .iter()
        .map(|node| {
            let location = tree
                .layout(*node)
                .unwrap_or_else(|error| unreachable!("{error}"))
                .location;
            (location.x, location.y)
        })
        .collect()
}

#[test]
fn an_overflowing_reversed_item_run_is_still_packed_at_start() {
    let items = placed(
        Style {
            flex_direction: FlexDirection::RowReverse,
            justify_content: JustifyContent::SPACE_BETWEEN,
            ..Style::default()
        },
        &[(47.0, 20.0)],
    );
    assert_eq!(
        items,
        [(0.0, 0.0)],
        "taffy places a 47-wide item under `row-reverse` and `space-between` \
         at {items:?} now, Chrome's x = -7: delete the [WORKAROUND] \
         `pack_space_between_at_flex_start` in layout.rs"
    );
}

#[test]
fn an_overflowing_reversed_line_stack_is_still_packed_at_start() {
    let lines = placed(
        Style {
            flex_wrap: FlexWrap::WrapReverse,
            align_content: AlignContent::SPACE_BETWEEN,
            ..Style::default()
        },
        &[(40.0, 30.0), (40.0, 30.0)],
    );
    assert_eq!(
        lines,
        [(0.0, 30.0), (0.0, 0.0)],
        "taffy places two 30-tall lines under `wrap-reverse` and \
         `space-between` at {lines:?} now, Chrome's y = 10 and -20: delete \
         the [WORKAROUND] `pack_space_between_at_flex_start` in layout.rs"
    );
}

// [FOUNDATION] the property the wrapped half of
// `pack_space_between_at_flex_start` rests on: an item wider than the line is
// alone on it and the line after it is distributed, so moving that item alone
// leaves the other lines Chrome's.
#[test]
fn an_item_wider_than_the_line_is_alone_on_it() {
    let items = placed(
        Style {
            flex_direction: FlexDirection::RowReverse,
            flex_wrap: FlexWrap::Wrap,
            justify_content: JustifyContent::SPACE_BETWEEN,
            ..Style::default()
        },
        &[(47.0, 10.0), (15.0, 10.0), (15.0, 10.0)],
    );
    assert_eq!(
        items,
        [(0.0, 0.0), (25.0, 20.0), (0.0, 20.0)],
        "the wider item shares its line, or the next line is not spread as \
         Chrome spreads it (25 and 0): moving the item alone is no longer \
         enough"
    );
}
