//! The nested tree a caller writes, and its flattening into the scene's arena.
//!
//! A scene is a `Vec<Node>` indexed by `NodeId` — a shape a codec round-trips
//! and a person does not write. An [`Element`] is the same tree nested, which
//! is how it is described:
//!
//! ```
//! use meo_canvas::{Column, Style, Text, all, hex_rgb, px};
//!
//! let card = Column::new()
//!     .style(Style::new().padding(all(px(24.0))).gap(px(8.0)))
//!     .children([
//!         Text::new("Ukasyah").style(Style::new().font_size(24.0).bold()),
//!         Text::new("Bandung").style(Style::new().color(hex_rgb(0x88_88_90))),
//!     ]);
//!
//! let scene = card.into_scene(320.0, 180.0)?;
//! assert_eq!(scene.nodes.len(), 3);
//! # Ok::<(), meo_canvas_scene::SceneError>(())
//! ```
//!
//! Three methods per node and the set never grows: `new` (or the constructor
//! that takes the node's essential argument), `style`, and `children`. A new
//! property is a new method on [`Style`], not a tenth method on nine types.

use meo_canvas_scene::{
    Length, Scene, SceneError, Size,
    node::{ImageSource, LineCap, LineJoin, Node, NodeId, NodeKind, PathPaint},
    style::{
        effect::FillRule,
        paint::{Color, ObjectFit},
        text::{ParagraphStyle, TextSegment, TextStyle},
    },
};

use crate::Style;

/// One node of the tree, with its style and its children.
///
/// Produced by the node constructors rather than written directly, though the
/// fields are reachable for a caller assembling a tree programmatically.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    /// What this node draws.
    pub kind: NodeKind,
    /// How it is styled.
    pub style: Style,
    /// Its children, in paint order before `z_index` applies.
    pub children: Vec<Self>,
}

impl Element {
    /// A node of the given kind, styled with nothing and holding no children.
    #[must_use]
    pub const fn new(kind: NodeKind) -> Self {
        Self {
            kind,
            style: Style::new(),
            children: Vec::new(),
        }
    }

    /// Replaces this node's style.
    ///
    /// Replaces rather than merges, because a caller who wants to layer styles
    /// composes them before calling this — and a merge would make the order of
    /// two `.style()` calls significant in a way the chain does not suggest.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Replaces this node's children.
    ///
    /// Takes anything iterable, so an array literal, a `Vec` and a `map` over
    /// data all work without the caller collecting first.
    #[must_use]
    pub fn children(
        mut self,
        children: impl IntoIterator<Item = Self>,
    ) -> Self {
        self.children = children.into_iter().collect();
        self
    }

    /// Flattens this tree into a scene of one page at the given size.
    ///
    /// This element becomes the page root, so its style is the page's.
    ///
    /// What [`Canvas`](crate::Canvas) uses for a single page, and public for
    /// the case where a caller wants a `Scene` and nothing else — to write
    /// to disk, to send over the wire, or to render more than once. A
    /// caller who wants pages, or options, reaches for `Canvas` instead.
    ///
    /// The size is bare pixels rather than a [`Length`], and deliberately: a
    /// canvas size is in device-independent pixels, and a percentage of nothing
    /// has no meaning. A `Length` parameter would admit `pct(50.0)`, compile,
    /// and produce a zero-width canvas with nothing said — a caller could only
    /// discover it by looking at the picture. The type refuses it instead:
    ///
    /// ```compile_fail
    /// use meo_canvas::{Box, pct, px};
    ///
    /// // `pct` yields a `Length`, which is not a canvas size.
    /// let scene = Box::new().into_scene(pct(50.0), 112.0);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`SceneError::TooManyNodes`] when the tree holds more nodes than
    /// the codec can address. Nothing else here can fail: the tree is a tree by
    /// construction, so the cycle and reachability rules
    /// [`Scene::validate`] enforces cannot be broken by building one this way.
    pub fn into_scene(
        self,
        width: f32,
        height: f32,
    ) -> Result<Scene, SceneError> {
        let mut scene = Scene::new(Size::new(width, height));
        let root = scene.root().ok_or(SceneError::NoPages)?;
        write_page(&mut scene, root, self)?;
        Ok(scene)
    }
}

/// Writes `element` onto an existing page root and adds its subtree.
///
/// The page root already exists, so the element styles it rather than becoming
/// a child of it -- otherwise every tree would gain an unstyled wrapper nobody
/// asked for, and a caller's `background` would paint a box inside the canvas
/// instead of the canvas.
///
/// Shared by [`Element::into_scene`] and [`crate::Canvas`], so a page means the
/// same thing whether one was built or several.
pub(crate) fn write_page(
    scene: &mut Scene,
    root: NodeId,
    element: Element,
) -> Result<(), SceneError> {
    let mut kind = element.kind;
    apply_image_style(&mut kind, &element.style);
    let (layout, paint, text, effects) = element.style.into_parts();
    if let Some(node) = scene.get_mut(root) {
        node.kind = kind;
        node.layout = layout;
        node.paint = paint;
        node.text = text;
        node.effects = effects;
    }

    for child in element.children {
        push(scene, root, child)?;
    }
    Ok(())
}

