//! How one value of one type occupies `f64` slots.
//!
//! Every property in the arena is written through [`ArenaValue`], so the
//! encoder and the decoder agree about slot counts by sharing one definition
//! rather than by two lists matching. A type whose slot layout changes changes
//! in one place.
//!
//! Slot counts are not constant across types and not always constant within
//! one: a `Vec` writes its length and then its items, and an `Option` writes a
//! presence flag and then perhaps a value. So the reader is a cursor rather
//! than an index calculation, and a truncated arena is an error at the slot it
//! ran out on.

use meo_canvas_scene::{
    geometry::{Corners, Sides},
    style::{Dimension, Length, paint::Color},
};

use super::{ArenaError, Reader};

/// A value that crosses the boundary as `f64` slots.
pub(crate) trait ArenaValue: Sized {
    /// Reads one value, advancing the cursor past its slots.
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError>;
}

/// The tag an absent [`Option`] is written as.
const ABSENT: f64 = 0.0;

/// The tag a present [`Option`] is written as.
const PRESENT: f64 = 1.0;

impl ArenaValue for f32 {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        // Narrowing is the format: the arena is `f64` because JavaScript has
        // no other number, and every geometric quantity in a scene is `f32`.
        Ok(input.slot()? as Self)
    }
}

impl ArenaValue for bool {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        match input.tag()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(ArenaError::NotABoolean {
                slot: input.offset() - 1,
                found: f64::from(other),
            }),
        }
    }
}

impl ArenaValue for u16 {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        input
            .bounded_integer(Self::MAX.into())
            .map(|value| value as Self)
    }
}

impl ArenaValue for u32 {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        input
            .bounded_integer(Self::MAX.into())
            .map(|value| value as Self)
    }
}

impl ArenaValue for i16 {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        let value = input.integer()?;
        if value < Self::MIN.into() || value > Self::MAX.into() {
            return Err(ArenaError::OutOfRange {
                slot: input.offset() - 1,
                found: value,
            });
        }
        Ok(value as Self)
    }
}

impl ArenaValue for i32 {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        let value = input.integer()?;
        if value < Self::MIN.into() || value > Self::MAX.into() {
            return Err(ArenaError::OutOfRange {
                slot: input.offset() - 1,
                found: value,
            });
        }
        Ok(value as Self)
    }
}

impl<T: ArenaValue> ArenaValue for Option<T> {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        let slot = input.offset();
        match input.flag()? {
            tag if (tag - ABSENT).abs() < f64::EPSILON => Ok(None),
            tag if (tag - PRESENT).abs() < f64::EPSILON => {
                T::read(input).map(Some)
            }
            found => Err(ArenaError::NotAPresenceFlag { slot, found }),
        }
    }
}

impl<T: ArenaValue> ArenaValue for Vec<T> {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        let count = input.count()?;
        let mut values = Self::with_capacity(count);
        for _ in 0..count {
            values.push(T::read(input)?);
        }
        Ok(values)
    }
}

impl<A: ArenaValue, B: ArenaValue> ArenaValue for (A, B) {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        Ok((A::read(input)?, B::read(input)?))
    }
}

impl<T: ArenaValue> ArenaValue for Sides<T> {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        Ok(Self {
            top: T::read(input)?,
            right: T::read(input)?,
            bottom: T::read(input)?,
            left: T::read(input)?,
        })
    }
}

impl<T: ArenaValue> ArenaValue for Corners<T> {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        Ok(Self {
            top_left: T::read(input)?,
            top_right: T::read(input)?,
            bottom_right: T::read(input)?,
            bottom_left: T::read(input)?,
        })
    }
}

impl ArenaValue for Color {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        // One slot, packed `r<<24 | g<<16 | b<<8 | a`. Four channels is 32
        // bits, well inside the 53 a double holds exactly, and one slot rather
        // than four is three fewer stores per colour on the JavaScript side --
        // which is the side this format exists to make cheap.
        let packed = input.bounded_integer(f64::from(u32::MAX))? as u32;
        Ok(Self::rgba(
            ((packed >> 24) & 0xFF) as u8,
            ((packed >> 16) & 0xFF) as u8,
            ((packed >> 8) & 0xFF) as u8,
            (packed & 0xFF) as u8,
        ))
    }
}

impl ArenaValue for Length {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        let slot = input.offset();
        let tag = input.tag()?;
        let value = f32::read(input)?;
        match tag {
            0 => Ok(Self::Points(value)),
            1 => Ok(Self::Percent(value)),
            found => Err(ArenaError::UnknownTag {
                slot,
                what: "Length",
                found: f64::from(found),
            }),
        }
    }
}

impl ArenaValue for Dimension {
    fn read(input: &mut Reader<'_>) -> Result<Self, ArenaError> {
        let slot = input.offset();
        let tag = input.tag()?;
        let value = f32::read(input)?;
        match tag {
            // The value slot is written even for `Auto`, which has none, so
            // every `Dimension` is two slots wide. A JavaScript writer emits a
            // pair unconditionally rather than branching, and a fixed width is
            // what lets a reader skip a property it does not recognise.
            0 => Ok(Self::Auto),
            1 => Ok(Self::Points(value)),
            2 => Ok(Self::Percent(value)),
            found => Err(ArenaError::UnknownTag {
                slot,
                what: "Dimension",
                found: f64::from(found),
            }),
        }
    }
}
