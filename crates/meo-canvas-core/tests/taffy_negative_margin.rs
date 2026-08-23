//! A taffy defect we inherit, pinned so that fixing it cannot pass unnoticed.
//!
//! # What is wrong
//!
//! **A column flex container with an automatic height resolves to zero when
//! its child has `flex-shrink: 0` and a negative main-axis margin.** The
//! container vanishes -- its own background with it -- while the child lays
//! out correctly at the negative offset. Positive margins are right, a zero
//! margin is right, and `flex-shrink: 1` is right.
//!
//! **It is the child's `flex-shrink` that triggers it, not the container's**,
//! which is the opposite of what the symptom suggests, since the box that
//! disappears is the container. Measured both ways round.
//!
//! # What the browser does
//!
//! Chrome, through the conformance harness's own Playwright rather than a page
//! written by hand: `display: flex; flex-direction: column; width: 903px;
//! height: auto`, one child `476x500` with `flex-shrink` and `margin-top`
//! varied, `getBoundingClientRect().height` on the parent.
//!
//! | `flex-shrink` | `margin-top` | Chrome | taffy |
//! |---|---|---|---|
//! | 0 | -24 | 476 | **0** |
//! | 1 | -24 | 476 | 476 |
//! | 0 | 0 | 500 | 500 |
//! | 1 | 0 | 500 | 500 |
//! | 0 | 24 | 524 | 524 |
//! | 1 | 24 | 524 | 524 |
//!
//! **Chrome gives the child's outer hypothetical main size in all six rows and
//! never lets `flex-shrink` into the answer** -- there is no free space
//! pressure anywhere in the tree, so the shrink axis is not a factor. So this
//! is a disagreement with the browser, which is our baseline, and not with a
//! reading of the specification.
//!
//! # Why the assertion is of the wrong number
//!
//! **A test asserting Chrome's 476 would fail today**, and a failing test
//! cannot be committed. So this pins what taffy actually does, with the right
//! answer beside it: **the day taffy is fixed, this test fails, and the
//! failure is the notification.** That is the whole reason it exists -- the
//! defect is otherwise silent, because a caller sees a missing subtree and no
//! error at all.
//!
//! Reproduced against taffy `0.13.0` and against `main` at `88125ce`, in
//! twenty lines of taffy with no code of ours in the picture. Not fixed
//! upstream and not filed: the changelog's only unreleased negative-margin
//! entry is for block and float layout, and issue #706 -- negative margins in
//! flexbox, closed -- reports sibling sizing and padding, mentions neither
//! `flex-shrink` nor a container resolving to zero.

use taffy::prelude::{
    AvailableSpace, Display, FlexDirection, Rect, Size, Style, TaffyTree, auto,
    length,
};

/// The container's resolved height for one `flex-shrink` and one margin.
fn container_height(shrink: f32, top: f32) -> f32 {
    let mut tree: TaffyTree<()> = TaffyTree::new();
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
fn the_five_rows_taffy_gets_right_agree_with_chrome() {
    // The control, and the reason the sixth row is a defect rather than a
    // convention: taffy and Chrome agree everywhere else in the table, so the
    // one disagreement cannot be explained by the two engines meaning
    // different things by these properties.
    for (shrink, top, chrome) in [
        (1.0_f32, -24.0_f32, 476.0_f32),
        (0.0, 0.0, 500.0),
        (1.0, 0.0, 500.0),
        (0.0, 24.0, 524.0),
        (1.0, 24.0, 524.0),
    ] {
        let ours = container_height(shrink, top);
        assert!(
            (ours - chrome).abs() < 0.01,
            "shrink {shrink}, margin {top}: taffy {ours}, Chrome {chrome}"
        );
    }
}

#[test]
fn a_negative_margin_under_a_rigid_child_still_collapses_its_container() {
    // **Asserting the wrong number on purpose.** Chrome says 476. When this
    // fails, taffy has been fixed: check the six rows above against the
    // browser, delete this test, and remove whatever workarounds cite it.
    let collapsed = container_height(0.0, -24.0);
    assert!(
        collapsed.abs() < 0.01,
        "taffy now gives {collapsed} where it gave 0 -- if this is Chrome's \
         476, the defect is fixed and this test has done its job"
    );
}
