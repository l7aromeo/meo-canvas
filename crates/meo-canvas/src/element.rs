//! The nested tree a caller writes, and its flattening into the scene's arena.
//!
//! A scene is a `Vec<Node>` indexed by `NodeId` — a shape a codec round-trips
//! and a person does not write. An [`Element`] is the same tree nested, which
//! is how it is described:
//!
//! ```
//! use meo_canvas::{Column, Styled, Text, all, hex_rgb, px};
//!
//! let card = Column::new().padding(all(px(24.0))).gap(px(8.0)).children([
//!     Text::new("Ukasyah").font_size(24.0).bold(),
//!     Text::new("Bandung").color(hex_rgb(0x88_88_90)),
//! ]);
//!
//! let scene = card.into_scene(320.0, 180.0)?;
//! assert_eq!(scene.nodes.len(), 3);
//! # Ok::<(), meo_canvas_scene::SceneError>(())
//! ```
//!
//! Properties are named flat on the node, from [`Styled`]. Beyond them a node
//! has `new` (or the constructor taking its essential argument) and
//! `children`, and that set never grows: a new property is a new entry in the
//! property table, not a method on nine types.

use meo_canvas_scene::{
    Length, Scene, SceneError, Size,
    node::{ImageSource, LineCap, LineJoin, Node, NodeId, NodeKind, PathPaint},
    style::{
        effect::FillRule,
        paint::{Color, ObjectFit},
        text::{ParagraphStyle, TextSegment},
    },
};

use crate::{Style, Styled};

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
    /// A name carried through for diagnostics, which the renderer never reads.
    pub name: Option<String>,
}

impl Element {
    /// A node of the given kind, styled with nothing and holding no children.
    #[must_use]
    pub const fn new(kind: NodeKind) -> Self {
        Self {
            kind,
            style: Style::new(),
            children: Vec::new(),
            name: None,
        }
    }

    /// Layers a whole style over this node's, property by property.
    ///
    /// The secondary idiom. A node is normally styled by naming properties on
    /// it directly — `Row::new().gap(px(16.0))` — and this is for the case the
    /// flat setters cannot express: a reusable base, declared once as a
    /// `const`, applied to many nodes.
    ///
    /// ```
    /// use meo_canvas::{Row, Style, Styled, all, px};
    ///
    /// const CARD: Style = Style::new().padding(all(px(24.0))).gap(px(16.0));
    ///
    /// let tight = Row::new().with_style(CARD).gap(px(8.0));
    /// ```
    ///
    /// Not called `style`, so it cannot read as a nested style object —
    /// properties sit directly on the node on both surfaces.
    ///
    /// # It merges rather than replacing
    ///
    /// A property the argument names wins. A property it leaves absent leaves
    /// what the node already had, including what its constructor set:
    ///
    /// ```
    /// use meo_canvas::{Column, FlexDirection, Style, Styled, pct};
    ///
    /// // Still a column. The style said nothing about the direction.
    /// let kept = Column::new().with_style(Style::new().width(pct(100.0)));
    ///
    /// // A row, because this style did say. Merge is not a rule that the
    /// // constructor always wins — it is that absent is not a value.
    /// let row = Column::new()
    ///     .with_style(Style::new().flex_direction(FlexDirection::Row));
    /// ```
    ///
    /// [`Style::merge`] is what this calls.
    ///
    /// # Answering the reason replace was chosen
    ///
    /// This replaced rather than merged until the merge landed, and the reason
    /// recorded for it was real: merging makes the order of two `with_style`
    /// calls significant, and a chain does not announce that it does. Three
    /// things answer it.
    ///
    /// Order was already significant, and more sharply. A replace made
    /// `with_style` destroy every property set before it — the constructor's
    /// direction, any flat setter earlier in the chain — none of which the
    /// caller had said a word about. Under a merge the order matters only
    /// between two callers who both named the same property, where the later
    /// wins, as the flat setters and CSS's cascade already do.
    ///
    /// Every flat setter is already a one-field merge. `.gap(..).opacity(..)`
    /// sets two properties and keeps the rest. Merging is that same rule over
    /// many properties at once; replace was the one operation on this surface
    /// whose semantics differed from the setters sitting beside it.
    ///
    /// And the JavaScript surface has always merged. `Row` and `Column` there
    /// are `{ flexDirection, ...props }` and `Grid` is `{ display: 'grid',
    /// ...props }` — spread after the default, so a caller who names the
    /// property keeps their value and a caller who does not keeps the
    /// factory's. Its own doc says so deliberately. Replace made the two
    /// surfaces disagree about the same call, which this repository counts as
    /// a defect rather than a difference.
    #[must_use]
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = core::mem::take(&mut self.style).merge(style);
        self
    }

    /// A name carried through for diagnostics.
    ///
    /// Not a style property and not on [`crate::Styled`]: the scene
    /// keeps it on the node beside the kind rather than in a style group, and
    /// nothing inherits it. The renderer never reads it — it is there so a
    /// tree dumped to `.mcs` and looked at later says which node is which.
    ///
    /// ```
    /// use meo_canvas::Row;
    ///
    /// let card = Row::new().name("card");
    /// ```
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Replaces this node's children.
    ///
    /// Takes one element, an array, a `Vec`, an `Option` that contributes
    /// nothing when it is `None`, or [`each`] over anything iterable.
    #[must_use]
    pub fn children(mut self, children: impl IntoElements) -> Self {
        let mut collected = Vec::new();
        children.write_elements(&mut collected);
        self.children = collected;
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
        // **The runtime half of the promise the type already makes.** The
        // signature refuses `pct(50.0)` here, with a `compile_fail` doctest
        // saying why a percentage of nothing has no meaning; a `NaN` or a
        // negative width is the same claim and the type cannot see it.
        let size = Size::new(width, height);
        if !meo_canvas_scene::size_is_pixels(size) {
            return Err(SceneError::canvas_size(width, height));
        }

        let mut scene = Scene::new(size);
        let root = scene.root().ok_or(SceneError::NoPages)?;
        write_page(&mut scene, root, self)?;
        Ok(scene)
    }
}

