//! Pins an inherited taffy defect: a ratio never transfers between axes when
//! the main size comes from `flex-grow` -- centred, `0 x 248` where Chrome
//! gives `248 x 248`; stretched, `424 x 248` against `424 x 424`. Asserts
//! taffy's numbers so a fix fails here. `DioxusLabs/taffy#804`.

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

// [FOUNDATION] the property `stretched_ratio_minimum` rests on: the cross size
// it divides by the ratio is read unrounded. If it stopped holding, the minimum
// would be up to half a pixel off and `ratio-stretch-main.tsv`, integers
// compared within a pixel, would stay green.
#[test]
fn a_stretched_cross_size_is_reported_unrounded() {
    let (width, height) = fractional_stretched_item();
    assert!(
        (width - 424.4).abs() < 0.01,
        "a stretched item in a 424.4-wide content box is {width} wide. The          transferred minimum is this number divided by the ratio, so a          rounded 424 here is a minimum wrong by 0.4 that no conformance row          can see"
    );
    assert!(
        (height - 248.0).abs() < 0.01,
        "the main axis is {height} rather than the line's 248, so this is no          longer measuring what the compensation reads"
    );
}

/// A stretched, ratio-free item in a container whose inner cross size has a
/// fractional part, solved with rounding off the way `solve_page` solves.
fn fractional_stretched_item() -> (f32, f32) {
    let mut tree: TaffyTree<()> = TaffyTree::new();
    tree.disable_rounding();
    let child = tree
        .new_leaf(Style {
            flex_grow: 1.0,
            ..Style::DEFAULT
        })
        .unwrap_or_else(|error| unreachable!("{error}"));
    let parent = tree
        .new_with_children(
            Style {
                display: Display::Flex,
                flex_direction: FlexDirection::Column,
                align_items: Some(AlignItems::STRETCH),
                size: Size {
                    width: Dimension::length(440.4),
                    height: Dimension::length(264.0),
                },
                box_sizing: BoxSizing::BorderBox,
                padding: Rect::length(8.0),
                ..Style::DEFAULT
            },
            &[child],
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    tree.compute_layout(parent, Size::MAX_CONTENT)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let solved = tree
        .layout(child)
        .unwrap_or_else(|error| unreachable!("{error}"));
    (solved.size.width, solved.size.height)
}

// [FOUNDATION] the property `compensate_ratio_direction`'s third arm rests on:
// taffy grows the main size to Chrome's number (the ratio-less `0 x 248` row),
// which the arm multiplies by the ratio. If growth stopped matching, derived
// widths would be wrong and no conformance row would say why.
#[test]
fn the_rows_taffy_gets_right_agree_with_chrome() {
    // Controls, each removing a suspect: a percentage main size reaches the
    // ratio; an automatic height has nothing to grow into; a ratio-less row
    // grows correctly. Chrome's numbers, on the same six shapes.
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

/// A grown item's solved size with a written size and optional cross maximum,
/// in a `424x248` column: the state `compensate_ratio_direction` leaves a node
/// in before its second solve.
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
// on: with no maximum on the style, the main size it writes is the one taffy
// solves. If not, removing the maximum would not help, and
// `flex-ratio-cross.tsv` would go red at once.
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
