//! The `f64` arena: the format JavaScript hands a scene across in.
//!
//! One `Float64Array` carries the whole scene. Strings and buffers cannot live
//! in a `Float64Array`, so they go into a side array and the arena stores an
//! index into it. Both this and [`meo_canvas_scene::codec`] produce the same
//! [`Scene`], so a scene captured here and written to disk round-trips without
//! loss.
//!
//! It is shaped this way because reading a value out of V8 is what costs, not
//! the crossing: a `lineTo` in `meo-skia-canvas` costs 82 nanoseconds, of which
//! 17 is the crossing and 39 is reading two floats out of the arguments.
//! Decoding from a `&[f64]` skips V8 entirely, and a store into a
//! `Float64Array` is one operation where writing varint bytes from JavaScript
//! is several.
//!
//! # Wire specification
//!
//! Every slot is an `f64`. There is no alignment and no padding; each value
//! begins at the slot after the previous one ends. Counts, indices, tags and
//! masks are integers stored as doubles and must be exact — a fractional value
//! where an integer is expected is an error, not a rounding.
//!
//! ```text
//! arena  := MAGIC VERSION f32(width) f32(height) f32(scale) surface
//!           list<node>(pages)
//! surface := opt<bool>(gpu) opt<enum>(color_type) opt<enum>(color_space)
//! node   := enum(kind)
//!           mask(layout) mask(paint) mask(text) mask(effects)
//!           layout-values paint-values text-values effect-values
//!           kind-payload
//!           opt<str>(name)
//!           list<node>(children)
//! ```
//!
//! ## Primitives
//!
//! | Form | Slots |
//! | --- | --- |
//! | `f32` | 1, narrowed from the double |
//! | `bool` | 1: `0` false, `1` true, anything else an error |
//! | `u16` `u32` `i16` `i32` | 1, and out of range is an error |
//! | `enum` | 1, the same discriminant the byte codec writes |
//! | `str` | 1: an index into the side values array |
//! | `bytes` | 1: an index into the side values array |
//! | `opt<T>` | 1 (`0`, and nothing follows) or 1 + `T` (`1`) |
//! | `list<T>` | 1 count, then that many `T` |
//! | `color` | 1, packed `r<<24 \| g<<16 \| b<<8 \| a` |
//! | `length` | 2: tag (`0` points, `1` percent) then the value |
//! | `dim` | 2: tag (`0` auto, `1` points, `2` percent) then the value, which is written even for `auto` |
//! | `track` | 2: tag (`0` auto, `1` points, `2` percent, `3` fraction) then the value |
//! | `spacing` | 2: tag (`0` normal, `1` points, `2` em) then the value |
//! | `sides<T>` | 4 × `T`, in `top right bottom left` order |
//! | `corners<T>` | 4 × `T`, in `top-left top-right bottom-right bottom-left` order |
//!
//! ## Masks
//!
//! A group's mask says which of its properties the record carries, and only
//! those are written. A node setting five properties costs five values rather
//! than every property the kind has.
//!
//! **A mask slot holds 53 bits, not 64.** A double is exact on integers only to
//! 2^53, so the 54th bit of a mask packed into one slot is lost with no
//! rounding a reader could detect. Two slots name 106 properties; a group
//! passing 106 takes a third. Every table asserts this at compile time.
//!
//! Each group's mask is `ceil(count / 53)` slots, and today every group is one
//! slot. The property counts are `layout::COUNT` (30), `paint::COUNT` (11),
//! `text::COUNT` (15) and `effects::COUNT` (6) -- the tables further down this
//! module. A writer computes its slot widths from those numbers rather than
//! hard-coding a width, and `every_group_fits_its_mask` fails if a table's
//! count changes without this list following it.
//!
//! Present properties are written in **ascending index order**, which the
//! tables assert at compile time — a table out of order would read the right
//! number of slots into the wrong fields, which no length check catches.
//!
//! ## Kind payloads
//!
//! ```text
//! payload(Box)   := (nothing)
//! payload(Text)  := opt<u32>(max_lines) opt<str>(ellipsis) text-content
//! text-content   := 1 str(markup) | 0 list<segment>
//! segment        := str(text) mask(text) text-values
//! payload(Image) := source enum(fit) length length opt<u32>(frame)
//! payload(Path)  := str(d) opt<paint> opt<paint> f32(line_width)
//!                   enum(fill_rule) enum(cap) enum(join) list<f32>(dash)
//!                   f32(dash_offset)
//!
//! source := 0 str | 1 str | 2 bytes        -- Path | Url | Bytes
//! paint  := 0 color | 1 gradient
//! ```
//!
//! ### Why a text node says which of the two it carries
//!
//! A paragraph reaches the arena in one of two states, and they are not
//! distinguishable once written. `Text("a <b>b</b>")` is a string the caller
//! expects to be *parsed*; `RichText([...])` is runs the caller built and
//! expects left alone -- and rich text of one run is byte-identical to plain
//! text of one run. Without a discriminant the decoder has to guess, and either
//! guess loses something: parse everything and `RichText` can no longer carry a
//! literal `<`, parse nothing and a JavaScript caller has no rich text at all,
//! which is what v1 gave them and what v2 took away.
//!
//! So the payload says which. It is spelled as the `opt<str>` the format
//! already has rather than as a new tag: present means "parse this", absent
//! means "the segments follow", and that is one slot either way. A tag would be
//! the same slot **plus** a keyword both sides must agree on, and a keyword
//! hand-carried into TypeScript is a disagreement nothing reports.
//!
//! The parse happens here, on the way in, through
//! [`meo_canvas_core::markup::parse_paragraph`] -- the same function the Rust
//! facade's `Text::new` calls, so `Text("")` cannot mean two things.
//! [`meo_canvas_scene::Scene`] holds segments either way, so the byte format is
//! untouched and both representations still decode to one scene.
//!
//! ## What a version bump is for
//!
//! [`VERSION`] changes when a reader of the current revision would misread a
//! newer arena: a new node kind, a property inserted rather than appended, or a
//! slot layout widened. Appending a property to a table is not such a change —
//! its bit is simply never set by an older writer.

#[cfg(test)]
pub(crate) mod cases;
pub(crate) mod group;
pub(crate) mod scene;
pub(crate) mod value;

use group::{BITS_PER_SLOT, Mask, arena_group, ascending};
use meo_canvas_scene::{
    Scene, SceneError, Size,
    node::{
        ImageSource, LineCap, LineJoin, Node, NodeId, NodeKind, NodeTag,
        PathPaint,
    },
    style::{
        Length,
        effect::FillRule,
        paint::ObjectFit,
        text::{ParagraphStyle, TextSegment},
    },
    surface::{ColorSpace, ColorType},
};
use value::ArenaValue;

/// The number every arena starts with.
///
/// `MCAR` read as four big-endian bytes. A recognisable constant rather than a
/// version alone, so an arena assembled from the wrong buffer fails at slot
/// zero instead of decoding into a plausible scene.
pub const MAGIC: f64 = 1_296_649_810.0;

/// The revision this crate reads.
pub const VERSION: f64 = 4.0;

/// The largest node count [`decode`] will allocate for.
///
/// The same bound the byte format uses, and for the same reason: the count is
/// read before any node is, so a corrupt slot would otherwise reserve whatever
/// it happened to say.
pub const MAX_NODES: usize = 1 << 20;

/// One value the arena could not carry itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SideValue {
    /// A string.
    Text(String),
    /// A buffer.
    Bytes(Vec<u8>),
}

/// The side array an arena's indices point into.
///
/// Built once by the addon from the JavaScript array. Owning it rather than
/// borrowing V8 handles is what lets the decoder be plain Rust and be tested
/// without a Node process.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Values(Vec<SideValue>);

impl Values {
    /// Wraps a list of side values.
    #[must_use]
    pub const fn new(values: Vec<SideValue>) -> Self {
        Self(values)
    }

