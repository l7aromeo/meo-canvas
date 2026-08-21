//! The byte format a scene is persisted in.
//!
//! Self-contained and self-describing: a magic number, a version, and every
//! string and buffer inside the buffer. It is what the CLI reads off disk and
//! what the golden fixtures are stored as.
//!
//! It is not the boundary format. JavaScript reaches the renderer through an
//! `f64` arena with a side array of values, decoded in `meo-canvas-node`,
//! because a store into a `Float64Array` is one operation where writing varint
//! bytes from JavaScript is several. Both representations produce the same
//! [`Scene`], so a scene captured from JavaScript and written here round-trips
//! without loss.
//!
//! Hand-written rather than derived. The layout below is a published contract:
//! a derive would make it a consequence of field declaration order, where
//! reordering two fields for readability silently breaks every other reader.
//!
//! # Wire specification
//!
//! Everything is little-endian. There is no alignment and no padding: each
//! value begins at the byte after the previous one ends. The format is
//! positional, not tagged -- every field of every node is written
//! unconditionally, in the order given below. A reader that knows the version
//! knows what comes next without inspecting it.
//!
//! ## Primitives
//!
//! | Form | Encoding |
//! | --- | --- |
//! | `u8` | one byte |
//! | `u16`, `u32` | 2 or 4 bytes, little-endian |
//! | `i16`, `i32` | 2 or 4 bytes, little-endian two's complement |
//! | `f32` | 4 bytes, IEEE-754 binary32, little-endian |
//! | `bool` | one byte: `0` false, `1` true. Any other value is [`CodecError::UnknownTag`] |
//! | `str` | `u32` byte length, then that many bytes of UTF-8 |
//! | `bytes` | `u32` length, then that many bytes |
//! | `opt<T>` | one byte: `0` and nothing more, or `1` followed by a `T` |
//! | `list<T>` | `u32` count, then that many `T` back to back |
//! | `enum` | one byte, the discriminant named in that type's documentation |
//!
//! A length or count larger than the bytes remaining is
//! [`CodecError::Truncated`] rather than an allocation: every element costs at
//! least one byte, so a count above the remaining length cannot be honest.
//!
//! ## Composites
//!
//! ```text
//! scene    := "MCSC" u16(version) f32 f32 f32 list<u32>(pages) list<node>
//!             ^magic         ^1   ^w  ^h  ^scale
//!
//! node     := kind layout paint text effects list<u32>(children) opt<str>(name)
//!
//! kind     := u8(0)                                          -- Box
//!           | u8(1) list<segment> opt<u32> opt<str>          -- Text: segments, max_lines, ellipsis
//!           | u8(2) source enum(fit) length length opt<u32>  -- Image: .., position x/y, frame
//!           | u8(3) str(d) opt<paint> opt<paint> f32 enum enum enum list<f32> f32
//!                                                            -- Path: fill, stroke, line_width,
//!                                                               fill_rule, cap, join, dash, offset
//!
//! segment  := str textstyle
//! source   := u8(0) str | u8(1) str | u8(2) bytes            -- Path | Url | Bytes
//! paint    := u8(0) color | u8(1) gradient                   -- Solid | Gradient
//!
//! length   := u8(0) f32 | u8(1) f32                          -- Points | Percent
//! dim      := u8(0) | u8(1) f32 | u8(2) f32                  -- Auto | Points | Percent
//! track    := u8(0) | u8(1) f32 | u8(2) f32 | u8(3) f32      -- Auto | Points | Percent | Fraction
//! spacing  := u8(0) | u8(1) f32 | u8(2) f32                  -- Normal | Points | Em
//! color    := u8 u8 u8 u8                                    -- r g b a
//! sides<T> := T T T T                                        -- top right bottom left
//! corners  := f32 f32 f32 f32                                -- tl tr br bl
//! place    := opt<i16>(start) opt<u16>(span)
//!
//! layout   := enum(display) enum(position_type) sides<opt<length>>(inset)
//!             dim dim (size) dim dim (min) dim dim (max) opt<f32>(aspect_ratio)
//!             sides<dim>(margin) sides<length>(padding) sides<f32>(border)
//!             enum(flex_direction) enum(flex_wrap) f32(grow) f32(shrink) dim(basis)
//!             opt<enum>(justify_content) opt<enum>(align_items)
//!             opt<enum>(align_self) opt<enum>(align_content)
//!             length length (gap row, column) enum enum (overflow x, y)
//!             enum(box_sizing) enum(direction)
//!             list<track>(columns) list<track>(rows)
//!             opt<track>(auto_rows) opt<track>(auto_columns) enum(auto_flow)
//!             place(grid_column) place(grid_row)
//!
//! paint    := color(background) opt<gradient> opt<bgimage>
//!             sides<opt<color>>(border_color) color(border_color_all)
//!             enum(border_style) corners(radius) f32(opacity) enum(blend_mode)
//!             bool(dither) i32(z_index)
//!
//! gradient := enum(kind) list<stop> f32(angle_degrees) length length (center)
//! stop     := f32(offset) color
//! bgimage  := source enum(repeat) opt<length> opt<length> length length
//!
//! text     := opt<str>(family) opt<f32>(size) opt<u16>(weight) opt<enum>(style)
//!             opt<color> opt<enum>(align) opt<enum>(decoration)
//!             opt<enum>(vertical_align) opt<enum>(paint_order)
//!             opt<f32>(line_height) opt<f32>(line_gap)
//!             opt<spacing>(letter) opt<spacing>(word)
//!             opt<list<enum>>(font_variant) opt<stroke>
//! stroke   := f32(width) color
//!
//! effects  := opt<transform> list<boxshadow> list<textshadow> opt<mask>
//!             opt<str>(filter) opt<str>(backdrop_filter)
//! transform:= length length f32(rotate_degrees) f32(scale_x) f32(scale_y) length length
//! boxshadow:= bool(inset) f32 f32 f32 f32 color                -- offset x/y, blur, spread
//! textshadow := f32 f32 f32 color                              -- offset x/y, blur
//! mask     := u8(0) source | u8(1) enum(shape)
//!           | u8(2) str(d) enum(fill_rule) | u8(3) gradient
//! ```
//!
//! The discriminant of every `enum` is given in that type's own documentation
//! and is fixed independently of the order its variants are declared in.
//!
//! ## Pages
//!
//! `pages` is the root of each page, in the order they are drawn: frame one of
//! an animation is `pages[0]`, and the format preserves that order because
//! nothing else records it. The list is written before the nodes so a reader
//! that only wants the page count need not decode the arena.
//!
//! One arena serves every page. [`decode`] checks that it is a forest -- one
//! parent per node, one page per node, every node reached -- so a buffer whose
//! pages share a subtree is refused rather than laid out twice. A buffer
//! declaring no pages is refused for the same reason one declaring no nodes is.
//!
//! ## What a version bump is for
//!
//! [`VERSION`] changes when a reader of the current revision would misread a
//! newer buffer. Adding a [`crate::NodeKind`] variant is such a change: its tag
//! byte names nothing to this revision, so the buffer is refused rather than
//! silently drawn without that node. Widening a field or reordering one is the
//! same. A version bump is the mechanism for that, not an obstacle to it --
//! the chart node this crate does not yet model arrives with one.

