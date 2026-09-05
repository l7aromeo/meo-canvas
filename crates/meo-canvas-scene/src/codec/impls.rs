//! [`Wire`] for every type a scene is built from.
//!
//! One file rather than an implementation beside each type, because the wire
//! layout is one specification and a reader checking it against the code should
//! find the whole thing in one place. The enums are absent here: their
//! implementation comes from the `wire_enum` macro, which is what keeps a
//! variant's byte declared beside the variant itself.
//!
//! Every implementation writes its fields in the order the type declares them
//! and reads them back in the same order. That is the whole invariant, and it
//! is checked by the round-trip tests rather than by inspection.

use super::{CodecError, Reader, Wire, Writer};
use crate::{
    geometry::{Corners, Sides},
    node::{
        ImageSource, LineCap, LineJoin, Node, NodeId, NodeKind, NodeTag,
        PathPaint,
    },
    style::{
        Dimension, Length,
        effect::{
            BoxShadow, Effects, FillRule, Mask, MaskShape, TextShadow,
            Transform,
        },
        layout::{GridPlacement, LayoutStyle, TrackSize},
        paint::{
            BackgroundImage, BackgroundSize, Color, Gradient, GradientGeometry,
            GradientKind, GradientStop, LinearDirection, PaintStyle,
        },
        text::{
            FontWeight, LineHeight, ParagraphStyle, Spacing, TextSegment,
            TextStroke, TextStyle,
        },
    },
    surface::{ImageFetchAttempt, ImageFetchFailure},
};

/// The tag byte for a variant that carries nothing after it.
///
/// Named because `0` appears as a discriminant, as a length and as a `false`,
/// and a reader tracing the format should be able to tell which is which.
const TAG_FIRST: u8 = 0;

impl Wire for u8 {
    fn write(&self, out: &mut Writer<'_>) {
        out.u8(*self);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        input.u8()
    }
}

impl Wire for u16 {
    fn write(&self, out: &mut Writer<'_>) {
        out.u16(*self);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        input.u16()
    }
}

impl Wire for u32 {
    fn write(&self, out: &mut Writer<'_>) {
        out.u32(*self);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        input.u32()
    }
}

impl Wire for i16 {
    fn write(&self, out: &mut Writer<'_>) {
        out.i16(*self);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        input.i16()
    }
}

impl Wire for i32 {
    fn write(&self, out: &mut Writer<'_>) {
        out.i32(*self);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        input.i32()
    }
}

impl Wire for f32 {
    fn write(&self, out: &mut Writer<'_>) {
        out.f32(*self);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        input.f32()
    }
}

impl Wire for bool {
    fn write(&self, out: &mut Writer<'_>) {
        out.bool(*self);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        input.bool()
    }
}

impl Wire for String {
    fn write(&self, out: &mut Writer<'_>) {
        out.str(self);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        input.str()
    }
}

impl<T: Wire> Wire for Option<T> {
    fn write(&self, out: &mut Writer<'_>) {
        out.opt(self.as_ref());
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        input.opt()
    }
}

impl<T: Wire> Wire for Vec<T> {
    fn write(&self, out: &mut Writer<'_>) {
        out.list(self);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        input.list()
    }
}

impl<T: Wire> Wire for Sides<T> {
    fn write(&self, out: &mut Writer<'_>) {
        self.top.write(out);
        self.right.write(out);
        self.bottom.write(out);
        self.left.write(out);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            top: T::read(input)?,
            right: T::read(input)?,
            bottom: T::read(input)?,
            left: T::read(input)?,
        })
    }
}

impl<T: Wire> Wire for Corners<T> {
    fn write(&self, out: &mut Writer<'_>) {
        self.top_left.write(out);
        self.top_right.write(out);
        self.bottom_right.write(out);
        self.bottom_left.write(out);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            top_left: T::read(input)?,
            top_right: T::read(input)?,
            bottom_right: T::read(input)?,
            bottom_left: T::read(input)?,
        })
    }
}

impl Wire for NodeId {
    /// A `u32`, always.
    const MIN_ENCODED: usize = 4;

    fn write(&self, out: &mut Writer<'_>) {
        out.u32(self.get());
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self::new(input.u32()?))
    }
}

