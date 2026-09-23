//! Pins two inherited taffy defects -- a growing flex item drops its negative
//! margin; a clipping grid item makes its ancestor ignore one -- as taffy's
//! numbers beside Chrome's, so a fix fails here. DioxusLabs/taffy#1162 and
//! DioxusLabs/taffy#1163 are fixed by DioxusLabs/taffy#1164, in no release.

use taffy::{
    geometry::Point,
    prelude::{
        AvailableSpace, Display, FlexDirection, Rect, Size, Style, TaffyTree,
        auto, length,
    },
    style::Overflow,
};

/// The container's resolved height for one `flex-shrink` and one margin.
fn container_height(shrink: f32, top: f32) -> f32 {
    let mut tree: TaffyTree<()> = TaffyTree::new();
    // **Without this a fractional row proves nothing**: taffy rounds layout to
    // whole pixels by default, so Chrome's 499.5 and taffy's 500 would read as
    // one number and the case would agree by being unable to disagree.
    tree.disable_rounding();
    let child = tree
        .new_leaf(Style {
            size: Size {
                width: length(476.0),
                height: length(500.0),
            },
            flex_shrink: shrink,
            flex_direction: FlexDirection::Column,
            margin: Rect {
                left: length(0.0),
                right: length(0.0),
                top: length(top),
                bottom: length(0.0),
            },
            ..Style::default()
        })
        .unwrap_or_else(|error| unreachable!("{error}"));
    let container = tree
        .new_with_children(
            Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                size: Size {
                    width: length(903.0),
                    height: auto(),
                },
                ..Style::default()
            },
            &[child],
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    let page = tree
        .new_with_children(
            Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                size: Size {
                    width: length(903.0),
                    height: length(700.0),
                },
                ..Style::default()
            },
            &[container],
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    tree.compute_layout(
        page,
        Size {
            width: AvailableSpace::Definite(903.0),
            height: AvailableSpace::Definite(700.0),
        },
    )
    .unwrap_or_else(|error| unreachable!("{error}"));
    tree.layout(container)
        .unwrap_or_else(|error| unreachable!("{error}"))
        .size
        .height
}

#[test]
fn the_rows_taffy_gets_right_agree_with_chrome() {
    // The control: taffy and Chrome agree on every row here, so the growing
    // rows are a defect, not two meanings of one property. The four fractional
    // rows are DioxusLabs/taffy#1152's fix and read 375, 250, 0 and 0 on
    // 0.13.0 -- the check that `Cargo.toml`'s 0.14 floor holds.
    for (shrink, top, chrome) in [
        (1.0_f32, -24.0_f32, 476.0_f32),
        (0.0, 0.0, 500.0),
        (1.0, 0.0, 500.0),
        (0.0, 24.0, 524.0),
        (1.0, 24.0, 524.0),
        (0.0, -0.25, 499.75),
        (0.0, -0.5, 499.5),
        (0.0, -1.0, 499.0),
        (0.0, -24.0, 476.0),
    ] {
        let ours = container_height(shrink, top);
        assert!(
            (ours - chrome).abs() < 0.01,
            "shrink {shrink}, margin {top}: taffy {ours}, Chrome {chrome}"
        );
    }
}

/// The container's height with a **growing** child.
///
/// Separate from [`container_height`] because it measures a different defect:
/// the same tree with `flex_grow` added.
fn grown_container_height(grow: f32, shrink: f32, top: f32) -> f32 {
    let mut tree: TaffyTree<()> = TaffyTree::new();
    tree.disable_rounding();
    let child = tree
        .new_leaf(Style {
            size: Size {
                width: length(476.0),
                height: length(500.0),
            },
            flex_grow: grow,
            flex_shrink: shrink,
            margin: Rect {
                left: length(0.0),
                right: length(0.0),
                top: length(top),
                bottom: length(0.0),
            },
            ..Style::default()
        })
        .unwrap_or_else(|error| unreachable!("{error}"));
    let container = tree
        .new_with_children(
            Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                size: Size {
                    width: length(903.0),
                    height: auto(),
                },
                ..Style::default()
            },
            &[child],
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    tree.compute_layout(
        container,
        Size {
            width: AvailableSpace::Definite(903.0),
            height: AvailableSpace::MaxContent,
        },
    )
    .unwrap_or_else(|error| unreachable!("{error}"));
    tree.layout(container)
        .unwrap_or_else(|error| unreachable!("{error}"))
        .size
        .height
}