    /// How many values the array holds.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the array is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// What can be wrong with an arena.
#[derive(Debug, Clone, PartialEq)]
pub enum ArenaError {
    /// The first slot is not [`MAGIC`].
    NotAnArena {
        /// What the first slot held.
        found: f64,
    },
    /// The arena is well formed but written by another revision.
    UnsupportedVersion {
        /// The revision the arena claims.
        found: f64,
        /// The revision this crate reads.
        expected: f64,
    },
    /// The arena ends where a value was expected.
    Truncated {
        /// The slot the read was attempted at.
        slot: usize,
        /// How many slots the arena holds.
        length: usize,
    },
    /// A slot that must hold an exact integer held something else.
    ///
    /// A double carries fractions, so "the writer computed an index" and "the
    /// writer computed a coordinate" are the same type on the wire and only
    /// this check separates them.
    NotAnInteger {
        /// The slot.
        slot: usize,
        /// What it held.
        found: f64,
    },
    /// An integer outside the range its field admits.
    OutOfRange {
        /// The slot.
        slot: usize,
        /// What it held.
        found: f64,
    },
    /// A slot that must be `0` or `1` held neither.
    NotABoolean {
        /// The slot.
        slot: usize,
        /// What it held.
        found: f64,
    },
    /// An option's presence flag was neither `0` nor `1`.
    NotAPresenceFlag {
        /// The slot.
        slot: usize,
        /// What it held.
        found: f64,
    },
    /// A discriminant that names no variant.
    UnknownTag {
        /// The slot.
        slot: usize,
        /// The type being read.
        what: &'static str,
        /// What it held.
        found: f64,
    },
    /// An index into the side array that names nothing there.
    NoSuchValue {
        /// The slot the index was read from.
        slot: usize,
        /// The index.
        index: usize,
        /// How many values the side array holds.
        length: usize,
    },
    /// A side value of the wrong kind for the property that named it.
    WrongValueKind {
        /// The slot the index was read from.
        slot: usize,
        /// What the property wanted.
        wanted: &'static str,
    },
    /// A node count above [`MAX_NODES`].
    TooManyNodes {
        /// The count the arena declared.
        found: usize,
        /// The largest this crate allocates for.
        limit: usize,
    },
    /// The arena decoded, but the tree it describes is not one.
    InvalidScene(SceneError),
    /// Slots remain after the scene ends.
    TrailingSlots {
        /// How many were left over.
        count: usize,
    },
}

impl core::fmt::Display for ArenaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotAnArena { found } => {
                write!(f, "the first slot is {found}, not the arena magic")
            }
            Self::UnsupportedVersion { found, expected } => {
                write!(f, "arena version {found}, expected {expected}")
            }
            Self::Truncated { slot, length } => write!(
                f,
                "the arena ends at slot {length}; slot {slot} was wanted"
            ),
            Self::NotAnInteger { slot, found } => {
                write!(f, "slot {slot} holds {found}, which is not an integer")
            }
            Self::OutOfRange { slot, found } => {
                write!(
                    f,
                    "slot {slot} holds {found}, outside the field's range"
                )
            }
            // One arm for two variants: the message is the same because the
            // mistake is, and the variants stay distinct because a caller
            // matching on them wants to know which field lied.
            Self::NotABoolean { slot, found }
            | Self::NotAPresenceFlag { slot, found } => {
                write!(f, "slot {slot} holds {found}, which is not 0 or 1")
            }
            Self::UnknownTag { slot, what, found } => {
                write!(f, "slot {slot} holds {found}, which names no {what}")
            }
            Self::NoSuchValue {
                slot,
                index,
                length,
            } => write!(f, "slot {slot} names side value {index} of {length}"),
            Self::WrongValueKind { slot, wanted } => {
                write!(f, "slot {slot} names a side value that is not {wanted}")
            }
            Self::TooManyNodes { found, limit } => {
                write!(
                    f,
                    "the arena declares {found} nodes, the limit is {limit}"
                )
            }
            Self::InvalidScene(error) => {
                write!(f, "the arena decodes but is not a scene: {error}")
            }
            Self::TrailingSlots { count } => {
                write!(f, "{count} slots remain after the scene ends")
            }
        }
    }
}

impl core::error::Error for ArenaError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::InvalidScene(error) => Some(error),
            _ => None,
        }
    }
}

/// A cursor over an arena and the side array beside it.
#[derive(Debug)]
pub(crate) struct Reader<'a> {
    slots: &'a [f64],
    values: &'a Values,
    offset: usize,
    /// The values a probe stream hands back, if this reader is one.
    ///
    /// `None` for every reader outside a probe, which is every reader the
    /// addon builds.
    #[cfg(test)]
    probe: Option<ProbeFills>,
}

/// What a probe hands back, by what the read asks for.
///
/// A probe stream cannot be one number. `bounded_integer` refuses a fractional
/// slot, so a tag, a count, an enum index and a side index all have to be whole
/// — and a fill that satisfies them is `1.0`, which is the one value at which a
/// hundredfold units error between the two surfaces encodes identically.
/// `Length::Percent(1.0)` is `'100%'`, and `'1%'` written without the division
/// is the same number, so a probe of it agrees with a writer that never
/// divides. That blind spot has already shipped a user-visible bug: `'50%'`
/// rendered at five thousand per cent, and every check in this project passed.
///
/// So the probe answers by what the reader wants rather than by one constant:
/// whole where a whole number is required, fractional where the slot is taken
/// as it is. The fractional half is where a percentage, an offset, a radius and
/// a scale factor all land.
#[cfg(test)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProbeFills {
    /// Handed to a read that demands a whole number.
    integer: f64,
    /// Handed to a read that takes the slot as written.
    fraction: f64,
}

/// How many ones a probe stream carries.
///
/// Thirty-two, comfortably past the widest single property. The largest is
/// `Option<BackgroundImage>` at fourteen slots -- a presence flag, a tagged
/// source with its side index, a repeat mode, two optional lengths and two
/// lengths -- and `Sides<Option<Length>>` is twelve. A stream longer than a
/// property needs costs nothing, because a read stops when its value is
/// complete; one slot short and the probe silently has no value, which is what
/// `every_property_has_a_probe_that_differs_from_its_default` catches.
#[cfg(test)]
const PROBE_SLOTS: usize = 32;

/// The value a probe stream is filled with.
///
/// One: the smallest fill that is a valid reading of every tagged type, since
/// every enum and every tag in the format has at least two variants.
#[cfg(test)]
const PROBE_FILL: f64 = 1.0;

/// The value handed to a read that takes its slot as written.
///
/// A quarter, and the two properties of it that matter are that it is exact in
/// an `f32` — so no case's expectation carries a rounding argument — and that
/// it is neither of the two numbers at which a hundredfold units error is
/// invisible. `1.0` is `'100%'` and is also what `'1%'` becomes if the division
/// is forgotten; `0.0` is a fixed point of any scaling at all.
#[cfg(test)]
const PROBE_FRACTION: f64 = 0.25;

/// The second pair, tried where the first lands on a property's own default.
///
/// Nothing needs it today: the only two properties that defaulted to the first
/// fill were `flex_shrink` and `opacity` at `1.0`, and both are read through
/// the fractional half, which is now a quarter. It is kept because the escape
/// hatch is the mechanism rather than the constant — a property added with a
/// default of `0.25` would have no probe at all, and
/// `every_property_has_a_probe_that_differs_from_its_default` would say so
/// rather than a case quietly exercising no write path.
///
/// Both halves differ from the first pair, since either could be the one that
/// collides. `2` is still a valid reading of every tagged type, and `0.5` is
/// still exact in an `f32`.
#[cfg(test)]
const PROBE_FILL_ALTERNATE: ProbeFills = ProbeFills {
    integer: 2.0,
    fraction: 0.5,
};

/// The pairs a probe tries, in order.
///
/// Trying rather than listing which property needs which: a list would be a
/// second table to keep in step, and
/// `every_property_has_a_probe_that_differs_from_its_default` fails if neither
/// pair produces a distinguishable value.
#[cfg(test)]
pub(crate) const fn probe_fills() -> [ProbeFills; 2] {
    [
        ProbeFills {
            integer: PROBE_FILL,
            fraction: PROBE_FRACTION,
        },
        PROBE_FILL_ALTERNATE,
    ]
}

/// The slot stream and side array a probe value is read out of.
///
/// A stream of one value throughout, which is a valid reading for every type
/// in the format: a number is the fill, a `bool` is true, an enum takes the
/// variant at that index, an `Option` is present, a list holds that many items,
/// and a tagged value takes that tag. Two side values so a string index of one
/// resolves.
///
/// What each read actually receives is decided by [`ProbeFills`], not by these
/// slots. See [`probe_fills`].
#[cfg(test)]
pub(crate) fn probe_slots() -> ([f64; PROBE_SLOTS], Values) {
    (
        // The contents no longer decide the values a probe reads -- the reader
        // substitutes by what each read asks for -- but the length still does:
        // a stream shorter than a property is a truncation, which is the check
        // that a probe reaching no value is caught.
        [PROBE_FILL; PROBE_SLOTS],
        Values::new(vec![
            SideValue::Text("probe".to_owned()),
            SideValue::Text("probe".to_owned()),
        ]),
    )
}

impl<'a> Reader<'a> {
    const fn new(slots: &'a [f64], values: &'a Values) -> Self {
        Self {
            slots,
            values,
            offset: 0,
            #[cfg(test)]
            probe: None,
        }
    }

    /// A reader over a probe stream.
    ///
    /// Separate constructor rather than making [`Reader::new`] visible, so the
    /// only thing outside this module that can build a reader is the probe.
    #[cfg(test)]
    pub(crate) const fn new_for_probe(
        slots: &'a [f64],
        values: &'a Values,
        fills: ProbeFills,
    ) -> Self {
        Self {
            slots,
            values,
            offset: 0,
            probe: Some(fills),
        }
    }

    /// The slot the next read starts at.
    pub(crate) const fn offset(&self) -> usize {
        self.offset
    }

    /// How many slots are left.
    pub(crate) const fn remaining(&self) -> usize {
        self.slots.len() - self.offset
    }

    /// Takes one slot.
    pub(crate) fn slot(&mut self) -> Result<f64, ArenaError> {
        let value = self.raw_slot()?;
        #[cfg(test)]
        if let Some(fills) = self.probe {
            return Ok(fills.fraction);
        }
        Ok(value)
    }

