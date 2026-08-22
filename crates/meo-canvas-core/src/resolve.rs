//! Turns a scene's external references into things the later passes can use.
//!
//! Three jobs, all of which have to finish before taffy is asked anything.
//! Fonts are registered, so a family a node names can be found. Images are read
//! and decoded, because an image sized `Auto` on both axes takes its extent
//! from the decoded bitmap and that extent is a layout input. Text styles are
//! folded down the tree, so a text node carries the family its container set
//! rather than a chain of ancestors to walk at measure time.
//!
//! This pass reads local files and accepts bytes the caller already holds. **It
//! does not fetch.** An [`ImageSource::Url`] arriving here is
//! [`Error::UnresolvedSource`] -- resolving it needs an HTTP client, an HTTP
//! client needs a policy about runtimes, and that policy belongs to the surface
//! talking to the user rather than to a library every surface links.
//!
//! # One resolve per scene, not per page
//!
//! The caches here are keyed by [`NodeId`] alone, with no page beside it. That
//! is the whole reason [`Scene`] holds one arena for every page: two pages that
//! draw the same file decode it once, and the layout pass that runs per page
//! reads a table that was built once.
//!
//! # No global state
//!
//! Nothing here is a `static`. [`Fonts`] owns its registry and a caller holds
//! it, so two renders on two threads share nothing and contend for nothing.
//! (`meo-skia-canvas` keeps a process-wide registry of its own behind
//! `FontLibrary`, which this crate cannot opt out of; what it can do, and does,
//! is add no second one.)

use std::{collections::HashMap, path::Path, sync::OnceLock};

use meo_canvas_scene::{
    Scene, Size,
    node::{ImageSource, NodeId, NodeKind},
    style::{
        PaintOrder,
        paint::Color,
        text::{
            FontStyle, FontVariant, FontWeight, Spacing, TextAlign,
            TextDecoration, TextStyle, VerticalAlign,
        },
    },
};

use crate::Error;

/// The fonts a render can draw with.
///
/// Wraps `meo-skia-canvas`'s registry rather than re-exporting it, because no
/// signature in this crate names a Skia type. Held by the caller and passed in,
/// so a server registers its faces once and every render after that is a
/// borrow.
///
/// `Debug` is written out rather than derived: the underlying registry does not
/// implement it, and what a reader of a debug dump wants from a font registry
/// is which families it holds, not the state of a Skia provider.
#[derive(Default)]
pub struct Fonts {
    library: meo_skia_canvas::FontLibrary,
    /// The families the platform already has, read once.
    ///
    /// Enumerating installed families walks the system's font directories, and
    /// the answer cannot change while the process runs. Reading it per scene
    /// would repeat that walk for every render.
    installed: OnceLock<Vec<String>>,
}

impl core::fmt::Debug for Fonts {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Fonts")
            .field("registered", &self.registered())
            .finish_non_exhaustive()
    }
}

impl Fonts {
    /// A registry holding no faces of its own.
    ///
    /// The platform's own fonts are still reachable: they were never registered
    /// here, and are found through the system's manager.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a face from a file, under a family name of the caller's
    /// choosing.
    ///
    /// The name need not match the one inside the file. Call it more than once
    /// with the same name to give a family several weights.
    ///
    /// # Errors
    ///
    /// Returns [`Error::FontRegister`] if the file cannot be read or its bytes
    /// are not a font this build can parse.
    pub fn register_path(
        &self,
        family: &str,
        path: impl AsRef<Path>,
    ) -> Result<(), Error> {
        self.library
            .register_font_from_path(family, path)
            .map_err(|source| Error::FontRegister {
                family: family.to_owned(),
                detail: source.to_string(),
            })
    }

    /// Registers a face from bytes the caller already holds.
    ///
    /// # Errors
    ///
    /// Returns [`Error::FontRegister`] if the bytes are not a font this build
    /// can parse.
    pub fn register_bytes(
        &self,
        family: &str,
        bytes: &[u8],
    ) -> Result<(), Error> {
        self.library
            .register_font_from_data(family, bytes)
            .map_err(|source| Error::FontRegister {
                family: family.to_owned(),
                detail: source.to_string(),
            })
    }