impl Styled for Element {
    fn style_mut(&mut self) -> &mut Style {
        &mut self.style
    }
}

/// What a `children` call accepts: one element, many, or none.
///
/// v1's props type is `Children | Children[]` where `Children` includes
/// `false`, so a conditional that does not render writes nothing rather than an
/// empty node. Rust reaches the same place through a trait: a single element,
/// an array, a `Vec`, and an `Option<Element>` that is `None` and contributes
/// nothing. The syntax differs because the languages do; what a caller can
/// express does not.
///
/// ```
/// use meo_canvas::{Row, Styled, Text, px};
///
/// let show_subtitle = false;
/// let card = Row::new().gap(px(8.0)).children([
///     Some(Text::new("Ukasyah")),
///     show_subtitle.then(|| Text::new("subtitle")),
/// ]);
/// ```
pub trait IntoElements {
    /// Appends what this contributes onto `out`.
    ///
    /// Appends rather than returning a `Vec`, so nesting one of these inside
    /// another costs no intermediate allocation.
    fn write_elements(self, out: &mut Vec<Element>);
}

impl IntoElements for Element {
    fn write_elements(self, out: &mut Vec<Element>) {
        out.push(self);
    }
}

impl IntoElements for Option<Element> {
    fn write_elements(self, out: &mut Vec<Element>) {
        if let Some(inner) = self {
            out.push(inner);
        }
    }
}

impl<T: IntoElements> IntoElements for Vec<T> {
    fn write_elements(self, out: &mut Vec<Element>) {
        for item in self {
            item.write_elements(out);
        }
    }
}

impl<T: IntoElements, const N: usize> IntoElements for [T; N] {
    fn write_elements(self, out: &mut Vec<Element>) {
        for item in self {
            item.write_elements(out);
        }
    }
}

/// Children from anything iterable, without collecting first.
///
/// ```
/// use meo_canvas::{Column, Text, each};
///
/// let names = ["Ada", "Grace", "Katherine"];
/// let list =
///     Column::new().children(each(names.iter().map(|name| Text::new(*name))));
/// ```
///
/// A wrapper rather than a blanket `impl IntoElements for I: IntoIterator`,
/// which cannot exist: `Vec`, `[T; N]` and `Option` are all `IntoIterator`, so
/// it would overlap every impl above and rustc refuses it — `Option<Element>`
/// conflicts even when narrowed, because a future std release could make it an
/// iterator and coherence has to assume one might.
pub const fn each<I>(items: I) -> Each<I>
where
    I: IntoIterator,
    I::Item: IntoElements,
{
    Each(items)
}