    /// Takes the next slot exactly as the stream holds it.
    ///
    /// The bounds check and the offset advance, with no probe substitution.
    /// Every read goes through here so a truncated stream is still an error
    /// during a probe — the length is what
    /// `every_property_has_a_probe_that_differs_from_its_default` rests on.
    fn raw_slot(&mut self) -> Result<f64, ArenaError> {
        let value = self.slots.get(self.offset).copied().ok_or(
            ArenaError::Truncated {
                slot: self.offset,
                length: self.slots.len(),
            },
        )?;
        self.offset += 1;
        Ok(value)
    }

    /// Takes a presence flag, which is a whole number.
    ///
    /// Separate from [`Reader::slot`] so a probe can tell the two apart: the
    /// fractional fill is for slots read as written, and a presence flag is not
    /// one of those. The value is returned unvalidated so the caller still
    /// reports [`ArenaError::NotAPresenceFlag`] for anything that is neither
    /// present nor absent, which names the mistake better than "not an
    /// integer" would.
    pub(crate) fn flag(&mut self) -> Result<f64, ArenaError> {
        let value = self.raw_slot()?;
        #[cfg(test)]
        let value = self.probe.map_or(value, |fills| fills.integer);
        Ok(value)
    }

    /// Takes a slot that must hold an exact integer.
    pub(crate) fn integer(&mut self) -> Result<f64, ArenaError> {
        let slot = self.offset;
        let value = self.raw_slot()?;
        // A probe answers a demand for a whole number with one, whatever the
        // stream holds: the fractional fill exists for the slots that are read
        // as written, and a tag read as `0.25` is a stream nobody writes.
        #[cfg(test)]
        let value = self.probe.map_or(value, |fills| fills.integer);
        if !value.is_finite() || value.fract() != 0.0 {
            return Err(ArenaError::NotAnInteger { slot, found: value });
        }
        Ok(value)
    }

    /// Takes a small non-negative tag.
    ///
    /// Separate from [`Reader::integer`] so a `match` is written against whole
    /// numbers rather than against `f64` patterns, which do not express what a
    /// discriminant is.
    pub(crate) fn tag(&mut self) -> Result<u32, ArenaError> {
        self.bounded_integer(f64::from(u32::MAX))
            .map(|value| value as u32)
    }

    /// Takes an exact integer within `0..=largest`.
    pub(crate) fn bounded_integer(
        &mut self,
        largest: f64,
    ) -> Result<f64, ArenaError> {
        let slot = self.offset;
        let value = self.integer()?;
        if value < 0.0 || value > largest {
            return Err(ArenaError::OutOfRange { slot, found: value });
        }
        Ok(value)
    }

    /// Takes a count, bounded by the slots that could possibly back it.
    ///
    /// Every element costs at least one slot, so a count above the remaining
    /// length is a corrupt slot rather than a large list, and reserving for it
    /// would honour a number the arena cannot back.
    pub(crate) fn count(&mut self) -> Result<usize, ArenaError> {
        let slot = self.offset;
        let value = self.integer()?;
        let remaining = self.remaining();
        if value < 0.0 || value > remaining as f64 {
            return Err(ArenaError::OutOfRange { slot, found: value });
        }
        Ok(value as usize)
    }

    /// Takes an index into the side array.
    ///
    /// Bounded by what a `u32` holds rather than by the slots remaining, as
    /// [`Reader::count`] is: an index names a place in the *other* array, so
    /// the arena's own length says nothing about whether it is plausible. The
    /// bound that matters is the side array's length, and missing it there is
    /// [`ArenaError::NoSuchValue`], which names the index a writer got wrong.
    pub(crate) fn index(&mut self) -> Result<usize, ArenaError> {
        self.bounded_integer(f64::from(u32::MAX))
            .map(|value| value as usize)
    }

    /// The string at a side-array index.
    pub(crate) fn text(
        &self,
        index: usize,
        slot: usize,
    ) -> Result<String, ArenaError> {
        match self.values.0.get(index) {
            Some(SideValue::Text(text)) => Ok(text.clone()),
            Some(SideValue::Bytes(_)) => Err(ArenaError::WrongValueKind {
                slot,
                wanted: "a string",
            }),
            None => Err(ArenaError::NoSuchValue {
                slot,
                index,
                length: self.values.0.len(),
            }),
        }
    }

    /// The buffer at a side-array index.
    pub(crate) fn bytes(
        &self,
        index: usize,
        slot: usize,
    ) -> Result<Vec<u8>, ArenaError> {
        match self.values.0.get(index) {
            Some(SideValue::Bytes(bytes)) => Ok(bytes.clone()),
            Some(SideValue::Text(_)) => Err(ArenaError::WrongValueKind {
                slot,
                wanted: "a buffer",
            }),
            None => Err(ArenaError::NoSuchValue {
                slot,
                index,
                length: self.values.0.len(),
            }),
        }
    }
}

arena_group! {
    /// Everything the layout pass reads.
    pub(crate) mod layout for meo_canvas_scene::style::layout::LayoutStyle {
        0 => display: meo_canvas_scene::style::layout::Display,
        1 => position_type: meo_canvas_scene::style::layout::PositionType,
        2 => inset: meo_canvas_scene::Sides<Option<Length>>,
        3 => size: (meo_canvas_scene::Dimension, meo_canvas_scene::Dimension),
        4 => min_size: (meo_canvas_scene::Dimension, meo_canvas_scene::Dimension),
        5 => max_size: (meo_canvas_scene::Dimension, meo_canvas_scene::Dimension),
        6 => aspect_ratio: Option<f32>,
        7 => margin: meo_canvas_scene::Sides<meo_canvas_scene::Dimension>,
        8 => padding: meo_canvas_scene::Sides<Length>,
        9 => border: meo_canvas_scene::Sides<f32>,
        10 => flex_direction: meo_canvas_scene::style::layout::FlexDirection,
        11 => flex_wrap: meo_canvas_scene::style::layout::FlexWrap,
        12 => flex_grow: f32,
        13 => flex_shrink: f32,
        14 => flex_basis: meo_canvas_scene::Dimension,
        15 => justify_content: Option<meo_canvas_scene::style::layout::Justify>,
        16 => align_items: Option<meo_canvas_scene::style::layout::Align>,
        17 => align_self: Option<meo_canvas_scene::style::layout::Align>,
        18 => align_content: Option<meo_canvas_scene::style::layout::Align>,
        19 => gap: (Length, Length),
        20 => overflow: (
            meo_canvas_scene::style::layout::Overflow,
            meo_canvas_scene::style::layout::Overflow
        ),
        21 => box_sizing: meo_canvas_scene::style::layout::BoxSizing,
        22 => direction: meo_canvas_scene::style::layout::Direction,
        23 => grid_template_columns: Vec<meo_canvas_scene::style::layout::TrackSize>,
        24 => grid_template_rows: Vec<meo_canvas_scene::style::layout::TrackSize>,
        25 => grid_auto_rows: Option<meo_canvas_scene::style::layout::TrackSize>,
        26 => grid_auto_columns: Option<meo_canvas_scene::style::layout::TrackSize>,
        27 => grid_auto_flow: meo_canvas_scene::style::layout::GridAutoFlow,
        28 => grid_column: meo_canvas_scene::style::layout::GridPlacement,
        29 => grid_row: meo_canvas_scene::style::layout::GridPlacement,
    }
}

arena_group! {
    /// Everything that fills, outlines or composites the box.
    pub(crate) mod paint for meo_canvas_scene::style::paint::PaintStyle {
        0 => background_color: meo_canvas_scene::style::paint::Color,
        1 => gradient: Option<meo_canvas_scene::style::paint::Gradient>,
        2 => background_image: Option<meo_canvas_scene::style::paint::BackgroundImage>,
        3 => border_color: meo_canvas_scene::Sides<Option<meo_canvas_scene::style::paint::Color>>,
        4 => border_color_all: meo_canvas_scene::style::paint::Color,
        5 => border_style: meo_canvas_scene::style::paint::BorderStyle,
        6 => border_radius: meo_canvas_scene::Corners<f32>,
        7 => opacity: f32,
        8 => blend_mode: meo_canvas_scene::style::paint::BlendMode,
        9 => dither: bool,
        10 => z_index: Option<i32>,
    }
}

arena_group! {
    /// Glyph styling, which inherits to descendants.
    pub(crate) mod text for meo_canvas_scene::style::text::TextStyle {
        0 => font_family: Option<String>,
        1 => font_size: Option<f32>,
        2 => font_weight: Option<meo_canvas_scene::style::text::FontWeight>,
        3 => font_style: Option<meo_canvas_scene::style::text::FontStyle>,
        4 => color: Option<meo_canvas_scene::style::paint::Color>,
        5 => text_align: Option<meo_canvas_scene::style::text::TextAlign>,
        6 => text_decoration: Option<meo_canvas_scene::style::text::TextDecoration>,
        7 => vertical_align: Option<meo_canvas_scene::style::text::VerticalAlign>,
        8 => paint_order: Option<meo_canvas_scene::style::PaintOrder>,
        9 => line_height: Option<f32>,
        10 => line_gap: Option<f32>,
        11 => letter_spacing: Option<meo_canvas_scene::style::text::Spacing>,
        12 => word_spacing: Option<meo_canvas_scene::style::text::Spacing>,
        13 => font_variant: Option<Vec<meo_canvas_scene::style::text::FontVariant>>,
        14 => text_stroke: Option<meo_canvas_scene::style::text::TextStroke>,
    }
}

