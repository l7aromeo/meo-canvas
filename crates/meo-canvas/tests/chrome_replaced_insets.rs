//! What insets do to a replaced element, which is not what they do to a box.
//!
//! `l7aromeo/meo-canvas#92`: an `Image` with `inset: 0` and no width or height
//! was stretched to its containing block. Chrome keeps its intrinsic size — a
//! replaced element's `auto` width and height are its own dimensions, and CSS
//! resolves the over-constraint by dropping an inset rather than by stretching
//! the element (CSS 2.2 §10.3.8, §10.6.5).
//!
//! **Ink, not `LayoutResult`.** The report is that the picture is wrong, and a
//! test reading the solved box would pass on a renderer that laid the image out
//! correctly and painted it somewhere else.
//!
//! **Every replaced row asserts 60x40 exactly, not "smaller than the
//! container".** Two of the defect's rows moved in opposite directions —
//! `left: 0; right: 0` was 200 wide and `top: 0; bottom: 0` was 45 — so a
//! threshold assertion passes on one and fails on the other, and "expect 60"
//! reached by two different routes is what the exact number rules out.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Scene, Size,
    node::{ImageSource, Node, NodeId, NodeKind},
    style::{
        Dimension, Length,
        layout::{Align, Display, PositionType},
        paint::{Color, ObjectFit},
    },
};

/// The page. Big enough that a box overflowing its parent is still on it.
const PAGE: f32 = 400.0;

/// The art, and the size every replaced row must come back as.
const ART: (f32, f32) = (60.0, 40.0);

/// The containing block: wider and shorter than the art, so stretching shows.
const BLOCK: (f32, f32) = (200.0, 30.0);

/// Where the block sits, so a negative offset is still on the page.
const ORIGIN: f32 = 100.0;

const INK: (u8, u8, u8) = (204, 0, 0);
const PAPER: (u8, u8, u8) = (255, 255, 255);

/// A 60x40 image, rendered rather than embedded.
///
/// The test carries no fixture: the bytes come from this renderer encoding a
/// scene of one filled box, so the intrinsic size under test is one the suite
/// can restate rather than a number in a file nobody re-derives.
fn art() -> Vec<u8> {
    let mut scene = Scene::new(Size::new(ART.0, ART.1));
    scene.nodes[0].paint.background_color = Color::rgb(INK.0, INK.1, INK.2);
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    renderer
        .render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"))
}

/// The recorded Chrome rows, so the assertions compare against a measurement
/// rather than against numbers retyped into this file.
///
/// `chrome_tables_are_read` requires it: a table nobody reads is a table nobody
/// checks, and the pairing is by `include_str!` of the path.
const TABLE: &str = include_str!("assets/chrome/replaced-insets.tsv");

/// Chrome's `(x, y, width, height)` for one case of the table.
///
/// **The columns after the fourth are not read, and saying so is what makes
/// that safe.** The row carries its declarations and a note as well, and both
/// are prose for a reader rather than values for an assertion -- so editing a
/// note changes the table and cannot change a result. A reader narrower than
/// the file it reads is fine exactly when someone has recorded that the tail is
/// ignored; it is a defect when the narrowing is silent and a later column
/// starts carrying something.
fn chrome(case: &str) -> (i32, i32, u32, u32) {
    for line in TABLE.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let mut columns = line.split('\t');
        if columns.next() != Some(case) {
            continue;
        }
        let mut number = || -> f32 {
            columns
                .next()
                .and_then(|cell| cell.parse::<f32>().ok())
                .unwrap_or_else(|| unreachable!("{case} has a malformed cell"))
        };
        let (x, y, width, height) = (number(), number(), number(), number());
        #[expect(
            clippy::cast_possible_truncation,
            reason = "every recorded extent is a whole number of pixels"
        )]
        return (
            x as i32,
            y as i32,
            width.round() as u32,
            height.round() as u32,
        );
    }
    unreachable!("{case} is not a row of replaced-insets.tsv")
}

fn push(scene: &mut Scene, parent: NodeId, node: Node) -> NodeId {
    scene
        .push(parent, node)
        .unwrap_or_else(|error| unreachable!("{error}"))
}

