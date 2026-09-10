//! What a percentage height resolves against when the box is out of flow.
//!
//! `l7aromeo/meo-canvas#84`: a percentage height on an absolutely positioned
//! box came out zero and painted nothing, while the same third written as
//! `top`/`bottom` came out right. The renderer decides definiteness itself and
//! hands taffy the answer, so the rule is ours and taffy never sees the
//! question.
//!
//! # Why this reads ink
//!
//! The report is that nothing is drawn. A `LayoutResult` can carry a height
//! that no pixel ever receives -- a clip, a zero-size ancestor, a paint order
//! -- so an assertion on the solved rectangle would pass for a page that came
//! out blank. Every row here renders the scene and measures the painted band,
//! which is the same instrument the reporter used.
//!
//! # The controls are most of the value
//!
//! This is a fix to a fix. `flex_settles_it` already refuses out-of-flow boxes
//! on the `Auto` arm, with a measured Chrome number behind it, and the arm
//! that is wrong is next to it. Six of the fourteen rows must not move, and
//! they are the ones that make the other eight mean something: a repair that
//! made every out-of-flow box definite everywhere would turn
//! `abs-minheight-200` from 20 into 40 and `abs-top-only-child` from nothing
//! into a full-height band, and a table without them would call that a success.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Scene, Size,
    node::{Node, NodeId, NodeKind},
    style::{
        Dimension, Length,
        effect::Transform,
        layout::{Align, Display, FlexDirection, LayoutStyle, PositionType},
        paint::Color,
    },
};

/// The page, and the viewport the Chrome side states.
const PAGE: f32 = 400.0;

/// The measured box's colour. Nothing else on any page is this.
const INK: (u8, u8, u8) = (232, 40, 200);

/// Every other page's colour, so "not paper" is unambiguous.
const PAPER: (u8, u8, u8) = (0, 255, 0);

/// A box with a layout and no paint.
fn boxed(layout: LayoutStyle) -> Node {
    let mut node = Node::new(NodeKind::Box);
    node.layout = layout;
    node
}

/// The box each row measures, painted so the scan can find it.
fn measured(layout: LayoutStyle) -> Node {
    let mut node = boxed(layout);
    node.paint.background_color = Color::rgb(INK.0, INK.1, INK.2);
    node
}

/// A column that lays its children out at their own width.
///
/// `align-items: flex-start` on the Chrome side too, and for the same reason
/// the existing definiteness test gives: a stretched flex item has a definite
/// cross size, so without it a row would be measuring the page rather than the
/// ancestor it names.
fn column() -> LayoutStyle {
    LayoutStyle {
        display: Display::Flex,
        flex_direction: FlexDirection::Column,
        align_items: Some(Align::FlexStart),
        ..LayoutStyle::default()
    }
}

/// A fixed-size box, which is how every row states a height it means literally.
fn sized(width: f32, height: f32) -> LayoutStyle {
    LayoutStyle {
        size: (Dimension::Points(width), Dimension::Points(height)),
        ..LayoutStyle::default()
    }
}

/// The height of the painted band, in whole pixels, and zero for nothing drawn.
///
/// Zero rather than `None`: the rows this exists for report *nothing painted*,
/// and that is a height rather than an absence of one.
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
        let mut inked = false;
        for x in 0..side {
            let at = ((y * side + x) * 4) as usize;
            if (bytes[at], bytes[at + 1], bytes[at + 2]) == INK {
                inked = true;
                break;
            }
        }
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

/// A page whose paper is not the ink.
fn page() -> Scene {
    let mut scene = Scene::new(Size::new(PAGE, PAGE));
    scene.nodes[0].paint.background_color =
        Color::rgb(PAPER.0, PAPER.1, PAPER.2);
    scene.nodes[0].layout.align_items = Some(Align::FlexStart);
    scene
}

/// Pushes a node and returns its id, failing loudly rather than silently.
fn push(scene: &mut Scene, parent: NodeId, node: Node) -> NodeId {
    scene
        .push(parent, node)
        .unwrap_or_else(|error| unreachable!("{error}"))
}

/// A percentage height, as CSS writes it.
const fn percent(fraction: f32) -> Dimension {
    Dimension::Percent(fraction)
}

/// A child asking for its containing block's whole height.
fn full_height() -> LayoutStyle {
    LayoutStyle {
        size: (Dimension::Points(30.0), percent(1.0)),
        ..LayoutStyle::default()
    }
}

/// An out-of-flow layout with the given position and top inset.
fn out_of_flow(position: PositionType, top: Length) -> LayoutStyle {
    let mut layout = LayoutStyle {
        position_type: position,
        ..LayoutStyle::default()
    };
    layout.inset.top = Some(top);
    layout
}