mod impls;
mod reader;
mod writer;

pub(crate) use reader::Reader;
pub(crate) use writer::Writer;

use crate::{Scene, node::NodeId};

/// The four bytes every encoded scene starts with.
///
/// Four rather than two or eight: two collide with too much, and eight costs
/// bytes in a fixture file for no further discrimination.
pub const MAGIC: [u8; 4] = *b"MCSC";

/// The format revision this crate reads and writes.
///
/// [`decode`] refuses anything else. A reader that skipped fields it did not
/// recognise would draw a picture missing whatever those fields said, which is
/// worse than refusing to draw one.
pub const VERSION: u16 = 1;

/// The largest node count [`decode`] will allocate for.
///
/// A decoded [`crate::Node`] is around 400 bytes of plain fields before any of
/// its `Vec`s allocate, so a million of them is roughly 400 MB. The bound is
/// there because the count is read before any node is, and a corrupt four bytes
/// would otherwise reserve whatever they happened to say. A million is far
/// above any scene a caller writes -- the busiest golden fixture is in the
/// hundreds -- and far below a count that exhausts a machine.
pub const MAX_NODES: u32 = 1 << 20;

/// What can be wrong with a buffer meant to hold a scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    /// The buffer does not begin with [`MAGIC`].
    NotAScene,
    /// The buffer is well formed but written by another revision.
    UnsupportedVersion {
        /// The revision the buffer claims.
        found: u16,
        /// The revision this crate reads.
        expected: u16,
    },
    /// The buffer ends inside a field.
    Truncated {
        /// Byte offset the read was attempted at.
        offset: usize,
        /// How many bytes the field needed.
        needed: usize,
        /// How many were left.
        available: usize,
    },
    /// A discriminant that names no variant.
    UnknownTag {
        /// Byte offset the tag was read from.
        offset: usize,
        /// The value found there.
        tag: u8,
    },
    /// A string field that is not valid UTF-8.
    InvalidUtf8 {
        /// Byte offset the string's contents started at.
        offset: usize,
    },
    /// A node count above [`MAX_NODES`].
    TooManyNodes {
        /// The count the buffer declared.
        found: u32,
        /// The largest count this crate allocates for.
        limit: u32,
    },
    /// The buffer decoded, but the tree it describes is not one.
    ///
    /// Every structural rule lives in [`Scene::validate`] and is reported by
    /// it, so the codec states the rules once rather than restating them as
    /// its own error variants. Wrapping rather than flattening also means a
    /// rule added there reaches a decoding caller without a change here.
    InvalidScene(crate::SceneError),
    /// Bytes remain after the scene ends.
    ///
    /// Refused rather than ignored: trailing bytes mean the writer and the
    /// reader disagree about the format, and a reader that shrugs at that
    /// difference will accept a file it has silently misread.
    TrailingBytes {
        /// How many bytes were left over.
        count: usize,
    },
}

