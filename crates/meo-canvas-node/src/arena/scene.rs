//! [`ArenaValue`] for the composite types a property can hold.
//!
//! Split from [`super::value`], which covers the primitives and the containers,
//! because these are the scene's own vocabulary rather than the format's.

use meo_canvas_scene::{
    node::{ImageSource, LineCap, LineJoin, NodeTag, PathPaint},
    style::{
        Length, PaintOrder,
        effect::{BoxShadow, FillRule, Mask, MaskShape, TextShadow, Transform},
        layout::{
            Align, BoxSizing, Direction, Display, FlexDirection, FlexWrap,
            GridAutoFlow, GridPlacement, Justify, Overflow, PositionType,
            TrackSize,
        },
        paint::{
            BackgroundImage, BackgroundRepeat, BlendMode, BorderStyle, Color,
            Gradient, GradientKind, GradientStop, ObjectFit,
        },
        text::{
            FontStyle, FontVariant, FontWeight, Spacing, TextAlign,
            TextDecoration, TextStroke, VerticalAlign,
        },
    },
};

use super::{ArenaError, Reader, value::ArenaValue};

/// Implements [`ArenaValue`] for a `wire_enum!` type.
///
/// One slot holding the same number the byte codec writes as its discriminant,
/// because both read `from_wire`. A keyword's number is therefore identical in
/// the two representations rather than similar.
macro_rules! arena_enum {
    ($($name:ident),+ $(,)?) => {
        $(
            impl ArenaValue for $name {
                fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
                    let slot = input.offset();
                    let tag = input.bounded_integer(f64::from(u8::MAX))? as u8;
                    Self::from_wire(tag).ok_or(ArenaError::UnknownTag {
                        slot,
                        what: stringify!($name),
                        found: f64::from(tag),
                    })
                }
            }
        )+
    };
}

arena_enum!(
    Align,
    BackgroundRepeat,
    BlendMode,
    BorderStyle,
    BoxSizing,
    Direction,
    Display,
    FillRule,
    FlexDirection,
    FlexWrap,
    FontStyle,
    FontVariant,
    GradientKind,
    GridAutoFlow,
    Justify,
    LineCap,
    LineJoin,
    MaskShape,
    NodeTag,
    ObjectFit,
    Overflow,
    PaintOrder,
    PositionType,
    TextAlign,
    TextDecoration,
    VerticalAlign,
);

impl ArenaValue for String {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        // Strings never enter the arena: a `Float64Array` cannot hold one. The
        // slot is an index into the side array the addon passes beside it.
        let slot = input.offset();
        let index = input.index()?;
        input.text(index, slot)
    }
}

impl ArenaValue for FontWeight {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        // Clamped rather than refused, as the byte codec does: a weight
        // outside the CSS range came from a writer that did not clamp, and a
        // browser handed the same value clamps it too.
        Ok(Self::new(u16::read(input)?))
    }
}

impl ArenaValue for TrackSize {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        let slot = input.offset();
        let tag = input.tag()?;
        let value = f32::read(input)?;
        match tag {
            0 => Ok(Self::Auto),
            1 => Ok(Self::Points(value)),
            2 => Ok(Self::Percent(value)),
            3 => Ok(Self::Fraction(value)),
            found => Err(ArenaError::UnknownTag {
                slot,
                what: "TrackSize",
                found: f64::from(found),
            }),
        }
    }
}

impl ArenaValue for Spacing {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        let slot = input.offset();
        let tag = input.tag()?;
        let value = f32::read(input)?;
        match tag {
            0 => Ok(Self::Normal),
            1 => Ok(Self::Points(value)),
            2 => Ok(Self::Em(value)),
            found => Err(ArenaError::UnknownTag {
                slot,
                what: "Spacing",
                found: f64::from(found),
            }),
        }
    }
}

impl ArenaValue for GridPlacement {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        Ok(Self {
            start: Option::<i16>::read(input)?,
            span: Option::<u16>::read(input)?,
        })
    }
}

impl ArenaValue for TextStroke {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        Ok(Self {
            width: f32::read(input)?,
            color: Color::read(input)?,
        })
    }
}

impl ArenaValue for GradientStop {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        Ok(Self {
            offset: f32::read(input)?,
            color: Color::read(input)?,
        })
    }
}

impl ArenaValue for Gradient {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        Ok(Self {
            kind: GradientKind::read(input)?,
            stops: Vec::read(input)?,
            angle_degrees: f32::read(input)?,
            center: <(Length, Length)>::read(input)?,
        })
    }
}

impl ArenaValue for ImageSource {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        let slot = input.offset();
        let tag = input.tag()?;
        let index = input.index()?;
        match tag {
            0 => Ok(Self::Path(input.text(index, slot)?)),
            1 => Ok(Self::Url(input.text(index, slot)?)),
            2 => Ok(Self::Bytes(input.bytes(index, slot)?)),
            found => Err(ArenaError::UnknownTag {
                slot,
                what: "ImageSource",
                found: f64::from(found),
            }),
        }
    }
}

impl ArenaValue for BackgroundImage {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        Ok(Self {
            source: ImageSource::read(input)?,
            repeat: BackgroundRepeat::read(input)?,
            size: <(Option<Length>, Option<Length>)>::read(input)?,
            position: <(Length, Length)>::read(input)?,
        })
    }
}

impl ArenaValue for Transform {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        Ok(Self {
            translate_x: Length::read(input)?,
            translate_y: Length::read(input)?,
            rotate_degrees: f32::read(input)?,
            scale_x: f32::read(input)?,
            scale_y: f32::read(input)?,
            origin: <(Length, Length)>::read(input)?,
        })
    }
}

impl ArenaValue for BoxShadow {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        Ok(Self {
            inset: bool::read(input)?,
            offset_x: f32::read(input)?,
            offset_y: f32::read(input)?,
            blur: f32::read(input)?,
            spread: f32::read(input)?,
            color: Color::read(input)?,
        })
    }
}

impl ArenaValue for TextShadow {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        Ok(Self {
            offset_x: f32::read(input)?,
            offset_y: f32::read(input)?,
            blur: f32::read(input)?,
            color: Color::read(input)?,
        })
    }
}

impl ArenaValue for Mask {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        let slot = input.offset();
        match input.tag()? {
            0 => Ok(Self::Image(ImageSource::read(input)?)),
            1 => Ok(Self::Shape(MaskShape::read(input)?)),
            2 => Ok(Self::Path {
                data: String::read(input)?,
                fill_rule: FillRule::read(input)?,
            }),
            3 => Ok(Self::Gradient(Gradient::read(input)?)),
            found => Err(ArenaError::UnknownTag {
                slot,
                what: "Mask",
                found: f64::from(found),
            }),
        }
    }
}

impl ArenaValue for PathPaint {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        let slot = input.offset();
        match input.tag()? {
            0 => Ok(Self::Solid(Color::read(input)?)),
            1 => Ok(Self::Gradient(Gradient::read(input)?)),
            found => Err(ArenaError::UnknownTag {
                slot,
                what: "PathPaint",
                found: f64::from(found),
            }),
        }
    }
}
