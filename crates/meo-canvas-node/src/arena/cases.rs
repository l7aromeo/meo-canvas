//! Emits `fixtures/arena-cases.json`, the round trip's expected bytes.
//!
//! One case per property of the four
//! [`arena_group!`](super::group::arena_group) tables, plus one setting every
//! property at once. Each case is a scene with that property set to its probe
//! value, and the bytes [`meo_canvas_scene::codec`] writes for it.
//!
//! Beside them, **one case per node kind**, keyed `__kind_*`. The property
//! cases cover the four style groups, which is an axis every node shares; the
//! kind payload is the axis they do not. Until these existed nothing in any
//! suite encoded a `Text`, an `Image` or a `Path` -- every property case is a
//! styled `Box` -- so a change to a payload's shape passed every gate in the
//! project, and an example caught it instead.
//!
//! # What the artefact is for
//!
//! The TypeScript encoder builds the same one-property scene, hands its arena
//! to the addon's `sceneBytes`, and compares. A disagreement names the
//! property rather than a byte count, and a property added to a Rust table
//! appears here on the next regeneration rather than being quietly untested.
//!
//! Comparing rendered images instead would be weaker in a way that matters:
//! two different scenes can render identically, so a green result would mean
//! less than it appears to, and a property the encoder forgot that happens to
//! change nothing visible would pass.
//!
//! # Keys are Rust field names
//!
//! No TypeScript spelling. The CSS names are the public API —
//! `background_color` is `backgroundColor`, `grid_auto_flow` is `autoFlow`,
//! `box_shadows` is singular `boxShadow` — and that mapping is three decisions
//! about what a caller writes, not a fact derivable from the format. Authoring
//! an API is correct; what must never be hand-authored is the index and type
//! data, which is generated. Agent Zero owns the names and its set-equality
//! check makes the mapping total.
//!
//! # Values mirror the Rust type
//!
//! A `Sides<T>` is emitted in wire order and a gradient carries resolved RGBA
//! stops, because those are what the format holds. Neither is what a caller
//! writes, and the adapters that turn them into the v1 shape live on the
//! TypeScript side, where that translation belongs.
//!
//! # Regenerated, never edited
//!
//! `just arena-cases` writes it and `just arena-cases-check` fails when it is
//! stale, by regenerating to a temp path and diffing. Editing an expected value
//! until a test passes is how a defect becomes the specification, and this
//! project has pinned one as correct once already.

use std::{collections::BTreeMap, fmt::Write as _};

use meo_canvas_scene::{
    Scene, Size,
    geometry::{Corners, Sides},
    node::{
        ImageSource, LineCap, LineJoin, Node, NodeId, NodeKind, NodeTag,
        PathPaint,
    },
    style::{
        Dimension, Length, PaintOrder,
        effect::{BoxShadow, FillRule, Mask, MaskShape, TextShadow, Transform},
        layout::{
            Align, BoxSizing, Direction, Display, FlexDirection, FlexWrap,
            GridAutoFlow, GridPlacement, Justify, Overflow, PositionType,
            TrackSize,
        },
        paint::{
            BackgroundImage, BackgroundRepeat, BackgroundSize, BlendMode,
            BorderStyle, Color, Gradient, GradientGeometry, GradientKind,
            GradientStop, LinearDirection, ObjectFit,
        },
        text::{
            FontStyle, FontVariant, FontWeight, LineHeight, ParagraphStyle,
            Spacing, TextAlign, TextDecoration, TextSegment, TextStroke,
            TextStyle, VerticalAlign,
        },
    },
};

use super::{effects, layout, paint, text};

/// The key the whole-scene case is filed under.
///
/// Leading underscores so it sorts before every field name and cannot collide
/// with one: a Rust field never starts with two.
const ALL_KEY: &str = "__all";

/// The prefix a node-kind case's key carries.
///
/// The same two underscores [`ALL_KEY`] uses, and for the same reason: a Rust
/// field never begins with them, so a kind case can never collide with a
/// property case however the tables grow.
const KIND_PREFIX: &str = "__kind_";

/// The surface every case's scene is drawn on.
///
/// Fixed rather than varied: the case is about one property, and a size that
/// differed per case would put unrelated bytes in every expectation.
const CASE_SIZE: Size = Size {
    width: 100.0,
    height: 50.0,
};

/// The device scale every case's scene carries.
const CASE_SCALE: f32 = 1.0;

/// A value rendered as JSON that mirrors the Rust type.
pub(crate) trait ToJson {
    fn to_json(&self) -> String;
}

