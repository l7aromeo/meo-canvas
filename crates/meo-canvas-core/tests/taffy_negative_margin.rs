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
//! ours in the picture.
//!
//! # The second live one: `overflow: hidden` on a grid item
//!
//! **A grid item that clips makes an ancestor's height ignore a negative
//! margin.** One configuration of eight, and the seven around it are the
//! control: flex is unaffected, `overflow: visible` is unaffected, and
//! wrapping the item in a bare box fixes it, because the wrapper becomes the
//! item and its own overflow is `visible`.
//!
//! ```text
//! grid,  direct,  hidden   ->  100     Chrome 68
//! grid,  direct,  visible  ->   68     Chrome 68
//! grid,  wrapped, either   ->   68     Chrome 68
//! flex,  any,     either   ->   68     Chrome 68
//! ```
//!
//! Chrome gives 68 for all eight, measured as HTML through Playwright.
//!
//! # Upstream: filed, fixed on `main`, and not in a release
//!
//! Both live defects are <https://github.com/DioxusLabs/taffy/issues/1162> and
//! <https://github.com/DioxusLabs/taffy/issues/1163>, closed together by PR
//! #1164 -- *apply main-axis margin after flex-basis floor in intrinsic
//! contributions* -- merged as `adef6dd`. **`adef6dd` is two commits ahead of
//! the `v0.14.0` tag**, published the day before it merged, so neither fix is
//! in the release `Cargo.toml` requires and both pins below still hold.
//!
//! One expression is the cause of both: an item's content contribution read
//! `(inner + margin).max(flex_basis)`, and a flex basis is a border-box size
//! carrying no margin, so any negative main-axis margin vanished into the floor
//! whenever `inner + margin` fell under it. The fix floors first and adds the
//! margin after. The growing case reaches it through the definite-size fast
//! path; the grid case through the measured-content path, where `overflow:
//! hidden` inflates the item's flex basis to its max-content size.
//!
//! **Check that these fail before deleting them: a close is not a release.**
//!
//! # The neighbouring defect, and why `0.14` is the floor
//!
//! A third sits in the same region and *is* released:
//! <https://github.com/DioxusLabs/taffy/issues/1151>, *intrinsic flex sizing
//! with negative margins* -- a negative margin on a **non-shrinking** item
//! applied as `child x max(0, 1 + margin)` instead of `child + margin`. PR
//! #1152 fixes it and `0.14.0` carries it, which `Cargo.toml` requires, so
//! [`the_rows_taffy_gets_right_agree_with_chrome`] asserts Chrome's numbers for
//! those rows rather than taffy's.
//!
//! **That fix moves one `max`.** Two expressions were meant to be the same
//! quantity: the divisor read `max(1, flex_shrink * inner_flex_basis)` while
//! the multiplier that undoes it read `max(1, flex_shrink) * inner_flex_basis`.
//! At `flex_shrink: 0` that is a divide by `1` against a multiply by the basis,
//! so the margin comes back scaled by the item's size.
//!
//! **Sharing a region and a symptom is not evidence of sharing a cause.** The
//! growing case reproduces identically on `0.13.0`, on #1152's branch head
//! `d680af5`, and on released `0.14.0` -- three measurements that say #1152 was
//! never going to fix it, and #1164 is why.

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
    // own**, measured the same on released `0.13.0`, on #1152's branch head
    // `d680af5`, and on released `0.14.0` -- identical on all three, so **this
    // is pre-existing and not a regression from #1152**.
    //
    // **Filed as #1162, fixed on `main` by PR #1164, and in no release.** The
    // module documentation says why that keeps this pinned.
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

/// The shrink-wrapping parent's height for one of the eight configurations.
///
/// A parent holding a strip with `margin-top: -32`, holding a grid or flex
/// container, holding one item with content. The parent should come out the
/// strip's height less the 32 it is pulled up by, in all eight.
fn clipped_item_parent_height(hidden: bool, grid: bool, wrapped: bool) -> f32 {
    let mut tree: TaffyTree<()> = TaffyTree::new();
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
    // The control, and what makes the one row a defect rather than a reading of
    // the specification: the same tree is right under flex, right under
    // `overflow: visible`, and right with a wrapper, so no property here means
    // something different in the two engines.
    //
    // **The wrapped-and-hidden row is the sharp one.** It clips exactly as the
    // failing row does and comes out correct, so clipping alone is not the
    // trigger -- being the grid item that clips is.
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