    /// Whether a family can be drawn with, whether registered here or installed
    /// on the platform.
    #[must_use]
    pub fn has(&self, family: &str) -> bool {
        if family.is_empty() || self.library.has_font(family) {
            return true;
        }
        self.installed()
            .iter()
            .any(|installed| installed.eq_ignore_ascii_case(family))
    }

    /// The families registered here, in registration order.
    #[must_use]
    pub fn registered(&self) -> Vec<String> {
        self.library.families()
    }

    /// The registry the measure pass builds its text engine from.
    pub(crate) const fn library(&self) -> &meo_skia_canvas::FontLibrary {
        &self.library
    }

    fn installed(&self) -> &[String] {
        self.installed
            .get_or_init(|| self.library.installed_families())
    }
}

/// A text style with every field decided.
///
/// [`TextStyle`] is all `Option` because it inherits; this is what inheriting
/// produced. Resolving once here rather than at measure time matters because
/// measure runs many times per leaf during one solve and inheritance does not
/// change between them.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedText {
    /// Family name, or [`crate::measure::DEFAULT_FONT_FAMILY`] for any face.
    pub family: String,
    /// Em size in logical pixels.
    pub size: f32,
    /// Weight on the CSS numeric scale.
    pub weight: FontWeight,
    /// Upright or slanted.
    pub style: FontStyle,
    /// Glyph fill colour.
    pub color: Color,
    /// Horizontal placement within the line box.
    pub align: TextAlign,
    /// A line through, over or under the text.
    pub decoration: TextDecoration,
    /// Vertical placement within the line box.
    pub vertical_align: VerticalAlign,
    /// Which of fill and stroke is drawn on top.
    pub paint_order: PaintOrder,
    /// Line box height as a multiple of the font size.
    pub line_height: f32,
    /// Extra space added to every line box, in logical pixels.
    pub line_gap: f32,
    /// Space between glyphs.
    pub letter_spacing: Spacing,
    /// Space between words.
    pub word_spacing: Spacing,
    /// OpenType feature keywords applied to the run.
    pub font_variant: Vec<FontVariant>,
}

impl ResolvedText {
    /// A line box exactly as tall as the font asks for.
    ///
    /// CSS's initial `line-height` is `normal`, which is the font's own metrics
    /// rather than a multiple. Skia takes a multiplier, and `1.0` is how it
    /// spells "use the metrics".
    pub const NORMAL_LINE_HEIGHT: f32 = 1.0;

    /// The style a text node with no ancestor styling at all resolves to.
    #[must_use]
    pub fn initial() -> Self {
        Self {
            family: crate::measure::DEFAULT_FONT_FAMILY.to_owned(),
            size: crate::measure::DEFAULT_FONT_SIZE,
            weight: FontWeight::NORMAL,
            style: FontStyle::Normal,
            color: Color::BLACK,
            align: TextAlign::Start,
            decoration: TextDecoration::None,
            vertical_align: VerticalAlign::Top,
            paint_order: PaintOrder::Fill,
            line_height: Self::NORMAL_LINE_HEIGHT,
            line_gap: 0.0,
            letter_spacing: Spacing::Normal,
            word_spacing: Spacing::Normal,
            font_variant: Vec::new(),
        }
    }

    /// This style with everything `overlay` sets applied over it.
    ///
    /// A `None` field leaves the inherited value standing, which is the
    /// difference between "said nothing" and "said the initial value".
    #[must_use]
    pub fn inherit(&self, overlay: &TextStyle) -> Self {
        Self {
            family: overlay
                .font_family
                .clone()
                .unwrap_or_else(|| self.family.clone()),
            size: overlay.font_size.unwrap_or(self.size),
            weight: overlay.font_weight.unwrap_or(self.weight),
            style: overlay.font_style.unwrap_or(self.style),
            color: overlay.color.unwrap_or(self.color),
            align: overlay.text_align.unwrap_or(self.align),
            decoration: overlay.text_decoration.unwrap_or(self.decoration),
            vertical_align: overlay
                .vertical_align
                .unwrap_or(self.vertical_align),
            paint_order: overlay.paint_order.unwrap_or(self.paint_order),
            line_height: overlay.line_height.unwrap_or(self.line_height),
            line_gap: overlay.line_gap.unwrap_or(self.line_gap),
            letter_spacing: overlay
                .letter_spacing
                .unwrap_or(self.letter_spacing),
            word_spacing: overlay.word_spacing.unwrap_or(self.word_spacing),
            font_variant: overlay
                .font_variant
                .clone()
                .unwrap_or_else(|| self.font_variant.clone()),
        }
    }
}