arena_group! {
    /// What is applied after the node and its children are drawn.
    pub(crate) mod effects for meo_canvas_scene::style::effect::Effects {
        0 => transform: Option<meo_canvas_scene::style::effect::Transform>,
        1 => box_shadows: Vec<meo_canvas_scene::style::effect::BoxShadow>,
        2 => text_shadows: Vec<meo_canvas_scene::style::effect::TextShadow>,
        3 => mask: Option<meo_canvas_scene::style::effect::Mask>,
        4 => filter: Option<String>,
        5 => backdrop_filter: Option<String>,
    }
}

/// Reads a scene out of an arena and its side values.
///
/// Every child index and page root is checked against the arena the decoder
/// built, so a scene from here always satisfies [`Scene::validate`].
///
/// # Errors
///
/// Returns [`ArenaError`] if the arena is not one, was written by another
/// revision, ends early, holds a slot the format cannot read, names a side
/// value that is not there, describes something that is not a forest of pages,
/// or carries slots past the end of the scene.
pub fn decode(slots: &[f64], values: &Values) -> Result<Scene, ArenaError> {
    let mut input = Reader::new(slots, values);

    let magic = input.slot()?;
    if (magic - MAGIC).abs() > f64::EPSILON {
        return Err(ArenaError::NotAnArena { found: magic });
    }
    let version = input.slot()?;
    if (version - VERSION).abs() > f64::EPSILON {
        return Err(ArenaError::UnsupportedVersion {
            found: version,
            expected: VERSION,
        });
    }

    let width = f32::read(&mut input)?;
    let height = f32::read(&mut input)?;
    let scale = f32::read(&mut input)?;
    // The surface's own description, between the geometry and the pages. Slots
    // inserted rather than appended, which is why `VERSION` moved to 2: an
    // older reader would take `gpu`'s discriminant for the page count.
    let gpu = Option::<bool>::read(&mut input)?;
    let color_type = Option::<ColorType>::read(&mut input)?;
    let color_space = Option::<ColorSpace>::read(&mut input)?;

    let page_count = input.count()?;
    let mut scene = Scene {
        size: Size::new(width, height),
        scale,
        gpu,
        color_type,
        color_space,
        nodes: Vec::new(),
        pages: Vec::with_capacity(page_count),
    };
    for _ in 0..page_count {
        let root = read_node(&mut input, &mut scene)?;
        scene.pages.push(root);
    }

    let remaining = input.remaining();
    if remaining != 0 {
        return Err(ArenaError::TrailingSlots { count: remaining });
    }

    scene.validate().map_err(ArenaError::InvalidScene)?;
    Ok(scene)
}

/// Reads one node and its subtree, appending them to the arena.
///
/// Recursion follows the record's own nesting, and the depth is bounded by
/// [`MAX_NODES`] because every node costs at least one slot and the count is
/// checked before each push.
fn read_node(
    input: &mut Reader<'_>,
    scene: &mut Scene,
) -> Result<NodeId, ArenaError> {
    let kind_tag = NodeTag::read(input)?;

    let layout_mask = Mask::read(input, layout::SLOTS)?;
    let paint_mask = Mask::read(input, paint::SLOTS)?;
    let text_mask = Mask::read(input, text::SLOTS)?;
    let effects_mask = Mask::read(input, effects::SLOTS)?;

    let layout = layout::read(&layout_mask, input)?;
    let paint = paint::read(&paint_mask, input)?;
    let text = text::read(&text_mask, input)?;
    let effects = effects::read(&effects_mask, input)?;

    let kind = read_kind(input, kind_tag)?;
    let name = Option::<String>::read(input)?;

    if scene.nodes.len() >= MAX_NODES {
        return Err(ArenaError::TooManyNodes {
            found: scene.nodes.len() + 1,
            limit: MAX_NODES,
        });
    }
    // The cast is exact: the bound above is `MAX_NODES`, well inside a `u32`.
    let id = NodeId::new(scene.nodes.len() as u32);
    scene.nodes.push(Node {
        kind,
        layout,
        paint,
        text,
        effects,
        children: Vec::new(),
        name,
    });

    let child_count = input.count()?;
    let mut children = Vec::with_capacity(child_count);
    for _ in 0..child_count {
        children.push(read_node(input, scene)?);
    }
    if let Some(node) = scene.get_mut(id) {
        node.children = children;
    }
    Ok(id)
}

/// Reads runs the caller built, which the decoder does not interpret.
fn read_segments(
    input: &mut Reader<'_>,
) -> Result<Vec<TextSegment>, ArenaError> {
    let count = input.count()?;
    let mut segments = Vec::with_capacity(count);
    for _ in 0..count {
        let content = String::read(input)?;
        let mask = Mask::read(input, text::SLOTS)?;
        segments.push(TextSegment {
            text: content,
            style: text::read(&mask, input)?,
        });
    }
    Ok(segments)
}

fn read_kind(
    input: &mut Reader<'_>,
    tag: NodeTag,
) -> Result<NodeKind, ArenaError> {
    match tag {
        NodeTag::Box => Ok(NodeKind::Box),
        NodeTag::Text => {
            let paragraph = ParagraphStyle {
                max_lines: Option::<u32>::read(input)?,
                ellipsis: Option::<String>::read(input)?,
            };
            // Present means the caller wrote a string and expects it parsed;
            // absent means they built the runs and expect them left alone. See
            // "Why a text node says which of the two it carries".
            let segments = match Option::<String>::read(input)? {
                Some(markup) => {
                    meo_canvas_core::markup::parse_paragraph(&markup)
                }
                None => read_segments(input)?,
            };
            Ok(NodeKind::Text {
                segments,
                paragraph,
            })
        }
        NodeTag::Image => Ok(NodeKind::Image {
            source: ImageSource::read(input)?,
            fit: ObjectFit::read(input)?,
            position: <(Length, Length)>::read(input)?,
            frame: Option::<u32>::read(input)?,
        }),
        NodeTag::Path => Ok(NodeKind::Path {
            data: String::read(input)?,
            fill: Option::<PathPaint>::read(input)?,
            stroke: Option::<PathPaint>::read(input)?,
            line_width: f32::read(input)?,
            fill_rule: FillRule::read(input)?,
            line_cap: LineCap::read(input)?,
            line_join: LineJoin::read(input)?,
            line_dash: Vec::<f32>::read(input)?,
            line_dash_offset: f32::read(input)?,
        }),
    }
}

/// Named so the documentation above can cite the pieces the tables rest on.
const _: fn(&[u32]) -> bool = ascending;
const _: u32 = BITS_PER_SLOT;

#[cfg(test)]
mod tests {
    use meo_canvas_scene::{
        SceneError, Size,
        node::{NodeId, NodeKind, NodeTag},
        style::paint::Color,
    };

    use super::{
        ArenaError, ColorSpace, ColorType, MAGIC, MAX_NODES, SideValue,
        VERSION, Values, decode, effects,
        group::{BITS_PER_SLOT, Mask},
        layout, paint, text,
    };

    /// Builds an arena the way the TypeScript writer is specified to.
    ///
    /// A second implementation of the format from the documentation rather
    /// than a call into the decoder's own helpers: the two agreeing is the
    /// property worth testing, and a test that shared the encoder's code with
    /// the decoder would agree with itself.
    #[derive(Default)]
    struct Writer {
        slots: Vec<f64>,
        values: Vec<SideValue>,
    }

    impl Writer {
        /// A header whose surface block says nothing, which is every test
        /// here but the one about the surface block.
        fn header(self, size: Size, scale: f32, pages: usize) -> Self {
            self.header_with(size, scale, &[0.0, 0.0, 0.0], pages)
        }

        /// A header with the surface block written out.
        fn header_with(
            mut self,
            size: Size,
            scale: f32,
            surface: &[f64],
            pages: usize,
        ) -> Self {
            self.slots.extend_from_slice(&[
                MAGIC,
                VERSION,
                f64::from(size.width),
                f64::from(size.height),
                f64::from(scale),
            ]);
            self.slots.extend_from_slice(surface);
            self.slots.push(pages as f64);
            self
        }

        fn slot(mut self, value: f64) -> Self {
            self.slots.push(value);
            self
        }

        fn slots(mut self, values: &[f64]) -> Self {
            self.slots.extend_from_slice(values);
            self
        }

        fn text_value(mut self, text: &str) -> (Self, f64) {
            let index = self.values.len() as f64;
            self.values.push(SideValue::Text(text.to_owned()));
            (self, index)
        }

        fn bytes_value(mut self, bytes: &[u8]) -> (Self, f64) {
            let index = self.values.len() as f64;
            self.values.push(SideValue::Bytes(bytes.to_vec()));
            (self, index)
        }

        /// A node with no properties set on any group.
        fn bare_node(self, tag: NodeTag) -> Self {
            self.slot(f64::from(tag.to_wire())).slots(&[0.0; 4]) // one empty mask per group
        }

        fn finish(self) -> (Vec<f64>, Values) {
            (self.slots, Values::new(self.values))
        }
    }

    /// The simplest complete arena: one page, one empty container, no name,
    /// no children.
    /// Slot index of the page count in a header whose surface says nothing:
    /// magic, version, three geometry floats and three absent surface
    /// discriminants. Named rather than written as a number at each use, so a
    /// header change moves one line instead of three.
    const PAGE_COUNT_SLOT: usize = 2 + 3 + 3;