#[test]
fn a_growing_child_has_its_negative_margin_ignored_instead() {
    // Dropped, not over-applied: with `flex-grow: 1` the container is the
    // child's size, margin excluded. `-24` and `-0.5` both give 500 and
    // `flex-shrink` does not matter; Chrome gives `child + margin` for a
    // `476x500` child in an auto-height column. DioxusLabs/taffy#1162.
    for (grow, shrink, margin, taffy, chrome) in [
        (1.0_f32, 0.0_f32, -24.0_f32, 500.0_f32, 476.0_f32),
        (1.0, 0.0, -0.5, 500.0, 499.5),
        (1.0, 1.0, -24.0, 500.0, 476.0),
    ] {
        let ours = grown_container_height(grow, shrink, margin);
        assert!(
            (ours - taffy).abs() < 0.01,
            "grow {grow}, shrink {shrink}, margin {margin}: taffy now gives \
             {ours} where it gave {taffy} -- if this is Chrome's {chrome}, the \
             defect is fixed and this test has done its job"
        );
    }
}

#[test]
fn the_same_rows_without_growing_are_not_wrong() {
    // The control: each grow row above has a counterpart taffy gets right, and
    // the `shrink: 1` pair is correct without grow and wrong with it. Chrome
    // gives `child + margin` in every row.
    for (shrink, margin, chrome) in [
        (1.0_f32, -24.0_f32, 476.0_f32),
        (1.0, -0.5, 499.5),
        (1.0, 0.0, 500.0),
    ] {
        let ours = grown_container_height(0.0, shrink, margin);
        assert!(
            (ours - chrome).abs() < 0.01,
            "grow 0, shrink {shrink}, margin {margin}: taffy {ours}, Chrome \
             {chrome} -- this row is supposed to agree"
        );
    }
}