impl core::fmt::Display for CodecError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotAScene => {
                f.write_str("buffer does not begin with the scene magic")
            }
            Self::UnsupportedVersion { found, expected } => {
                write!(f, "scene format version {found}, expected {expected}")
            }
            Self::Truncated {
                offset,
                needed,
                available,
            } => write!(
                f,
                "buffer ends inside a field at byte {offset}: \
                 needed {needed} bytes, {available} remain"
            ),
            Self::UnknownTag { offset, tag } => {
                write!(f, "unknown tag {tag} at byte {offset}")
            }
            Self::InvalidUtf8 { offset } => {
                write!(f, "string at byte {offset} is not valid UTF-8")
            }
            Self::TooManyNodes { found, limit } => {
                write!(f, "buffer declares {found} nodes, the limit is {limit}")
            }
            Self::InvalidScene(error) => {
                write!(f, "the buffer decodes but is not a scene: {error}")
            }
            Self::TrailingBytes { count } => {
                write!(f, "{count} bytes remain after the scene ends")
            }
        }
    }
}

impl core::error::Error for CodecError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::InvalidScene(error) => Some(error),
            _ => None,
        }
    }
}

/// How a value of one type is written to and read from the wire.
///
/// Crate-internal: the format is a promise about bytes, not about which Rust
/// trait produces them, and a public trait would let a downstream crate add an
/// implementation the specification above does not describe.
pub(crate) trait Wire: Sized {
    /// Appends this value's bytes.
    fn write(&self, out: &mut Writer<'_>);

    /// Reads one value, advancing the reader past it.
    fn read(input: &mut Reader<'_>) -> Result<Self, CodecError>;
}

/// Writes a scene into a fresh buffer.
#[must_use]
pub fn encode(scene: &Scene) -> Vec<u8> {
    let mut out = Vec::new();
    encode_into(scene, &mut out);
    out
}

/// Writes a scene into an existing buffer, appending to whatever it holds.
///
/// The allocating [`encode`] is the convenient form; this one is what a caller
/// rendering a sequence uses, so one buffer serves every frame.
pub fn encode_into(scene: &Scene, out: &mut Vec<u8>) {
    let mut writer = Writer::new(out);
    writer.raw(&MAGIC);
    writer.u16(VERSION);
    writer.f32(scene.size.width);
    writer.f32(scene.size.height);
    writer.f32(scene.scale);
    writer.list(&scene.pages);
    writer.list(&scene.nodes);
}

/// Reads a scene back out of a buffer.
///
/// The decoded arena is checked against every structural rule, so a scene from
/// here always satisfies [`Scene::validate`].
///
/// # Errors
///
/// Returns [`CodecError`] if the buffer is not a scene, was written by another
/// revision, ends early, declares more than [`MAX_NODES`], holds a tag or a
/// string this revision cannot read, describes something that is not a forest
/// of pages, or carries bytes past the end of the scene.
pub fn decode(bytes: &[u8]) -> Result<Scene, CodecError> {
    let mut input = Reader::new(bytes);

    if input.raw(MAGIC.len())? != MAGIC {
        return Err(CodecError::NotAScene);
    }
    let version = input.u16()?;
    if version != VERSION {
        return Err(CodecError::UnsupportedVersion {
            found: version,
            expected: VERSION,
        });
    }

    let width = input.f32()?;
    let height = input.f32()?;
    let scale = input.f32()?;
    let pages: Vec<NodeId> = input.list()?;

    let count = input.peek_u32()?;
    if count > MAX_NODES {
        return Err(CodecError::TooManyNodes {
            found: count,
            limit: MAX_NODES,
        });
    }
    let nodes = input.list()?;

    let remaining = input.remaining();
    if remaining != 0 {
        return Err(CodecError::TrailingBytes { count: remaining });
    }

    let scene = Scene {
        size: crate::Size::new(width, height),
        scale,
        nodes,
        pages,
    };
    scene.validate().map_err(CodecError::InvalidScene)?;
    Ok(scene)
}

#[cfg(test)]
mod tests {
    use super::{
        CodecError, MAGIC, MAX_NODES, Reader, VERSION, Wire, Writer, decode,
        encode, encode_into,
    };
    use crate::{
        Scene, SceneError,
        geometry::{Corners, Sides, Size},
        node::{
            ImageSource, LineCap, LineJoin, Node, NodeId, NodeKind, PathPaint,
        },
        style::{
            Dimension, Length, PaintOrder,
            effect::{
                BoxShadow, Effects, FillRule, Mask, MaskShape, TextShadow,
                Transform,
            },
            layout::{
                Align, BoxSizing, Direction, Display, FlexDirection, FlexWrap,
                GridAutoFlow, GridPlacement, Justify, LayoutStyle, Overflow,
                PositionType, TrackSize,
            },
            paint::{
                BackgroundImage, BackgroundRepeat, BlendMode, BorderStyle,
                Color, Gradient, GradientKind, GradientStop, ObjectFit,
                PaintStyle,
            },
            text::{
                FontStyle, FontVariant, FontWeight, ParagraphStyle, Spacing,
                TextAlign, TextDecoration, TextSegment, TextStroke, TextStyle,
                VerticalAlign,
            },
        },
    };

    /// Byte offset of the node count in a one-page scene: magic, version, two
    /// size floats, the scale, then a page list of one `u32` behind its own
    /// `u32` count. Every test that reaches for it encodes a one-page scene.
    const NODE_COUNT_OFFSET: usize = MAGIC.len() + 2 + 4 + 4 + 4 + (4 + 4);

    fn round_trip<T: Wire + PartialEq + core::fmt::Debug>(value: &T) {
        let mut bytes = Vec::new();
        value.write(&mut Writer::new(&mut bytes));
        let mut input = Reader::new(&bytes);
        let decoded = T::read(&mut input);
        assert_eq!(decoded.as_ref(), Ok(value), "round trip changed the value");
        assert_eq!(input.remaining(), 0, "read consumed the wrong length");
    }

    fn read_one<T: Wire>(bytes: &[u8]) -> Result<T, CodecError> {
        T::read(&mut Reader::new(bytes))
    }

    /// The parts of a populated scene, one builder per style group: a single
    /// function setting every field of all four trips `clippy::too_many_lines`,
    /// and the groups are what a reader compares against the wire layout
    /// anyway.
    fn populated_gradient() -> Gradient {
        Gradient {
            kind: GradientKind::Conic,
            stops: vec![
                GradientStop {
                    offset: 0.0,
                    color: Color::rgb(255, 0, 0),
                },
                GradientStop {
                    offset: 1.0,
                    color: Color::rgba(0, 0, 255, 128),
                },
            ],
            angle_degrees: 45.0,
            center: (Length::Percent(0.25), Length::Points(8.0)),
        }
    }

    fn populated_layout() -> LayoutStyle {
        LayoutStyle {
            display: Display::Grid,
            position_type: PositionType::Absolute,
            inset: Sides {
                top: Some(Length::Points(1.0)),
                right: None,
                bottom: Some(Length::Percent(0.5)),
                left: None,
            },
            size: (Dimension::Points(100.0), Dimension::Percent(0.5)),
            min_size: (Dimension::Points(10.0), Dimension::Auto),
            max_size: (Dimension::Auto, Dimension::Points(900.0)),
            aspect_ratio: Some(1.75),
            margin: Sides::symmetric(Dimension::Auto, Dimension::Points(4.0)),
            padding: Sides::all(Length::Percent(0.125)),
            border: Sides::all(2.0),
            flex_direction: FlexDirection::ColumnReverse,
            flex_wrap: FlexWrap::WrapReverse,
            flex_grow: 2.0,
            flex_shrink: 0.5,
            flex_basis: Dimension::Percent(0.3),
            justify_content: Some(Justify::SpaceEvenly),
            align_items: Some(Align::Baseline),
            align_self: Some(Align::Stretch),
            align_content: Some(Align::SpaceAround),
            gap: (Length::Points(6.0), Length::Percent(0.02)),
            overflow: (Overflow::Hidden, Overflow::Scroll),
            box_sizing: BoxSizing::ContentBox,
            direction: Direction::Rtl,
            grid_template_columns: vec![
                TrackSize::Fraction(1.0),
                TrackSize::Points(120.0),
                TrackSize::Percent(0.25),
                TrackSize::Auto,
            ],
            grid_template_rows: vec![TrackSize::Auto],
            grid_auto_rows: Some(TrackSize::Points(40.0)),
            grid_auto_columns: Some(TrackSize::Fraction(2.0)),
            grid_auto_flow: GridAutoFlow::ColumnDense,
            grid_column: GridPlacement::spanning(-2, 3),
            grid_row: GridPlacement::AUTO,
        }
    }

    fn populated_paint(gradient: Gradient) -> PaintStyle {
        PaintStyle {
            background_color: Color::rgba(1, 2, 3, 4),
            gradient: Some(gradient),
            background_image: Some(BackgroundImage {
                source: ImageSource::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF]),
                repeat: BackgroundRepeat::Space,
                size: (Some(Length::Points(16.0)), None),
                position: (Length::Percent(1.0), Length::Points(-3.0)),
            }),
            border_color: Sides {
                top: Some(Color::BLACK),
                right: None,
                bottom: Some(Color::rgb(9, 9, 9)),
                left: None,
            },
            border_color_all: Color::rgb(7, 7, 7),
            border_style: BorderStyle::Dotted,
            border_radius: Corners {
                top_left: 1.0,
                top_right: 2.0,
                bottom_right: 3.0,
                bottom_left: 4.0,
            },
            opacity: 0.75,
            blend_mode: BlendMode::Luminosity,
            dither: true,
            z_index: -12,
        }
    }