    /// Slot index of the first node's tag, one past the page count.
    const FIRST_TAG_SLOT: usize = PAGE_COUNT_SLOT + 1;

    fn minimal() -> (Vec<f64>, Values) {
        Writer::default()
            .header(Size::new(40.0, 20.0), 2.0, 1)
            .bare_node(NodeTag::Box)
            .slot(0.0) // name: absent
            .slot(0.0) // no children
            .finish()
    }

    #[test]
    fn the_header_carries_the_surface() {
        let (slots, values) = minimal();
        let scene = decode(&slots, &values)
            .unwrap_or_else(|error| unreachable!("{error}"));

        assert_eq!(scene.size, Size::new(40.0, 20.0));
        assert!((scene.scale - 2.0).abs() < f32::EPSILON);
        assert_eq!(scene.pages, vec![NodeId::ROOT]);
        assert_eq!(scene.len(), 1);
        assert_eq!(scene.nodes[0].kind, NodeKind::Box);
        assert!(scene.validate().is_ok());
    }

    #[test]
    fn the_header_carries_a_surface_the_scene_states_and_omits_one_it_does_not()
    {
        // Absent and stated are different scenes, and the header is where the
        // difference lives: `None` is the caller leaving it to the renderer.
        let (slots, values) = minimal();
        let scene = decode(&slots, &values)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(scene.gpu, None);
        assert_eq!(scene.color_type, None);
        assert_eq!(scene.color_space, None);

        let (slots, values) = Writer::default()
            .header_with(
                Size::new(40.0, 20.0),
                2.0,
                &[
                    1.0,
                    0.0, // gpu: present, false
                    1.0,
                    f64::from(ColorType::F16.to_wire()),
                    1.0,
                    f64::from(ColorSpace::DisplayP3.to_wire()),
                ],
                1,
            )
            .bare_node(NodeTag::Box)
            .slot(0.0)
            .slot(0.0)
            .finish();
        let scene = decode(&slots, &values)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(scene.gpu, Some(false));
        assert_eq!(scene.color_type, Some(ColorType::F16));
        assert_eq!(scene.color_space, Some(ColorSpace::DisplayP3));
    }

    #[test]
    fn a_mask_names_only_the_properties_that_follow_it() {
        // Bit 0 is `display` and bit 12 is `flex_grow`: two properties out of
        // thirty, so the record carries two values rather than sixty.
        let mask = (1_u64 << 0) | (1_u64 << 12);
        let (slots, values) = Writer::default()
            .header(Size::new(10.0, 10.0), 1.0, 1)
            .slot(f64::from(NodeTag::Box.to_wire()))
            .slot(mask as f64)
            .slots(&[0.0; 3])
            .slot(f64::from(
                meo_canvas_scene::style::layout::Display::Grid.to_wire(),
            ))
            .slot(3.0) // flex_grow
            .slot(0.0)
            .slot(0.0)
            .finish();

        let scene = decode(&slots, &values)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let node = &scene.nodes[0];
        assert_eq!(
            node.layout.display,
            meo_canvas_scene::style::layout::Display::Grid
        );
        assert!((node.layout.flex_grow - 3.0).abs() < f32::EPSILON);
        // Everything the mask did not name keeps its default.
        assert!((node.layout.flex_shrink - 1.0).abs() < f32::EPSILON);
        assert_eq!(
            node.paint,
            meo_canvas_scene::style::paint::PaintStyle::default()
        );
    }

    /// The reason a mask slot is 53 bits and not 64.
    ///
    /// A double is exact on integers only to 2^53, so bit 53 of a mask packed
    /// into one slot is lost. The reader refuses the value rather than reading
    /// a mask with properties silently missing.
    #[test]
    fn a_mask_wider_than_a_double_can_hold_is_refused() {
        let too_wide = (1_u64 << BITS_PER_SLOT) as f64;
        let (slots, values) = Writer::default()
            .header(Size::new(10.0, 10.0), 1.0, 1)
            .slot(f64::from(NodeTag::Box.to_wire()))
            .slot(too_wide)
            .slots(&[0.0; 3])
            .slot(0.0)
            .slot(0.0)
            .finish();

        assert!(matches!(
            decode(&slots, &values),
            Err(ArenaError::OutOfRange { .. })
        ));
    }

    /// Every group fits the mask it declares, checked here as well as at
    /// compile time so the numbers appear in a failure a reader can act on.
    #[test]
    fn every_group_fits_its_mask() {
        for (name, count, slots) in [
            ("layout", layout::COUNT, layout::SLOTS),
            ("paint", paint::COUNT, paint::SLOTS),
            ("text", text::COUNT, text::SLOTS),
            ("effects", effects::COUNT, effects::SLOTS),
        ] {
            assert!(
                count <= slots * BITS_PER_SLOT as usize,
                "{name} declares {count} properties in {slots} slots"
            );
            assert!(slots <= Mask::MAX_SLOTS, "{name} wants {slots} slots");
            assert!(count > 0, "{name} declares nothing");
        }
        // The tables are the format; a change to one is a format change.
        assert_eq!(layout::COUNT, 30);
        assert_eq!(paint::COUNT, 11);
        assert_eq!(text::COUNT, 15);
        assert_eq!(effects::COUNT, 6);
    }

    #[test]
    fn a_buffer_that_is_not_an_arena_is_refused() {
        assert!(matches!(
            decode(&[0.0], &Values::default()),
            Err(ArenaError::NotAnArena { .. })
        ));
        assert!(matches!(
            decode(&[], &Values::default()),
            Err(ArenaError::Truncated { .. })
        ));
        assert!(matches!(
            decode(&[MAGIC, 99.0], &Values::default()),
            Err(ArenaError::UnsupportedVersion { found, expected })
                if (found - 99.0).abs() < f64::EPSILON
                    && (expected - VERSION).abs() < f64::EPSILON
        ));
    }

    #[test]
    fn every_truncation_is_an_error_and_not_a_panic() {
        let (slots, values) = minimal();
        for length in 0..slots.len() {
            assert!(
                decode(&slots[..length], &values).is_err(),
                "a {length}-slot prefix decoded as a whole scene"
            );
        }
    }

    #[test]
    fn slots_after_the_scene_are_refused() {
        let (mut slots, values) = minimal();
        slots.push(0.0);
        assert!(matches!(
            decode(&slots, &values),
            Err(ArenaError::TrailingSlots { count: 1 })
        ));
    }

    #[test]
    fn a_fractional_slot_where_an_integer_belongs_is_refused() {
        let (mut slots, values) = minimal();
        // The page count, which must be a whole number of pages.
        slots[PAGE_COUNT_SLOT] = 1.5;
        assert!(matches!(
            decode(&slots, &values),
            Err(ArenaError::NotAnInteger { .. })
        ));
    }

    #[test]
    fn a_kind_that_names_nothing_is_refused() {
        let (mut slots, values) = minimal();
        slots[FIRST_TAG_SLOT] = 99.0;
        assert!(matches!(
            decode(&slots, &values),
            Err(ArenaError::UnknownTag {
                what: "NodeTag",
                ..
            })
        ));
    }

    #[test]
    fn text_and_buffers_come_from_the_side_array() {
        let (writer, content) = Writer::default().text_value("hello");
        let (writer, family) = writer.text_value("Fixture");
        let (slots, values) = writer
            .header(Size::new(10.0, 10.0), 1.0, 1)
            .slot(f64::from(NodeTag::Text.to_wire()))
            .slots(&[0.0; 4])
            // paragraph: no max_lines, no ellipsis; not markup; one segment
            .slots(&[0.0, 0.0, 0.0, 1.0])
            .slot(content)
            // the segment's own text mask: bit 0 is font_family
            .slot(1.0)
            .slot(1.0) // present
            .slot(family)
            .slot(0.0) // name
            .slot(0.0) // children
            .finish();

        let scene = decode(&slots, &values)
            .unwrap_or_else(|error| unreachable!("{error}"));
        match &scene.nodes[0].kind {
            NodeKind::Text {
                segments,
                paragraph,
            } => {
                assert_eq!(segments.len(), 1);
                assert_eq!(segments[0].text, "hello");
                assert_eq!(
                    segments[0].style.font_family.as_deref(),
                    Some("Fixture")
                );
                assert!(paragraph.max_lines.is_none());
            }
            other => unreachable!("expected text, found {other:?}"),
        }
    }

