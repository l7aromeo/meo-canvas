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

/// Where an image finds its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageSource {
    /// A path on the local filesystem.
    Path(String),
    /// An absolute URL.
    ///
    /// `meo-canvas-core` does not fetch. A scene reaching the renderer with a
    /// URL still unresolved is an error there, not a network call; the surface
    /// that accepted the URL is the one that resolves it.
    Url(String),
    /// Bytes the caller already holds, in any container the renderer decodes.
    Bytes(Vec<u8>),
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
        /// The `d` attribute of an SVG path, in the node's coordinate space.
        data: String,
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
        ImageSource, LineCap, LineJoin, Node, NodeId, NodeKind, PathPaint,
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
        assert_ne!(
            ImageSource::Path("a".to_owned()),
            ImageSource::Url("a".to_owned())
        );
        assert_eq!(LineCap::ALL.len(), 3);
        assert_eq!(LineJoin::ALL.len(), 3);
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