/// The height of a shrink-wrapping parent over a strip with `margin-top: -32`
/// holding a grid or flex container of one item. Chrome gives the strip's
/// height less 32 in all eight configurations.
fn clipped_item_parent_height(hidden: bool, grid: bool, wrapped: bool) -> f32 {
    let mut tree: TaffyTree<()> = TaffyTree::new();
    // Off, as in every harness here: a probe pins taffy's raw arithmetic, so a
    // fraction must survive to the assertion. `solve` rounds because that is
    // what a caller's pixel shows, which is a different question.
    tree.disable_rounding();
    let content = tree
        .new_leaf(Style {
            size: Size {
                width: length(100.0),
                height: length(100.0),
            },
            ..Style::default()
        })
        .unwrap_or_else(|error| unreachable!("{error}"));
    let item_style = Style {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        overflow: Point {
            x: if hidden {
                Overflow::Hidden
            } else {
                Overflow::Visible
            },
            y: if hidden {
                Overflow::Hidden
            } else {
                Overflow::Visible
            },
        },
        ..Style::default()
    };
    // The wrapper is a control rather than a variation: it becomes the grid
    // item, and its own overflow is `visible`, so the clipping box is no
    // longer the one the container measures.
    let item = if wrapped {
        let card = tree
            .new_with_children(item_style, &[content])
            .unwrap_or_else(|error| unreachable!("{error}"));
        tree.new_with_children(Style::default(), &[card])
            .unwrap_or_else(|error| unreachable!("{error}"))
    } else {
        tree.new_with_children(item_style, &[content])
            .unwrap_or_else(|error| unreachable!("{error}"))
    };
    let container = tree
        .new_with_children(
            Style {
                display: if grid { Display::Grid } else { Display::Flex },
                grid_template_columns: if grid {
                    vec![length(100.0)]
                } else {
                    vec![]
                },
                ..Style::default()
            },
            &[item],
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    let strip = tree
        .new_with_children(
            Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                margin: Rect {
                    left: length(0.0),
                    right: length(0.0),
                    top: length(-32.0),
                    bottom: length(0.0),
                },
                ..Style::default()
            },
            &[container],
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    let parent = tree
        .new_with_children(
            Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                ..Style::default()
            },
            &[strip],
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    tree.compute_layout(
        parent,
        Size {
            width: AvailableSpace::Definite(400.0),
            height: AvailableSpace::MaxContent,
        },
    )
    .unwrap_or_else(|error| unreachable!("{error}"));
    tree.layout(parent)
        .unwrap_or_else(|error| unreachable!("{error}"))
        .size
        .height
}

#[test]
fn a_clipping_grid_item_makes_an_ancestor_ignore_a_negative_margin() {
    // **Asserting the wrong number on purpose**, the same way the growing case
    // does. Chrome gives 68 here, and 68 is what the seven rows around it give.
    assert!(
        (clipped_item_parent_height(true, true, false) - 100.0).abs() < 0.01,
        "grid, direct, hidden: taffy now gives {} where it gave 100 -- if this \
         is Chrome's 68, the defect is fixed and this test has done its job",
        clipped_item_parent_height(true, true, false)
    );
}

#[test]
fn the_seven_configurations_around_it_agree_with_chrome() {
    // The control: the tree is right under flex, under `overflow: visible` and
    // with a wrapper. The wrapped-and-hidden row clips like the failing one and
    // comes out right, so the trigger is being the grid item that clips.
    for (hidden, grid, wrapped) in [
        (false, true, false),
        (true, true, true),
        (false, true, true),
        (true, false, false),
        (false, false, false),
        (true, false, true),
        (false, false, true),
    ] {
        let ours = clipped_item_parent_height(hidden, grid, wrapped);
        assert!(
            (ours - 68.0).abs() < 0.01,
            "{} , {} , overflow {}: taffy {ours}, Chrome 68 -- this row is \
             supposed to agree",
            if grid { "grid" } else { "flex" },
            if wrapped { "wrapped" } else { "direct" },
            if hidden { "hidden" } else { "visible" },
        );
    }
}

/// One container, one pin, and the whole subtree read back.
fn pinned_subtree(
    kids: &[(f32, f32)],
    pin: Option<f32>,
) -> (f32, Vec<(f32, f32)>) {
    let mut tree: TaffyTree<()> = TaffyTree::new();
    tree.disable_rounding();
    let ids: Vec<_> = kids
        .iter()
        .map(|&(grow, margin)| {
            tree.new_leaf(Style {
                size: Size {
                    width: length(476.0),
                    height: length(200.0),
                },
                flex_grow: grow,
                margin: Rect {
                    left: length(0.0),
                    right: length(0.0),
                    top: length(margin),
                    bottom: length(0.0),
                },
                ..Style::default()
            })
            .unwrap_or_else(|error| unreachable!("{error}"))
        })
        .collect();
    let container = tree
        .new_with_children(
            Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                size: Size {
                    width: length(903.0),
                    height: pin.map_or_else(auto, length),
                },
                ..Style::default()
            },
            &ids,
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    tree.compute_layout(
        container,
        Size {
            width: AvailableSpace::Definite(903.0),
            height: AvailableSpace::MaxContent,
        },
    )
    .unwrap_or_else(|error| unreachable!("{error}"));
    (
        tree.layout(container)
            .unwrap_or_else(|error| unreachable!("{error}"))
            .size
            .height,
        ids.iter()
            .map(|id| {
                let solved = tree
                    .layout(*id)
                    .unwrap_or_else(|error| unreachable!("{error}"));
                (solved.size.height, solved.location.y)
            })
            .collect(),
    )
}

/// [FOUNDATION] taffy's error is confined to the container's main size, which
/// is what lets `compensate_dropped_margins` write that one number and trust
/// the subtree. If it stops holding, the box comes out right around children
/// wrong by 224, 217 or 212, and nothing says so.
#[test]
fn a_correct_main_size_lays_the_subtree_out_correctly() {
    /// One shape: its name, its children as `(flex_grow, margin-top)` at 200
    /// tall, Chrome's container height, and Chrome's `(height, y)` per child.
    type Shape<'a> = (&'a str, &'a [(f32, f32)], f32, &'a [(f32, f32)]);

    let shapes: &[Shape<'_>] = &[
        ("one growing", &[(1.0, -24.0)], 176.0, &[(200.0, -24.0)]),
        (
            "two growing",
            &[(1.0, -24.0), (1.0, -10.0)],
            366.0,
            &[(200.0, -24.0), (200.0, 166.0)],
        ),
        (
            "growing beside static",
            &[(1.0, -24.0), (0.0, -10.0)],
            366.0,
            &[(200.0, -24.0), (200.0, 166.0)],
        ),
        (
            "negative beside positive",
            &[(1.0, -24.0), (1.0, 10.0)],
            386.0,
            &[(200.0, -24.0), (200.0, 186.0)],
        ),
    ];

    for (name, kids, chrome_container, chrome_kids) in shapes {
        let (unpinned, _) = pinned_subtree(kids, None);
        assert!(
            (unpinned - chrome_container).abs() > 0.5,
            "{name}: taffy resolved {unpinned} unpinned, which is Chrome's \
             {chrome_container} -- the defect this rests on is gone and the \
             compensation can go with it"
        );

        let (container, kids_out) =
            pinned_subtree(kids, Some(*chrome_container));
        assert!(
            (container - chrome_container).abs() < 0.01,
            "{name}: pinned to {chrome_container} and taffy resolved \
             {container} -- writing the main size is no longer enough to \
             reach it, which is the half of this property the compensation \
             does itself"
        );
        for (index, ((height, y), (want_height, want_y))) in
            kids_out.iter().zip(chrome_kids.iter()).enumerate()
        {
            assert!(
                (height - want_height).abs() < 0.01
                    && (y - want_y).abs() < 0.01,
                "{name}: child {index} is {height} tall at {y}, Chrome has it \
                 {want_height} tall at {want_y} -- a correct main size no \
                 longer yields a correct subtree"
            );
        }
    }
}

