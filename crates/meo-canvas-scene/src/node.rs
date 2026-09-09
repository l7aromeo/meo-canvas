//! The nodes a scene is made of.
//!
//! One [`Node`] struct carrying four style groups and a [`NodeKind`] payload,
//! rather than a struct per kind. `canvas.type.ts` reaches the same shape from
//! the other direction: every one of its components extends `BoxProps`, so a
//! `Text` accepts padding and a `Path` accepts a background. Sharing the style
//! groups here is what keeps that true without repeating them six times.
//!
//! The renderer matches [`NodeKind`] exhaustively, so a kind added here
//! produces a compile error in the paint stage rather than a node that silently
//! draws nothing.
//!
//! Children are [`NodeId`] indices into [`crate::Scene::nodes`] rather than
//! owned `Vec<Node>`. A flat arena is what makes an id stable across the
//! [`crate::codec`] round trip and what lets a caller keep a handle to a node
//! it has already given away.

use crate::{
    style::{
        Length,
        effect::{Effects, FillRule},
        layout::LayoutStyle,
        paint::{Color, Gradient, ObjectFit, PaintStyle},
        text::{ParagraphStyle, TextSegment, TextStyle},
    },
    wire::wire_enum,
};

/// A handle to a node within one scene.
///
/// An arena index, so it is meaningful only against the scene it came from. Two
/// scenes index their own arenas and their ids are unrelated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct NodeId(u32);

impl NodeId {
    /// The id of the node a scene is rooted at.
    ///
    /// [`crate::Scene::new`] puts the root at index zero and nothing moves it,
    /// so a caller building a scene from scratch can name the root without
    /// keeping the value [`crate::Scene::new`] returned.
    pub const ROOT: Self = Self(0);

