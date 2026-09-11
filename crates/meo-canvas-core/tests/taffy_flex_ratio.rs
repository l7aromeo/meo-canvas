//! A taffy defect we inherit, pinned so that fixing it cannot pass unnoticed.
//!
//! # What is wrong
//!
//! **A ratio does not derive the cross size when the main size came from
//! `flex-grow`.** A column item that grows into its line gets the height the
//! growth gives it and keeps a cross size of nothing, where the ratio should
//! turn that height into a width.
//!
//! ```text
//! column, align-items: center, height 264, grow 1, ratio 1
//!     taffy    0 x 248        Chrome  248 x 248
//! column, align-items: stretch, height 264, grow 1, ratio 1
//!     taffy  424 x 248        Chrome  424 x 424
//! ```
//!
//! The two rows fail in opposite directions and that is the point: **taffy
//! never transfers between the axes at all.** Centred, the width is never
//! derived from the grown height; stretched, the height is never derived from
//! the stretched width. In both, whichever axis the ratio should have produced
//! is the one that stayed as it was.
//!
//! # The conditions, each measured rather than assumed
//!
//! - **`flex-grow` is what breaks it, not the ratio.** The same tree with
//!   `height: 100%` instead of a grow derives the width correctly -- `248 x
//!   248`, agreeing with Chrome. A percentage main size and a grown one are the
//!   same number and only one of them reaches the ratio.
//! - **An automatic container height is unaffected.** With nothing to grow
//!   into, centred gives `0 x 0` and stretched `424 x 424`, both Chrome's.
//! - **A ratio-less row is correct**, so the growth itself is not what fails:
//!   `0 x 248` centred with no ratio is what Chrome gives.
//!
//! Those three are [`the_rows_taffy_gets_right_agree_with_chrome`], and they
//! are what make the two wrong rows a statement about the ratio rather than
//! about flex.
//!
//! # What the browser does
//!
//! Chrome, through the conformance harness's own Playwright rather than a page
//! written by hand, `getBoundingClientRect()` unrounded, on the same six
//! shapes: a `440x264` column with `box-sizing: border-box` and 8px of padding,
//! so the content box is `424x248` in every row.
//!
//! So this is a disagreement with the browser, which is our baseline, and not
//! with a reading of the specification.
//!
//! # Why the assertions are of the wrong numbers
//!
//! **A test asserting Chrome's values would fail today**, and a failing test
//! cannot be committed. So this pins what taffy actually does, with the right
//! answer beside it: **the day taffy transfers the ratio, this fails, and the
//! failure is the notification.** The defect is otherwise silent, because a
//! caller sees a box of the wrong width and no error.
//!
//! Reproduced in twenty lines of taffy with no code of ours in the picture.
//!
//! # Upstream, and what it is not
//!
//! <https://github.com/DioxusLabs/taffy/issues/804>, *`aspect_ratio` is not
//! respected in flex layouts*, open. Reached from the row that actually
//! diverges rather than from a title: `l7aromeo/meo-canvas#123` reports an
//! image laid out at its intrinsic size, and the image is not what is wrong
//! there -- a replaced element in that scene is `1024x1024` in Chrome too. The
//! report substituted a `div` for the image when measuring the browser, and
//! the `div` is the thing that diverges.
//!
//! **This is the third `[WORKAROUND]` in `layout.rs`.** The other two are
//! about a ratio box whose *inline* size is fit-content; this is one whose
//! *main* size arrives from flex growth, which is a different question about
//! the same upstream defect.
//!
//! The compensation covers the half where the item has no cross contribution
//! of its own: `derived_cross` multiplies the grown main size by the ratio.
//! The stretched half is left alone deliberately -- Chrome's answer overflows
//! the line, and the `align-items: stretch` row below pins what taffy does
//! with it instead.

use taffy::prelude::*;

