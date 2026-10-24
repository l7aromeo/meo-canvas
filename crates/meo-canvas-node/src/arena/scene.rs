//! [`ArenaValue`] for the composite types a property can hold.
//!
//! Split from [`super::value`], which covers the primitives and the containers,
//! because these are the scene's own vocabulary rather than the format's.

use meo_canvas_scene::{
    OnImageError,
    node::{HttpOptions, ImageSource, LineCap, LineJoin, NodeTag, PathPaint},
    style::{
        Dimension, Length, PaintOrder,
        effect::{BoxShadow, FillRule, Mask, MaskShape, TextShadow, Transform},
        layout::{
            Align, BoxSizing, Direction, Display, FlexDirection, FlexWrap,
            GridAutoFlow, GridPlacement, Justify, JustifyItems, JustifySelf,
            LegacyPosition, Overflow, OverflowPosition, PositionType,
            SelfPosition, TrackSize,
        },
        paint::{
            BackgroundImage, BackgroundRepeat, BackgroundSize, BlendMode,
            BorderStyle, Color, Gradient, GradientGeometry, GradientKind,
            GradientStop, LinearDirection, ObjectFit,
        },
        text::{
            FontStyle, FontVariant, FontWeight, Spacing, TextAlign,
            TextDecoration, TextStroke, VerticalAlign,
        },
    },
    surface::{ColorSpace, ColorType},
};

use super::{ArenaError, Reader, value::ArenaValue};

/// Implements [`ArenaValue`] for a `wire_enum!` type: one slot holding the
/// number the byte codec writes as its discriminant, since both read
/// `from_wire`, so a keyword's number is identical in the two representations.
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
    ColorSpace,
    ColorType,
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
    LegacyPosition,
    LineCap,
    LineJoin,
    MaskShape,
    NodeTag,
    ObjectFit,
    OnImageError,
    Overflow,
    OverflowPosition,
    PaintOrder,
    PositionType,
    SelfPosition,
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

/// One `justify-self` or `justify-items` value, before its variant is chosen:
/// four slots, the byte codec's tag, a keyword, and an overflow flag and
/// keyword, each written even where unused, as a `Dimension`'s value is, so the
/// width is fixed.
struct Justification {
    tag: u32,
    /// Where the tag was, for the error naming it.
    tag_slot: usize,
    keyword: u8,
    keyword_slot: usize,
    overflow: Option<OverflowPosition>,
}

impl ArenaValue for Justification {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        let tag_slot = input.offset();
        let tag = input.tag()?;
        let keyword_slot = input.offset();
        let keyword = input.bounded_integer(f64::from(u8::MAX))? as u8;
        let present = bool::read(input)?;
        let overflow = OverflowPosition::read(input)?;
        Ok(Self {
            tag,
            tag_slot,
            keyword,
            keyword_slot,
            overflow: present.then_some(overflow),
        })
    }
}

impl Justification {
    /// The `<self-position>` its keyword names.
    fn position(&self) -> Result<SelfPosition, ArenaError> {
        SelfPosition::from_wire(self.keyword).ok_or_else(|| {
            ArenaError::UnknownTag {
                slot: self.keyword_slot,
                what: "SelfPosition",
                found: f64::from(self.keyword),
            }
        })
    }

    fn unknown(&self, what: &'static str) -> ArenaError {
        ArenaError::UnknownTag {
            slot: self.tag_slot,
            what,
            found: f64::from(self.tag),
        }
    }
}

impl ArenaValue for JustifySelf {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        let read = Justification::read(input)?;
        match read.tag {
            0 => Ok(Self::Normal),
            1 => Ok(Self::Stretch),
            2 => Ok(Self::Baseline),
            3 => Ok(Self::LastBaseline),
            4 => Ok(Self::Position {
                position: read.position()?,
                overflow: read.overflow,
            }),
            _ => Err(read.unknown("JustifySelf")),
        }
    }
}