/// The inked rectangle, parent-relative: `(x, y, width, height)`.
fn painted(scene: &Scene) -> (i32, i32, u32, u32) {
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    let bytes = renderer
        .render_to_buffer(scene, ImageFormat::Raw, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"));

    let side = PAGE as u32;
    let (mut left, mut top, mut right, mut bottom) = (side, side, 0_u32, 0_u32);
    let mut seen = false;
    for y in 0..side {
        for x in 0..side {
            let at = ((y * side + x) * 4) as usize;
            if (bytes[at], bytes[at + 1], bytes[at + 2]) == INK {
                seen = true;
                left = left.min(x);
                top = top.min(y);
                right = right.max(x);
                bottom = bottom.max(y);
            }
        }
    }
    if !seen {
        return (0, 0, 0, 0);
    }
    // `cast_signed` rather than `as`: every value here is a pixel index inside
    // a 400-square page, so the conversion cannot wrap -- but `-D warnings`
    // does not know that and an `as` would be the reviewer's problem forever.
    let origin = (ORIGIN as u32).cast_signed();
    (
        left.cast_signed() - origin,
        top.cast_signed() - origin,
        right - left + 1,
        bottom - top + 1,
    )
}

/// A page carrying the containing block at [`ORIGIN`], and the node inside it.
fn scene_with(inner: Node) -> Scene {
    let mut scene = Scene::new(Size::new(PAGE, PAGE));
    scene.nodes[0].paint.background_color =
        Color::rgb(PAPER.0, PAPER.1, PAPER.2);
    scene.nodes[0].layout.padding = meo_canvas_scene::Sides {
        top: Length::Points(ORIGIN),
        right: Length::Points(ORIGIN),
        bottom: Length::Points(ORIGIN),
        left: Length::Points(ORIGIN),
    };

    let mut block = Node::new(NodeKind::Box);
    block.layout.size =
        (Dimension::Points(BLOCK.0), Dimension::Points(BLOCK.1));
    block.layout.position_type = PositionType::Relative;
    // **`FlexStart`, or an in-flow child's cross size is the line's.** The
    // block is a flex container, so an item with an `auto` cross size stretches
    // to it -- measured, the overflow control below reads 200x40 under the
    // default `stretch`, and 200 is what a clamped renderer and an unclamped
    // one both produce. Absolutely positioned children are not flex items and
    // do not notice this either way.
    block.layout.align_items = Some(Align::FlexStart);
    let block = push(&mut scene, NodeId::ROOT, block);
    push(&mut scene, block, inner);
    scene
}

/// An absolutely positioned image with the given insets.
fn image(insets: [Option<f32>; 4]) -> Node {
    let mut node = Node::new(NodeKind::Image {
        source: ImageSource::Bytes(art()),
        fit: ObjectFit::Fill,
        position: (Length::ZERO, Length::ZERO),
        frame: None,
    });
    node.layout.position_type = PositionType::Absolute;
    node.layout.inset = meo_canvas_scene::Sides {
        top: insets[0].map(Length::Points),
        right: insets[1].map(Length::Points),
        bottom: insets[2].map(Length::Points),
        left: insets[3].map(Length::Points),
    };
    node
}

/// Every shape of inset leaves a replaced element at its intrinsic size.
///
/// The rows are `replaced-insets.tsv`'s, which reads 60x40 for each of these in
/// Chrome. `inset: 0` is the reported case; the others are the class it belongs
/// to, and three of them were wrong in ways the ticket does not mention.
#[test]
fn insets_never_size_a_replaced_element() {
    let cases: [(&str, [Option<f32>; 4]); 5] = [
        ("img inset 0", [Some(0.0), Some(0.0), Some(0.0), Some(0.0)]),
        ("img top 0 only", [Some(0.0), None, None, None]),
        ("img left 0 right 0", [None, Some(0.0), None, Some(0.0)]),
        ("img top 0 bottom 0", [Some(0.0), None, Some(0.0), None]),
        ("img no insets", [None, None, None, None]),
    ];
    for (name, insets) in cases {
        let (_, _, width, height) = painted(&scene_with(image(insets)));
        let (_, _, want_width, want_height) = chrome(name);
        assert_eq!(
            (width, height),
            (want_width, want_height),
            "{name}: painted {width}x{height}, and Chrome paints \
             {want_width}x{want_height} -- insets do not size a replaced \
             element"
        );
    }
}

/// A lone **end** inset positions and is not dropped.
///
/// The rule drops the end inset only where both on an axis are set, so these
/// two are untouched by construction — which is an argument from the code until
/// a row says otherwise. They are what goes red if anyone simplifies the
/// condition to "drop the end inset for a replaced node", which reads like a
/// tidy-up and is the obvious wrong generalisation. Chrome: `right: 0` alone
/// puts the image at x=140 in a 200-wide block, and `bottom: 0` alone at y=-10
/// in a 30-tall one, both still 60x40.
#[test]
fn a_lone_end_inset_still_positions() {
    let (x, _, width, height) =
        painted(&scene_with(image([None, Some(0.0), None, None])));
    let (want_x, _, want_width, want_height) = chrome("img right 0 only");
    assert_eq!(
        (x, width, height),
        (want_x, want_width, want_height),
        "a lone `right: 0` put the image at x={x}; it should sit against the \
         right edge at its intrinsic size"
    );

    let (_, y, width, height) =
        painted(&scene_with(image([None, None, Some(0.0), None])));
    let (_, want_y, want_width, want_height) = chrome("img bottom 0 only");
    assert_eq!(
        (y, width, height),
        (want_y, want_width, want_height),
        "a lone `bottom: 0` put the image at y={y}; it should sit against the \
         bottom edge and overflow the top"
    );
}

/// **The control for the whole rule: a non-replaced box still stretches.**
///
/// This is what fails against the broadest way to get the fix wrong — keying on
/// `out_of_flow && insets` and forgetting `replaced`. Chrome stretches a `div`
/// with `inset: 0` to 200x30, and so must we.
#[test]
fn a_non_replaced_box_still_stretches_to_its_insets() {
    let mut inner = Node::new(NodeKind::Box);
    inner.paint.background_color = Color::rgb(INK.0, INK.1, INK.2);
    inner.layout.position_type = PositionType::Absolute;
    inner.layout.inset = meo_canvas_scene::Sides {
        top: Some(Length::ZERO),
        right: Some(Length::ZERO),
        bottom: Some(Length::ZERO),
        left: Some(Length::ZERO),
    };

    let (_, _, width, height) = painted(&scene_with(inner));
    let (_, _, want_width, want_height) = chrome("div inset 0");
    assert_eq!(
        (width, height),
        (want_width, want_height),
        "a Box with `inset: 0` painted {width}x{height} against Chrome's \
         {want_width}x{want_height}; insets DO size a non-replaced element \
         and this row is what says the rule is scoped"
    );
}

/// An in-flow replaced element overflows a container shorter than itself.
///
/// **This is not the control for the clamp, and it was written believing it
/// was.** Run as a mutation with both halves of the fix reverted, it passes —
/// so it cannot be what pins the clamp removal. The in-flow path reaches
/// `fit_intrinsic` with a known width, which is a different arm from the one
/// that used to narrow, and the height comes back 40 either way.
///
/// **What actually pins the clamp is the `no insets` case of
/// `insets_never_size_a_replaced_element`**, which reads 60x30 with the clamp
/// and 60x40 without it, and which fails under the same mutation. The row is
/// kept because an in-flow replaced element overflowing is worth stating and
/// Chrome agrees — 60x40 in a 200x30 block — but it is stated here as a fact
/// and not as a guard.
#[test]
fn an_in_flow_replaced_element_overflows_a_shorter_container() {
    let mut inner = Node::new(NodeKind::Image {
        source: ImageSource::Bytes(art()),
        fit: ObjectFit::Fill,
        position: (Length::ZERO, Length::ZERO),
        frame: None,
    });
    inner.layout.position_type = PositionType::Relative;

    // **`FlexStart`, or the answer is the flex line's and not the image's.**
    // The block is a flex container, so an in-flow item with an `auto` cross
    // size stretches to the line: measured, this scene reads 200x40 under the
    // default `stretch` — the width from the line and the height from the art —
    // and 200 is what a clamped renderer and an unclamped one both produce.
    // Chrome says the same in the same two arrangements: `display:flex` with
    // `align-items: stretch` gives 45x30 and `flex-start` gives 60x40, which is
    // `align-items` doing its job rather than a clamp anywhere.
    // **The height is the axis under test and the width is not.** The block is
    // 30 tall and the art is 40, so an unnarrowed intrinsic extent overflows by
    // ten and a narrowed one does not.
    //
    // The width comes back as the block's 200 rather than the art's 60, and
    // `align_items` does not move it -- **not because it is ignored but because
    // it does not apply**: `LayoutStyle`'s default `display` is `Block`
    // (`meo-canvas-scene/src/style/layout.rs:365`), so this container is a
    // block container and there is no flex line to align against. In block
    // flow CSS gives a block-level box its containing block's width and an
    // inline-level replaced element its intrinsic one, which is why Chrome says
    // 60 and we say 200: we are treating a replaced element as block-level.
    //
    // **That is a second sizing rule and it is not repaired here.** It is the
    // same family as this fix -- surroundings deciding a replaced element's
    // size -- reached down the in-flow block path rather than the out-of-flow
    // inset path, and folding it in would leave the mutation table unable to
    // say which change did what. Filed separately, with a Chrome row built for
    // it rather than read off a scene built for this. So the width is left
    // unasserted deliberately: pinning 200 would pin a defect, and pinning 60
    // would fail until that fix lands.
    let (_, _, _, height) = painted(&scene_with(inner));
    assert_eq!(
        height, ART.1 as u32,
        "an in-flow image in a {}-tall block painted {height} tall; it should \
         overflow at its intrinsic {} rather than be narrowed to fit",
        BLOCK.1, ART.1
    );
}

/// A declared size still wins over the intrinsic one.
///
/// What stops a repair pinning the intrinsic size unconditionally. Chrome gives
/// `inset: 0` with `width/height: 100%` the containing block, 200x30, and so do
/// we — before this change and after it.
#[test]
fn a_declared_size_still_wins() {
    let mut inner = image([Some(0.0), Some(0.0), Some(0.0), Some(0.0)]);
    inner.layout.size = (Dimension::Percent(1.0), Dimension::Percent(1.0));

    let (_, _, width, height) = painted(&scene_with(inner));
    assert_eq!(
        (width, height),
        (BLOCK.0 as u32, BLOCK.1 as u32),
        "a declared `100%` painted {width}x{height}; an explicit size wins \
         over the intrinsic one"
    );
}

/// An `Image` **with children** is a container, not a replaced element.
///
/// **This scene has no Chrome row and cannot have one**: an `<img>` cannot have
/// children in HTML, so the browser has no answer and the rule comes from the
/// engine's own split instead — `layout.rs` gives a childless node the
/// measurer's context and a node with children a taffy container, so an `Image`
/// with a subtree is never measured and has no intrinsic size to prefer.
/// Saying which source the rule came from is what stops the next reader hunting
/// a row in `replaced-insets.tsv` that cannot exist.
///
/// Measured, and it is why `is_replaced` asks arity: keyed on `NodeKind::Image`
/// alone this painted **0**, because `unstretch_replaced` removed the inset
/// that was the box's only height and left it sized by the `height: 100%` child
/// that was waiting on that height. **Discriminated rather than assumed** —
/// with `unstretch_replaced` disabled and the `insets_settle_it` guard left in
/// it painted 30, and with the guard disabled and `unstretch_replaced` left in
/// it painted 0, so the sizing half was the cause and the definiteness guard
/// was not.
#[test]
fn an_image_with_children_is_a_container() {
    let mut inner = image([Some(0.0), None, Some(0.0), None]);
    inner.layout.display = Display::Flex;
    let mut scene = scene_with(inner);

    let mut child = Node::new(NodeKind::Box);
    child.paint.background_color = Color::rgb(INK.0, INK.1, INK.2);
    child.layout.size = (Dimension::Points(10.0), Dimension::Percent(1.0));
    push(&mut scene, NodeId::new(2), child);

    let (_, _, _, height) = painted(&scene);
    assert_eq!(
        height, BLOCK.1 as u32,
        "a `height: 100%` child of an absolutely positioned image with \
         `top`/`bottom` painted {height} tall; the image has children, so it is \
         a container and its height is the distance between its insets"
    );
}