/// The item's solved size in a `440x264` column with 8px of padding.
///
/// `height: None` leaves the container automatic, which is the control for
/// "there is nothing to grow into".
fn item(
    parent_height: Option<f32>,
    ratio: Option<f32>,
    grow: f32,
    centre: bool,
    percentage_height: bool,
) -> (f32, f32) {
    let mut tree: TaffyTree<()> = TaffyTree::new();
    let child = tree
        .new_leaf(Style {
            flex_grow: grow,
            aspect_ratio: ratio,
            size: Size {
                width: Dimension::auto(),
                height: if percentage_height {
                    Dimension::percent(1.0)
                } else {
                    Dimension::auto()
                },
            },
            ..Default::default()
        })
        .unwrap_or_else(|error| unreachable!("{error}"));
    let column = tree
        .new_with_children(
            Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                align_items: if centre {
                    Some(AlignItems::CENTER)
                } else {
                    None
                },
                padding: Rect {
                    left: LengthPercentage::length(8.0),
                    right: LengthPercentage::length(8.0),
                    top: LengthPercentage::length(8.0),
                    bottom: LengthPercentage::length(8.0),
                },
                size: Size {
                    width: Dimension::length(440.0),
                    height: parent_height
                        .map_or_else(Dimension::auto, Dimension::length),
                },
                ..Default::default()
            },
            &[child],
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    tree.compute_layout(
        column,
        Size {
            width: AvailableSpace::Definite(440.0),
            height: AvailableSpace::Definite(264.0),
        },
    )
    .unwrap_or_else(|error| unreachable!("{error}"));
    let solved = tree
        .layout(child)
        .unwrap_or_else(|error| unreachable!("{error}"));
    (solved.size.width, solved.size.height)
}

#[test]
fn a_grown_main_size_never_reaches_the_ratio() {
    // Both rows have `flex-grow: 1` and a ratio, and in both the axis the
    // ratio should have produced is the one that did not move. Centred, the
    // width stays at the content's nothing; stretched, the height stays at the
    // line's 248 instead of following the 424 width.
    for (centre, taffy_width, taffy_height, chrome_width, chrome_height) in [
        (true, 0.0_f32, 248.0_f32, 248.0_f32, 248.0_f32),
        (false, 424.0, 248.0, 424.0, 424.0),
    ] {
        let (width, height) = item(Some(264.0), Some(1.0), 1.0, centre, false);
        let how = if centre { "center" } else { "stretch" };
        assert!(
            (width - taffy_width).abs() < 0.01
                && (height - taffy_height).abs() < 0.01,
            "align-items {how}, grow 1, ratio 1: taffy now gives \
             {width} x {height} where it gave {taffy_width} x {taffy_height} \
             -- if this is Chrome's {chrome_width} x {chrome_height}, the \
             defect is fixed and this test has done its job"
        );
    }
}

// [FOUNDATION] the property `compensate_ratio_direction`'s third arm rests on:
// **taffy grows the main size to the number Chrome gives.** The compensation
// reads that solved main size and multiplies it by the ratio, so the `grow with
// no ratio` row below -- `0 x 248`, Chrome's own answer -- is the 248 every
// derived cross size is computed from.
//
// If taffy's growth stopped being Chrome's, the compensation would derive a
// width from a wrong height and **every conformance row would stay green**: no
// table in this tree has a grown flex item with a ratio in it except
// `flex-ratio-cross.tsv`, whose rows are all computed from the same wrong
// number and would move together. The first sign would be a golden shifting
// with nothing saying why.
#[test]
fn the_rows_taffy_gets_right_agree_with_chrome() {
    // The controls, and each removes one suspect. A percentage main size
    // reaches the ratio, so the ratio is not simply unimplemented here. An
    // automatic container height has nothing to grow into, so the defect needs
    // the growth rather than the ratio. And a ratio-less row grows correctly,
    // so the growth itself is not what fails.
    //
    // Chrome's numbers, measured on the same six shapes.
    for (name, height, ratio, grow, centre, percentage, chrome) in [
        (
            "height 100% instead of grow",
            Some(264.0),
            Some(1.0),
            0.0,
            true,
            true,
            (248.0_f32, 248.0_f32),
        ),
        (
            "grow with no ratio",
            Some(264.0),
            None,
            1.0,
            true,
            false,
            (0.0, 248.0),
        ),
        (
            "automatic height, centred",
            None,
            Some(1.0),
            1.0,
            true,
            false,
            (0.0, 0.0),
        ),
        (
            "automatic height, stretched",
            None,
            Some(1.0),
            1.0,
            false,
            false,
            (424.0, 424.0),
        ),
    ] {
        let (width, height_out) = item(height, ratio, grow, centre, percentage);
        assert!(
            (width - chrome.0).abs() < 0.01
                && (height_out - chrome.1).abs() < 0.01,
            "{name}: taffy gives {width} x {height_out} where Chrome gives \
             {} x {} -- this row is a control for \
             `a_grown_main_size_never_reaches_the_ratio` and it has stopped \
             being one",
            chrome.0,
            chrome.1
        );
    }
}

