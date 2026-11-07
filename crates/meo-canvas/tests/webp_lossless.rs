//! What `EncodeOptions` documents of `meo-skia-canvas`, which reads them: a
//! WebP is VP8L and decodes to the drawn pixels under `lossless: Some(true)`
//! or, with `lossless` unset, at quality 1.0, still and animated; `Some(false)`
//! is lossy at any quality. A change upstream fails here. The JavaScript side
//! is `webp-lossless.test.ts`; the CLI is `cli.rs`.
use std::fmt::Debug;

use meo_canvas::{
    Box, Canvas, Display, Element, EncodeOptions, Format, Image, PageInfo,
    Renderer, Root, Styled, corners_all, hex_rgb, px, rgba,
};

/// The page, small enough to compare whole and large enough for every edge.
const SIZE: (f32, f32) = (64.0, 40.0);

/// The lossless bitstream's chunk tag.
const VP8L: [u8; 4] = *b"VP8L";

/// The lossy bitstream's chunk tag.
const VP8: [u8; 4] = *b"VP8 ";

/// An animation frame's chunk tag.
const ANMF: [u8; 4] = *b"ANMF";

/// The value of a step this file cannot go on without.
fn must<T, E: Debug>(result: Result<T, E>, what: &str) -> T {
    result.unwrap_or_else(|error| unreachable!("{what}: {error:?}"))
}

/// A page with antialiased curves and a translucent fill over a second colour,
/// which is where a lossy encode moves pixels first.
fn page(index: u32) -> Element {
    Box::new()
        .display(Display::Block)
        .size(px(SIZE.0), px(SIZE.1))
        .background_color(hex_rgb(0x12_34_56))
        .children(
            Box::new()
                .display(Display::Block)
                .size(px(6.0_f32.mul_add(index as f32, 20.0)), px(24.0))
                .border_radius_corners(corners_all(9.0))
                .background_color(rgba(232, 40, 140, 0.7)),
        )
}

/// The chunk tags at the top level of a RIFF WebP, and inside each `ANMF`.
fn chunks(webp: &[u8]) -> Vec<[u8; 4]> {
    fn walk(bytes: &[u8], mut at: usize, end: usize, out: &mut Vec<[u8; 4]>) {
        while at + 8 <= end {
            let tag = [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]];
            let len = u32::from_le_bytes([
                bytes[at + 4],
                bytes[at + 5],
                bytes[at + 6],
                bytes[at + 7],
            ]) as usize;
            if tag == ANMF {
                // Sixteen bytes of frame header precede the frame's own chunks.
                walk(bytes, at + 8 + 16, at + 8 + len, out);
            }
            out.push(tag);
            at += 8 + len + (len & 1);
        }
    }
    assert_eq!(&webp[..4], b"RIFF", "not a RIFF file");
    assert_eq!(&webp[8..12], b"WEBP", "not a WebP");
    let mut out = Vec::new();
    walk(webp, 12, webp.len(), &mut out);
    out
}

/// How many chunks of `tag` a WebP carries.
fn count(tags: &[[u8; 4]], tag: [u8; 4]) -> usize {
    tags.iter().filter(|each| **each == tag).count()
}

/// The pixels of `webp` as this renderer decodes them, drawn at their own size.
fn decoded(webp: &[u8]) -> Vec<u8> {
    let mut canvas = must(
        Root::new(SIZE.0)
            .height(SIZE.1)
            .display(Display::Block)
            .children(
                Image::bytes(webp.to_vec())
                    .display(Display::Block)
                    .size(px(SIZE.0), px(SIZE.1)),
            )
            .render(&Renderer::new()),
        "the decode renders",
    );
    must(canvas.to_buffer(Format::Raw), "the decode reads back")
}

fn still() -> Canvas {
    must(
        Root::new(SIZE.0)
            .height(SIZE.1)
            .display(Display::Block)
            .children(page(0))
            .render(&Renderer::new()),
        "the page renders",
    )
}

/// The encode `EncodeOptions` documents as lossless WebP.
fn full_quality() -> EncodeOptions {
    EncodeOptions {
        quality: Some(1.0),
        ..EncodeOptions::default()
    }
}

fn lossless() -> EncodeOptions {
    EncodeOptions {
        lossless: Some(true),
        ..EncodeOptions::default()
    }
}

#[test]
fn a_full_quality_still_is_vp8l_and_decodes_to_the_drawn_pixels() {
    let mut canvas = still();
    let drawn = must(canvas.to_buffer(Format::Raw), "raw");

    let webp = must(
        canvas.to_buffer_with(Format::Webp, &full_quality()),
        "full-quality WebP encodes",
    );
    let tags = chunks(&webp);
    assert_eq!(count(&tags, VP8L), 1, "{tags:?}");
    assert_eq!(count(&tags, VP8), 0, "{tags:?}");
    assert!(decoded(&webp) == drawn, "the decode is not the drawing");

    // The control: the default quality is lossy, so the comparison above can
    // fail.
    let lossy = must(canvas.to_buffer(Format::Webp), "lossy WebP encodes");
    assert_eq!(count(&chunks(&lossy), VP8), 1);
    assert!(
        decoded(&lossy) != drawn,
        "the lossy control decoded exactly"
    );
}

