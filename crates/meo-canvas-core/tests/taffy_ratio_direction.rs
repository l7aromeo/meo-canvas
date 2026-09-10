//! A taffy defect we compensate for, pinned so that fixing it cannot pass
//! unnoticed.
//!
//! # What is wrong
//!
//! **A ratio box whose inline size is an outcome of layout has its axes
//! resolved in the wrong order.** taffy derives the *width* from the block size
//! rather than deriving the block size from a fit-content width, so every ratio
//! box in a solved tree satisfies `width = round(height x ratio)`.
//!
//! ```text
//! ratio 0.85, a 30x10 child, shrink-to-fit parent   taffy 9 x 10    Chrome 29.98 x 35.28
//! ratio 2.0,  the same child                        taffy 20 x 10   Chrome 30 x 15
//! ```
//!
//! 9 is `round(10 x 0.85)` and 20 is `10 x 2.0`. Chrome takes the inline size
//! as fit-content and derives the block size from it, transferring back to the
//! inline axis only when some other term -- the content's own height, or an
//! author `min-height` -- wins the block size instead.
//!
//! # Why the assertions are of the wrong numbers
//!
//! A test asserting Chrome's values would fail, and a failing test cannot be
//! committed. So this pins what taffy actually does, with the right answer
//! beside it: **the day taffy is fixed, this fails, and the failure is the
//! notification** that the compensation in `layout.rs` can be deleted. Grep
//! `[WORKAROUND]` to find it.
//!
//! The same shape is measured against Chrome from the other side, through the
//! renderer rather than through taffy alone, in
//! `crates/meo-canvas/tests/assets/chrome/aspect-ratio-percentage.tsv` --
//! `ratio-shrink-issue-97` and the rows beside it.
//!
//! # The control
//!
//! [`clearing_the_ratio_restores_the_fit_content_width`] is why the
//! compensation is possible at all: taffy computes the inline size correctly
//! when the ratio is absent. Without that row this file would say the defect
//! exists and nothing about whether it can be worked around.
//!
//! Upstream: `DioxusLabs/taffy#804`, tracked as `l7aromeo/meo-canvas#97`.
//! Reproduced against taffy 0.14.0.

use meo_canvas_core::layout::to_taffy_style;
use meo_canvas_scene::style::{layout::LayoutStyle, paint::BorderStyle};

/// The ratio every row here uses, matching the conformance table's.
const RATIO: f32 = 0.85;

