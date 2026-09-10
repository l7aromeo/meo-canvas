//! An SVG source is rasterised for the surface it lands on, not for its own
//! stated size.
//!
//! # Why this file exists
//!
//! The unit tests beside `raster` prove the document is rasterised at the size
//! it is *asked* for. **They cannot see what asks.** A painter that asked in
//! layout pixels would pass every one of them and still draw a page at
//! `scale: 2` from a raster half the surface's resolution -- which is the
//! bitmap upscale that keeping the document was meant to avoid.
//!
//! So this measures the pixels. A diagonal edge rasterised at device
//! resolution steps one pixel at a time; the same edge rasterised at layout
//! resolution and drawn twice as large steps two, and **every one of its
//! transitions lands on an even column**. That is a property of the upscale
//! rather than of the drawing, which is what makes it a test rather than a
//! golden.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Scene, Size,
    node::{ImageSource, Node, NodeId, NodeKind},
    style::{
        Dimension,
        paint::{Color, ObjectFit},
    },
};

/// A triangle, so the drawing has an edge that is neither horizontal nor
/// vertical at any scale.
const WEDGE: &str = concat!(
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" "##,
    r##"viewBox="0 0 20 20"><path d="M0 0 L20 20 L0 20 Z" fill="#000000"/>"##,
    r##"</svg>"##
);

/// A document authored for `currentColor`, and the same drawing with its fill
/// written out.
///
/// The pair is the point: a tint recolours the first and leaves the second
/// exactly as its author wrote it, and **that is the row that would have
/// caught a tint implemented as a string replace over every `fill`.**
const CURRENT_COLOR: &str = concat!(
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" "##,
    r##"viewBox="0 0 20 20"><rect width="20" height="20" "##,
    r##"fill="currentColor"/></svg>"##
);
/// A document that declares its own `color` and paints with `currentColor`.
///
/// **The row that makes "absent" mean something.** With no tint the document's
/// own green is what `currentColor` resolves to; the mutation that calls
/// `set_current_color(black)` when nothing was asked for replaces a colour the
/// root declared, and this is the only case where that is visible -- SVG's
/// initial `color` is black, so a document with no `color` of its own renders
/// identically either way.
const SELF_COLOURED: &str = concat!(
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" "##,
    r##"viewBox="0 0 20 20" color="#00ff00"><rect width="20" height="20" "##,
    r##"fill="currentColor"/></svg>"##
);
const HARDCODED: &str = concat!(
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" "##,
    r##"viewBox="0 0 20 20"><rect width="20" height="20" fill="#0000ff"/>"##,
    r##"</svg>"##
);

/// Renders one document at scale 1 with the node's own colour set or not, and
/// returns the pixel at its middle.
fn tinted(xml: &str, color: Option<Color>) -> [u8; 3] {
    let mut scene = Scene::new(Size::new(20.0, 20.0));
    if let Some(page) = scene.get_mut(NodeId::ROOT) {
        page.paint.background_color = Color::rgb(255, 255, 255);
    }
    let id = scene
        .push(
            NodeId::ROOT,
            Node::new(NodeKind::Image {
                source: ImageSource::Bytes(xml.as_bytes().to_vec()),
                fit: ObjectFit::Fill,
                position: (
                    meo_canvas_scene::style::Length::Percent(0.5),
                    meo_canvas_scene::style::Length::Percent(0.5),
                ),
                frame: None,
            }),
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(id) {
        node.layout.size = (Dimension::Points(20.0), Dimension::Points(20.0));
        node.text.color = color;
    }

    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    let png = renderer
        .render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("it did not render: {error}"));
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(
        png::Transformations::normalize_to_color8()
            | png::Transformations::ALPHA,
    );
    let mut reader = decoder
        .read_info()
        .unwrap_or_else(|error| unreachable!("{error}"));
    let mut bytes = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader
        .next_frame(&mut bytes)
        .unwrap_or_else(|error| unreachable!("{error}"));
    let at = ((10 * info.width as usize) + 10) * 4;
    [bytes[at], bytes[at + 1], bytes[at + 2]]
}

