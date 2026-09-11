//! A taffy defect we compensate for, pinned so that fixing it cannot pass
//! unnoticed.
//!
//! # What is wrong
//!
//! **A flex container builds its content-based main size from its items'
//! content contributions where CSS Flexbox builds it from their outer
//! hypothetical main sizes.** §9.2 Line Length Determination resolves an item's
//! flex base size from `flex-basis` when that is definite and clamps it by the
//! min and max on the main axis; with a definite basis and a definite minimum
//! the result owes nothing to the content. taffy asks the content anyway.
//!
//! ```text
//! a 600-wide row, a 300x200 sibling, and a stretch-sized column whose item
//! carries flex-basis: 0 and min-height: 0 around 1024 of content
//!
//!     taffy   row 1024      Chrome   row 200
//! ```
//!
//! # Why the assertions are of the wrong numbers
//!
//! A test asserting Chrome's values would fail, and a failing test cannot be
//! committed. So this pins what taffy actually does, with the right answer
//! beside it: **the day taffy builds that size from hypothetical main sizes,
//! this fails, and the failure is the notification** that
//! `collapse_definite_bases` in `layout.rs` can be deleted. Grep
//! `[WORKAROUND]` to find it.
//!
//! The same family is measured against Chrome from the other side, through the
//! renderer rather than through taffy alone, in
//! `crates/meo-canvas/tests/assets/chrome/flex-basis-collapse.tsv`.
//!
//! # Upstream, and what it is not
//!
//! **No upstream issue covers this and none is cited.**
//! `DioxusLabs/taffy#950` is percentages against a stretched item with an
//! indefinite basis; `DioxusLabs/taffy#733` is node sizing with flex and image
//! nodes. Neither is this step. A reference that does not cover the defect is
//! worse than an admitted gap, because a reader follows it and concludes the
//! thing is tracked.
//!
//! **`DioxusLabs/taffy#351`, §9.9 Intrinsic Sizes, is a different missing
//! step** and is why the inline-axis case is excluded from the compensation
//! rather than repaired by it.
//!
//! Reproduced against taffy 0.14.0.

/// The row's height for a column whose item carries the given pair.
fn row_height(
    basis: Option<f32>,
    min: Option<f32>,
    column: Option<f32>,
) -> f32 {
    let mut tree: taffy::TaffyTree<()> = taffy::TaffyTree::new();
    let content = tree
        .new_leaf(taffy::Style {
            size: taffy::Size {
                width: taffy::Dimension::length(100.0),
                height: taffy::Dimension::length(1024.0),
            },
            ..taffy::Style::default()
        })
        .unwrap_or_else(|error| unreachable!("{error}"));

    let item = tree
        .new_with_children(
            taffy::Style {
                flex_grow: 1.0,
                flex_basis: basis
                    .map_or(taffy::Dimension::auto(), taffy::Dimension::length),
                min_size: taffy::Size {
                    width: taffy::LengthPercentageAuto::auto(),
                    height: min.map_or(
                        taffy::LengthPercentageAuto::auto(),
                        taffy::LengthPercentageAuto::length,
                    ),
                },
                ..taffy::Style::default()
            },
            &[content],
        )
        .unwrap_or_else(|error| unreachable!("{error}"));

    let column = tree
        .new_with_children(
            taffy::Style {
                display: taffy::Display::Flex,
                flex_direction: taffy::FlexDirection::Column,
                flex_grow: 1.0,
                size: taffy::Size {
                    width: taffy::Dimension::auto(),
                    height: column.map_or(
                        taffy::Dimension::auto(),
                        taffy::Dimension::length,
                    ),
                },
                ..taffy::Style::default()
            },
            &[item],
        )
        .unwrap_or_else(|error| unreachable!("{error}"));

    let sibling = tree
        .new_leaf(taffy::Style {
            size: taffy::Size {
                width: taffy::Dimension::length(300.0),
                height: taffy::Dimension::length(200.0),
            },
            ..taffy::Style::default()
        })
        .unwrap_or_else(|error| unreachable!("{error}"));

    let root = tree
        .new_with_children(
            taffy::Style {
                display: taffy::Display::Flex,
                flex_direction: taffy::FlexDirection::Row,
                size: taffy::Size {
                    width: taffy::Dimension::length(600.0),
                    height: taffy::Dimension::auto(),
                },
                ..taffy::Style::default()
            },
            &[sibling, column],
        )
        .unwrap_or_else(|error| unreachable!("{error}"));

    tree.compute_layout(
        root,
        taffy::Size {
            width: taffy::AvailableSpace::Definite(600.0),
            height: taffy::AvailableSpace::MaxContent,
        },
    )
    .unwrap_or_else(|error| unreachable!("{error}"));
    tree.layout(root)
        .unwrap_or_else(|error| unreachable!("{error}"))
        .size
        .height
}