    fn populated_text() -> TextStyle {
        TextStyle {
            font_family: Some("Oswald".to_owned()),
            font_size: Some(18.5),
            font_weight: Some(FontWeight::BOLD),
            font_style: Some(FontStyle::Italic),
            color: Some(Color::rgb(20, 30, 40)),
            text_align: Some(TextAlign::Justify),
            text_decoration: Some(TextDecoration::LineThrough),
            vertical_align: Some(VerticalAlign::Middle),
            paint_order: Some(PaintOrder::Stroke),
            line_height: Some(1.4),
            line_gap: Some(2.0),
            letter_spacing: Some(Spacing::Em(0.05)),
            word_spacing: Some(Spacing::Points(3.0)),
            font_variant: Some(vec![
                FontVariant::SmallCaps,
                FontVariant::TabularNums,
                FontVariant::SlashedZero,
            ]),
            text_stroke: Some(TextStroke {
                width: 0.5,
                color: Color::rgba(0, 0, 0, 200),
            }),
        }
    }

    fn populated_effects() -> Effects {
        Effects {
            transform: Some(Transform {
                translate_x: Length::Percent(0.1),
                translate_y: Length::Points(-4.0),
                rotate_degrees: 30.0,
                scale_x: 1.5,
                scale_y: 0.5,
                origin: (Length::Points(0.0), Length::Percent(1.0)),
            }),
            box_shadows: vec![
                BoxShadow::default(),
                BoxShadow {
                    inset: true,
                    offset_x: 1.0,
                    offset_y: 2.0,
                    blur: 3.0,
                    spread: 4.0,
                    color: Color::rgba(5, 6, 7, 8),
                },
            ],
            text_shadows: vec![TextShadow {
                offset_x: -1.0,
                offset_y: -2.0,
                blur: 0.5,
                color: Color::BLACK,
            }],
            mask: Some(Mask::Path {
                data: "M0 0 L10 10 Z".to_owned(),
                fill_rule: FillRule::EvenOdd,
            }),
            filter: Some("blur(2px)".to_owned()),
            backdrop_filter: Some("saturate(1.5)".to_owned()),
        }
    }

