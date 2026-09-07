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
//! [`Error::UnresolvedSource`], exactly as it always was, and no HTTP stack is
//! linked. With it on, the URL is fetched over a blocking client.
//!
//! The policy that used to keep fetching out of this crate was about
//! **runtimes**, not about the network: an async client would put a runtime in
//! every Rust consumer of the public crate, including those already inside one.
//! A blocking client puts none, so the objection does not reach it. The
//! dependency is still real, which is why the feature is off unless asked for.
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
    node::{ImageSource, NodeId, NodeKind},
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
    /// box exactly one em tall is legal CSS and is not `normal`; carrying the
    /// two as one `f32` made them the same number, and every
    /// `line-height: 1` in a document silently became the face's metrics.
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
            // `.or`, not `unwrap_or`: the latter turns an absent value
            // into the number `1.0`, and from there an explicit `1.0` and an
            // inherited `normal` are indistinguishable.
            //
            // **A percentage resolves here and a number does not**, which is
            // the whole of CSS's rule and the one asymmetry in this merge. A
            // percentage is a share of the declaring element's own size, so
            // the length it becomes is what descends; a number descends as a
            // number and is recomputed against each inheritor's size.
            //
            // Both mistakes pass every test that only declares: resolve a
            // percentage late and a 32px child reads 48 where Chrome says 24;
            // resolve a number early and the same child reads 24 where Chrome
            // says 48.
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
    /// A parsed document, shared by every node that names this source.
    ///
    /// `Rc` rather than a fresh document per node: `taken` clones one decode
    /// per node, and a document is expensive to parse and cheap to share.
    /// `RefCell` because [`meo_skia_canvas::Svg::rasterize`] takes `&mut
    /// self` -- it sets the container size on the document before drawing --
    /// and because the raster it produces is memoised beside it.
    Vector(Rc<RefCell<Vector>>),
}

