//! What a percentage height resolves against when a ratio settles the parent.
//!
//! `l7aromeo/meo-canvas#91`: an in-flow `height: 100%` under a box sized by
//! `width` and `aspect-ratio` painted nothing, while the parent's own box was
//! right. The renderer decides definiteness itself and hands taffy the answer,
//! so the rule is ours -- and it knew a declared length and opposing insets and
//! nothing about ratios.
//!
//! # Why this reads ink
//!
//! The report is that nothing is drawn, and a solved rectangle can carry a
//! height no pixel ever receives. Every row renders the scene and measures the
//! painted band, which is the instrument the reporter used.
//!
//! # The third repair to one rule, so the controls are half the table
//!
//! `ratio-and-declared-height` is 60 rather than 141 because a declared height
//! wins outright, and `inflow-100-no-ratio` paints nothing because without a
//! ratio a content-sized parent settles nothing -- the same shape as
//! `inflow-percent-content-cb` in `absolute-percentage.tsv`, re-measured here
//! so the ratio is the only difference between it and the row above it. A
//! repair that made every ratio parent definite breaks the first; one that made
//! every parent definite breaks the second.

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

/// The ratio every case shares, and the numbers it produces.
///
/// `0.85` is width over height, so a 120-wide box is 141.17 tall. Written as
/// the fraction CSS writes rather than as its reciprocal, because the table is
/// generated from `aspect-ratio:.85` and a reader comparing the two should not
/// have to invert one of them.
const RATIO: f32 = 0.85;

/// The outer ratio in the nested rows, chosen to differ from [`RATIO`].
///
/// **Two different ratios, because one repeated cannot show an ordering.** With
/// the same value on both boxes a derived height and a derived width coincide,
/// and the nested rows would agree with each other whatever order Chrome
/// resolved them in.
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

/// A child asking for a fraction of its parent's height.
///
/// **No width, because the markup declares none.** A block-level child with an
/// `auto` inline size fills its containing block, and Chrome measures 120 for
/// these rows against the parent's 120. This stated `30` until the width column
/// was asserted, which made every scene built from it a different scene from
/// the one the browser measured -- invisible while only the height was
/// compared, because the parent's height in these shapes does not depend on the
/// child's width.
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

/// The width of the painted band in whole pixels, zero for nothing drawn.
///
/// The same walk as [`painted_height`] over the other axis. Written out rather
/// than folded into one function returning a pair, because every existing
/// caller wants the height alone and a pair would make the height's callers
/// read a tuple to ignore half of it.
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

/// One scene per row, written out rather than assembled by a helper.
///
/// Split in two because the rows ask two different questions. One set varies
/// the **child** under a parent that is always width-plus-ratio; the other
/// varies **how the parent is sized** and keeps the child at `height: 100%`.
/// A helper that assembled either would hide the thing being varied.
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

