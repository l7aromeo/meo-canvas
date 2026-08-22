//! Emits `fixtures/arena-cases.json`, the round trip's expected bytes.
//!
//! One case per property of the four
//! [`arena_group!`](super::group::arena_group) tables, plus one setting every
//! property at once. Each case is a scene with that property set to its probe
//! value, and the bytes [`meo_canvas_scene::codec`] writes for it.
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
    node::{ImageSource, LineCap, LineJoin, Node, NodeId, PathPaint},
    style::{
        Dimension, Length, PaintOrder,
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

use super::{effects, layout, paint, text};

/// The key the whole-scene case is filed under.
///
/// Leading underscores so it sorts before every field name and cannot collide
/// with one: a Rust field never starts with two.
const ALL_KEY: &str = "__all";

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
json_struct!(Gradient {
    kind,
    stops,
    angle_degrees,
    center
});
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

    use super::{ALL_KEY, cases, render};

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
            expected + 1,
            "one case per property plus the whole-scene one"
        );
        assert!(cases.contains_key(ALL_KEY));
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