    /// Wraps a raw arena index.
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    /// The raw arena index.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// What the request for a [`ImageSource::Url`] carries beyond the URL.
///
/// One source's options, not one scene's. A scene names several URLs and they
/// are as often several origins, so the credential for one is the wrong thing
/// to send to another -- and the three places a URL can appear (an image's
/// source, a background image, a mask) share this type rather than each
/// growing an options field of their own.
///
/// A struct rather than a bare list of headers so a timeout or a redirect
/// policy can join it later. Open -- no `#[non_exhaustive]` -- because callers
/// build one, which is the test that also keeps [`NodeKind::Text`] open.
///
/// # Empty is the absence
///
/// There is no `Option<HttpOptions>` anywhere: `None` and an empty `headers`
/// send the same request, so the distinction would be one the wire carries and
/// nobody can observe. [`Default`] is empty.
///
/// # The encoded form is canonical
///
/// [`HttpOptions::canonical`] is what reaches the wire, not this list as the
/// caller wrote it: names are ASCII-lower-cased, repeats of one name are
/// combined into a single comma-joined value in the order they were written,
/// and the result is sorted by name. The reason is cross-surface byte
/// equality. The npm surface builds its headers through the platform's
/// `Headers`, which does all three itself -- measured, not assumed:
///
/// ```text
/// new Headers([["X-Zeta","1"],["Authorization","B"],["x-zeta","2"]])
///   -> [["authorization","B"],["x-zeta","1, 2"]]
/// ```
///
/// so a scene built the same way on the two surfaces has to encode to the same
/// bytes or the fixture comparison between them reads as a codec defect. All
/// three are lossless as HTTP: field names are case-insensitive, the order of
/// *different* names is not significant, and a repeated name is defined as the
/// comma-joined list. `Set-Cookie` is the one field that is not, and it is a
/// response header that cannot appear on a request.
///
/// **At the wire rather than in the constructor**, because `headers` is public
/// and a struct literal bypasses anything a constructor maintains.
///
/// # These reach the wire
///
/// [`crate::codec::encode`] writes them, so a scene saved to disk carries
/// whatever headers it was built with. That is what makes a scene the whole
/// contract, and it is worth knowing before putting a bearer token in one.
/// [`Debug`] prints header **names** and redacts every value, so a scene
/// logged or printed in a panic does not leak the credential -- encoding is
/// the deliberate act, printing is the accidental one.
#[derive(Clone, PartialEq, Eq, Hash, Default)]
pub struct HttpOptions {
    /// Header names and values, in the order they are sent.
    ///
    /// A list rather than a map: a header may legitimately appear more than
    /// once, and a map would silently keep one of them.
    pub headers: Vec<(String, String)>,
}

impl HttpOptions {
    /// Options carrying nothing.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            headers: Vec::new(),
        }
    }

    /// Appends one header.
    #[must_use]
    pub fn header(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    /// The headers as the wire carries them.
    ///
    /// Lower-cased, repeats combined, then sorted -- see the type's own doc
    /// for why each of the three, and why here rather than in
    /// [`HttpOptions::header`].
    ///
    /// **Combining runs before sorting, and the two do not commute in spirit
    /// even though they do in effect.** A repeated name's values are joined in
    /// the order the caller wrote them, which is the order HTTP says is
    /// significant; sorting first would rely on the sort being stable to get
    /// the same answer, and a later reader reaching for `sort_unstable_by`
    /// would then break something no test names.
    ///
    /// **ASCII case folding, deliberately, and `str::to_lowercase` is the one
    /// that looks more correct.** It is Unicode-aware; `Headers` folds ASCII
    /// only, as the HTTP specification says. The two disagree on `İ`, which
    /// Unicode lower-cases to *two* scalars -- a different name, sorting
    /// somewhere else, encoding to different bytes than the other surface. It
    /// does not matter whether a header name can carry one: "unreachable" is a
    /// claim about a validator neither surface has been shown to run, and
    /// `to_ascii_lowercase` closes the gap without needing the claim to hold.
    /// Pinned by the last row of
    /// `the_canonical_form_lower_cases_combines_and_sorts`.
    #[must_use]
    pub fn canonical(&self) -> Vec<(String, String)> {
        let mut combined: Vec<(String, String)> = Vec::new();
        for (name, value) in &self.headers {
            let name = name.to_ascii_lowercase();
            if let Some((_, held)) =
                combined.iter_mut().find(|(held, _)| *held == name)
            {
                held.push_str(", ");
                held.push_str(value);
            } else {
                combined.push((name, value.clone()));
            }
        }
        combined.sort_by(|(left, _), (right, _)| left.cmp(right));
        combined
    }

    /// These options over `base`, one name at a time.
    ///
    /// A source's options merge over the scene's rather than replacing them,
    /// so a source naming one header does not silently drop the scene's
    /// credentials -- the same rule the npm surface applies, and the same one
    /// `Style::merge` follows for the same reason. Both sides are canonical
    /// first, so the comparison is case-insensitive by construction.
    #[must_use]
    pub fn over(&self, base: &Self) -> Self {
        let mut headers = base.canonical();
        for (name, value) in self.canonical() {
            if let Some((_, held)) =
                headers.iter_mut().find(|(held, _)| *held == name)
            {
                *held = value;
            } else {
                headers.push((name, value));
            }
        }
        headers.sort_by(|(left, _), (right, _)| left.cmp(right));
        Self { headers }
    }
}

impl core::fmt::Debug for HttpOptions {
    /// Names every header and prints no value.
    ///
    /// A derived `Debug` here puts `Authorization: Bearer ...` into whatever
    /// printed the scene -- a panic message, a test failure, a log line -- and
    /// none of those is a place the caller chose to put a credential. The
    /// names are kept because "which headers are set" is the question a person
    /// debugging a 401 is actually asking.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        struct Redacted<'a>(&'a [(String, String)]);

        impl core::fmt::Debug for Redacted<'_> {
            fn fmt(
                &self,
                f: &mut core::fmt::Formatter<'_>,
            ) -> core::fmt::Result {
                let mut list = f.debug_list();
                for (name, _) in self.0 {
                    list.entry(&format_args!("{name}: <redacted>"));
                }
                list.finish()
            }
        }

        f.debug_struct("HttpOptions")
            .field("headers", &Redacted(&self.headers))
            .finish()
    }
}

/// Where an image finds its bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ImageSource {
    /// A path on the local filesystem.
    Path(String),
    /// An absolute URL, and what the request for it carries.
    ///
    /// `meo-canvas-core` does not fetch unless it was built with `net`. A
    /// scene reaching a renderer without that feature is
    /// [`crate::surface::OnImageError`]'s business, not a network call; the
    /// surface that accepted the URL is the one that resolves it.
    ///
    /// Two sources naming one URL with different [`HttpOptions`] are two
    /// sources: the decode cache is keyed by the whole `ImageSource`, so one
    /// authenticated fetch does not answer for an anonymous one.
    Url {
        /// The URL to fetch.
        url: String,
        /// What the request carries beyond the URL.
        http: HttpOptions,
    },
    /// Bytes the caller already holds, in any container the renderer decodes.
    Bytes(Vec<u8>),
}