/// One scene per row, written out rather than assembled by a helper.
///
/// The question every row asks is *which box is the containing block*, so a
/// builder that composed the ancestor chain would hide the only thing being
/// varied.
fn scene_for(case: &str) -> Scene {
    let mut scene = page();
    if case.starts_with("fixed-") {
        fixed_case(&mut scene, case);
    } else {
        absolute_case(&mut scene, case);
    }
    scene
}

/// The rows whose box is `position: absolute`, plus the two in-flow controls.
fn absolute_case(scene: &mut Scene, case: &str) {
    if case.starts_with("abs-percent-") {
        percent_height_case(scene, case);
    } else {
        inset_and_control_case(scene, case);
    }
}

/// The four rows where the percentage is on the out-of-flow box itself.
///
/// They vary only in what establishes the containing block, which is the
/// whole question: a content-sized relative ancestor, one that states its
/// height, one that is itself an auto-height absolute box, and one that is a
/// grandparent with a static box in between.
fn percent_height_case(scene: &mut Scene, case: &str) {
    match case {
        "abs-percent-content-cb" => {
            let cb = push(
                scene,
                NodeId::ROOT,
                boxed(LayoutStyle {
                    position_type: PositionType::Relative,
                    ..column()
                }),
            );
            push(scene, cb, boxed(sized(50.0, 120.0)));
            let mut layout =
                out_of_flow(PositionType::Absolute, Length::Points(0.0));
            layout.size = (Dimension::Points(30.0), percent(0.3333));
            push(scene, cb, measured(layout));
        }
        "abs-percent-declared-cb" => {
            let mut cb = sized(200.0, 120.0);
            cb.position_type = PositionType::Relative;
            let cb = push(scene, NodeId::ROOT, boxed(cb));
            let mut layout =
                out_of_flow(PositionType::Absolute, Length::Points(0.0));
            layout.size = (Dimension::Points(30.0), percent(0.3333));
            push(scene, cb, measured(layout));
        }
        "abs-percent-grandparent-sized" => {
            let cb = push(
                scene,
                NodeId::ROOT,
                boxed(LayoutStyle {
                    position_type: PositionType::Relative,
                    ..column()
                }),
            );
            push(scene, cb, boxed(sized(50.0, 120.0)));
            // **Sixty, which makes the parent definite as well as visible.**
            // That is why this cannot be the row above: with a definite parent
            // the percentage survives whatever the containing-block rule says,
            // so this row is blind to *whether* and sharp about *which* --
            // 59.98 against the grandparent's 180 of content, 20 against this
            // box, and the two are forty pixels apart.
            let between = push(
                scene,
                cb,
                boxed(LayoutStyle {
                    size: (Dimension::Points(70.0), Dimension::Points(60.0)),
                    ..LayoutStyle::default()
                }),
            );
            let mut layout =
                out_of_flow(PositionType::Absolute, Length::Points(0.0));
            layout.size = (Dimension::Points(30.0), percent(0.3333));
            push(scene, between, measured(layout));
        }
        "abs-percent-abs-cb" => {
            let mut outer = sized(200.0, 300.0);
            outer.position_type = PositionType::Relative;
            let outer = push(scene, NodeId::ROOT, boxed(outer));
            let mut middle =
                out_of_flow(PositionType::Absolute, Length::Points(0.0));
            middle.size = (Dimension::Points(60.0), Dimension::Auto);
            let middle = push(scene, outer, boxed(middle));
            push(scene, middle, boxed(sized(60.0, 90.0)));
            let mut layout =
                out_of_flow(PositionType::Absolute, Length::Points(0.0));
            layout.size = (Dimension::Points(30.0), percent(0.3333));
            push(scene, middle, measured(layout));
        }
        "abs-percent-grandparent-cb" => {
            let cb = push(
                scene,
                NodeId::ROOT,
                boxed(LayoutStyle {
                    position_type: PositionType::Relative,
                    ..column()
                }),
            );
            push(scene, cb, boxed(sized(50.0, 120.0)));
            // Left to its content, so the only thing that can paint a band
            // here is a percentage resolved against the grandparent. This row
            // asks *whether* it resolved; `abs-percent-grandparent-sized`
            // asks *which* box it resolved against, and needs a different
            // scene to do it.
            let mut between = LayoutStyle {
                size: (Dimension::Points(70.0), Dimension::Auto),
                ..LayoutStyle::default()
            };
            between.position_type = PositionType::Static;
            let between = push(scene, cb, boxed(between));
            let mut layout =
                out_of_flow(PositionType::Absolute, Length::Points(0.0));
            layout.size = (Dimension::Points(30.0), percent(0.3333));
            push(scene, between, measured(layout));
        }
        other => unreachable!("no percentage scene for `{other}`"),
    }
}