    #[test]
    fn a_text_node_carrying_markup_is_parsed_and_one_carrying_runs_is_not() {
        // The distinction the discriminant exists for. The same string reaches
        // the decoder twice: once as markup, where `<b>` opens a bold run, and
        // once as a run the caller built, where it is three characters of text.
        let source = "a <b>b</b>";

        let (writer, content) = Writer::default().text_value(source);
        let (slots, values) = writer
            .header(Size::new(10.0, 10.0), 1.0, 1)
            .slot(f64::from(NodeTag::Text.to_wire()))
            .slots(&[0.0; 4])
            // no max_lines, no ellipsis, and markup present
            .slots(&[0.0, 0.0, 1.0])
            .slot(content)
            .slot(0.0) // name
            .slot(0.0) // children
            .finish();
        let scene = decode(&slots, &values)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let NodeKind::Text { segments, .. } = &scene.nodes[0].kind else {
            unreachable!("expected text");
        };
        assert_eq!(segments.len(), 2, "the markup was parsed");
        assert_eq!(segments[0].text, "a ");
        assert_eq!(segments[1].text, "b");
        assert!(segments[1].style.font_weight.is_some());

        let (writer, content) = Writer::default().text_value(source);
        let (slots, values) = writer
            .header(Size::new(10.0, 10.0), 1.0, 1)
            .slot(f64::from(NodeTag::Text.to_wire()))
            .slots(&[0.0; 4])
            // no max_lines, no ellipsis, not markup, one segment
            .slots(&[0.0, 0.0, 0.0, 1.0])
            .slot(content)
            .slot(0.0) // the segment's own text mask: nothing set
            .slot(0.0) // name
            .slot(0.0) // children
            .finish();
        let scene = decode(&slots, &values)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let NodeKind::Text { segments, .. } = &scene.nodes[0].kind else {
            unreachable!("expected text");
        };
        assert_eq!(segments.len(), 1, "the runs were left alone");
        assert_eq!(segments[0].text, source);
    }

    #[test]
    fn markup_that_says_nothing_still_decodes_to_a_paragraph() {
        // `Text("")` is a node with a run in it, not a node with none. The
        // guarantee lives in `parse_paragraph`, so this side and the Rust
        // facade cannot answer it differently.
        let (writer, content) = Writer::default().text_value("");
        let (slots, values) = writer
            .header(Size::new(10.0, 10.0), 1.0, 1)
            .slot(f64::from(NodeTag::Text.to_wire()))
            .slots(&[0.0; 4])
            .slots(&[0.0, 0.0, 1.0])
            .slot(content)
            .slot(0.0)
            .slot(0.0)
            .finish();

        let scene = decode(&slots, &values)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let NodeKind::Text { segments, .. } = &scene.nodes[0].kind else {
            unreachable!("expected text");
        };
        assert_eq!(segments.len(), 1);
        assert!(segments[0].text.is_empty());
    }

    #[test]
    fn an_index_naming_nothing_in_the_side_array_is_refused() {
        let (slots, values) = Writer::default()
            .header(Size::new(10.0, 10.0), 1.0, 1)
            .slot(f64::from(NodeTag::Text.to_wire()))
            .slots(&[0.0; 4])
            .slots(&[0.0, 0.0, 0.0, 1.0])
            .slot(7.0) // no such side value
            .slot(0.0)
            .slot(0.0)
            .slot(0.0)
            .finish();

        assert!(matches!(
            decode(&slots, &values),
            Err(ArenaError::NoSuchValue {
                index: 7,
                length: 0,
                ..
            })
        ));
    }

    #[test]
    fn a_side_value_of_the_wrong_kind_is_refused() {
        let (writer, index) = Writer::default().bytes_value(&[1, 2, 3]);
        let (slots, values) = writer
            .header(Size::new(10.0, 10.0), 1.0, 1)
            .slot(f64::from(NodeTag::Text.to_wire()))
            .slots(&[0.0; 4])
            .slots(&[0.0, 0.0, 0.0, 1.0])
            .slot(index) // a buffer where the format wants a string
            .slot(0.0)
            .slot(0.0)
            .slot(0.0)
            .finish();

        assert!(matches!(
            decode(&slots, &values),
            Err(ArenaError::WrongValueKind {
                wanted: "a string",
                ..
            })
        ));
    }

    #[test]
    fn a_colour_is_one_packed_slot() {
        // Bit 0 of the paint mask is `background_color`.
        let packed = f64::from(u32::from_be_bytes([0x11, 0x22, 0x33, 0x44]));
        let (slots, values) = Writer::default()
            .header(Size::new(10.0, 10.0), 1.0, 1)
            .slot(f64::from(NodeTag::Box.to_wire()))
            .slots(&[0.0, 1.0, 0.0, 0.0])
            .slot(packed)
            .slot(0.0)
            .slot(0.0)
            .finish();

        let scene = decode(&slots, &values)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(
            scene.nodes[0].paint.background_color,
            Color::rgba(0x11, 0x22, 0x33, 0x44)
        );
    }

    #[test]
    fn children_nest_and_the_arena_stays_a_forest() {
        let (slots, values) = Writer::default()
            .header(Size::new(10.0, 10.0), 1.0, 1)
            .bare_node(NodeTag::Box)
            .slot(0.0)
            .slot(2.0) // two children
            .bare_node(NodeTag::Box)
            .slots(&[0.0, 0.0])
            .bare_node(NodeTag::Box)
            .slots(&[0.0, 0.0])
            .finish();

        let scene = decode(&slots, &values)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(scene.len(), 3);
        assert_eq!(scene.nodes[0].children.len(), 2);
        assert!(scene.validate().is_ok());
    }

    #[test]
    fn a_scene_with_no_pages_is_refused() {
        let (slots, values) = Writer::default()
            .header(Size::new(10.0, 10.0), 1.0, 0)
            .finish();
        assert!(matches!(
            decode(&slots, &values),
            Err(ArenaError::InvalidScene(SceneError::NoPages))
        ));
    }

    #[test]
    fn a_child_count_larger_than_the_arena_is_refused() {
        let (slots, values) = Writer::default()
            .header(Size::new(10.0, 10.0), 1.0, 1)
            .bare_node(NodeTag::Box)
            .slot(0.0)
            .slot(1000.0) // more children than there are slots
            .finish();
        assert!(matches!(
            decode(&slots, &values),
            Err(ArenaError::OutOfRange { .. })
        ));
    }

    #[test]
    fn every_error_says_what_is_wrong() {
        use core::error::Error as _;

        let messages = [
            ArenaError::NotAnArena { found: 3.0 }.to_string(),
            ArenaError::UnsupportedVersion {
                found: 2.0,
                expected: 1.0,
            }
            .to_string(),
            ArenaError::Truncated { slot: 4, length: 2 }.to_string(),
            ArenaError::NotAnInteger {
                slot: 1,
                found: 1.5,
            }
            .to_string(),
            ArenaError::OutOfRange {
                slot: 1,
                found: -1.0,
            }
            .to_string(),
            ArenaError::NotABoolean {
                slot: 1,
                found: 7.0,
            }
            .to_string(),
            ArenaError::NotAPresenceFlag {
                slot: 1,
                found: 7.0,
            }
            .to_string(),
            ArenaError::UnknownTag {
                slot: 1,
                what: "Length",
                found: 9.0,
            }
            .to_string(),
            ArenaError::NoSuchValue {
                slot: 1,
                index: 3,
                length: 1,
            }
            .to_string(),
            ArenaError::WrongValueKind {
                slot: 1,
                wanted: "a string",
            }
            .to_string(),
            ArenaError::TooManyNodes {
                found: 5,
                limit: MAX_NODES,
            }
            .to_string(),
            ArenaError::TrailingSlots { count: 2 }.to_string(),
        ];
        for message in &messages {
            assert!(!message.is_empty());
        }
        assert!(messages[2].contains("slot 4"));
        assert!(messages[7].contains("Length"));

        let invalid = ArenaError::InvalidScene(SceneError::NoPages);
        assert!(invalid.source().is_some());
        assert!(ArenaError::NotAnArena { found: 0.0 }.source().is_none());
    }

    #[test]
    fn the_side_array_reports_its_own_shape() {
        let empty = Values::default();
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);