/// [FOUNDATION] in the grid case the children are already right and only the
/// ancestor's size ignores the margin, so the extent over them is its answer.
/// If it stops holding, a compensation reading that extent writes a number
/// with nothing behind it.
#[test]
fn a_clipping_grid_item_leaves_only_the_aggregate_wrong() {
    for overflow in [Overflow::Hidden, Overflow::Scroll] {
        let mut tree: TaffyTree<()> = TaffyTree::new();
        tree.disable_rounding();
        let content = tree
            .new_leaf(Style {
                size: Size {
                    width: length(100.0),
                    height: length(100.0),
                },
                ..Style::default()
            })
            .unwrap_or_else(|error| unreachable!("{error}"));
        let item = tree
            .new_with_children(
                Style {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    overflow: Point {
                        x: overflow,
                        y: overflow,
                    },
                    ..Style::default()
                },
                &[content],
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        let grid = tree
            .new_with_children(
                Style {
                    display: Display::Grid,
                    grid_template_columns: vec![length(100.0)],
                    ..Style::default()
                },
                &[item],
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        let strip = tree
            .new_with_children(
                Style {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    margin: Rect {
                        left: length(0.0),
                        right: length(0.0),
                        top: length(-32.0),
                        bottom: length(0.0),
                    },
                    ..Style::default()
                },
                &[grid],
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        let ancestor = tree
            .new_with_children(
                Style {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    ..Style::default()
                },
                &[strip],
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        tree.compute_layout(
            ancestor,
            Size {
                width: AvailableSpace::Definite(400.0),
                height: AvailableSpace::MaxContent,
            },
        )
        .unwrap_or_else(|error| unreachable!("{error}"));

        let solved = tree
            .layout(strip)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(
            (solved.location.y - -32.0).abs() < 0.01
                && (solved.size.height - 100.0).abs() < 0.01,
            "{overflow:?}: the strip is {} tall at {}, and the property this \
             rests on is that it is 100 tall at -32",
            solved.size.height,
            solved.location.y
        );

        let extent = solved.location.y + solved.size.height;
        assert!(
            (extent - 68.0).abs() < 0.01,
            "{overflow:?}: the children reach {extent}, Chrome's answer is 68"
        );

        let own = tree
            .layout(ancestor)
            .unwrap_or_else(|error| unreachable!("{error}"))
            .size
            .height;
        assert!(
            (own - 100.0).abs() < 0.01,
            "{overflow:?}: the ancestor is {own} and taffy no longer ignores \
             the margin -- the compensation can go"
        );
    }
}
