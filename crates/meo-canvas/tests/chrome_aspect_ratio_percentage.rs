//! What a percentage height resolves against when a ratio settles the parent
//! (`l7aromeo/meo-canvas#91`). Each row renders and measures the painted band,
//! since the report was that nothing is drawn. `ratio-and-declared-height` and
//! `inflow-100-no-ratio` are the controls a too-broad repair breaks.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Scene, Size,
    node::{Node, NodeId, NodeKind},
    style::{
        Dimension, Length,
        layout::{
            Align, Display, FlexDirection, LayoutStyle, Overflow, PositionType,
            TrackSize,
        },
        paint::Color,
    },
};

/// The page, and the viewport the Chrome side states.
const PAGE: f32 = 400.0;

/// The measured box's colour. Nothing else on any page is this.
const INK: (u8, u8, u8) = (232, 40, 200);

/// Every other page's colour, so "not paper" is unambiguous.
const PAPER: (u8, u8, u8) = (0, 255, 0);

/// The ratio every case shares, width over height, so a 120-wide box is 141.17
/// tall. Written as CSS writes it, since the table is generated from
/// `aspect-ratio:.85`.
const RATIO: f32 = 0.85;

/// The outer ratio in the nested rows, different from [`RATIO`]: with one value
/// on both boxes, a derived height and width coincide and no ordering shows.
const OUTER_RATIO: f32 = 2.0;

fn boxed(layout: LayoutStyle) -> Node {
    let mut node = Node::new(NodeKind::Box);
    node.layout = layout;
    node
}

fn measured(layout: LayoutStyle) -> Node {
    let mut node = boxed(layout);
    node.paint.background_color = Color::rgb(INK.0, INK.1, INK.2);
    node
}

fn page() -> Scene {
    let mut scene = Scene::new(Size::new(PAGE, PAGE));
    scene.nodes[0].paint.background_color =
        Color::rgb(PAPER.0, PAPER.1, PAPER.2);
    scene.nodes[0].layout.align_items = Some(Align::FlexStart);
    scene
}

fn push(scene: &mut Scene, parent: NodeId, node: Node) -> NodeId {
    scene
        .push(parent, node)
        .unwrap_or_else(|error| unreachable!("{error}"))
}

/// A parent sized on one axis with the ratio deriving the other.
fn ratio_parent(size: (Dimension, Dimension)) -> LayoutStyle {
    LayoutStyle {
        size,
        aspect_ratio: Some(RATIO),
        align_items: Some(Align::FlexStart),
        ..LayoutStyle::default()
    }
}

/// A child asking for a fraction of its parent's height, with no width, as the
/// markup has none: a block-level child with `auto` width fills its containing
/// block, 120 in Chrome.
fn tall(fraction: f32) -> LayoutStyle {
    LayoutStyle {
        size: (Dimension::Auto, Dimension::Percent(fraction)),
        ..LayoutStyle::default()
    }
}

/// The same child where the markup **does** declare a width.
///
/// Four rows do, and in three of them the 30 is load-bearing: it is what makes
/// the parent shrink-to-fit. The two spellings are not interchangeable.
fn tall_and_wide(fraction: f32) -> LayoutStyle {
    LayoutStyle {
        size: (Dimension::Points(30.0), Dimension::Percent(fraction)),
        ..LayoutStyle::default()
    }
}

