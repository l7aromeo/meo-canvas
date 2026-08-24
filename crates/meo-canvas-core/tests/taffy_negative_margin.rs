//! A taffy defect we inherit, pinned so that fixing it cannot pass unnoticed.
//!
//! # What is wrong
//!
//! **A negative margin on a non-shrinking flex item is applied as a multiplier
//! rather than as a length.** A flex container with an automatic main size
//! resolves to `child x max(0, 1 + margin)` where it should resolve to
//! `child + margin` -- the two agreeing only when the child is one pixel.
//!
//! ```text
//! child 500, margin  -0.25   ->  375     Chrome 499.75
//! child 500, margin  -0.5    ->  250     Chrome 499.5
//! child 500, margin  -1      ->    0     Chrome 499
//! child 500, margin -24      ->    0     Chrome 476
//! child 200, margin  -0.5    ->  100     Chrome 199.5
//! ```
//!
//! **Proportional to the child's own main size**, which is what makes it a
//! multiply and not a clamp: `500 -> 250`, `200 -> 100`, `80 -> 40`, all at
//! `-0.5`. Every realistic margin is at or beyond `-1`, so it presents as a
//! container that collapses to nothing -- which is how it was first described
//! here, and the description was of the symptom rather than the mechanism.
//!
//! # The conditions, each measured rather than assumed
//!
//! - **All four main-axis edges.** `margin-top` and `margin-bottom` in a
//!   column, `margin-left` and `margin-right` in a row. Not top-specific.
//! - **The child's `flex-shrink: 0` is required.** With `flex-shrink: 1` every
//!   configuration is correct -- both edges, both axes, nested, with a sibling,
//!   with an explicit `flex-basis`.
//! - **The container's own `flex-shrink` is irrelevant**, which is the opposite
//!   of what the symptom suggests, since the box that vanishes is the
//!   container.
//! - **A definite child and a content-sized child behave identically**, so the
//!   conversion is not reading an explicit size.
//! - **A percentage margin collapses too** -- consistent with the multiply,
//!   since a resolved `-5%` of a 903-wide container is `-45.15`, and `1 -
//!   45.15` is negative before anything is clamped.
//! - **A definite container height is correct**, and a growing container fills
//!   its parent in both engines. The defect needs an automatic main size.
//!
//! # What the browser does
//!
//! Chrome, through the conformance harness's own Playwright rather than a page
//! written by hand, `getBoundingClientRect()` unrounded: **the container is
//! the child's outer main size, `child + margin`, in every one of seventeen
//! rows, and `flex-shrink` never enters it.** Thirteen of those rows disagree
//! with taffy; three agree (a growing container, a definite height, and a
//! shrinking child with a sibling); one differs only by rounding.
//!
//! So this is a disagreement with the browser, which is our baseline, and not
//! with a reading of the specification.
//!
//! # Why the assertions are of the wrong numbers
//!
//! **A test asserting Chrome's values would fail today**, and a failing test
//! cannot be committed. So this pins what taffy actually does, with the right
//! answer beside it: **the day taffy is fixed, this fails, and the failure is
//! the notification.** That is the whole reason it exists -- the defect is
//! otherwise silent, because a caller sees a missing subtree and no error.
//!
//! Reproduced against taffy `0.13.0` and against `main` at `88125ce`, in
//! twenty lines of taffy with no code of ours in the picture. Not fixed
//! upstream: the changelog's only unreleased negative-margin entry is for
//! block and float layout, and issue #706 -- negative margins in flexbox,
//! closed -- reports sibling sizing and padding, mentions neither
//! `flex-shrink` nor a container resolving to zero.
//!
//! **Filed upstream as <https://github.com/DioxusLabs/taffy/issues/1151>.**
//! The report carries this mechanism, the six conditions above, and a
//! fourteen-row table against Chrome. It is the other half of this test: the
//! test notices the fix, the issue asks for it. When the issue closes, check
//! that this test fails before deleting it -- a close for any other reason
//! leaves the defect here.

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
fn a_negative_margin_is_still_applied_as_a_multiplier() {
    // **Asserting the wrong numbers on purpose.** Chrome's are in the comment
    // beside each. When one of these fails, taffy has been fixed: check the
    // table in this file's header against the browser, delete the test, and
    // remove whatever cites it.
    for (margin, taffy, chrome) in [
        (-24.0_f32, 0.0_f32, 476.0_f32),
        (-1.0, 0.0, 499.0),
        (-0.5, 250.0, 499.5),
        (-0.25, 375.0, 499.75),
    ] {
        let ours = container_height(0.0, margin);
        assert!(
            (ours - taffy).abs() < 0.01,
            "margin {margin}: taffy now gives {ours} where it gave {taffy} -- \
             if this is Chrome's {chrome}, the defect is fixed and this test \
             has done its job"
        );
    }
}