    /// A scene that sets every field to something other than its default, so a
    /// field the codec forgets shows up as an inequality rather than as a
    /// coincidence.
    fn populated_scene() -> Scene {
        let gradient = populated_gradient();

        let mut scene = Scene::new(Size::new(640.0, 480.0));
        scene.scale = 3.0;
        scene.nodes[0].layout = populated_layout();
        scene.nodes[0].paint = populated_paint(gradient.clone());
        scene.nodes[0].text = populated_text();
        scene.nodes[0].effects = populated_effects();
        scene.nodes[0].name = Some("root".to_owned());

        for kind in every_node_kind(&gradient) {
            scene
                .push(NodeId::ROOT, Node::new(kind))
                .unwrap_or_else(|error| unreachable!("{error}"));
        }
        scene
    }

    /// One of each [`NodeKind`], with every optional field of each one set.
    fn every_node_kind(gradient: &Gradient) -> Vec<NodeKind> {
        vec![
            NodeKind::Box,
            NodeKind::Text {
                segments: vec![
                    TextSegment {
                        text: "plain".to_owned(),
                        style: TextStyle::default(),
                    },
                    TextSegment {
                        text: "héllo — ünicode".to_owned(),
                        style: TextStyle {
                            font_weight: Some(FontWeight::new(250)),
                            ..TextStyle::default()
                        },
                    },
                ],
                paragraph: ParagraphStyle {
                    max_lines: Some(3),
                    ellipsis: Some("…".to_owned()),
                },
            },
            NodeKind::Image {
                source: ImageSource::Url(
                    "https://example.test/a.png".to_owned(),
                ),
                fit: ObjectFit::ScaleDown,
                position: (Length::Percent(0.5), Length::Points(2.0)),
                frame: Some(7),
            },
            NodeKind::Path {
                data: "M0 0 H10".to_owned(),
                fill: Some(PathPaint::Solid(Color::rgb(1, 1, 1))),
                stroke: Some(PathPaint::Gradient(gradient.clone())),
                line_width: 2.5,
                fill_rule: FillRule::NonZero,
                line_cap: LineCap::Square,
                line_join: LineJoin::Miter,
                line_dash: vec![4.0, 2.0, 1.0],
                line_dash_offset: 0.5,
            },
        ]
    }

    #[test]
    fn a_populated_scene_survives_the_round_trip_whole() {
        let scene = populated_scene();
        let bytes = encode(&scene);
        assert_eq!(&bytes[..MAGIC.len()], &MAGIC);
        assert_eq!(decode(&bytes), Ok(scene));
    }

    #[test]
    fn a_scene_of_only_a_root_round_trips() {
        let scene = Scene::new(Size::new(1.0, 2.0));
        assert_eq!(decode(&encode(&scene)), Ok(scene));
    }

    #[test]
    fn every_node_kind_round_trips_on_its_own() {
        let gradient = Gradient {
            kind: GradientKind::Radial,
            stops: Vec::new(),
            angle_degrees: 0.0,
            center: (Length::ZERO, Length::ZERO),
        };
        for kind in every_node_kind(&gradient) {
            let mut scene = Scene::new(Size::ZERO);
            scene
                .push(NodeId::ROOT, Node::new(kind))
                .unwrap_or_else(|error| unreachable!("{error}"));
            assert_eq!(decode(&encode(&scene)), Ok(scene));
        }
    }