impl Wire for Length {
    fn write(&self, out: &mut Writer<'_>) {
        match self {
            Self::Points(value) => {
                out.u8(TAG_FIRST);
                out.f32(*value);
            }
            Self::Percent(value) => {
                out.u8(1);
                out.f32(*value);
            }
        }
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let offset = input.offset();
        match input.u8()? {
            TAG_FIRST => Ok(Self::Points(input.f32()?)),
            1 => Ok(Self::Percent(input.f32()?)),
            tag => Err(CodecError::UnknownTag { offset, tag }),
        }
    }
}

impl Wire for LineHeight {
    fn write(&self, out: &mut Writer<'_>) {
        match self {
            Self::Number(value) => {
                out.u8(TAG_FIRST);
                out.f32(*value);
            }
            Self::Length(value) => {
                out.u8(1);
                out.f32(*value);
            }
            Self::Percent(value) => {
                out.u8(2);
                out.f32(*value);
            }
        }
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let offset = input.offset();
        match input.u8()? {
            TAG_FIRST => Ok(Self::Number(input.f32()?)),
            1 => Ok(Self::Length(input.f32()?)),
            2 => Ok(Self::Percent(input.f32()?)),
            tag => Err(CodecError::UnknownTag { offset, tag }),
        }
    }
}

impl Wire for Dimension {
    fn write(&self, out: &mut Writer<'_>) {
        match self {
            Self::Auto => out.u8(TAG_FIRST),
            Self::Points(value) => {
                out.u8(1);
                out.f32(*value);
            }
            Self::Percent(value) => {
                out.u8(2);
                out.f32(*value);
            }
        }
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let offset = input.offset();
        match input.u8()? {
            TAG_FIRST => Ok(Self::Auto),
            1 => Ok(Self::Points(input.f32()?)),
            2 => Ok(Self::Percent(input.f32()?)),
            tag => Err(CodecError::UnknownTag { offset, tag }),
        }
    }
}

impl Wire for TrackSize {
    fn write(&self, out: &mut Writer<'_>) {
        match self {
            Self::Auto => out.u8(TAG_FIRST),
            Self::Points(value) => {
                out.u8(1);
                out.f32(*value);
            }
            Self::Percent(value) => {
                out.u8(2);
                out.f32(*value);
            }
            Self::Fraction(value) => {
                out.u8(3);
                out.f32(*value);
            }
        }
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let offset = input.offset();
        match input.u8()? {
            TAG_FIRST => Ok(Self::Auto),
            1 => Ok(Self::Points(input.f32()?)),
            2 => Ok(Self::Percent(input.f32()?)),
            3 => Ok(Self::Fraction(input.f32()?)),
            tag => Err(CodecError::UnknownTag { offset, tag }),
        }
    }
}

impl Wire for Spacing {
    fn write(&self, out: &mut Writer<'_>) {
        match self {
            Self::Normal => out.u8(TAG_FIRST),
            Self::Points(value) => {
                out.u8(1);
                out.f32(*value);
            }
            Self::Em(value) => {
                out.u8(2);
                out.f32(*value);
            }
        }
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let offset = input.offset();
        match input.u8()? {
            TAG_FIRST => Ok(Self::Normal),
            1 => Ok(Self::Points(input.f32()?)),
            2 => Ok(Self::Em(input.f32()?)),
            tag => Err(CodecError::UnknownTag { offset, tag }),
        }
    }
}

impl Wire for Color {
    fn write(&self, out: &mut Writer<'_>) {
        out.u8(self.r);
        out.u8(self.g);
        out.u8(self.b);
        out.u8(self.a);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            r: input.u8()?,
            g: input.u8()?,
            b: input.u8()?,
            a: input.u8()?,
        })
    }
}

impl Wire for FontWeight {
    fn write(&self, out: &mut Writer<'_>) {
        out.u16(self.get());
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        // Clamped rather than refused: `FontWeight::new` is the only way to
        // build one, so a value outside the CSS range came from a writer that
        // did not clamp, and a browser handed the same value would clamp it
        // too.
        Ok(Self::new(input.u16()?))
    }
}

impl Wire for GridPlacement {
    fn write(&self, out: &mut Writer<'_>) {
        self.start.write(out);
        self.span.write(out);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            start: Option::<i16>::read(input)?,
            span: Option::<u16>::read(input)?,
        })
    }
}