        let one = Values::new(vec![SideValue::Text("x".to_owned())]);
        assert!(!one.is_empty());
        assert_eq!(one.len(), 1);
        assert!(!format!("{one:?}").is_empty());
    }

    #[test]
    fn a_mask_counts_the_bits_it_carries() {
        let (slots, values) = minimal();
        let mut input = super::Reader::new(&slots[6..], &values);
        let _ = input.slot();
        let mask = Mask::read(&mut input, 1)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(mask.count(), 0);
        assert!(!mask.has(0));
        // A bit beyond the slots a group declares is absent rather than a
        // panic, which is what lets a reader skip a property it does not know.
        assert!(!mask.has(BITS_PER_SLOT * 4));
    }
    /// Reads one value straight out of a slot sequence.
    ///
    /// The unit of the format a TypeScript writer gets wrong first: a type's
    /// slot count and tag numbering. Each case below is the smallest arena
    /// that carries one value, so a failure names the type rather than the
    /// scene it was buried in.
    fn read_one<T: super::value::ArenaValue>(
        slots: &[f64],
        values: &Values,
    ) -> Result<T, ArenaError> {
        let mut input = super::Reader::new(slots, values);
        T::read(&mut input)
    }

    #[test]
    fn every_primitive_reads_the_slots_the_specification_says() {
        use meo_canvas_scene::{
            Corners, Sides,
            style::{Dimension, Length},
        };

        let none = Values::default();

        // f32 narrows from the double; bool is 0 or 1 and nothing else.
        assert_eq!(read_one::<f32>(&[1.5], &none), Ok(1.5));
        assert_eq!(read_one::<bool>(&[0.0], &none), Ok(false));
        assert_eq!(read_one::<bool>(&[1.0], &none), Ok(true));
        assert!(matches!(
            read_one::<bool>(&[2.0], &none),
            Err(ArenaError::NotABoolean { .. })
        ));

        // Integers refuse what they cannot hold rather than wrapping.
        assert_eq!(read_one::<u16>(&[65_535.0], &none), Ok(u16::MAX));
        assert!(read_one::<u16>(&[65_536.0], &none).is_err());
        assert_eq!(read_one::<i16>(&[-3.0], &none), Ok(-3));
        assert!(read_one::<i16>(&[40_000.0], &none).is_err());
        assert_eq!(read_one::<i32>(&[-7.0], &none), Ok(-7));
        assert!(read_one::<i32>(&[3.0e12], &none).is_err());
        assert_eq!(read_one::<u32>(&[9.0], &none), Ok(9));

        // Option is a flag and perhaps a value; anything else is an error.
        assert_eq!(read_one::<Option<f32>>(&[0.0], &none), Ok(None));
        assert_eq!(read_one::<Option<f32>>(&[1.0, 4.0], &none), Ok(Some(4.0)));
        assert!(matches!(
            read_one::<Option<f32>>(&[5.0], &none),
            Err(ArenaError::NotAPresenceFlag { .. })
        ));

        // Vec is a count then the items.
        assert_eq!(read_one::<Vec<f32>>(&[0.0], &none), Ok(Vec::new()));
        assert_eq!(
            read_one::<Vec<f32>>(&[2.0, 1.0, 2.0], &none),
            Ok(vec![1.0, 2.0])
        );

        // Pairs, sides and corners are their parts back to back, in the order
        // the specification names.
        assert_eq!(read_one::<(f32, f32)>(&[1.0, 2.0], &none), Ok((1.0, 2.0)));
        assert_eq!(
            read_one::<Sides<f32>>(&[1.0, 2.0, 3.0, 4.0], &none),
            Ok(Sides {
                top: 1.0,
                right: 2.0,
                bottom: 3.0,
                left: 4.0
            })
        );
        assert_eq!(
            read_one::<Corners<f32>>(&[1.0, 2.0, 3.0, 4.0], &none),
            Ok(Corners {
                top_left: 1.0,
                top_right: 2.0,
                bottom_right: 3.0,
                bottom_left: 4.0
            })
        );

        // Two slots each, tag then value, and the value slot is written even
        // where the tag has no use for it.
        assert_eq!(
            read_one::<Length>(&[0.0, 5.0], &none),
            Ok(Length::Points(5.0))
        );
        assert_eq!(
            read_one::<Length>(&[1.0, 0.5], &none),
            Ok(Length::Percent(0.5))
        );
        assert!(read_one::<Length>(&[2.0, 0.0], &none).is_err());
        assert_eq!(
            read_one::<Dimension>(&[0.0, 0.0], &none),
            Ok(Dimension::Auto)
        );
        assert_eq!(
            read_one::<Dimension>(&[1.0, 8.0], &none),
            Ok(Dimension::Points(8.0))
        );
        assert_eq!(
            read_one::<Dimension>(&[2.0, 0.25], &none),
            Ok(Dimension::Percent(0.25))
        );
        assert!(read_one::<Dimension>(&[3.0, 0.0], &none).is_err());
    }

    #[test]
    fn every_scene_type_reads_the_slots_the_specification_says() {
        use meo_canvas_scene::style::{
            layout::{Display, GridPlacement, TrackSize},
            paint::GradientStop,
            text::{FontWeight, Spacing, TextStroke},
        };

        let none = Values::default();
        let side = Values::new(vec![
            SideValue::Text("a string".to_owned()),
            SideValue::Bytes(vec![7, 8]),
        ]);

        // An enum is one slot holding the byte codec's own discriminant.
        assert_eq!(
            read_one::<Display>(&[f64::from(Display::Grid.to_wire())], &none),
            Ok(Display::Grid)
        );
        assert!(matches!(
            read_one::<Display>(&[200.0], &none),
            Err(ArenaError::UnknownTag {
                what: "Display",
                ..
            })
        ));

        assert_eq!(
            read_one::<String>(&[0.0], &side),
            Ok("a string".to_owned())
        );
        assert_eq!(
            read_one::<FontWeight>(&[700.0], &none),
            Ok(FontWeight::BOLD)
        );
        // Clamped, as the byte codec clamps.
        assert_eq!(
            read_one::<FontWeight>(&[5000.0], &none),
            Ok(FontWeight::new(FontWeight::MAX))
        );

        assert_eq!(
            read_one::<TrackSize>(&[3.0, 2.0], &none),
            Ok(TrackSize::Fraction(2.0))
        );
        assert!(read_one::<TrackSize>(&[4.0, 0.0], &none).is_err());
        assert_eq!(
            read_one::<Spacing>(&[2.0, 0.5], &none),
            Ok(Spacing::Em(0.5))
        );
        assert!(read_one::<Spacing>(&[9.0, 0.0], &none).is_err());
        assert_eq!(
            read_one::<GridPlacement>(&[1.0, -2.0, 1.0, 3.0], &none),
            Ok(GridPlacement::spanning(-2, 3))
        );
        assert_eq!(
            read_one::<TextStroke>(&[1.5, 255.0], &none),
            Ok(TextStroke {
                width: 1.5,
                color: Color::rgba(0, 0, 0, 255)
            })
        );

        let stop = read_one::<GradientStop>(&[0.5, 255.0], &none)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!((stop.offset - 0.5).abs() < f32::EPSILON);
    }

    /// A gradient's geometry, split from the composites above because three
    /// kinds and two arms of a linear direction do not fit beside them.
    #[test]
    fn a_gradient_reads_the_geometry_its_kind_names() {
        use meo_canvas_scene::style::{
            Length,
            paint::{
                Gradient, GradientGeometry, GradientKind, LinearDirection,
            },
        };

        let none = Values::default();

        // A conic gradient: the kind tag, then the geometry that kind reads --
        // a centre of two `Length`s and the angle the sweep begins at -- then
        // the stops. The geometry precedes the stops because it belongs to the
        // shape, which is what the tag named.
        let gradient = read_one::<Gradient>(
            &[
                f64::from(GradientKind::Conic.to_wire()),
                1.0,
                0.25,
                1.0,
                0.75,
                45.0,
                1.0,
                0.5,
                0.0,
                255.0,
            ],
            &none,
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(
            gradient.geometry,
            GradientGeometry::Conic {
                at: (Length::Percent(0.25), Length::Percent(0.75)),
                from: 45.0,
            }
        );
        assert_eq!(gradient.stops.len(), 1);
        assert!(gradient.stops[0].offset >= 0.0);

        // The endpoint form of a linear direction, which is the arm the shape
        // change exists for and which no other test here reaches.
        let gradient = read_one::<Gradient>(
            &[
                f64::from(GradientKind::Linear.to_wire()),
                1.0,
                1.0,
                0.25,
                1.0,
                0.0,
                1.0,
                0.75,
                1.0,
                1.0,
                0.0,
            ],
            &none,
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(
            gradient.geometry,
            GradientGeometry::Linear {
                direction: LinearDirection::Between {
                    start: (Length::Percent(0.25), Length::Percent(0.0)),
                    end: (Length::Percent(0.75), Length::Percent(1.0)),
                },
            }
        );
        assert!(gradient.stops.is_empty());
    }

    /// The three image sources, split out for the line cap.
    #[test]
    fn every_image_source_reads_the_slots_the_specification_says() {
        use meo_canvas_scene::node::ImageSource;

        let side = Values::new(vec![
            SideValue::Text("a string".to_owned()),
            SideValue::Bytes(vec![7, 8]),
        ]);

        assert_eq!(
            read_one::<ImageSource>(&[0.0, 0.0], &side),
            Ok(ImageSource::Path("a string".to_owned()))
        );
        assert_eq!(
            read_one::<ImageSource>(&[1.0, 0.0], &side),
            Ok(ImageSource::Url("a string".to_owned()))
        );
        assert_eq!(
            read_one::<ImageSource>(&[2.0, 1.0], &side),
            Ok(ImageSource::Bytes(vec![7, 8]))
        );
        assert!(matches!(
            read_one::<ImageSource>(&[2.0, 0.0], &side),
            Err(ArenaError::WrongValueKind {
                wanted: "a buffer",
                ..
            })
        ));
        assert!(read_one::<ImageSource>(&[9.0, 0.0], &side).is_err());
    }

    /// The composites that live on paint and effects, split from the test
    /// above only because one function listing every type runs past the line
    /// limit.
    #[test]
    fn every_composite_type_reads_the_slots_the_specification_says() {
        use meo_canvas_scene::style::{
            Dimension,
            paint::{BackgroundImage, BackgroundRepeat, BackgroundSize},
        };

        let side = Values::new(vec![SideValue::Text("a string".to_owned())]);

        let background = read_one::<BackgroundImage>(
            &[
                // source: a path, from side value zero
                0.0,
                0.0,
                f64::from(BackgroundRepeat::Space.to_wire()),
                // size: per-axis, a quarter of the box wide and auto tall.
                // A quarter rather than a whole, because `Percent(1.0)` is the
                // one value at which a hundredfold units error between the two
                // surfaces reads as correct.
                0.0,
                2.0,
                0.25,
                0.0,
                0.0,
                // position
                0.0,
                4.0,
                1.0,
                0.25,
            ],
            &side,
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(background.repeat, BackgroundRepeat::Space);
        assert_eq!(
            background.size,
            BackgroundSize::PerAxis(Dimension::Percent(0.25), Dimension::Auto)
        );
    }

    /// The effect and path types, split from the test above only because one
    /// function listing every composite runs past the line limit.
    #[test]
    fn every_effect_type_reads_the_slots_the_specification_says() {
        use meo_canvas_scene::{
            node::PathPaint,
            style::effect::{
                BoxShadow, Mask, MaskShape, TextShadow, Transform,
            },
        };

        let none = Values::default();
        let side = Values::new(vec![SideValue::Text("a string".to_owned())]);

        let transform = read_one::<Transform>(
            &[0.0, 1.0, 0.0, 2.0, 30.0, 1.5, 0.5, 1.0, 0.5, 1.0, 0.5],
            &none,
        )
        .unwrap_or_else(|error| unreachable!("{error}"));
        assert!((transform.rotate_degrees - 30.0).abs() < f32::EPSILON);
        assert_eq!(transform.origin, Transform::ORIGIN_CENTER);

        let shadow =
            read_one::<BoxShadow>(&[1.0, 1.0, 2.0, 3.0, 4.0, 255.0], &none)
                .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(shadow.inset);
        assert!((shadow.spread - 4.0).abs() < f32::EPSILON);
        let text_shadow =
            read_one::<TextShadow>(&[1.0, 2.0, 3.0, 255.0], &none)
                .unwrap_or_else(|error| unreachable!("{error}"));
        assert!((text_shadow.blur - 3.0).abs() < f32::EPSILON);

        assert_eq!(
            read_one::<Mask>(
                &[1.0, f64::from(MaskShape::Circle.to_wire())],
                &none
            ),
            Ok(Mask::Shape(MaskShape::Circle))
        );
        assert!(matches!(
            read_one::<Mask>(&[2.0, 0.0, 0.0], &side),
            Ok(Mask::Path { .. })
        ));
        assert!(read_one::<Mask>(&[7.0], &none).is_err());

        assert_eq!(
            read_one::<PathPaint>(&[0.0, 255.0], &none),
            Ok(PathPaint::Solid(Color::rgba(0, 0, 0, 255)))
        );
        assert!(read_one::<PathPaint>(&[7.0], &none).is_err());
    }

    /// The whole pipeline, arena in and image bytes out, with no V8 anywhere.
    ///
    /// This is what makes the addon's own logic testable: everything in
    /// `lib.rs` above `render_off_thread` is V8 marshalling that only a
    /// JavaScript test can reach.
    #[test]
    fn an_arena_renders_to_an_image() {
        let (slots, values) = minimal();
        let bytes = crate::render_off_thread(&slots, &values, "png")
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(&bytes[..4], b"\x89PNG");

        // A 40x20 scene at scale 2.
        let decoded = meo_skia_canvas::Image::from_encoded(&bytes)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!((decoded.width(), decoded.height()), (80, 40));
    }

    #[test]
    fn a_render_reports_what_went_wrong_rather_than_panicking() {
        let (slots, values) = minimal();
        assert!(
            crate::render_off_thread(&slots, &values, "nonsense")
                .is_err_and(|message| message.contains("nonsense"))
        );

        let broken = [MAGIC, 98.0];
        assert!(crate::render_off_thread(&broken, &values, "png").is_err());
    }
    /// A mask reads every slot its group declares, and a bit past them is
    /// absent rather than a panic.
    #[test]
    fn a_two_slot_mask_reads_both_slots() {
        let none = Values::default();
        // Bit 0 in the first slot and bit 0 of the second, which is property
        // 53 -- the first property a one-slot mask could not have named.
        let slots = [1.0, 1.0];
        let mut input = super::Reader::new(&slots, &none);
        let mask = Mask::read(&mut input, 2)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(mask.has(0));
        assert!(mask.has(BITS_PER_SLOT));
        assert!(!mask.has(1));
        assert_eq!(mask.count(), 2);
        assert_eq!(input.remaining(), 0);
    }

    #[test]
    fn a_mask_that_runs_out_of_slots_is_truncation() {
        let none = Values::default();
        let slots = [1.0];
        let mut input = super::Reader::new(&slots, &none);
        assert!(matches!(
            Mask::read(&mut input, 2),
            Err(ArenaError::Truncated { .. })
        ));
    }
    /// `sceneBytes`'s own path, without the V8 half.
    ///
    /// The neon export is `decode` then `codec::encode`; this checks the pair
    /// produces a scene the byte format reads back identically, which is the
    /// property the TypeScript round trip rests on.
    #[test]
    fn an_arena_re_encodes_through_the_byte_format() {
        let (slots, values) = minimal();
        let scene = decode(&slots, &values)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let bytes = meo_canvas_scene::codec::encode(&scene);
        let round_tripped = meo_canvas_scene::codec::decode(&bytes)
            .unwrap_or_else(|error| unreachable!("{error}"));

        // The two representations produce one scene, which is the whole claim.
        assert_eq!(scene, round_tripped);
    }

    /// A malformed arena is an error, not short bytes.
    #[test]
    fn re_encoding_a_broken_arena_fails_rather_than_truncating() {
        let (mut slots, values) = minimal();
        slots[FIRST_TAG_SLOT] = 99.0;
        assert!(decode(&slots, &values).is_err());
    }
    /// Every property has a probe value distinguishable from its default.
    ///
    /// The check Agent Zero asked for, as a test rather than as a rule stated
    /// in prose. A case whose value equals the property's default gives an
    /// encoder nothing to do: a correct writer may legitimately leave the mask
    /// bit clear, the case still round-trips, and the test passes while
    /// exercising no write path. Sixty such cases would be sixty green results
    /// that prove nothing, and nothing would say which.
    ///
    /// Where the ones-stream lands on the default -- `flex_shrink` and
    /// `opacity` both default to `1.0` -- the generator must choose another
    /// value, and this is what tells it which properties those are.
    #[test]
    fn every_property_has_a_probe_that_differs_from_its_default() {
        let mut same = Vec::new();

        for (index, name) in layout::INDICES.iter().zip(layout::NAMES) {
            let probe = layout::probe(*index);
            assert!(probe.is_some(), "layout::{name} has no probe");
            if probe
                == Some(meo_canvas_scene::style::layout::LayoutStyle::default())
            {
                same.push(format!("layout::{name}"));
            }
        }
        for (index, name) in paint::INDICES.iter().zip(paint::NAMES) {
            let probe = paint::probe(*index);
            assert!(probe.is_some(), "paint::{name} has no probe");
            if probe
                == Some(meo_canvas_scene::style::paint::PaintStyle::default())
            {
                same.push(format!("paint::{name}"));
            }
        }
        for (index, name) in text::INDICES.iter().zip(text::NAMES) {
            let probe = text::probe(*index);
            assert!(probe.is_some(), "text::{name} has no probe");
            if probe
                == Some(meo_canvas_scene::style::text::TextStyle::default())
            {
                same.push(format!("text::{name}"));
            }
        }
        for (index, name) in effects::INDICES.iter().zip(effects::NAMES) {
            let probe = effects::probe(*index);
            assert!(probe.is_some(), "effects::{name} has no probe");
            if probe
                == Some(meo_canvas_scene::style::effect::Effects::default())
            {
                same.push(format!("effects::{name}"));
            }
        }

        assert!(
            same.is_empty(),
            "these properties probe to their own default, so a case built from \
             the ones-stream would exercise nothing: {same:?}"
        );
    }

    /// No two groups name a property the same thing.
    ///
    /// The round-trip artefact is keyed by the Rust field name, flat across all
    /// four groups, so a name used twice would put two cases in one key and
    /// silently drop one of them. Sixty-two names are unique today; this is
    /// what keeps that true when a group grows. `text::opacity` beside
    /// `paint::opacity` is the shape of the mistake, and it would be an
    /// entirely reasonable field to add.
    #[test]
    fn no_property_name_is_used_by_two_groups() {
        let mut seen: std::collections::BTreeMap<&str, &str> =
            std::collections::BTreeMap::new();
        let mut collisions = Vec::new();

        for (group, names) in [
            ("layout", layout::NAMES),
            ("paint", paint::NAMES),
            ("text", text::NAMES),
            ("effects", effects::NAMES),
        ] {
            for name in names {
                if let Some(first) = seen.insert(name, group) {
                    collisions.push(format!("{name} in {first} and {group}"));
                }
            }
        }

        assert!(
            collisions.is_empty(),
            "the artefact is keyed by field name across every group, so these \
             would collide: {collisions:?}"
        );
        assert_eq!(
            seen.len(),
            layout::COUNT + paint::COUNT + text::COUNT + effects::COUNT
        );
    }

    /// Each table names as many properties as it indexes, without repeats.
    #[test]
    fn every_table_names_each_of_its_properties_once() {
        for (group, indices, names) in [
            ("layout", layout::INDICES, layout::NAMES),
            ("paint", paint::INDICES, paint::NAMES),
            ("text", text::INDICES, text::NAMES),
            ("effects", effects::INDICES, effects::NAMES),
        ] {
            assert_eq!(
                indices.len(),
                names.len(),
                "{group} indexes {} properties and names {}",
                indices.len(),
                names.len()
            );
            let unique: std::collections::BTreeSet<&&str> =
                names.iter().collect();
            assert_eq!(
                unique.len(),
                names.len(),
                "{group} names a property twice"
            );
        }
    }
}