/// Renders the wedge at `scale`, drawn into a 20x20 box, and returns the
/// pixels.
fn drawn(scale: f32) -> (usize, Vec<u8>) {
    let mut scene = Scene::new(Size::new(20.0, 20.0));
    scene.scale = scale;
    if let Some(page) = scene.get_mut(NodeId::ROOT) {
        page.paint.background_color = Color::rgb(255, 255, 255);
    }
    let id = scene
        .push(
            NodeId::ROOT,
            Node::new(NodeKind::Image {
                source: ImageSource::Bytes(WEDGE.as_bytes().to_vec()),
                fit: ObjectFit::Fill,
                position: (
                    meo_canvas_scene::style::Length::Percent(0.5),
                    meo_canvas_scene::style::Length::Percent(0.5),
                ),
                frame: None,
            }),
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(id) {
        node.layout.size = (Dimension::Points(20.0), Dimension::Points(20.0));
    }

    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    let png = renderer
        .render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("it did not render: {error}"));
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(
        png::Transformations::normalize_to_color8()
            | png::Transformations::ALPHA,
    );
    let mut reader = decoder
        .read_info()
        .unwrap_or_else(|error| unreachable!("{error}"));
    let mut bytes = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader
        .next_frame(&mut bytes)
        .unwrap_or_else(|error| unreachable!("{error}"));
    bytes.truncate(info.buffer_size());
    (info.width as usize, bytes)
}

/// How many pixels sit between ink and paper -- the softness of the edge.
///
/// **Two discriminators were tried before this one.** Where a row's ink
/// *starts* is column zero on every row, because the wedge's diagonal is its
/// right edge, so it could not separate anything. Where it *ends*, and whether
/// those ends step by one column or two, does separate a doubled raster in
/// principle and not in practice: the upscale is smoothed rather than blocky,
/// so its ends step by one as well and the mutation passed.
///
/// What does separate them is how far the smoothing spreads. A document
/// rasterised at device resolution has about one part-covered pixel per row
/// along the diagonal; the same document rasterised at half that and scaled up
/// has the blend spread over two, plus the interpolation's own ramp.
fn edge_softness(width: usize, bytes: &[u8]) -> usize {
    let height = bytes.len() / (width * 4);
    (0..height)
        .map(|y| {
            (0..width)
                .filter(|x| (40..=215).contains(&bytes[(y * width + x) * 4]))
                .count()
        })
        .sum()
}

#[test]
fn a_document_is_rasterised_for_the_surface_and_not_for_itself() {
    let (width, bytes) = drawn(2.0);
    assert_eq!(width, 40, "a 20pt page at scale 2 is 40 pixels wide");

    let softness = edge_softness(width, &bytes);
    // **Measured both ways rather than reasoned about.** Rasterising for the
    // surface gives 20 part-covered pixels over the whole edge; asking in
    // layout pixels and letting the draw call scale gives 118. The bound sits
    // between them with room on each side, so neither antialiasing noise nor
    // a change of one pixel's coverage moves the answer.
    assert!(
        softness < 60,
        "the wedge's edge is {softness} pixels of blend, where a document \
         rasterised for this surface is about 20 and one rasterised at half \
         the surface and scaled up is about 118: the rasterisation is being \
         asked for in layout pixels rather than device pixels"
    );
}

#[test]
fn the_same_document_at_scale_one_is_the_control() {
    // Without this the row above could pass on a page that ignored `scale`
    // entirely: 40 is asserted there, and here is the same scene at 1.
    let (width, bytes) = drawn(1.0);
    assert_eq!(width, 20);
    assert!(
        edge_softness(width, &bytes) > 0,
        "the wedge drew no edge at all at scale 1"
    );
}

#[test]
fn a_colour_recolours_a_document_that_asked_for_one() {
    assert_eq!(
        tinted(CURRENT_COLOR, Some(Color::rgb(255, 0, 0))),
        [255, 0, 0],
        "a `currentColor` document did not take the colour it was given"
    );

    // **Absent is absent, and the first version of this row could not say
    // so.** Asserting that an untinted `currentColor` document renders black
    // passes for a renderer that sets black when nothing was asked for --
    // because SVG's own initial `color` is black, so the two are the same
    // picture. Measured: the mutation passed.
    //
    // A document that declares its own `color` is where they separate. With
    // nothing set it keeps its green; setting black "for consistency" would
    // replace a colour its author wrote.
    assert_eq!(tinted(CURRENT_COLOR, None), [0, 0, 0]);
    assert_eq!(
        tinted(SELF_COLOURED, None),
        [0, 255, 0],
        "a document's own `color` was overwritten by a tint nobody asked for"
    );
    // And it is still tintable: the tint replaces the root's own colour, which
    // is what the backend documents and what a caller asking for a colour
    // means.
    assert_eq!(
        tinted(SELF_COLOURED, Some(Color::rgb(255, 0, 0))),
        [255, 0, 0]
    );
}