/// Implements [`ToJson`] for a `wire_enum!` type as its variant name.
///
/// Taken from `Debug` rather than from a table of strings: a fieldless enum's
/// `Debug` output *is* its variant name, so there is nothing to keep in step.
macro_rules! json_enum {
    ($($name:ident),+ $(,)?) => {
        $(
            impl ToJson for $name {
                fn to_json(&self) -> String {
                    format!("\"{self:?}\"")
                }
            }
        )+
    };
}

json_enum!(
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
    ObjectFit,
    Overflow,
    PaintOrder,
    PositionType,
    TextAlign,
    TextDecoration,
    VerticalAlign,
);

/// Implements [`ToJson`] for a struct as an object of its named fields.
macro_rules! json_struct {
    ($name:ty { $($field:ident),+ $(,)? }) => {
        impl ToJson for $name {
            fn to_json(&self) -> String {
                let parts: Vec<String> = vec![$(
                    format!(
                        "{}:{}",
                        json_string(stringify!($field)),
                        self.$field.to_json()
                    )
                ),+];
                format!("{{{}}}", parts.join(","))
            }
        }
    };
}

/// One arm of a [`json_tagged`] match: a tag, and a value where there is one.
///
/// A variant with nothing to hold omits the value rather than writing null,
/// because "absent" and "null" are different to the reader parsing this.
macro_rules! json_arm {
    ($tag:literal) => {
        format!("{{\"tag\":{}}}", json_string($tag))
    };
    ($tag:literal, $bound:ident) => {
        format!(
            "{{\"tag\":{},\"value\":{}}}",
            json_string($tag),
            $bound.to_json()
        )
    };
}

/// Implements [`ToJson`] for a tagged value as `{"tag":..,"value":..}`.
///
/// The shape a Rust enum with payloads has: which variant, and what it holds.
macro_rules! json_tagged {
    ($name:ty { $($variant:pat => $tag:literal $(, $bound:ident)? );+ $(;)? }) => {
        impl ToJson for $name {
            fn to_json(&self) -> String {
                match self {
                    $( $variant => json_arm!($tag $(, $bound)?), )+
                }
            }
        }
    };
}

impl ToJson for f32 {
    fn to_json(&self) -> String {
        // Non-finite values have no JSON spelling. None reaches here — every
        // probe value is a small finite number — so this is a statement that
        // the artefact never carries one rather than a fallback anyone relies
        // on.
        if self.is_finite() {
            format!("{self}")
        } else {
            String::from("null")
        }
    }
}

impl ToJson for bool {
    fn to_json(&self) -> String {
        format!("{self}")
    }
}

impl ToJson for u8 {
    fn to_json(&self) -> String {
        format!("{self}")
    }
}

impl ToJson for u16 {
    fn to_json(&self) -> String {
        format!("{self}")
    }
}

impl ToJson for u32 {
    fn to_json(&self) -> String {
        format!("{self}")
    }
}

impl ToJson for i16 {
    fn to_json(&self) -> String {
        format!("{self}")
    }
}

impl ToJson for i32 {
    fn to_json(&self) -> String {
        format!("{self}")
    }
}

impl ToJson for String {
    fn to_json(&self) -> String {
        json_string(self)
    }
}

impl<T: ToJson> ToJson for Option<T> {
    fn to_json(&self) -> String {
        self.as_ref()
            .map_or_else(|| String::from("null"), ToJson::to_json)
    }
}

impl<T: ToJson> ToJson for Vec<T> {
    fn to_json(&self) -> String {
        let items: Vec<String> = self.iter().map(ToJson::to_json).collect();
        format!("[{}]", items.join(","))
    }
}

impl<A: ToJson, B: ToJson> ToJson for (A, B) {
    fn to_json(&self) -> String {
        format!("[{},{}]", self.0.to_json(), self.1.to_json())
    }
}

impl<T: ToJson> ToJson for Sides<T> {
    fn to_json(&self) -> String {
        format!(
            "[{},{},{},{}]",
            self.top.to_json(),
            self.right.to_json(),
            self.bottom.to_json(),
            self.left.to_json()
        )
    }
}

impl<T: ToJson> ToJson for Corners<T> {
    fn to_json(&self) -> String {
        format!(
            "[{},{},{},{}]",
            self.top_left.to_json(),
            self.top_right.to_json(),
            self.bottom_right.to_json(),
            self.bottom_left.to_json()
        )
    }
}

impl ToJson for FontWeight {
    fn to_json(&self) -> String {
        self.get().to_json()
    }
}

json_struct!(Color { r, g, b, a });
json_struct!(GradientStop { offset, color });
json_struct!(GridPlacement { start, span });
json_struct!(TextStroke { width, color });
json_struct!(Gradient { geometry, stops });