impl ImageSource {
    /// A URL fetched with no options.
    ///
    /// The spelling the great majority of call sites want, and the reason the
    /// struct variant costs them nothing.
    #[must_use]
    pub fn url(url: impl Into<String>) -> Self {
        Self::Url {
            url: url.into(),
            http: HttpOptions::new(),
        }
    }

    /// A URL fetched with the given options.
    #[must_use]
    pub fn url_with(url: impl Into<String>, http: HttpOptions) -> Self {
        Self::Url {
            url: url.into(),
            http,
        }
    }
}

wire_enum! {
    /// How a stroke's ends are drawn.
    pub enum LineCap {
        /// Cut off flush with the end point.
        Butt = 0,
        /// A half-disc past the end point.
        Round = 1,
        /// A half-square past the end point.
        Square = 2,
    }
}

wire_enum! {
    /// How a stroke's corners are drawn.
    pub enum LineJoin {
        /// Cut across the corner.
        Bevel = 0,
        /// An arc around the corner.
        Round = 1,
        /// Extended until the outer edges meet.
        Miter = 2,
    }
}

/// How a path is filled or stroked.
///
/// A gradient is held by value rather than by a reference into a shared table:
/// a scene has a handful of painted paths, and a table would add an indirection
/// the wire format would then have to keep consistent.
#[derive(Debug, Clone, PartialEq)]
pub enum PathPaint {
    /// A flat colour.
    Solid(Color),
    /// A gradient.
    Gradient(Gradient),
}

wire_enum! {
    /// Which [`NodeKind`] a record holds, as one number.
    ///
    /// The single definition both representations read. [`crate::codec`]
    /// writes it as its kind tag byte and the addon's `f64` arena writes it as
    /// its opcode, so the two are the same number by construction rather than
    /// by two tables agreeing -- which is the failure the byte format's
    /// hand-written discriminants exist to avoid in the first place.
    pub enum NodeTag {
        /// A container.
        Box = 0,
        /// A paragraph.
        Text = 1,
        /// A raster image.
        Image = 2,
        /// An SVG path.
        Path = 3,
    }
}