    #[test]
    fn a_deeply_nested_scene_keeps_its_shape() {
        /// Deep enough that a recursive decoder would be visible in a stack
        /// trace, shallow enough to stay well inside a test thread's 2 MiB
        /// stack. The decoder is iterative -- nodes are a flat list -- so this
        /// pins that the arena, not the tree, is what is written.
        const DEPTH: u32 = 2_000;

        let mut scene = Scene::new(Size::new(10.0, 10.0));
        let mut parent = NodeId::ROOT;
        for level in 0..DEPTH {
            parent = scene
                .push(parent, Node::container().named(format!("level {level}")))
                .unwrap_or_else(|error| unreachable!("{error}"));
        }
        assert_eq!(scene.len(), DEPTH as usize + 1);

        let decoded = decode(&encode(&scene));
        assert_eq!(decoded, Ok(scene));
    }

    #[test]
    fn encode_into_appends_rather_than_replacing() {
        let scene = Scene::new(Size::ZERO);
        let mut buffer = vec![0xAA, 0xBB];
        encode_into(&scene, &mut buffer);
        assert_eq!(&buffer[..2], &[0xAA, 0xBB]);
        assert_eq!(decode(&buffer[2..]), Ok(scene));
    }

    #[test]
    fn a_buffer_that_is_not_a_scene_is_refused() {
        assert_eq!(decode(b"NOPE\x01\x00"), Err(CodecError::NotAScene));
        assert!(matches!(decode(b"MC"), Err(CodecError::Truncated { .. })));
    }

    #[test]
    fn another_revision_is_refused_by_number() {
        let mut bytes = encode(&Scene::new(Size::ZERO));
        bytes[MAGIC.len()] = 2;
        assert_eq!(
            decode(&bytes),
            Err(CodecError::UnsupportedVersion {
                found: 2,
                expected: VERSION,
            })
        );
    }

    #[test]
    fn every_truncation_of_a_scene_is_an_error_and_not_a_panic() {
        let bytes = encode(&populated_scene());
        for length in 0..bytes.len() {
            assert!(
                decode(&bytes[..length]).is_err(),
                "a {length}-byte prefix decoded as a whole scene"
            );
        }
    }

    #[test]
    fn a_length_prefix_larger_than_the_buffer_is_truncation_not_allocation() {
        let mut bytes = encode(&Scene::new(Size::ZERO));
        // The node count, overwritten with a value that fits in a `u32` and is
        // under MAX_NODES, so the count check passes and the list check is what
        // catches it.
        bytes[NODE_COUNT_OFFSET..NODE_COUNT_OFFSET + 4]
            .copy_from_slice(&1_000_u32.to_le_bytes());
        assert_eq!(
            decode(&bytes),
            Err(CodecError::Truncated {
                offset: NODE_COUNT_OFFSET,
                needed: 1_000,
                available: bytes.len() - NODE_COUNT_OFFSET - 4,
            })
        );
    }

    #[test]
    fn a_node_count_above_the_limit_is_refused_before_anything_is_reserved() {
        let mut bytes = encode(&Scene::new(Size::ZERO));
        let found = MAX_NODES + 1;
        bytes[NODE_COUNT_OFFSET..NODE_COUNT_OFFSET + 4]
            .copy_from_slice(&found.to_le_bytes());
        assert_eq!(
            decode(&bytes),
            Err(CodecError::TooManyNodes {
                found,
                limit: MAX_NODES,
            })
        );
    }

    #[test]
    fn a_scene_that_declares_no_nodes_has_no_root() {
        let mut bytes = encode(&Scene::new(Size::ZERO));
        bytes.truncate(NODE_COUNT_OFFSET);
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        assert_eq!(
            decode(&bytes),
            Err(CodecError::InvalidScene(SceneError::UnknownNode(
                NodeId::ROOT
            )))
        );
    }

    #[test]
    fn a_child_index_with_no_node_behind_it_is_refused() {
        let mut scene = Scene::new(Size::ZERO);
        let dangling = NodeId::new(41);
        scene.nodes[0].children.push(dangling);
        assert_eq!(
            decode(&encode(&scene)),
            Err(CodecError::InvalidScene(SceneError::UnknownNode(dangling)))
        );
    }

    #[test]
    fn bytes_after_the_scene_are_refused_rather_than_ignored() {
        let mut bytes = encode(&Scene::new(Size::ZERO));
        bytes.extend_from_slice(&[0, 0, 0]);
        assert_eq!(decode(&bytes), Err(CodecError::TrailingBytes { count: 3 }));
    }

    #[test]
    fn a_string_that_is_not_utf8_names_the_offset_it_started_at() {
        // A one-byte string whose content is a lone continuation byte, which no
        // UTF-8 sequence begins with.
        let bytes = [1, 0, 0, 0, 0x80];
        assert_eq!(
            read_one::<String>(&bytes),
            Err(CodecError::InvalidUtf8 { offset: 4 })
        );
    }