/// The width of the painted band in whole pixels, zero for nothing drawn:
/// [`painted_height`]'s walk over the other axis, kept apart since its callers
/// want the height alone.
fn painted_width(scene: &Scene) -> u32 {
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    let bytes = renderer
        .render_to_buffer(scene, ImageFormat::Raw, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"));

    let side = PAGE as u32;
    let mut left: Option<u32> = None;
    let mut right: Option<u32> = None;
    for x in 0..side {
        let inked = (0..side).any(|y| {
            let at = ((y * side + x) * 4) as usize;
            (bytes[at], bytes[at + 1], bytes[at + 2]) == INK
        });
        if inked {
            left = left.or(Some(x));
            right = Some(x);
        }
    }
    match (left, right) {
        (Some(first), Some(last)) => last - first + 1,
        _ => 0,
    }
}

/// The height of the painted band in whole pixels, zero for nothing drawn.
fn painted_height(scene: &Scene) -> u32 {
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    let bytes = renderer
        .render_to_buffer(scene, ImageFormat::Raw, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"));

    let side = PAGE as u32;
    let mut top: Option<u32> = None;
    let mut bottom: Option<u32> = None;
    for y in 0..side {
        let inked = (0..side).any(|x| {
            let at = ((y * side + x) * 4) as usize;
            (bytes[at], bytes[at + 1], bytes[at + 2]) == INK
        });
        if inked {
            top = top.or(Some(y));
            bottom = Some(y);
        }
    }
    match (top, bottom) {
        (Some(first), Some(last)) => last - first + 1,
        _ => 0,
    }
}

/// One scene per row, written out: one set varies the child under a
/// width-plus-ratio parent, the other how the parent is sized, and a helper
/// would hide what varies.
fn scene_for(case: &str) -> Scene {
    let mut scene = page();
    if RATIO_BOX.contains(&case) {
        ratio_box_case(&mut scene, case);
    } else if PARENT_SHAPE.contains(&case) {
        parent_shape_case(&mut scene, case);
    } else {
        child_percentage_case(&mut scene, case);
    }
    scene
}

/// The rows where the box clips, which is what removes CSS's automatic minimum.
const CLIPPED: &[&str] = &[
    "ratio-definite-clipped",
    "ratio-definite-overflow-scroll",
    "ratio-definite-overflow-auto",
    "ratio-definite-clipped-author-min",
];

/// `ratio-with-taller-content`'s box, clipped three ways: the same 100-wide box
/// holding 300 of content, varying only the overflow.
fn clipped_case(scene: &mut Scene, case: &str) {
    // Clipping removes CSS's automatic minimum, so Chrome reports the ratio's
    // 117.64 rather than the content's 300, as taffy does, and the compensation
    // must not fire. `overflow: clip` has no row: `Overflow` cannot express it.
    let overflow = if case == "ratio-definite-overflow-scroll"
        || case == "ratio-definite-overflow-auto"
    {
        Overflow::Scroll
    } else {
        Overflow::Hidden
    };
    let outer = push(
        scene,
        NodeId::ROOT,
        boxed(LayoutStyle {
            size: (Dimension::Points(100.0), Dimension::Auto),
            ..LayoutStyle::default()
        }),
    );
    let mut style = bare_ratio(RATIO);
    style.overflow = (overflow, overflow);
    // The row separating CSS's two minimums: clipping removes the automatic
    // one, but an author's `min-height` survives and transfers back through the
    // ratio -- Chrome gives `170 x 200` in a 100-wide block.
    if case == "ratio-definite-clipped-author-min" {
        style.min_size = (Dimension::Auto, Dimension::Points(200.0));
    }
    let parent = push(scene, outer, measured(style));
    push(
        scene,
        parent,
        boxed(LayoutStyle {
            size: (Dimension::Auto, Dimension::Points(300.0)),
            ..LayoutStyle::default()
        }),
    );
}

/// The rows measuring the ratio box itself: whether the ratio derives a height
/// at all when the width is an outcome of layout, which no row with a declared,
/// percentage or shrink-to-fit width can answer.
const RATIO_BOX: &[&str] = &[
    "block-auto-width-ratio",
    "nested-ratio-outer",
    "nested-ratio-inner",
    "column-flex-ratio-cross",
    "grid-item-ratio",
    "ratio-with-taller-content",
    "ratio-definite-clipped",
    "ratio-definite-overflow-scroll",
    "ratio-definite-overflow-auto",
    "ratio-definite-clipped-author-min",
    "ratio-shrink-with-author-min-height",
    "ratio-shrink-50-child",
    "ratio-shrink-taller-content",
    "ratio-shrink-issue-97",
    "ratio-shrink-min-width-binds",
    "ratio-shrink-min-width-slack",
    "ratio-shrink-min-width-just-under",
    "ratio-shrink-min-over-max",
    "ratio-shrink-max-width-binds",
    "ratio-under-definite-ratio-parent",
    "ratio-shrink-content-just-under",
    "ratio-shrink-content-just-over",
    "ratio-gt-one-shrink",
    "ratio-gt-one-taller-content",
    "ratio-far-above-one",
    "ratio-far-above-one-vanishing",
];

/// A shrink-to-fit ratio box holding one child, measured on the box. One
/// builder for seven rows, since the ratio, the child's height and the floor
/// are the only differences.
fn shrink_ratio_box(
    scene: &mut Scene,
    ratio: f32,
    child_height: f32,
    floor: Option<f32>,
) -> NodeId {
    let wrapper = push(
        scene,
        NodeId::ROOT,
        boxed(LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            align_items: Some(Align::FlexStart),
            ..LayoutStyle::default()
        }),
    );
    let mut style = bare_ratio(ratio);
    if let Some(floor) = floor {
        style.min_size = (Dimension::Auto, Dimension::Points(floor));
    }
    let parent = push(scene, wrapper, measured(style));
    push(
        scene,
        parent,
        boxed(LayoutStyle {
            size: (Dimension::Points(30.0), Dimension::Points(child_height)),
            ..LayoutStyle::default()
        }),
    );
    parent
}

/// The same shape with an author bound on the inline axis, which settles the
/// width before any derivation. These rows check the pin's minimum clause is
/// written right; `flex-ratio-cross.tsv` is what fails if it is deleted.
fn bounded_ratio_box(
    scene: &mut Scene,
    min_width: Option<f32>,
    max_width: Option<f32>,
) -> NodeId {
    let wrapper = push(
        scene,
        NodeId::ROOT,
        boxed(LayoutStyle {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            align_items: Some(Align::FlexStart),
            ..LayoutStyle::default()
        }),
    );
    let mut style = bare_ratio(RATIO);
    if let Some(value) = min_width {
        style.min_size = (Dimension::Points(value), Dimension::Auto);
    }
    if let Some(value) = max_width {
        style.max_size = (Dimension::Points(value), Dimension::Auto);
    }
    let parent = push(scene, wrapper, measured(style));
    push(
        scene,
        parent,
        boxed(LayoutStyle {
            size: (Dimension::Points(30.0), Dimension::Points(10.0)),
            ..LayoutStyle::default()
        }),
    );
    parent
}

/// A ratio box with no stated extent on either axis.
fn bare_ratio(ratio: f32) -> LayoutStyle {
    LayoutStyle {
        aspect_ratio: Some(ratio),
        ..LayoutStyle::default()
    }
}

/// The rows measuring the ratio box itself.
fn ratio_box_case(scene: &mut Scene, case: &str) {
    match case {
        "block-auto-width-ratio" => {
            // Block-level with no width of its own, so the width is the
            // containing block's and is an outcome rather than a declaration.
            let outer = push(
                scene,
                NodeId::ROOT,
                boxed(LayoutStyle {
                    size: (Dimension::Points(200.0), Dimension::Auto),
                    ..LayoutStyle::default()
                }),
            );
            push(scene, outer, measured(bare_ratio(RATIO)));
        }
        "nested-ratio-outer" | "nested-ratio-inner" => {
            // Two shrink-to-fit ratios resolved in one ordered pass: the inner
            // derives 35.28 from its 30 of content, the outer turns that height
            // into a width, and the inner fills it and ends taller than the
            // outer.
            let wrapper = push(
                scene,
                NodeId::ROOT,
                boxed(LayoutStyle {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    align_items: Some(Align::FlexStart),
                    ..LayoutStyle::default()
                }),
            );
            let outer_style = bare_ratio(OUTER_RATIO);
            let inner_style = bare_ratio(RATIO);
            let outer = if case == "nested-ratio-outer" {
                push(scene, wrapper, measured(outer_style))
            } else {
                push(scene, wrapper, boxed(outer_style))
            };
            let inner = if case == "nested-ratio-inner" {
                push(scene, outer, measured(inner_style))
            } else {
                push(scene, outer, boxed(inner_style))
            };
            push(
                scene,
                inner,
                boxed(LayoutStyle {
                    size: (Dimension::Points(30.0), Dimension::Points(10.0)),
                    ..LayoutStyle::default()
                }),
            );
        }
        "column-flex-ratio-cross" => {
            // A column flex container makes the width the cross axis, so the
            // item stretches to it and the width is again an outcome.
            let outer = push(
                scene,
                NodeId::ROOT,
                boxed(LayoutStyle {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    size: (Dimension::Points(200.0), Dimension::Auto),
                    ..LayoutStyle::default()
                }),
            );
            push(scene, outer, measured(bare_ratio(RATIO)));
        }
        "grid-item-ratio" => {
            // The width comes from the track, which is a third way of arriving
            // at one without declaring it.
            let outer = push(
                scene,
                NodeId::ROOT,
                boxed(LayoutStyle {
                    display: Display::Grid,
                    grid_template_columns: vec![TrackSize::Points(200.0)],
                    ..LayoutStyle::default()
                }),
            );
            push(scene, outer, measured(bare_ratio(RATIO)));
        }
        other if CLIPPED.contains(&other) => clipped_case(scene, other),
        "ratio-with-taller-content" => {
            // The row that says a derived height is a floor rather than an
            // override: the ratio implies 117.64 and the content is 300, and
            // Chrome gives the box 300.
            let outer = push(
                scene,
                NodeId::ROOT,
                boxed(LayoutStyle {
                    size: (Dimension::Points(100.0), Dimension::Auto),
                    ..LayoutStyle::default()
                }),
            );
            let parent = push(scene, outer, measured(bare_ratio(RATIO)));
            push(
                scene,
                parent,
                boxed(LayoutStyle {
                    size: (Dimension::Auto, Dimension::Points(300.0)),
                    ..LayoutStyle::default()
                }),
            );
        }
        other => shrink_family_case(scene, other),
    }
}

/// The rows varying what competes with the derived height: the content's own
/// height or an author's floor, at a fit-content width. `-just-under` and
/// `-just-over` straddle the crossing, 34 and 36 against a derived 35.28.
fn shrink_family_case(scene: &mut Scene, case: &str) {
    match case {
        "ratio-shrink-with-author-min-height" => {
            shrink_ratio_box(scene, RATIO, 10.0, Some(200.0));
        }
        "ratio-shrink-taller-content" => {
            shrink_ratio_box(scene, RATIO, 300.0, None);
        }
        "ratio-shrink-issue-97" => {
            shrink_ratio_box(scene, RATIO, 10.0, None);
        }
        "ratio-shrink-min-width-binds" => {
            bounded_ratio_box(scene, Some(100.0), None);
        }
        "ratio-shrink-min-width-slack" => {
            bounded_ratio_box(scene, Some(20.0), None);
        }
        "ratio-shrink-min-width-just-under" => {
            bounded_ratio_box(scene, Some(29.0), None);
        }
        "ratio-shrink-min-over-max" => {
            bounded_ratio_box(scene, Some(100.0), Some(20.0));
        }
        "ratio-shrink-max-width-binds" => {
            bounded_ratio_box(scene, None, Some(20.0));
        }
        "ratio-under-definite-ratio-parent" => {
            // The control on the entanglement predicate: the outer has a ratio
            // and a declared width, so its inline size is not an outcome and
            // the inner must still be compensated.
            let outer = push(
                scene,
                NodeId::ROOT,
                boxed(LayoutStyle {
                    size: (Dimension::Points(200.0), Dimension::Auto),
                    aspect_ratio: Some(OUTER_RATIO),
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    align_items: Some(Align::FlexStart),
                    ..LayoutStyle::default()
                }),
            );
            let parent = push(scene, outer, measured(bare_ratio(RATIO)));
            push(
                scene,
                parent,
                boxed(LayoutStyle {
                    size: (Dimension::Points(30.0), Dimension::Points(10.0)),
                    ..LayoutStyle::default()
                }),
            );
        }
        "ratio-shrink-content-just-under" => {
            shrink_ratio_box(scene, RATIO, 34.0, None);
        }
        "ratio-shrink-content-just-over" => {
            shrink_ratio_box(scene, RATIO, 36.0, None);
        }
        "ratio-gt-one-shrink" => {
            shrink_ratio_box(scene, OUTER_RATIO, 10.0, None);
        }
        "ratio-far-above-one" | "ratio-far-above-one-vanishing" => {
            // A definite width, so these rows sit with
            // `ratio-with-taller-content`: at fit-content taffy already gives
            // Chrome's answer and pins nothing. Ratio 3 is where derivation and
            // content are both 10; the family diverges from 4 upward.
            let ratio = if case == "ratio-far-above-one" {
                10.0
            } else {
                100.0
            };
            let outer = push(
                scene,
                NodeId::ROOT,
                boxed(LayoutStyle {
                    size: (Dimension::Points(30.0), Dimension::Auto),
                    ..LayoutStyle::default()
                }),
            );
            let parent = push(scene, outer, measured(bare_ratio(ratio)));
            push(
                scene,
                parent,
                boxed(LayoutStyle {
                    size: (Dimension::Auto, Dimension::Points(10.0)),
                    ..LayoutStyle::default()
                }),
            );
        }
        "ratio-gt-one-taller-content" => {
            shrink_ratio_box(scene, OUTER_RATIO, 40.0, None);
        }
        "ratio-shrink-50-child" => {
            // Half the height of a shrink-to-fit ratio parent. Without this a
            // compensation could satisfy `100%` by any means and the table
            // could not tell the difference.
            let wrapper = push(
                scene,
                NodeId::ROOT,
                boxed(LayoutStyle {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    align_items: Some(Align::FlexStart),
                    ..LayoutStyle::default()
                }),
            );
            let parent = push(scene, wrapper, boxed(bare_ratio(RATIO)));
            push(scene, parent, measured(tall_and_wide(0.5)));
        }
        other => unreachable!("no shrink-family scene for `{other}`"),
    }
}

/// The rows varying how the parent gets its height, named rather than matched
/// on a prefix: `min-height-200-under-ratio` ends in the same word and belongs
/// elsewhere.
const PARENT_SHAPE: &[&str] = &[
    "ratio-parent-box",
    "ratio-and-declared-height",
    "ratio-from-height",
    "ratio-with-no-definite-length",
    "ratio-with-percentage-width",
    "inflow-100-no-ratio",
];

/// The rows where the parent is width-plus-ratio and the child varies.
fn child_percentage_case(scene: &mut Scene, case: &str) {
    match case {
        "inflow-100-under-ratio" => {
            let parent = push(
                scene,
                NodeId::ROOT,
                boxed(ratio_parent((
                    Dimension::Points(120.0),
                    Dimension::Auto,
                ))),
            );
            push(scene, parent, measured(tall(1.0)));
        }
        "inflow-50-under-ratio" => {
            let parent = push(
                scene,
                NodeId::ROOT,
                boxed(ratio_parent((
                    Dimension::Points(120.0),
                    Dimension::Auto,
                ))),
            );
            push(scene, parent, measured(tall(0.5)));
        }
        "abs-100-under-ratio" => {
            let mut parent =
                ratio_parent((Dimension::Points(120.0), Dimension::Auto));
            parent.position_type = PositionType::Relative;
            let parent = push(scene, NodeId::ROOT, boxed(parent));
            let mut layout = tall_and_wide(1.0);
            layout.position_type = PositionType::Absolute;
            layout.inset.top = Some(Length::Points(0.0));
            push(scene, parent, measured(layout));
        }
        "min-height-200-under-ratio" => {
            let parent = push(
                scene,
                NodeId::ROOT,
                boxed(ratio_parent((
                    Dimension::Points(120.0),
                    Dimension::Auto,
                ))),
            );
            let mut layout = LayoutStyle {
                size: (Dimension::Auto, Dimension::Points(20.0)),
                ..LayoutStyle::default()
            };
            layout.min_size = (Dimension::Auto, Dimension::Percent(2.0));
            push(scene, parent, measured(layout));
        }
        "max-height-25-under-ratio" => {
            let parent = push(
                scene,
                NodeId::ROOT,
                boxed(ratio_parent((
                    Dimension::Points(120.0),
                    Dimension::Auto,
                ))),
            );
            let mut layout = LayoutStyle {
                size: (Dimension::Auto, Dimension::Points(300.0)),
                ..LayoutStyle::default()
            };
            layout.max_size = (Dimension::Auto, Dimension::Percent(0.25));
            push(scene, parent, measured(layout));
        }
        other => unreachable!("no child-percentage scene for `{other}`"),
    }
}

/// The rows where the parent's own sizing is what is under test.
fn parent_shape_case(scene: &mut Scene, case: &str) {
    match case {
        "ratio-parent-box" => {
            push(
                scene,
                NodeId::ROOT,
                measured(ratio_parent((
                    Dimension::Points(120.0),
                    Dimension::Auto,
                ))),
            );
        }
        "ratio-and-declared-height" => {
            let parent = push(
                scene,
                NodeId::ROOT,
                boxed(ratio_parent((
                    Dimension::Points(120.0),
                    Dimension::Points(60.0),
                ))),
            );
            push(scene, parent, measured(tall(1.0)));
        }
        "ratio-from-height" => {
            let parent = push(
                scene,
                NodeId::ROOT,
                boxed(ratio_parent((
                    Dimension::Auto,
                    Dimension::Points(120.0),
                ))),
            );
            push(scene, parent, measured(tall(1.0)));
        }
        "ratio-with-no-definite-length" => {
            // Neither axis states a length: the width is shrink-to-fit from the
            // child's 30 and the ratio derives the height. The column wrapper
            // is Chrome's, and without it the box takes the page's height.
            let wrapper = push(
                scene,
                NodeId::ROOT,
                boxed(LayoutStyle {
                    display: Display::Flex,
                    flex_direction: FlexDirection::Column,
                    align_items: Some(Align::FlexStart),
                    ..LayoutStyle::default()
                }),
            );
            let parent = push(
                scene,
                wrapper,
                boxed(ratio_parent((Dimension::Auto, Dimension::Auto))),
            );
            push(scene, parent, measured(tall_and_wide(1.0)));
        }
        "ratio-with-percentage-width" => {
            let outer = push(
                scene,
                NodeId::ROOT,
                boxed(LayoutStyle {
                    size: (Dimension::Points(200.0), Dimension::Points(50.0)),
                    align_items: Some(Align::FlexStart),
                    ..LayoutStyle::default()
                }),
            );
            let parent = push(
                scene,
                outer,
                boxed(ratio_parent((Dimension::Percent(0.5), Dimension::Auto))),
            );
            push(scene, parent, measured(tall_and_wide(1.0)));
        }
        "inflow-100-no-ratio" => {
            let parent = push(
                scene,
                NodeId::ROOT,
                boxed(LayoutStyle {
                    size: (Dimension::Points(120.0), Dimension::Auto),
                    align_items: Some(Align::FlexStart),
                    ..LayoutStyle::default()
                }),
            );
            push(scene, parent, measured(tall(1.0)));
        }
        other => unreachable!("no parent-shape scene for `{other}`"),
    }
}

/// Chrome's answers, as measured.
const TABLE: &str = include_str!("assets/chrome/aspect-ratio-percentage.tsv");

/// Rows this renderer is known to disagree with: empty. A named row that starts
/// agreeing fails and says to delete its entry. The upstream defect behind the
/// ratio rows is `DioxusLabs/taffy#804`, tracked as `l7aromeo/meo-canvas#97`.
const KNOWN: &[&str] = &[];

/// One row: the case's key and the height Chrome gave it.
fn rows() -> Vec<(String, f32, f32)> {
    TABLE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let mut columns = line.split('\t');
            let key = columns
                .next()
                .unwrap_or_else(|| unreachable!("a row with no case"));
            let height = columns
                .next()
                .unwrap_or_else(|| unreachable!("{key} has no height"))
                .parse::<f32>()
                .unwrap_or_else(|error| unreachable!("{key}: {error}"));
            // The width Chrome measured, compared here as well as the height.
            let width = columns
                .next()
                .unwrap_or_else(|| unreachable!("{key} has no width"))
                .parse::<f32>()
                .unwrap_or_else(|error| unreachable!("{key}: {error}"));
            (key.to_owned(), height, width)
        })
        .collect()
}

