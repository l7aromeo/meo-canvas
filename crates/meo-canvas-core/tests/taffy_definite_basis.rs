//! Pins an inherited taffy defect: a flex container sizes itself from its
//! items' content, not their hypothetical main sizes (§9.2), so a definite
//! basis and minimum give a 1024 row where Chrome gives 200. Asserts taffy's
//! numbers so a fix fails here; no upstream issue covers it.

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

/// A written definite size still grows into its line, since §9.7 grows from it.
// [FOUNDATION] taffy grows an item from a written definite size, so writing the
// hypothetical main size is a repair; a maximum would pin the item at zero. If
// it stopped holding, `collapse_definite_bases` would pin items at their basis
// and `flex-basis-collapse.tsv`, which measures containers, would stay green.
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
