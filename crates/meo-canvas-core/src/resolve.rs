//! Turns a scene's external references into things the later passes can use.
//!
//! Three jobs, all of which have to finish before taffy is asked anything.
//! Fonts are registered, so a family a node names can be found. Images are read
//! and decoded, because an image sized `Auto` on both axes takes its extent
//! from the decoded bitmap and that extent is a layout input. Text styles are
//! folded down the tree, so a text node carries the family its container set
//! rather than a chain of ancestors to walk at measure time.
//!
//! This pass reads local files and accepts bytes the caller already holds.
//! **Whether it also fetches is a build-time decision**: with the `net` feature
//! off -- the default -- an [`ImageSource::Url`] arriving here is
//! [`Error::UnresolvedSource`] and no HTTP stack is linked. With it on, the URL
//! is fetched over a blocking client.
//!
//! Blocking because an async client would put a runtime in every Rust consumer
//! of the public crate, including those already inside one, and a blocking one
//! puts none. The dependency is still real, which is why the feature is off
//! unless asked for.
//!
//! The TypeScript surface fetches before it encodes and sends bytes, so it
//! never produces a URL source at all. **The two surfaces therefore fail the
//! same way** -- a URL reaching a build without `net` is refused on both sides,
//! and the difference between them is a flag rather than a capability gap.
//!
//! # One resolve per scene, not per page
//!
//! The caches here are keyed by [`NodeId`] alone, with no page beside it. That
//! is the whole reason [`Scene`] holds one arena for every page: two pages that
//! draw the same file decode it once, and the layout pass that runs per page
//! reads a table that was built once.
//!
//! # Registering a font changes the thread, not the registry
//!
//! **A face registered through [`Fonts`] is registered for the whole thread,
//! and stays registered after the `Fonts` that registered it is dropped.**
//! `meo-skia-canvas` keeps a `FontLibrary` behind its API and this crate
//! cannot opt out of it; what this crate can do, and does, is add no second
//! one.
//!
//! Measured, because none of it is guessable from the type. Register a family
//! in one `Fonts`, drop it, and a `Fonts` built afterwards **on the same
//! thread** answers `has(family)` with `true` while its own `registered()` is
//! empty, and a `Renderer` built afterwards draws text in that family. Nothing
//! on this surface unregisters anything.
//!
//! **The scope is the thread and not the process**, which is the difference
//! between a hazard and a catastrophe: a family registered on a worker is
//! invisible to the main thread and to every other worker, and dies when that
//! thread does. Measured in both directions -- registering inside a spawned
//! thread leaves the main thread answering `false` after it joins, and a
//! family registered on the main thread is `false` in a thread spawned after
//! it.
//!
//! **This is the shape a server has to plan around.** Faces belong at the
//! start of each thread that renders, not per request: a request that
//! registers a family has changed every later request **on that thread**, and
//! a request that forgets to register one may still render, with whatever an
//! earlier request on the same thread left behind -- which is not an error in
//! a log, it is the wrong typeface in a picture nobody looks at twice. A pool
//! of render threads does not share the problem; it has one copy of it each,
//! and each worker still needs its own registration.
//!
//! `Fonts` being a value a caller holds reads as a scope and is not one.
//!
//! The signature says so where the type does not: `register_path` and
//! `register_bytes` take `&self`. Registering is not a mutation of this value,
//! because this value is not what changes.
//!
//! Everything else here is scoped as it looks. Nothing in this module is a
//! `static`, the caches are owned per resolve, and two renders on two threads
//! share nothing and contend for nothing **except the font registry**.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, hash_map::Entry},
    path::Path,
    rc::Rc,
    sync::OnceLock,
};

use base64::{
    Engine as _,
    engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig},
};
use meo_canvas_scene::{
    OnImageError, Scene, Size,
    node::{HttpOptions, ImageSource, NodeId, NodeKind},
    style::{
        PaintOrder,
        effect::Mask,
        paint::Color,
        text::{
            FontStyle, FontVariant, FontWeight, LineHeight, Spacing, TextAlign,
            TextDecoration, TextStroke, TextStyle, VerticalAlign,
        },
    },
};
use meo_skia_canvas::image::Svg;

use crate::{Error, FetchFailure, ImageWarning};

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
    /// The families the platform already has, read once: enumerating them
    /// walks the system's font directories, and the answer cannot change
    /// while the process runs.
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

    /// Whether a family can be drawn with **anywhere on this thread**.
    ///
    /// True for a family registered through any `Fonts` on this thread, not
    /// only this one, and for a family installed on the platform. It stays
    /// true after the `Fonts` that registered the family is dropped, and it is
    /// false on a thread where nothing registered it.
    ///
    /// **This and [`Fonts::registered`] answer about different scopes, and
    /// that is deliberate rather than an oversight.** This one answers "can
    /// this be drawn"; that one answers "what did I register". A caller who
    /// asks both of a registry that registered nothing gets `true` here and an
    /// empty list there, which looks like an inconsistent library and is two
    /// correct answers to two different questions. The scope is in each
    /// signature's doc because it is not in the type: `Fonts` is a value a
    /// caller holds, and the registry underneath it is the thread's.
    #[must_use]
    pub fn has(&self, family: &str) -> bool {
        if family.is_empty() || self.library.has_font(family) {
            return true;
        }
        self.installed()
            .iter()
            .any(|installed| installed.eq_ignore_ascii_case(family))
    }

    /// The families **this** registry registered, in registration order.
    ///
    /// Not what the thread can draw with -- that is [`Fonts::has`], and the
    /// two differ whenever anything else on this thread has registered a face.
    /// This is the narrower and less obvious of the two, and it is the one
    /// worth keeping: "what did I register" has callers a diagnostic, a test
    /// and a service logging its own start-up, and the thread-wide answer is
    /// already available from `has`.
    #[must_use]
    pub fn registered(&self) -> Vec<String> {
        self.library.families()
    }

    /// The registry the measure pass builds its text engine from.
    /// Test-only: the live path measures through
    /// [`crate::lines::TextMeasurer`], whose canvas resolves families from the
    /// thread-wide font library without being handed one.
    #[cfg(test)]
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
    /// An outline drawn around the glyphs, if the style asks for one.
    pub text_stroke: Option<TextStroke>,
    /// Which of fill and stroke is drawn on top.
    pub paint_order: PaintOrder,
    /// How tall a line box is, or `None` for the face's own -- CSS's
    /// `normal`.
    ///
    /// **An `Option` because `1.0` is a value a caller can ask for.** A line
    /// box exactly one em tall is legal CSS and is not `normal`.
    ///
    /// **Never [`LineHeight::Percent`].** A percentage resolves against the
    /// font size of the element that declared it, and [`Self::inherit`] does
    /// that as it merges -- so what descends is a length and nothing here
    /// resolves one late. A [`LineHeight::Number`] is deliberately *not*
    /// resolved there: it is recomputed by whoever inherits it, against their
    /// own size.
    pub line_height: Option<LineHeight>,
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
            text_stroke: None,
            paint_order: PaintOrder::Fill,
            line_height: None,
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
        // Hoisted out of the literal because the line-height arm below needs
        // it: a percentage resolves against **the declaring element's own**
        // font size, which is this one and not the parent's.
        let size = overlay.font_size.unwrap_or(self.size);
        Self {
            family: overlay
                .font_family
                .clone()
                .unwrap_or_else(|| self.family.clone()),
            size,
            weight: overlay.font_weight.unwrap_or(self.weight),
            style: overlay.font_style.unwrap_or(self.style),
            color: overlay.color.unwrap_or(self.color),
            align: overlay.text_align.unwrap_or(self.align),
            decoration: overlay.text_decoration.unwrap_or(self.decoration),
            vertical_align: overlay
                .vertical_align
                .unwrap_or(self.vertical_align),
            text_stroke: overlay.text_stroke.or(self.text_stroke),
            paint_order: overlay.paint_order.unwrap_or(self.paint_order),
            // `.or`, not `unwrap_or`, so an explicit `1.0` stays distinct from
            // an inherited `normal`. A percentage resolves here, against the
            // declaring element's size, and a number descends as a number;
            // `inherited` below pins both.
            line_height: match overlay.line_height {
                Some(LineHeight::Percent(share)) => {
                    Some(LineHeight::Length(share * size))
                }
                Some(stated) => Some(stated),
                None => self.line_height,
            },
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

/// A decoded image, and what the layout pass needs to know about it.
///
/// **Two kinds, because a vector document is not a bitmap.** A raster source
/// decodes once into pixels of a fixed size; an SVG has an intrinsic size and
/// a rasterisation that takes the size it is drawn at, so the document is kept
/// and pixels are made at each use.
///
/// **This type is `Clone` and not `Send`.** `Clone` because every node drawing
/// a source gets its own handle to the one decode; `!Send` because
/// `meo_skia_canvas::Svg` wraps `SkSVGDOM`, which is neither `Send` nor `Sync`
/// -- so a parsed document cannot leave the thread that parsed it, and
/// [`Resolved`] cannot either. The alternative was to keep the XML and parse
/// again on the drawing thread, which is a second 21 ms parse of every
/// document on every render, because the layout pass asks for
/// [`Self::intrinsic_size`] before the paint pass exists.
#[derive(Debug, Clone)]
pub struct DecodedImage {
    kind: Kind,
}

/// What a decoded source turned out to be.
#[derive(Debug, Clone)]
enum Kind {
    /// Pixels, at the size the file states.
    Raster(meo_skia_canvas::Image),
    /// A parsed document shared by every node naming this source, being
    /// expensive to parse and cheap to share. `RefCell` because `rasterize`
    /// takes `&mut self` to set the container size, and its raster is memoised
    /// beside it.
    Vector(Rc<RefCell<Vector>>),
}

/// A parsed SVG document and the last raster made from it.
struct Vector {
    /// The document itself.
    svg: Svg,
    /// Its own size, which an `Auto` box takes, read once at parse time:
    /// layout asks for it twice per node even when both dimensions are
    /// stated.
    intrinsic: Size,
    /// The last size and tint this document was rasterised for, and the
    /// pixels. One entry, since re-rasterising costs microseconds against
    /// a parse of milliseconds; keyed by the tint because one decode is
    /// shared by every node.
    raster: Option<((u32, u32), Option<Color>, meo_skia_canvas::Image)>,
}

/// **Written out because `meo_skia_canvas::Svg` has no `Debug`.** The document
/// itself has nothing a reader wants anyway; what identifies one here is the
/// size it states and whether it has been rasterised yet.
impl std::fmt::Debug for Vector {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Vector")
            .field("intrinsic", &self.intrinsic)
            .field(
                "raster",
                &self.raster.as_ref().map(|(size, tint, _)| (size, tint)),
            )
            // The document itself is the field left out, and it is left out
            // because `meo_skia_canvas::Svg` has no `Debug` to call.
            .finish_non_exhaustive()
    }
}