/// What [`each`] returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Each<I>(I);

impl<I> IntoElements for Each<I>
where
    I: IntoIterator,
    I::Item: IntoElements,
{
    fn write_elements(self, out: &mut Vec<Element>) {
        for item in self.0 {
            item.write_elements(out);
        }
    }
}

/// Writes `element` onto an existing page root and adds its subtree.
///
/// The page root already exists, so the element styles it rather than becoming
/// a child of it -- otherwise every tree would gain an unstyled wrapper nobody
/// asked for, and a caller's `background_color` would paint a box inside the
/// canvas instead of the canvas.
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
        node.name = element.name;
    }

    for child in element.children {
        push(scene, root, child)?;
    }
    Ok(())
}

/// Writes the image properties a flat style carries onto an image node.
///
/// `object_fit`, `object_position` and `frame` live on [`Style`] because the
/// authoring surface is one flat style, and they live in [`NodeKind::Image`]
/// because that is where the scene keeps them. This is where the two meet. A
/// node that is not an image ignores them, as CSS ignores a property an element
/// does not define.
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
    node.name = element.name;

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
/// use meo_canvas::{Box, Styled, px};
///
/// let spacer = Box::new().size(px(8.0), px(8.0));
/// ```
///
/// # Importing this shadows `std::boxed::Box`
///
/// `Box<T>` is in the prelude, so a `use meo_canvas::Box` takes that name for
/// the rest of the file. Nothing warns.
///
/// **The fix is to spell the heap allocation in full, not to rename the
/// node.** `std::boxed::Box<dyn Error>` is unambiguous, it is one occurrence
/// in a file that mostly draws, and it leaves the component called what it is
/// called everywhere else -- in the JavaScript surface, in v9, and in every
/// example. Aliasing our own name to make room for the standard library's is
/// backwards: the qualification belongs on the thing that is not the subject
/// of the file.
///
/// Both names in one file, compiled here rather than asserted:
///
/// ```
/// use meo_canvas::{Box, Styled, px};
///
/// let spacer = Box::new().size(px(8.0), px(8.0));
/// let held: std::boxed::Box<dyn std::error::Error> = "in full".into();
/// # let _ = (spacer, held);
/// ```
///
/// A file that never heap-allocates needs none of this, which is most of them.
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
/// let bar = Row::new().with_style(Style::new().gap(px(12.0)));
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
        Element::new(NodeKind::Box)
            .flex_direction(meo_canvas_scene::style::layout::FlexDirection::Row)
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
        Element::new(NodeKind::Box).flex_direction(
            meo_canvas_scene::style::layout::FlexDirection::Column,
        )
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
        Element::new(NodeKind::Box)
            .display(meo_canvas_scene::style::layout::Display::Grid)
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
    /// The string is markup, not a literal: escape sequences and the five
    /// styling tags are resolved by [`meo_canvas_core::markup::parse`], which
    /// is the same parser the JavaScript surface's `Text()` has always run.
    /// A Rust caller gets rich text for the same string that gives a
    /// JavaScript caller rich text, which is the whole reason that parser
    /// is in Rust.
    ///
    /// ```
    /// use meo_canvas::{Style, Text};
    ///
    /// let name = Text::new("Ukasyah").with_style(Style::new().font_size(24.0));
    /// let mixed = Text::new("plain <b>bold</b>");
    /// ```
    ///
    /// Use [`Text::rich`] for content that must not be interpreted, or that
    /// carries styles the five tags cannot name.
    #[must_use]
    #[expect(
        clippy::new_ret_no_self,
        reason = "the node types are constructors for `Element`, not types a caller holds"
    )]
    pub fn new(content: impl Into<String>) -> Element {
        Element::new(NodeKind::Text {
            segments: meo_canvas_core::markup::parse_paragraph(&content.into()),
            paragraph: ParagraphStyle::default(),
        })
    }

    /// Text made of runs that differ in style, given directly.
    ///
    /// Each segment's own style overrides the node's for that run. Nothing here
    /// is parsed, which is what makes this the way to write content containing
    /// a literal `<` — [`Text::new`] would read it as markup — and the way to
    /// give a run a style the five markup tags cannot name.
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
    /// let avatar = Image::path("avatar.png")
    ///     .with_style(Style::new().size(px(64.0), px(64.0)));
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
///
/// # Importing this shadows `std::path::Path`
///
/// The same collision as [`Box`], and a likelier one: **both spell their
/// constructor `Path::new`**, so a file that draws a shape and opens a font
/// file has two `Path::new` calls meaning different things, and the compiler
/// reports the second as a type error rather than as a name clash.
///
/// The fix is the same and so is the reason: spell the standard library's in
/// full where you need it, and leave the component called `Path`.
///
/// ```
/// use meo_canvas::Path;
///
/// let arrow = Path::d("M0 0 L10 5 L0 10 Z");
/// let font = std::path::Path::new("/usr/share/fonts/Inter.ttf");
/// # let _ = (arrow, font);
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub struct Path;