impl Wire for ImageSource {
    fn write(&self, out: &mut Writer<'_>) {
        match self {
            Self::Path(path) => {
                out.u8(TAG_FIRST);
                out.str(path);
            }
            Self::Url(url) => {
                out.u8(1);
                out.str(url);
            }
            Self::Bytes(bytes) => {
                out.u8(2);
                out.bytes(bytes);
            }
        }
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let offset = input.offset();
        match input.u8()? {
            TAG_FIRST => Ok(Self::Path(input.str()?)),
            1 => Ok(Self::Url(input.str()?)),
            2 => Ok(Self::Bytes(input.bytes()?)),
            tag => Err(CodecError::UnknownTag { offset, tag }),
        }
    }
}

impl Wire for GradientStop {
    fn write(&self, out: &mut Writer<'_>) {
        out.f32(self.offset);
        self.color.write(out);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            offset: input.f32()?,
            color: Color::read(input)?,
        })
    }
}

impl Wire for LinearDirection {
    fn write(&self, out: &mut Writer<'_>) {
        match self {
            Self::Angle(degrees) => {
                out.u8(TAG_FIRST);
                out.f32(*degrees);
            }
            Self::Between { start, end } => {
                out.u8(TAG_FIRST + 1);
                start.0.write(out);
                start.1.write(out);
                end.0.write(out);
                end.1.write(out);
            }
        }
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let offset = input.offset();
        match input.u8()? {
            TAG_FIRST => Ok(Self::Angle(input.f32()?)),
            1 => Ok(Self::Between {
                start: (Length::read(input)?, Length::read(input)?),
                end: (Length::read(input)?, Length::read(input)?),
            }),
            tag => Err(CodecError::UnknownTag { offset, tag }),
        }
    }
}

impl Wire for GradientGeometry {
    fn write(&self, out: &mut Writer<'_>) {
        // The tag first, and it is `GradientKind` rather than a number written
        // here: one definition of which shapes exist, shared with the arena and
        // with the generated TypeScript table.
        self.kind().write(out);
        match self {
            Self::Linear { direction } => direction.write(out),
            Self::Radial { at } => {
                at.0.write(out);
                at.1.write(out);
            }
            Self::Conic { at, from } => {
                at.0.write(out);
                at.1.write(out);
                out.f32(*from);
            }
        }
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(match GradientKind::read(input)? {
            GradientKind::Linear => Self::Linear {
                direction: LinearDirection::read(input)?,
            },
            GradientKind::Radial => Self::Radial {
                at: (Length::read(input)?, Length::read(input)?),
            },
            GradientKind::Conic => Self::Conic {
                at: (Length::read(input)?, Length::read(input)?),
                from: input.f32()?,
            },
        })
    }
}

impl Wire for Gradient {
    fn write(&self, out: &mut Writer<'_>) {
        self.geometry.write(out);
        out.list(&self.stops);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            geometry: GradientGeometry::read(input)?,
            stops: input.list()?,
        })
    }
}

impl Wire for BackgroundSize {
    fn write(&self, out: &mut Writer<'_>) {
        match self {
            Self::PerAxis(width, height) => {
                out.u8(TAG_FIRST);
                width.write(out);
                height.write(out);
            }
            Self::Cover => out.u8(TAG_FIRST + 1),
            Self::Contain => out.u8(TAG_FIRST + 2),
        }
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let offset = input.offset();
        match input.u8()? {
            TAG_FIRST => Ok(Self::PerAxis(
                Dimension::read(input)?,
                Dimension::read(input)?,
            )),
            1 => Ok(Self::Cover),
            2 => Ok(Self::Contain),
            tag => Err(CodecError::UnknownTag { offset, tag }),
        }
    }
}

impl Wire for BackgroundImage {
    fn write(&self, out: &mut Writer<'_>) {
        self.source.write(out);
        self.repeat.write(out);
        self.size.write(out);
        self.position.0.write(out);
        self.position.1.write(out);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            source: ImageSource::read(input)?,
            repeat: Wire::read(input)?,
            size: BackgroundSize::read(input)?,
            position: (Length::read(input)?, Length::read(input)?),
        })
    }
}