impl DecodedImage {
    /// Pixels for the paint pass at the size they will be drawn: a raster
    /// ignores the size, a document is rasterised at exactly it.
    /// [`Error::UndecodableImage`] names the node when a document cannot be
    /// rasterised.
    pub(crate) fn raster(
        &self,
        size: (u32, u32),
        tint: Option<Color>,
        node: NodeId,
    ) -> Result<meo_skia_canvas::Image, Error> {
        match &self.kind {
            // A colour has no reading on a bitmap, so it is refused rather than
            // ignored, here because a writer sees only a filename or a URL for
            // two of the three source forms. The caller learns at render time.
            Kind::Raster(_) if tint.is_some() => Err(Error::TintOnRaster(node)),
            Kind::Raster(image) => Ok(image.clone()),
            Kind::Vector(document) => {
                let mut document = document.borrow_mut();
                if let Some((made_at, made_for, image)) = &document.raster
                    && *made_at == size
                    && *made_for == tint
                {
                    return Ok(image.clone());
                }
                // Set only when asked: calling with a default would make that
                // default ours rather than the document's, and nothing
                // downstream could tell them apart.
                if let Some(color) = tint {
                    document.svg.set_current_color(
                        meo_skia_canvas::RgbaLinear::from_srgb8(
                            color.r,
                            color.g,
                            color.b,
                            f32::from(color.a) / 255.0,
                        ),
                    );
                }
                let image = document
                    .svg
                    .rasterize(size.0.max(1), size.1.max(1))
                    .map_err(|_| Error::UndecodableImage(node))?;
                document.raster = Some((size, tint, image.clone()));
                Ok(image)
            }
        }
    }

    /// The frame a node asked for, or this image unchanged. A one-frame raster
    /// ignores the index; a document refuses any but zero, and an index past a
    /// raster's last frame is [`Error::UndecodableImage`] naming the node.
    fn at_frame(self, frame: Option<u32>, node: NodeId) -> Result<Self, Error> {
        let Some(index) = frame.map(|index| index as usize) else {
            return Ok(self);
        };
        // A document has one frame and SVG animation is not rasterised here, so
        // a later frame is refused as the fourth frame of a two-frame GIF is.
        let Kind::Raster(image) = &self.kind else {
            return if index == 0 {
                Ok(self)
            } else {
                Err(Error::UndecodableImage(node))
            };
        };
        if index == 0 || image.frame_count() <= 1 {
            return Ok(self);
        }
        if index >= image.frame_count() {
            return Err(Error::UndecodableImage(node));
        }
        image
            .frame(index)
            .map(|image| Self {
                kind: Kind::Raster(image),
            })
            .map_err(|_| Error::UndecodableImage(node))
    }

    /// The image's own size in pixels, which is what an `Auto` box takes.
    #[must_use]
    pub fn intrinsic_size(&self) -> Size {
        match &self.kind {
            Kind::Raster(image) => {
                Size::new(image.width() as f32, image.height() as f32)
            }
            Kind::Vector(document) => document.borrow().intrinsic,
        }
    }
}