/// A decoded raster image, and what the layout pass needs to know about it.
#[derive(Debug, Clone)]
pub struct DecodedImage {
    image: meo_skia_canvas::Image,
}

impl DecodedImage {
    /// The decoded bitmap, for the paint pass.
    pub(crate) const fn inner(&self) -> &meo_skia_canvas::Image {
        &self.image
    }

    /// The image's own size in pixels, which is what an `Auto` box takes.
    #[must_use]
    pub fn intrinsic_size(&self) -> Size {
        Size::new(self.image.width() as f32, self.image.height() as f32)
    }
}

/// A scene whose images are decoded and whose text styles are resolved.
///
/// Holds those beside the scene rather than inside it, so the scene stays the
/// cheap, `Send`, serialisable thing it is defined to be.
#[derive(Debug)]
pub struct Resolved<'scene> {
    /// The scene these tables belong to.
    pub scene: &'scene Scene,
    images: HashMap<NodeId, DecodedImage>,
    backgrounds: HashMap<NodeId, DecodedImage>,
    text: HashMap<NodeId, ResolvedText>,
}

impl<'scene> Resolved<'scene> {
    /// Reads every local source, decodes it, and folds text styles down each
    /// page.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnresolvedSource`] for any [`ImageSource::Url`],
    /// [`Error::ImageRead`] for a path that cannot be read,
    /// [`Error::UndecodableImage`] for bytes no decoder recognises, and
    /// [`Error::UnknownFont`] for a family neither registered nor installed.
    pub fn new(scene: &'scene Scene, fonts: &Fonts) -> Result<Self, Error> {
        let mut resolved = Self {
            scene,
            images: HashMap::new(),
            backgrounds: HashMap::new(),
            text: HashMap::new(),
        };

        for (id, node) in scene.nodes.iter().enumerate() {
            // The cast is exact: the arena is bounded by `MAX_NODES`, a `u32`.
            let id = NodeId::new(id as u32);
            match &node.kind {
                NodeKind::Image { source, .. } => {
                    let decoded = decode(id, source)?;
                    resolved.images.insert(id, decoded);
                }
                NodeKind::Box
                | NodeKind::Text { .. }
                | NodeKind::Path { .. } => {}
            }
            if let Some(background) = node.paint.background_image.as_ref() {
                // Kept in its own table rather than beside the image nodes: a
                // background is drawn into a box layout has already sized, so
                // its extent is not a layout input the way an image node's is,
                // and the measure pass must not find it by asking for a node's
                // image.
                let decoded = decode(id, &background.source)?;
                resolved.backgrounds.insert(id, decoded);
            }
        }

        resolved.resolve_text(fonts)?;
        Ok(resolved)
    }

    /// The decoded bitmap for an image node, if it is one.
    #[must_use]
    pub fn image(&self, node: NodeId) -> Option<&DecodedImage> {
        self.images.get(&node)
    }

    /// The decoded background bitmap for a node, if it has one.
    #[must_use]
    pub fn background(&self, node: NodeId) -> Option<&DecodedImage> {
        self.backgrounds.get(&node)
    }

    /// The fully-inherited text style for a text node, if it is one.
    #[must_use]
    pub fn text(&self, node: NodeId) -> Option<&ResolvedText> {
        self.text.get(&node)
    }