/// Hand-written rather than emitted by [`json_tagged`], which carries one bound
/// value per variant where these carry two. Same reason [`kind_json`] is.
impl ToJson for LinearDirection {
    fn to_json(&self) -> String {
        match self {
            Self::Angle(degrees) => {
                format!("{{\"tag\":\"angle\",\"value\":{}}}", degrees.to_json())
            }
            Self::Between { start, end } => format!(
                "{{\"tag\":\"between\",\"start\":{},\"end\":{}}}",
                start.to_json(),
                end.to_json()
            ),
        }
    }
}

impl ToJson for GradientGeometry {
    fn to_json(&self) -> String {
        match self {
            Self::Linear { direction } => format!(
                "{{\"kind\":\"Linear\",\"direction\":{}}}",
                direction.to_json()
            ),
            Self::Radial { at } => {
                format!("{{\"kind\":\"Radial\",\"at\":{}}}", at.to_json())
            }
            Self::Conic { at, from } => format!(
                "{{\"kind\":\"Conic\",\"at\":{},\"from\":{}}}",
                at.to_json(),
                from.to_json()
            ),
        }
    }
}

impl ToJson for BackgroundSize {
    fn to_json(&self) -> String {
        match self {
            Self::PerAxis(width, height) => format!(
                "{{\"tag\":\"per-axis\",\"width\":{},\"height\":{}}}",
                width.to_json(),
                height.to_json()
            ),
            Self::Cover => String::from("{\"tag\":\"cover\"}"),
            Self::Contain => String::from("{\"tag\":\"contain\"}"),
        }
    }
}
json_struct!(BackgroundImage {
    source,
    repeat,
    size,
    position
});
json_struct!(Transform {
    translate_x,
    translate_y,
    rotate_degrees,
    scale_x,
    scale_y,
    origin
});
json_struct!(BoxShadow {
    inset,
    offset_x,
    offset_y,
    blur,
    spread,
    color
});
json_struct!(TextShadow {
    offset_x,
    offset_y,
    blur,
    color
});

json_tagged!(Length {
    Self::Points(v) => "points", v;
    Self::Percent(v) => "percent", v;
});
json_tagged!(Dimension {
    Self::Auto => "auto";
    Self::Points(v) => "points", v;
    Self::Percent(v) => "percent", v;
});
json_tagged!(LineHeight {
    Self::Number(v) => "number", v;
    Self::Length(v) => "length", v;
    Self::Percent(v) => "percent", v;
});
json_tagged!(TrackSize {
    Self::Auto => "auto";
    Self::Points(v) => "points", v;
    Self::Percent(v) => "percent", v;
    Self::Fraction(v) => "fraction", v;
});
json_tagged!(Spacing {
    Self::Normal => "normal";
    Self::Points(v) => "points", v;
    Self::Em(v) => "em", v;
});
json_tagged!(ImageSource {
    Self::Path(v) => "path", v;
    Self::Url(v) => "url", v;
    Self::Bytes(v) => "bytes", v;
});
json_struct!(ParagraphStyle {
    max_lines,
    ellipsis
});
json_struct!(TextSegment { text, style });

/// A segment's style, as the properties it actually sets.
///
/// Walked from the [`text`] table rather than written out field by field, so a
/// property added there appears here on the next regeneration. A field the
/// style leaves unset renders as `null` and is omitted: the artefact describes
/// what the case *says*, and a paragraph of fifteen nulls describes nothing.
impl ToJson for TextStyle {
    fn to_json(&self) -> String {
        let parts: Vec<String> = text::INDICES
            .iter()
            .zip(text::NAMES)
            .filter_map(|(index, name)| {
                let value = text::field_json(self, *index)?;
                (value != "null")
                    .then(|| format!("{}:{value}", json_string(name)))
            })
            .collect();
        format!("{{{}}}", parts.join(","))
    }
}

json_tagged!(PathPaint {
    Self::Solid(v) => "solid", v;
    Self::Gradient(v) => "gradient", v;
});

impl ToJson for Mask {
    fn to_json(&self) -> String {
        match self {
            Self::Image(source) => {
                format!("{{\"tag\":\"image\",\"value\":{}}}", source.to_json())
            }
            Self::Shape(shape) => {
                format!("{{\"tag\":\"shape\",\"value\":{}}}", shape.to_json())
            }
            Self::Path { data, fill_rule } => format!(
                "{{\"tag\":\"path\",\"data\":{},\"fillRule\":{}}}",
                data.to_json(),
                fill_rule.to_json()
            ),
            Self::Gradient(gradient) => format!(
                "{{\"tag\":\"gradient\",\"value\":{}}}",
                gradient.to_json()
            ),
        }
    }
}