/// A scene whose images are decoded and whose text styles are resolved.
///
/// Holds those beside the scene rather than inside it, so the scene stays the
/// cheap, `Send`, serialisable thing it is defined to be.
#[derive(Debug)]
pub struct Resolved<'scene> {
    /// Every image source that could not be resolved, in node order.
    ///
    /// Empty on every render where nothing failed, and `Vec::new` does not
    /// allocate, so the ordinary case costs nothing.
    warnings: Vec<ImageWarning>,
    /// The scene these tables belong to.
    pub scene: &'scene Scene,
    images: HashMap<NodeId, DecodedImage>,
    backgrounds: HashMap<NodeId, DecodedImage>,
    masks: HashMap<NodeId, DecodedImage>,
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
            warnings: Vec::new(),
            scene,
            images: HashMap::new(),
            backgrounds: HashMap::new(),
            masks: HashMap::new(),
            text: HashMap::new(),
        };

        // One decode per distinct source rather than per node. Sixty nodes
        // drawing one picture decoded it sixty times, which nothing pointed
        // at because every table was keyed by node and every node got the
        // right bytes -- just not the same ones.
        let (once, warnings, softened) = decode_sources(scene)?;
        resolved.warnings = warnings;

        for (id, node) in scene.nodes.iter().enumerate() {
            // The cast is exact: the arena is bounded by `MAX_NODES`, a `u32`.
            let id = NodeId::new(id as u32);
            // Only an image names a source to resolve. Every other kind
            // has nothing to decode, including one this build does not know:
            // `NodeKind` is `#[non_exhaustive]`, so `if let` says that more
            // honestly than a match with a wildcard that means "and the rest".
            if let NodeKind::Image { source, frame, .. } = &node.kind {
                // `at_frame` per node, since two nodes may share a decode and
                // want different frames. A softened source is absent rather
                // than inserted, so the measurer and painter take the arm they
                // have for an image they were never given.
                if let Some(decoded) = taken(&once, &softened, id, source)? {
                    resolved.images.insert(id, decoded.at_frame(*frame, id)?);
                }
            }
            if let Some(background) = node.paint.background_image.as_ref() {
                // In its own table: a background is drawn into a box layout has
                // already sized, so the measure pass must not find it by asking
                // for a node's image.
                if let Some(decoded) =
                    taken(&once, &softened, id, &background.source)?
                {
                    resolved.backgrounds.insert(id, decoded);
                }
            }
            // A third table for the same reason the second one exists: a mask
            // image is neither the node's own picture nor its background, and
            // a node may carry all three at once.
            if let Some(Mask::Image(source)) = node.effects.mask.as_ref()
                && let Some(decoded) = taken(&once, &softened, id, source)?
            {
                resolved.masks.insert(id, decoded);
            }
        }

        resolved.resolve_text(fonts)?;
        Ok(resolved)
    }

    /// Takes the warnings out, leaving the tables behind.
    ///
    /// Consuming rather than cloning: the render result owns them afterwards
    /// and a copy would be a second list to keep in step.
    #[must_use]
    pub fn into_warnings(self) -> Vec<ImageWarning> {
        self.warnings
    }

    /// The scene these tables were resolved against.
    #[must_use]
    pub const fn scene(&self) -> &'scene Scene {
        self.scene
    }

    /// Every image source that could not be resolved, in node order.
    ///
    /// One entry per distinct source rather than per node, because sixty nodes
    /// drawing one dead URL is one thing that went wrong.
    #[must_use]
    pub fn warnings(&self) -> &[ImageWarning] {
        &self.warnings
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

    /// The decoded bitmap for a node's mask, if its mask is an image.
    #[must_use]
    pub fn mask(&self, node: NodeId) -> Option<&DecodedImage> {
        self.masks.get(&node)
    }

    /// The fully-inherited text style for a text node, if it is one.
    #[must_use]
    pub fn text(&self, node: NodeId) -> Option<&ResolvedText> {
        self.text.get(&node)
    }

    /// Walks every page, carrying the inherited style down, on an explicit
    /// stack as [`Scene::validate`] does: a tree deeper than the thread's stack
    /// would abort the process.
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
        ImageSource::Url { .. } => false,
    }
}

fn check_family(fonts: &Fonts, family: &str) -> Result<(), Error> {
    if fonts.has(family) {
        Ok(())
    } else {
        Err(Error::UnknownFont(family.to_owned()))
    }
}

/// Every source in a scene, decoded once each and in parallel, keyed by the
/// source alone: the frame is applied per node afterwards. The error a scene
/// gets is the first failing source in node order, not the first thread to
/// finish.
type Decoded<'scene> = (
    HashMap<&'scene ImageSource, DecodedImage>,
    Vec<ImageWarning>,
    HashSet<&'scene ImageSource>,
);

fn decode_sources<'scene>(
    scene: &'scene Scene,
) -> Result<Decoded<'scene>, Error> {
    let mut wanted: Vec<(&ImageSource, NodeId)> = Vec::new();
    // How many nodes named each source. The map is built either way because
    // the dedup needs it; counting is one increment on a lookup that already
    // happens, so a scene where nothing fails pays nothing extra for it.
    let mut seen: HashMap<&ImageSource, usize> = HashMap::new();
    let mut want =
        |source: &'scene ImageSource, id: NodeId| match seen.entry(source) {
            Entry::Occupied(mut count) => *count.get_mut() += 1,
            Entry::Vacant(slot) => {
                slot.insert(1);
                wanted.push((source, id));
            }
        };
    for (id, node) in scene.nodes.iter().enumerate() {
        let id = NodeId::new(id as u32);
        if let NodeKind::Image { source, .. } = &node.kind {
            want(source, id);
        }
        if let Some(background) = node.paint.background_image.as_ref() {
            want(&background.source, id);
        }
        if let Some(Mask::Image(source)) = node.effects.mask.as_ref() {
            want(source, id);
        }
    }

    // **Parsed here rather than on the workers.** Reading and raster-decoding
    // stay parallel; an SVG comes back as bytes and is parsed on this thread,
    // because a parsed document is neither `Send` nor `Sync` and cannot be
    // carried out of a worker at all.
    let decoded: Vec<Result<DecodedImage, Error>> =
        in_parallel(&wanted, &scene.http)
            .into_iter()
            .zip(wanted.iter())
            .map(|(fetched, (_, node))| {
                fetched.and_then(|it| parsed(it, *node))
            })
            .collect();
    let mut once = HashMap::with_capacity(decoded.len());
    // **`Vec::new` does not allocate.** A render where every source resolves
    // never pushes, so this costs three words of stack and no heap at all --
    // which is what lets the diagnostic exist without the loaded path paying
    // for it.
    let mut warnings = Vec::new();
    // Empty on every render where nothing fails, and `HashSet::new` does not
    // allocate.
    let mut softened: HashSet<&ImageSource> = HashSet::new();
    for ((source, node), result) in wanted.iter().zip(decoded) {
        match result {
            Ok(image) => {
                once.insert(*source, image);
            }
            // Only a `Url` may soften, and only when the scene asked: an
            // unreadable `Path` or undecodable `Bytes` is the caller's own
            // input, and softening it would turn a defect in our decoders into
            // a missing picture.
            Err(error) => {
                let warning = soft(scene, *node, source, error, seen[source])?;
                // **Recorded as a fact, not inferred later from the source's
                // type.** `taken` has to tell "softened on purpose" from
                // "absent for a reason nobody noticed", and only this loop
                // knows which it was.
                softened.insert(*source);
                warnings.push(warning);
            }
        }
    }
    Ok((once, warnings, softened))
}

/// Turns a failed source into a warning or gives the error back, so which
/// source softens under which policy is decided in one place.
fn soft(
    scene: &Scene,
    node: NodeId,
    source: &ImageSource,
    error: Error,
    nodes: usize,
) -> Result<ImageWarning, Error> {
    if scene.on_image_error == OnImageError::Throw {
        return Err(error);
    }
    let ImageSource::Url { url, .. } = source else {
        return Err(error);
    };
    // A `data:` URI in a `Url` wrapper is still the caller's own bytes, so it
    // goes with `Bytes`; otherwise the same payload would soften as `{ url }`
    // and throw as a bare string.
    if is_data_uri(url) {
        return Err(error);
    }
    match error {
        Error::SourceFetch {
            detail, failure, ..
        } => Ok(ImageWarning {
            url: url.clone(),
            node,
            failure,
            detail,
            nodes,
        }),
        // Fetched, and then would not decode. The bytes came from the world
        // rather than from the caller, so this is the same class of fact as a
        // 404 and softens with it. An SVG that would not parse is the same
        // fact about the same bytes, said more precisely.
        Error::UndecodableImage(_) | Error::UnparsableSvg(_) => {
            Ok(ImageWarning {
                url: url.clone(),
                node,
                failure: FetchFailure::Other,
                detail:
                    "the bytes fetched are not an image any decoder here reads"
                        .to_owned(),
                nodes,
            })
        }
        // `UnresolvedSource` alone is the `net` feature being off, which must
        // keep naming the flag. It softens only when `image_fetch_attempts`
        // says the npm surface already tried, carrying the reason, so a 404
        // warns alike on both.
        Error::UnresolvedSource(_) => {
            let Some(attempt) = scene
                .image_fetch_attempts
                .iter()
                .find(|attempt| attempt.url == *url)
            else {
                return Err(error);
            };
            // No reconstruction: the status travels inside the variant, so
            // there is no absent-code case to invent a number for.
            Ok(ImageWarning {
                url: url.clone(),
                node,
                failure: FetchFailure::from(attempt.failure),
                detail: attempt.detail.clone(),
                nodes,
            })
        }

        // Everything else stays loud: a missing picture earns a placeholder,
        // and a panicked decoder or a broken invariant must not. No catch-all,
        // so a new `Error` variant is loud until somebody decides otherwise
        // here.
        other => Err(other),
    }
}

