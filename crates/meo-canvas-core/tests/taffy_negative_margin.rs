//! A taffy defect we inherit, pinned so that fixing it cannot pass unnoticed.
//!
//! # What is wrong
//!
//! **A negative margin on a *growing* flex item is dropped rather than
//! applied.** A flex container with an automatic main size resolves to the
//! child's size with the margin excluded, where it should resolve to
//! `child + margin`.
//!
//! ```text
//! child 500, grow 1, margin  -0.5   ->  500     Chrome 499.5
//! child 500, grow 1, margin -24     ->  500     Chrome 476
//! ```
//!
//! **It does not scale with the margin**, which is what separates it from the
//! multiply that shares this region of taffy: `-24` and `-0.5` give the same
//! 500. And **`flex-shrink` is irrelevant** -- the `shrink: 1` row is correct
//! without grow and wrong with it, so this is not another trigger for the same
//! mechanism.
//!
//! # The conditions, each measured rather than assumed
//!
//! - **All four main-axis edges.** `margin-top` and `margin-bottom` in a
//!   column, `margin-left` and `margin-right` in a row. Not top-specific.
//! - **A definite container height is correct.** The defect needs an automatic
//!   main size.
//! - **The container's own `flex-shrink` is irrelevant**, which is the opposite
//!   of what the symptom suggests, since the box that comes out wrong is the
//!   container.
//!
//! # What the browser does
//!
//! Chrome, through the conformance harness's own Playwright rather than a page
//! written by hand, `getBoundingClientRect()` unrounded: **the container is
//! the child's outer main size, `child + margin`, in every one of seventeen
//! rows, and `flex-shrink` never enters it.**
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
//! otherwise silent, because a caller sees a box of the wrong height and no
//! error.
//!
//! Reproduced against taffy `0.14.0` in twenty lines of taffy with no code of
//! ours in the picture. **Not filed upstream.**
//!
//! # The neighbouring defect, and why `0.14` is the floor
//!
//! One region of taffy holds two of these. The other is
//! <https://github.com/DioxusLabs/taffy/issues/1151>, *intrinsic flex sizing
//! with negative margins*: a negative margin on a **non-shrinking** item
//! applied as `child x max(0, 1 + margin)` instead of `child + margin`. It is
//! fixed by PR #1152 and released in `0.14.0`, which `Cargo.toml` requires,
//! so [`the_rows_taffy_gets_right_agree_with_chrome`] asserts Chrome's numbers
//! for those rows rather than taffy's.
//!
//! **The fix moves one `max`.** Two expressions were meant to be the same
//! quantity: the divisor read `max(1, flex_shrink * inner_flex_basis)` while
//! the multiplier that undoes it read `max(1, flex_shrink) * inner_flex_basis`.
//! At `flex_shrink: 0` that is a divide by `1` against a multiply by the basis,
//! so the margin comes back scaled by the item's size.
//!
//! **Sharing a region and a symptom is not evidence of sharing a cause**, and
//! the two here are the proof: the growing case reproduces identically on
//! `0.13.0`, on #1152's branch head `d680af5`, and on released `0.14.0`.

use taffy::prelude::{
    AvailableSpace, Display, FlexDirection, Rect, Size, Style, TaffyTree, auto,
    length,
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
    // The control, and the reason the growing rows are a defect rather than a
    // convention: taffy and Chrome agree everywhere else in the table, so the
    // one disagreement cannot be explained by the two engines meaning
    // different things by these properties.
    //
    // The four fractional rows are the ones #1152 fixes, so they are also the
    // check that `Cargo.toml`'s floor is doing its job: on `0.13.0` they read
    // 375, 250, 0 and 0.
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
    // **A second defect, and not the one above.** With `flex-grow: 1` the
    // negative margin is dropped rather than over-applied: the container comes
    // out the child's size, margin excluded.
    //
    // Two properties separate it from the multiply, and both are why it earns
    // its own pin rather than a row in the other test:
    //
    // - **It does not scale with the margin.** `-24` and `-0.5` both give 500,
    //   where proportionality was the multiply's entire signature.
    // - **`flex-shrink` is irrelevant.** The `shrink: 1` row is correct
    //   *without* grow and wrong *with* it, so this is not the same trigger
    //   wearing another hat.
    //
    // **Chrome measured by MC Main today through Playwright**: parent
    // `display: flex; flex-direction: column; width: 903px; height: auto;
    // align-items: flex-start`, child `476x500`, reading the parent's
    // `getBoundingClientRect().height`. **The taffy values are this test's
    // own**, and MC Main measured the same on released `0.13.0` and on PR head
    // `d680af5` -- identical on both, so **this is pre-existing and not a
    // regression from #1152**.
    //
    // **Not filed upstream.**
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
    // The control. Each row above has a counterpart here that taffy gets
    // right, so the grow rows cannot be explained by the margin being wrong in
    // this tree generally. **The `shrink: 1` pair is the sharp one**: correct
    // without grow, wrong with it.
    //
    // Chrome's `child + margin` in every row, which is the rule MC Main's
    // seventeen-row table found and which these three sit inside.
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