/// Properties that belong to one node kind rather than to every node.
///
/// These sit on [`Element`] beside the flat style setters, but they are not
/// style: `max_lines` describes a paragraph and `stroke` describes a path, and
/// neither inherits or means anything on a node of another kind. The scene
/// holds them in the node's [`NodeKind`] for that reason, and these write
/// there.
///
/// **A setter for the wrong kind is ignored**, as CSS ignores a property an
/// element does not define. A `max_lines` on a `Box` is a caller saying
/// something about a paragraph that is not there, and the alternatives are
/// worse: a panic makes a typo fatal, and a `Result` puts error handling on
/// every line of a builder chain.
impl Element {
    /// How many lines a paragraph draws before it is truncated.
    ///
    /// ```
    /// use meo_canvas::Text;
    ///
    /// let excerpt = Text::new("a long paragraph").max_lines(2).ellipsis("…");
    /// ```
    #[must_use]
    pub const fn max_lines(mut self, lines: u32) -> Self {
        if let NodeKind::Text { paragraph, .. } = &mut self.kind {
            paragraph.max_lines = Some(lines);
        }
        self
    }

    /// What a truncated last line ends with.
    ///
    /// Only drawn when [`Element::max_lines`] truncates something. Unset
    /// truncates without a marker, and so does an empty one.
    ///
    /// [`DEFAULT_ELLIPSIS`](crate::scene::DEFAULT_ELLIPSIS) is the marker CSS
    /// uses, so a caller who wants the ordinary one need not know the code
    /// point. It is what the JavaScript surface's `ellipsis: true` resolves to.
    ///
    /// ```
    /// use meo_canvas::{Text, scene::DEFAULT_ELLIPSIS};
    ///
    /// let excerpt = Text::new("a long paragraph")
    ///     .max_lines(2)
    ///     .ellipsis(DEFAULT_ELLIPSIS);
    /// ```
    #[must_use]
    pub fn ellipsis(mut self, marker: impl Into<String>) -> Self {
        if let NodeKind::Text { paragraph, .. } = &mut self.kind {
            paragraph.ellipsis = Some(marker.into());
        }
        self
    }

    /// How a path's interior is painted. `None` leaves it unfilled.
    ///
    /// ```
    /// use meo_canvas::{Path, hex_rgb, scene::PathPaint};
    ///
    /// let tick = Path::d("M2 8 L6 12 L14 3")
    ///     .fill(None)
    ///     .stroke(Some(PathPaint::Solid(hex_rgb(0x22_cc_66))))
    ///     .line_width(2.0);
    /// ```
    #[must_use]
    pub fn fill(mut self, paint: Option<PathPaint>) -> Self {
        if let NodeKind::Path { fill, .. } = &mut self.kind {
            *fill = paint;
        }
        self
    }

    /// The coordinate space this path's `d` is written in, as SVG's
    /// `viewBox`: `(min_x, min_y, width, height)`.
    ///
    /// **The node must have a size**, since the drawing is scaled into it and
    /// a path node has no intrinsic one. Without a box the `d` is absolute, as
    /// it has always been.
    ///
    /// ```
    /// use meo_canvas::Path;
    ///
    /// // A unit square that fills whatever box it is given.
    /// let square =
    ///     Path::d("M0 0 H10 V10 H0 Z").view_box(Some((0.0, 0.0, 10.0, 10.0)));
    /// ```
    #[must_use]
    pub const fn view_box(
        mut self,
        view: Option<(f32, f32, f32, f32)>,
    ) -> Self {
        if let NodeKind::Path { view_box, .. } = &mut self.kind {
            *view_box = view;
        }
        self
    }

