//! A paragraph that fits must not be truncated because its box was rounded.
//!
//! # The defect
//!
//! taffy rounds a rect to whole pixels, so a run whose exact width is `27.73`
//! is handed a box of `27` or `28` depending on the fraction of its own `x`.
//! At `27` the wrap breaks it, and `maxLines: 1` then truncates the break to a
//! marker: `HP HP` renders as `HP …` in a box with room for all of it.
//!
//! `measure.rs` already carries the rescue for this -- it re-lays a paragraph
//! that broke at unconstrained width, and takes that result when one more pixel
//! would have been enough. **Its trigger is the line count of the laid-out
//! block, tested after `max_lines` has already collapsed two lines into one
//! plus a marker**, so the paragraph that most needs rescuing is the one the
//! test can no longer see.
//!
//! # Why this asserts a pair rather than a picture
//!
//! Truncation that changes nothing is the claim, so the control is the same
//! scene without `maxLines`. Where the text fits, the two renders must be
//! **identical**: a paragraph that fits is not affected by a rule about what to
//! do when it does not. That is a comparison the renderer cannot satisfy by
//! being consistently wrong, and it needs no golden image.
//!
//! # Both directions, and the second is the one a careless fix breaks
//!
//! Making the rescue reachable for truncating paragraphs risks a paragraph that
//! *should* truncate no longer doing so. The `< 1.0` guard should prevent it,
//! but "should" is what the guard says rather than what a test says. So the
//! over-long case asserts the opposite: the two renders must **differ**.
//!
//! # Two widths, because one proves one width
//!
//! The break is a function of where the box's left edge lands, so a fixture at
//! a lucky offset passes on broken code and looks exactly like a passing test.
//! `161.4` and `300` are the repro's own pair and they round differently.
//!
//! # The face is not optional
//!
//! On the fallback face the exact widths are different numbers and may never
//! land badly. A render on it passes either side of this defect.

use meo_canvas::{
    BorderStyle, Column, Element, Format, Renderer, Root, Row, Styled, Text,
    hex_rgb, px,
    scene::{Align, Justify, LineHeight, Overflow},
    sides,
};

/// The repository's own face, whose `HP HP` at 12px is `27.73` and rounds
/// badly.
const FONT: (&str, &str) = (
    "Fixture",
    "../meo-canvas-core/tests/assets/fonts/Oswald-VariableFont_wght.ttf",
);

/// The two container widths from the report.
///
/// Kept, though the sweep showed the **offset** is what decides the rounding
/// rather than the width: widening the container moves every `x`, which is why
/// the report saw the defect get worse at `300` than at `161.4` and read it as
/// a width effect. Both are here so a future reader can see the width was not
/// the variable, rather than being told.
const WIDTHS: [f32; 2] = [161.4, 300.0];

/// The two shapes from the report that failed, and they differ in x offset.
#[derive(Clone, Copy)]
enum Shape {
    /// A bare column holding an unpadded row.
    Bare,
    /// A bordered column with `overflow: hidden`, holding a padded row.
    Clipped,
}

impl Shape {
    /// What a failure calls it.
    const fn name(self) -> &'static str {
        match self {
            Self::Bare => "bare row",
            Self::Clipped => "clipped wrapper",
        }
    }
}