/// Writes the image properties a flat style carries onto an image node.
///
/// `fit`, `object_position` and `frame` live on [`Style`] because the authoring
/// surface is one flat style, and they live in [`NodeKind::Image`] because that
/// is where the scene keeps them. This is where the two meet. A node that is
/// not an image ignores them, as CSS ignores a property an element does not
/// define.
const fn apply_image_style(kind: &mut NodeKind, style: &Style) {
    if let NodeKind::Image {
        fit,
        position,
        frame,
        ..
    } = kind
    {
        if let Some(value) = style.object_fit {
            *fit = value;
        }
        if let Some(value) = style.object_position {
            *position = value;
        }
        if style.frame.is_some() {
            *frame = style.frame;
        }
    }
}

/// Adds `element` and its subtree under `parent`.
///
/// Depth-first and iterative in shape only where it matters: the recursion
/// mirrors the tree a caller wrote, and a tree deep enough to overflow the
/// stack here is one deep enough to overflow it in the codec too.
fn push(
    scene: &mut Scene,
    parent: NodeId,
    element: Element,
) -> Result<NodeId, SceneError> {
    let mut kind = element.kind;
    apply_image_style(&mut kind, &element.style);
    let (layout, paint, text, effects) = element.style.into_parts();
    let mut node = Node::new(kind);
    node.layout = layout;
    node.paint = paint;
    node.text = text;
    node.effects = effects;

    let id = scene.push(parent, node)?;
    for child in element.children {
        push(scene, id, child)?;
    }
    Ok(id)
}

/// A plain container.
///
/// Lays its children out as a row by default, following CSS's `display: flex`
/// rather than Yoga's column.
///
/// ```
/// use meo_canvas::{Box, Style, px};
///
/// let spacer = Box::new().style(Style::new().size(px(8.0), px(8.0)));
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub struct Box;

impl Box {
    /// A container with nothing set.
    #[must_use]
    #[expect(
        clippy::new_ret_no_self,
        reason = "the node types are constructors for `Element`, not types a caller holds"
    )]
    pub const fn new() -> Element {
        Element::new(NodeKind::Box)
    }
}

/// A container whose children run horizontally.
///
/// ```
/// use meo_canvas::{Row, Style, px};
///
/// let bar = Row::new().style(Style::new().gap(px(12.0)));
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub struct Row;

impl Row {
    /// A row, with `flex-direction: row` set explicitly.
    ///
    /// Explicit rather than inherited from the default, so the direction
    /// survives a caller layering a style that names one.
    #[must_use]
    #[expect(
        clippy::new_ret_no_self,
        reason = "the node types are constructors for `Element`, not types a caller holds"
    )]
    pub fn new() -> Element {
        Element::new(NodeKind::Box).style(Style::new().flex_direction(
            meo_canvas_scene::style::layout::FlexDirection::Row,
        ))
    }
}

/// A container whose children run vertically.
#[derive(Debug)]
#[non_exhaustive]
pub struct Column;

impl Column {
    /// A column, with `flex-direction: column` set explicitly.
    #[must_use]
    #[expect(
        clippy::new_ret_no_self,
        reason = "the node types are constructors for `Element`, not types a caller holds"
    )]
    pub fn new() -> Element {
        Element::new(NodeKind::Box).style(Style::new().flex_direction(
            meo_canvas_scene::style::layout::FlexDirection::Column,
        ))
    }
}

/// A container whose children are placed on a grid.
#[derive(Debug)]
#[non_exhaustive]
pub struct Grid;

impl Grid {
    /// A grid, with `display: grid` set explicitly.
    #[must_use]
    #[expect(
        clippy::new_ret_no_self,
        reason = "the node types are constructors for `Element`, not types a caller holds"
    )]
    pub fn new() -> Element {
        Element::new(NodeKind::Box).style(
            Style::new()
                .display(meo_canvas_scene::style::layout::Display::Grid),
        )
    }
}

/// A run of text.
#[derive(Debug)]
#[non_exhaustive]
pub struct Text;