    /// Whether a path with a `view_box` may be stretched to fill its node.
    ///
    /// SVG's `preserveAspectRatio`, and only its `none` value: `false` is the
    /// default `xMidYMid meet`, which fits the drawing without distorting it,
    /// and `true` scales each axis independently so it fills the node exactly.
    ///
    /// **It does not distort the pen**, for the reason
    /// [`view_box`](Self::view_box) gives -- so a circle authored in a
    /// stretched box comes out an ellipse while its stroke stays even.
    ///
    /// ```
    /// use meo_canvas::Path;
    ///
    /// // A line plot fills its box rather than being letterboxed into it.
    /// let plot = Path::d("M 0 100 L 100 0")
    ///     .view_box(Some((0.0, 0.0, 100.0, 100.0)))
    ///     .stretch(true);
    /// ```
    #[must_use]
    pub const fn stretch(mut self, stretched: bool) -> Self {
        if let NodeKind::Path { stretch, .. } = &mut self.kind {
            *stretch = stretched;
        }
        self
    }

    /// How a path's outline is painted. `None` leaves it unstroked.
    #[must_use]
    pub fn stroke(mut self, paint: Option<PathPaint>) -> Self {
        if let NodeKind::Path { stroke, .. } = &mut self.kind {
            *stroke = paint;
        }
        self
    }

    /// How wide a path's stroke is drawn, in logical pixels.
    #[must_use]
    pub const fn line_width(mut self, width: f32) -> Self {
        if let NodeKind::Path { line_width, .. } = &mut self.kind {
            *line_width = width;
        }
        self
    }

    /// Which side of a path's winding counts as inside.
    #[must_use]
    pub const fn fill_rule(mut self, rule: FillRule) -> Self {
        if let NodeKind::Path { fill_rule, .. } = &mut self.kind {
            *fill_rule = rule;
        }
        self
    }

    /// How a path's stroke ends are drawn.
    #[must_use]
    pub const fn line_cap(mut self, cap: LineCap) -> Self {
        if let NodeKind::Path { line_cap, .. } = &mut self.kind {
            *line_cap = cap;
        }
        self
    }

    /// How a path's stroke corners are drawn.
    #[must_use]
    pub const fn line_join(mut self, join: LineJoin) -> Self {
        if let NodeKind::Path { line_join, .. } = &mut self.kind {
            *line_join = join;
        }
        self
    }

    /// Alternating dash and gap lengths. Empty draws a solid line.
    #[must_use]
    pub fn line_dash(mut self, pattern: impl IntoIterator<Item = f32>) -> Self {
        if let NodeKind::Path { line_dash, .. } = &mut self.kind {
            *line_dash = pattern.into_iter().collect();
        }
        self
    }