/// A JSON string literal, with the escapes the grammar requires.
fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // Everything below a space needs the `\u` form; nothing above it
            // does, and passing text through unescaped keeps the artefact
            // readable.
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Standard base64, which is how bytes reach JSON.
///
/// Sixteen lines rather than a dependency: the artefact is the only thing in
/// this crate that needs it, and a crate added for one encoder is a crate every
/// consumer of the addon then carries.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let mut buffer = [0_u8; 3];
        buffer[..chunk.len()].copy_from_slice(chunk);
        let packed = (u32::from(buffer[0]) << 16)
            | (u32::from(buffer[1]) << 8)
            | u32::from(buffer[2]);
        for slot in 0..4 {
            if slot <= chunk.len() {
                let index = (packed >> (18 - 6 * slot)) & 0x3F;
                out.push(char::from(ALPHABET[index as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// One case: the property's value, and what the byte format writes for it.
struct Case {
    group: &'static str,
    index: u32,
    value: String,
    bytes: String,
}

/// The scene a case is built from: one page, one container, styled.
fn case_scene(apply: impl FnOnce(&mut Node)) -> Scene {
    let mut scene = Scene::new(CASE_SIZE);
    scene.scale = CASE_SCALE;
    if let Some(root) = scene.get_mut(NodeId::ROOT) {
        apply(root);
    }
    scene
}

/// The node kinds a case is built for, each with a payload that sets every
/// field the format writes for it.
///
/// A property case covers the four style groups, which is an axis every node
/// shares. **The kind payload is the axis they do not**, and until these
/// existed nothing encoded a `Text`, an `Image` or a `Path` at all -- the
/// fifty-four property cases are all styled Boxes, so a change to a payload's
/// shape passed every gate in the project and was caught by running an example.
///
/// One entry per [`NodeTag`], asserted in `every_node_kind_has_a_case`, plus
/// one per [`ImageSource`] variant -- the three write a string, a string and a
/// byte buffer, so a writer that got the buffer wrong would pass on the
/// strength of a path -- plus both arms of the text discriminant.
fn kind_cases() -> Vec<KindCase> {
    let image = |source| NodeKind::Image {
        source,
        // Every field away from its default, so a payload that stopped writing
        // one shows as a byte difference rather than as a value that happened
        // to match.
        fit: ObjectFit::Cover,
        position: (Length::Percent(0.25), Length::Points(3.0)),
        frame: Some(2),
    };

    vec![
        KindCase::built(
            format!("{KIND_PREFIX}box"),
            NodeTag::Box,
            NodeKind::Box,
        ),
        // The markup arm of the text discriminant. The scene holds segments
        // either way -- the discriminant is the arena's, not the codec's -- so
        // what this pins is that a probe writing the string produces the runs
        // Rust parses out of it. A string chosen to parse into something no
        // other case encodes, since two cases with equal bytes mean one of
        // them writes nothing.
        KindCase::markup(
            format!("{KIND_PREFIX}text_markup"),
            "one <b>two</b>",
            ParagraphStyle::default(),
        ),
        KindCase::built(
            format!("{KIND_PREFIX}text"),
            NodeTag::Text,
            NodeKind::Text {
                segments: vec![
                    // One segment carrying a mask bit and one carrying none:
                    // the empty style still writes its mask, which is the slot
                    // a writer is most likely to skip.
                    TextSegment {
                        text: String::from("a"),
                        style: TextStyle {
                            font_weight: Some(FontWeight::BOLD),
                            ..TextStyle::default()
                        },
                    },
                    TextSegment {
                        text: String::from("b"),
                        style: TextStyle::default(),
                    },
                ],
                // Two segments rather than one, so a count written as a
                // constant fails here instead of passing.
                paragraph: ParagraphStyle {
                    max_lines: Some(2),
                    ellipsis: Some(String::from("...")),
                },
            },
        ),
        KindCase::built(
            format!("{KIND_PREFIX}image_path"),
            NodeTag::Image,
            image(ImageSource::Path(String::from("probe.png"))),
        ),
        KindCase::built(
            format!("{KIND_PREFIX}image_url"),
            NodeTag::Image,
            image(ImageSource::Url(String::from("https://probe.invalid/a"))),
        ),
        KindCase::built(
            format!("{KIND_PREFIX}image_bytes"),
            NodeTag::Image,
            image(ImageSource::Bytes(vec![1, 2, 3])),
        ),
        // Two path cases, split along what a surface can say rather than along
        // anything in the format. Everything here is solid paint, which both
        // surfaces spell, so this one is probed from both sides.
    ]
    .into_iter()
    .chain(path_cases())
    .collect()
}

/// The path kind's cases, lifted out because `kind_cases` outgrew its line
/// budget — and because the pair belongs together: one carries a `view_box`
/// and a `stretch` and the other carries neither, which is what covers both
/// arms of each.
fn path_cases() -> Vec<KindCase> {
    let gradient = Gradient {
        // The endpoint arm, which is the reason the geometry moved onto the
        // kinds and which nothing else in the artefact reaches. Quarters and
        // three-quarters rather than whole percentages: `Percent(1.0)` is the
        // one value at which a hundredfold units disagreement between the two
        // surfaces encodes identically, and a quarter is exact in an `f32`.
        geometry: GradientGeometry::Linear {
            direction: LinearDirection::Between {
                start: (Length::Percent(0.25), Length::Points(3.0)),
                end: (Length::Percent(0.75), Length::Percent(0.25)),
            },
        },
        stops: vec![GradientStop {
            offset: 0.5,
            color: Color::rgba(9, 8, 7, 6),
        }],
    };

    vec![
        KindCase::built(
            format!("{KIND_PREFIX}path"),
            NodeTag::Path,
            NodeKind::Path {
                data: String::from("M0 0 L4 4"),
                view_box: None,
                stretch: false,
                fill: Some(PathPaint::Solid(Color::rgba(1, 2, 3, 4))),
                stroke: Some(PathPaint::Solid(Color::rgba(5, 6, 7, 8))),
                line_width: 2.5,
                fill_rule: FillRule::EvenOdd,
                line_cap: LineCap::Round,
                line_join: LineJoin::Bevel,
                line_dash: vec![1.0, 2.0],
                line_dash_offset: 0.5,
            },
        ),
        // The gradient arm of the paint tag, which is a two-armed tag inside
        // an option: a case with only solid paint leaves half of it
        // unwritten. Kept apart because a gradient has no spelling on
        // the TypeScript surface at all -- `gradient` and
        // `background_image` are absent from its paint table too -- so
        // folding it into the case above would make the whole path
        // payload unreachable from that side to cover one arm.
        KindCase::built(
            format!("{KIND_PREFIX}path_gradient"),
            NodeTag::Path,
            NodeKind::Path {
                data: String::from("M0 0 L4 4"),
                // The one case that carries a box. Both cases writing `None`
                // would leave the four floats on the wire unread by any case,
                // and an encoder that dropped them would still pass.
                view_box: Some((-1.0, -2.0, 8.0, 4.0)),
                stretch: true,
                fill: None,
                stroke: Some(PathPaint::Gradient(gradient)),
                line_width: 1.0,
                fill_rule: FillRule::NonZero,
                line_cap: LineCap::Butt,
                line_join: LineJoin::Miter,
                line_dash: Vec::new(),
                line_dash_offset: 0.0,
            },
        ),
    ]
}

/// One node kind's case: the scene to build, and how a probe writes it.
struct KindCase {
    /// The artefact key.
    key: String,
    /// The opcode the node's first slot carries.
    tag: NodeTag,
    /// The payload the scene ends up holding.
    kind: NodeKind,
    /// The markup a probe writes instead of building the payload.
    ///
    /// `Some` for the one case that exercises the arena's text discriminant.
    /// The scene is the same either way -- [`meo_canvas_scene::Scene`] holds
    /// segments however they arrived -- so a probe that wrote the runs
    /// directly would produce these bytes without ever setting the
    /// discriminant, and the case would pass while testing nothing.
    markup: Option<String>,
}

impl KindCase {
    /// A case whose payload the probe builds field by field.
    fn built(key: String, tag: NodeTag, kind: NodeKind) -> Self {
        Self {
            key,
            tag,
            kind,
            markup: None,
        }
    }

    /// A case whose payload the probe writes as a markup string.
    ///
    /// The segments come from [`meo_canvas_core::markup::parse_paragraph`], the
    /// same function the decoder calls, so the expectation is what the parser
    /// actually produces rather than a second reading of the markup.
    fn markup(key: String, source: &str, paragraph: ParagraphStyle) -> Self {
        Self {
            key,
            tag: NodeTag::Text,
            kind: NodeKind::Text {
                segments: meo_canvas_core::markup::parse_paragraph(source),
                paragraph,
            },
            markup: Some(source.to_owned()),
        }
    }

    /// The case's `value`: the payload, and the markup where there is one.
    fn value(&self) -> String {
        let payload = kind_json(&self.kind);
        self.markup.as_ref().map_or_else(
            || payload.clone(),
            |markup| {
                format!(
                    "{{\"markup\":{},\"parses_to\":{payload}}}",
                    json_string(markup)
                )
            },
        )
    }
}

/// One node kind's payload as JSON.
///
/// Hand-written rather than emitted by [`json_tagged`], which carries one bound
/// value per variant and these carry up to nine. The fields are named in wire
/// order, which is the order the grammar in [`crate::arena`] lists them.
fn kind_json(kind: &NodeKind) -> String {
    match kind {
        NodeKind::Box => String::from("{}"),
        NodeKind::Text {
            segments,
            paragraph,
        } => format!(
            "{{\"paragraph\":{},\"segments\":{}}}",
            paragraph.to_json(),
            segments.to_json()
        ),
        NodeKind::Image {
            source,
            fit,
            position,
            frame,
        } => format!(
            "{{\"source\":{},\"fit\":{},\"position\":{},\"frame\":{}}}",
            source.to_json(),
            fit.to_json(),
            position.to_json(),
            frame.to_json()
        ),
        NodeKind::Path {
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
        } => format!(
            "{{\"data\":{},\"view_box\":{},\"stretch\":{},\"fill\":{},\"stroke\":{},\
             \"line_width\":{},\"fill_rule\":{},\"line_cap\":{},\
             \"line_join\":{},\"line_dash\":{},\"line_dash_offset\":{}}}",
            data.to_json(),
            match view_box {
                None => String::from("null"),
                Some((min_x, min_y, width, height)) => format!(
                    "[{},{},{},{}]",
                    min_x.to_json(),
                    min_y.to_json(),
                    width.to_json(),
                    height.to_json()
                ),
            },
            stretch.to_json(),
            fill.to_json(),
            stroke.to_json(),
            line_width.to_json(),
            fill_rule.to_json(),
            line_cap.to_json(),
            line_join.to_json(),
            line_dash.to_json(),
            line_dash_offset.to_json()
        ),
    }
}

fn case_bytes(scene: &Scene) -> String {
    base64(&meo_canvas_scene::codec::encode(scene))
}

/// Every case, keyed by Rust field name.
fn cases() -> BTreeMap<String, Case> {
    let mut cases = BTreeMap::new();

    for (index, name) in layout::INDICES.iter().zip(layout::NAMES) {
        let Some(style) = layout::probe(*index) else {
            continue;
        };
        let scene = case_scene(|root| root.layout = style.clone());
        cases.insert(
            (*name).to_owned(),
            Case {
                group: "layout",
                index: *index,
                value: layout::field_json(&style, *index)
                    .unwrap_or_else(|| String::from("null")),

                bytes: case_bytes(&scene),
            },
        );
    }
    for (index, name) in paint::INDICES.iter().zip(paint::NAMES) {
        let Some(style) = paint::probe(*index) else {
            continue;
        };
        let scene = case_scene(|root| root.paint = style.clone());
        cases.insert(
            (*name).to_owned(),
            Case {
                group: "paint",
                index: *index,
                value: paint::field_json(&style, *index)
                    .unwrap_or_else(|| String::from("null")),

                bytes: case_bytes(&scene),
            },
        );
    }
    for (index, name) in text::INDICES.iter().zip(text::NAMES) {
        let Some(style) = text::probe(*index) else {
            continue;
        };
        let scene = case_scene(|root| root.text = style.clone());
        cases.insert(
            (*name).to_owned(),
            Case {
                group: "text",
                index: *index,
                value: text::field_json(&style, *index)
                    .unwrap_or_else(|| String::from("null")),

                bytes: case_bytes(&scene),
            },
        );
    }
    for (index, name) in effects::INDICES.iter().zip(effects::NAMES) {
        let Some(style) = effects::probe(*index) else {
            continue;
        };
        let scene = case_scene(|root| root.effects = style.clone());
        cases.insert(
            (*name).to_owned(),
            Case {
                group: "effects",
                index: *index,
                value: effects::field_json(&style, *index)
                    .unwrap_or_else(|| String::from("null")),

                bytes: case_bytes(&scene),
            },
        );
    }

    for case in kind_cases() {
        let scene = case_scene(|root| root.kind = case.kind.clone());
        cases.insert(
            case.key.clone(),
            Case {
                group: "kind",
                // The tag's own wire value, which is what the TypeScript writer
                // puts in the node's first slot. A property case's index names
                // a mask bit; a kind case's names the opcode.
                index: u32::from(case.tag.to_wire()),
                value: case.value(),
                bytes: case_bytes(&scene),
            },
        );
    }

    let whole = case_scene(|root| {
        root.layout = layout::probe_all();
        root.paint = paint::probe_all();
        root.text = text::probe_all();
        root.effects = effects::probe_all();
    });
    cases.insert(
        ALL_KEY.to_owned(),
        Case {
            group: "all",
            index: u32::MAX,
            value: String::from("null"),
            bytes: case_bytes(&whole),
        },
    );

    cases
}

/// The artefact's own text.
fn render(cases: &BTreeMap<String, Case>) -> String {
    let mut out = String::from("{\n");
    out.push_str("  \"$comment\": [\n");
    for line in [
        "GENERATED by `just arena-cases`. Never edit this file.",
        "One case per property of the arena tables in crates/meo-canvas-node/src/arena.rs, plus __all setting every property at once.",
        "One case per node kind, keyed __kind_*, whose `index` is the NodeTag opcode rather than a mask bit and whose `value` describes the payload in wire order. The property cases are all styled Boxes, so these are the only ones that encode a payload at all.",
        "Two text cases, because the arena's text payload carries a discriminant the codec does not: __kind_text_markup writes a string the renderer parses and carries `markup` beside the `parses_to` it must produce, and __kind_text writes runs the caller built. Both reach the same kind of scene, so a probe that built runs for the markup case would match its bytes without ever setting the discriminant.",
        "Keys are Rust field names. The TypeScript spelling is the public API and lives in the encoder, not here.",
        "`value` mirrors the Rust type; the adapters that turn it into what a caller writes live on the TypeScript side.",
        "`bytes` is base64 of meo_canvas_scene::codec::encode of a scene with that one property set.",
        "IF THE TWO DISAGREE, THE ENCODER IS WRONG UNTIL PROVEN OTHERWISE. Adjusting an expected value until a test passes is how a defect becomes the specification, and this project has pinned one as correct once already.",
    ] {
        let _ = writeln!(out, "    {},", json_string(line));
    }
    // The last entry carries no comma, which JSON requires and a list built by
    // a loop forgets.
    let _ = writeln!(
        out,
        "    {}",
        json_string(
            "Regenerate after changing a table: `just arena-cases`. `just arena-cases-check` fails when this file is stale."
        )
    );
    out.push_str("  ],\n");

    let _ = writeln!(
        out,
        "  \"$size\": [{}, {}],",
        CASE_SIZE.width, CASE_SIZE.height
    );
    let _ = writeln!(out, "  \"$scale\": {CASE_SCALE},");
    out.push_str("  \"cases\": {\n");

    let total = cases.len();
    for (position, (name, case)) in cases.iter().enumerate() {
        let comma = if position + 1 == total { "" } else { "," };
        let _ = writeln!(
            out,
            "    {}: {{ \"group\": {}, \"index\": {}, \"value\": {}, \"bytes\": {} }}{comma}",
            json_string(name),
            json_string(case.group),
            case.index,
            case.value,
            json_string(&case.bytes)
        );
    }

    out.push_str("  }\n}\n");
    out
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use meo_canvas_scene::node::{ImageSource, NodeKind, NodeTag};

    use super::{ALL_KEY, cases, kind_cases, render};

    /// Where the artefact is written unless `MEO_ARENA_CASES` names elsewhere.
    fn output_path() -> PathBuf {
        std::env::var("MEO_ARENA_CASES").map_or_else(
            |_| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../fixtures/arena-cases.json")
            },
            PathBuf::from,
        )
    }

    /// Writes the artefact.
    ///
    /// Ignored by default: it writes a tracked file, which a plain `cargo test`
    /// must not do. `just arena-cases` runs it, and `just arena-cases-check`
    /// runs it against a temp path and diffs — a file comparison rather than a
    /// question to git, because `git status` reports a file as changed whether
    /// it is untracked, written or staged, so a check built on it refuses the
    /// very workflow it exists to support.
    #[test]
    #[ignore = "writes a tracked artefact; run through `just arena-cases`"]
    fn emit_arena_cases() {
        let path = output_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|error| unreachable!("{error}"));
        }
        std::fs::write(&path, render(&cases()))
            .unwrap_or_else(|error| unreachable!("{error}"));
    }

    /// Every property has a case, and the whole-scene case is beside them.
    #[test]
    fn there_is_a_case_for_every_property() {
        let cases = cases();
        let expected = super::layout::COUNT
            + super::paint::COUNT
            + super::text::COUNT
            + super::effects::COUNT;
        assert_eq!(
            cases.len(),
            expected + 1 + kind_cases().len(),
            "one case per property, one per node kind, plus the whole-scene one"
        );
        assert!(cases.contains_key(ALL_KEY));
    }

    /// The text discriminant has a case for each of its two arms.
    ///
    /// A markup case and a runs case reach the same `Scene` -- the discriminant
    /// is the arena's and the codec never sees it -- so a probe that built the
    /// runs directly would produce the markup case's bytes without ever setting
    /// the discriminant. The two arms are the reason both cases exist; one of
    /// them alone would pass while half the format went unwritten.
    #[test]
    fn both_arms_of_the_text_discriminant_have_a_case() {
        let text: Vec<super::KindCase> = kind_cases()
            .into_iter()
            .filter(|case| case.tag == NodeTag::Text)
            .collect();

        assert!(
            text.iter().any(|case| case.markup.is_some()),
            "no case writes text as markup"
        );
        assert!(
            text.iter().any(|case| case.markup.is_none()),
            "no case writes text as runs the caller built"
        );
    }

    /// Every node kind has a case, so a payload cannot go untested.
    ///
    /// The partition rule the property cases follow, applied to the axis they
    /// do not cover. A kind added to [`NodeTag`] fails here rather than
    /// shipping with a payload nothing encodes -- which is what happened: the
    /// arena's text payload was changed, every gate passed, and an example
    /// caught it.
    #[test]
    fn every_node_kind_has_a_case() {
        let covered: std::collections::HashSet<NodeTag> =
            kind_cases().into_iter().map(|case| case.tag).collect();
        for tag in NodeTag::ALL {
            assert!(
                covered.contains(tag),
                "{tag:?} has no round-trip case, so nothing encodes its payload"
            );
        }
    }

    /// Every `ImageSource` variant has a case.
    ///
    /// Separate from the kind rule because the three are one node kind and
    /// three wire shapes -- two strings and a byte buffer. A writer that got
    /// the buffer wrong would pass on the strength of the path case.
    #[test]
    fn every_image_source_variant_has_a_case() {
        let payloads: Vec<NodeKind> =
            kind_cases().into_iter().map(|case| case.kind).collect();
        let sources: Vec<&ImageSource> = payloads
            .iter()
            .filter_map(|kind| match kind {
                NodeKind::Image { source, .. } => Some(source),
                _ => None,
            })
            .collect();

        assert!(
            sources.iter().any(|s| matches!(s, ImageSource::Path(_))),
            "no case carries a path source"
        );
        assert!(
            sources.iter().any(|s| matches!(s, ImageSource::Url(_))),
            "no case carries a url source"
        );
        assert!(
            sources.iter().any(|s| matches!(s, ImageSource::Bytes(_))),
            "no case carries a bytes source"
        );
    }

    /// A kind case's key is prefixed, so it can never shadow a property.
    #[test]
    fn a_kind_case_cannot_collide_with_a_property_case() {
        for case in kind_cases() {
            assert!(
                case.key.starts_with(super::KIND_PREFIX),
                "{} is not in the kind namespace",
                case.key
            );
        }
    }

    /// No two cases carry the same bytes.
    ///
    /// The check that the cases are actually distinct scenes. Two properties
    /// producing identical bytes would mean one of them wrote nothing — the
    /// silent-pass failure the probe rule exists to prevent, caught here
    /// against the encoded output rather than against the value.
    #[test]
    fn every_case_encodes_to_something_of_its_own() {
        let cases = cases();
        let mut seen: std::collections::BTreeMap<&str, &str> =
            std::collections::BTreeMap::new();
        let mut collisions = Vec::new();

        for (name, case) in &cases {
            if let Some(first) = seen.insert(&case.bytes, name) {
                collisions.push(format!("{first} and {name}"));
            }
        }
        assert!(
            collisions.is_empty(),
            "these cases encode identically, so one of them writes nothing: \
             {collisions:?}"
        );
    }

    /// The artefact is valid JSON with the header it promises.
    #[test]
    fn the_artefact_says_what_it_is() {
        let text = render(&cases());
        assert!(text.contains("GENERATED by `just arena-cases`"));
        assert!(text.contains("THE ENCODER IS WRONG UNTIL PROVEN OTHERWISE"));
        // Balanced braces is a cheap structural check; the JavaScript side
        // parsing it is the real one.
        assert_eq!(
            text.matches('{').count(),
            text.matches('}').count(),
            "the artefact's braces do not balance"
        );
        assert!(text.ends_with("}\n"));
    }

    #[test]
    fn base64_matches_the_standard_alphabet() {
        assert_eq!(super::base64(b""), "");
        assert_eq!(super::base64(b"f"), "Zg==");
        assert_eq!(super::base64(b"fo"), "Zm8=");
        assert_eq!(super::base64(b"foo"), "Zm9v");
        assert_eq!(super::base64(b"foob"), "Zm9vYg==");
        assert_eq!(super::base64(&[0xFF, 0xFE, 0xFD]), "//79");
    }

    #[test]
    fn strings_are_escaped_the_way_json_requires() {
        assert_eq!(super::json_string("a\"b"), "\"a\\\"b\"");
        assert_eq!(super::json_string("a\\b"), "\"a\\\\b\"");
        assert_eq!(super::json_string("a\nb"), "\"a\\nb\"");
        assert_eq!(super::json_string("\u{1}"), "\"\\u0001\"");
        assert_eq!(super::json_string("héllo"), "\"héllo\"");
    }
}
