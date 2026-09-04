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
    collections::{HashMap, HashSet},
    path::Path,
    sync::OnceLock,
};

use meo_canvas_scene::{
    Scene, Size,
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
        if index == 0 || self.image.frame_count() <= 1 {
            return Ok(self);
        }
        if index >= self.image.frame_count() {
            return Err(Error::UndecodableImage(node));
        }
        self.image
            .frame(index)
            .map(|image| Self { image })
            .map_err(|_| Error::UndecodableImage(node))
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
        let once = decode_sources(scene)?;

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
                let decoded = taken(&once, id, source)?.at_frame(*frame, id)?;
                resolved.images.insert(id, decoded);
            }
            if let Some(background) = node.paint.background_image.as_ref() {
                // Kept in its own table rather than beside the image nodes: a
                // background is drawn into a box layout has already sized, so
                // its extent is not a layout input the way an image node's is,
                // and the measure pass must not find it by asking for a node's
                // image.
                let decoded = taken(&once, id, &background.source)?;
                resolved.backgrounds.insert(id, decoded);
            }
            // A third table for the same reason the second one exists: a mask
            // image is neither the node's own picture nor its background, and
            // a node may carry all three at once.
            if let Some(Mask::Image(source)) = node.effects.mask.as_ref() {
                let decoded = taken(&once, id, source)?;
                resolved.masks.insert(id, decoded);
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
fn decode_sources<'scene>(
    scene: &'scene Scene,
) -> Result<HashMap<&'scene ImageSource, DecodedImage>, Error> {
    let mut wanted: Vec<(&ImageSource, NodeId)> = Vec::new();
    let mut seen: HashSet<&ImageSource> = HashSet::new();
    let mut want = |source: &'scene ImageSource, id: NodeId| {
        if seen.insert(source) {
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

    let decoded = in_parallel(&wanted);
    let mut once = HashMap::with_capacity(decoded.len());
    for ((source, _), result) in wanted.iter().zip(decoded) {
        once.insert(*source, result?);
    }
    Ok(once)
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
) -> Vec<Result<DecodedImage, Error>> {
    let threads = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(wanted.len().max(1));
    if threads <= 1 || wanted.len() <= 1 {
        return wanted
            .iter()
            .map(|(source, id)| decode(*id, source))
            .collect();
    }

    let mut out: Vec<Result<DecodedImage, Error>> = Vec::new();
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
                Err(_) => out.push(Err(Error::UndecodableImage(NodeId::ROOT))),
            }
        }
    });
    out
}

/// The decode a node asked for, taken from what was decoded up front.
fn taken(
    once: &HashMap<&ImageSource, DecodedImage>,
    node: NodeId,
    source: &ImageSource,
) -> Result<DecodedImage, Error> {
    once.get(source)
        .cloned()
        .ok_or(Error::UndecodableImage(node))
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
/// No redirect limit, timeout or size cap is set here beyond `ureq`'s own. That
/// is not an oversight and it is not a promise: a caller wanting a policy has
/// the same escape the TypeScript surface has, which is to fetch the bytes
/// themselves and pass `ImageSource::Bytes`.
#[cfg(feature = "net")]
fn fetch(url: &str) -> Result<Vec<u8>, Error> {
    let mut response =
        ureq::get(url).call().map_err(|error| Error::SourceFetch {
            url: url.to_owned(),
            detail: error.to_string(),
            failure: classify(&error),
        })?;
    response
        .body_mut()
        .read_to_vec()
        .map_err(|error| Error::SourceFetch {
            url: url.to_owned(),
            detail: error.to_string(),
            failure: classify(&error),
        })
}

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
const fn classify(error: &ureq::Error) -> crate::FetchFailure {
    use crate::FetchFailure;

    match error {
        ureq::Error::StatusCode(code) => FetchFailure::Status(*code),
        ureq::Error::HostNotFound => FetchFailure::HostNotFound,
        ureq::Error::BadUri(_) => FetchFailure::BadUrl,
        ureq::Error::Io(_)
        | ureq::Error::ConnectionFailed
        | ureq::Error::Timeout(_) => FetchFailure::Transport,
        _ => FetchFailure::Other,
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

    use super::{Fonts, LineHeight, Resolved, ResolvedText, is_local};
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