    #[test]
    fn an_unknown_tag_names_its_offset_and_its_value() {
        assert_eq!(
            read_one::<Option<u8>>(&[9]),
            Err(CodecError::UnknownTag { offset: 0, tag: 9 })
        );
        assert_eq!(
            read_one::<bool>(&[2]),
            Err(CodecError::UnknownTag { offset: 0, tag: 2 })
        );
        assert_eq!(
            read_one::<Length>(&[7, 0, 0, 0, 0]),
            Err(CodecError::UnknownTag { offset: 0, tag: 7 })
        );
        assert_eq!(
            read_one::<Dimension>(&[7]),
            Err(CodecError::UnknownTag { offset: 0, tag: 7 })
        );
        assert_eq!(
            read_one::<TrackSize>(&[7]),
            Err(CodecError::UnknownTag { offset: 0, tag: 7 })
        );
        assert_eq!(
            read_one::<Spacing>(&[7]),
            Err(CodecError::UnknownTag { offset: 0, tag: 7 })
        );
        assert_eq!(
            read_one::<ImageSource>(&[7]),
            Err(CodecError::UnknownTag { offset: 0, tag: 7 })
        );
        assert_eq!(
            read_one::<PathPaint>(&[7]),
            Err(CodecError::UnknownTag { offset: 0, tag: 7 })
        );
        assert_eq!(
            read_one::<Mask>(&[7]),
            Err(CodecError::UnknownTag { offset: 0, tag: 7 })
        );
        assert_eq!(
            read_one::<NodeKind>(&[7]),
            Err(CodecError::UnknownTag { offset: 0, tag: 7 })
        );
        assert_eq!(
            read_one::<Display>(&[7]),
            Err(CodecError::UnknownTag { offset: 0, tag: 7 })
        );
    }

    /// Round-trips every variant of a wire enum, and checks that a byte past
    /// the last discriminant is refused.
    macro_rules! check_wire_enum {
        ($($name:ty),+ $(,)?) => {
            $({
                for variant in <$name>::ALL {
                    round_trip(variant);
                    assert_eq!(
                        <$name>::from_wire(variant.to_wire()),
                        Some(*variant),
                    );
                }
                let past_the_end = u8::try_from(<$name>::ALL.len())
                    .unwrap_or(u8::MAX);
                assert_eq!(<$name>::from_wire(past_the_end), None);
                assert_eq!(
                    read_one::<$name>(&[past_the_end]),
                    Err(CodecError::UnknownTag {
                        offset: 0,
                        tag: past_the_end,
                    }),
                );
            })+
        };
    }

    #[test]
    fn every_wire_enum_round_trips_and_refuses_an_unknown_byte() {
        check_wire_enum!(
            Display,
            FlexDirection,
            FlexWrap,
            Justify,
            Align,
            PositionType,
            Overflow,
            BoxSizing,
            Direction,
            GridAutoFlow,
            BorderStyle,
            BlendMode,
            GradientKind,
            ObjectFit,
            BackgroundRepeat,
            TextAlign,
            TextDecoration,
            VerticalAlign,
            FontStyle,
            PaintOrder,
            MaskShape,
            FillRule,
            LineCap,
            LineJoin,
        );
    }

    #[test]
    fn every_primitive_round_trips() {
        round_trip(&7_u8);
        round_trip(&1_234_u16);
        round_trip(&123_456_789_u32);
        round_trip(&-321_i16);
        round_trip(&-987_654_i32);
        round_trip(&core::f32::consts::PI);
        round_trip(&true);
        round_trip(&false);
        round_trip(&"a string".to_owned());
        round_trip(&String::new());
        round_trip(&Some(5_u8));
        round_trip(&Option::<u8>::None);
        round_trip(&vec![1_u8, 2, 3]);
        round_trip(&Vec::<u8>::new());
        round_trip(&NodeId::new(11));
        round_trip(&Sides::all(3_u8));
        round_trip(&Corners::all(4_u8));
        round_trip(&FontWeight::BOLD);
        round_trip(&GridPlacement::spanning(1, 2));
        round_trip(&Color::rgba(1, 2, 3, 4));
    }

    #[test]
    fn every_length_shaped_variant_round_trips() {
        round_trip(&Length::Points(1.5));
        round_trip(&Length::Percent(0.25));
        round_trip(&Dimension::Auto);
        round_trip(&Dimension::Points(2.0));
        round_trip(&Dimension::Percent(0.5));
        round_trip(&TrackSize::Auto);
        round_trip(&TrackSize::Points(3.0));
        round_trip(&TrackSize::Percent(0.75));
        round_trip(&TrackSize::Fraction(2.0));
        round_trip(&Spacing::Normal);
        round_trip(&Spacing::Points(1.0));
        round_trip(&Spacing::Em(0.1));
        round_trip(&ImageSource::Path("/a".to_owned()));
        round_trip(&ImageSource::Url("https://a.test".to_owned()));
        round_trip(&ImageSource::Bytes(vec![1, 2, 3]));
        round_trip(&Mask::Image(ImageSource::Path("/m".to_owned())));
        round_trip(&Mask::Shape(MaskShape::Circle));
        round_trip(&Mask::Gradient(Gradient {
            kind: GradientKind::Linear,
            stops: vec![GradientStop {
                offset: 0.5,
                color: Color::BLACK,
            }],
            angle_degrees: 90.0,
            center: (Length::ZERO, Length::ZERO),
        }));
        round_trip(&PathPaint::Solid(Color::BLACK));
    }