/// `ratio-with-taller-content`'s box, clipped three ways.
///
/// **Split from [`ratio_box_case`] because they vary the overflow rather than
/// what supplies the width.** All three are the same 100-wide box holding 300
/// of content, and only the clipping differs -- which is the whole of what
/// these rows are about.
fn clipped_case(scene: &mut Scene, case: &str) {
    // `ratio-with-taller-content`'s box, clipped. Clipping removes
    // CSS's automatic minimum, so Chrome reports the ratio's own
    // 117.64 rather than the content's 300 -- which is what taffy
    // already gives, and why the compensation must not fire here.
    //
    // `overflow: clip` was measured at 300 and has no row: it is the
    // one non-visible value establishing no scroll container, and
    // `Overflow` here cannot express it.
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
    // **The row that separates CSS's two minimums.** Clipping removes the
    // automatic one; an author's `min-height` survives it, and unlike the
    // automatic one it transfers back through the ratio into the inline axis --
    // Chrome gives `170 x 200` on a box whose containing block is 100 wide.
    // That asymmetry is what `l7aromeo/meo-canvas#104` reports upstream: taffy
    // has one slot and it behaves like the explicit kind.
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

/// The rows where the measured element is the ratio box itself.
///
/// **A third question, not a third spelling of the first two.** The other
/// groups ask what a percentage resolves against; these ask whether the ratio
/// derives a height at all when the box's width is an *outcome* of layout
/// rather than a declared length. Every ratio box in the other groups has a
/// width that is declared, a percentage, or shrink-to-fit, so none of them can
/// answer it.
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

/// A shrink-to-fit ratio box holding one child, measured on the box itself.
///
/// **One builder for seven rows, because here the shape is the constant and the
/// numbers are what vary.** The older groups write each scene out, since what
/// varies there is which box settles which axis and a helper would hide it.
/// These rows all ask the same question -- what does the box do when its
/// content or its floor beats the height the ratio derives -- so the ratio, the
/// child's height and the floor are the whole of the difference and writing
/// them seven times would bury three numbers in ninety lines.
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

/// The same shape with an author bound on the **inline** axis.
///
/// **Separate from [`shrink_ratio_box`] because it asks the other axis's
/// question.** That builder varies the child's height and a block-axis floor,
/// which is what decides whether the content beats the derived height. These
/// two rows vary what settles the width before any derivation happens, and
/// folding them in would put three block-axis arguments and two inline ones on
/// one signature with only one pair ever set.
///
/// The pair is measured together on purpose: a minimum and a maximum reach the
/// solved width by the same route and Chrome treats them differently, so a row
/// for one without the other would read as a rule about author bounds.
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
            // Two ratios, both shrink-to-fit, and Chrome resolves them in one
            // ordered pass rather than to a fixed point: the inner derives
            // 35.28 from its own 30 of content, the outer's ratio turns that
            // height into a width, and the inner then fills the wider box and
            // ends up taller than the outer that contains it.
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

/// The rows that vary what competes with the height the ratio derives.
///
/// **Split from [`ratio_box_case`] because they vary a different thing.** Those
/// rows vary what supplies the box's width -- a block container, a flex cross
/// axis, a grid track; these hold the width at fit-content and vary the term
/// that wins Chrome's maximum: the content's own height, or an author's floor.
/// `ratio-shrink-content-just-under` and `-just-over` straddle the crossing at
/// 34 and 36 against a derived 35.28, and the pair is why the boundary is
/// located rather than described.
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
            // **The control on the entanglement predicate.** The outer carries
            // a ratio and a declared width, so its inline size is not an
            // outcome and the inner's answer does not depend on a derivation
            // any clearing pass would remove. A predicate asking whether an
            // ancestor merely *has* a ratio would skip the inner; one asking
            // whether an ancestor's inline size is also an outcome does not.
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
            // **A definite width, which is what makes these the same family as
            // `ratio-with-taller-content` rather than the shrink-to-fit one.**
            // At a fit-content width the content's height wins and taffy
            // transfers it back into the inline axis, which is already Chrome's
            // answer -- so the shrink-to-fit spelling of these rows passes with
            // the compensation absent and pins nothing.
            //
            // **Nothing else in this table sits above ratio 3**, and at 3 the
            // derivation and the content are both 10, so sampling 0.85, 2 and 3
            // reports perfect agreement across a family that was wrong from 4
            // upward. That coincidence is why the earlier coverage found
            // nothing.
            //
            // The two differ in the kind of failure: at 10 the box came back a
            // third of its height, at 100 it came back absent. 1e30 behaves as
            // 100 does and is deliberately not a row -- it would pin nothing
            // extra and would suggest the family is about extreme values when
            // it starts at 4.
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

/// The rows where what varies is how the parent gets its height.
///
/// Named rather than matched on a prefix: `min-height-200-under-ratio` ends in
/// the same word as these and belongs with the other group, which a
/// `starts_with` split got wrong.
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
            // Neither axis states a length. The parent's width is
            // shrink-to-fit from the child's 30, the ratio derives the height
            // from that, and the percentage resolves against it -- which is
            // what says the rule is about the ratio rather than about a
            // declared width.
            // The column wrapper is Chrome's and is carried rather than
            // dropped: it is what makes the ratio box shrink-to-fit on both
            // axes. Without it the box is a flex item of the page root and
            // takes the page's height, which is a different scene.
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

/// Rows this renderer is known to disagree with.
///
/// **`ratio-with-no-definite-length` fails for a different reason than this
/// file is about, and the difference is measurable.** Painting the *parent*
/// rather than the child in that scene gives **10** -- its content height --
/// where Chrome gives 35.28. So the ratio is not deriving a height from a
/// shrink-to-fit width at all, and the percentage beneath it has nothing right
/// to resolve against. `to_taffy_style` passes the ratio through: `0.85` is
/// finite and positive, so it survives the filter and taffy has it.
///
/// **The cause is upstream, measured by driving taffy 0.14 directly** in a
/// scratch crate with none of this renderer in the path:
///
/// ```text
/// no-definite-length parent    30.0 x  10.0    Chrome 29.98 x 35.28
/// control, ratio removed       30.0 x  10.0    want   29.98 x 10
/// control, percentage width   100.0 x 118.0    Chrome 100.00 x 117.64
/// ```
///
/// **The first two rows together are the finding.** With the ratio and without
/// it taffy returns the identical box, so the ratio is not applied badly -- it
/// is not applied at all when the inline axis is shrink-to-fit. The third row
/// is what stops that being a claim about ratios in general: given a width
/// taffy sizes itself, it derives the height correctly, the 118 against 117.64
/// being taffy's whole-pixel rounding, which is also why 29.98 reads as 30.0
/// above. So the boundary is exact: **a ratio settles the block axis when the
/// inline axis is definite and is discarded when it is shrink-to-fit.**
///
/// **Not repaired by narrowing `ratio_settles_it`, deliberately.** It says
/// "there is a usable ratio" and does not ask about the width, so on this shape
/// it answers *definite* while taffy discards the ratio, and the percentage
/// resolves against a height taffy never produced. Excluding shrink-to-fit here
/// would make this renderer wrong in a second way to compensate for taffy being
/// wrong in the first, and the day taffy fixes it somebody has to find that
/// compensation and undo it.
///
/// **Upstream, and it does not come out when the fix in flight lands.** It is
/// `DioxusLabs/taffy#804`, reproduced here against `v0.14.0` and against
/// `main` -- eight commits ahead of the release and identical on this row.
/// `DioxusLabs/taffy#1179` is open and fixes **the other spelling** of that
/// issue, where a flex-grown item's cross size is not transferred: measured
/// on that branch, that case goes from 128x64 to 128x128 and matches Chrome.
///
/// **This row is not that case, and that pull request makes it worse.**
/// On the branch a shrink-to-fit parent goes from `30x10` -- the ratio ignored
/// -- to `9x10`, narrower than the child inside it, because the transfer runs
/// from the block axis to the inline one: holding a 30x10 child and varying
/// only the ratio gives 9, 20 and 5 for 0.85, 2.0 and 0.5, so the width is the
/// content *height* times the ratio. Released `0.13.0` gives `30x10` for every
/// ratio, so the branches introduce that direction rather than inheriting it.
///
/// **taffy's maintainer characterised it before we did, on 2026-09-09**, in
/// review on `src/compute/flexbox.rs:1817`: the condition there is "a
/// pragmatic workaround", and the correct fix is that an auto-width column
/// container's width should come from its items' max-content contributions
/// rather than from the line cross size. That is a larger change they are
/// deliberately not making in that pull request.
///
/// So **a release carrying `DioxusLabs/taffy#1179` does not retire this
/// entry**, and the next reader should not delete it on seeing one. What
/// retires it is a release in which a shrink-to-fit parent with a ratio
/// reports its content width -- which the rows above will say plainly,
/// because they fail in both directions.
///
/// **Tracked as `l7aromeo/meo-canvas#97`**, which carries the reproduction
/// against taffy alone -- none of this repository in the path -- and states the
/// boundary the two upstream reports do not state together: the ratio is
/// applied when the inline size is definite **before** the item is laid out, a
/// declared length or a percentage of a definite parent, and dropped when the
/// inline size is itself an **outcome** of layout, whether from shrink-to-fit
/// content or from flex-grow distribution. The upstream change that produces
/// such a release is section 9.9.2 of the flexbox specification, intrinsic
/// cross sizes for column containers, open as `DioxusLabs/taffy#351` since
/// 2023-02-04 with 9.9.1 landed and 9.9.2 not.
///
/// The rule this file tests is still pinned without the row --
/// `ratio-with-percentage-width` is the same claim with a width taffy does
/// size, and it agrees -- so this entry records a second defect rather than
/// excusing the first.
///
/// The list fails in **both** directions: a row named here that starts agreeing
/// fails and says to delete the entry, so this cannot outlive the divergence.
/// **`ratio-with-taller-content` is taffy's, measured rather than assumed**,
/// and it is a different defect from the row above it. A ratio box 100 wide
/// with a 300-tall child: the ratio implies 117.64, Chrome gives the box 300,
/// and this renderer gives 118. So Chrome treats a derived height as a **floor
/// that content can exceed** and taffy treats it as an override.
///
/// Which side it belongs to was settled by driving taffy with **the style
/// `to_taffy_style` produces for that node** rather than a hand-built one --
/// the distinction matters, because a reconstruction is a claim about the
/// translation as well as about taffy. That style carries `size` auto on both
/// axes, `min_size` auto on both, and `aspect_ratio: Some(0.85)`; taffy returns
/// `100 x 118` from it, so nothing of ours is clamping.
///
/// It is recorded rather than repaired because the repair is not the one this
/// file's other rows are about, and folding it in would make a change scoped to
/// a percentage rule into a change to what a ratio means. It is also the row
/// that refuses the obvious shape of any future compensation here: anything
/// writing a definite `size.height` derived from the width clamps this box from
/// 300 to 118, trading a broken shape for a working one. A compensation has to
/// say *at least this tall*.
///
/// None of the eleven rows this table carried before could see it, because not
/// one of them has content taller than its ratio implies.
/// **`ratio-shrink-issue-97` is the shape `l7aromeo/meo-canvas#97` reports**,
/// and it had no row here until it was added: the nearest was
/// `ratio-gt-one-shrink`, the same scene at ratio 2.0, while
/// `ratio-with-no-definite-length` carries a percentage-height child and
/// collapses instead of dropping a derivation.
///
/// **The issue's figure for this renderer does not reproduce.** It records
/// `30.00 x 10.00`; measured here the box is `9 x 10`, and 9 is
/// `round(10 x 0.85)`. That matters beyond this row: every ratio box in the
/// tree satisfies `width = round(height x ratio)`, so taffy transfers block to
/// inline and never inline to block on these shapes. The rows that agree with
/// Chrome agree because the two rules coincide wherever the height is
/// content-determined, which is why none of them can witness the direction.
///
/// **Five of these six are one defect, and the rows say which.** A ratio box
/// whose inline size is fit-content gets no derived block size from taffy, so
/// `ratio-with-no-definite-length`, `ratio-shrink-issue-97`,
/// `ratio-shrink-50-child`, `ratio-shrink-content-just-under` and
/// `ratio-gt-one-shrink` all report the
/// content's own height where Chrome reports the ratio's.
///
/// **What separates them from the rows that pass is which term wins a
/// maximum.** Chrome takes a ratio box's block size as the largest of the
/// height the ratio derives, the content's min-content height, and any author
/// `min-height`, and transfers that back through the ratio to the inline axis
/// when the winner is not the derivation. taffy performs the transfer back
/// correctly -- which is why `ratio-shrink-taller-content` at `300 x 255`,
/// `ratio-shrink-content-just-over` at `36 x 30.59`,
/// `ratio-gt-one-taller-content` at `40 x 80` and
/// `ratio-shrink-with-author-min-height` at `200 x 170` all agree here -- and
/// supplies nothing when the derivation is the term that should win.
///
/// `ratio-shrink-content-just-under` and `ratio-shrink-content-just-over`
/// bracket that boundary at 34 and 36 against a derived 35.28: the first
/// diverges and the second does not, so the two rows locate the defect at the
/// crossing rather than describing it.
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
            // **The width was measured, recorded and dropped here.** Every row
            // of this table has carried a Chrome width since it was written and
            // this parser stopped at the height, so the assertion below could
            // not have compared one. The column is data the check was narrower
            // than.
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
        // **A row Chrome measures as zero-tall has no ink to measure a width
        // from**, so the width is not compared there. `inflow-100-no-ratio` is
        // the case: Chrome reports `120 x 0`, a box that exists and paints
        // nothing, and this walk over painted pixels reports `0 x 0`. The row's
        // point is the zero height and it is asserted; skipping the width is
        // narrower than the data by one column on one row, said here rather
        // than left for the next reader to find as a silent pass.
        // **And a box wider than the page reports the page.** `painted_width`
        // walks painted pixels, so a box Chrome measures at 1000 on a 400-wide
        // sheet can only ink 400 -- the measurement is truncated by the surface
        // rather than disagreeing with the browser.
        // `ratio-far-above-one-vanishing` is the case, and its point is the
        // height: it came back `0 x 0` before the compensation, so the row is
        // about whether the box exists at all.
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