#[test]
fn a_colour_leaves_a_hardcoded_fill_alone_and_does_not_complain() {
    // The limit, as a row rather than as a sentence in a doc comment: this
    // document's author wrote the colour they wanted, and a tint is not an
    // instruction to overwrite it. **No error either** -- there is nothing
    // wrong with the asset or with the request.
    assert_eq!(
        tinted(HARDCODED, Some(Color::rgb(255, 0, 0))),
        [0, 0, 255],
        "a hardcoded fill was overwritten by a tint"
    );
}

#[test]
fn a_colour_on_a_bitmap_is_refused() {
    // A 4x2 PNG, the same one the resolve tests use.
    const RED_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
        0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x02,
        0x08, 0x06, 0x00, 0x00, 0x00, 0x7F, 0xA8, 0x7D, 0x63, 0x00, 0x00, 0x00,
        0x12, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
        0x1F, 0x19, 0x33, 0xA0, 0x0B, 0x00, 0x00, 0x0F, 0x21, 0x0F, 0xF1, 0xFE,
        0x45, 0x14, 0x63, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
        0x42, 0x60, 0x82,
    ];
    let mut scene = Scene::new(Size::new(8.0, 8.0));
    let id = scene
        .push(
            NodeId::ROOT,
            Node::new(NodeKind::Image {
                source: ImageSource::Bytes(RED_PNG.to_vec()),
                fit: ObjectFit::Fill,
                position: (
                    meo_canvas_scene::style::Length::Percent(0.5),
                    meo_canvas_scene::style::Length::Percent(0.5),
                ),
                frame: None,
            }),
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(id) {
        node.layout.size = (Dimension::Points(4.0), Dimension::Points(2.0));
        node.text.color = Some(Color::rgb(255, 0, 0));
    }

    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    let result = renderer.render_to_buffer(
        &scene,
        ImageFormat::Png,
        &EncodeOptions::default(),
    );
    assert!(
        matches!(result, Err(meo_canvas_core::Error::TintOnRaster(_))),
        "a colour on a bitmap was ignored rather than refused"
    );

    // The control: the same scene without the colour renders. Otherwise the
    // row above would pass for a renderer that refused every bitmap.
    if let Some(node) = scene.get_mut(id) {
        node.text.color = None;
    }
    assert!(
        renderer
            .render_to_buffer(
                &scene,
                ImageFormat::Png,
                &EncodeOptions::default()
            )
            .is_ok(),
        "the same bitmap without a colour did not render"
    );
}

/// The 8x4 marks as a document, so the two source kinds are the same picture.
///
/// Asymmetric on both axes, because a symmetric one reads the same stretched
/// as cropped and `fill` and `cover` would agree by construction.
const MARKS: &str = concat!(
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="4" "##,
    r##"viewBox="0 0 8 4"><rect width="8" height="4" fill="#101014"/>"##,
    r##"<rect width="1" height="4" fill="#ff00ff"/>"##,
    r##"<rect x="7" width="1" height="4" fill="#00ffff"/></svg>"##,
);

/// The cell colour, which is what "not ink" means below.
const CELL: (u8, u8, u8) = (240, 240, 240);

/// Renders one source at one fit and returns the bounding box of its ink.
///
/// Reads [`ImageFormat::Raw`] rather than a PNG because the question is where
/// the picture landed, and a decode step between the paint and the assertion
/// is one more thing that can be wrong.
fn ink(
    source: ImageSource,
    fit: ObjectFit,
    (w, h): (f32, f32),
) -> (usize, usize, usize, usize) {
    let mut scene = Scene::new(Size::new(w, h));
    if let Some(page) = scene.get_mut(NodeId::ROOT) {
        page.paint.background_color = Color::rgb(CELL.0, CELL.1, CELL.2);
    }
    let id = scene
        .push(
            NodeId::ROOT,
            Node::new(NodeKind::Image {
                source,
                fit,
                position: (
                    meo_canvas_scene::style::Length::Percent(0.5),
                    meo_canvas_scene::style::Length::Percent(0.5),
                ),
                frame: None,
            }),
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(id) {
        node.layout.size = (Dimension::Points(w), Dimension::Points(h));
    }

    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    let bytes = renderer
        .render_to_buffer(&scene, ImageFormat::Raw, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("it did not render: {error}"));

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "both extents are whole numbers chosen by this test"
    )]
    let (width, height) = (w as usize, h as usize);
    let (mut left, mut top, mut right, mut bottom) =
        (usize::MAX, usize::MAX, 0_usize, 0_usize);
    for y in 0..height {
        for x in 0..width {
            let index = ((y * width) + x) * 4;
            let pixel = (bytes[index], bytes[index + 1], bytes[index + 2]);
            if pixel != CELL {
                left = left.min(x);
                top = top.min(y);
                right = right.max(x);
                bottom = bottom.max(y);
            }
        }
    }
    assert!(left != usize::MAX, "nothing was drawn at all");
    (left, top, right - left + 1, bottom - top + 1)
}