/// Solves a shrink-to-fit box holding one 30x10 child and reports its size.
///
/// Built from `to_taffy_style`'s own output rather than a hand-written
/// `taffy::Style`, because a reconstruction would be a claim about the
/// translation as well as about taffy.
fn shrink_box(ratio: Option<f32>) -> (f32, f32) {
    let style = to_taffy_style(
        &LayoutStyle {
            aspect_ratio: ratio,
            ..LayoutStyle::default()
        },
        BorderStyle::Solid,
    );
    let mut tree: taffy::TaffyTree<()> = taffy::TaffyTree::new();
    let child = tree
        .new_leaf(taffy::Style {
            size: taffy::Size {
                width: taffy::Dimension::length(30.0),
                height: taffy::Dimension::length(10.0),
            },
            ..taffy::Style::default()
        })
        .unwrap_or_else(|error| unreachable!("{error}"));
    let box_node = tree
        .new_with_children(style, &[child])
        .unwrap_or_else(|error| unreachable!("{error}"));
    let column = tree
        .new_with_children(
            taffy::Style {
                display: taffy::Display::Flex,
                flex_direction: taffy::FlexDirection::Column,
                align_items: Some(taffy::AlignItems::FLEX_START),
                ..taffy::Style::default()
            },
            &[box_node],
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    tree.compute_layout(
        column,
        taffy::Size {
            width: taffy::AvailableSpace::Definite(400.0),
            height: taffy::AvailableSpace::Definite(400.0),
        },
    )
    .unwrap_or_else(|error| unreachable!("{error}"));
    let solved = tree
        .layout(box_node)
        .unwrap_or_else(|error| unreachable!("{error}"));
    (solved.size.width, solved.size.height)
}

/// taffy derives the width from the height, where Chrome does the reverse.
#[test]
fn a_ratio_box_derives_its_width_from_its_height() {
    let (width, height) = shrink_box(Some(RATIO));
    assert_eq!(
        (width, height),
        (9.0, 10.0),
        "taffy no longer reports 9x10 for a shrink-to-fit ratio box with a \
         30x10 child. Chrome gives 29.98 x 35.28. If this now derives the \
         height from a fit-content width, the defect is fixed upstream and the \
         `[WORKAROUND]` in `crates/meo-canvas-core/src/layout.rs` should be \
         deleted along with this file"
    );
}

/// The same at a ratio above one, so the defect is not a property of the side
/// of one the ratio sits on.
#[test]
fn the_same_holds_for_a_ratio_above_one() {
    let (width, height) = shrink_box(Some(2.0));
    assert_eq!(
        (width, height),
        (20.0, 10.0),
        "20 is 10 x 2.0, the same transfer as the row above and not an \
         artefact of a ratio below one. Chrome gives 30 x 15"
    );
}

/// Without the ratio the inline size is fit-content, which is what the
/// compensation asks taffy for.
#[test]
fn clearing_the_ratio_restores_the_fit_content_width() {
    let (width, height) = shrink_box(None);
    assert_eq!(
        (width, height),
        (30.0, 10.0),
        "taffy no longer returns the child's own 30 as the box's fit-content \
         width when no ratio is present. The compensation reads this number \
         and derives the block size from it, so if this row moves the \
         compensation is measuring something else"
    );
}

/// The second workaround's defect: a content-derived floor is transferred.
///
/// **CSS has two minimums and taffy has one slot.** An automatic minimum block
/// size, taken from the content, does **not** transfer back into the inline
/// axis; an author's `min-height` does. taffy's `min_size.height` behaves like
/// the second, so a content-derived floor written there takes a 100-wide box to
/// 255 -- Chrome keeps it at 100 and makes it 300 tall.
///
/// This is why `l7aromeo/meo-canvas#104` is compensated by clearing the ratio
/// rather than by writing a floor. The day taffy distinguishes the two, this
/// fails and the compensation can go.
#[test]
fn a_content_derived_floor_is_transferred_into_the_width() {
    let mut tree: taffy::TaffyTree<()> = taffy::TaffyTree::new();
    let child = tree
        .new_leaf(taffy::Style {
            size: taffy::Size {
                width: taffy::Dimension::auto(),
                height: taffy::Dimension::length(300.0),
            },
            ..taffy::Style::default()
        })
        .unwrap_or_else(|error| unreachable!("{error}"));
    let mut style = to_taffy_style(
        &LayoutStyle {
            aspect_ratio: Some(RATIO),
            ..LayoutStyle::default()
        },
        BorderStyle::Solid,
    );
    style.min_size.height = taffy::LengthPercentageAuto::length(300.0);
    let box_node = tree
        .new_with_children(style, &[child])
        .unwrap_or_else(|error| unreachable!("{error}"));
    let outer = tree
        .new_with_children(
            taffy::Style {
                display: taffy::Display::Block,
                size: taffy::Size {
                    width: taffy::Dimension::length(100.0),
                    height: taffy::Dimension::auto(),
                },
                ..taffy::Style::default()
            },
            &[box_node],
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    tree.compute_layout(
        outer,
        taffy::Size {
            width: taffy::AvailableSpace::Definite(400.0),
            height: taffy::AvailableSpace::Definite(400.0),
        },
    )
    .unwrap_or_else(|error| unreachable!("{error}"));
    let solved = tree
        .layout(box_node)
        .unwrap_or_else(|error| unreachable!("{error}"));
    assert_eq!(
        (solved.size.width, solved.size.height),
        (255.0, 300.0),
        "taffy no longer transfers a `min_size.height` into the width at a \
         definite width. Chrome gives 100 x 300. If it now distinguishes an \
         automatic minimum from an author's, `floor_ratio_heights` in \
         `crates/meo-canvas-core/src/layout.rs` can be deleted"
    );
}

/// The property the second workaround depends on, as the first has its own.
///
/// A ratio-cleared solve of a definite-width block box reports the content's
/// height at that width -- which is the quantity Chrome floors with, measured:
/// text 100 wide is 120 tall with a ratio and 120 without, against 160 at its
/// min-content width. If this stopped holding, the compensation would floor
/// with the wrong number and every conformance row would stay green.
#[test]
fn clearing_the_ratio_reports_the_content_height_at_that_width() {
    let mut tree: taffy::TaffyTree<()> = taffy::TaffyTree::new();
    let child = tree
        .new_leaf(taffy::Style {
            size: taffy::Size {
                width: taffy::Dimension::auto(),
                height: taffy::Dimension::length(300.0),
            },
            ..taffy::Style::default()
        })
        .unwrap_or_else(|error| unreachable!("{error}"));
    let style = to_taffy_style(&LayoutStyle::default(), BorderStyle::Solid);
    let box_node = tree
        .new_with_children(style, &[child])
        .unwrap_or_else(|error| unreachable!("{error}"));
    let outer = tree
        .new_with_children(
            taffy::Style {
                display: taffy::Display::Block,
                size: taffy::Size {
                    width: taffy::Dimension::length(100.0),
                    height: taffy::Dimension::auto(),
                },
                ..taffy::Style::default()
            },
            &[box_node],
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    tree.compute_layout(
        outer,
        taffy::Size {
            width: taffy::AvailableSpace::Definite(400.0),
            height: taffy::AvailableSpace::Definite(400.0),
        },
    )
    .unwrap_or_else(|error| unreachable!("{error}"));
    let solved = tree
        .layout(box_node)
        .unwrap_or_else(|error| unreachable!("{error}"));
    assert_eq!(
        (solved.size.width, solved.size.height),
        (100.0, 300.0),
        "a ratio-free box no longer reports its containing block's width and \
         its content's height. `floor_ratio_heights` reads both of these and \
         would floor with the wrong number if either moved"
    );
}