impl Wire for PathPaint {
    fn write(&self, out: &mut Writer<'_>) {
        match self {
            Self::Solid(color) => {
                out.u8(TAG_FIRST);
                color.write(out);
            }
            Self::Gradient(gradient) => {
                out.u8(1);
                gradient.write(out);
            }
        }
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let offset = input.offset();
        match input.u8()? {
            TAG_FIRST => Ok(Self::Solid(Color::read(input)?)),
            1 => Ok(Self::Gradient(Gradient::read(input)?)),
            tag => Err(CodecError::UnknownTag { offset, tag }),
        }
    }
}

impl Wire for LayoutStyle {
    fn write(&self, out: &mut Writer<'_>) {
        self.display.write(out);
        self.position_type.write(out);
        self.inset.write(out);
        self.size.0.write(out);
        self.size.1.write(out);
        self.min_size.0.write(out);
        self.min_size.1.write(out);
        self.max_size.0.write(out);
        self.max_size.1.write(out);
        self.aspect_ratio.write(out);
        self.margin.write(out);
        self.padding.write(out);
        self.border.write(out);
        self.flex_direction.write(out);
        self.flex_wrap.write(out);
        out.f32(self.flex_grow);
        out.f32(self.flex_shrink);
        self.flex_basis.write(out);
        self.justify_content.write(out);
        self.align_items.write(out);
        self.align_self.write(out);
        self.align_content.write(out);
        self.gap.0.write(out);
        self.gap.1.write(out);
        self.overflow.0.write(out);
        self.overflow.1.write(out);
        self.box_sizing.write(out);
        self.direction.write(out);
        out.list(&self.grid_template_columns);
        out.list(&self.grid_template_rows);
        self.grid_auto_rows.write(out);
        self.grid_auto_columns.write(out);
        self.grid_auto_flow.write(out);
        self.grid_column.write(out);
        self.grid_row.write(out);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            display: Wire::read(input)?,
            position_type: Wire::read(input)?,
            inset: Sides::read(input)?,
            size: (Dimension::read(input)?, Dimension::read(input)?),
            min_size: (Dimension::read(input)?, Dimension::read(input)?),
            max_size: (Dimension::read(input)?, Dimension::read(input)?),
            aspect_ratio: Option::read(input)?,
            margin: Sides::read(input)?,
            padding: Sides::read(input)?,
            border: Sides::read(input)?,
            flex_direction: Wire::read(input)?,
            flex_wrap: Wire::read(input)?,
            flex_grow: input.f32()?,
            flex_shrink: input.f32()?,
            flex_basis: Dimension::read(input)?,
            justify_content: Option::read(input)?,
            align_items: Option::read(input)?,
            align_self: Option::read(input)?,
            align_content: Option::read(input)?,
            gap: (Length::read(input)?, Length::read(input)?),
            overflow: (Wire::read(input)?, Wire::read(input)?),
            box_sizing: Wire::read(input)?,
            direction: Wire::read(input)?,
            grid_template_columns: input.list()?,
            grid_template_rows: input.list()?,
            grid_auto_rows: Option::read(input)?,
            grid_auto_columns: Option::read(input)?,
            grid_auto_flow: Wire::read(input)?,
            grid_column: GridPlacement::read(input)?,
            grid_row: GridPlacement::read(input)?,
        })
    }
}

impl Wire for PaintStyle {
    fn write(&self, out: &mut Writer<'_>) {
        self.background_color.write(out);
        self.gradient.write(out);
        self.background_image.write(out);
        self.border_color.write(out);
        self.border_color_all.write(out);
        self.border_style.write(out);
        self.border_radius.write(out);
        out.f32(self.opacity);
        self.blend_mode.write(out);
        out.bool(self.dither);
        out.opt(self.z_index.as_ref());
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            background_color: Color::read(input)?,
            gradient: Option::read(input)?,
            background_image: Option::read(input)?,
            border_color: Sides::read(input)?,
            border_color_all: Color::read(input)?,
            border_style: Wire::read(input)?,
            border_radius: Corners::read(input)?,
            opacity: input.f32()?,
            blend_mode: Wire::read(input)?,
            dither: input.bool()?,
            z_index: input.opt()?,
        })
    }
}

impl Wire for TextStroke {
    fn write(&self, out: &mut Writer<'_>) {
        out.f32(self.width);
        self.color.write(out);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            width: input.f32()?,
            color: Color::read(input)?,
        })
    }
}