#[test]
fn every_row_paints_the_band_chrome_measured() {
    let rows = rows();
    assert_eq!(
        rows.len(),
        37,
        "the table changed shape; the scenes here are per row"
    );

    // Every row is measured before anything is asserted: a per-row assertion
    // stops at the first disagreement, and the question a repair to this rule
    // raises is whether it moved a row somewhere else in the table.
    let mut failing = Vec::new();
    let mut stale = Vec::new();
    for (key, chrome, chrome_wide) in &rows {
        let painted = painted_height(&scene_for(key));
        let painted_wide = painted_width(&scene_for(key));
        // Within one pixel, and stated rather than assumed: Chrome reports a
        // fractional used height and this counts whole rows of pixels, so
        // 141.17 and 141 are the same answer. The rows this exists for are
        // separated by a hundred pixels, not one.
        let agrees = (f32::from(u16::try_from(painted).unwrap_or(u16::MAX))
            - chrome)
            .abs()
            <= 1.0;
        // The width is not compared where Chrome measures zero height --
        // `inflow-100-no-ratio` has no ink to measure -- or where the box is
        // wider than the page, which painted pixels truncate to 400.
        let off_page = *chrome_wide > PAGE;
        let wide_agrees = *chrome < 1.0
            || off_page
            || (f32::from(u16::try_from(painted_wide).unwrap_or(u16::MAX))
                - chrome_wide)
                .abs()
                <= 1.0;
        let agrees = agrees && wide_agrees;
        if !agrees && !KNOWN.contains(&key.as_str()) {
            failing.push(format!(
                "{key}: chrome {chrome_wide:.2} x {chrome:.2}, here \
                 {painted_wide} x {painted}"
            ));
        }
        if agrees && KNOWN.contains(&key.as_str()) {
            stale.push(key.clone());
        }
    }
    assert!(
        failing.is_empty(),
        "{} row(s) disagree: {failing:#?}",
        failing.len()
    );
    assert!(
        stale.is_empty(),
        "these rows agree and are still in KNOWN; delete them: {stale:?}"
    );
}
