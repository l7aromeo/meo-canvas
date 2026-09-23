//! A paragraph that fits must not be truncated because its box was rounded:
//! taffy hands a `27.73`-wide run 27 or 28, and at 27 `maxLines: 1` turned `HP
//! HP` into `HP …`. Asserted as a pair with the same scene without `maxLines`,
//! on the repository's face, at offsets that round both ways.

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

/// The two container widths from the report, kept though the offset rather than
/// the width decides the rounding.
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
            // A spacer of fractional width, as the report's side-by-side cases
            // supplied: the fraction of the label's own `x` decides which way
            // its box rounds.
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

/// The offsets the label is placed at: taffy rounds a rect as `round(x + w) -
/// round(x)`, and across 400 offsets every fraction at or above `.5`
/// reproduces. `0.0` is the agreeing row, so a broken harness cannot pass as a
/// defect.
const OFFSETS: [f32; 3] = [0.0, 0.5, 0.7];

/// A label that fits is drawn whole whatever `maxLines` says: the same scene
/// without it must render identically to the byte.
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

/// The control: a label that genuinely does not fit is still truncated, passing
/// before the fix and after. It carries an explicit width, since as a free flex
/// item it would take its natural width and fit.
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