/// The same document rasterised at its own size, as PNG bytes.
///
/// **The control is generated rather than committed**, so the two source kinds
/// are provably the same picture. A committed bitmap would be a second asset
/// that could drift from the document and turn a divergence in the drawing
/// into a divergence in the art.
fn marks_as_raster() -> Vec<u8> {
    let mut scene = Scene::new(Size::new(8.0, 4.0));
    let id = scene
        .push(
            NodeId::ROOT,
            Node::new(NodeKind::Image {
                source: ImageSource::Bytes(MARKS.as_bytes().to_vec()),
                fit: ObjectFit::Fill,
                position: (
                    meo_canvas_scene::style::Length::Percent(0.0),
                    meo_canvas_scene::style::Length::Percent(0.0),
                ),
                frame: None,
            }),
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(id) {
        node.layout.size = (Dimension::Points(8.0), Dimension::Points(4.0));
    }
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    renderer
        .render_to_buffer(&scene, ImageFormat::Png, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("it did not render: {error}"))
}

/// Every object-fit rule but `fill` puts a document where it puts a bitmap.
///
/// **`fill` differing is correct, and this test exists to stop it being
/// "fixed".** An `<img>` whose source is a document with a `viewBox` and no
/// `preserveAspectRatio` carries SVG's default, `xMidYMid meet`: `object-fit`
/// sizes the replaced element, and the document then fits itself inside that
/// uniformly and centres it. So a stretch never reaches the drawing, and the
/// visible result is `contain`'s rectangle. Chrome does exactly this --
/// `tests/assets/chrome/object-fit.tsv` has `fill` and `contain` on the same
/// rectangle for every `svg` row and on different ones for every `raster` row,
/// at all three box sizes.
///
/// The other four rules preserve the source's aspect, so the document fills
/// what it is given and the two kinds agree.
///
/// **Written the other way round first, and the table refused it.** Making the
/// kinds agree under `fill` took a renderer that matched Chrome on all thirty
/// rows and broke three of them.
///
/// `l7aromeo/meo-canvas#95` is where this was reported, and it expects the two
/// kinds to agree -- true only at its own box, 200x133 against 60x40 art, where
/// a correct implementation draws 199.5x133 and a copy of the bitmap's rule
/// draws 200x133. Half a pixel apart, inside the slack that issue declares for
/// a vector edge. The placement itself was wrong until `meo-skia-canvas` 0.16.1
/// (`l7aromeo/meo-skia-canvas#212`); measured on 0.16.0,
/// `fill`, `contain` and `cover` are wrong at every box here and `scale-down`
/// at 6x6.
#[test]
fn a_document_is_placed_like_a_bitmap_except_where_it_fits_itself() {
    let raster = marks_as_raster();
    for box_size in [(72.0, 72.0), (6.0, 6.0), (100.0, 200.0)] {
        for (name, fit) in [
            ("contain", ObjectFit::Contain),
            ("cover", ObjectFit::Cover),
            ("none", ObjectFit::None),
            ("scale-down", ObjectFit::ScaleDown),
        ] {
            let vector = ink(
                ImageSource::Bytes(MARKS.as_bytes().to_vec()),
                fit,
                box_size,
            );
            let bitmap = ink(ImageSource::Bytes(raster.clone()), fit, box_size);
            assert_eq!(
                vector, bitmap,
                "{name} at {box_size:?}: the document landed at {vector:?} \
                 and the bitmap at {bitmap:?}"
            );
        }

        // `fill` is the one that differs, and it differs by landing on
        // `contain`'s rectangle. Asserted rather than merely excluded: an
        // exclusion would also pass if the document stopped being drawn.
        let filled = ink(
            ImageSource::Bytes(MARKS.as_bytes().to_vec()),
            ObjectFit::Fill,
            box_size,
        );
        let contained = ink(
            ImageSource::Bytes(MARKS.as_bytes().to_vec()),
            ObjectFit::Contain,
            box_size,
        );
        let stretched = ink(
            ImageSource::Bytes(raster.clone()),
            ObjectFit::Fill,
            box_size,
        );
        assert_eq!(
            filled, contained,
            "fill at {box_size:?}: a document fits itself under \
             `xMidYMid meet`, so it should land on contain's rectangle"
        );
        assert_ne!(
            filled, stretched,
            "fill at {box_size:?}: the document and the bitmap agreed, which \
             means the document was stretched to the box -- Chrome does not \
             stretch it, because the document re-fits itself inside whatever \
             viewport it is given"
        );
    }
}