/// taffy asks the content where §9.2 says to ask the basis.
#[test]
fn a_definite_base_is_not_the_content() {
    assert!(
        (row_height(Some(0.0), Some(0.0), None) - 1024.0).abs() < f32::EPSILON,
        "taffy no longer reports the content's 1024 for a column whose item \
         carries a definite basis and a definite minimum. Chrome gives 200. If \
         it now builds the container's content-based main size from the items' \
         hypothetical main sizes, the defect is fixed upstream and \
         `collapse_definite_bases` in `crates/meo-canvas-core/src/layout.rs` \
         should be deleted along with this file"
    );
}

/// Neither property alone, which is why the compensation reads both.
#[test]
fn neither_half_of_the_pair_is_enough() {
    assert!(
        (row_height(Some(0.0), None, None) - 1024.0).abs() < f32::EPSILON,
        "a definite basis with an automatic minimum: §4.5 floors the item at \
         its content, so the content still decides and Chrome agrees at 1024"
    );
    assert!(
        (row_height(None, Some(0.0), None) - 1024.0).abs() < f32::EPSILON,
        "an automatic basis with a definite minimum: §9.2 takes the base from \
         the content, so the content still decides and Chrome agrees at 1024"
    );
}

/// The property the compensation rests on, and what a change here would cost.
///
/// **§9.7 Resolving Flexible Lengths distributes free space from the base**, so
/// writing the hypothetical main size onto the item as a definite `size` does
/// not freeze it: the item grows into its line exactly as it would have. That
/// is the whole reason `collapse_definite_bases` writes a size rather than a
/// maximum -- a maximum reaches the same container extent and pins the item at
/// zero, which no conformance row would catch, because every row in
/// `flex-basis-collapse.tsv` measures the container rather than the item.
///
/// If this stopped holding, the compensation would pin every item it touches at
/// its basis and the whole table would stay green.
// [FOUNDATION] taffy still grows a flex item from a written definite size,
// which is what makes writing the hypothetical main size a repair rather than a
// clamp.
#[test]
fn a_written_base_still_grows_into_its_line() {
    let mut tree: taffy::TaffyTree<()> = taffy::TaffyTree::new();
    let item = tree
        .new_leaf(taffy::Style {
            flex_grow: 1.0,
            size: taffy::Size {
                width: taffy::Dimension::auto(),
                height: taffy::Dimension::length(0.0),
            },
            ..taffy::Style::default()
        })
        .unwrap_or_else(|error| unreachable!("{error}"));
    let column = tree
        .new_with_children(
            taffy::Style {
                display: taffy::Display::Flex,
                flex_direction: taffy::FlexDirection::Column,
                size: taffy::Size {
                    width: taffy::Dimension::length(300.0),
                    height: taffy::Dimension::length(200.0),
                },
                ..taffy::Style::default()
            },
            &[item],
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    tree.compute_layout(
        column,
        taffy::Size {
            width: taffy::AvailableSpace::Definite(300.0),
            height: taffy::AvailableSpace::Definite(200.0),
        },
    )
    .unwrap_or_else(|error| unreachable!("{error}"));
    assert!(
        (tree
            .layout(item)
            .unwrap_or_else(|error| unreachable!("{error}"))
            .size
            .height
            - 200.0)
            .abs()
            < f32::EPSILON,
        "a flex item given a definite size of zero no longer grows into its \
         line. `collapse_definite_bases` writes exactly that size, so if this \
         is not 200 the compensation is pinning items at their basis and every \
         row of `flex-basis-collapse.tsv` is still green while the picture is \
         wrong"
    );
}