/// The rows sized by insets, and the controls that bound them.
///
/// `abs-top-only-child` and `abs-minheight-200` are the two that must not
/// move: the first is an absolute box with one inset, which states a
/// position and leaves the height to the content, and the second is the
/// in-flow child of a content-sized absolute box, whose percentage has a
/// genuinely circular containing block.
fn inset_and_control_case(scene: &mut Scene, case: &str) {
    match case {
        "abs-insets-box" => {
            let cb = declared_cb(scene);
            let mut layout =
                out_of_flow(PositionType::Absolute, Length::Percent(0.3333));
            layout.inset.bottom = Some(Length::Percent(0.3334));
            layout.size = (Dimension::Points(30.0), Dimension::Auto);
            push(scene, cb, measured(layout));
        }
        "abs-insets-child" => {
            let cb = declared_cb(scene);
            let mut outer =
                out_of_flow(PositionType::Absolute, Length::Percent(0.3333));
            outer.inset.bottom = Some(Length::Percent(0.3334));
            outer.size = (Dimension::Points(30.0), Dimension::Auto);
            let outer = push(scene, cb, boxed(outer));
            push(scene, outer, measured(full_height()));
        }
        "abs-top-only-child" => {
            let cb = declared_cb(scene);
            let mut outer =
                out_of_flow(PositionType::Absolute, Length::Points(10.0));
            outer.size = (Dimension::Points(30.0), Dimension::Auto);
            let outer = push(scene, cb, boxed(outer));
            push(scene, outer, boxed(sized(30.0, 25.0)));
            push(scene, outer, measured(full_height()));
        }
        "abs-declared-over-insets-child" => {
            let cb = declared_cb(scene);
            let mut outer =
                out_of_flow(PositionType::Absolute, Length::Points(0.0));
            outer.inset.bottom = Some(Length::Points(0.0));
            outer.size = (Dimension::Points(30.0), Dimension::Points(50.0));
            let outer = push(scene, cb, boxed(outer));
            push(scene, outer, measured(full_height()));
        }
        "abs-minheight-200" => {
            let mut holder = column();
            holder.size = (Dimension::Points(200.0), Dimension::Points(120.0));
            let holder = push(scene, NodeId::ROOT, boxed(holder));
            let mut middle = LayoutStyle {
                position_type: PositionType::Absolute,
                size: (Dimension::Points(200.0), Dimension::Auto),
                ..LayoutStyle::default()
            };
            middle.align_self = Some(Align::FlexStart);
            let middle = push(scene, holder, boxed(middle));
            let mut layout = sized(30.0, 20.0);
            layout.min_size = (Dimension::Auto, percent(2.0));
            push(scene, middle, measured(layout));
        }
        "inflow-percent-content-cb" => {
            let cb = push(scene, NodeId::ROOT, boxed(column()));
            push(scene, cb, boxed(sized(50.0, 120.0)));
            push(
                scene,
                cb,
                measured(LayoutStyle {
                    size: (Dimension::Points(30.0), percent(0.3333)),
                    ..LayoutStyle::default()
                }),
            );
        }
        other => unreachable!("no absolute scene for `{other}`"),
    }
}

/// The rows whose box is `position: fixed`.
///
/// Separate because the containing block is a different one: the page, or a
/// transformed ancestor where there is one, and never the nearest merely
/// positioned ancestor -- which the first row here is the control for.
fn fixed_case(scene: &mut Scene, case: &str) {
    match case {
        "fixed-percent-positioned-ancestor" => {
            let cb = push(
                scene,
                NodeId::ROOT,
                boxed(LayoutStyle {
                    position_type: PositionType::Relative,
                    ..column()
                }),
            );
            push(scene, cb, boxed(sized(50.0, 120.0)));
            let mut layout =
                out_of_flow(PositionType::Fixed, Length::Points(0.0));
            layout.size = (Dimension::Points(30.0), percent(0.3333));
            push(scene, cb, measured(layout));
        }
        "fixed-percent-no-ancestor" => {
            let cb = push(scene, NodeId::ROOT, boxed(column()));
            push(scene, cb, boxed(sized(50.0, 120.0)));
            let mut layout =
                out_of_flow(PositionType::Fixed, Length::Points(0.0));
            layout.size = (Dimension::Points(30.0), percent(0.3333));
            push(scene, cb, measured(layout));
        }
        "fixed-percent-transformed" => {
            let mut holder = sized(80.0, 150.0);
            holder.position_type = PositionType::Static;
            let mut holder = boxed(holder);
            holder.effects.transform = Some(Transform::default());
            let holder = push(scene, NodeId::ROOT, holder);
            let mut layout =
                out_of_flow(PositionType::Fixed, Length::Points(0.0));
            layout.size = (Dimension::Points(30.0), percent(0.3333));
            push(scene, holder, measured(layout));
        }
        "fixed-percent-transformed-auto" => {
            let mut holder = boxed(LayoutStyle {
                size: (Dimension::Points(80.0), Dimension::Auto),
                ..column()
            });
            holder.effects.transform = Some(Transform::default());
            let holder = push(scene, NodeId::ROOT, holder);
            push(scene, holder, boxed(sized(80.0, 150.0)));
            let mut layout =
                out_of_flow(PositionType::Fixed, Length::Points(0.0));
            layout.size = (Dimension::Points(30.0), percent(0.3333));
            push(scene, holder, measured(layout));
        }
        other => unreachable!("no fixed scene for `{other}`"),
    }
}