    #[test]
    fn a_weight_outside_the_css_range_is_clamped_rather_than_refused() {
        let bytes = 5_000_u16.to_le_bytes();
        assert_eq!(
            read_one::<FontWeight>(&bytes),
            Ok(FontWeight::new(FontWeight::MAX))
        );
    }

    #[test]
    fn corrupting_any_single_byte_never_panics() {
        // Stepping rather than every byte: the buffer is tens of kilobytes and
        // a full sweep decodes it once per byte. A stride of 7 is coprime with
        // every field width in the format -- 1, 2 and 4 -- so it lands on the
        // first, second, third and fourth byte of a `u32` in turn rather than
        // always on the same one.
        const STRIDE: usize = 7;

        let bytes = encode(&populated_scene());
        for index in (0..bytes.len()).step_by(STRIDE) {
            let mut corrupt = bytes.clone();
            corrupt[index] = 0xFF;
            let _ = decode(&corrupt);
        }
    }

    #[test]
    fn every_error_says_what_is_wrong() {
        let messages = [
            CodecError::NotAScene.to_string(),
            CodecError::UnsupportedVersion {
                found: 2,
                expected: 1,
            }
            .to_string(),
            CodecError::Truncated {
                offset: 4,
                needed: 8,
                available: 2,
            }
            .to_string(),
            CodecError::UnknownTag { offset: 1, tag: 9 }.to_string(),
            CodecError::InvalidUtf8 { offset: 3 }.to_string(),
            CodecError::TooManyNodes { found: 5, limit: 4 }.to_string(),
            CodecError::InvalidScene(SceneError::UnknownNode(NodeId::new(12)))
                .to_string(),
            CodecError::TrailingBytes { count: 6 }.to_string(),
        ];
        for message in &messages {
            assert!(!message.is_empty());
        }
        assert!(messages[1].contains('2'));
        assert!(messages[2].contains("byte 4"));
        assert!(messages[3].contains("tag 9"));
        assert!(messages[6].contains("node 12"));
    }

    #[test]
    fn pushing_past_the_node_limit_is_refused() {
        // The limit is checked against the arena's length, so a scene whose
        // arena already claims to be full is refused without building a million
        // nodes to get there.
        let mut scene = Scene::new(Size::ZERO);
        scene.nodes.resize(MAX_NODES as usize, Node::container());
        assert_eq!(
            scene.push(NodeId::ROOT, Node::container()),
            Err(SceneError::TooManyNodes)
        );
    }
    #[test]
    fn a_multi_page_scene_round_trips_with_its_page_order() {
        let mut scene = Scene::new(Size::new(64.0, 64.0));
        let mut expected = vec![NodeId::ROOT];
        for frame in 0..4_u32 {
            let page = scene
                .push_page()
                .unwrap_or_else(|error| unreachable!("{error}"));
            scene
                .push(page, Node::text(format!("frame {frame}")))
                .unwrap_or_else(|error| unreachable!("{error}"));
            expected.push(page);
        }
        assert_eq!(scene.pages, expected);

        let decoded = decode(&encode(&scene));
        assert_eq!(decoded, Ok(scene));
        // Whole-scene equality already pins the order; asserting it separately
        // says which property a failure broke.
        assert_eq!(decoded.map(|scene| scene.pages), Ok(expected));
    }

    #[test]
    fn a_buffer_declaring_no_pages_is_refused() {
        let scene = Scene::new(Size::ZERO);
        let mut bytes = encode(&scene);
        // The page list sits immediately after the scale: replace its one entry
        // with an empty list, which shortens the buffer by one `u32`.
        let pages_at = MAGIC.len() + 2 + 4 + 4 + 4;
        let mut rebuilt = bytes[..pages_at].to_vec();
        rebuilt.extend_from_slice(&0_u32.to_le_bytes());
        rebuilt.extend_from_slice(&bytes[pages_at + 8..]);
        bytes = rebuilt;
        assert_eq!(
            decode(&bytes),
            Err(CodecError::InvalidScene(SceneError::NoPages))
        );
    }

    #[test]
    fn a_buffer_whose_pages_share_a_subtree_is_refused() {
        let mut scene = Scene::new(Size::ZERO);
        scene.pages.push(NodeId::ROOT);
        assert_eq!(
            decode(&encode(&scene)),
            Err(CodecError::InvalidScene(SceneError::MultipleParents(
                NodeId::ROOT
            )))
        );
    }

    #[test]
    fn an_invalid_scene_error_carries_the_rule_that_failed() {
        use core::error::Error as _;

        let error = CodecError::InvalidScene(SceneError::NoPages);
        assert!(error.source().is_some());
        assert!(error.to_string().contains("at least one page"));
        assert!(CodecError::NotAScene.source().is_none());
    }

    #[test]
    fn every_font_variant_keyword_survives_the_wire() {
        for variant in FontVariant::ALL {
            round_trip(variant);
        }
        round_trip(&Some(FontVariant::ALL.to_vec()));
        round_trip(&Option::<Vec<FontVariant>>::None);
    }
}