impl Wire for TextStyle {
    fn write(&self, out: &mut Writer<'_>) {
        self.font_family.write(out);
        self.font_size.write(out);
        self.font_weight.write(out);
        self.font_style.write(out);
        self.color.write(out);
        self.text_align.write(out);
        self.text_decoration.write(out);
        self.vertical_align.write(out);
        self.paint_order.write(out);
        self.line_height.write(out);
        self.line_gap.write(out);
        self.letter_spacing.write(out);
        self.word_spacing.write(out);
        self.font_variant.write(out);
        self.text_stroke.write(out);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            font_family: Option::read(input)?,
            font_size: Option::read(input)?,
            font_weight: Option::read(input)?,
            font_style: Option::read(input)?,
            color: Option::read(input)?,
            text_align: Option::read(input)?,
            text_decoration: Option::read(input)?,
            vertical_align: Option::read(input)?,
            paint_order: Option::read(input)?,
            line_height: Option::read(input)?,
            line_gap: Option::read(input)?,
            letter_spacing: Option::read(input)?,
            word_spacing: Option::read(input)?,
            font_variant: Option::read(input)?,
            text_stroke: Option::read(input)?,
        })
    }
}

impl Wire for TextSegment {
    fn write(&self, out: &mut Writer<'_>) {
        out.str(&self.text);
        self.style.write(out);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            text: input.str()?,
            style: TextStyle::read(input)?,
        })
    }
}

impl Wire for Transform {
    fn write(&self, out: &mut Writer<'_>) {
        self.translate_x.write(out);
        self.translate_y.write(out);
        out.f32(self.rotate_degrees);
        out.f32(self.scale_x);
        out.f32(self.scale_y);
        self.origin.0.write(out);
        self.origin.1.write(out);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            translate_x: Length::read(input)?,
            translate_y: Length::read(input)?,
            rotate_degrees: input.f32()?,
            scale_x: input.f32()?,
            scale_y: input.f32()?,
            origin: (Length::read(input)?, Length::read(input)?),
        })
    }
}

impl Wire for BoxShadow {
    fn write(&self, out: &mut Writer<'_>) {
        out.bool(self.inset);
        out.f32(self.offset_x);
        out.f32(self.offset_y);
        out.f32(self.blur);
        out.f32(self.spread);
        self.color.write(out);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            inset: input.bool()?,
            offset_x: input.f32()?,
            offset_y: input.f32()?,
            blur: input.f32()?,
            spread: input.f32()?,
            color: Color::read(input)?,
        })
    }
}

impl Wire for TextShadow {
    fn write(&self, out: &mut Writer<'_>) {
        out.f32(self.offset_x);
        out.f32(self.offset_y);
        out.f32(self.blur);
        self.color.write(out);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            offset_x: input.f32()?,
            offset_y: input.f32()?,
            blur: input.f32()?,
            color: Color::read(input)?,
        })
    }
}

impl Wire for Mask {
    fn write(&self, out: &mut Writer<'_>) {
        match self {
            Self::Image(source) => {
                out.u8(TAG_FIRST);
                source.write(out);
            }
            Self::Shape(shape) => {
                out.u8(1);
                shape.write(out);
            }
            Self::Path { data, fill_rule } => {
                out.u8(2);
                out.str(data);
                fill_rule.write(out);
            }
            Self::Gradient(gradient) => {
                out.u8(3);
                gradient.write(out);
            }
        }
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let offset = input.offset();
        match input.u8()? {
            TAG_FIRST => Ok(Self::Image(ImageSource::read(input)?)),
            1 => Ok(Self::Shape(MaskShape::read(input)?)),
            2 => Ok(Self::Path {
                data: input.str()?,
                fill_rule: FillRule::read(input)?,
            }),
            3 => Ok(Self::Gradient(Gradient::read(input)?)),
            tag => Err(CodecError::UnknownTag { offset, tag }),
        }
    }
}

impl Wire for Effects {
    fn write(&self, out: &mut Writer<'_>) {
        self.transform.write(out);
        out.list(&self.box_shadows);
        out.list(&self.text_shadows);
        self.mask.write(out);
        self.filter.write(out);
        self.backdrop_filter.write(out);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            transform: Option::read(input)?,
            box_shadows: input.list()?,
            text_shadows: input.list()?,
            mask: Option::read(input)?,
            filter: Option::read(input)?,
            backdrop_filter: Option::read(input)?,
        })
    }
}