/// What a node draws, and the properties only that kind of drawing has.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum NodeKind {
    /// A container. Draws its background and border and lays out its children.
    ///
    /// The direction is [`LayoutStyle::flex_direction`] rather than a separate
    /// kind, so the `Box`, `Row` and `Column` factories of `canvas.type.ts` are
    /// three defaults over one node rather than three nodes.
    Box,

    /// A paragraph, measured during layout and shaped during paint.
    ///
    /// The measured extent is absent on purpose: it depends on the space the
    /// solver offers, so it belongs to the layout result rather than to the
    /// scene.
    Text {
        /// The runs that make up the paragraph, in order.
        ///
        /// Plain text is one segment carrying an empty [`TextStyle`]; there is
        /// no separate unsegmented form, because two representations of one
        /// paragraph is one more than the renderer should have to handle.
        segments: Vec<TextSegment>,
        /// Properties of the paragraph as a whole.
        paragraph: ParagraphStyle,
    },

    /// A raster image fitted into its box.
    Image {
        /// Where the bytes come from.
        source: ImageSource,
        /// How the image fills the box.
        fit: ObjectFit,
        /// Where the image sits within the box when it does not fill it, as a
        /// fraction of the leftover space on each axis.
        position: (Length, Length),
        /// Which frame of an animated source to draw. `None` draws the first.
        frame: Option<u32>,
    },

    /// An arbitrary shape from SVG path data.
    Path {
        /// The `d` attribute of an SVG path, in the node's coordinate space —
        /// or in `view_box`'s space when one is given.
        data: String,
        /// The coordinate space `data` is written in, as SVG's `viewBox`:
        /// `(min_x, min_y, width, height)`.
        ///
        /// **`None` means the path draws in absolute local coordinates**,
        /// which is what every path did before this existed and what one still
        /// does without it. With a box, the path is scaled and centred into
        /// the node's resolved size under SVG's default
        /// `preserveAspectRatio` — `xMidYMid meet` — so it fits
        /// without distorting.
        ///
        /// **Equivalent to SVG's `viewBox` with
        /// `vector-effect: non-scaling-stroke`.** The drawing scales; the pen
        /// does not. In SVG a two-pixel stroke in a box scaled five times is
        /// drawn ten pixels wide, and ours stays two — deliberately, because a
        /// caller authoring a `d` in a unit square wants `line_width` to mean
        /// pixels and a chart's gridlines to stay hairlines whatever the box.
        /// Nothing here consumes SVG artwork, so nothing wants the other
        /// behaviour yet; `vector-effect` is the piece to add if something
        /// does. Asserted in `tests/path_view_box.rs` rather than left true by
        /// accident.
        ///
        /// **The node must have a size for this to mean anything.** A path
        /// node has no intrinsic size, so one with neither a width nor a
        /// height gets an empty box — and scaling a drawing into nothing draws
        /// nothing. Without a box that is harmless, since only the origin is
        /// used; with one it is the difference between a picture and a blank.
        ///
        /// It exists because a path in a percentage-sized box was otherwise
        /// undrawable: `d` is absolute, `Transform::scale_x` is an `f32`
        /// rather than a length, and a percentage-sized path node
        /// still draws `d` in absolute local coordinates. A chart's
        /// line, pie and doughnut all hit that, where its bars did not
        /// — a rectangle can be a percentage and a path cannot. No
        /// `preserve_aspect_ratio` field: the web's default is
        /// the only one anything here has needed, and a knob nobody uses is
        /// worse than none.
        view_box: Option<(f32, f32, f32, f32)>,
        /// Whether the drawing may be stretched to fill the node.
        ///
        /// SVG's `preserveAspectRatio`, and **only its `none` value**: `false`
        /// is the default `xMidYMid meet`, which fits the drawing without
        /// distorting it, and `true` is `none`, which scales each axis
        /// independently so the drawing fills the node exactly.
        ///
        /// A subset rather than a private spelling, so the other eight
        /// alignments stay addable without breaking a caller. `none` is here
        /// because a line chart needs it: a plot must fill its box, `meet`
        /// preserves aspect, and no viewBox fixes that — the box's aspect
        /// would have to match the node's, which is exactly what is unknown
        /// when the drawing is authored.
        ///
        /// It does **not** distort the pen, for the reason `view_box` gives.
        stretch: bool,
        /// How the interior is painted, if at all.
        fill: Option<PathPaint>,
        /// How the outline is painted, if at all.
        stroke: Option<PathPaint>,
        /// Stroke width in logical pixels.
        line_width: f32,
        /// Which side of the winding counts as inside.
        fill_rule: FillRule,
        /// How the stroke's ends are drawn.
        line_cap: LineCap,
        /// How the stroke's corners are drawn.
        line_join: LineJoin,
        /// Alternating dash and gap lengths. Empty draws a solid line.
        line_dash: Vec<f32>,
        /// How far into the dash pattern the stroke begins.
        line_dash_offset: f32,
    },
}

/// One entry in a scene's arena.
///
/// The four style groups are always present rather than optional. An absent
/// group would save bytes on the wire and cost a branch on every read in the
/// renderer, and the groups are cheap: the whole struct is plain data with one
/// `Vec` per variable-length field.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// What this node draws.
    pub kind: NodeKind,
    /// How it and its children are sized and placed.
    pub layout: LayoutStyle,
    /// How its box is filled and outlined.
    pub paint: PaintStyle,
    /// Glyph styling, which inherits to descendants.
    pub text: TextStyle,
    /// What is applied after it and its children are drawn.
    pub effects: Effects,
    /// Children, in paint order before `z_index` is applied.
    pub children: Vec<NodeId>,
    /// A name carried through for diagnostics, which the renderer never reads.
    pub name: Option<String>,
}

impl NodeKind {
    /// Which kind this is, as the number both representations write.
    #[must_use]
    pub const fn tag(&self) -> NodeTag {
        match self {
            Self::Box => NodeTag::Box,
            Self::Text { .. } => NodeTag::Text,
            Self::Image { .. } => NodeTag::Image,
            Self::Path { .. } => NodeTag::Path,
        }
    }
}

impl Node {
    /// Creates a node with default styling and no children.
    #[must_use]
    pub fn new(kind: NodeKind) -> Self {
        Self {
            kind,
            layout: LayoutStyle::default(),
            paint: PaintStyle::default(),
            text: TextStyle::default(),
            effects: Effects::default(),
            children: Vec::new(),
            name: None,
        }
    }