impl Text {
    /// Text with the given content.
    ///
    /// The content is a constructor argument rather than a setter, so it cannot
    /// be forgotten — a `Text` with no text is not a thing worth being able to
    /// write.
    ///
    /// ```
    /// use meo_canvas::{Style, Text};
    ///
    /// let name = Text::new("Ukasyah").style(Style::new().font_size(24.0));
    /// ```
    #[must_use]
    #[expect(
        clippy::new_ret_no_self,
        reason = "the node types are constructors for `Element`, not types a caller holds"
    )]
    pub fn new(content: impl Into<String>) -> Element {
        Element::new(NodeKind::Text {
            segments: vec![TextSegment {
                text: content.into(),
                style: TextStyle::default(),
            }],
            paragraph: ParagraphStyle::default(),
        })
    }

    /// Text made of runs that differ in style.
    ///
    /// The one case a single string cannot express: a sentence with one word
    /// bold. Each segment's own style overrides the node's for that run.
    #[must_use]
    pub fn rich(
        segments: impl IntoIterator<Item = (String, Style)>,
    ) -> Element {
        let segments = segments
            .into_iter()
            .map(|(text, style)| {
                let (_, _, text_style, _) = style.into_parts();
                TextSegment {
                    text,
                    style: text_style,
                }
            })
            .collect();
        Element::new(NodeKind::Text {
            segments,
            paragraph: ParagraphStyle::default(),
        })
    }
}

/// A raster image.
#[derive(Debug)]
#[non_exhaustive]
pub struct Image;

impl Image {
    /// An image read from a local path.
    ///
    /// ```
    /// use meo_canvas::{Image, Style, px};
    ///
    /// let avatar =
    ///     Image::path("avatar.png").style(Style::new().size(px(64.0), px(64.0)));
    /// ```
    #[must_use]
    pub fn path(path: impl Into<String>) -> Element {
        Self::source(ImageSource::Path(path.into()))
    }

    /// An image from bytes the caller already holds.
    #[must_use]
    pub fn bytes(bytes: impl Into<Vec<u8>>) -> Element {
        Self::source(ImageSource::Bytes(bytes.into()))
    }

    /// An image from a URL.
    ///
    /// The renderer does not fetch. A scene carrying one of these reaches
    /// `meo-canvas-core` as an error; the surface that accepted the URL is the
    /// one that resolves it, which for the command-line renderer is its `net`
    /// feature.
    #[must_use]
    pub fn url(url: impl Into<String>) -> Element {
        Self::source(ImageSource::Url(url.into()))
    }

    /// An image from a source already in hand.
    #[must_use]
    pub const fn source(source: ImageSource) -> Element {
        Element::new(NodeKind::Image {
            source,
            fit: ObjectFit::Fill,
            position: (Length::Percent(0.5), Length::Percent(0.5)),
            frame: None,
        })
    }
}

/// An arbitrary shape from SVG path data.
#[derive(Debug)]
#[non_exhaustive]
pub struct Path;