#[test]
fn every_frame_of_a_full_quality_animation_is_vp8l() {
    let mut canvas = must(
        Root::new(SIZE.0)
            .height(SIZE.1)
            .display(Display::Block)
            .pages(3)
            .page_builder(|info: PageInfo| page(info.index))
            .render(&Renderer::new()),
        "the sequence renders",
    );

    let webp = must(
        canvas.to_buffer_with(Format::Webp, &full_quality()),
        "full-quality animated WebP encodes",
    );
    let tags = chunks(&webp);
    assert_eq!(count(&tags, ANMF), 3, "{tags:?}");
    assert_eq!(count(&tags, VP8L), 3, "a frame is not VP8L: {tags:?}");
    assert_eq!(count(&tags, VP8), 0, "{tags:?}");
}

fn lossy() -> EncodeOptions {
    EncodeOptions {
        lossless: Some(false),
        ..EncodeOptions::default()
    }
}

#[test]
fn lossless_true_on_a_webp_is_vp8l_and_decodes_to_the_drawn_pixels() {
    let mut canvas = still();
    let drawn = must(canvas.to_buffer(Format::Raw), "raw");
    let webp = must(
        canvas.to_buffer_with(Format::Webp, &lossless()),
        "lossless WebP encodes",
    );
    let tags = chunks(&webp);
    assert_eq!(count(&tags, VP8L), 1, "{tags:?}");
    assert_eq!(count(&tags, VP8), 0, "{tags:?}");
    assert!(decoded(&webp) == drawn, "the decode is not the drawing");
}

#[test]
fn every_frame_of_a_lossless_animation_is_vp8l() {
    let mut canvas = must(
        Root::new(SIZE.0)
            .height(SIZE.1)
            .display(Display::Block)
            .pages(3)
            .page_builder(|info: PageInfo| page(info.index))
            .render(&Renderer::new()),
        "the sequence renders",
    );
    let tags = chunks(&must(
        canvas.to_buffer_with(Format::Webp, &lossless()),
        "lossless animated WebP encodes",
    ));
    assert_eq!(count(&tags, ANMF), 3, "{tags:?}");
    assert_eq!(count(&tags, VP8L), 3, "a frame is not VP8L: {tags:?}");
    assert_eq!(count(&tags, VP8), 0, "{tags:?}");
}

#[test]
fn lossless_false_on_a_webp_is_lossy_even_at_full_quality() {
    let mut canvas = still();
    let drawn = must(canvas.to_buffer(Format::Raw), "raw");
    let forced = EncodeOptions {
        quality: Some(1.0),
        ..lossy()
    };
    for options in [lossy(), forced] {
        let webp = must(
            canvas.to_buffer_with(Format::Webp, &options),
            "lossy WebP encodes",
        );
        let tags = chunks(&webp);
        assert_eq!(count(&tags, VP8), 1, "{options:?}: {tags:?}");
        assert_eq!(count(&tags, VP8L), 0, "{options:?}: {tags:?}");
        assert!(decoded(&webp) != drawn, "{options:?} decoded exactly");
    }
}

#[test]
fn lossless_unset_on_a_webp_follows_the_quality() {
    let mut canvas = still();
    let default = chunks(&must(canvas.to_buffer(Format::Webp), "default WebP"));
    assert_eq!(count(&default, VP8), 1, "{default:?}");
    let full = chunks(&must(
        canvas.to_buffer_with(Format::Webp, &full_quality()),
        "full-quality WebP",
    ));
    assert_eq!(count(&full, VP8L), 1, "{full:?}");
}

#[test]
fn lossless_beside_a_quality_below_one_is_refused() {
    let options = EncodeOptions {
        quality: Some(0.5),
        ..lossless()
    };
    for format in [Format::Webp, Format::Avif] {
        let Err(error) = still().to_buffer_with(format, &options) else {
            unreachable!("{format:?} took lossless at quality 0.5");
        };
        let message = error.to_string();
        assert!(message.contains("asks for a lossy file"), "{message}");
    }
}

#[test]
fn lossless_reaches_avif_and_is_refused_where_it_means_nothing() {
    let mut canvas = still();
    must(
        canvas.to_buffer_with(Format::Avif, &lossless()),
        "lossless AVIF encodes",
    );
    for format in [Format::Png, Format::Jpeg, Format::Gif] {
        assert!(
            canvas.to_buffer_with(format, &lossless()).is_err(),
            "{format:?} took lossless"
        );
    }
}