/// Decodes a list of sources across the machine's threads, results in the order
/// asked. No speed-up is claimed: whether Skia's decode is bound by the CPU or
/// by a lock inside it is unmeasured.
fn in_parallel(
    wanted: &[(&ImageSource, NodeId)],
    scene_http: &HttpOptions,
) -> Vec<Result<Fetched, Error>> {
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(wanted.len().max(1));
    if threads <= 1 || wanted.len() <= 1 {
        return wanted
            .iter()
            .map(|(source, id)| decode(*id, source, scene_http))
            .collect();
    }

    let mut out: Vec<Result<Fetched, Error>> = Vec::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = wanted
            .chunks(wanted.len().div_ceil(threads))
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .map(|(source, id)| decode(*id, source, scene_http))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for handle in handles {
            match handle.join() {
                Ok(part) => out.extend(part),
                // A decoder that panicked is a bug in the decoder, and the
                // scene still answers. Its own variant, so `soft` cannot
                // downgrade the crash to a placeholder.
                Err(_) => out.push(Err(Error::DecoderPanicked(NodeId::ROOT))),
            }
        }
    });
    out
}

/// The decode a node asked for, taken from what was decoded up front.
fn taken(
    once: &HashMap<&ImageSource, DecodedImage>,
    softened: &HashSet<&ImageSource>,
    node: NodeId,
    source: &ImageSource,
) -> Result<Option<DecodedImage>, Error> {
    match once.get(source) {
        Some(image) => Ok(Some(image.clone())),
        // Absent for one of two reasons, told apart by what happened rather
        // than by the source's type: a softened source reaches layout as no
        // image, and anything else absent is this function's invariant broken.
        None if softened.contains(source) => Ok(None),
        None => Err(Error::UndecodableImage(node)),
    }
}

/// Reads a URL source over HTTP, blocking, within the constants below. They are
/// fixed, since a configurable timeout can be infinity; other bounds mean
/// passing `ImageSource::Bytes`. `http` refuses a malformed header before
/// sending.
#[cfg(feature = "net")]
fn fetch(url: &str, http: &HttpOptions) -> Result<Vec<u8>, Error> {
    use std::io::Read as _;

    let refuse = |error: ureq::Error| Error::SourceFetch {
        url: url.to_owned(),
        detail: error.to_string(),
        failure: classify(&error),
    };

    let mut request = agent().get(url);
    for (name, value) in &http.headers {
        request = request.header(name, value);
    }
    let mut response = request.call().map_err(refuse)?;

    // The limit is counted here: `ureq`'s own reports `BodyExceedsLimit`
    // without a timeout and a bare `Io` with one, measured on a 33 MiB
    // response. One byte past the limit is read, so exactly the limit passes
    // without `Content-Length`.
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| Error::SourceFetch {
            url: url.to_owned(),
            detail: error.to_string(),
            failure: FetchFailure::Transport,
        })?;

    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err(Error::SourceFetch {
            url: url.to_owned(),
            detail: format!(
                "the image is larger than the {} MiB this renderer fetches",
                MAX_IMAGE_BYTES / (1024 * 1024)
            ),
            failure: FetchFailure::TooLarge,
        });
    }
    Ok(bytes)
}

/// How long a connection may take to establish: about sixteen worst-case
/// intercontinental round trips, and a host that cannot complete a handshake
/// in that will not deliver an image.
#[cfg(feature = "net")]
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// How long the whole fetch may take, body included: only a global clock
/// bounds a host dripping one byte inside every window. Sixty seconds against
/// [`MAX_IMAGE_BYTES`] is a floor of about 4.5 Mbit/s; the two are one
/// decision.
#[cfg(feature = "net")]
const GLOBAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// The largest image this crate fetches, chosen here rather than inherited
/// from `ureq`. Arithmetic, not measured: 720p GIF at about a bit per pixel is
/// 10.4 MB for three seconds, past `ureq`'s ten mebibytes, and thirty-two holds
/// about nine seconds.
#[cfg(feature = "net")]
const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;

/// `classify` against errors the fetch agent really raises, and the lookup
/// rules behind `HostNotFound` against each platform's own wording.
#[cfg(all(test, feature = "net"))]
mod fetch_classification {
    use super::{LOOKUP_PREFIX, agent, classify, lookup_is_transient, relabel};
    use crate::FetchFailure;

    /// A Unix lookup failure as std builds it: `gai_strerror`'s text after
    /// [`LOOKUP_PREFIX`], with no OS code.
    fn unix_lookup(detail: &str) -> std::io::Error {
        std::io::Error::other(format!("{LOOKUP_PREFIX}{detail}"))
    }

    #[test]
    fn a_url_with_no_scheme_is_the_callers_to_fix_and_not_to_retry() {
        // `BadUri` is the one class a caller can act on without a network at
        // all, and the only one this test can reach without one.
        let Err(refused) = agent().get("not-a-url").call() else {
            unreachable!("a URL with no scheme is refused")
        };
        assert_eq!(classify(&refused), FetchFailure::BadUrl);
    }

    #[test]
    fn a_host_that_does_not_resolve_is_not_a_transport_failure() {
        // A name that does not resolve will not resolve on a retry, and a
        // socket that dropped may connect on one. RFC 6761 reserves `.invalid`
        // never to resolve, so a resolver that answers it is rewriting
        // NXDOMAIN.
        let result = agent().get("http://invalid.invalid/a.png").call();
        let refused = result.err();
        assert_eq!(
            refused.as_ref().map(classify),
            Some(FetchFailure::HostNotFound),
            "`invalid.invalid` came back as {refused:?}, not as a name that \
             does not resolve; if this network's resolver answers `.invalid`, \
             which RFC 6761 reserves, it is rewriting NXDOMAIN"
        );
    }

    #[test]
    fn transient_lookups_by_platform() {
        // `EAI_AGAIN`'s text on each libc, and the codes std reports.
        for (code, message, windows, transient) in [
            (None, "Temporary failure in name resolution", false, true),
            (None, "Try again", false, true),
            (
                None,
                "nodename nor servname provided, or not known",
                false,
                false,
            ),
            (None, "Name or service not known", false, false),
            (None, "Name does not resolve", false, false),
            (
                None,
                "Non-recoverable failure in name resolution",
                false,
                false,
            ),
            (Some(61), "", false, true),
            (Some(11001), "", true, false),
            (Some(11002), "", true, true),
            (Some(11004), "", true, false),
            (Some(10050), "", true, true),
        ] {
            let text = if code.is_some() {
                String::from("os error")
            } else {
                format!("{LOOKUP_PREFIX}{message}")
            };
            assert_eq!(
                lookup_is_transient(code, &text, windows),
                transient,
                "{code:?} {message:?} on windows={windows}"
            );
        }
    }