/// The 120-tall relative box four rows put their absolute box inside.
fn declared_cb(scene: &mut Scene) -> NodeId {
    let mut cb = sized(200.0, 120.0);
    cb.position_type = PositionType::Relative;
    push(scene, NodeId::ROOT, boxed(cb))
}

/// Chrome's answers, as measured.
const TABLE: &str = include_str!("assets/chrome/absolute-percentage.tsv");

/// Rows this renderer is known to disagree with, and none is expected.
///
/// Empty, and it fails in both directions: a row named here that agrees says
/// to delete the entry, so the list cannot outlive the divergence it records.
const KNOWN: &[&str] = &[];

/// One row of the table: the case's key and the height Chrome gave it.
fn rows() -> Vec<(String, f32)> {
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
            (key.to_owned(), height)
        })
        .collect()
}

#[test]
fn every_row_paints_the_band_chrome_measured() {
    let rows = rows();
    assert_eq!(
        rows.len(),
        15,
        "the table changed shape; the scenes here are per row"
    );

    // **Every row is measured before anything is asserted.** A loop that
    // asserted per row stops at the first disagreement, and a truncated list
    // of failures is not a count of them -- which matters most here, where the
    // question is whether a repair traded one wrong answer for another and the
    // evidence for that is a row that moved somewhere else in the table.
    let mut failing = Vec::new();
    let mut stale = Vec::new();
    for (key, chrome) in &rows {
        let painted = painted_height(&scene_for(key));
        // **Within one pixel, and stated rather than assumed.** Chrome reports
        // a fractional used height and this counts whole rows of pixels, so
        // 39.98 and 40 are the same answer. The rows this test exists for are
        // separated by forty pixels, not by one, so the allowance cannot
        // absorb the defect: `abs-percent-content-cb` is 0 against 39.98 on
        // the code this was written against.
        let agrees = (f32::from(u16::try_from(painted).unwrap_or(u16::MAX))
            - chrome)
            .abs()
            <= 1.0;
        if !agrees && !KNOWN.contains(&key.as_str()) {
            failing.push(format!("{key}: chrome {chrome:.2}, here {painted}"));
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

/// The reproduction from `l7aromeo/meo-canvas#84`, as the reporter wrote it.
///
/// **The same third, said two ways.** A box positioned a third down a 120-tall
/// containing block and a third tall, against one positioned a third down and a
/// third up from the bottom. The report is that the first painted nothing and
/// the second painted 40, and the ticket is the instance rather than the
/// defect -- but an issue's own case is the thing a reader checks first, so it
/// is here in its own right and not folded into the table.
#[test]
fn the_issue_s_two_spellings_of_a_third_agree() {
    fn painted(by_height: bool) -> u32 {
        let mut scene = page();
        let cb = declared_cb(&mut scene);
        let mut layout =
            out_of_flow(PositionType::Absolute, Length::Percent(0.3333));
        if by_height {
            layout.size = (Dimension::Points(30.0), percent(0.3333));
        } else {
            layout.inset.bottom = Some(Length::Percent(0.3334));
            layout.size = (Dimension::Points(30.0), Dimension::Auto);
        }
        push(&mut scene, cb, measured(layout));
        painted_height(&scene)
    }

    let by_height = painted(true);
    let by_insets = painted(false);
    assert_eq!(
        by_height, by_insets,
        "the two spellings still disagree: height {by_height}, insets {by_insets}"
    );
    // **Equal is not enough on its own.** Two spellings that both painted
    // nothing would satisfy the assertion above, which is the state the ticket
    // was filed about with one of them already correct.
    assert_eq!(
        by_height, 40,
        "a third of 120 is 40, and this painted {by_height}"
    );
}