impl Path {
    /// A path from an SVG `d` attribute.
    ///
    /// ```
    /// use meo_canvas::Path;
    ///
    /// let tick = Path::d("M2 8 L6 12 L14 3");
    /// ```
    #[must_use]
    pub fn d(data: impl Into<String>) -> Element {
        Element::new(NodeKind::Path {
            data: data.into(),
            fill: Some(PathPaint::Solid(Color::BLACK)),
            stroke: None,
            line_width: 1.0,
            fill_rule: FillRule::NonZero,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            line_dash: Vec::new(),
            line_dash_offset: 0.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use meo_canvas_scene::{
        node::{ImageSource, NodeKind},
        style::{
            layout::{Display, FlexDirection},
            paint::ObjectFit,
        },
    };

    use super::{Box, Column, Element, Grid, Image, Path, Row, Text};
    use crate::{Style, hex_rgb, px};

    #[test]
    fn the_root_element_styles_the_page_rather_than_becoming_a_child_of_it() {
        // Otherwise every tree gains a wrapper nobody asked for, and a caller's
        // `background` paints a box inside the canvas instead of the canvas.
        let scene = Box::new()
            .style(Style::new().background(hex_rgb(0x10_10_14)))
            .into_scene(100.0, 50.0)
            .unwrap_or_else(|error| unreachable!("{error}"));

        assert_eq!(scene.nodes.len(), 1);
        assert_eq!(scene.size.width.to_bits(), 100.0_f32.to_bits());
        assert_eq!(scene.size.height.to_bits(), 50.0_f32.to_bits());
        let root = scene
            .get(scene.pages[0])
            .unwrap_or_else(|| unreachable!("the page has a root"));
        assert_eq!(root.paint.background_color, hex_rgb(0x10_10_14));
    }

    #[test]
    fn the_nested_tree_flattens_into_the_arena_in_order() {
        let scene = Column::new()
            .children([
                Text::new("one"),
                Row::new().children([Text::new("two"), Text::new("three")]),
            ])
            .into_scene(10.0, 10.0)
            .unwrap_or_else(|error| unreachable!("{error}"));

        // Root, "one", the row, "two", "three".
        assert_eq!(scene.nodes.len(), 5);
        assert!(scene.validate().is_ok(), "a built tree is a valid scene");
    }

    #[test]
    fn the_container_constructors_set_the_direction_they_name() {
        let (row, ..) = Row::new().style.into_parts();
        assert_eq!(row.flex_direction, FlexDirection::Row);

        let (column, ..) = Column::new().style.into_parts();
        assert_eq!(column.flex_direction, FlexDirection::Column);

        let (grid, ..) = Grid::new().style.into_parts();
        assert_eq!(grid.display, Display::Grid);
    }

    #[test]
    fn a_style_replaces_rather_than_merges() {
        // A merge would make the order of two `.style()` calls significant in a
        // way the chain does not suggest.
        let element = Box::new()
            .style(Style::new().gap(px(4.0)))
            .style(Style::new().opacity(0.5));

        assert!(element.style.gap.is_none());
        assert_eq!(element.style.opacity, Some(0.5));
    }

    #[test]
    fn children_takes_anything_iterable() {
        let from_map = Row::new().children((0..3).map(|_| Text::new("x")));
        assert_eq!(from_map.children.len(), 3);

        let from_vec = Row::new().children(vec![Text::new("x")]);
        assert_eq!(from_vec.children.len(), 1);
    }

    #[test]
    fn the_image_properties_reach_the_image_node_and_nothing_else() {
        let styled = Style::new().fit(ObjectFit::Cover).frame(3);

        let image = Image::path("a.png").style(styled.clone());
        let scene = image
            .into_scene(10.0, 10.0)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let root = scene
            .get(scene.pages[0])
            .unwrap_or_else(|| unreachable!("the page has a root"));

        match &root.kind {
            NodeKind::Image { fit, frame, .. } => {
                assert_eq!(*fit, ObjectFit::Cover);
                assert_eq!(*frame, Some(3));
            }
            other => unreachable!("expected an image, found {other:?}"),
        }

        // A node that is not an image ignores them, as CSS ignores a property
        // an element does not define.
        let boxed = Box::new().style(styled);
        let scene = boxed
            .into_scene(10.0, 10.0)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let root = scene
            .get(scene.pages[0])
            .unwrap_or_else(|| unreachable!("the page has a root"));
        assert_eq!(root.kind, NodeKind::Box);
    }

    #[test]
    fn each_image_source_is_carried_as_the_kind_it_is() {
        let cases = [
            (Image::path("a.png"), ImageSource::Path("a.png".to_owned())),
            (
                Image::url("https://example.invalid/a.png"),
                ImageSource::Url("https://example.invalid/a.png".to_owned()),
            ),
            (Image::bytes(vec![1, 2]), ImageSource::Bytes(vec![1, 2])),
        ];

        for (element, expected) in cases {
            match element.kind {
                NodeKind::Image { source, .. } => assert_eq!(source, expected),
                other => unreachable!("expected an image, found {other:?}"),
            }
        }
    }

    #[test]
    fn text_carries_its_content_and_rich_text_carries_a_run_per_segment() {
        match Text::new("hello").kind {
            NodeKind::Text { segments, .. } => {
                assert_eq!(segments.len(), 1);
                assert_eq!(segments[0].text, "hello");
            }
            other => unreachable!("expected text, found {other:?}"),
        }

        let rich = Text::rich([
            ("plain ".to_owned(), Style::new()),
            ("bold".to_owned(), Style::new().bold()),
        ]);
        match rich.kind {
            NodeKind::Text { segments, .. } => {
                assert_eq!(segments.len(), 2);
                assert!(segments[0].style.font_weight.is_none());
                assert!(segments[1].style.font_weight.is_some());
            }
            other => unreachable!("expected text, found {other:?}"),
        }
    }

    #[test]
    fn a_path_carries_its_data() {
        match Path::d("M0 0 L1 1").kind {
            NodeKind::Path { data, .. } => assert_eq!(data, "M0 0 L1 1"),
            other => unreachable!("expected a path, found {other:?}"),
        }
    }

    #[test]
    fn the_canvas_takes_the_size_it_was_given() {
        // A canvas size is device-independent pixels. A percentage of nothing
        // has no meaning, and the parameter's type is what refuses one -- the
        // `compile_fail` example on `into_scene` is the check that it does.
        let scene = Box::new()
            .into_scene(120.0, 40.0)
            .unwrap_or_else(|error| unreachable!("{error}"));

        assert_eq!(scene.size.width.to_bits(), 120.0_f32.to_bits());
        assert_eq!(scene.size.height.to_bits(), 40.0_f32.to_bits());
    }

    #[test]
    fn an_element_can_be_built_from_a_kind_directly() {
        let element = Element::new(NodeKind::Box);
        assert_eq!(element.children.len(), 0);
        assert_eq!(element.style, Style::new());
    }
}