    #[test]
    fn only_a_lookup_answer_becomes_host_not_found() {
        let answered = relabel(ureq::Error::Io(unix_lookup(
            "nodename nor servname provided, or not known",
        )));
        assert_eq!(classify(&answered), FetchFailure::HostNotFound);

        let outage = relabel(ureq::Error::Io(unix_lookup(
            "Temporary failure in name resolution",
        )));
        assert_eq!(classify(&outage), FetchFailure::Transport);

        let late = relabel(ureq::Error::Timeout(ureq::Timeout::Resolve));
        assert!(
            matches!(late, ureq::Error::Timeout(ureq::Timeout::Resolve)),
            "a lookup that ran out of time came back as {late:?}"
        );
    }
}

/// The agent every fetch goes through: this crate's bounds, and a resolver
/// that tells a name that does not exist from a lookup that could not finish.
#[cfg(feature = "net")]
fn agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_global(Some(GLOBAL_TIMEOUT))
        .build();
    ureq::Agent::with_parts(
        config,
        ureq::unversioned::transport::DefaultConnector::default(),
        Lookup::default(),
    )
}

/// `ureq`'s own resolver, with its failures passed through [`relabel`].
/// `ureq` raises `HostNotFound` only for a lookup that succeeds with no usable
/// address; a lookup that fails arrives as `Io`, which reads as transport.
#[cfg(feature = "net")]
#[derive(Debug, Default)]
struct Lookup(ureq::unversioned::resolver::DefaultResolver);

#[cfg(feature = "net")]
impl ureq::unversioned::resolver::Resolver for Lookup {
    fn resolve(
        &self,
        uri: &ureq::http::Uri,
        config: &ureq::config::Config,
        timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<ureq::unversioned::resolver::ResolvedSocketAddrs, ureq::Error>
    {
        self.0.resolve(uri, config, timeout).map_err(relabel)
    }
}

/// A failed lookup as `HostNotFound`, unless it was an outage, which stays
/// `Io` and so reads as transport. A lookup that ran out of time is already
/// `Timeout` and passes through, as does every other error.
#[cfg(feature = "net")]
fn relabel(error: ureq::Error) -> ureq::Error {
    match error {
        ureq::Error::Io(io)
            if !lookup_is_transient(
                io.raw_os_error(),
                &io.to_string(),
                cfg!(windows),
            ) =>
        {
            ureq::Error::HostNotFound
        }
        other => other,
    }
}

/// Whether a failed lookup was an outage rather than an answer. Windows gives
/// its socket error code; Unix gives an OS code only for `EAI_SYSTEM`, and
/// otherwise `gai_strerror`'s text after [`LOOKUP_PREFIX`], so that text is
/// what tells `EAI_AGAIN` apart. Pinned by `transient_lookups_by_platform`.
#[cfg(feature = "net")]
fn lookup_is_transient(
    code: Option<i32>,
    message: &str,
    windows: bool,
) -> bool {
    match code {
        Some(code) if windows => !WINSOCK_ANSWERS.contains(&code),
        Some(_) => true,
        None => message
            .strip_prefix(LOOKUP_PREFIX)
            .is_some_and(|detail| LOOKUP_TRANSIENT.contains(&detail)),
    }
}

/// What std writes before `gai_strerror`'s text when a Unix lookup fails.
#[cfg(feature = "net")]
const LOOKUP_PREFIX: &str = "failed to lookup address information: ";

/// `gai_strerror(EAI_AGAIN)`: macOS 27 (measured) and glibc
/// (`gai_strerror-strs.h`) give the first, and musl (measured, Alpine 3.24.1)
/// the second. Every other text is a name that does not resolve.
#[cfg(feature = "net")]
const LOOKUP_TRANSIENT: [&str; 2] =
    ["Temporary failure in name resolution", "Try again"];

/// The Windows socket codes that answer a lookup: `WSAHOST_NOT_FOUND`,
/// `WSANO_RECOVERY` and `WSANO_DATA`. Any other code, `WSATRY_AGAIN` (11002)
/// among them, is an outage.
#[cfg(feature = "net")]
const WINSOCK_ANSWERS: [i32; 3] = [11001, 11003, 11004];

/// What `ureq` reported, as the class a caller branches on: `HostNotFound`
/// from [`relabel`]; `Io`, `ConnectionFailed` and `Timeout` as transport,
/// worth a retry; TLS, proxy, protocol, redirect and cookie failures as
/// `Other`, which a retry does not fix.
#[cfg(feature = "net")]
const fn classify(error: &ureq::Error) -> FetchFailure {
    use crate::FetchFailure;

    match error {
        ureq::Error::StatusCode(code) => FetchFailure::Status(*code),
        ureq::Error::HostNotFound => FetchFailure::HostNotFound,
        ureq::Error::BadUri(_) => FetchFailure::BadUrl,
        // Kept for the case `ureq` raises it directly, which it does when no
        // timeout is configured. This crate configures one, so the size case
        // is caught by counting bytes in `fetch` instead -- see the note
        // there.
        ureq::Error::BodyExceedsLimit(_) => FetchFailure::TooLarge,
        ureq::Error::Io(_)
        | ureq::Error::ConnectionFailed
        | ureq::Error::Timeout(_) => FetchFailure::Transport,
        _ => FetchFailure::Other,
    }
}

/// The prefix that makes a source string carry its own bytes rather than name
/// a place to read them from.
const DATA_URI: &str = "data:";

/// The base64 a `data:` URI carries. Padding is optional, since RFC 2397 does
/// not say, and whitespace is stripped first: a URI wrapped across lines is the
/// same URI.
const DATA_URI_BASE64: GeneralPurpose = GeneralPurpose::new(
    &base64::alphabet::STANDARD,
    GeneralPurposeConfig::new()
        .with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

/// Whether a source string carries its own bytes, asked of `Path` and `Url`
/// alike: a bare string is a path on both surfaces, and `{ url: "data:..." }`
/// must not reach the fetch either.
fn is_data_uri(source: &str) -> bool {
    source.starts_with(DATA_URI)
}

/// The bytes a `data:` URI carries, or what is wrong with it. The media type is
/// read and not trusted: the decoder sniffs, so `image/png` over JPEG bytes
/// renders a JPEG, as a browser does.
fn data_uri_bytes(uri: &str) -> Result<Vec<u8>, Error> {
    let body = uri.strip_prefix(DATA_URI).unwrap_or(uri);
    let Some((meta, payload)) = body.split_once(',') else {
        return Err(Error::DataUri {
            detail: format!(
                "{uri:.40?} has no comma; a data URI is \
                 data:[<media-type>][;base64],<payload>"
            ),
        });
    };

    if meta.trim_end().to_ascii_lowercase().ends_with(";base64") {
        let compact: Vec<u8> = payload
            .bytes()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect();
        return DATA_URI_BASE64.decode(&compact).map_err(|error| {
            Error::DataUri {
                detail: format!(
                    "it declares `;base64` and its payload is not valid \
                     base64: {error}"
                ),
            }
        });
    }

    percent_decode(payload)
}

/// The payload of a `data:` URI without `;base64`, strict about `%`: one not
/// followed by two hexadecimal digits is refused rather than passed through as
/// a literal.
fn percent_decode(payload: &str) -> Result<Vec<u8>, Error> {
    let bytes = payload.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%' {
            let hex = bytes.get(at + 1..at + 3).and_then(|pair| {
                std::str::from_utf8(pair)
                    .ok()
                    .and_then(|pair| u8::from_str_radix(pair, 16).ok())
            });
            let Some(byte) = hex else {
                return Err(Error::DataUri {
                    detail: format!(
                        "its payload has a `%` at {at} that is not followed by \
                         two hexadecimal digits"
                    ),
                });
            };
            out.push(byte);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    Ok(out)
}

/// What a worker hands back: a parsed SVG cannot cross a thread, `SkSVGDOM`
/// being neither `Send` nor `Sync`, so its bytes come back and the caller
/// parses them.
enum Fetched {
    /// Pixels, decoded on the worker.
    Raster(meo_skia_canvas::Image),
    /// Bytes that no raster decoder read and that look like an SVG document.
    Vector(Vec<u8>),
}

/// Whether these bytes look like an SVG document, asked only of bytes the
/// raster decoders refused, to tell "not an image" from "an SVG that will not
/// parse". A leading XML declaration or comment is skipped.
fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(1024)];
    let text = String::from_utf8_lossy(head);
    let text = text.trim_start();
    text.starts_with("<svg")
        || (text.starts_with("<?xml") || text.starts_with("<!--"))
            && text.contains("<svg")
}