impl ArenaValue for JustifyItems {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        let read = Justification::read(input)?;
        match read.tag {
            0 => Ok(Self::Normal),
            1 => Ok(Self::Stretch),
            2 => Ok(Self::Baseline),
            3 => Ok(Self::LastBaseline),
            4 => Ok(Self::Position {
                position: read.position()?,
                overflow: read.overflow,
            }),
            5 => LegacyPosition::from_wire(read.keyword)
                .map(Self::Legacy)
                .ok_or_else(|| ArenaError::UnknownTag {
                    slot: read.keyword_slot,
                    what: "LegacyPosition",
                    found: f64::from(read.keyword),
                }),
            _ => Err(read.unknown("JustifyItems")),
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

impl ArenaValue for LinearDirection {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        let slot = input.offset();
        match input.tag()? {
            0 => Ok(Self::Angle(f32::read(input)?)),
            1 => Ok(Self::Between {
                start: <(Length, Length)>::read(input)?,
                end: <(Length, Length)>::read(input)?,
            }),
            found => Err(ArenaError::UnknownTag {
                slot,
                what: "LinearDirection",
                found: f64::from(found),
            }),
        }
    }
}

impl ArenaValue for GradientGeometry {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        // The tag is `GradientKind`, the same fieldless enum the codec writes
        // and the generator emits, so there is one list of which shapes exist.
        Ok(match GradientKind::read(input)? {
            GradientKind::Linear => Self::Linear {
                direction: LinearDirection::read(input)?,
            },
            GradientKind::Radial => Self::Radial {
                at: <(Length, Length)>::read(input)?,
            },
            GradientKind::Conic => Self::Conic {
                at: <(Length, Length)>::read(input)?,
                from: f32::read(input)?,
            },
        })
    }
}

impl ArenaValue for Gradient {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        Ok(Self {
            geometry: GradientGeometry::read(input)?,
            stops: Vec::read(input)?,
        })
    }
}

impl ArenaValue for BackgroundSize {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        let slot = input.offset();
        match input.tag()? {
            0 => Ok(Self::PerAxis(
                Dimension::read(input)?,
                Dimension::read(input)?,
            )),
            1 => Ok(Self::Cover),
            2 => Ok(Self::Contain),
            found => Err(ArenaError::UnknownTag {
                slot,
                what: "BackgroundSize",
                found: f64::from(found),
            }),
        }
    }
}

impl ArenaValue for ImageSource {
    /// A tag, one side-array index, and -- for a URL -- its options after it.
    /// The index is read inside each arm, since the `Url` arm's header count
    /// follows it and the other arms have none.
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        let slot = input.offset();
        let tag = input.tag()?;
        match tag {
            0 => {
                let index = input.index()?;
                Ok(Self::Path(input.text(index, slot)?))
            }
            1 => {
                let index = input.index()?;
                let url = input.text(index, slot)?;
                Ok(Self::Url {
                    url,
                    http: HttpOptions::read(input)?,
                })
            }
            2 => {
                let index = input.index()?;
                Ok(Self::Bytes(input.bytes(index, slot)?))
            }
            found => Err(ArenaError::UnknownTag {
                slot,
                what: "ImageSource",
                found: f64::from(found),
            }),
        }
    }
}

impl ArenaValue for HttpOptions {
    /// A header count, then a name and a value per header. Nothing is reserved
    /// from the count: two slots per pair back two `String`s in memory, so the
    /// loop grows instead, and a short arena fails on the read that runs out.
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        let count = input.count()?;
        let mut headers = Vec::new();
        for _ in 0..count {
            let slot = input.offset();
            let name = input.index()?;
            let name = input.text(name, slot)?;
            let slot = input.offset();
            let value = input.index()?;
            headers.push((name, input.text(value, slot)?));
        }
        Ok(Self { headers })
    }
}

impl ArenaValue for BackgroundImage {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        Ok(Self {
            source: ImageSource::read(input)?,
            repeat: BackgroundRepeat::read(input)?,
            size: BackgroundSize::read(input)?,
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