/// A parsed SVG document and the last raster made from it.
struct Vector {
    /// The document itself.
    svg: Svg,
    /// Its own size, which is what an `Auto` box takes.
    ///
    /// Read once at parse time rather than asked of the document each time:
    /// the layout pass asks for it twice per node even when the caller states
    /// both dimensions, measured on a scene with an explicit width and
    /// height.
    intrinsic: Size,
    /// The last size and colour this document was rasterised for, and the
    /// pixels.
    ///
    /// One entry rather than a map. A document is normally drawn once, and
    /// re-rasterising is tens of microseconds against a parse of tens of
    /// milliseconds -- so a second size or a second tint costs a redraw rather
    /// than a reparse, and a map would be machinery for a case nobody has.
    ///
    /// **Keyed by the tint as well as the size**, because the tint belongs to
    /// this drawing of the document rather than to the document: one decode is
    /// shared by every node naming the source, so two nodes drawing the same
    /// star in two colours would otherwise be one tinted document and the
    /// second colour would lose.
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
    /// Pixels for the paint pass, at the size they will be drawn.
    ///
    /// A raster source ignores the size -- its pixels are what the file
    /// carried, and scaling them is the drawing call's business. A vector
    /// source is rasterised at exactly this size, which is the whole reason
    /// the document is kept rather than turned into pixels at decode time.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UndecodableImage`] naming the node when a document
    /// cannot be rasterised at the size asked for.
    pub(crate) fn raster(
        &self,
        size: (u32, u32),
        tint: Option<Color>,
        node: NodeId,
    ) -> Result<meo_skia_canvas::Image, Error> {
        match &self.kind {
            // **A colour has no reading on a bitmap, so it is refused rather
            // than ignored.** The check is here and not on either writer
            // because neither can tell: a writer sees a filename or a URL for
            // two of the three source forms, and sniffing there as well as
            // here would be two spellings of one rule. The cost is that a
            // caller learns at render time, which is when the information
            // exists.
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
                // **Set before rasterising, and only when asked.** Calling
                // with a default instead of not calling would make every
                // future change to that default silently ours rather than the
                // document's -- and the two are the same picture, so nothing
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

    /// The frame a node asked for, or this image unchanged.
    ///
    /// **An animated source drew its first frame whatever the scene said**:
    /// `NodeKind::Image::frame` crossed the wire, reached here, and nothing
    /// ever read it. A one-frame source ignores the index rather than
    /// refusing it, since asking for frame zero of a still picture is what
    /// every unanimated node does.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UndecodableImage`] naming the node when the index is
    /// past the last frame -- a scene asking for the fourth frame of a
    /// two-frame source has said something the source cannot answer, and
    /// drawing the first instead would be the silent wrong picture this whole
    /// property was.
    fn at_frame(self, frame: Option<u32>, node: NodeId) -> Result<Self, Error> {
        let Some(index) = frame.map(|index| index as usize) else {
            return Ok(self);
        };
        // **A document has one frame, and asking for a later one is the same
        // mistake as asking for the fourth frame of a two-frame GIF.** SVG
        // animation is not rasterised here, so saying yes by drawing the only
        // frame there is would be the silent wrong picture this property
        // exists to refuse.
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
                // `at_frame` is applied per node and not shared: it is
                // what a node asked of the source rather than part of
                // decoding it, so two nodes may hold one decode and
                // still want different frames of it.
                // Absent rather than inserted when the source softened, so
                // `Resolved::image` answers `None` and both the measurer and
                // the painter take the arm they already have for an image they
                // were never given. No new branch runs for one that resolved.
                if let Some(decoded) = taken(&once, &softened, id, source)? {
                    resolved.images.insert(id, decoded.at_frame(*frame, id)?);
                }
            }
            if let Some(background) = node.paint.background_image.as_ref() {
                // Kept in its own table rather than beside the image nodes: a
                // background is drawn into a box layout has already sized, so
                // its extent is not a layout input the way an image node's is,
                // and the measure pass must not find it by asking for a node's
                // image.
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

/// Every source in a scene, decoded once each and in parallel.
///
/// **The three tables ask for the same thing.** An image node, a background
/// and a mask each hand `decode` a source and get a bitmap back, and nothing
/// about the use site changes what comes out -- no scale, no colour type, no
/// rasterisation size. So the source is the whole key, and a picture wanted
/// sixty times is decoded once.
///
/// The frame index is the one thing that varies, and it is **not** part of the
/// key: [`DecodedImage::at_frame`] derives a frame from a decode that already
/// happened, so it is applied per node afterwards. `shared_decode.rs` asserts
/// that two nodes sharing a source keep their own frames, through the renderer.
///
/// # Why the walk happens twice
///
/// The first walk finds what is distinct and the second fills the tables. That
/// costs a pass over the arena and buys the decodes being independent of each
/// other, which is what lets them run at once.
///
/// # The error a scene gets
///
/// The **first failing source in node order**, not the first thread to finish.
/// Decoding concurrently must not make which error a caller sees depend on
/// scheduling, and the node named is the first that asked for those bytes.
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
    let decoded: Vec<Result<DecodedImage, Error>> = in_parallel(&wanted)
        .into_iter()
        .zip(wanted.iter())
        .map(|(fetched, (_, node))| fetched.and_then(|it| parsed(it, *node)))
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
            // **Only a `Url` may soften, and only when the scene asked.** A
            // `Path` that cannot be read and `Bytes` that will not decode are
            // the caller's own input, checkable before rendering, so they stay
            // errors whatever the policy says -- and softening them would make
            // this silent path reachable with no network in it, turning any
            // future defect in our own decoders into a missing picture rather
            // than a failure.
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

/// Turns a failed source into a warning, or gives the error back.
///
/// **Two outcomes, and the type says so.** This returned
/// `Result<Option<_>, _>` once, with a `None` no arm could produce -- and the
/// caller answered it with a `continue`, which would have dropped a failed
/// source silently the first time somebody added an arm that returned it. A
/// type admitting a state its function cannot reach is the caller's problem
/// tomorrow.
///
/// Softening is decided here rather than at the call site so that the rule --
/// which source, under which policy -- lives in one place.
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
    let ImageSource::Url(url) = source else {
        return Err(error);
    };
    // **A `data:` URI in a `Url` wrapper is still the caller's own bytes.**
    // Nothing was fetched, so there is no 404 to draw a placeholder for, and
    // the rule the arms below rest on -- that what came from the world may
    // soften and what came from the caller may not -- puts it with `Bytes`.
    // Without this the wrapper alone would decide, and the same payload would
    // soften as `{ url }` and throw as a bare string.
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
        // `UnresolvedSource` is the `net` feature being off, which is a build
        // decision rather than a fact about the world: a caller who did not
        // compile an HTTP client has not had a fetch fail, they have asked for
        // something this build cannot do.
        // **`UnresolvedSource` softens only when somebody else already tried.**
        // On its own it is the `net` feature being off -- a build decision
        // rather than a fact about the world, and a caller who did not compile
        // an HTTP client has not had a fetch fail, they have asked for
        // something this build cannot do. That must keep naming the flag.
        //
        // The npm surface resolves URLs in JavaScript, so a URL it could not
        // fetch arrives here unresolved and looks identical.
        // `image_fetch_attempts` is how that surface says otherwise, and it
        // carries the reason so one real 404 produces the same warning on both
        // public surfaces rather than a vaguer one here.
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

        // **Everything else stays loud, and the list above is the whole of
        // what may soften.** A 404, a reset connection, a body past the limit
        // and bytes a decoder refuses are all "the picture is missing", which
        // is the case a placeholder is for. An allocation failure, a decoder
        // that panicked, a font that will not resolve, a broken invariant in
        // this crate -- those are "we are not working", and drawing a neat
        // grey rectangle over one is how a defect ships silently. No
        // catch-all: a new `Error` variant is loud until somebody decides
        // otherwise here, deliberately.
        other => Err(other),
    }
}

/// Decodes a list of sources across the machine's threads, in order.
///
/// One thread per source up to the machine's parallelism, and the results come
/// back in the order they were asked for so the caller's error is the first in
/// node order rather than the first to fail.
///
/// **No number is claimed for this.** Decode is Skia's, and whether it is
/// bound by the CPU or by a lock inside the decoder is not something the shape
/// of this function establishes. The argument for it is that the decodes are
/// independent and were serial; the measurement is owed and is not here.
fn in_parallel(
    wanted: &[(&ImageSource, NodeId)],
) -> Vec<Result<Fetched, Error>> {
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(wanted.len().max(1));
    if threads <= 1 || wanted.len() <= 1 {
        return wanted
            .iter()
            .map(|(source, id)| decode(*id, source))
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
                        .map(|(source, id)| decode(*id, source))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for handle in handles {
            match handle.join() {
                Ok(part) => out.extend(part),
                // A decoder that panicked is a bug in the decoder, and the
                // scene still has to answer. The source is named through the
                // node that asked for it.
                //
                // **Its own variant, so `soft` cannot downgrade it.** This
                // used to be `UndecodableImage`, which is on the softenable
                // list -- so once a URL could soften, a panicking decoder
                // became a placeholder and the crash was a grey rectangle.
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
        // Absent for one of two reasons, and they are not the same.
        //
        // **Decided from what happened, not from what the source is.** A
        // source `decode_sources` softened is deliberately missing, which is
        // how a failed node reaches layout and paint as "no image" through the
        // `Option` both already match on. Anything else absent is this
        // function's own invariant broken -- every source in the scene was
        // asked for -- and stays an error.
        //
        // Reading the source's *type* instead would have been the same
        // silence this feature exists to prevent: a fourth image-bearing site
        // added later and missed by `want` would be an error for a path and a
        // blank for a URL, on a card that then looks finished.
        None if softened.contains(source) => Ok(None),
        None => Err(Error::UndecodableImage(node)),
    }
}

/// Reads a URL source over HTTP, blocking until it has the bytes.
///
/// **Blocking on purpose, and it is the whole reason this crate may have an
/// HTTP client at all.** The pipeline is a function from bytes to bytes, called
/// from whatever thread the consumer has; an async client would put a runtime
/// in every Rust consumer of the public crate, including those already inside
/// one. `ureq` is blocking by construction and brings no runtime -- audited by
/// walking its tree rather than by reading the note beside it, which lists a
/// smaller set than it pulls.
///
/// # The policy, which is this crate's rather than the client's
///
/// **Five seconds to connect and thirty seconds for everything.** Until 5
/// September 2026 no timeout was set at all: `ureq` 3.4 defaults every field of
/// `Timeouts` to `None` except `await_100`, which needs a request body and so
/// cannot fire for the bodiless `GET` this makes. A host that accepted a
/// connection and then said nothing held a render thread until the process
/// died.
///
/// `global` rather than a set of per-phase timeouts, because **only the global
/// clock bounds the thread**: a host dripping one byte inside every window
/// keeps `recv_body` alive forever. `Timeout::Global` is checked at every phase
/// including `RecvBody` and its clock starts when the call is created, so it
/// spans `read_to_vec` below -- which matters, since a timeout that stopped at
/// `.call()` would leave the hang exactly where it was and look like a fix.
///
/// The numbers are derived rather than chosen. Five seconds is about sixteen
/// worst-case intercontinental round trips, and a host that cannot complete a
/// handshake in that will not deliver an image. Thirty seconds against the
/// ten-mebibyte ceiling below is a claim that a host sustains **about
/// 2.8 Mbit/s**, which is the honest way to state it: disagree with the number
/// by disagreeing with the floor. It is also about 1,300 times a whole render,
/// so it is not competing with a legitimate slow case -- a request whose image
/// fetch took thirty seconds has already missed its own deadline.
///
/// **Thirty-two mebibytes, set here rather than inherited.** `ureq`'s own
/// `MAX_BODY_SIZE` is ten, and taking it meant this crate's size policy was
/// whatever a dependency happened to choose and free to move under a version
/// bump. It is a *functional* limit and not only a safety one: an image larger
/// than this named by a URL does not render, and the caller gets
/// [`FetchFailure::TooLarge`], which says so in this crate's own words.
///
/// **The number is arithmetic, not a measurement, and that is worth knowing
/// before trusting it.** There is no photographic corpus in this repository to
/// weigh -- measuring what *this* library emits gives 0.37 MiB for a 90-frame
/// 720p WebP, which is a fact about flat vector content and says nothing about
/// someone else's screen recording. So: GIF is the least efficient animated
/// format and the one most often linked, a palettised LZW frame runs about one
/// bit per pixel on photographic content, and 1280x720 at 30 fps is therefore
/// roughly 115 KB a frame -- **three seconds of 720p GIF is about 10.4 MB**,
/// which the inherited ten-mebibyte cap refused. Thirty-two buys about nine
/// seconds of that, or three at 1080p, and animated WebP and AVIF are five to
/// twenty times denser so anything that fits GIF fits them.
///
/// # Fixed, not configurable
///
/// A caller wanting a policy has the same escape the TypeScript surface has:
/// fetch the bytes themselves and pass `ImageSource::Bytes`. Making these
/// configurable would start this crate down the road of being an HTTP client --
/// then redirects, proxies, headers, TLS -- and, more directly, **a
/// configurable timeout can be set to infinity, which is this defect with a
/// supported spelling.**
///
/// What is still `ureq`'s: ten redirects then an error, and a 64 KiB cap on
/// response headers.
#[cfg(feature = "net")]
fn fetch(url: &str) -> Result<Vec<u8>, Error> {
    use std::io::Read as _;

    let refuse = |error: ureq::Error| Error::SourceFetch {
        url: url.to_owned(),
        detail: error.to_string(),
        failure: classify(&error),
    };

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_global(Some(GLOBAL_TIMEOUT))
        .build()
        .into();

    let mut response = agent.get(url).call().map_err(refuse)?;

    // **The size policy is enforced here rather than by the client**, and that
    // is not tidiness. `ureq`'s own `limit` reports `BodyExceedsLimit` when no
    // timeout is configured and a bare `Io(Os { code: 22, InvalidInput })`
    // when `timeout_global` is -- measured, both ways, against the same 33 MiB
    // response. So a classification resting on its error variant would have
    // been correct in a test and wrong in this crate, which configures a
    // timeout. Counting the bytes ourselves makes the answer ours in every
    // configuration.
    //
    // One byte past the limit is read so that "exactly at the limit" is
    // accepted and "one more" is not, without a `Content-Length` the server
    // may not send or may lie about.
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

/// How long a connection may take to establish.
///
/// About sixteen worst-case intercontinental round trips. See [`fetch`].
#[cfg(feature = "net")]
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// How long the whole fetch may take, connection and body together.
///
/// Sixty seconds against [`MAX_IMAGE_BYTES`] is a floor of about 4.5 Mbit/s.
/// The two are one decision; see [`fetch`].
#[cfg(feature = "net")]
const GLOBAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// The largest image this crate will fetch over HTTP.
///
/// Thirty-two mebibytes, chosen here rather than inherited from `ureq`, so it
/// does not move when a dependency does. A functional limit, not only a safety
/// one. See [`fetch`] for the arithmetic and for why it is arithmetic.
#[cfg(feature = "net")]
const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;

/// What `ureq` reported, as the class a caller branches on.
///
/// **Mapped from the variants that are certain, and `Other` for the rest.**
/// `StatusCode` is the default behaviour for 4xx and 5xx, `HostNotFound` is
/// resolution, `BadUri` is a URL with no scheme or host. `Io` and
/// `ConnectionFailed` are both the transport, and `ConnectionFailed` is
/// `ureq`'s own fallback for a connector that gave no reason -- retrying is
/// still the right first move for either.
///
/// `Timeout` is folded into the transport rather than named, because the
/// configuration this crate uses cannot produce it: `ureq` 3.4 defaults every
/// timeout to `None` except `await_100`, which needs a request body. The arm
/// is here so the mapping stays right if a timeout is ever configured.
///
/// Everything else -- TLS, proxy, protocol, redirects, cookies -- is `Other`.
/// They have nothing in common except that repeating the request does not fix
/// them, which is what `Other` tells a caller.
#[cfg(all(test, feature = "net"))]
mod fetch_classification {
    use super::classify;
    use crate::FetchFailure;

    #[test]
    fn a_url_with_no_scheme_is_the_callers_to_fix_and_not_to_retry() {
        // `BadUri` is the one class a caller can act on without a network at
        // all, and the only one this test can reach without one.
        let Err(refused) = ureq::get("not-a-url").call() else {
            unreachable!("a URL with no scheme is refused")
        };
        assert_eq!(classify(&refused), FetchFailure::BadUrl);
    }

    #[test]
    fn a_host_that_does_not_resolve_is_not_a_transport_failure() {
        // Separating these two is the point of the classification: a name that
        // does not resolve will not resolve on a retry, and a socket that
        // dropped may well connect on one.
        let Err(refused) = ureq::get("http://invalid.invalid/a.png").call()
        else {
            unreachable!("the reserved TLD does not resolve")
        };
        assert!(
            matches!(
                classify(&refused),
                FetchFailure::HostNotFound | FetchFailure::Transport
            ),
            "a resolution failure came back as {:?}",
            classify(&refused)
        );
    }
}

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

/// The base64 a `data:` URI carries.
///
/// Padding is **indifferent** rather than required: RFC 2397 does not say, and
/// a caller pasting from a tool that trims `=` is not making a different
/// statement about the bytes. Whitespace is stripped before decoding for the
/// same reason -- a URI wrapped across lines in a source file is the same URI.
const DATA_URI_BASE64: GeneralPurpose = GeneralPurpose::new(
    &base64::alphabet::STANDARD,
    GeneralPurposeConfig::new()
        .with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

/// Whether a source string carries its own bytes.
///
/// **Asked of `Path` and of `Url` alike.** A bare string is a path on both
/// public surfaces, so this is where a `data:` URI arrives; and
/// `{ url: "data:..." }` is the same statement in a different wrapper, which
/// must not reach the fetch machinery either.
fn is_data_uri(source: &str) -> bool {
    source.starts_with(DATA_URI)
}

/// The bytes a `data:` URI carries, or what is wrong with it.
///
/// `data:[<media-type>][;base64],<payload>`. **The media type is read and not
/// trusted**: the decoder that receives these bytes sniffs them, so a caller
/// who writes `image/png` over JPEG bytes renders a JPEG, as a browser does.
/// Trusting it would refuse working input on the strength of a label.
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

/// The payload of a `data:` URI that did not declare `;base64`.
///
/// **Strict about `%`**, where a browser's own leniency varies: a `%` that is
/// not followed by two hexadecimal digits is a payload the caller did not mean
/// to write, and passing it through as a literal renders bytes nobody asked
/// for rather than saying so.
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

/// What a worker can hand back, which is not always a decoded image.
///
/// **A parsed SVG cannot cross a thread** -- `meo_skia_canvas::Svg` wraps
/// `SkSVGDOM`, which is neither `Send` nor `Sync` -- so the bytes come back
/// instead and the caller parses them. Bytes are `Send`, and reading the file
/// stays where the parallelism is.
enum Fetched {
    /// Pixels, decoded on the worker.
    Raster(meo_skia_canvas::Image),
    /// Bytes that no raster decoder read and that look like an SVG document.
    Vector(Vec<u8>),
}

/// Whether these bytes look like an SVG document.
///
/// **A gate in front of the parser rather than the thing that decides.** The
/// raster decoders are asked first, so this only ever sees bytes they refused;
/// what it does is separate "not an image at all" from "an SVG that will not
/// parse", which are different sentences for the caller. A file that starts
/// with an XML declaration or a comment before its root is still an SVG, so
/// the leading bytes are skipped rather than matched exactly.
fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(1024)];
    let text = String::from_utf8_lossy(head);
    let text = text.trim_start();
    text.starts_with("<svg")
        || (text.starts_with("<?xml") || text.starts_with("<!--"))
            && text.contains("<svg")
}

fn decode(node: NodeId, source: &ImageSource) -> Result<Fetched, Error> {
    // The `Path` arm owns what it read and the `Bytes` arm borrows what the
    // caller already holds, so only the one that has to allocate does. Making
    // both arms `Vec<u8>` reads more evenly and copies the whole file: a 5 MB
    // PNG already in the scene was copied 5 MB per image node and then dropped
    // unread, because `Image::from_encoded` takes a slice either way.
    let read;
    let bytes: &[u8] = match source {
        ImageSource::Bytes(bytes) => bytes,
        // Before the filesystem, and before the fetch arms below: a `data:`
        // URI names no file and no host. It reached `std::fs::read` until
        // this arm existed, and failed as a missing file quoting a string that
        // was never a filename.
        ImageSource::Path(source) | ImageSource::Url(source)
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
        ImageSource::Url(url) => {
            read = fetch(url)?;
            &read
        }
        #[cfg(not(feature = "net"))]
        ImageSource::Url(_) => return Err(Error::UnresolvedSource(node)),
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
        (
            scene,
            ImageSource::Url("http://example.invalid/x.png".to_owned()),
        )
    }

    /// The allowlist, asserted from both sides.
    ///
    /// **A catch-all here would be the instrument failure this repository
    /// keeps naming**: a path that cannot report the thing it exists to
    /// report. So the softenable set is named, and everything outside it stays
    /// an error even for a URL under a tolerant policy.
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
        let other = ImageSource::Url("http://example.invalid/y.png".to_owned());
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

    /// Chrome's four kinds, declared and inherited, measured by MC Main.
    ///
    /// A parent at `16px` declares; a child at `32px` inherits and states
    /// nothing of its own.
    ///
    /// ```text
    ///                  declared at 16   inherited by the 32px child
    /// number 1.5             24                    48
    /// length 24px            24                    24
    /// percent 150%           24                    24
    /// ```
    ///
    /// **The declared column cannot tell the three apart** -- every one of
    /// them is 24 at 16px. Only the inherited column separates them, and it
    /// separates them in two directions: a percentage resolved late reads 48
    /// where Chrome says 24, and a number resolved early reads 24 where
    /// Chrome says 48. **Two opposite mistakes, each invisible to a test that
    /// only declares.**
    ///
    /// `normal` is not here. It is face-dependent -- Chrome gives 25 and 48
    /// for Poppins against 24 and 47 for Oswald -- and it is its own task.
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

    /// A URL that fails without leaving the machine.
    ///
    /// Port 1 on the loopback: the connection is refused immediately, so the
    /// `net` build's fetch fails fast with **no DNS lookup and no traffic**. A
    /// hostname would resolve -- even a reserved one asks the resolver -- and a
    /// test that touches the network is a test that fails on an aeroplane.
    const UNREACHABLE: &str = "http://127.0.0.1:1/image.png";

    /// Asserts that a scene naming a URL is refused, in whichever way this
    /// build refuses it.
    ///
    /// **The two builds refuse differently and both are correct**, which is the
    /// point of the feature: with `net` off nothing fetches and the node is
    /// [`Error::UnresolvedSource`]; with it on the fetch is attempted and fails
    /// as [`Error::SourceFetch`]. Asserting only one of them would make the
    /// suite pass on one build and fail on the other for no defect.
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
        {
            let _ = node;
            assert!(
                matches!(result, Err(Error::SourceFetch { .. })),
                "a URL should have been fetched and failed, got {result:?}"
            );
        }
    }

    #[test]
    fn a_url_is_refused_because_the_core_does_not_fetch() {
        assert!(is_local(&ImageSource::Path("a".to_owned())));
        assert!(is_local(&ImageSource::Bytes(Vec::new())));
        assert!(!is_local(&ImageSource::Url("https://a.test".to_owned())));

        let mut scene = Scene::new(Size::ZERO);
        let node = scene
            .push(
                NodeId::ROOT,
                image_node(ImageSource::Url(UNREACHABLE.to_owned())),
            )
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

    /// The same 4x2 red PNG as a `data:` URI, base64 and percent-encoded.
    ///
    /// Built from `RED_PNG` rather than pasted, so the two forms cannot drift
    /// from the bytes they are supposed to carry -- a hand-copied payload that
    /// decodes to *something* would pass every assertion below while carrying
    /// a different picture.
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
            ("base64 url", ImageSource::Url(red_png_data_uri(true))),
            ("percent path", ImageSource::Path(red_png_data_uri(false))),
            ("percent url", ImageSource::Url(red_png_data_uri(false))),
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

    /// A 40x20 document that states its size, and the same drawing with only
    /// a `viewBox`.
    ///
    /// `currentColor` rather than a literal, because it is what the tint in
    /// #28b will set and what this build must leave alone: with nothing
    /// setting a colour, SVG's initial `color` is black and that is what these
    /// assertions see.
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
        // **The pair a single-size golden cannot make.** A document
        // rasterised once and stretched would report the small size at both
        // asks; these are two rasterisations, so the pixels differ in count as
        // well as in scale. Without this row a renderer that rasterised at 40
        // and drew at 200 passes.
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
        // **The row the over-broad mutation is for.** A predicate written as
        // "contains a comma" is right for every data URI anybody would type
        // and wrong for `/tmp/logo,v2.png`, which is a filename people write.
        // Without this the mutation failed only my own message test, which is
        // a check on the error text rather than on the classification.
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
            ImageSource::Url("data:image/png;base64,!!!".to_owned()),
            ImageSource::Url("data:text/plain;base64,aGVsbG8=".to_owned()),
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
                source: ImageSource::Url(UNREACHABLE.to_owned()),
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