fn decode(
    node: NodeId,
    source: &ImageSource,
    scene_http: &HttpOptions,
) -> Result<Fetched, Error> {
    // Read only by the fetch arm below, which this build does not compile.
    #[cfg(not(feature = "net"))]
    let _ = scene_http;

    // The `Path` arm owns what it read and the `Bytes` arm borrows, so only the
    // one that must allocate does: a `Vec` in both copies a 5 MB PNG per image
    // node.
    let read;
    let bytes: &[u8] = match source {
        ImageSource::Bytes(bytes) => bytes,
        // Before the filesystem, and before the fetch arms below: a `data:`
        // URI names no file and no host. It reached `std::fs::read` until
        // this arm existed, and failed as a missing file quoting a string that
        // was never a filename.
        ImageSource::Path(source) | ImageSource::Url { url: source, .. }
            if is_data_uri(source) =>
        {
            read = data_uri_bytes(source)?;
            &read
        }
        ImageSource::Path(path) => {
            read = std::fs::read(path).map_err(|source| Error::ImageRead {
                path: path.clone(),
                source,
            })?;
            &read
        }
        // The two arms are one decision spelt twice, and the second is why
        // the first is safe to add: with `net` off this is the refusal it has
        // always been, so a build that did not ask for an HTTP stack behaves
        // exactly as it did before the feature existed.
        #[cfg(feature = "net")]
        ImageSource::Url { url, http } => {
            // The source's own options over the scene's, one header name at a
            // time -- so a source naming one header keeps the scene's
            // credentials rather than replacing them. The same rule the npm
            // surface applies, so the two answer a shared question alike.
            read = fetch(url, &http.over(scene_http))?;
            &read
        }
        #[cfg(not(feature = "net"))]
        ImageSource::Url { .. } => {
            return Err(Error::UnresolvedSource(node));
        }
    };

    if let Ok(image) = meo_skia_canvas::Image::from_encoded(bytes) {
        return Ok(Fetched::Raster(image));
    }
    if looks_like_svg(bytes) {
        return Ok(Fetched::Vector(bytes.to_vec()));
    }
    Err(Error::UndecodableImage(node))
}

/// Parses what a worker handed back, on the thread that will draw it.
fn parsed(fetched: Fetched, node: NodeId) -> Result<DecodedImage, Error> {
    match fetched {
        Fetched::Raster(image) => Ok(DecodedImage {
            kind: Kind::Raster(image),
        }),
        Fetched::Vector(bytes) => {
            let xml = std::str::from_utf8(&bytes)
                .map_err(|_| Error::UnparsableSvg(node))?;
            let svg =
                Svg::parse(xml).map_err(|_| Error::UnparsableSvg(node))?;
            let size = svg.intrinsic_size();
            Ok(DecodedImage {
                kind: Kind::Vector(Rc::new(RefCell::new(Vector {
                    intrinsic: Size::new(size.width, size.height),
                    svg,
                    raster: None,
                }))),
            })
        }
    }
}

#[cfg(test)]
mod softening {
    use super::{ImageSource, NodeId, OnImageError, Scene, Size, soft};
    use crate::Error;

    fn url_scene() -> (Scene, ImageSource) {
        let mut scene = Scene::new(Size::new(8.0, 8.0));
        scene.on_image_error = OnImageError::Placeholder;
        (scene, ImageSource::url("http://example.invalid/x.png"))
    }

    /// The allowlist, asserted from both sides: the softenable set is named,
    /// and everything outside it stays an error even for a URL under a tolerant
    /// policy.
    #[test]
    fn only_the_named_failures_may_be_downgraded() {
        let (scene, source) = url_scene();
        let node = NodeId::ROOT;

        // Missing picture: softens.
        assert!(
            soft(
                &scene,
                node,
                &source,
                Error::SourceFetch {
                    url: "http://example.invalid/x.png".to_owned(),
                    detail: "404 Not Found".to_owned(),
                    failure: crate::FetchFailure::Status(404),
                },
                1,
            )
            .is_ok(),
            "a 404 on a URL is the broken-image case and should soften"
        );
        assert!(
            soft(&scene, node, &source, Error::UndecodableImage(node), 1)
                .is_ok(),
            "bytes a decoder refuses are a fact about the bytes"
        );

        // **We are not working: stays loud.** A decoder that panicked says
        // nothing about the bytes -- they may have been perfect -- and a grey
        // rectangle over our own crash is how a defect ships for a year.
        assert!(matches!(
            soft(&scene, node, &source, Error::DecoderPanicked(node), 1),
            Err(Error::DecoderPanicked(_))
        ));
        // A build that cannot fetch has not had a fetch fail; it has been
        // asked for something it does not do, and the message names the flag.
        // A build that cannot fetch has not had a fetch fail; it has been
        // asked for something it does not do, and the message names the flag.
        assert!(matches!(
            soft(&scene, node, &source, Error::UnresolvedSource(node), 1),
            Err(Error::UnresolvedSource(_))
        ));

        // **Unless a surface that fetches for itself says it tried.** Then the
        // same error is the broken-image case, and the reason it carries is
        // the one that surface measured rather than one synthesised here.
        let mut tried = scene.clone();
        tried
            .image_fetch_attempts
            .push(meo_canvas_scene::ImageFetchAttempt {
                url: "http://example.invalid/x.png".to_owned(),
                failure: meo_canvas_scene::ImageFetchFailure::Status(404),
                detail: "404 Not Found".to_owned(),
            });
        let softened =
            soft(&tried, node, &source, Error::UnresolvedSource(node), 1);
        assert!(
            matches!(
                softened,
                Ok(ref warning) if warning.failure == crate::FetchFailure::Status(404)
            ),
            "a recorded attempt should soften and keep its own reason: {softened:?}"
        );

        // And only for the URL that was actually tried.
        let other = ImageSource::url("http://example.invalid/y.png");
        assert!(matches!(
            soft(&tried, node, &other, Error::UnresolvedSource(node), 1),
            Err(Error::UnresolvedSource(_))
        ));
        assert!(matches!(
            soft(
                &scene,
                node,
                &source,
                Error::UnknownFont("Nope".to_owned()),
                1
            ),
            Err(Error::UnknownFont(_))
        ));
    }