    /// How far into the dash pattern the stroke begins.
    #[must_use]
    pub const fn line_dash_offset(mut self, offset: f32) -> Self {
        if let NodeKind::Path {
            line_dash_offset, ..
        } = &mut self.kind
        {
            *line_dash_offset = offset;
        }
        self
    }
}

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
            // Absolute coordinates unless a caller asks otherwise, which is
            // what `d` alone has always meant.
            view_box: None,
            stretch: false,
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
        SceneError,
        node::{ImageSource, LineCap, LineJoin, NodeKind, PathPaint},
        style::{
            PaintOrder,
            effect::{FillRule, Mask, MaskShape},
            layout::{
                Align, BoxSizing, Direction, Display, FlexDirection, FlexWrap,
                GridAutoFlow, Justify, Overflow, PositionType,
            },
            paint::{
                BackgroundImage, BackgroundRepeat, BlendMode, BorderStyle,
                Color, Gradient, GradientGeometry, ObjectFit,
            },
            text::{
                FontStyle, LineHeight, ParagraphStyle, TextAlign,
                TextDecoration, TextStroke, VerticalAlign,
            },
        },
    };

    use super::{Box, Column, Element, Grid, Image, Path, Row, Text};
    use crate::{Style, Styled, hex_rgb, pct, px};

    #[test]
    fn a_paragraph_setter_writes_the_node_and_not_the_style() {
        let NodeKind::Text { paragraph, .. } =
            Text::new("x").max_lines(2).ellipsis("…").kind
        else {
            unreachable!("Text::new builds a text node");
        };
        assert_eq!(paragraph.max_lines, Some(2));
        assert_eq!(paragraph.ellipsis.as_deref(), Some("…"));
    }

    #[test]
    fn a_path_setter_writes_every_part_of_the_payload() {
        let NodeKind::Path {
            fill,
            stroke,
            line_width,
            fill_rule,
            line_cap,
            line_join,
            line_dash,
            line_dash_offset,
            ..
        } = Path::d("M0 0 L4 4")
            .fill(None)
            .stroke(Some(PathPaint::Solid(Color::rgba(5, 6, 7, 8))))
            .line_width(2.5)
            .fill_rule(FillRule::EvenOdd)
            .line_cap(LineCap::Round)
            .line_join(LineJoin::Bevel)
            .line_dash([1.0, 2.0])
            .line_dash_offset(0.5)
            .kind
        else {
            unreachable!("Path::d builds a path node");
        };
        assert_eq!(fill, None);
        assert_eq!(stroke, Some(PathPaint::Solid(Color::rgba(5, 6, 7, 8))));
        assert!((line_width - 2.5).abs() < f32::EPSILON);
        assert_eq!(fill_rule, FillRule::EvenOdd);
        assert_eq!(line_cap, LineCap::Round);
        assert_eq!(line_join, LineJoin::Bevel);
        assert_eq!(line_dash, vec![1.0, 2.0]);
        assert!((line_dash_offset - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn a_setter_for_the_wrong_kind_is_ignored() {
        // As CSS ignores a property an element does not define. The
        // alternatives are worse: a panic makes a typo fatal, and a `Result`
        // puts error handling on every line of a builder chain.
        let box_node = Box::new().max_lines(2).ellipsis("…").line_width(9.0);
        assert_eq!(box_node.kind, NodeKind::Box);

        // And a path setter on a text node leaves the paragraph alone.
        let NodeKind::Text { paragraph, .. } =
            Text::new("x").line_width(9.0).kind
        else {
            unreachable!("Text::new builds a text node");
        };
        assert_eq!(paragraph, ParagraphStyle::default());
    }

    #[test]
    fn text_new_reads_its_string_as_markup() {
        // The parser lives in `meo-canvas-core`, below every surface, so a
        // Rust caller gets a bold run where a literal `<b>` would otherwise
        // reach the glyphs.
        let NodeKind::Text { segments, .. } =
            Text::new("plain <b>bold</b>").kind
        else {
            unreachable!("Text::new builds a text node");
        };
        let texts: Vec<&str> =
            segments.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, vec!["plain ", "bold"]);
        assert!(segments[1].style.font_weight.is_some());
        assert!(segments[0].style.font_weight.is_none());
    }

    #[test]
    fn text_rich_reads_nothing_so_a_literal_angle_bracket_survives() {
        let NodeKind::Text { segments, .. } =
            Text::rich([("a <b> b".to_owned(), Style::new())]).kind
        else {
            unreachable!("Text::rich builds a text node");
        };
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "a <b> b");
    }

    #[test]
    fn text_that_says_nothing_is_still_a_paragraph() {
        // A string of nothing but tags parses to no runs. The node still has
        // to be a paragraph, so one empty run stands in for it.
        let NodeKind::Text { segments, .. } = Text::new("<b></b>").kind else {
            unreachable!("Text::new builds a text node");
        };
        assert_eq!(segments.len(), 1);
        assert!(segments[0].text.is_empty());
    }

    #[test]
    fn the_root_element_styles_the_page_rather_than_becoming_a_child_of_it() {
        // Otherwise every tree gains a wrapper nobody asked for, and a caller's
        // `background_color` paints a box inside the canvas instead of the
        // canvas.
        let scene = Box::new()
            .with_style(Style::new().background_color(hex_rgb(0x10_10_14)))
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
    fn a_canvas_size_that_is_not_a_length_is_refused() {
        // Until 5 September 2026 every one of these returned `Ok` with a scene
        // sized `NaN`, `-1` or `inf`, which passed `validate` and reached the
        // renderer -- so the picture was the first thing that reported the
        // caller's arithmetic. The signature already refuses `pct(50.0)` here,
        // with a `compile_fail` doctest saying a percentage of nothing has no
        // meaning; these are the same claim in numbers the type cannot see.
        for (width, height) in [
            (f32::NAN, 10.0),
            (10.0, f32::NAN),
            (-1.0, 10.0),
            (10.0, -1.0),
            (f32::INFINITY, 10.0),
            (10.0, f32::NEG_INFINITY),
        ] {
            let refused = Box::new().into_scene(width, height);
            assert!(
                matches!(refused, Err(SceneError::CanvasSize { .. })),
                "{width} by {height} was accepted"
            );
        }

        // **Zero is not in that list and must not be.** An empty canvas is a
        // canvas with nothing on it, which the encoders already answer for; a
        // rule that refused it would be a new refusal wearing a bug fix.
        assert!(Box::new().into_scene(0.0, 0.0).is_ok());
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
    fn a_style_merges_rather_than_replaces() {
        // What the argument names wins; what it leaves absent, the node keeps.
        // A replace discarded the first call's `gap` here, and discarded the
        // direction a constructor had just set, which is the defect this is.
        let element = Box::new()
            .with_style(Style::new().gap(px(4.0)))
            .with_style(Style::new().opacity(0.5));

        assert_eq!(element.style.gap, Some((px(4.0), px(4.0))));
        assert_eq!(element.style.opacity, Some(0.5));
    }

    #[test]
    fn a_constructor_property_the_style_does_not_name_survives_it() {
        // `Column::new()` is a box plus `flex-direction: column`, and a style
        // that says nothing about the direction must not turn it into a row.
        let column = Column::new().with_style(Style::new().width(pct(100.0)));

        assert_eq!(column.style.flex_direction, Some(FlexDirection::Column));
        assert!(column.style.width.is_some(), "the style still applied");
    }

    #[test]
    fn a_constructor_property_the_style_does_name_is_overridden() {
        // The control for the test above: merge is not "the constructor always
        // wins". A `Some` in the argument replaces what the node held, which is
        // what makes the merge a merge rather than a precedence rule.
        let row = Column::new()
            .with_style(Style::new().flex_direction(FlexDirection::Row));

        assert_eq!(row.style.flex_direction, Some(FlexDirection::Row));
    }

    #[test]
    fn a_none_in_the_style_leaves_what_the_node_already_had() {
        // Absent is not a value. A style that sets nothing changes nothing,
        // which is what lets `with_style` be applied to an already-styled node
        // without auditing all sixty-eight properties for what it will erase.
        let kept = Box::new().gap(px(12.0)).with_style(Style::new());

        assert_eq!(kept.style.gap, Some((px(12.0), px(12.0))));
    }

    #[test]
    fn merge_carries_every_property_it_is_given() {
        // The destructure in `merge` is exhaustive, so a new field cannot be
        // *missing* from it. It can still be dismissed: rustc's own suggested
        // fix for the resulting E0027 is `new_field: _`, which compiles and
        // silently drops that property from every merge. Nothing above catches
        // that, because the field is named and the code is wrong.
        //
        // This does. Every property is `Some`; merging over an empty style has
        // to return all of them. A field dismissed with `_` comes back `None`
        // and fails the equality, and the literal below has no `..`, so a
        // sixty-ninth property fails to compile here rather than going
        // unmerged and untested.
        //
        // The literal **is** the mechanism, and it has no `..` deliberately:
        // a sixty-ninth property fails to compile here, and one dismissed with
        // `_` in `merge` fails the equality below. Replacing it with
        // `..Default::default()` would compile and assert nothing new.
        //
        // `<_>::default()` for a value wherever the type offers one: what each
        // property *means* is not the question, only whether it carried.
        //
        // **This does not test that `merge` merges.** A pure replace returns
        // the full style here too, since it is merged over an empty one — the
        // measurement: with `merge` reduced to `self = other`, this passes and
        // the three direction tests above fail. The two sets are orthogonal
        // and both load-bearing. They ask *carried or dropped* and *merged or
        // replaced*; neither answers the other, and the name of this one reads
        // broader than it is.
        let full = Style {
            display: Some(Display::Flex),
            position_type: Some(PositionType::Absolute),
            position: Some(<_>::default()),
            width: Some(<_>::default()),
            height: Some(<_>::default()),
            min_width: Some(<_>::default()),
            min_height: Some(<_>::default()),
            max_width: Some(<_>::default()),
            max_height: Some(<_>::default()),
            aspect_ratio: Some(<_>::default()),
            margin: Some(<_>::default()),
            padding: Some(<_>::default()),
            border: Some(<_>::default()),
            flex_direction: Some(FlexDirection::Row),
            flex_wrap: Some(FlexWrap::NoWrap),
            flex_grow: Some(<_>::default()),
            flex_shrink: Some(<_>::default()),
            flex_basis: Some(<_>::default()),
            justify_content: Some(Justify::FlexStart),
            align_items: Some(Align::FlexStart),
            align_self: Some(Align::FlexStart),
            align_content: Some(Align::FlexStart),
            gap: Some(<_>::default()),
            overflow: Some((Overflow::Visible, Overflow::Visible)),
            box_sizing: Some(BoxSizing::BorderBox),
            direction: Some(Direction::Ltr),
            grid_template_columns: Some(<_>::default()),
            grid_template_rows: Some(<_>::default()),
            grid_auto_rows: Some(<_>::default()),
            grid_auto_columns: Some(<_>::default()),
            grid_auto_flow: Some(GridAutoFlow::Row),
            grid_column: Some(<_>::default()),
            grid_row: Some(<_>::default()),
            background_color: Some(<_>::default()),
            gradient: Some(Gradient {
                geometry: GradientGeometry::Linear {
                    direction: <_>::default(),
                },
                stops: Vec::new(),
            }),
            background_image: Some(BackgroundImage {
                source: ImageSource::Path("a.png".to_owned()),
                repeat: BackgroundRepeat::Repeat,
                size: <_>::default(),
                position: <_>::default(),
            }),
            border_color: Some(<_>::default()),
            border_color_all: Some(<_>::default()),
            border_style: Some(BorderStyle::Solid),
            border_radius: Some(<_>::default()),
            opacity: Some(<_>::default()),
            mix_blend_mode: Some(BlendMode::Normal),
            dither: Some(<_>::default()),
            z_index: Some(<_>::default()),
            object_fit: Some(ObjectFit::Fill),
            object_position: Some(<_>::default()),
            frame: Some(<_>::default()),
            font_family: Some(<_>::default()),
            font_size: Some(<_>::default()),
            font_weight: Some(<_>::default()),
            font_style: Some(FontStyle::Normal),
            color: Some(<_>::default()),
            text_align: Some(TextAlign::Start),
            text_decoration: Some(TextDecoration::None),
            vertical_align: Some(VerticalAlign::Top),
            paint_order: Some(PaintOrder::Fill),
            line_height: Some(LineHeight::Number(1.5)),
            line_gap: Some(<_>::default()),
            font_variant: Some(<_>::default()),
            letter_spacing: Some(<_>::default()),
            word_spacing: Some(<_>::default()),
            text_stroke: Some(TextStroke {
                width: 1.0,
                color: <_>::default(),
            }),
            transform: Some(<_>::default()),
            box_shadows: Some(<_>::default()),
            text_shadows: Some(<_>::default()),
            mask: Some(Mask::Shape(MaskShape::Circle)),
            filter: Some(<_>::default()),
            backdrop_filter: Some(<_>::default()),
        };

        assert_eq!(Style::new().merge(full.clone()), full);

        // And through the node surface, which is where a caller meets it.
        assert_eq!(Box::new().with_style(full.clone()).style, full);
    }

    #[test]
    fn children_takes_anything_iterable() {
        let from_map =
            Row::new().children(super::each((0..3).map(|_| Text::new("x"))));
        assert_eq!(from_map.children.len(), 3);

        let from_vec = Row::new().children(vec![Text::new("x")]);
        assert_eq!(from_vec.children.len(), 1);
    }

    #[test]
    fn the_image_properties_reach_the_image_node_and_nothing_else() {
        let styled = Style::new().object_fit(ObjectFit::Cover).frame(3);

        let image = Image::path("a.png").with_style(styled.clone());
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
        let boxed = Box::new().with_style(styled);
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