    /// Creates a plain container.
    #[must_use]
    pub fn container() -> Self {
        Self::new(NodeKind::Box)
    }

    /// Creates a single-run paragraph.
    #[must_use]
    pub fn text(content: impl Into<String>) -> Self {
        Self::new(NodeKind::Text {
            segments: vec![TextSegment {
                text: content.into(),
                style: TextStyle::default(),
            }],
            paragraph: ParagraphStyle::default(),
        })
    }

    /// Replaces this node's layout style.
    #[must_use]
    pub fn with_layout(mut self, layout: LayoutStyle) -> Self {
        self.layout = layout;
        self
    }

    /// Replaces this node's paint style.
    #[must_use]
    pub fn with_paint(mut self, paint: PaintStyle) -> Self {
        self.paint = paint;
        self
    }

    /// Replaces this node's inheritable text style.
    #[must_use]
    pub fn with_text_style(mut self, text: TextStyle) -> Self {
        self.text = text;
        self
    }

    /// Replaces this node's effects.
    #[must_use]
    pub fn with_effects(mut self, effects: Effects) -> Self {
        self.effects = effects;
        self
    }

    /// Attaches a diagnostic name.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{
        HttpOptions, ImageSource, LineCap, LineJoin, Node, NodeId, NodeKind,
        PathPaint,
    };
    use crate::style::{
        effect::Effects,
        layout::{Display, LayoutStyle},
        paint::{Color, PaintStyle},
        text::TextStyle,
    };

    #[test]
    fn node_id_is_a_transparent_index() {
        assert_eq!(NodeId::new(7).get(), 7);
        assert_eq!(NodeId::ROOT.get(), 0);
        assert_eq!(NodeId::default(), NodeId::ROOT);
        assert!(NodeId::new(1) > NodeId::ROOT);
    }

    #[test]
    fn a_new_node_carries_every_style_group_at_its_default() {
        let node = Node::container();
        assert_eq!(node.kind, NodeKind::Box);
        assert_eq!(node.layout, LayoutStyle::default());
        assert_eq!(node.paint, PaintStyle::default());
        assert!(node.children.is_empty());
        assert!(node.name.is_none());
    }

    #[test]
    fn text_helper_makes_one_segment() {
        let node = Node::text("hello");
        match &node.kind {
            NodeKind::Text {
                segments,
                paragraph,
            } => {
                assert_eq!(segments.len(), 1);
                assert_eq!(segments[0].text, "hello");
                assert!(paragraph.max_lines.is_none());
            }
            other => {
                unreachable!("Node::text builds a text node, found {other:?}")
            }
        }
    }

    #[test]
    fn builders_replace_the_group_they_name() {
        let layout = LayoutStyle {
            display: Display::Grid,
            ..LayoutStyle::default()
        };
        let paint = PaintStyle {
            background_color: Color::BLACK,
            ..PaintStyle::default()
        };
        let node = Node::container()
            .with_layout(layout.clone())
            .with_paint(paint.clone())
            .with_text_style(TextStyle::default())
            .with_effects(Effects::default())
            .named("root");
        assert_eq!(node.layout, layout);
        assert_eq!(node.paint, paint);
        assert_eq!(node.name.as_deref(), Some("root"));
    }

    #[test]
    fn image_sources_are_distinguished_by_kind_not_by_string() {
        assert_ne!(ImageSource::Path("a".to_owned()), ImageSource::url("a"));
        assert_eq!(LineCap::ALL.len(), 3);
        assert_eq!(LineJoin::ALL.len(), 3);
    }

    /// One URL under two credentials is two sources.
    ///
    /// `resolve` keys its decode cache by the whole [`ImageSource`], so this
    /// is what stops an authenticated fetch answering for an anonymous one --
    /// the equality that matters is the derived one, and it is derived only
    /// because the options are inside the variant.
    #[test]
    fn options_are_part_of_a_source_s_identity() {
        use std::collections::HashSet;

        let bare = ImageSource::url("https://a.test/x.png");
        let signed = ImageSource::url_with(
            "https://a.test/x.png",
            HttpOptions::new().header("authorization", "Bearer t"),
        );

        assert_ne!(bare, signed);
        let mut seen = HashSet::new();
        assert!(seen.insert(bare));
        assert!(seen.insert(signed));
    }

    /// The three normalisations, each pinned by a row that fails without it.
    ///
    /// Measured against the platform the other surface uses rather than
    /// asserted: `new Headers([["X-Zeta","1"],["Authorization","B"],
    /// ["x-zeta","2"]])` iterates as `[["authorization","B"],
    /// ["x-zeta","1, 2"]]`, so these are the answers the two surfaces have to
    /// agree on. The join is `", "` with the space.
    #[test]
    fn the_canonical_form_lower_cases_combines_and_sorts() {
        /// What the caller wrote, and what the wire carries for it.
        type Row = (
            &'static [(&'static str, &'static str)],
            &'static [(&'static str, &'static str)],
        );

        let rows: [Row; 5] = [
            // Sorting alone.
            (
                &[("x-zeta", "1"), ("accept", "2")],
                &[("accept", "2"), ("x-zeta", "1")],
            ),
            // Case alone.
            (&[("X-Zeta", "1")], &[("x-zeta", "1")]),
            // Combining alone, in the order the caller wrote them.
            (&[("a", "1"), ("a", "2")], &[("a", "1, 2")]),
            // All three, which is the fixture case's own shape.
            (
                &[
                    ("X-Zeta", "1"),
                    ("authorization", "Bearer probe"),
                    ("x-zeta", "2"),
                ],
                &[("authorization", "Bearer probe"), ("x-zeta", "1, 2")],
            ),
            // ASCII only. `str::to_lowercase` maps this to `i` plus a
            // combining dot -- two scalars, a name that sorts elsewhere, and
            // bytes the other surface cannot produce, since `Headers` folds
            // ASCII alone.
            (&[("\u{130}-name", "1")], &[("\u{130}-name", "1")]),
        ];

        for (written, want) in rows {
            let mut options = HttpOptions::new();
            for (name, value) in written {
                options = options.header(*name, *value);
            }
            let want: Vec<(String, String)> = want
                .iter()
                .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
                .collect();
            assert_eq!(options.canonical(), want, "written: {written:?}");
        }
    }

    /// A source's options over a scene's, one name at a time.
    #[test]
    fn merging_replaces_by_name_and_keeps_the_rest() {
        let scene = HttpOptions::new()
            .header("Authorization", "scene")
            .header("accept", "image/png");
        let source = HttpOptions::new().header("AUTHORIZATION", "source");

        // Case-insensitive because both sides are canonical first: the scene
        // wrote `Authorization` and the source `AUTHORIZATION`, and one header
        // comes out. `accept` survives, which is the half that says this is a
        // merge and not a replacement.
        assert_eq!(
            source.over(&scene).headers,
            vec![
                ("accept".to_owned(), "image/png".to_owned()),
                ("authorization".to_owned(), "source".to_owned()),
            ]
        );
        // Nothing in the source leaves the scene's set untouched, which is
        // what every source in an ordinary scene does.
        assert_eq!(HttpOptions::new().over(&scene).headers, scene.canonical());
    }

    /// A credential must not arrive in a log because something printed a node.
    #[test]
    fn debug_names_the_headers_and_prints_no_value() {
        let printed = format!(
            "{:?}",
            ImageSource::url_with(
                "https://a.test/x.png",
                HttpOptions::new().header("authorization", "Bearer sekrit"),
            )
        );

        assert!(
            printed.contains("authorization"),
            "the header name is gone, and it is the useful half: {printed}"
        );
        assert!(
            !printed.contains("sekrit"),
            "the header value was printed: {printed}"
        );
        // The URL is still printed: it is what a person reading the line is
        // looking for, and it is not the secret.
        assert!(printed.contains("https://a.test/x.png"), "{printed}");
    }
    /// A caller keeps a `NodeId` in a map and prints a node when a test fails,
    /// so the derives are exercised rather than assumed.
    #[test]
    fn the_derived_traits_work_on_every_node_shape() {
        use std::collections::HashSet;

        let mut seen = HashSet::new();
        assert!(seen.insert(NodeId::new(1)));
        assert!(!seen.insert(NodeId::new(1)));

        let node = Node::text("x");
        for rendered in [
            format!("{node:?}"),
            format!("{:?}", NodeId::ROOT),
            format!("{:?}", ImageSource::Bytes(vec![1])),
            format!("{:?}", LineCap::Butt),
            format!("{:?}", LineJoin::Bevel),
            format!("{:?}", PathPaint::Solid(Color::BLACK)),
        ] {
            assert!(!rendered.is_empty());
        }
        assert_eq!(node.clone(), node);
    }
}