    /// Walks every page, carrying the inherited style down as it goes.
    ///
    /// Iterative with an explicit stack rather than recursive, for the reason
    /// [`Scene::validate`] is: a scene is caller data, and a tree deeper than
    /// the thread's stack would abort the process instead of returning.
    fn resolve_text(&mut self, fonts: &Fonts) -> Result<(), Error> {
        let mut stack: Vec<(NodeId, ResolvedText)> = self
            .scene
            .pages
            .iter()
            .map(|&page| (page, ResolvedText::initial()))
            .collect();

        while let Some((id, inherited)) = stack.pop() {
            let Some(node) = self.scene.get(id) else {
                continue;
            };
            let here = inherited.inherit(&node.text);

            if let NodeKind::Text { segments, .. } = &node.kind {
                for segment in segments {
                    let run = here.inherit(&segment.style);
                    check_family(fonts, &run.family)?;
                }
                check_family(fonts, &here.family)?;
                self.text.insert(id, here.clone());
            }

            for &child in &node.children {
                stack.push((child, here.clone()));
            }
        }
        Ok(())
    }
}

/// Whether this pass can obtain the given source without a network.
#[must_use]
pub const fn is_local(source: &ImageSource) -> bool {
    match source {
        ImageSource::Path(_) | ImageSource::Bytes(_) => true,
        ImageSource::Url(_) => false,
    }
}

fn check_family(fonts: &Fonts, family: &str) -> Result<(), Error> {
    if fonts.has(family) {
        Ok(())
    } else {
        Err(Error::UnknownFont(family.to_owned()))
    }
}

fn decode(node: NodeId, source: &ImageSource) -> Result<DecodedImage, Error> {
    // The `Path` arm owns what it read and the `Bytes` arm borrows what the
    // caller already holds, so only the one that has to allocate does. Making
    // both arms `Vec<u8>` reads more evenly and copies the whole file: a 5 MB
    // PNG already in the scene was copied 5 MB per image node and then dropped
    // unread, because `Image::from_encoded` takes a slice either way.
    let read;
    let bytes: &[u8] = match source {
        ImageSource::Bytes(bytes) => bytes,
        ImageSource::Path(path) => {
            read = std::fs::read(path).map_err(|source| Error::ImageRead {
                path: path.clone(),
                source,
            })?;
            &read
        }
        ImageSource::Url(_) => return Err(Error::UnresolvedSource(node)),
    };

    meo_skia_canvas::Image::from_encoded(bytes)
        .map(|image| DecodedImage { image })
        .map_err(|_| Error::UndecodableImage(node))
}

#[cfg(test)]
pub(crate) mod tests {
    use meo_canvas_scene::{
        Scene, Size,
        node::{ImageSource, Node, NodeId, NodeKind},
        style::{
            paint::Color,
            text::{FontWeight, TextStyle},
        },
    };

    use super::{Fonts, Resolved, ResolvedText, is_local};
    use crate::Error;

