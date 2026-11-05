//! Pins an inherited taffy defect, taffy 0.15.1 `src/compute/leaf.rs`: a
//! block-flow leaf with an `aspect_ratio` is floored at `width / ratio` after
//! its sizes apply, 333x167 for a declared 333x150 or under `max-height: 150`,
//! where Chrome gives 333x150. Asserts taffy's numbers, so a fix fails here and
//! the two `[WORKAROUND]`s in `layout.rs` can go. `l7aromeo/meo-canvas#189`.

use taffy::prelude::{
    AvailableSpace, Dimension, Display, LengthPercentageAuto, Size, Style,
    TaffyTree, length,
};

/// A width and a declared height below what the ratio derives from it.
const DECLARED: (f32, f32) = (333.0, 150.0);

/// The ratio, which derives 166.5 from that width.
const RATIO: f32 = 2.0;

/// The leaf's laid-out size, declared at [`DECLARED`], in a container of
/// `display` that declares the same size.
fn leaf(display: Display, ratio: Option<f32>) -> (f32, f32) {
    leaf_with(display, ratio, false)
}

/// As [`leaf`], with the height a maximum rather than declared when `capped`.
fn leaf_with(display: Display, ratio: Option<f32>, capped: bool) -> (f32, f32) {
    let mut tree: TaffyTree<()> = TaffyTree::new();
    let size = Size {
        width: length(DECLARED.0),
        height: length(DECLARED.1),
    };
    let leaf = tree
        .new_leaf(Style {
            size: if capped {
                Size {
                    width: length(DECLARED.0),
                    height: Dimension::auto(),
                }
            } else {
                size
            },
            max_size: if capped {
                Size {
                    width: LengthPercentageAuto::auto(),
                    height: LengthPercentageAuto::length(DECLARED.1),
                }
            } else {
                Size::auto()
            },
            aspect_ratio: ratio,
            item_is_replaced: true,
            ..Style::default()
        })
        .unwrap_or_else(|error| unreachable!("{error}"));
    let container = tree
        .new_with_children(
            Style {
                display,
                size,
                ..Style::default()
            },
            &[leaf],
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    tree.compute_layout(
        container,
        Size {
            width: AvailableSpace::Definite(DECLARED.0),
            height: AvailableSpace::Definite(DECLARED.1),
        },
    )
    .unwrap_or_else(|error| unreachable!("{error}"));
    let layout = tree
        .layout(leaf)
        .unwrap_or_else(|error| unreachable!("{error}"));
    (layout.size.width, layout.size.height)
}

#[test]
fn a_declared_height_is_floored_at_the_ratio_in_block_flow() {
    // 166.5, rounded. Chrome keeps the declared 150; when taffy does too, this
    // fails and `both_sizes_declared` in `layout.rs` can be deleted.
    assert_eq!(
        leaf(Display::Block, Some(RATIO)),
        (333.0, 167.0),
        "taffy keeps a declared height under a ratio now: delete the \
         [WORKAROUND] `both_sizes_declared` in layout.rs"
    );
}

#[test]
fn a_maximum_height_is_floored_at_the_ratio_in_block_flow() {
    // The same floor over a maximum: `clamp_ratio_heights` re-applies it.
    assert_eq!(
        leaf_with(Display::Block, Some(RATIO), true),
        (333.0, 167.0),
        "taffy keeps a maximum height under a ratio now: delete the \
         [WORKAROUND] `clamp_ratio_heights` in layout.rs"
    );
}

// [FOUNDATION] the property `both_sizes_declared` and `clamp_ratio_heights`
// rest on: without a ratio, taffy keeps both declared sizes in block flow. If
// it stopped, dropping the ratio would no longer give the declared size.
#[test]
fn without_a_ratio_both_declared_sizes_are_kept() {
    assert_eq!(leaf(Display::Block, None), DECLARED);
}

#[test]
fn a_flex_container_keeps_the_declared_height_with_the_ratio() {
    // The control: flex sizes the item itself, so the floor never reaches it.
    assert_eq!(leaf(Display::Flex, Some(RATIO)), DECLARED);
}