/// Renders one case, with or without the truncation.
fn render(
    width: f32,
    label: &str,
    truncating: bool,
    shape: Shape,
    offset: f32,
    label_width: Option<f32>,
) -> Vec<u8> {
    let text = || {
        let node = Text::new(label)
            .font_size(12.0)
            .line_height(LineHeight::Length(12.0))
            .color(hex_rgb(0xff_ff_ff));
        let node = if truncating {
            node.max_lines(1).ellipsis("\u{2026}")
        } else {
            node
        };
        match label_width {
            Some(w) => node.width(px(w)),
            None => node,
        }
    };

    let row = |pad: f32| -> Element {
        let r = Row::new()
            .gap_xy(px(8.0), px(8.0))
            .align_items(Align::Center)
            .justify_content(Justify::SpaceBetween)
            .children([
                text(),
                Text::new("46.6%")
                    .font_size(14.0)
                    .line_height(LineHeight::Length(14.0))
                    .color(hex_rgb(0xff_ff_ff)),
            ]);
        if pad > 0.0 { r.padding(px(pad)) } else { r }
    };
    let case: Element = match shape {
        Shape::Bare => Column::new().width(px(width)).children(row(0.0)),
        Shape::Clipped => Column::new()
            .width(px(width))
            .border(sides(2.0, 2.0, 2.0, 2.0))
            .border_style(BorderStyle::Solid)
            .border_color(hex_rgb(0xe7_00_0b))
            .overflow(Overflow::Hidden)
            .children(row(8.0)),
    };

    let mut renderer = Renderer::new();
    // Off for the reason every pixel-reading test here turns it off: two
    // rasterisers do not agree to the byte.
    renderer.set_gpu(false);
    renderer
        .register_font(FONT.0, FONT.1)
        .unwrap_or_else(|error| {
            unreachable!("the font did not register: {error}")
        });
    let mut canvas = Root::new(900.0)
        .height(60.0)
        .background_color(hex_rgb(0x2a_05_08))
        .font_family(FONT.0)
        .padding(px(12.0))
        .gap_xy(px(12.0), px(12.0))
        .align_items(Align::FlexStart)
        .children([
            // A spacer of fractional width, which is what the report's five
            // side-by-side cases supplied: it is the fraction of the label's
            // own `x` that decides which way its box rounds, and a case
            // sitting alone at a whole-numbered offset never
            // reproduces.
            Column::new()
                .width(px(offset))
                .children(Text::new(" ").font_size(1.0)),
            case,
        ])
        .render(&renderer)
        .unwrap_or_else(|error| {
            unreachable!("the scene did not render: {error}")
        });
    canvas.to_buffer(Format::Raw).unwrap_or_else(|error| {
        unreachable!("the canvas did not encode: {error}")
    })
}

/// The offsets the label's box is placed at.
///
/// **The container width is not the causal variable; the fraction of the
/// label's own `x` is.** taffy rounds a rect as `round(x + w) - round(x)`, so a
/// run whose exact width is `12.49` is handed `12` or `13` depending only on
/// where its left edge falls. Swept across 400 offsets, every one with a
/// fraction at or above `.5` reproduces and every one below it does not.
///
/// `0.0` is here as the **agreeing row**: it already renders correctly today.
/// A run where every offset fails is a broken harness rather than a defect, and
/// without a row whose answer is known there is nothing to tell the two apart.
const OFFSETS: [f32; 3] = [0.0, 0.5, 0.7];

/// A label that fits its box is drawn whole, whatever `maxLines` says.
///
/// Truncation is a rule about what to do when text does not fit. Applied to
/// text that fits, it must change nothing -- so the control is the same scene
/// without `maxLines`, and the two renders must be identical to the byte.
#[test]
fn truncation_changes_nothing_for_a_label_that_fits() {
    let mut wrong = Vec::new();
    for shape in [Shape::Bare, Shape::Clipped] {
        for width in WIDTHS {
            for offset in OFFSETS {
                for label in ["HP", "HP HP"] {
                    let plain =
                        render(width, label, false, shape, offset, None);
                    let clamped =
                        render(width, label, true, shape, offset, None);
                    if plain != clamped {
                        wrong.push(format!(
                            "{} at container {width}, offset {offset}: \
                             `maxLines: 1` changed {label:?}, which fits its \
                             box. Its exact width is not a whole number, so at \
                             this offset the box rounds down, the wrap breaks \
                             it, and the break is truncated to a marker",
                            shape.name()
                        ));
                    }
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The control: a label that genuinely does not fit is still truncated.
///
/// The failure mode of the fix is a rescue that reaches too far and stops
/// truncating anything. **This passes before the fix as well as after**, which
/// is what makes it a control rather than a second copy of the case above: a
/// run where both tests fail says nothing about this one.
///
/// **The label carries an explicit width, and the first version of this test
/// did not.** Without one it is a flex item, and at a container of 300 the row
/// simply overflows and hands the label its full natural width -- at which
/// point the text fits and *not* truncating is the correct answer. The test
/// failed and the code was right. A control has to constrain the thing it is
/// controlling for.
#[test]
fn a_label_that_genuinely_overflows_is_still_truncated() {
    const LONG: &str = "Antidisestablishmentarianism and then some more words";
    let mut wrong = Vec::new();
    for shape in [Shape::Bare, Shape::Clipped] {
        for width in WIDTHS {
            for offset in OFFSETS {
                let plain =
                    render(width, LONG, false, shape, offset, Some(40.0));
                let clamped =
                    render(width, LONG, true, shape, offset, Some(40.0));
                if plain == clamped {
                    wrong.push(format!(
                        "{} at container {width}, offset {offset}: \
                         `maxLines: 1` changed nothing for a label far too long \
                         for its box, so nothing is being truncated at all",
                        shape.name()
                    ));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