    /// Neither of the caller's own inputs softens, whatever the policy says.
    #[test]
    fn a_path_and_bytes_are_never_downgraded() {
        let (scene, _) = url_scene();
        let node = NodeId::ROOT;
        for source in [
            ImageSource::Path("/no/such".to_owned()),
            ImageSource::Bytes(vec![0, 1, 2]),
        ] {
            assert!(
                soft(&scene, node, &source, Error::UndecodableImage(node), 1)
                    .is_err(),
                "{source:?} softened, and only a URL may"
            );
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use base64::Engine as _;
    use meo_canvas_scene::{
        OnImageError, Scene, Size,
        node::{ImageSource, Node, NodeId, NodeKind},
        style::{
            paint::Color,
            text::{FontWeight, TextStyle},
        },
    };

    use super::{
        DATA_URI_BASE64, DecodedImage, Fonts, LineHeight, Resolved,
        ResolvedText, is_local,
    };
    use crate::Error;

    /// Chrome, parent at 16px declaring and child at 32px inheriting: `1.5`
    /// gives 24 and 48, `24px` and `150%` 24 and 24. Only inheritance separates
    /// them, in opposite directions for a late percentage and an early number.
    fn inherited(declared: LineHeight) -> Option<LineHeight> {
        let parent = ResolvedText {
            size: 16.0,
            line_height: None,
            ..ResolvedText::initial()
        };
        let declaring = parent.inherit(&TextStyle {
            line_height: Some(declared),
            ..TextStyle::default()
        });
        let child = declaring.inherit(&TextStyle {
            font_size: Some(32.0),
            ..TextStyle::default()
        });
        child.line_height
    }

    /// The pixels a line box of that height comes to at `size`.
    fn pixels(height: Option<LineHeight>, size: f32) -> Option<f32> {
        crate::lines::Metrics::of(&ResolvedText {
            size,
            line_height: height,
            ..ResolvedText::initial()
        })
        .line_height
    }

    #[test]
    fn a_percentage_resolves_where_it_is_declared() {
        // 150% of the DECLARING element's 16px is 24, and 24 is what
        // descends. Chrome: 24, not 48.
        assert_eq!(
            inherited(LineHeight::Percent(1.5)),
            Some(LineHeight::Length(24.0))
        );
        assert_eq!(
            pixels(inherited(LineHeight::Percent(1.5)), 32.0),
            Some(24.0)
        );
    }

    #[test]
    fn a_number_is_recomputed_by_whoever_inherits_it() {
        // The number descends as a number, so the 32px child gets 48 rather
        // than the 24 the parent would have had.
        assert_eq!(
            inherited(LineHeight::Number(1.5)),
            Some(LineHeight::Number(1.5))
        );
        assert_eq!(
            pixels(inherited(LineHeight::Number(1.5)), 32.0),
            Some(48.0)
        );
    }

    #[test]
    fn a_length_descends_unchanged() {
        assert_eq!(
            inherited(LineHeight::Length(24.0)),
            Some(LineHeight::Length(24.0))
        );
        assert_eq!(
            pixels(inherited(LineHeight::Length(24.0)), 32.0),
            Some(24.0)
        );
    }

    #[test]
    fn the_declared_column_cannot_tell_the_three_apart() {
        // **The control, and the reason the tests above read the child.** All
        // three are 24 at the element that declares them, so a suite that
        // stopped here would pass with the percentage and the number resolved
        // at either end.
        for declared in [
            LineHeight::Number(1.5),
            LineHeight::Length(24.0),
            LineHeight::Percent(1.5),
        ] {
            let resolved = ResolvedText {
                size: 16.0,
                ..ResolvedText::initial()
            }
            .inherit(&TextStyle {
                line_height: Some(declared),
                ..TextStyle::default()
            });
            assert_eq!(
                pixels(resolved.line_height, 16.0),
                Some(24.0),
                "{declared:?} is not 24 at the element that declares it"
            );
        }
    }

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

    /// A URL that fails without leaving the machine: port 1 on the loopback
    /// refuses at once, with no DNS lookup, where even a reserved hostname asks
    /// the resolver.
    const UNREACHABLE: &str = "http://127.0.0.1:1/image.png";

    /// Asserts a scene naming a URL is refused as this build refuses it:
    /// [`Error::UnresolvedSource`] without `net`. With it the fetch fails, and
    /// the scene's default `Placeholder` policy softens that into one warning
    /// for the URL; [`Error::SourceFetch`] is accepted for a stricter policy.
    fn assert_url_is_refused(scene: &Scene, node: Option<NodeId>) {
        let result = Resolved::new(scene, &Fonts::new());
        #[cfg(not(feature = "net"))]
        match (result, node) {
            (Err(Error::UnresolvedSource(id)), Some(want)) => {
                assert_eq!(id, want, "the refusal names the wrong node");
            }
            (Err(Error::UnresolvedSource(_)), None) => {}
            (other, _) => {
                unreachable!("a URL should be unresolved here, got {other:?}")
            }
        }
        #[cfg(feature = "net")]
        match result {
            Ok(resolved) => {
                let warnings = resolved.into_warnings();
                assert!(
                    matches!(
                        warnings.as_slice(),
                        [warning] if warning.url == UNREACHABLE
                            && node.is_none_or(|want| warning.node == want)
                    ),
                    "a URL should have been fetched, failed and softened into \
                     one warning, got {warnings:?}"
                );
            }
            Err(Error::SourceFetch { .. }) => {}
            other => {
                unreachable!("a URL should have been fetched, got {other:?}")
            }
        }
    }

    #[test]
    fn a_url_is_refused_because_the_core_does_not_fetch() {
        assert!(is_local(&ImageSource::Path("a".to_owned())));
        assert!(is_local(&ImageSource::Bytes(Vec::new())));
        assert!(!is_local(&ImageSource::url("https://a.test")));

        let mut scene = Scene::new(Size::ZERO);
        let node = scene
            .push(NodeId::ROOT, image_node(ImageSource::url(UNREACHABLE)))
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_url_is_refused(&scene, Some(node));
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

    /// The same 4x2 red PNG as a `data:` URI, base64 or percent-encoded, built
    /// from `RED_PNG` so the payload cannot drift from the bytes it should
    /// carry.
    fn red_png_data_uri(base64: bool) -> String {
        if base64 {
            format!("data:image/png;base64,{}", DATA_URI_BASE64.encode(RED_PNG))
        } else {
            // Hex by hand rather than through `format!` per byte: the
            // lint against building a string from formatted pieces is
            // right, and two table lookups are clearer than the escape
            // hatch would be.
            const HEX: &[u8; 16] = b"0123456789ABCDEF";
            let mut out = String::from("data:image/png,");
            for byte in RED_PNG {
                out.push('%');
                out.push(char::from(HEX[usize::from(byte >> 4)]));
                out.push(char::from(HEX[usize::from(byte & 0x0F)]));
            }
            out
        }
    }

    fn decoded_size(source: ImageSource) -> Result<Size, Error> {
        let mut scene = Scene::new(Size::ZERO);
        let node = scene
            .push(NodeId::ROOT, image_node(source))
            .unwrap_or_else(|error| unreachable!("{error}"));
        Resolved::new(&scene, &Fonts::new()).map(|resolved| {
            resolved
                .image(node)
                .unwrap_or_else(|| unreachable!("the node is an image"))
                .intrinsic_size()
        })
    }

    #[test]
    fn a_data_uri_carries_its_own_bytes_in_either_encoding() {
        // Both encodings, and both wrappers: a bare string is a `Path` on both
        // public surfaces, and `{ url: "data:..." }` is the same statement in
        // a different one. The four have to agree, because the difference
        // between them is spelling rather than meaning.
        for (name, source) in [
            ("base64 path", ImageSource::Path(red_png_data_uri(true))),
            ("base64 url", ImageSource::url(red_png_data_uri(true))),
            ("percent path", ImageSource::Path(red_png_data_uri(false))),
            ("percent url", ImageSource::url(red_png_data_uri(false))),
        ] {
            assert_eq!(
                decoded_size(source).ok(),
                Some(Size::new(4.0, 2.0)),
                "{name} did not decode to the picture it carries"
            );
        }
    }

    #[test]
    fn a_data_uri_that_is_not_one_says_what_the_form_takes() {
        let refused = |uri: &str| {
            let error = decoded_size(ImageSource::Path(uri.to_owned()))
                .err()
                .unwrap_or_else(|| unreachable!("{uri} should not decode"));
            let text = error.to_string();
            // Never a filesystem error quoting something that is not a
            // filename, which is what this issue was.
            assert!(
                !text.contains("cannot read image at"),
                "{uri} was reported as a file: {text}"
            );
            text
        };

        assert!(
            refused("data:image/png;base64").contains("has no comma"),
            "a data URI with no comma should say so"
        );
        assert!(
            refused("data:image/png;base64,not base64!!").contains("base64"),
            "a bad base64 payload should name base64"
        );
        assert!(
            refused("data:image/png,%ZZ").contains("hexadecimal"),
            "a bad escape should say what a `%` takes"
        );
        // Decodes, and is not a picture: that is the existing variant, because
        // by then it is bytes like any other.
        assert!(
            matches!(
                decoded_size(ImageSource::Path(
                    "data:text/plain;base64,aGVsbG8=".to_owned()
                )),
                Err(Error::UndecodableImage(_))
            ),
            "bytes that are not an image should be the undecodable case"
        );
    }

    /// A 40x20 document that states its size, and the same drawing with only a
    /// `viewBox`, both in `currentColor`: untinted, SVG's initial `color` is
    /// black, which these assertions see.
    const SIZED_SVG: &str = concat!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" "#,
        r#"viewBox="0 0 40 20"><rect width="40" height="20" "#,
        r#"fill="currentColor"/></svg>"#
    );
    const AUTOSIZED_SVG: &str = concat!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 20">"#,
        r#"<rect width="40" height="20" fill="currentColor"/></svg>"#
    );

    fn svg_source(xml: &str) -> ImageSource {
        ImageSource::Bytes(xml.as_bytes().to_vec())
    }

    fn decoded(source: ImageSource) -> Result<DecodedImage, Error> {
        let mut scene = Scene::new(Size::ZERO);
        let node = scene
            .push(NodeId::ROOT, image_node(source))
            .unwrap_or_else(|error| unreachable!("{error}"));
        Resolved::new(&scene, &Fonts::new()).map(|resolved| {
            resolved
                .image(node)
                .unwrap_or_else(|| unreachable!("the node is an image"))
                .clone()
        })
    }

    #[test]
    fn an_svg_source_reports_the_size_the_document_states() {
        let sized = decoded(svg_source(SIZED_SVG))
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(sized.intrinsic_size(), Size::new(40.0, 20.0));

        // A document with no stated size still has an extent, derived from its
        // `viewBox`. Layout has to be given a number either way, and the pair
        // is what says the sized one is not answering by accident.
        let autosized = decoded(svg_source(AUTOSIZED_SVG))
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(
            autosized.intrinsic_size().width > 0.0,
            "an autosized document reported no width"
        );
    }

    #[test]
    fn an_svg_is_rasterised_at_the_size_it_is_drawn() {
        // Two rasterisations rather than one stretched, so the pixels differ in
        // count as well as scale: a renderer rasterising at 40 and drawing at
        // 200 fails.
        let image = decoded(svg_source(SIZED_SVG))
            .unwrap_or_else(|error| unreachable!("{error}"));
        let small = image
            .raster((40, 20), None, NodeId::ROOT)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let large = image
            .raster((200, 100), None, NodeId::ROOT)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!((small.width(), small.height()), (40, 20));
        assert_eq!((large.width(), large.height()), (200, 100));

        // And the same size twice is the memo, which must hand back the same
        // pixels rather than a second rasterisation of them.
        let again = image
            .raster((200, 100), None, NodeId::ROOT)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!((again.width(), again.height()), (200, 100));
    }

    #[test]
    fn a_raster_source_ignores_the_size_it_is_asked_for() {
        // The other arm of `raster`: pixels that came from a file are what the
        // file carried, whatever size the drawing call wants. Without this the
        // vector row above would pass for an implementation that resized
        // everything.
        let image = decoded(ImageSource::Bytes(RED_PNG.to_vec()))
            .unwrap_or_else(|error| unreachable!("{error}"));
        let asked = image
            .raster((200, 100), None, NodeId::ROOT)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!((asked.width(), asked.height()), (4, 2));
    }

    #[test]
    fn an_svg_that_will_not_parse_says_it_was_an_svg() {
        // The sniff's whole purpose: these bytes were refused by every raster
        // decoder *and* looked like a document, so the caller hears about the
        // document rather than about the decoders.
        let broken = "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect";
        assert!(
            matches!(decoded(svg_source(broken)), Err(Error::UnparsableSvg(_))),
            "a malformed document did not fail as an SVG"
        );
        // And bytes that look like nothing stay the other variant.
        assert!(
            matches!(
                decoded(ImageSource::Bytes(b"not a picture".to_vec())),
                Err(Error::UndecodableImage(_))
            ),
            "bytes that are not a document should not fail as an SVG"
        );
    }

    #[test]
    fn a_document_has_one_frame() {
        // A frame index past the only frame is refused rather than answered
        // with that frame, which is the rule the raster arm already has for a
        // two-frame GIF asked for its fourth.
        let mut scene = Scene::new(Size::ZERO);
        let mut node = image_node(svg_source(SIZED_SVG));
        if let NodeKind::Image { frame, .. } = &mut node.kind {
            *frame = Some(3);
        }
        scene
            .push(NodeId::ROOT, node)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(
            matches!(
                Resolved::new(&scene, &Fonts::new()),
                Err(Error::UndecodableImage(_))
            ),
            "a document answered for a frame it does not have"
        );
    }

    #[test]
    fn a_path_with_a_comma_in_it_is_still_a_path() {
        // A predicate written as "contains a comma" is right for every data URI
        // anyone types and wrong for `/tmp/logo,v2.png`, a filename people
        // write.
        let error =
            decoded_size(ImageSource::Path("/nope,comma.png".to_owned()))
                .err()
                .unwrap_or_else(|| unreachable!("that path does not exist"));
        assert!(
            matches!(error, Error::ImageRead { .. }),
            "a path with a comma was classified as something else: {error}"
        );
    }

    #[test]
    fn a_data_uris_media_type_is_read_and_not_trusted() {
        // A PNG announced as a JPEG still renders, as it does in a browser:
        // the decoder sniffs the bytes it is given. Refusing on the strength
        // of the label would reject working input for a caller's typo.
        let uri = red_png_data_uri(true).replace("image/png", "image/jpeg");
        assert_eq!(
            decoded_size(ImageSource::Path(uri)).ok(),
            Some(Size::new(4.0, 2.0))
        );
    }

    #[test]
    fn a_data_uri_is_never_softened_by_the_image_error_policy() {
        // The wrapper alone must not decide. `{ url }` softens a 404 into a
        // placeholder; a `data:` URI in the same wrapper was never fetched, so
        // it stays an error under both policies -- otherwise the identical
        // payload would soften as `{ url }` and throw as a bare string.
        for source in [
            ImageSource::url("data:image/png;base64,!!!"),
            ImageSource::url("data:text/plain;base64,aGVsbG8="),
        ] {
            let mut scene = Scene::new(Size::ZERO);
            scene.on_image_error = OnImageError::Placeholder;
            scene
                .push(NodeId::ROOT, image_node(source))
                .unwrap_or_else(|error| unreachable!("{error}"));
            assert!(
                Resolved::new(&scene, &Fonts::new()).is_err(),
                "a data: URI softened into a placeholder"
            );
        }
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
                source: ImageSource::url(UNREACHABLE),
                repeat:
                    meo_canvas_scene::style::paint::BackgroundRepeat::Repeat,
                size: meo_canvas_scene::style::paint::BackgroundSize::AUTO,
                position: (
                    meo_canvas_scene::Length::ZERO,
                    meo_canvas_scene::Length::ZERO,
                ),
            });
        assert_url_is_refused(&scene, None);
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
        assert!(initial.line_height.is_none());
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