impl Wire for NodeKind {
    fn write(&self, out: &mut Writer<'_>) {
        match self {
            Self::Box => out.u8(NodeTag::Box.to_wire()),
            Self::Text {
                segments,
                paragraph,
            } => {
                out.u8(NodeTag::Text.to_wire());
                out.list(segments);
                paragraph.max_lines.write(out);
                paragraph.ellipsis.write(out);
            }
            Self::Image {
                source,
                fit,
                position,
                frame,
            } => {
                out.u8(NodeTag::Image.to_wire());
                source.write(out);
                fit.write(out);
                position.0.write(out);
                position.1.write(out);
                frame.write(out);
            }
            Self::Path {
                data,
                view_box,
                stretch,
                fill,
                stroke,
                line_width,
                fill_rule,
                line_cap,
                line_join,
                line_dash,
                line_dash_offset,
            } => {
                out.u8(NodeTag::Path.to_wire());
                out.str(data);
                // Written as four separate optionals rather than one optional
                // tuple: `Option<(f32, f32, f32, f32)>` has no `Wire` impl and
                // adding one for a shape used once would be a wider change
                // than the field itself.
                view_box.is_some().write(out);
                if let Some((min_x, min_y, width, height)) = view_box {
                    out.f32(*min_x);
                    out.f32(*min_y);
                    out.f32(*width);
                    out.f32(*height);
                }
                stretch.write(out);
                fill.write(out);
                stroke.write(out);
                out.f32(*line_width);
                fill_rule.write(out);
                line_cap.write(out);
                line_join.write(out);
                out.list(line_dash);
                out.f32(*line_dash_offset);
            }
        }
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let offset = input.offset();
        let tag = input.u8()?;
        match NodeTag::from_wire(tag) {
            Some(NodeTag::Box) => Ok(Self::Box),
            Some(NodeTag::Text) => Ok(Self::Text {
                segments: input.list()?,
                paragraph: ParagraphStyle {
                    max_lines: Option::read(input)?,
                    ellipsis: Option::read(input)?,
                },
            }),
            Some(NodeTag::Image) => Ok(Self::Image {
                source: ImageSource::read(input)?,
                fit: Wire::read(input)?,
                position: (Length::read(input)?, Length::read(input)?),
                frame: Option::read(input)?,
            }),
            Some(NodeTag::Path) => Ok(Self::Path {
                data: input.str()?,
                view_box: if bool::read(input)? {
                    Some((
                        input.f32()?,
                        input.f32()?,
                        input.f32()?,
                        input.f32()?,
                    ))
                } else {
                    None
                },
                stretch: bool::read(input)?,
                fill: Option::read(input)?,
                stroke: Option::read(input)?,
                line_width: input.f32()?,
                fill_rule: FillRule::read(input)?,
                line_cap: LineCap::read(input)?,
                line_join: LineJoin::read(input)?,
                line_dash: input.list()?,
                line_dash_offset: input.f32()?,
            }),
            None => Err(CodecError::UnknownTag { offset, tag }),
        }
    }
}

impl Wire for Node {
    /// Measured rather than counted by hand, and asserted in
    /// `a_node_never_encodes_smaller_than_the_reservation_assumes`: a default
    /// container is 184 bytes of node plus the four its parent spends naming
    /// it. Every style field is fixed width, so nothing a caller sets makes a
    /// node smaller -- only larger.
    const MIN_ENCODED: usize = 184;

    fn write(&self, out: &mut Writer<'_>) {
        self.kind.write(out);
        self.layout.write(out);
        self.paint.write(out);
        self.text.write(out);
        self.effects.write(out);
        out.list(&self.children);
        self.name.write(out);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            kind: NodeKind::read(input)?,
            layout: LayoutStyle::read(input)?,
            paint: PaintStyle::read(input)?,
            text: TextStyle::read(input)?,
            effects: Effects::read(input)?,
            children: input.list()?,
            name: Option::read(input)?,
        })
    }
}

impl Wire for ImageFetchAttempt {
    fn write(&self, out: &mut Writer<'_>) {
        out.str(&self.url);
        self.failure.write(out);
        out.opt(self.status.as_ref());
        out.str(&self.detail);
    }

    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError> {
        Ok(Self {
            url: String::read(input)?,
            failure: ImageFetchFailure::read(input)?,
            status: Option::<u16>::read(input)?,
            detail: String::read(input)?,
        })
    }
}