    /// A 4x2 opaque red PNG, written byte by byte rather than committed as a
    /// file. Seventy-five bytes is smaller than the smallest fixture worth
    /// tracking, it needs no licence note, and its 2:1 ratio is what the
    /// aspect-ratio branch of image measurement is checked against.
    pub(crate) const RED_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D,
        0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x02,
        0x08, 0x06, 0x00, 0x00, 0x00, 0x7F, 0xA8, 0x7D, 0x63, 0x00, 0x00, 0x00,
        0x12, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
        0x1F, 0x19, 0x33, 0xA0, 0x0B, 0x00, 0x00, 0x0F, 0x21, 0x0F, 0xF1, 0xFE,
        0x45, 0x14, 0x63, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE,
        0x42, 0x60, 0x82,
    ];

    /// Oswald, under the SIL Open Font Licence 1.1; the licence travels with it
    /// as `Oswald-OFL.txt` in the same directory.
    pub(crate) const TEST_FONT: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/assets/fonts/Oswald-VariableFont_wght.ttf"
    );

    /// The family name the test font is registered under.
    pub(crate) const TEST_FAMILY: &str = "MeoTest";

    pub(crate) fn test_fonts() -> Fonts {
        let fonts = Fonts::new();
        fonts
            .register_path(TEST_FAMILY, TEST_FONT)
            .unwrap_or_else(|error| unreachable!("{error}"));
        fonts
    }

    #[test]
    fn a_registered_family_is_found_and_an_invented_one_is_not() {
        let fonts = test_fonts();
        assert!(fonts.has(TEST_FAMILY));
        assert_eq!(fonts.registered(), vec![TEST_FAMILY.to_owned()]);
        assert!(!fonts.has("NoSuchFamilyExistsAnywhere"));
        // The empty family is what a node that named none resolves to, and it
        // always matches: Skia reads it as "any registered face".
        assert!(fonts.has(""));
        assert!(!format!("{fonts:?}").is_empty());
    }

    #[test]
    fn registering_something_that_is_not_a_font_is_an_error() {
        let fonts = Fonts::new();
        let error = fonts.register_bytes("Broken", b"not a font at all");
        assert!(matches!(error, Err(Error::FontRegister { .. })));
        assert!(fonts.register_path("Missing", "/no/such/font.ttf").is_err());
        assert!(!fonts.has("Broken"));
    }

    #[test]
    fn registering_from_bytes_matches_registering_from_a_path() {
        let bytes = std::fs::read(TEST_FONT)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let fonts = Fonts::new();
        assert!(fonts.register_bytes("FromBytes", &bytes).is_ok());
        assert!(fonts.has("FromBytes"));
    }

    #[test]
    fn a_url_is_refused_because_the_core_does_not_fetch() {
        assert!(is_local(&ImageSource::Path("a".to_owned())));
        assert!(is_local(&ImageSource::Bytes(Vec::new())));
        assert!(!is_local(&ImageSource::Url("https://a.test".to_owned())));

        let mut scene = Scene::new(Size::ZERO);
        let node = scene
            .push(NodeId::ROOT, image_node(ImageSource::Url("u".to_owned())))
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(matches!(
            Resolved::new(&scene, &Fonts::new()),
            Err(Error::UnresolvedSource(id)) if id == node
        ));
    }

    #[test]
    fn inline_bytes_decode_to_their_intrinsic_size() {
        let mut scene = Scene::new(Size::ZERO);
        let node = scene
            .push(
                NodeId::ROOT,
                image_node(ImageSource::Bytes(RED_PNG.to_vec())),
            )
            .unwrap_or_else(|error| unreachable!("{error}"));

        let resolved = Resolved::new(&scene, &Fonts::new())
            .unwrap_or_else(|error| unreachable!("{error}"));
        let image = resolved
            .image(node)
            .unwrap_or_else(|| unreachable!("the node is an image"));
        assert_eq!(image.intrinsic_size(), Size::new(4.0, 2.0));
        assert!(resolved.image(NodeId::ROOT).is_none());
        assert!(!format!("{image:?}").is_empty());
    }

    #[test]
    fn a_path_that_cannot_be_read_and_bytes_that_are_not_an_image() {
        let mut unreadable = Scene::new(Size::ZERO);
        unreadable
            .push(
                NodeId::ROOT,
                image_node(ImageSource::Path("/no/such/file.png".to_owned())),
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(matches!(
            Resolved::new(&unreadable, &Fonts::new()),
            Err(Error::ImageRead { .. })
        ));

        let mut garbage = Scene::new(Size::ZERO);
        let node = garbage
            .push(
                NodeId::ROOT,
                image_node(ImageSource::Bytes(vec![1, 2, 3, 4])),
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(matches!(
            Resolved::new(&garbage, &Fonts::new()),
            Err(Error::UndecodableImage(id)) if id == node
        ));
    }

    #[test]
    fn a_background_image_is_decoded_too() {
        let mut scene = Scene::new(Size::ZERO);
        scene.nodes[0].paint.background_image =
            Some(meo_canvas_scene::style::paint::BackgroundImage {
                source: ImageSource::Url("https://a.test/bg.png".to_owned()),
                repeat:
                    meo_canvas_scene::style::paint::BackgroundRepeat::Repeat,
                size: (None, None),
                position: (
                    meo_canvas_scene::Length::ZERO,
                    meo_canvas_scene::Length::ZERO,
                ),
            });
        assert!(matches!(
            Resolved::new(&scene, &Fonts::new()),
            Err(Error::UnresolvedSource(_))
        ));
    }

    #[test]
    fn a_text_style_inherits_down_the_tree() {
        let mut scene = Scene::new(Size::ZERO);
        scene.nodes[0].text = TextStyle {
            font_family: Some(TEST_FAMILY.to_owned()),
            font_size: Some(24.0),
            color: Some(Color::rgb(1, 2, 3)),
            ..TextStyle::default()
        };
        let middle = scene
            .push(NodeId::ROOT, Node::container())
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(middle) {
            node.text.font_weight = Some(FontWeight::BOLD);
        }
        let leaf = scene
            .push(middle, Node::text("inherited"))
            .unwrap_or_else(|error| unreachable!("{error}"));

        let fonts = test_fonts();
        let resolved = Resolved::new(&scene, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let style = resolved
            .text(leaf)
            .unwrap_or_else(|| unreachable!("the leaf is text"));

        // The family and size come from the root, the weight from the middle,
        // and nothing overwrote what an ancestor said.
        assert_eq!(style.family, TEST_FAMILY);
        assert!((style.size - 24.0).abs() < f32::EPSILON);
        assert_eq!(style.weight, FontWeight::BOLD);
        assert_eq!(style.color, Color::rgb(1, 2, 3));
        // Only text nodes get an entry.
        assert!(resolved.text(middle).is_none());
    }

    #[test]
    fn a_family_no_one_has_is_refused_before_layout_starts() {
        let mut scene = Scene::new(Size::ZERO);
        let leaf = scene
            .push(NodeId::ROOT, Node::text("x"))
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(leaf) {
            node.text.font_family = Some("NoSuchFamilyExists".to_owned());
        }
        assert!(matches!(
            Resolved::new(&scene, &Fonts::new()),
            Err(Error::UnknownFont(family)) if family == "NoSuchFamilyExists"
        ));
    }

    #[test]
    fn a_segment_may_name_a_family_of_its_own_and_it_is_checked() {
        let mut scene = Scene::new(Size::ZERO);
        let leaf = scene
            .push(NodeId::ROOT, Node::text("x"))
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(leaf)
            && let NodeKind::Text { segments, .. } = &mut node.kind
        {
            segments[0].style.font_family = Some("AlsoMissing".to_owned());
        }
        assert!(matches!(
            Resolved::new(&scene, &Fonts::new()),
            Err(Error::UnknownFont(family)) if family == "AlsoMissing"
        ));
    }

    #[test]
    fn the_initial_style_is_what_nothing_set() {
        let initial = ResolvedText::initial();
        assert_eq!(initial.family, crate::measure::DEFAULT_FONT_FAMILY);
        assert!(
            (initial.size - crate::measure::DEFAULT_FONT_SIZE).abs()
                < f32::EPSILON
        );
        assert_eq!(initial.weight, FontWeight::NORMAL);
        assert_eq!(initial.color, Color::BLACK);
        assert!(
            (initial.line_height - ResolvedText::NORMAL_LINE_HEIGHT).abs()
                < f32::EPSILON
        );
        assert!(initial.font_variant.is_empty());

        // An overlay that sets nothing changes nothing.
        assert_eq!(initial.inherit(&TextStyle::default()), initial);
        assert!(!format!("{initial:?}").is_empty());
    }

    #[test]
    fn resolving_a_scene_with_nothing_in_it_succeeds() {
        let scene = Scene::new(Size::new(1.0, 1.0));
        let resolved = Resolved::new(&scene, &Fonts::new())
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(resolved.scene.len(), 1);
        assert!(!format!("{resolved:?}").is_empty());
    }

    fn image_node(source: ImageSource) -> Node {
        Node::new(NodeKind::Image {
            source,
            fit: meo_canvas_scene::style::paint::ObjectFit::Fill,
            position: (
                meo_canvas_scene::Length::ZERO,
                meo_canvas_scene::Length::ZERO,
            ),
            frame: None,
        })
    }
}