/// A grown item's solved size with a written size and an optional cross-axis
/// maximum, in a `424x248` column.
///
/// Written rather than derived, because that is the state
/// `compensate_ratio_direction` leaves a node in before the second solve: the
/// compensation has already put a size on both axes and the question is what
/// taffy does with the maximum still sitting beside them.
fn written_with_maximum(
    ratio: Option<f32>,
    max_width: Option<f32>,
) -> (f32, f32) {
    let mut tree: TaffyTree<()> = TaffyTree::new();
    let mut style = Style {
        flex_grow: 1.0,
        aspect_ratio: ratio,
        size: Size {
            width: Dimension::length(100.0),
            height: Dimension::length(248.0),
        },
        ..Default::default()
    };
    if let Some(limit) = max_width {
        style.max_size.width = LengthPercentageAuto::length(limit);
    }
    let child = tree
        .new_leaf(style)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let column = tree
        .new_with_children(
            Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                size: Size {
                    width: Dimension::length(424.0),
                    height: Dimension::length(248.0),
                },
                ..Default::default()
            },
            &[child],
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    tree.compute_layout(
        column,
        Size {
            width: AvailableSpace::Definite(424.0),
            height: AvailableSpace::Definite(248.0),
        },
    )
    .unwrap_or_else(|error| unreachable!("{error}"));
    let solved = tree
        .layout(child)
        .unwrap_or_else(|error| unreachable!("{error}"));
    (solved.size.width, solved.size.height)
}

/// taffy transfers a cross-axis maximum through the ratio and clamps the main
/// size with it, where Chrome clamps only the axis the maximum was written on.
#[test]
fn a_cross_maximum_transfers_into_the_main_size() {
    let (width, height) = written_with_maximum(Some(1.0), Some(100.0));
    assert!(
        (width - 100.0).abs() < 0.01 && (height - 100.0).abs() < 0.01,
        "taffy gives {width} x {height} for a written 100x248 at ratio 1 under \
         `max-width: 100px`, where it gave 100 x 100. Chrome gives 100 x 248: \
         the maximum clamps the axis it is written on and the main size keeps \
         the line's. If this is now 100 x 248 the transfer is gone and the \
         `[WORKAROUND]` for `l7aromeo/meo-canvas#129` in \
         `crates/meo-canvas-core/src/layout.rs` can be deleted"
    );
}

// [FOUNDATION] the property the `l7aromeo/meo-canvas#129` compensation rests
// on: **with no maximum on the style, the main size the compensation writes is
// the main size taffy solves.** The repair is to apply the maximum itself and
// take it off the style, which buys nothing if a written main size does not
// hold on its own.
//
// It is the third row of the isolation that found the mechanism, and the one
// that separates *the maximum transfers* from *a definite cross drags the main
// down*: the same written pair with the ratio and no maximum keeps 248.
//
// If this stopped holding, the compensation would remove the maximum and still
// get the wrong main size, and every row of `flex-ratio-cross.tsv` would go red
// at once rather than silently -- which is the good case, and is why this row
// is cheap insurance rather than the only thing standing between us and a
// silent failure.
#[test]
fn a_written_main_size_holds_without_a_maximum() {
    let (width, height) = written_with_maximum(Some(1.0), None);
    assert!(
        (width - 100.0).abs() < 0.01 && (height - 248.0).abs() < 0.01,
        "taffy gives {width} x {height} for a written 100x248 at ratio 1 with \
         no maximum, where it gave 100 x 248. The compensation for \
         `l7aromeo/meo-canvas#129` writes a main size and removes the maximum so that nothing transfers into it; \
         if a written main size no longer holds, removing the maximum is not \
         what makes it hold and the compensation is resting on nothing"
    );
}
