//! An SVG source is rasterised for the surface it lands on, not its own size.
//! The unit tests beside `raster` see the size asked for, not what asks, so
//! this reads pixels: rasterised in layout pixels and scaled up, a diagonal
//! edge's blend spreads over twice as many part-covered pixels.

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
/// written out: a tint recolours the first and not the second, which is what
/// catches a tint done as a string replace over every `fill`.
const CURRENT_COLOR: &str = concat!(
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" "##,
    r##"viewBox="0 0 20 20"><rect width="20" height="20" "##,
    r##"fill="currentColor"/></svg>"##
);
/// A document that declares its own `color` and paints with `currentColor`.
/// SVG's initial `color` is black, so only a document with its own colour shows
/// the difference between no tint and a black one.
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

/// How many part-covered pixels lie between ink and paper along the edge. Where
/// a row's ink starts or ends cannot separate the two rasters, since the
/// upscale is smoothed and steps by one too; how far the blend spreads can --
/// about one pixel per row at device resolution, two when scaled.
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
    // Measured both ways: rasterising for the surface gives 20 part-covered
    // pixels, asking in layout pixels gives 118. The bound sits well
    // between them.
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

    // Absent is absent: an untinted `currentColor` document renders black
    // either way, since SVG's initial `color` is black. A document
    // declaring its own `color` is where a tint set "for consistency" would
    // show.
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

/// Renders one source at one fit and returns the bounding box of its ink. Reads
/// [`ImageFormat::Raw`], so no decode sits between the paint and the assertion.
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

/// The same document rasterised at its own size, as PNG bytes. Generated rather
/// than committed, so the two source kinds are provably one picture.
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

/// Every object-fit rule but `fill` places a document where it places a bitmap.
/// `fill` differs correctly: a document with a `viewBox` keeps SVG's default
/// `xMidYMid meet` and fits itself inside the stretched box, as `contain` does;
/// `tests/assets/chrome/object-fit.tsv` shows Chrome doing the same.
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
