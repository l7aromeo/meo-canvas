//! Walks the solved tree and draws it through `meo-skia-canvas`'s
//! `Context2D`.
//!
//! By the time this pass runs every node has an absolute rectangle, so it is a
//! flat traversal with no measurement and no arithmetic beyond the device scale
//! and the per-node transform. No drawing call crosses a language boundary:
//! the whole stage is Rust calling Rust.
//!
//! # Text is positioned from its baseline
//!
//! Never from the box top. `draw_paragraph` takes the paragraph's top-left, so
//! this pass computes the absolute baseline from [`LayoutResult::baseline`] and
//! subtracts the paragraph's own ascent to reach the top it passes in.
//!
//! Today the two agree and the subtraction cancels. Writing it the other way
//! would tie glyph placement to the box, and any later change that makes a
//! text box taller than its text -- encoding a baseline into the height is the
//! candidate on the table -- would silently move every glyph. The baseline is
//! the fixed point; the box is not.
//!
//! # Nothing here is verified by executing it
//!
//! Running a fill proves the call was made, not that the pixels are right.
//! The assertions in this module cover the arithmetic that can be checked
//! without a rasteriser -- object-fit rectangles, z-order, length resolution,
//! page selection. Everything from the first `Context2D` call onward is
//! covered by golden fixtures or not at all, and the tests do not pretend
//! otherwise.

use std::fmt::Write as _;

use meo_canvas_scene::{
    ColorSpace, ColorType, Rect, Sides, Size,
    node::{Node, NodeId, NodeKind, PathPaint},
    style::{
        Dimension, Length,
        effect::{BoxShadow, FillRule, Mask, MaskShape, Transform},
        layout::{Display, Overflow, PositionType},
        paint::{
            BackgroundImage, BackgroundRepeat, BackgroundSize, BlendMode,
            Color, Gradient, GradientGeometry, LinearDirection, ObjectFit,
            PaintStyle,
        },
        text::{TextAlign, VerticalAlign},
    },
};
use meo_skia_canvas::{
    Affine, BlendMode as SkiaBlendMode, Canvas, CanvasOptions, Context2D,
    FillRule as SkiaFillRule, GradientInterpolation,
    GradientStop as SkiaGradientStop, Image as SkiaImage, Path2D, PathBuilder,
    PixelColorSpace, PixelDepth, PixelExportOptions, PixelFormat, Point,
    RgbaLinear, Shader, StrokeCap, StrokeJoin,
};

use crate::{
    Error,
    layout::LayoutResult,
    measure::SceneMeasurer,
    resolve::{DecodedImage, Resolved},
};

/// Opacity at or above which a node needs no isolation layer.
///
/// Exactly one. Below it the node's children must be composited together and
/// then faded as a group, or overlapping siblings show through each other; at
/// it there is nothing to fade and the layer would cost an offscreen surface
/// for no visible difference.
const OPAQUE: f32 = 1.0;

/// How much narrower than its content a text box may be before painting
/// treats the shortfall as a caller's wrap rather than as rounding.
///
/// One pixel, which is exactly taffy's bound and not a tolerance chosen by
/// eye: `round_layout` rounds each edge to a whole pixel and takes the
/// difference, so a width can differ from the unrounded one by at most one --
/// half a pixel at each edge, rounding opposite ways. An auto-sized text node
/// is settled at precisely its content width, so its rounded box is routinely
/// a fraction narrower than the text it was measured to hold. Re-wrapping on
/// that shortfall would contradict the measurement layout was solved from and
/// put a word on its own line in every such node.
const ROUNDING_SLACK: f32 = 1.0;

/// Degrees in a full turn, for converting a scene's rotation to radians.
const DEGREES_PER_TURN: f32 = 360.0;

/// Everything about a surface that is not its size.
///
/// Resolved values, not the scene's `Option`s: deciding between what a scene
/// asks for and what a renderer offers happens once, in
/// [`Renderer::render`](crate::Renderer::render), and by the time a surface is
/// made there is one answer. A struct rather than three more parameters,
/// because three `bool`-ish arguments in a row is a call nobody can read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SurfaceOptions {
    /// Whether to ask for the GPU.
    pub gpu: bool,
    /// The pixel layout to composite in.
    pub color_type: ColorType,
    /// The colour space to composite in.
    pub color_space: ColorSpace,
}

/// This crate's [`ColorType`] as the renderer's.
///
/// Exhaustive over ours, so a variant added here fails the build. The other
/// direction is [`from_skia_color_type`], which is test-only and exists for
/// exactly the direction this match cannot see.
const fn to_skia_color_type(color_type: ColorType) -> PixelDepth {
    use meo_skia_canvas::PixelDepth as Skia;
    match color_type {
        ColorType::Uint8 => Skia::Uint8,
        ColorType::F16 => Skia::F16,
        ColorType::F32 => Skia::F32,
        ColorType::Alpha8 => Skia::Alpha8,
        ColorType::Gray8 => Skia::Gray8,
        ColorType::R8UNorm => Skia::R8UNorm,
        ColorType::R8G8UNorm => Skia::R8G8UNorm,
        ColorType::A16Float => Skia::A16Float,
        ColorType::A16UNorm => Skia::A16UNorm,
        ColorType::Argb4444 => Skia::Argb4444,
        ColorType::Rgb565 => Skia::Rgb565,
        ColorType::Rgb888x => Skia::Rgb888x,
        ColorType::Bgra8888 => Skia::Bgra8888,
        ColorType::Srgba8888 => Skia::Srgba8888,
        ColorType::N32 => Skia::N32,
        ColorType::Rgba1010102 => Skia::Rgba1010102,
        ColorType::Bgra1010102 => Skia::Bgra1010102,
        ColorType::Rgb101010x => Skia::Rgb101010x,
        ColorType::Bgr101010x => Skia::Bgr101010x,
        ColorType::R16G16Float => Skia::R16G16Float,
        ColorType::R16G16UNorm => Skia::R16G16UNorm,
        ColorType::R16G16B16A16UNorm => Skia::R16G16B16A16UNorm,
        ColorType::F16Norm => Skia::F16Norm,
    }
}

/// This crate's [`ColorSpace`] as the renderer's.
const fn to_skia_color_space(color_space: ColorSpace) -> PixelColorSpace {
    use meo_skia_canvas::PixelColorSpace as Skia;
    match color_space {
        ColorSpace::Srgb => Skia::Srgb,
        ColorSpace::SrgbLinear => Skia::SrgbLinear,
        ColorSpace::DisplayP3 => Skia::DisplayP3,
        ColorSpace::DisplayP3Linear => Skia::DisplayP3Linear,
        ColorSpace::Rec2020 => Skia::Rec2020,
        ColorSpace::Rec2020Linear => Skia::Rec2020Linear,
        ColorSpace::Rec2020Pq => Skia::Rec2020Pq,
        ColorSpace::Rec2020Hlg => Skia::Rec2020Hlg,
    }
}

/// A drawable surface and the pages drawn onto it.
///
/// Owns one `meo-skia-canvas` `Canvas` for the whole scene rather than one per
/// page, because PDF, GIF, APNG, TIFF and ICO all encode N pages out of a
/// single canvas: a multi-page encode cannot be assembled from N single-page
/// surfaces.
pub struct Surface {
    canvas: Canvas,
    scale: f32,
    gpu: bool,
}

impl core::fmt::Debug for Surface {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // `Canvas` does not implement `Debug`, and a dump of a Skia surface
        // would not help a reader. The scale and page count are what say
        // whether the surface is the one the caller meant to build.
        f.debug_struct("Surface")
            .field("scale", &self.scale)
            .field("gpu", &self.gpu)
            .field("pages", &self.canvas.page_count())
            .finish_non_exhaustive()
    }
}

impl Surface {
    /// Allocates a canvas and its first page.
    ///
    /// The pixel size is the logical size times the scale, rounded up: a
    /// fractional scale would otherwise lose the last row of pixels rather
    /// than half of one.
    ///
    /// The canvas options are stated rather than inherited. `Canvas::new`
    /// takes `CanvasOptions::default()`, which sets `gpu: true`
    /// (`meo-skia-canvas-0.11.0/src/canvas.rs:217`), so every render would ask
    /// for the GPU whether or not anyone decided it should. Naming the field
    /// here is what makes `gpu` a decision the caller took rather than a
    /// default nobody read.
    ///
    /// `gpu` is a request, not an outcome: `Canvas::gpu`'s own documentation
    /// says "the request, not the outcome -- on a machine with no reachable
    /// GPU backend this still reports what was asked". A build with no backend
    /// compiled rasterises on the CPU regardless of what is asked for here.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Paint`] when the size is not something a surface can
    /// be made from -- a non-finite or non-positive extent -- or when the
    /// backend refuses the options.
    pub fn new(
        size: Size,
        scale: f32,
        surface: SurfaceOptions,
    ) -> Result<Self, Error> {
        let pixels = pixel_size(size, scale)?;
        let options = CanvasOptions {
            gpu: surface.gpu,
            color_type: to_skia_color_type(surface.color_type),
            color_space: to_skia_color_space(surface.color_space),
            ..CanvasOptions::default()
        };
        let canvas = Canvas::with_options(pixels.width, pixels.height, options)
            .map_err(|error| Error::Paint(error.to_string()))?;
        Ok(Self {
            canvas,
            scale,
            gpu: surface.gpu,
        })
    }

    /// Appends a page and makes it current.
    ///
    /// Not called for the first page, which [`Surface::new`] created.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Paint`] when the size is not one a page can be made
    /// at.
    pub fn begin_page(&mut self, size: Size) -> Result<(), Error> {
        let pixels = pixel_size(size, self.scale)?;
        self.canvas.new_page_with(pixels.width, pixels.height);
        Ok(())
    }

    /// Whether this surface asked for the GPU.
    ///
    /// The request rather than the outcome, for the reason [`Surface::new`]
    /// gives.
    #[must_use]
    pub const fn gpu(&self) -> bool {
        self.gpu
    }

    /// Which rasteriser this surface actually got.
    ///
    /// Distinct from [`Surface::gpu`], which is what was asked for: a build
    /// with no GPU backend compiled reports `"cpu"` however the request was
    /// set. A string rather than the backend's own enum, because no public
    /// signature of this crate names a Skia type.
    #[must_use]
    pub fn engine(&self) -> &'static str {
        match self.canvas.engine_kind() {
            meo_skia_canvas::EngineKind::Gpu => "gpu",
            meo_skia_canvas::EngineKind::Cpu => "cpu",
        }
    }

    /// The device-pixel multiplier every page is drawn at.
    #[must_use]
    pub const fn scale(&self) -> f32 {
        self.scale
    }

    /// How many pages have been begun.
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.canvas.page_count()
    }

    /// The canvas, for the encode pass.
    ///
    /// Mutable because encoding mutates: `Canvas::to_buffer` takes `&mut self`
    /// (`canvas.rs:551`) since it prepares the surface before reading it. A
    /// shared borrow cannot encode, so there is no shared accessor beside this
    /// one -- nothing reads the canvas without also preparing it.
    ///
    /// Crate-internal: no public signature of this crate names a Skia type,
    /// and `encode` is in the same crate, so it costs nothing to keep the
    /// promise here. There is deliberately no `to_buffer` on `Surface` --
    /// that would be encode's job living in paint.
    pub(crate) const fn canvas_mut(&mut self) -> &mut Canvas {
        &mut self.canvas
    }

    fn context(&mut self) -> &mut Context2D {
        self.canvas.context()
    }
}

/// The pixel extent a logical size occupies at a device scale.
fn pixel_size(size: Size, scale: f32) -> Result<Size, Error> {
    let width = (size.width * scale).ceil();
    let height = (size.height * scale).ceil();
    if !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
    {
        return Err(Error::Paint(format!(
            "a surface of {}x{} at scale {scale} has no pixels",
            size.width, size.height
        )));
    }
    Ok(Size::new(width, height))
}

/// Draws one page of the scene onto the surface's current page.
///
/// Which scene page is drawn comes from `layout` rather than from an argument:
/// a [`LayoutResult`] is solved for exactly one page, and the arena is a
/// forest, so the one page root it holds a rectangle for is unambiguous. Two
/// ways to say which page would be one way too many.
///
/// The measurer is taken by `&mut` because a paragraph must be laid out at its
/// final width before it is painted -- Skia draws a paragraph at whatever
/// width it was last laid out at -- and that mutates it. It is not an ordering
/// hazard: every reader lays the paragraph out itself before reading, so no
/// paragraph carries state from one caller to the next, and repeated
/// measurements come from the answer cache rather than from the paragraph.
///
/// # Errors
///
/// Returns [`Error::Paint`] when the layout names no page of this scene, and
/// whatever a draw reports.
pub fn draw(
    surface: &mut Surface,
    resolved: &Resolved<'_>,
    layout: &LayoutResult,
    measurer: &mut SceneMeasurer<'_>,
) -> Result<(), Error> {
    let scene = resolved.scene;
    let page = scene
        .pages
        .iter()
        .copied()
        .find(|page| layout.get(*page).is_some())
        .ok_or_else(|| {
            Error::Paint(
                "the layout holds no rectangle for any page of this scene"
                    .to_owned(),
            )
        })?;

    let scale = surface.scale;
    // Read before the context borrows the canvas: a backdrop readback has to
    // stay inside the surface, and this is the only place its extent is known.
    let device = Size::new(surface.canvas.width(), surface.canvas.height());
    let context = surface.context();
    context.save();
    context.scale(scale, scale);
    let result = walk(context, resolved, layout, measurer, page, device);
    context.restore();
    result
}

/// One entry in the traversal's own stack.
///
/// Iterative rather than recursive, for the reason `Scene::validate` is: a
/// scene is caller data, and a tree deeper than the thread's stack would abort
/// the process instead of returning an error. The explicit `Leave` is what
/// keeps `save`/`restore` balanced without the call stack to unwind it.
#[derive(Debug, Clone)]
enum Step {
    Enter(NodeId),
    /// A participant, and the clipping ancestors between it and its context.
    ///
    /// A hoisted node paints as a sibling of its context root rather than
    /// nested under its parent, so nothing would apply the `overflow` of the
    /// parents it was lifted past. Clipping is not stacking: CSS applies an
    /// ancestor's clip to a descendant however the two are ordered.
    ///
    /// **Only a clip can be owed.** Every other thing an ancestor could impose
    /// — a transform, an opacity, a blend, a mask, a filter — establishes a
    /// stacking context, so a node carrying one is never an intermediate. That
    /// is what makes this a list of rectangles rather than a replay of the
    /// ancestors' state.
    EnterClipped {
        id: NodeId,
        clips: Vec<NodeId>,
    },
    Leave {
        layers: u8,
        /// The node whose mask is composited as its group layer closes.
        ///
        /// `Some` only for a node that opened one: masking is `DestinationIn`
        /// against everything the group drew, so it has to happen inside the
        /// layer and after the last of it.
        masked: Option<NodeId>,
    },
}

fn walk(
    context: &mut Context2D,
    resolved: &Resolved<'_>,
    layout: &LayoutResult,
    measurer: &mut SceneMeasurer<'_>,
    page: NodeId,
    device: Size,
) -> Result<(), Error> {
    let scene = resolved.scene;
    let mut stack = vec![Step::Enter(page)];

    while let Some(step) = stack.pop() {
        match step {
            Step::Leave { layers, masked } => {
                if let Some(id) = masked
                    && let (Some(rect), Some(node)) =
                        (layout.get(id), scene.get(id))
                {
                    apply_mask(context, resolved, id, node, rect)?;
                }
                for _ in 0..layers {
                    context.restore();
                }
                context.restore();
            }
            Step::EnterClipped { id, clips } => {
                let (Some(rect), Some(node)) = (layout.get(id), scene.get(id))
                else {
                    continue;
                };

                context.save();
                // The clips this node was hoisted past, outermost first, so a
                // nested `overflow` narrows in the order the tree does.
                for owed in &clips {
                    let (Some(rect), Some(owed)) =
                        (layout.get(*owed), scene.get(*owed))
                    else {
                        continue;
                    };
                    clip_to_box(context, &owed.paint, rect)?;
                }

                let layers = enter_node(context, node, rect, device)?;
                paint_own_content(
                    context, resolved, layout, measurer, id, node, rect,
                )?;

                stack.push(Step::Leave {
                    layers,
                    masked: node.effects.mask.as_ref().map(|_| id),
                });
                // **Only a context gathers.** A participant that establishes
                // none was reached by its own context's walk, which already
                // collected everything beneath it; gathering again here would
                // paint that subtree a second time.
                if establishes_stacking_context(node) {
                    for participant in
                        participants(scene, id, node).into_iter().rev()
                    {
                        stack.push(participant);
                    }
                }
            }
            Step::Enter(id) => {
                // A node with no rectangle was never laid out, which for a
                // `Display::None` subtree is the whole subtree: layout does not
                // build them, so there are no children to skip either.
                let (Some(rect), Some(node)) = (layout.get(id), scene.get(id))
                else {
                    continue;
                };

                context.save();
                let layers = enter_node(context, node, rect, device)?;
                paint_own_content(
                    context, resolved, layout, measurer, id, node, rect,
                )?;

                stack.push(Step::Leave {
                    layers,
                    masked: node.effects.mask.as_ref().map(|_| id),
                });
                // Everything this context paints, in CSS's order, gathered
                // *through* descendants that establish no context of their own.
                // A participant that does establish one is entered here and
                // gathers its own.
                for participant in
                    participants(scene, id, node).into_iter().rev()
                {
                    stack.push(participant);
                }
            }
        }
    }
    Ok(())
}

/// Everything one stacking context paints, in the order it paints them.
///
/// # Why this is not simply the children
///
/// A stacking context gathers its descendants **through** any that establish no
/// context of their own. A `z-index: -1` child of a plain `<div>` does not
/// belong to that div's stack — the div has no stack — it belongs to the
/// nearest ancestor that has one, where it paints *before* that ancestor's
/// content and so behind the div's own background.
///
/// Painting each node's children under that node, which is what this did
/// before, gives every node a stack of its own. Measured: a `z-index: -1` child
/// showed through in all three of a plain parent, an `overflow: hidden` parent
/// and an `isolation: isolate` one, where Chrome shows it only in the third.
///
/// # The order
///
/// CSS's painting order, restricted to what a scene here can hold: negative
/// `z_index` first, then descendants the index does not apply to, then those it
/// does at zero or above. The sort is stable, so tree order decides within a
/// rank — which is what makes "document order within a z-index" true rather
/// than incidental.
///
/// A participant that establishes a context is one entry, entered whole. Its
/// own descendants are gathered by its own call and never appear here.
fn participants(
    scene: &meo_canvas_scene::Scene,
    root: NodeId,
    node: &Node,
) -> Vec<Step> {
    /// One participant, the two keys it sorts by, and what it was hoisted past.
    struct Ranked {
        id: NodeId,
        z: i32,
        /// Clipping ancestors between this node and the context root,
        /// outermost first. See [`Step::EnterClipped`].
        clips: Vec<NodeId>,
    }

    let mut found: Vec<Ranked> = Vec::new();
    // Iterative for the reason `walk` is: a scene is caller data, and a tree
    // deeper than the thread's stack would abort rather than return an error.
    // A pre-order walk, so `found` is in document order before it is sorted and
    // the sort's stability is what decides ties.
    // The context's own clip is owed by its participants just as any
    // intermediate's is — it is not applied around them, so that a node
    // entitled to escape it can.
    let root_clips = clips_its_children(node);
    let mut pending: Vec<(NodeId, NodeId, Vec<NodeId>)> = node
        .children
        .iter()
        .rev()
        .map(|child| {
            let owed = if root_clips
                && scene
                    .get(*child)
                    .is_none_or(|child| !escapes_clip(node, child))
            {
                vec![root]
            } else {
                Vec::new()
            };
            (*child, root, owed)
        })
        .collect();

    while let Some((id, parent_id, clips)) = pending.pop() {
        let (Some(source), Some(parent)) =
            (scene.get(id), scene.get(parent_id))
        else {
            continue;
        };

        found.push(Ranked {
            id,
            z: if stacks_by_z_index(parent, source) {
                source.paint.z_index.unwrap_or(0)
            } else {
                0
            },
            clips: clips.clone(),
        });

        // Descend only through nodes with no context of their own. One that
        // has a context is a single participant here and gathers its own
        // descendants when it is entered.
        if !establishes_stacking_context(source) {
            let clipper = clips_its_children(source);
            for child in source.children.iter().rev() {
                let mut inherited = clips.clone();
                // Decided per child rather than once for all of them: whether
                // this node's clip reaches a child depends on that child.
                if clipper
                    && scene
                        .get(*child)
                        .is_none_or(|child| !escapes_clip(source, child))
                {
                    inherited.push(id);
                }
                pending.push((*child, id, inherited));
            }
        }
    }

    // By `z` alone, and the sort is stable, so everything else keeps tree
    // order — which is what puts an ancestor before its own descendant.
    //
    // A second key almost went in here: CSS paints in-flow descendants before
    // positioned ones at the same level, so ranking by "did the index apply"
    // looked like a free improvement. It is not. Applied across depths it sorts
    // a static grandchild *before* its own parent, and the parent's background
    // then covers it — `display: block` below the page root painted no children
    // at all, because a block container's static child is not indexed while the
    // container itself is. Getting that nicety right needs grouping by ancestor
    // rather than a flat key, and a flat key breaks the invariant that a
    // descendant paints after the box it sits in.
    found.sort_by_key(|ranked| ranked.z);
    found
        .into_iter()
        .map(|ranked| Step::EnterClipped {
            id: ranked.id,
            clips: ranked.clips,
        })
        .collect()
}

/// Whether `child` slips out of `clipper`'s `overflow`.
///
/// `overflow` clips a node's **content**, and an absolutely positioned node is
/// not a box's content merely by sitting inside it: it is laid out against its
/// containing block, and CSS clips it only where the clipper is that containing
/// block or lies between it and one. So an unpositioned box clips its in-flow
/// children and lets an absolute one through.
///
/// Ported from v1's `b434a23`, which fixed the same defect there, and measured
/// against it: a 50-wide absolute child in a 20-wide clipper is clipped when
/// the clipper is `relative` and not when the clipper names no position.
///
/// A [`PositionType::Fixed`] node escapes **every** clipper reached this way.
/// Its containing block is not any positioned ancestor — it is the transform or
/// filter that captures it, or nothing at all — so neither a static nor a
/// relative box cuts one where either would cut an absolute node. A capturing
/// ancestor does still cut it, and needs no case here: a transform or a filter
/// establishes a stacking context, so such a node is never an intermediate and
/// its clip is applied around everything its context gathers.
///
/// Ported from v1's `4f542d8`. Measured before: a 50-wide fixed child in a
/// 20-wide clipper painted 20 columns under a static clipper and 20 under a
/// relative one, where both should be 50.
const fn escapes_clip(clipper: &Node, child: &Node) -> bool {
    match child.layout.position_type {
        PositionType::Fixed => true,
        PositionType::Absolute => {
            matches!(clipper.layout.position_type, PositionType::Static)
        }
        PositionType::Static
        | PositionType::Relative
        | PositionType::Sticky => false,
    }
}

/// Whether this node's `overflow` clips what is inside it.
///
/// Not a stacking-context trigger, which is the whole point: a clip binds a
/// descendant however the two are ordered, so a node hoisted out of a clipping
/// parent is still clipped by it.
const fn clips_its_children(node: &Node) -> bool {
    !matches!(node.layout.overflow.0, Overflow::Visible)
        || !matches!(node.layout.overflow.1, Overflow::Visible)
}

/// Whether this node establishes a stacking context.
///
/// The declarations that create one, of the twenty-seven CSS lists, restricted
/// to those a still renderer can observe and this scene can express:
///
/// - **positioned with a `z_index` other than `auto`** — `Some(_)`, not
///   `Some(0)` against `None`: `Some(0)` creates a context and `None` does not,
///   which is the whole reason [`PaintStyle::z_index`] is an `Option`
/// - an `opacity` below one
/// - a `blend_mode` other than `Normal`
/// - a `mask`, which stands for CSS's `clip-path` and `mask-image` both
/// - a `transform`
/// - a `filter` or a `backdrop_filter`
///
/// `overflow` is **not** one, and that is the trap this list exists to avoid:
/// clipping is not isolation. Measured in Chrome — a `z-index: -1` child of an
/// `overflow: hidden` parent is hidden exactly as it is under a plain one.
///
/// Absent because the scene cannot say them: `isolation` and `contain`, whose
/// only observable effect in a still render *is* the context, and which are
/// worth adding once the painter can act on one. Absent because they mean
/// nothing here: `will-change`, a promise about a future value in a renderer
/// that draws once, and `perspective`, since nothing else here is 3D.
fn establishes_stacking_context(node: &Node) -> bool {
    let positioned = !matches!(node.layout.position_type, PositionType::Static);
    (positioned && node.paint.z_index.is_some())
        || node.paint.opacity < OPAQUE
        || node.paint.blend_mode != BlendMode::Normal
        || node.effects.mask.is_some()
        || node.effects.transform.is_some()
        || node.effects.filter.is_some()
        || node.effects.backdrop_filter.is_some()
}

/// Whether `child`'s `z_index` gives it a place in `parent`'s stack.
///
/// CSS 2.1 §9.9.1 gives `z-index` to positioned elements only. Flexbox §5.4 and
/// Grid §6.2 each extend it to their items whatever their position, because
/// being an item of that container is itself what earns the place. So: the
/// child is positioned -- anything but [`PositionType::Static`] -- or its
/// parent lays out as flex or grid.
///
/// This is neither v1's rule, which is absolutely positioned only, nor "every
/// sibling", which was this renderer's first answer and is wrong for a block
/// container.
///
/// Measured in Chrome across all five combinations rather than derived from the
/// three specifications, because a rule assembled from three documents is a
/// rule nobody has seen run:
///
/// | container | child | `z_index` |
/// |---|---|---|
/// | block | static | ignored |
/// | block | relative | applied |
/// | flex | static | applied |
/// | flex | relative | applied |
/// | grid | static | applied |
const fn stacks_by_z_index(parent: &Node, child: &Node) -> bool {
    !matches!(child.layout.position_type, PositionType::Static)
        || matches!(parent.layout.display, Display::Flex | Display::Grid)
}

/// Paints a node's own box and kind, under its own `overflow`.
///
/// In a save of its own, because a node's `overflow` clips **its** content and
/// not what a descendant painted elsewhere in the order: the clip has to be
/// gone by the time this context's participants are painted, so that one
/// entitled to escape it can. See [`escapes_clip`].
fn paint_own_content(
    context: &mut Context2D,
    resolved: &Resolved<'_>,
    layout: &LayoutResult,
    measurer: &mut SceneMeasurer<'_>,
    id: NodeId,
    node: &Node,
    rect: Rect,
) -> Result<(), Error> {
    context.save();
    // The one place the backend is asked for it. A gradient shallow enough to
    // band is what shows the difference -- a steep ramp over twenty pixels
    // moves no bytes with it on -- and `RGB565` and `ARGB4444` are the
    // layouts it exists for.
    context.set_dither(node.paint.dither);
    if clips_its_children(node) {
        clip_to_box(context, &node.paint, rect)?;
    }
    let result = paint_box(context, resolved, id, node, rect).and_then(|()| {
        paint_kind(context, resolved, layout, measurer, id, node, rect)
    });
    context.restore();
    result
}

/// Applies the transform and opens whatever isolation layers the node needs.
///
/// Returns how many layers were opened, so the matching `Leave` closes exactly
/// those.
fn enter_node(
    context: &mut Context2D,
    node: &Node,
    rect: Rect,
    device: Size,
) -> Result<u8, Error> {
    apply_transform(context, node.effects.transform.as_ref(), rect);

    // Before the node's own filter is set and before its group opens: the
    // backdrop is what is **already** on the canvas, so it has to be read and
    // put back while that is still all there is.
    draw_backdrop(context, node, rect, device)?;

    // The node's own `overflow` is **not** applied here. It clips descendants,
    // and a descendant reaches this painter as a participant of whichever
    // context gathers it rather than nested inside this call — so the clip
    // travels with the participant, in the list `Step::EnterClipped` carries,
    // where a node entitled to escape it can. Applied here it would wrap
    // everything this node gathers with no way out, which is how an absolute
    // child of a clipper that was *also* a stacking context stayed clipped
    // after `escapes_clip` was taught to let it through.
    //
    // The node's own content is clipped by the caller, in a save of its own.

    let mut layers = 0_u8;
    let alpha = node.paint.opacity.clamp(0.0, OPAQUE);
    let blend = node.paint.blend_mode;
    let needs_group = alpha < OPAQUE
        || blend != BlendMode::Normal
        || node.effects.mask.is_some();

    if let Some(filter) = node.effects.filter.as_deref() {
        context
            .set_filter_css(filter)
            .map_err(|error| Error::Paint(error.to_string()))?;
    }

    if needs_group {
        context.set_global_composite_operation(to_skia_blend(blend));
        context.save_layer_with(alpha, None, None);
        layers += 1;
        // Inside the group the children composite normally against each other;
        // the group's own alpha and blend apply when the layer closes.
        context.set_global_composite_operation(SkiaBlendMode::SourceOver);
        context.set_global_alpha(OPAQUE);
    }

    Ok(layers)
}

/// Draws the node's own box: shadows, background, gradient, image, border.
fn paint_box(
    context: &mut Context2D,
    resolved: &Resolved<'_>,
    id: NodeId,
    node: &Node,
    rect: Rect,
) -> Result<(), Error> {
    let paint = &node.paint;
    // Outer shadows first, under everything: they fall outside the box, so the
    // background about to be painted cannot reach them.
    for shadow in node.effects.box_shadows.iter().filter(|s| !s.inset) {
        draw_box_shadow(context, paint, rect, shadow)?;
    }

    if !paint.background_color.is_invisible() {
        context.set_fill_style(to_skia_color(paint.background_color));
        fill_box(context, paint, rect)?;
    }

    if let Some(gradient) = paint.gradient.as_ref() {
        let shader = build_gradient(gradient, rect)?;
        context.set_fill_shader(&shader);
        fill_box(context, paint, rect)?;
    }

    // Inset shadows after the background and before the border, which is where
    // CSS puts them. Drawn with the outer ones they were painted **and then
    // covered by the very background they fall on**, which is why the arm
    // looked unimplemented from the outside.
    for shadow in node.effects.box_shadows.iter().filter(|s| s.inset) {
        draw_box_shadow(context, paint, rect, shadow)?;
    }

    if let Some(background) = paint.background_image.as_ref()
        && let Some(image) = resolved.background(id).map(DecodedImage::inner)
    {
        draw_background_image(context, paint, background, image, rect)?;
    }

    draw_border(context, node, rect)?;
    Ok(())
}

/// Draws whatever the node's kind is.
fn paint_kind(
    context: &mut Context2D,
    resolved: &Resolved<'_>,
    layout: &LayoutResult,
    measurer: &mut SceneMeasurer<'_>,
    id: NodeId,
    node: &Node,
    rect: Rect,
) -> Result<(), Error> {
    match &node.kind {
        NodeKind::Box => Ok(()),
        NodeKind::Text { .. } => {
            draw_text(context, layout, measurer, id, rect);
            Ok(())
        }
        NodeKind::Image { fit, position, .. } => {
            let Some(image) = resolved.image(id).map(DecodedImage::inner)
            else {
                return Ok(());
            };
            let intrinsic =
                Size::new(image.width() as f32, image.height() as f32);
            let placed = fit_image(intrinsic, rect, *fit, *position);
            context.draw_image_sized(
                image,
                placed.origin.x,
                placed.origin.y,
                placed.size.width,
                placed.size.height,
            );
            Ok(())
        }
        NodeKind::Path {
            data,
            fill,
            stroke,
            line_width,
            fill_rule,
            line_cap,
            line_join,
            line_dash,
            line_dash_offset,
        } => {
            let path = Path2D::from_svg(data, to_skia_rule(*fill_rule))
                .map_err(|error| Error::Paint(error.to_string()))?
                .offset(rect.origin.x, rect.origin.y);

            if let Some(fill) = fill {
                set_paint(context, fill, rect, true)?;
                context.fill_path(&path, to_skia_rule(*fill_rule));
            }
            if let Some(stroke) = stroke {
                set_paint(context, stroke, rect, false)?;
                context.set_line_width(*line_width);
                context.set_line_dash(line_dash);
                context.set_line_dash_offset(*line_dash_offset);
                context.set_line_cap(to_skia_cap(*line_cap));
                context.set_line_join(to_skia_join(*line_join));
                context.stroke_path(&path);
            }
            Ok(())
        }
    }
}

/// Draws a paragraph, placed by its baseline.
fn draw_text(
    context: &mut Context2D,
    layout: &LayoutResult,
    measurer: &mut SceneMeasurer<'_>,
    id: NodeId,
    rect: Rect,
) {
    let Some(baseline_from_top) = layout.baseline(id) else {
        return;
    };
    let baseline = rect.origin.y + baseline_from_top;

    // Read before the paragraph is borrowed mutably.
    let text = measurer.resolved().text(id);
    let needs_a_finite_width = text.is_some_and(|text| {
        !matches!(text.align, TextAlign::Start | TextAlign::Left)
    });
    let vertical_align =
        text.map_or(VerticalAlign::Top, |text| text.vertical_align);

    let Some(paragraph) = measurer.paragraph_mut(id) else {
        return;
    };
    // Laid out again at the width layout settled on: Skia paints a paragraph
    // at whatever width it last saw, and the measurer's last question was not
    // necessarily this one.
    //
    // Unconstrained first, then narrowed only if the box is genuinely narrower
    // than the content, for the reason `measure_text` does the same: laying
    // out at exactly the content's own width loses the last word to a float
    // comparison, and an auto-sized text node's box *is* exactly its content
    // width. Painting at the box width directly wrapped every such node.
    paragraph.layout(f32::INFINITY);
    if rect.size.width + ROUNDING_SLACK < paragraph.width() {
        paragraph.layout(rect.size.width);
    } else if needs_a_finite_width {
        // **Skia aligns within the width it last saw**, so a paragraph left at
        // `INFINITY` centres about infinity and paints nothing at all:
        // `text_align: center` and `right` drew zero ink where `left` was
        // correct, because only they depend on the width.
        //
        // Only for those alignments, and never below the natural width. Doing
        // it for every paragraph moved `baseline-alignment` — a word vanished
        // and the ink ran 50 pixels lower — because the height and baseline
        // Skia reports depend on the width it was laid out at, and every
        // left-aligned node had been measured against the unconstrained one.
        paragraph.layout(rect.size.width.max(paragraph.width()));
    }
    // **The block within the node's box, not a line within its line box.**
    // v1 measures the whole paragraph against the box and shifts it by what
    // is left over, and where v2 and v1 disagree on what is drawn v1 wins.
    // The name is CSS's and the behaviour is not: `vertical-align` in CSS
    // places an inline box on its line, which a scene with one paragraph per
    // node has no way to ask for.
    //
    // Not clamped at zero, also v1: a paragraph taller than its box hangs out
    // of it, centred or bottom-aligned, rather than being pinned to the top.
    // An auto-sized node has no leftover at all, so every alignment agrees
    // there -- which is why a control for this has to give the box a height.
    let free = rect.size.height - paragraph.height();
    let shift = match vertical_align {
        VerticalAlign::Top => 0.0,
        VerticalAlign::Middle => free / 2.0,
        VerticalAlign::Bottom => free,
    };
    let top = baseline - paragraph.alphabetic_baseline() + shift;
    context.draw_paragraph(paragraph, rect.origin.x, top);
}

/// Where an image sits inside its box under an object-fit rule.
///
/// Returns the destination rectangle, which may be larger than the box for
/// [`ObjectFit::Cover`] -- the caller crops rather than the fit shrinking.
fn fit_image(
    intrinsic: Size,
    box_rect: Rect,
    fit: ObjectFit,
    position: (Length, Length),
) -> Rect {
    let box_size = box_rect.size;
    if intrinsic.width <= 0.0 || intrinsic.height <= 0.0 {
        return box_rect;
    }

    let scale_x = box_size.width / intrinsic.width;
    let scale_y = box_size.height / intrinsic.height;
    let size = match fit {
        ObjectFit::Fill => box_size,
        ObjectFit::Contain => scaled(intrinsic, scale_x.min(scale_y)),
        ObjectFit::Cover => scaled(intrinsic, scale_x.max(scale_y)),
        ObjectFit::None => intrinsic,
        // The smaller of `None` and `Contain`, which is `Contain` only when
        // the image is larger than its box.
        ObjectFit::ScaleDown => {
            scaled(intrinsic, scale_x.min(scale_y).min(1.0))
        }
    };

    // The leftover space, distributed by `position`. A fraction of zero pins
    // the image to the start edge and one to the end, which is how CSS's
    // `object-position` percentages read.
    let free =
        Size::new(box_size.width - size.width, box_size.height - size.height);
    let offset_x = resolve_length(position.0, free.width);
    let offset_y = resolve_length(position.1, free.height);

    Rect::new(
        meo_canvas_scene::Point::new(
            box_rect.origin.x + offset_x,
            box_rect.origin.y + offset_y,
        ),
        size,
    )
}

fn scaled(size: Size, factor: f32) -> Size {
    Size::new(size.width * factor, size.height * factor)
}

/// A length against the extent it is a fraction of.
fn resolve_length(length: Length, reference: f32) -> f32 {
    match length {
        Length::Points(points) => points,
        Length::Percent(fraction) => fraction * reference,
    }
}

/// Applies a node's transform about its own origin point.
fn apply_transform(
    context: &mut Context2D,
    transform: Option<&Transform>,
    rect: Rect,
) {
    let Some(transform) = transform else {
        return;
    };
    // The origin is a fraction of the node's own box, so a transform written
    // once behaves the same on a box of any size. Translating to it, applying,
    // and translating back is what makes the rotation and scale act about that
    // point rather than about the surface origin.
    let origin_x =
        rect.origin.x + resolve_length(transform.origin.0, rect.size.width);
    let origin_y =
        rect.origin.y + resolve_length(transform.origin.1, rect.size.height);

    context.translate(origin_x, origin_y);
    context.rotate(
        transform.rotate_degrees / DEGREES_PER_TURN * core::f32::consts::TAU,
    );
    context.scale(transform.scale_x, transform.scale_y);
    context.translate(-origin_x, -origin_y);
    context.translate(
        resolve_length(transform.translate_x, rect.size.width),
        resolve_length(transform.translate_y, rect.size.height),
    );
}

/// Adds the node's box, rounded if it has radii, as the current path.
fn box_path(
    context: &mut Context2D,
    paint: &PaintStyle,
    rect: Rect,
) -> Result<(), Error> {
    context.begin_path();
    box_path_continuing(context, paint, rect)
}

/// The same contour, added to whatever path is already open.
///
/// Split from [`box_path`] for the callers that need two contours in one path —
/// a ring, and an inset shadow's surround-with-a-hole — where a second
/// `begin_path` would discard the first.
fn box_path_continuing(
    context: &mut Context2D,
    paint: &PaintStyle,
    rect: Rect,
) -> Result<(), Error> {
    let radii = paint.border_radius;
    let corners = [
        radii.top_left,
        radii.top_right,
        radii.bottom_right,
        radii.bottom_left,
    ];
    // A square box is a rounded one with every radius at zero, **not**
    // `Context2D::rect`. The two add contours by different mechanisms —
    // `rect` calls Skia's `add_rect`, and `round_rect_elliptical` calls
    // `add_path_with_transform` with `AddPathMode::Extend`
    // (`meo-skia-canvas-0.11.0/src/context2d.rs:1837` against `:2354`) — and
    // mixing them in one path joins the two contours instead of leaving them
    // separate.
    //
    // That matters here because `ring_path` fills an outer contour and an inner
    // one with the even-odd rule to leave a border. Joined, they become one
    // self-intersecting contour, and a 40x40 box with a 4px border painted
    // **a blue triangle over half of it** rather than a border. Any radius at
    // all, even one, took the other branch and was correct — which is why every
    // bordered golden, all of them rounded, missed it.
    if corners.iter().all(|radius| *radius <= 0.0) {
        return context
            .round_rect_elliptical(
                rect.origin.x,
                rect.origin.y,
                rect.size.width,
                rect.size.height,
                [(0.0, 0.0); 4],
            )
            .map_err(|error| Error::Paint(error.to_string()));
    }
    context
        .round_rect(
            rect.origin.x,
            rect.origin.y,
            rect.size.width,
            rect.size.height,
            corners,
        )
        .map_err(|error| Error::Paint(error.to_string()))
}

fn fill_box(
    context: &mut Context2D,
    paint: &PaintStyle,
    rect: Rect,
) -> Result<(), Error> {
    box_path(context, paint, rect)?;
    context.fill(SkiaFillRule::NonZero);
    Ok(())
}

fn clip_to_box(
    context: &mut Context2D,
    paint: &PaintStyle,
    rect: Rect,
) -> Result<(), Error> {
    box_path(context, paint, rect)?;
    context.clip(SkiaFillRule::NonZero);
    Ok(())
}

/// Paints a node's background picture across its box.
///
/// **Tiled by drawing the tiles, not by a pattern shader**, which is v1's own
/// choice and its reason: `Space` distributes the leftover between whole tiles
/// and `Round` stretches them so a whole number fits, and a repeating fill can
/// express neither. Drawing them keeps the size, the origin and the step in
/// one place for all six modes rather than two rules in two shapes.
///
/// Clipped to the box, corners included: a background stops where its node
/// does, and a tile that overhangs the last row is cut rather than skipped.
fn draw_background_image(
    context: &mut Context2D,
    paint: &PaintStyle,
    background: &BackgroundImage,
    image: &SkiaImage,
    rect: Rect,
) -> Result<(), Error> {
    let intrinsic = Size::new(image.width() as f32, image.height() as f32);
    let tile = background_tile(background.size, intrinsic, rect.size);
    if tile.width <= 0.0
        || tile.height <= 0.0
        || rect.size.width <= 0.0
        || rect.size.height <= 0.0
    {
        return Ok(());
    }

    // `Space` and `Round` tile on both axes; the two directional modes tile on
    // one. CSS says so, and it is the only place the six modes are not one
    // rule applied twice.
    let across = matches!(
        background.repeat,
        BackgroundRepeat::Repeat
            | BackgroundRepeat::RepeatX
            | BackgroundRepeat::Space
            | BackgroundRepeat::Round
    );
    let down = matches!(
        background.repeat,
        BackgroundRepeat::Repeat
            | BackgroundRepeat::RepeatY
            | BackgroundRepeat::Space
            | BackgroundRepeat::Round
    );

    let columns = lay_tiles_out(
        rect.size.width,
        tile.width,
        background.repeat,
        across,
        tile_origin(background.position.0, rect.size.width, tile.width),
    );
    let rows = lay_tiles_out(
        rect.size.height,
        tile.height,
        background.repeat,
        down,
        tile_origin(background.position.1, rect.size.height, tile.height),
    );

    context.save();
    let result = (|context: &mut Context2D| {
        clip_to_box(context, paint, rect)?;
        for left in &columns.offsets {
            for top in &rows.offsets {
                context.draw_image_sized(
                    image,
                    rect.origin.x + left,
                    rect.origin.y + top,
                    columns.extent,
                    rows.extent,
                );
            }
        }
        Ok(())
    })(context);
    context.restore();
    result
}

/// Where the tiles sit along one axis, and how long each one is.
struct TileRun {
    /// Each tile's offset from the box's own edge.
    offsets: Vec<f32>,
    /// The length one tile is drawn at, which `Round` changes.
    extent: f32,
}

/// How big one tile is drawn.
///
/// A single length sets that axis and the other follows the picture's own
/// proportions, which is CSS's rule for `background-size: 40px` and the reason
/// [`Dimension::Auto`] has to survive as far as here. `Cover` and `Contain`
/// scale to the box the way an image node's `object-fit` does.
fn background_tile(
    size: BackgroundSize,
    intrinsic: Size,
    box_size: Size,
) -> Size {
    if intrinsic.width <= 0.0 || intrinsic.height <= 0.0 {
        return intrinsic;
    }
    let ratio = intrinsic.width / intrinsic.height;

    match size {
        BackgroundSize::Cover | BackgroundSize::Contain => {
            if box_size.height <= 0.0 {
                return intrinsic;
            }
            let box_ratio = box_size.width / box_size.height;
            // Cover matches the width when the picture is the narrower of the
            // two; contain matches it when the picture is the wider. The one
            // comparison, read the two ways round.
            let match_width = match size {
                BackgroundSize::Cover => ratio < box_ratio,
                _ => ratio > box_ratio,
            };
            if match_width {
                Size::new(box_size.width, box_size.width / ratio)
            } else {
                Size::new(box_size.height * ratio, box_size.height)
            }
        }
        BackgroundSize::PerAxis(width, height) => {
            let width = resolve_dimension(width, box_size.width);
            let height = resolve_dimension(height, box_size.height);
            match (width, height) {
                (Some(width), Some(height)) => Size::new(width, height),
                (Some(width), None) => Size::new(width, width / ratio),
                (None, Some(height)) => Size::new(height * ratio, height),
                (None, None) => intrinsic,
            }
        }
    }
}

/// A [`Dimension`] against the axis it lies along, or `None` for `Auto`.
fn resolve_dimension(dimension: Dimension, reference: f32) -> Option<f32> {
    match dimension {
        Dimension::Auto => None,
        Dimension::Points(points) => Some(points),
        Dimension::Percent(fraction) => Some(fraction * reference),
    }
}

/// Where the first tile's near edge sits.
///
/// **A percentage is a share of the slack, not a distance from the edge.** CSS
/// lines the same fraction of the picture up with that fraction of the box, so
/// `100%` puts the picture's far edge against the box's far edge rather than
/// pushing it out by a whole width.
///
/// Truncated to a whole pixel, which is v1's behaviour: its `tileOrigin` ends
/// in a `| 0`. That reads as incidental rather than intended -- a length in
/// points is not truncated two lines above it -- but it is what v1 draws, and
/// half a pixel of origin is a different row of anti-aliasing.
fn tile_origin(position: Length, extent: f32, tile: f32) -> f32 {
    match position {
        Length::Points(points) => points,
        Length::Percent(fraction) => (fraction * (extent - tile)).trunc(),
    }
}

/// Every tile offset along one axis.
///
/// `Space` fits whole tiles and shares the remainder out as equal gaps,
/// pinning the first and last to the edges -- so it ignores the origin, as CSS
/// does. `Round` scales the tile instead, so a whole number fills the axis
/// exactly. Every other mode leaves the tile at its own length and steps by
/// it, from the origin, **both ways**: a positive origin still has to cover
/// the near edge, which is the tile the naive loop leaves out.
fn lay_tiles_out(
    extent: f32,
    tile: f32,
    repeat: BackgroundRepeat,
    repeats: bool,
    origin: f32,
) -> TileRun {
    // A tile far below a pixel would otherwise ask for millions of draws, and
    // a scene is caller data: the cap is a hang turned into a picture. v1 has
    // no cap, and at these sizes neither picture is one anybody looks at.
    const MOST_TILES: usize = 4096;

    if !repeats {
        return TileRun {
            offsets: vec![origin],
            extent: tile,
        };
    }

    match repeat {
        BackgroundRepeat::Round => {
            #[expect(
                clippy::cast_sign_loss,
                clippy::cast_possible_truncation,
                reason = "the round is clamped to at least one and capped"
            )]
            let count =
                ((extent / tile).round().max(1.0) as usize).min(MOST_TILES);
            let rounded = extent / count as f32;
            TileRun {
                offsets: (0..count)
                    .map(|index| index as f32 * rounded)
                    .collect(),
                extent: rounded,
            }
        }
        BackgroundRepeat::Space => {
            #[expect(
                clippy::cast_sign_loss,
                clippy::cast_possible_truncation,
                reason = "the floor of a positive ratio, capped"
            )]
            let count =
                ((extent / tile).floor().max(0.0) as usize).min(MOST_TILES);
            if count <= 1 {
                return TileRun {
                    offsets: vec![0.0],
                    extent: tile,
                };
            }
            let gap =
                (count as f32).mul_add(-tile, extent) / (count as f32 - 1.0);
            TileRun {
                offsets: (0..count)
                    .map(|index| index as f32 * (tile + gap))
                    .collect(),
                extent: tile,
            }
        }
        BackgroundRepeat::Repeat
        | BackgroundRepeat::RepeatX
        | BackgroundRepeat::RepeatY
        | BackgroundRepeat::NoRepeat => {
            let mut offsets = Vec::new();
            let mut position = origin % tile;
            // One tile before the first, whenever the origin leaves a gap at
            // the near edge.
            if position > 0.0 {
                position -= tile;
            }
            while position < extent && offsets.len() < MOST_TILES {
                offsets.push(position);
                position += tile;
            }
            TileRun {
                offsets,
                extent: tile,
            }
        }
    }
}

/// Strokes the border, one edge at a time where the edges differ.
///
/// A single stroked rounded rectangle would be wrong wherever two edges have
/// different widths or colours, which CSS allows and `Sides` carries.
/// Fills the border ring, edge by edge where the edges differ.
///
/// CSS puts a border **inside** the border box: the outer edge of the stroke is
/// the box itself and the border grows inward, so a border is the ring between
/// the border box and the padding box rather than a line centred on the box
/// edge. Both rings are rounded — the inner radii derive from the outer ones,
/// each axis reduced by that side's width and floored at zero
/// (CSS Backgrounds 3 §5.2) — which is what makes a rounded card's border
/// follow its fill instead of squaring off around it.
///
/// The ring is one path: the outer rounded rectangle and the inner one, filled
/// even-odd. Where the edges differ, each edge clips that same ring to its own
/// share before filling. **The clip is the specification, not an
/// approximation**: CSS Backgrounds 3 §4.4 divides a corner between its two
/// edges along the straight line joining the outer corner point to the inner
/// corner point, and a quadrilateral through those four points is exactly that
/// line on both ends. Unequal widths move the inner corner, so the join angle
/// follows the widths without being computed from them.
fn draw_border(
    context: &mut Context2D,
    node: &Node,
    rect: Rect,
) -> Result<(), Error> {
    let paint = &node.paint;
    let widths = node.layout.border;
    if widths.top <= 0.0
        && widths.right <= 0.0
        && widths.bottom <= 0.0
        && widths.left <= 0.0
    {
        return Ok(());
    }

    // Bit equality rather than an epsilon: these are four values a caller set,
    // not four results of arithmetic, and "the author wrote the same number on
    // every edge" is exactly the question. Two widths a hair apart are two
    // widths, and painting them as one would be the wrong picture.
    let uniform = widths.top.to_bits() == widths.right.to_bits()
        && widths.right.to_bits() == widths.bottom.to_bits()
        && widths.bottom.to_bits() == widths.left.to_bits();
    let edge_colors = paint.border_color;
    let same_colour = [
        edge_colors.top,
        edge_colors.right,
        edge_colors.bottom,
        edge_colors.left,
    ]
    .iter()
    .all(Option::is_none);

    let inner = inner_box(rect, widths);

    // One ring, one fill, no clip. The same shape the edge-by-edge path
    // produces, reached without four clips when nothing distinguishes the
    // edges.
    if uniform && same_colour {
        context.set_fill_style(to_skia_color(paint.border_color_all));
        let ring = ring_path(paint, rect, inner, widths)?;
        context.fill_path(&ring, SkiaFillRule::EvenOdd);
        return Ok(());
    }

    let outer_corners = [
        (rect.origin.x, rect.origin.y),
        (rect.right(), rect.origin.y),
        (rect.right(), rect.bottom()),
        (rect.origin.x, rect.bottom()),
    ];
    let inner_corners = [
        (inner.origin.x, inner.origin.y),
        (inner.right(), inner.origin.y),
        (inner.right(), inner.bottom()),
        (inner.origin.x, inner.bottom()),
    ];

    let divisions = divisions_at(outer_corners, inner_corners);

    // Top, right, bottom, left: each edge owns the part of the ring between
    // the division lines at its two corners.
    for (edge, (width, colour)) in [
        (widths.top, edge_colors.top),
        (widths.right, edge_colors.right),
        (widths.bottom, edge_colors.bottom),
        (widths.left, edge_colors.left),
    ]
    .into_iter()
    .enumerate()
    {
        if width <= 0.0 {
            continue;
        }
        let next = (edge + 1) % outer_corners.len();
        // Far enough along each division line to clear the ring at that
        // corner -- the ring never reaches further from a corner than its
        // radius plus the thickest edge -- and never past where the two lines
        // meet, because beyond their meeting point the wedge folds over
        // itself and a bow-tie is not the shape either fill rule reads.
        let radii = radii_at(paint);
        let clearance = radii[edge].max(radii[next])
            + widths
                .top
                .max(widths.right)
                .max(widths.bottom)
                .max(widths.left)
            + 1.0;
        let limit = meeting_point(
            outer_corners[edge],
            divisions[edge],
            outer_corners[next],
            divisions[next],
        )
        .unwrap_or(clearance)
        .min(clearance);
        let far = |corner: (f32, f32), direction: (f32, f32)| {
            (
                direction.0.mul_add(limit, corner.0),
                direction.1.mul_add(limit, corner.1),
            )
        };
        let far_edge = far(outer_corners[edge], divisions[edge]);
        let far_next = far(outer_corners[next], divisions[next]);

        context.save();
        context.begin_path();
        context.move_to(outer_corners[edge].0, outer_corners[edge].1);
        context.line_to(outer_corners[next].0, outer_corners[next].1);
        context.line_to(far_next.0, far_next.1);
        context.line_to(far_edge.0, far_edge.1);
        context.close_path();
        context.clip(SkiaFillRule::NonZero);

        context.set_fill_style(to_skia_color(
            colour.unwrap_or(paint.border_color_all),
        ));
        let path = ring_path(paint, rect, inner, widths);
        if let Ok(ring) = &path {
            context.fill_path(ring, SkiaFillRule::EvenOdd);
        }
        context.restore();
        path?;
    }
    Ok(())
}

/// The padding box: the border box less each side's width.
///
/// Collapses to zero rather than going negative when the widths meet, which is
/// a border thick enough to cover the box and is a picture rather than an
/// error.
fn inner_box(rect: Rect, widths: Sides<f32>) -> Rect {
    let width = (rect.size.width - widths.left - widths.right).max(0.0);
    let height = (rect.size.height - widths.top - widths.bottom).max(0.0);
    Rect::new(
        meo_canvas_scene::Point::new(
            rect.origin.x + widths.left,
            rect.origin.y + widths.top,
        ),
        Size::new(width, height),
    )
}

/// The direction every corner's division line runs in.
///
/// The fallback is the 45-degree mitre, reached only when a corner's two
/// points coincide -- both its widths are zero, and there is no ring there to
/// divide.
fn divisions_at(
    outer: [(f32, f32); 4],
    inner: [(f32, f32); 4],
) -> [(f32, f32); 4] {
    const MITRE: f32 = core::f32::consts::FRAC_1_SQRT_2;
    let fallbacks = [
        (MITRE, MITRE),
        (-MITRE, MITRE),
        (-MITRE, -MITRE),
        (MITRE, -MITRE),
    ];
    core::array::from_fn(|corner| {
        division(outer[corner], inner[corner], fallbacks[corner])
    })
}

/// The direction a corner's division line runs in.
///
/// CSS Backgrounds 3 §4.4 divides a corner between its two edges along the
/// line **from the corner's outer point to its inner point**. With equal
/// widths that is the 45-degree mitre everyone pictures; with unequal ones it
/// leans towards the thinner edge; and **when one width is zero the inner
/// point lies on that side, the line degenerates to the box edge, and the
/// whole arc falls to the other edge**.
///
/// That last case is what this function exists for. Each edge used to be
/// clipped to a quadrilateral running outer corner, outer corner, inner
/// corner, inner corner -- which is the right region only where the corner is
/// square. Where it is rounded, the ring sweeps *past* the inner box's own
/// edge, into a part of the corner that quadrilateral does not contain: with
/// `border-left: 0` and a 20px radius, the top edge painted its two pixels and
/// the arc below them was handed to an edge with no width to paint it, leaving
/// a gap the fill showed straight through.
///
/// `fallback` is used only when the two points coincide, which means both
/// widths at that corner are zero and there is no ring there to divide.
fn division(
    outer: (f32, f32),
    inner: (f32, f32),
    fallback: (f32, f32),
) -> (f32, f32) {
    let (dx, dy) = (inner.0 - outer.0, inner.1 - outer.1);
    let length = dx.hypot(dy);
    if length > 0.0 {
        (dx / length, dy / length)
    } else {
        fallback
    }
}

/// How far along its own division line one corner is from where that line
/// meets the next corner's.
///
/// `None` when the two are parallel, which two opposite mitres of equal widths
/// are not but two vertical ones can be. The caller falls back to its own
/// clearance then.
fn meeting_point(
    from: (f32, f32),
    along: (f32, f32),
    other: (f32, f32),
    other_along: (f32, f32),
) -> Option<f32> {
    let determinant =
        other_along.0.mul_add(along.1, -(along.0 * other_along.1));
    if determinant.abs() < f32::EPSILON {
        return None;
    }
    let (dx, dy) = (other.0 - from.0, other.1 - from.1);
    let distance =
        other_along.0.mul_add(dy, -(dx * other_along.1)) / determinant;
    (distance > 0.0).then_some(distance)
}

/// The four corner radii, in the same order as the corners.
const fn radii_at(paint: &PaintStyle) -> [f32; 4] {
    let radii = paint.border_radius;
    [
        radii.top_left,
        radii.top_right,
        radii.bottom_right,
        radii.bottom_left,
    ]
}

/// Draws a shadow that falls **inside** the box.
///
/// The outer arms of this feature all worked — offset, spread, colour and two
/// at once — while `inset` returned without drawing, so every test and the
/// `box-shadow` fixture passed with one arm of it doing nothing.
///
/// # How it is drawn
///
/// A shadow is a property of the paint rather than a separate draw, so an inset
/// one is the same trick as an outer one turned inside out: clip to the box,
/// then fill **everything except** the box — offset, and shrunk by the spread —
/// with the shadow configured. Skia casts that fill's shadow inwards, the clip
/// keeps it to the box, and the fill itself is invisible because it lies
/// entirely outside the clip.
///
/// The outer rectangle is the box grown by enough to cover any offset and blur,
/// so the hole is what casts and the surround never shows an edge of its own.
fn draw_inset_box_shadow(
    context: &mut Context2D,
    paint: &PaintStyle,
    rect: Rect,
    shadow: &BoxShadow,
) -> Result<(), Error> {
    context.save();
    let result = (|| -> Result<(), Error> {
        // Only inside the box: this is the whole of what makes it inset.
        clip_to_box(context, paint, rect)?;

        context.set_shadow_blur(shadow.blur);
        context.set_shadow_color(to_skia_color(shadow.color));
        context.set_shadow_offset(shadow.offset_x, shadow.offset_y);
        context.set_fill_style(to_skia_color(shadow.color));

        // The hole: the box pulled in by the spread, then moved by the offset.
        // Pulling in is what makes a positive spread a *thicker* inset shadow,
        // where on an outer one it makes a larger shape.
        let spread = shadow.spread;
        let hole = Rect::new(
            meo_canvas_scene::Point::new(
                rect.origin.x + spread + shadow.offset_x,
                rect.origin.y + spread + shadow.offset_y,
            ),
            Size::new(
                spread.mul_add(-2.0, rect.size.width).max(0.0),
                spread.mul_add(-2.0, rect.size.height).max(0.0),
            ),
        );

        // Far enough out that the surround's own edges cannot reach the clip
        // even at the largest offset and blur.
        let margin = shadow.blur.mul_add(3.0, spread.abs())
            + shadow.offset_x.abs()
            + shadow.offset_y.abs()
            + rect.size.width
            + rect.size.height;
        let surround = Rect::new(
            meo_canvas_scene::Point::new(
                rect.origin.x - margin,
                rect.origin.y - margin,
            ),
            Size::new(
                margin.mul_add(2.0, rect.size.width),
                margin.mul_add(2.0, rect.size.height),
            ),
        );

        context.begin_path();
        context
            .round_rect_elliptical(
                surround.origin.x,
                surround.origin.y,
                surround.size.width,
                surround.size.height,
                [(0.0, 0.0); 4],
            )
            .map_err(|error| Error::Paint(error.to_string()))?;
        box_path_continuing(context, paint, hole)?;
        context.fill(SkiaFillRule::EvenOdd);
        Ok(())
    })();
    context.restore();
    result
}

/// One box contour as a path of its own.
///
/// The corner radii are given per axis because an inner corner shrinks by a
/// different side's width on each axis.
fn contour(rect: Rect, radii: [(f32, f32); 4]) -> Result<PathBuilder, Error> {
    let mut path = PathBuilder::new();
    path.round_rect_elliptical(
        rect.origin.x,
        rect.origin.y,
        rect.size.width,
        rect.size.height,
        radii,
    )
    .map_err(|error| Error::Paint(error.to_string()))?;
    Ok(path)
}

/// Builds the ring between the border box and the padding box as one path.
///
/// Two subpaths filled even-odd, which is what makes the inner one a hole. The
/// inner corners are elliptical because each axis shrinks by a different side's
/// width: a 20px corner inside a 2px top and an 8px right is 18px tall and 12px
/// wide, and drawing it circular would leave the fill and the border
/// disagreeing about where the curve is.
fn ring_path(
    paint: &PaintStyle,
    outer: Rect,
    inner: Rect,
    widths: Sides<f32>,
) -> Result<Path2D, Error> {
    let radii = paint.border_radius;
    let mut ring = contour(
        outer,
        [
            (radii.top_left, radii.top_left),
            (radii.top_right, radii.top_right),
            (radii.bottom_right, radii.bottom_right),
            (radii.bottom_left, radii.bottom_left),
        ],
    )?;

    if inner.size.width <= 0.0 || inner.size.height <= 0.0 {
        // No hole: the border meets in the middle and the ring is the whole
        // box.
        return Ok(ring.build(SkiaFillRule::EvenOdd));
    }

    let hole = contour(
        inner,
        [
            (
                (radii.top_left - widths.left).max(0.0),
                (radii.top_left - widths.top).max(0.0),
            ),
            (
                (radii.top_right - widths.right).max(0.0),
                (radii.top_right - widths.top).max(0.0),
            ),
            (
                (radii.bottom_right - widths.right).max(0.0),
                (radii.bottom_right - widths.bottom).max(0.0),
            ),
            (
                (radii.bottom_left - widths.left).max(0.0),
                (radii.bottom_left - widths.bottom).max(0.0),
            ),
        ],
    )?;

    // **`Path2D::add_path` appends; `Context2D::round_rect_elliptical`
    // extends.** That is the whole of this bug. Building both contours on the
    // context joined them into one self-intersecting shape, and filling it
    // even-odd drew a diagonal across the box — half of it at first, and after
    // a narrower fix still a wedge across the bottom edge. Two paths added as
    // separate subpaths are two regions, which is what a ring is.
    ring.add_path(&hole.build(SkiaFillRule::EvenOdd));
    Ok(ring.build(SkiaFillRule::EvenOdd))
}

/// Draws one box shadow.
///
/// Skia's shadow is a property of the paint rather than a separate draw, so
/// the shadow is produced by filling the box again with the shadow configured
/// and the fill itself transparent where the box is about to be painted over.
fn draw_box_shadow(
    context: &mut Context2D,
    paint: &PaintStyle,
    rect: Rect,
    shadow: &BoxShadow,
) -> Result<(), Error> {
    if shadow.inset {
        return draw_inset_box_shadow(context, paint, rect, shadow);
    }
    context.save();
    context.set_shadow_blur(shadow.blur);
    context.set_shadow_color(to_skia_color(shadow.color));
    context.set_shadow_offset(shadow.offset_x, shadow.offset_y);
    context.set_fill_style(to_skia_color(shadow.color));

    let spread = shadow.spread;
    let spread_rect = Rect::new(
        meo_canvas_scene::Point::new(
            rect.origin.x - spread,
            rect.origin.y - spread,
        ),
        Size::new(
            spread.mul_add(2.0, rect.size.width),
            spread.mul_add(2.0, rect.size.height),
        ),
    );
    let result = fill_box(context, paint, spread_rect);
    context.restore();
    result
}

/// Sets the fill or stroke source for a painted path.
fn set_paint(
    context: &mut Context2D,
    paint: &PathPaint,
    rect: Rect,
    fill: bool,
) -> Result<(), Error> {
    match paint {
        PathPaint::Solid(color) => {
            if fill {
                context.set_fill_style(to_skia_color(*color));
            } else {
                context.set_stroke_style(to_skia_color(*color));
            }
        }
        PathPaint::Gradient(gradient) => {
            let shader = build_gradient(gradient, rect)?;
            if fill {
                context.set_fill_shader(&shader);
            } else {
                context.set_stroke_shader(&shader);
            }
        }
    }
    Ok(())
}

/// Builds a shader for a gradient placed against a node's box.
fn build_gradient(gradient: &Gradient, rect: Rect) -> Result<Shader, Error> {
    let stops: Vec<SkiaGradientStop> = gradient
        .stops
        .iter()
        .map(|stop| SkiaGradientStop {
            position: stop.offset,
            color: to_skia_color(stop.color),
        })
        .collect();

    // A point given as a fraction of the box, placed in the box.
    // Named `place` rather than `at`, which is now a field name in the
    // geometry and would shadow it at every call site here.
    let place = |point: (Length, Length)| {
        Point::new(
            rect.origin.x + resolve_length(point.0, rect.size.width),
            rect.origin.y + resolve_length(point.1, rect.size.height),
        )
    };

    let shader = match gradient.geometry {
        GradientGeometry::Linear { direction } => {
            let (start, end) = match direction {
                LinearDirection::Angle(degrees) => gradient_line(degrees, rect),
                // Explicit endpoints, which is the thing an angle cannot say:
                // where the ramp begins and ends rather than merely which way
                // it runs.
                LinearDirection::Between { start, end } => {
                    (place(start), place(end))
                }
            };
            Shader::linear_gradient(
                start,
                end,
                &stops,
                GradientInterpolation::default(),
            )
        }
        GradientGeometry::Radial { at } => {
            // The radius that reaches the furthest corner, which is CSS's
            // `farthest-corner` default for a radial gradient.
            let radius = rect.size.width.hypot(rect.size.height) / 2.0;
            Shader::radial_gradient(
                place(at),
                radius,
                &stops,
                GradientInterpolation::default(),
            )
        }
        GradientGeometry::Conic { at, from } => Shader::sweep_gradient(
            place(at),
            from,
            from + DEGREES_PER_TURN,
            &stops,
            GradientInterpolation::default(),
        ),
    };
    shader.map_err(|error| Error::Paint(error.to_string()))
}

/// The two endpoints of a linear gradient's line.
///
/// The angle is CSS's: measured clockwise from twelve o'clock, so zero runs
/// bottom to top. The line passes through the box's centre and is long enough
/// that its ends fall outside the box, which is what makes the first and last
/// stops reach the corners.
fn gradient_line(angle_degrees: f32, rect: Rect) -> (Point, Point) {
    let radians = angle_degrees / DEGREES_PER_TURN * core::f32::consts::TAU;
    let (sin, cos) = radians.sin_cos();
    let half = f32::midpoint(
        rect.size.width * sin.abs(),
        rect.size.height * cos.abs(),
    );
    let cx = rect.origin.x + rect.size.width / 2.0;
    let cy = rect.origin.y + rect.size.height / 2.0;
    (
        Point::new(sin.mul_add(-half, cx), cos.mul_add(half, cy)),
        Point::new(sin.mul_add(half, cx), cos.mul_add(-half, cy)),
    )
}

fn to_skia_color(color: Color) -> RgbaLinear {
    RgbaLinear::from_srgb8(
        color.r,
        color.g,
        color.b,
        f32::from(color.a) / 255.0,
    )
}

const fn to_skia_rule(rule: FillRule) -> SkiaFillRule {
    match rule {
        FillRule::NonZero => SkiaFillRule::NonZero,
        FillRule::EvenOdd => SkiaFillRule::EvenOdd,
    }
}

const fn to_skia_cap(cap: meo_canvas_scene::node::LineCap) -> StrokeCap {
    match cap {
        meo_canvas_scene::node::LineCap::Butt => StrokeCap::Butt,
        meo_canvas_scene::node::LineCap::Round => StrokeCap::Round,
        meo_canvas_scene::node::LineCap::Square => StrokeCap::Square,
    }
}

const fn to_skia_join(join: meo_canvas_scene::node::LineJoin) -> StrokeJoin {
    match join {
        meo_canvas_scene::node::LineJoin::Bevel => StrokeJoin::Bevel,
        meo_canvas_scene::node::LineJoin::Round => StrokeJoin::Round,
        meo_canvas_scene::node::LineJoin::Miter => StrokeJoin::Miter,
    }
}

const fn to_skia_blend(mode: BlendMode) -> SkiaBlendMode {
    match mode {
        BlendMode::Normal => SkiaBlendMode::SourceOver,
        BlendMode::Multiply => SkiaBlendMode::Multiply,
        BlendMode::Screen => SkiaBlendMode::Screen,
        BlendMode::Overlay => SkiaBlendMode::Overlay,
        BlendMode::Darken => SkiaBlendMode::Darken,
        BlendMode::Lighten => SkiaBlendMode::Lighten,
        BlendMode::ColorDodge => SkiaBlendMode::ColorDodge,
        BlendMode::ColorBurn => SkiaBlendMode::ColorBurn,
        BlendMode::HardLight => SkiaBlendMode::HardLight,
        BlendMode::SoftLight => SkiaBlendMode::SoftLight,
        BlendMode::Difference => SkiaBlendMode::Difference,
        BlendMode::Exclusion => SkiaBlendMode::Exclusion,
        BlendMode::Hue => SkiaBlendMode::Hue,
        BlendMode::Saturation => SkiaBlendMode::Saturation,
        BlendMode::Color => SkiaBlendMode::Color,
        BlendMode::Luminosity => SkiaBlendMode::Luminosity,
    }
}

/// Filters what is already on the canvas behind the node, in its own box.
///
/// # Why a readback rather than a filter on the layer
///
/// `save_layer_with` takes a backdrop [`ImageFilter`], which is the direct
/// route — and the binding builds one only from its own typed `FilterOp`s.
/// The CSS chain a scene carries is parsed by `parse_filter`, which is
/// `pub(crate)`: reachable from `set_filter_css` and from nowhere else. So
/// the chain is applied the one way a caller can apply it, to a draw, and the
/// thing drawn is the backdrop itself read back off the surface.
///
/// # Why the transform is reset
///
/// `get_image_data` works in device pixels and ignores the transform, as the
/// Canvas standard says it does. Rather than trying to unrotate a readback,
/// the pixels go back down in device space too: the **clip is kept**, since
/// Skia stores it in device space and it survives the reset, so a rotated or
/// scaled node still filters exactly its own box. The blur is therefore in
/// device pixels and grows with the page scale, which is what a browser does
/// with a device pixel ratio.
///
/// A backdrop is read at eight bits in sRGB whatever the surface's own depth
/// is. The image carries that space, so drawing it back into a wide-gamut or
/// float surface converts rather than reinterprets; what is lost is precision
/// in the filtered region, not colour.
fn draw_backdrop(
    context: &mut Context2D,
    node: &Node,
    rect: Rect,
    device: Size,
) -> Result<(), Error> {
    let Some(css) = node.effects.backdrop_filter.as_deref() else {
        return Ok(());
    };

    let transform = context.get_transform();
    // The chain is applied to a draw made with the transform reset, so its
    // lengths have to be device lengths. v1 rewrites them for exactly this
    // reason, and without it the same tree exported at two scales is two
    // different pictures -- `blur(6px)` at `scale: 2` covering three page
    // pixels rather than six.
    let scaled = scale_filter_lengths(css, page_scale(transform));
    // Grown by how far the chain reaches in from outside, so a blur at the
    // node's edge pulls in what is beyond it rather than smearing the edge
    // pixel. Clamped to the surface, because a readback that leaves it is a
    // failed read rather than a short one.
    let reach = filter_spill(&scaled);
    let bounds = device_bounds(transform, rect);
    // Rounded outward, so a box landing between pixels filters the whole of
    // every pixel it touches rather than losing its last row to a truncation.
    let left = (bounds.origin.x - reach).floor().max(0.0);
    let top = (bounds.origin.y - reach).floor().max(0.0);
    let right = (bounds.origin.x + bounds.size.width + reach)
        .ceil()
        .min(device.width);
    let bottom = (bounds.origin.y + bounds.size.height + reach)
        .ceil()
        .min(device.height);
    let (width, height) = (right - left, bottom - top);
    if width < 1.0 || height < 1.0 {
        return Ok(());
    }

    // Premultiplied, which is how the surface already holds them: asking for
    // unpremultiplied divides every channel by its alpha and the draw
    // multiplies it straight back, which costs precision wherever the
    // backdrop is translucent and gains nothing.
    let data = context
        .get_image_data_as(
            left,
            top,
            width,
            height,
            PixelExportOptions {
                color_space: PixelColorSpace::Srgb,
                depth: PixelDepth::Uint8,
                premultiplied: true,
            },
        )
        .map_err(|error| Error::Paint(error.to_string()))?;
    let backdrop = SkiaImage::from_pixels(
        data.pixels(),
        data.width(),
        data.height(),
        data.stride(),
        PixelFormat::Rgba8UnormPremul,
        PixelColorSpace::Srgb,
    )
    .map_err(|error| Error::Paint(error.to_string()))?;

    context.save();
    let result = (|context: &mut Context2D| {
        clip_to_box(context, &node.paint, rect)?;
        context.reset_transform();
        context
            .set_filter_css(&scaled)
            .map_err(|error| Error::Paint(error.to_string()))?;
        context.draw_image_sized(&backdrop, left, top, width, height);
        Ok(())
    })(context);
    context.restore();
    result
}

/// The average of the transform's two axis lengths.
///
/// One number for a transform that may scale the axes differently, which is
/// what a filter's single radius needs. v1's own reading of the matrix.
fn page_scale(transform: Affine) -> f32 {
    let horizontal = transform.a.hypot(transform.b);
    let vertical = transform.c.hypot(transform.d);
    let scale = horizontal.midpoint(vertical);
    if scale > 0.0 && scale.is_finite() {
        scale
    } else {
        1.0
    }
}

/// Rewrites a filter chain's `px` lengths into device pixels.
///
/// Only `blur` and `drop-shadow` carry lengths; a `brightness` factor or a
/// `hue-rotate` angle means the same thing at any resolution. Only `px` is
/// rewritten, which is v1's rule too -- an `em` resolves against the context's
/// font size inside the binding, and a chain arriving in `em` is one this
/// renderer has never been asked for.
fn scale_filter_lengths(css: &str, scale: f32) -> String {
    if (scale - 1.0).abs() < f32::EPSILON {
        return css.to_owned();
    }
    let mut out = String::with_capacity(css.len());
    let mut last = 0;
    for call in filter_calls(css) {
        let carries_lengths = call.name.eq_ignore_ascii_case("blur")
            || call.name.eq_ignore_ascii_case("drop-shadow");
        if !carries_lengths {
            continue;
        }
        out.push_str(&css[last..call.args_start]);
        out.push_str(&scale_pixel_lengths(
            &css[call.args_start..call.args_end],
            scale,
        ));
        last = call.args_end;
    }
    out.push_str(&css[last..]);
    out
}

/// Multiplies every `<number>px` in one function's arguments by `scale`.
fn scale_pixel_lengths(args: &str, scale: f32) -> String {
    let mut out = String::with_capacity(args.len());
    let mut rest = args;
    while let Some((number, text, tail)) = next_pixel_length(rest) {
        out.push_str(text);
        write!(out, "{}px", number * scale).unwrap_or_else(|error| {
            unreachable!("writing to a string cannot fail: {error}")
        });
        rest = tail;
    }
    out.push_str(rest);
    out
}

/// The next `<number>px` in `args`: its value, what preceded it, what follows.
fn next_pixel_length(args: &str) -> Option<(f32, &str, &str)> {
    let mut search = 0;
    while let Some(offset) = args[search..].find("px") {
        let end = search + offset;
        let start = args[..end]
            .rfind(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
            .map_or(0, |index| index + 1);
        if let Ok(number) = args[start..end].parse::<f32>() {
            return Some((number, &args[..start], &args[end + 2..]));
        }
        search = end + 2;
    }
    None
}

/// How far past its own box a filter chain reaches, in the chain's own units.
///
/// Three standard deviations, which is where a Gaussian has given up
/// nine-thousand-nine-hundred-and-seventy parts in ten thousand of its weight;
/// v1's constant, and the reason a blurred backdrop does not show a seam at
/// the node's edge. `drop-shadow` adds its offset on top of its blur.
fn filter_spill(css: &str) -> f32 {
    const DEVIATIONS_TO_COVER: f32 = 3.0;
    let mut spill = 0.0;
    for call in filter_calls(css) {
        let args = &css[call.args_start..call.args_end];
        if call.name.eq_ignore_ascii_case("blur") {
            spill = pixel_lengths(args)
                .first()
                .unwrap_or(&0.0)
                .abs()
                .mul_add(DEVIATIONS_TO_COVER, spill);
        } else if call.name.eq_ignore_ascii_case("drop-shadow") {
            let lengths = pixel_lengths(args);
            let offset_x = lengths.first().copied().unwrap_or(0.0);
            let offset_y = lengths.get(1).copied().unwrap_or(0.0);
            let blur = lengths.get(2).copied().unwrap_or(0.0);
            spill += blur.abs().mul_add(
                DEVIATIONS_TO_COVER,
                offset_x.abs().max(offset_y.abs()),
            );
        }
    }
    spill.ceil()
}

/// Every `<number>px` in one function's arguments, in order.
fn pixel_lengths(args: &str) -> Vec<f32> {
    let mut lengths = Vec::new();
    let mut rest = args;
    while let Some((number, _, tail)) = next_pixel_length(rest) {
        lengths.push(number);
        rest = tail;
    }
    lengths
}

/// One `name(...)` in a filter chain.
struct FilterCall<'css> {
    /// The function's name, exactly as written.
    name: &'css str,
    /// Byte offset of the first character inside the parentheses.
    args_start: usize,
    /// Byte offset of the closing parenthesis.
    args_end: usize,
}

/// The top-level function calls in a filter chain, outermost only.
///
/// Nested parentheses are skipped rather than reported: `drop-shadow(2px 2px
/// 4px rgb(0 0 0 / 50%))` is one call whose arguments happen to contain
/// another, and reading the `rgb` as a filter would look for lengths in a
/// colour.
fn filter_calls(css: &str) -> Vec<FilterCall<'_>> {
    let mut calls = Vec::new();
    let mut chars = css.char_indices();
    while let Some((index, character)) = chars.next() {
        if character != '(' {
            continue;
        }
        let name_start = css[..index]
            .rfind(|c: char| !c.is_ascii_alphanumeric() && c != '-')
            .map_or(0, |at| at + c_len(css, at));
        let mut depth = 1_u32;
        let mut close = None;
        for (at, inner) in chars.by_ref() {
            match inner {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(at);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close) = close else { break };
        calls.push(FilterCall {
            name: &css[name_start..index],
            args_start: index + 1,
            args_end: close,
        });
    }
    calls
}

/// The byte length of the character starting at `at`.
fn c_len(css: &str, at: usize) -> usize {
    css[at..].chars().next().map_or(1, char::len_utf8)
}

/// The device-space bounding box of `rect` under `transform`.
///
/// All four corners rather than two, because a rotation sends the top-left
/// corner somewhere that is neither the left nor the top.
fn device_bounds(transform: Affine, rect: Rect) -> Rect {
    let map = |x: f32, y: f32| {
        (
            transform.a.mul_add(x, transform.c * y) + transform.tx,
            transform.b.mul_add(x, transform.d * y) + transform.ty,
        )
    };
    let right = rect.origin.x + rect.size.width;
    let bottom = rect.origin.y + rect.size.height;
    let corners = [
        map(rect.origin.x, rect.origin.y),
        map(right, rect.origin.y),
        map(rect.origin.x, bottom),
        map(right, bottom),
    ];
    let left = corners.iter().map(|c| c.0).fold(f32::INFINITY, f32::min);
    let top = corners.iter().map(|c| c.1).fold(f32::INFINITY, f32::min);
    let far = corners
        .iter()
        .map(|c| c.0)
        .fold(f32::NEG_INFINITY, f32::max);
    let low = corners
        .iter()
        .map(|c| c.1)
        .fold(f32::NEG_INFINITY, f32::max);
    Rect::new(
        meo_canvas_scene::Point::new(left, top),
        Size::new(far - left, low - top),
    )
}

/// Composites the node's mask over everything its group layer drew.
///
/// # Why a layer of its own rather than a blended fill
///
/// `DestinationIn` keeps the destination only where the source has alpha —
/// but a blend touches **only the pixels the draw covers**. Filling an
/// ellipse with `DestinationIn` therefore trims nothing outside the ellipse:
/// the rest of the group survives untouched, which is a mask that keeps
/// everything. Drawing the mask into its own layer and closing that layer
/// under `DestinationIn` composites the mask as one rectangle covering the
/// group, so the transparent parts of it clear what they cover.
///
/// This is why the mask cannot be applied where the node is entered. The
/// group has to be complete first, which is the moment [`Step::Leave`] names.
///
/// The group it composites against is the one [`enter_node`] opens: a mask is
/// one of the three things `needs_group` tests for, so a masked node always
/// has one. Drop it from that test and this would trim the page instead of
/// the node.
///
/// # What each kind contributes
///
/// [`Mask::Shape`] and [`Mask::Path`] are opaque geometry: a hard edge, and
/// the alpha is the coverage. [`Mask::Gradient`] and [`Mask::Image`] carry
/// their own alpha, which is what makes a fade-out edge expressible — the
/// gradient's stops and the image's alpha channel are read as the mask
/// directly, not as a luminance the way CSS's default `mask-mode` does.
fn apply_mask(
    context: &mut Context2D,
    resolved: &Resolved<'_>,
    id: NodeId,
    node: &Node,
    rect: Rect,
) -> Result<(), Error> {
    let Some(mask) = node.effects.mask.as_ref() else {
        return Ok(());
    };

    context.save();
    context.set_global_composite_operation(SkiaBlendMode::DestinationIn);
    context.save_layer();
    // Inside the mask's own layer the drawing is ordinary again: the
    // `DestinationIn` above belongs to the layer's composite, and leaving it
    // set here would have the mask trim itself against an empty layer.
    context.set_global_composite_operation(SkiaBlendMode::SourceOver);
    context.set_global_alpha(OPAQUE);
    context.set_fill_style(to_skia_color(Color::rgb(255, 255, 255)));

    let result = draw_mask(context, resolved, id, mask, rect);

    context.restore();
    context.restore();
    result
}

/// Draws one mask's coverage into the layer [`apply_mask`] opened.
fn draw_mask(
    context: &mut Context2D,
    resolved: &Resolved<'_>,
    id: NodeId,
    mask: &Mask,
    rect: Rect,
) -> Result<(), Error> {
    match mask {
        Mask::Shape(shape) => {
            let (radius_x, radius_y) = match shape {
                // The largest circle that fits, so the smaller half-extent
                // is both radii.
                MaskShape::Circle => {
                    let radius = rect.size.width.min(rect.size.height) / 2.0;
                    (radius, radius)
                }
                MaskShape::Ellipse => {
                    (rect.size.width / 2.0, rect.size.height / 2.0)
                }
            };
            context.begin_path();
            context
                .ellipse(
                    rect.origin.x + rect.size.width / 2.0,
                    rect.origin.y + rect.size.height / 2.0,
                    radius_x,
                    radius_y,
                    0.0,
                    0.0,
                    std::f32::consts::TAU,
                    false,
                )
                .map_err(|error| Error::Paint(error.to_string()))?;
            context.fill(SkiaFillRule::NonZero);
            Ok(())
        }
        // Offset like a path node's own data, because the mask is written in
        // the node's coordinate space and drawn in the page's.
        Mask::Path { data, fill_rule } => {
            let rule = to_skia_rule(*fill_rule);
            let path = Path2D::from_svg(data, rule)
                .map_err(|error| Error::Paint(error.to_string()))?
                .offset(rect.origin.x, rect.origin.y);
            context.fill_path(&path, rule);
            Ok(())
        }
        Mask::Gradient(gradient) => {
            let shader = build_gradient(gradient, rect)?;
            context.set_fill_shader(&shader);
            context.fill_rect(
                rect.origin.x,
                rect.origin.y,
                rect.size.width,
                rect.size.height,
            );
            Ok(())
        }
        Mask::Image(_) => {
            if let Some(image) = resolved.mask(id).map(DecodedImage::inner) {
                context.draw_image_sized(
                    image,
                    rect.origin.x,
                    rect.origin.y,
                    rect.size.width,
                    rect.size.height,
                );
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use meo_canvas_scene::{
        Length, Point, Rect, Scene, Size,
        node::{ImageSource, Node, NodeId, NodeKind},
        style::paint::{BlendMode, ObjectFit, PaintStyle},
    };

    use super::{
        Affine, ColorSpace, ColorType, Display, GradientGeometry,
        LinearDirection, Overflow, PositionType, SkiaFillRule, Surface,
        SurfaceOptions, device_bounds, draw, fill_box, filter_spill, fit_image,
        gradient_line, inner_box, page_scale, participants, pixel_size,
        resolve_length, ring_path, scale_filter_lengths, to_skia_blend,
        to_skia_color,
    };
    use crate::{
        layout::LayoutResult,
        measure::SceneMeasurer,
        resolve::{
            Fonts, Resolved,
            tests::{RED_PNG, TEST_FAMILY, test_fonts},
        },
    };

    /// A surface that rasterises on the CPU, which is what a test wants.
    ///
    /// Named rather than written out at each of the dozen call sites: a
    /// three-field literal repeated that many times is a change to
    /// [`SurfaceOptions`] rippling through a file that does not care about it.
    const fn on_the_cpu() -> SurfaceOptions {
        SurfaceOptions {
            gpu: false,
            color_type: ColorType::Uint8,
            color_space: ColorSpace::Srgb,
        }
    }

    /// The same, asking for the GPU. Used only where the request is the point.
    const fn on_the_gpu() -> SurfaceOptions {
        SurfaceOptions {
            gpu: true,
            ..on_the_cpu()
        }
    }

    /// The ids `participants` returns, in order, for a test that cares only
    /// about the order.
    fn ordered_ids(scene: &Scene, root: NodeId) -> Vec<NodeId> {
        let node = scene
            .get(root)
            .unwrap_or_else(|| unreachable!("the root is in the scene"));
        participants(scene, root, node)
            .into_iter()
            .map(|step| match step {
                super::Step::EnterClipped { id, .. }
                | super::Step::Enter(id) => id,
                super::Step::Leave { .. } => {
                    unreachable!("participants yields no leave")
                }
            })
            .collect()
    }

    fn box_rect(width: f32, height: f32) -> Rect {
        Rect::new(Point::new(0.0, 0.0), Size::new(width, height))
    }

    #[test]
    fn a_surface_rounds_its_pixel_size_up() {
        // A fractional scale that truncated would lose the last row entirely
        // rather than half of one.
        let size = pixel_size(Size::new(100.0, 50.5), 1.5)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(size, Size::new(150.0, 76.0));
    }

    #[test]
    fn a_surface_with_no_pixels_is_refused() {
        for (size, scale) in [
            (Size::new(0.0, 10.0), 1.0),
            (Size::new(10.0, 0.0), 1.0),
            (Size::new(10.0, 10.0), 0.0),
            (Size::new(f32::NAN, 10.0), 1.0),
            (Size::new(10.0, 10.0), -1.0),
        ] {
            assert!(
                pixel_size(size, scale).is_err(),
                "{size:?} at {scale} should have no pixels"
            );
        }
    }

    /// A surface states its GPU request rather than inheriting one.
    ///
    /// `Canvas::new` takes `CanvasOptions::default()`, which sets `gpu: true`
    /// (`meo-skia-canvas-0.11.0/src/canvas.rs:217`). Before this was explicit
    /// every render asked for the GPU and rasterised on the CPU only because
    /// no backend was compiled — a property of the feature set rather than a
    /// decision. This fails if the field stops being named.
    #[test]
    fn a_surface_asks_for_the_backend_it_was_told_to() {
        let off = Surface::new(Size::new(8.0, 8.0), 1.0, on_the_cpu())
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(!off.gpu());

        let on = Surface::new(Size::new(8.0, 8.0), 1.0, on_the_gpu())
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(on.gpu(), "the request is recorded even where no backend is");
        assert!(format!("{on:?}").contains("gpu"));
    }

    #[test]
    fn a_surface_begins_a_page_per_call_after_the_first() {
        let mut surface =
            Surface::new(Size::new(20.0, 10.0), 2.0, on_the_cpu())
                .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(surface.page_count(), 1, "new() creates the first page");
        assert!((surface.scale() - 2.0).abs() < f32::EPSILON);

        surface
            .begin_page(Size::new(20.0, 10.0))
            .unwrap_or_else(|error| unreachable!("{error}"));
        surface
            .begin_page(Size::new(20.0, 10.0))
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert_eq!(surface.page_count(), 3);
        assert!(!format!("{surface:?}").is_empty());
    }

    #[test]
    fn object_fit_places_the_image_the_way_css_does() {
        // A 4x2 image, ratio 2:1, in a 10x10 box.
        let image = Size::new(4.0, 2.0);
        let target = box_rect(10.0, 10.0);
        let origin = (Length::Points(0.0), Length::Points(0.0));

        // Fill ignores the ratio and takes the box.
        assert_eq!(
            fit_image(image, target, ObjectFit::Fill, origin).size,
            Size::new(10.0, 10.0)
        );
        // Contain fits inside: the width binds at 2.5x, giving 10x5.
        assert_eq!(
            fit_image(image, target, ObjectFit::Contain, origin).size,
            Size::new(10.0, 5.0)
        );
        // Cover fills the box: the height binds at 5x, giving 20x10 and a crop.
        assert_eq!(
            fit_image(image, target, ObjectFit::Cover, origin).size,
            Size::new(20.0, 10.0)
        );
        // None draws it at its own size.
        assert_eq!(
            fit_image(image, target, ObjectFit::None, origin).size,
            image
        );
        // ScaleDown is the smaller of None and Contain, so None here, because
        // the image is already smaller than its box.
        assert_eq!(
            fit_image(image, target, ObjectFit::ScaleDown, origin).size,
            image
        );
        // And Contain when the image is larger than the box.
        assert_eq!(
            fit_image(
                Size::new(40.0, 20.0),
                target,
                ObjectFit::ScaleDown,
                origin
            )
            .size,
            Size::new(10.0, 5.0)
        );
    }

    #[test]
    fn object_position_distributes_the_leftover_space() {
        let image = Size::new(4.0, 2.0);
        let target = box_rect(10.0, 10.0);

        // Contain leaves 0 across and 5 down. A fraction of 0 pins to the
        // start edge, 1 to the end, 0.5 centres.
        let start = fit_image(
            image,
            target,
            ObjectFit::Contain,
            (Length::Percent(0.0), Length::Percent(0.0)),
        );
        assert_eq!(start.origin, Point::new(0.0, 0.0));

        let centred = fit_image(
            image,
            target,
            ObjectFit::Contain,
            (Length::Percent(0.5), Length::Percent(0.5)),
        );
        assert_eq!(centred.origin, Point::new(0.0, 2.5));

        let end = fit_image(
            image,
            target,
            ObjectFit::Contain,
            (Length::Percent(1.0), Length::Percent(1.0)),
        );
        assert_eq!(end.origin, Point::new(0.0, 5.0));
    }

    #[test]
    fn a_degenerate_image_takes_its_box() {
        let target = box_rect(10.0, 10.0);
        let origin = (Length::Points(0.0), Length::Points(0.0));
        for degenerate in [
            Size::new(0.0, 2.0),
            Size::new(4.0, 0.0),
            Size::new(0.0, 0.0),
        ] {
            assert_eq!(
                fit_image(degenerate, target, ObjectFit::Contain, origin),
                target,
                "a {degenerate:?} image has no ratio to preserve"
            );
        }
    }

    #[test]
    fn lengths_resolve_against_their_reference() {
        assert!(
            (resolve_length(Length::Points(7.0), 100.0) - 7.0).abs()
                < f32::EPSILON
        );
        assert!(
            (resolve_length(Length::Percent(0.25), 80.0) - 20.0).abs()
                < f32::EPSILON
        );
        // A percentage of nothing is nothing, which is what an unfilled box
        // leaves for object-position to distribute.
        assert!(
            (resolve_length(Length::Percent(0.5), 0.0) - 0.0).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn a_gradient_line_runs_bottom_to_top_at_zero_degrees() {
        let rect = box_rect(100.0, 100.0);
        let (start, end) = gradient_line(0.0, rect);
        // CSS measures clockwise from twelve o'clock, so zero degrees points
        // up: the line starts below the centre and ends above it.
        assert!(start.y > end.y, "0deg runs upward: {start:?} to {end:?}");
        assert!((start.x - 50.0).abs() < 0.01);
        assert!((end.x - 50.0).abs() < 0.01);

        // 90 degrees points right.
        let (start, end) = gradient_line(90.0, rect);
        assert!(end.x > start.x, "90deg runs rightward");
        assert!((start.y - 50.0).abs() < 0.01);
    }

    #[test]
    fn children_draw_in_z_order_then_document_order() {
        let mut scene = Scene::new(Size::new(10.0, 10.0));
        let mut ids = Vec::new();
        for z in [2_i32, -1, 0, 0] {
            let id = scene
                .push(NodeId::ROOT, Node::container())
                .unwrap_or_else(|error| unreachable!("{error}"));
            if let Some(node) = scene.get_mut(id) {
                node.paint.z_index = Some(z);
            }
            ids.push(id);
        }
        let ordered = ordered_ids(&scene, NodeId::ROOT);

        // -1 first, then the two zeroes in the order they were added, then 2.
        assert_eq!(ordered, vec![ids[1], ids[2], ids[3], ids[0]]);
    }

    #[test]
    fn a_static_child_of_a_block_container_ignores_its_z_index() {
        // CSS 2.1 §9.9.1: `z-index` applies to positioned elements. A block
        // container's in-flow children are not positioned, so an index on one
        // of them names nothing and the children stay in document order.
        let mut scene = Scene::new(Size::new(10.0, 10.0));
        if let Some(root) = scene.get_mut(NodeId::ROOT) {
            root.layout.display = Display::Block;
        }
        let mut ids = Vec::new();
        for z in [5_i32, -5, 0] {
            let id = scene
                .push(NodeId::ROOT, Node::container())
                .unwrap_or_else(|error| unreachable!("{error}"));
            if let Some(node) = scene.get_mut(id) {
                node.paint.z_index = Some(z);
                // Explicit rather than left to the default, so the test still
                // means what it says if the default ever moves.
                node.layout.position_type = PositionType::Static;
            }
            ids.push(id);
        }
        assert_eq!(ordered_ids(&scene, NodeId::ROOT), ids);
    }

    #[test]
    fn a_descendant_paints_after_the_box_it_sits_in() {
        // The invariant a second sort key broke: a static child of a `Block`
        // container is not indexed while the container is, so ranking by that
        // put the child first and the container's background covered it.
        // `display: block` below the page root painted no children at all.
        let mut scene = Scene::new(Size::new(60.0, 40.0));
        let mut panel = Node::container();
        panel.layout.display = Display::Block;
        let panel = scene
            .push(NodeId::ROOT, panel)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let child = scene
            .push(panel, Node::container())
            .unwrap_or_else(|error| unreachable!("{error}"));

        assert_eq!(
            ordered_ids(&scene, NodeId::ROOT),
            vec![panel, child],
            "the panel must paint before the child inside it"
        );
    }

    #[test]
    fn a_negative_child_is_hoisted_out_of_a_parent_that_makes_no_context() {
        // The defect `fixtures/stacking-hoist` pins, as an ordering assertion.
        // A `z_index: -1` child of a parent with no stacking context belongs to
        // the *grandparent's* context, where it paints before the parent's own
        // background — so it comes first in the page's participant list rather
        // than being nested under a parent that paints before it.
        let mut scene = Scene::new(Size::new(40.0, 40.0));
        let parent = scene
            .push(NodeId::ROOT, Node::container())
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut child = Node::container();
        child.layout.position_type = PositionType::Relative;
        child.paint.z_index = Some(-1);
        let child = scene
            .push(parent, child)
            .unwrap_or_else(|error| unreachable!("{error}"));

        assert_eq!(
            ordered_ids(&scene, NodeId::ROOT),
            vec![child, parent],
            "the child should paint before the parent it was hoisted out of"
        );
    }

    #[test]
    fn a_parent_that_makes_a_context_keeps_its_negative_child() {
        // The control cell. `z_index: Some(0)` on a positioned parent is a
        // stacking context where `None` is not — the two sort identically and
        // differ only here — so the child stays inside and the page's list
        // holds the parent alone.
        let mut scene = Scene::new(Size::new(40.0, 40.0));
        let mut parent = Node::container();
        parent.layout.position_type = PositionType::Relative;
        parent.paint.z_index = Some(0);
        let parent = scene
            .push(NodeId::ROOT, parent)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut child = Node::container();
        child.layout.position_type = PositionType::Relative;
        child.paint.z_index = Some(-1);
        let child = scene
            .push(parent, child)
            .unwrap_or_else(|error| unreachable!("{error}"));

        assert_eq!(ordered_ids(&scene, NodeId::ROOT), vec![parent]);
        assert_eq!(ordered_ids(&scene, parent), vec![child]);
    }

    #[test]
    fn a_hoisted_child_still_owes_the_clip_it_was_lifted_past() {
        // Clipping is not stacking. `overflow` creates no context, so the child
        // is hoisted — and it is still clipped by the parent it left, because
        // CSS applies an ancestor's clip however the two are ordered.
        let mut scene = Scene::new(Size::new(40.0, 40.0));
        let mut parent = Node::container();
        parent.layout.overflow = (Overflow::Hidden, Overflow::Hidden);
        let parent = scene
            .push(NodeId::ROOT, parent)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut child = Node::container();
        child.layout.position_type = PositionType::Relative;
        child.paint.z_index = Some(-1);
        let child = scene
            .push(parent, child)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let root = scene
            .get(NodeId::ROOT)
            .unwrap_or_else(|| unreachable!("a new scene has a root"));
        let owed: Vec<Vec<NodeId>> = participants(&scene, NodeId::ROOT, root)
            .into_iter()
            .filter_map(|step| match step {
                super::Step::EnterClipped { id, clips } if id == child => {
                    Some(clips)
                }
                _ => None,
            })
            .collect();

        assert_eq!(
            owed,
            vec![vec![parent]],
            "the hoisted child owes its clipping parent"
        );
    }

    #[test]
    fn an_absolute_child_escapes_an_unpositioned_clipper_and_not_a_positioned_one()
     {
        // `overflow` clips a box's content, and an absolute node is not a box's
        // content merely by sitting inside it. Ported from v1's `b434a23` and
        // measured against it: a 50-wide absolute child in a 20-wide clipper
        // paints 50 columns through a static clipper and 20 through a relative
        // one.
        let owed_by = |clipper_position| {
            let mut scene = Scene::new(Size::new(40.0, 40.0));
            let mut clipper = Node::container();
            clipper.layout.position_type = clipper_position;
            clipper.layout.overflow = (Overflow::Hidden, Overflow::Hidden);
            let clipper = scene
                .push(NodeId::ROOT, clipper)
                .unwrap_or_else(|error| unreachable!("{error}"));

            let mut child = Node::container();
            child.layout.position_type = PositionType::Absolute;
            let child = scene
                .push(clipper, child)
                .unwrap_or_else(|error| unreachable!("{error}"));

            let root = scene
                .get(NodeId::ROOT)
                .unwrap_or_else(|| unreachable!("a new scene has a root"));
            participants(&scene, NodeId::ROOT, root)
                .into_iter()
                .find_map(|step| match step {
                    super::Step::EnterClipped { id, clips } if id == child => {
                        Some(clips)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| unreachable!("the child is a participant"))
        };

        assert!(
            owed_by(PositionType::Static).is_empty(),
            "an unpositioned clipper is not the child's containing block"
        );
        assert_eq!(
            owed_by(PositionType::Relative).len(),
            1,
            "a positioned clipper is, and clips it"
        );
    }

    #[test]
    fn a_relative_child_of_a_block_container_takes_its_z_index() {
        // Measured in Chrome: the one combination of five where a block
        // container's child stacks. `relative` is positioned, and positioned is
        // what CSS 2.1 asks for.
        let mut scene = Scene::new(Size::new(10.0, 10.0));
        if let Some(root) = scene.get_mut(NodeId::ROOT) {
            root.layout.display = Display::Block;
        }
        let mut ids = Vec::new();
        for z in [0_i32, -1] {
            let id = scene
                .push(NodeId::ROOT, Node::container())
                .unwrap_or_else(|error| unreachable!("{error}"));
            if let Some(node) = scene.get_mut(id) {
                node.paint.z_index = Some(z);
                node.layout.position_type = PositionType::Relative;
            }
            ids.push(id);
        }
        assert_eq!(ordered_ids(&scene, NodeId::ROOT), vec![ids[1], ids[0]]);
    }

    #[test]
    fn an_absolute_child_of_a_block_container_takes_its_z_index() {
        // The other half of the same rule: the child is positioned, so the
        // index applies whatever its parent lays out as.
        let mut scene = Scene::new(Size::new(10.0, 10.0));
        if let Some(root) = scene.get_mut(NodeId::ROOT) {
            root.layout.display = Display::Block;
        }
        let mut ids = Vec::new();
        for z in [0_i32, -1] {
            let id = scene
                .push(NodeId::ROOT, Node::container())
                .unwrap_or_else(|error| unreachable!("{error}"));
            if let Some(node) = scene.get_mut(id) {
                node.paint.z_index = Some(z);
                node.layout.position_type = PositionType::Absolute;
            }
            ids.push(id);
        }
        // Second-added draws first: it is positioned, so its -1 counts.
        assert_eq!(ordered_ids(&scene, NodeId::ROOT), vec![ids[1], ids[0]]);
    }

    #[test]
    fn a_grid_item_takes_its_z_index_without_being_positioned() {
        // Grid §6.2, the counterpart to Flexbox §5.4 the first test covers
        // through the default `Display::Flex` root.
        let mut scene = Scene::new(Size::new(10.0, 10.0));
        if let Some(root) = scene.get_mut(NodeId::ROOT) {
            root.layout.display = Display::Grid;
        }
        let mut ids = Vec::new();
        for z in [3_i32, 1] {
            let id = scene
                .push(NodeId::ROOT, Node::container())
                .unwrap_or_else(|error| unreachable!("{error}"));
            if let Some(node) = scene.get_mut(id) {
                node.paint.z_index = Some(z);
            }
            ids.push(id);
        }
        assert_eq!(ordered_ids(&scene, NodeId::ROOT), vec![ids[1], ids[0]]);
    }

    /// The renderer's [`meo_skia_canvas::PixelDepth`] as ours.
    ///
    /// Test-only, and it exists for its exhaustiveness rather than for anything
    /// it computes: `to_skia_color_type` is exhaustive over our enum, so it
    /// catches a variant we add, and this is exhaustive over theirs, so it
    /// catches a variant they add. Upstream's own `all()` is `pub(crate)`
    /// (`meo-skia-canvas-0.11.0/src/pixels.rs:493`) and cannot be walked from
    /// here, so a compile error is the only guard available -- and it is a
    /// stronger one than a runtime conformance test, which would fail after a
    /// build rather than instead of one.
    const fn from_skia_color_type(
        color_type: meo_skia_canvas::PixelDepth,
    ) -> ColorType {
        use meo_skia_canvas::PixelDepth as Skia;
        match color_type {
            Skia::Uint8 => ColorType::Uint8,
            Skia::F16 => ColorType::F16,
            Skia::F32 => ColorType::F32,
            Skia::Alpha8 => ColorType::Alpha8,
            Skia::Gray8 => ColorType::Gray8,
            Skia::R8UNorm => ColorType::R8UNorm,
            Skia::R8G8UNorm => ColorType::R8G8UNorm,
            Skia::A16Float => ColorType::A16Float,
            Skia::A16UNorm => ColorType::A16UNorm,
            Skia::Argb4444 => ColorType::Argb4444,
            Skia::Rgb565 => ColorType::Rgb565,
            Skia::Rgb888x => ColorType::Rgb888x,
            Skia::Bgra8888 => ColorType::Bgra8888,
            Skia::Srgba8888 => ColorType::Srgba8888,
            Skia::N32 => ColorType::N32,
            Skia::Rgba1010102 => ColorType::Rgba1010102,
            Skia::Bgra1010102 => ColorType::Bgra1010102,
            Skia::Rgb101010x => ColorType::Rgb101010x,
            Skia::Bgr101010x => ColorType::Bgr101010x,
            Skia::R16G16Float => ColorType::R16G16Float,
            Skia::R16G16UNorm => ColorType::R16G16UNorm,
            Skia::R16G16B16A16UNorm => ColorType::R16G16B16A16UNorm,
            Skia::F16Norm => ColorType::F16Norm,
        }
    }

    /// The renderer's [`meo_skia_canvas::PixelColorSpace`] as ours.
    const fn from_skia_color_space(
        color_space: meo_skia_canvas::PixelColorSpace,
    ) -> ColorSpace {
        use meo_skia_canvas::PixelColorSpace as Skia;
        match color_space {
            Skia::Srgb => ColorSpace::Srgb,
            Skia::SrgbLinear => ColorSpace::SrgbLinear,
            Skia::DisplayP3 => ColorSpace::DisplayP3,
            Skia::DisplayP3Linear => ColorSpace::DisplayP3Linear,
            Skia::Rec2020 => ColorSpace::Rec2020,
            Skia::Rec2020Linear => ColorSpace::Rec2020Linear,
            Skia::Rec2020Pq => ColorSpace::Rec2020Pq,
            Skia::Rec2020Hlg => ColorSpace::Rec2020Hlg,
        }
    }

    #[test]
    fn the_pixel_enums_are_a_bijection_with_the_renderers() {
        // The two exhaustive matches are the real guard; this walks ours to
        // check the pair actually composes, which neither compiler check does.
        for color_type in ColorType::ALL {
            assert_eq!(
                from_skia_color_type(super::to_skia_color_type(*color_type)),
                *color_type
            );
        }
        for color_space in ColorSpace::ALL {
            assert_eq!(
                from_skia_color_space(super::to_skia_color_space(*color_space)),
                *color_space
            );
        }
    }

    #[test]
    fn every_blend_mode_maps_to_one_of_skia_s() {
        // A mode that mapped to the wrong constant would compose wrongly and
        // never fail; that the map is total is what a test can check here.
        let mut seen = std::collections::HashSet::new();
        for mode in BlendMode::ALL {
            assert!(
                seen.insert(format!("{:?}", to_skia_blend(*mode))),
                "{mode:?} maps onto a constant another mode already uses"
            );
        }
        assert_eq!(seen.len(), BlendMode::ALL.len());
    }

    #[test]
    fn drawing_a_layout_that_names_no_page_is_refused() {
        let scene = Scene::new(Size::new(10.0, 10.0));
        let fonts = Fonts::new();
        let resolved = Resolved::new(&scene, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut surface = Surface::new(scene.size, 1.0, on_the_cpu())
            .unwrap_or_else(|error| unreachable!("{error}"));

        let empty = LayoutResult::default();
        assert!(draw(&mut surface, &resolved, &empty, &mut measurer).is_err());
    }

    /// Draws a scene exercising every node kind and most of the paint surface.
    ///
    /// This asserts only that the traversal completes without error. It cannot
    /// assert what was drawn: executing a fill proves the call was made, not
    /// that the pixels are right. Everything visual here is covered by golden
    /// fixtures or not at all — see the module documentation.
    /// Renders one bordered box and returns its pixels, eight bits per
    /// channel.
    fn bordered_corner(top: f32, left: f32, radius: f32) -> Vec<u8> {
        use meo_canvas_scene::style::paint::Color;

        let mut scene = Scene::new(Size::new(60.0, 60.0));
        if let Some(root) = scene.get_mut(NodeId::ROOT) {
            root.paint = PaintStyle {
                // White, so "the page" and "the box" are told apart by the
                // box's own fill rather than by position.
                background_color: Color::rgb(255, 255, 255),
                ..PaintStyle::default()
            };
        }
        let box_id = scene
            .push(NodeId::ROOT, Node::new(NodeKind::Box))
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(box_id) {
            node.layout.size = (
                meo_canvas_scene::style::Dimension::Points(60.0),
                meo_canvas_scene::style::Dimension::Points(60.0),
            );
            node.layout.border = meo_canvas_scene::Sides {
                top,
                right: 6.0,
                bottom: 6.0,
                left,
            };
            node.paint = PaintStyle {
                background_color: FILL,
                border_radius: meo_canvas_scene::Corners::all(radius),
                border_color_all: BORDER,
                border_color: meo_canvas_scene::Sides {
                    top: Some(BORDER),
                    right: Some(BORDER),
                    bottom: Some(BORDER),
                    left: Some(BORDER),
                },
                ..PaintStyle::default()
            };
        }

        let fonts = test_fonts();
        let resolved = Resolved::new(&scene, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut surface = Surface::new(scene.size, 1.0, on_the_cpu())
            .unwrap_or_else(|error| unreachable!("{error}"));
        let page = scene.pages[0];
        let solved = crate::layout::solve(&scene, page, &mut measurer)
            .unwrap_or_else(|error| unreachable!("{error}"));
        draw(&mut surface, &resolved, &solved, &mut measurer)
            .unwrap_or_else(|error| unreachable!("{error}"));

        surface
            .context()
            .get_image_data(0.0, 0.0, 60.0, 60.0)
            .unwrap_or_else(|error| unreachable!("{error}"))
            .into_pixels()
    }

    /// The box's own background, which is what shows through a gap in a ring.
    const FILL: meo_canvas_scene::style::paint::Color =
        meo_canvas_scene::style::paint::Color::rgb(0, 160, 0);

    /// The border colour, one for every edge: the question here is whether
    /// the ring is *covered*, and four colours would only make the reading
    /// harder.
    const BORDER: meo_canvas_scene::style::paint::Color =
        meo_canvas_scene::style::paint::Color::rgb(200, 40, 40);

    /// The ring the border *should* cover, drawn as one fill.
    ///
    /// The oracle the per-edge path is checked against. `ring_path` is what
    /// the uniform path already uses and what the per-edge path fills through
    /// its clips, so with one colour on every edge the two must agree: the
    /// division decides which colour a part of the ring takes, and where
    /// every part takes the same colour it must not be visible at all.
    fn reference_ring(top: f32, left: f32, radius: f32) -> Vec<u8> {
        use meo_canvas_scene::{Point, style::paint::Color};

        let rect = Rect::new(Point::new(0.0, 0.0), Size::new(60.0, 60.0));
        let widths = meo_canvas_scene::Sides {
            top,
            right: 6.0,
            bottom: 6.0,
            left,
        };
        let paint = PaintStyle {
            background_color: FILL,
            border_radius: meo_canvas_scene::Corners::all(radius),
            border_color_all: BORDER,
            ..PaintStyle::default()
        };

        let mut surface =
            Surface::new(Size::new(60.0, 60.0), 1.0, on_the_cpu())
                .unwrap_or_else(|error| unreachable!("{error}"));
        let context = surface.context();
        context.set_fill_style(to_skia_color(Color::rgb(255, 255, 255)));
        context.fill_rect(0.0, 0.0, 60.0, 60.0);
        context.set_fill_style(to_skia_color(FILL));
        fill_box(context, &paint, rect)
            .unwrap_or_else(|error| unreachable!("{error}"));
        context.set_fill_style(to_skia_color(BORDER));
        let ring = ring_path(&paint, rect, inner_box(rect, widths), widths)
            .unwrap_or_else(|error| unreachable!("{error}"));
        context.fill_path(&ring, SkiaFillRule::EvenOdd);

        context
            .get_image_data(0.0, 0.0, 60.0, 60.0)
            .unwrap_or_else(|error| unreachable!("{error}"))
            .into_pixels()
    }

    /// Whether a pixel is unmistakably the border rather than the fill.
    fn is_border(pixels: &[u8], at: usize) -> bool {
        i16::from(pixels[at]) - i16::from(pixels[at + 1]) > 40
    }

    /// Whether a pixel is unmistakably the fill rather than the border.
    fn is_fill(pixels: &[u8], at: usize) -> bool {
        i16::from(pixels[at + 1]) - i16::from(pixels[at]) > 40
    }

    /// A corner between two edges of different widths leaves no gap in the
    /// ring.
    ///
    /// # What the oracle is
    ///
    /// Not the picture we drew last time, and not a row of pixels read by
    /// eye: **the ring drawn as a single fill**, which is the same
    /// [`ring_path`] the per-edge pass fills through its clips. Every edge is
    /// given the same colour, so the division between them must leave no
    /// trace -- and where the reference says "border", a gap in the division
    /// shows as the box's own fill.
    ///
    /// The comparison is one-sided and deliberately loose: seams between two
    /// clipped fills antialias differently from one unclipped fill, and that
    /// difference is not the question. Only "the reference has border here and
    /// we have fill" is.
    ///
    /// # Why these five pairs
    ///
    /// A rule that special-cased zero would pass `0` against `2` and fail at
    /// `1` against `20`, where the division should sit almost entirely on the
    /// thick side. Both orders, because the division is **not** symmetric: it
    /// runs from the outer corner point to the inner one, so swapping the two
    /// widths reflects it. Equal widths are the control -- the 45-degree
    /// mitre every bordered fixture already draws, which must not move.
    #[test]
    fn a_corner_between_unequal_edges_leaves_no_gap() {
        const RADIUS: f32 = 20.0;
        for (top, left) in
            [(2.0, 0.0), (0.0, 2.0), (1.0, 20.0), (20.0, 1.0), (6.0, 6.0)]
        {
            let drawn = bordered_corner(top, left, RADIUS);
            let reference = reference_ring(top, left, RADIUS);
            let mut gaps = Vec::new();
            for y in 0..60_usize {
                for x in 0..60_usize {
                    let at = (y * 60 + x) * 4;
                    if is_border(&reference, at) && is_fill(&drawn, at) {
                        gaps.push((x, y));
                    }
                }
            }
            assert!(
                gaps.is_empty(),
                "top {top}, left {left}: {} pixels of ring are painted as \
                 fill, the first at {:?}",
                gaps.len(),
                gaps.first()
            );
        }
    }

    #[test]
    fn a_scene_of_every_kind_draws_without_error() {
        let mut scene = Scene::new(Size::new(200.0, 120.0));

        let text = scene
            .push(NodeId::ROOT, Node::text("baseline placed"))
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(node) = scene.get_mut(text) {
            node.text.font_family = Some(TEST_FAMILY.to_owned());
            node.text.font_size = Some(18.0);
        }

        scene
            .push(
                NodeId::ROOT,
                Node::new(NodeKind::Image {
                    source: ImageSource::Bytes(RED_PNG.to_vec()),
                    fit: ObjectFit::Cover,
                    position: (Length::Percent(0.5), Length::Percent(0.5)),
                    frame: None,
                }),
            )
            .unwrap_or_else(|error| unreachable!("{error}"));

        scene
            .push(
                NodeId::ROOT,
                Node::new(NodeKind::Path {
                    data: "M0 0 L20 20 L0 20 Z".to_owned(),
                    fill: Some(meo_canvas_scene::node::PathPaint::Solid(
                        meo_canvas_scene::style::paint::Color::BLACK,
                    )),
                    stroke: Some(meo_canvas_scene::node::PathPaint::Solid(
                        meo_canvas_scene::style::paint::Color::rgb(1, 2, 3),
                    )),
                    line_width: 2.0,
                    fill_rule:
                        meo_canvas_scene::style::effect::FillRule::EvenOdd,
                    line_cap: meo_canvas_scene::node::LineCap::Round,
                    line_join: meo_canvas_scene::node::LineJoin::Bevel,
                    line_dash: vec![3.0, 1.0],
                    line_dash_offset: 0.5,
                }),
            )
            .unwrap_or_else(|error| unreachable!("{error}"));

        if let Some(root) = scene.get_mut(NodeId::ROOT) {
            root.paint = PaintStyle {
                background_color: meo_canvas_scene::style::paint::Color::rgb(
                    250, 250, 250,
                ),
                opacity: 0.9,
                blend_mode: BlendMode::Multiply,
                border_radius: meo_canvas_scene::Corners::all(4.0),
                ..PaintStyle::default()
            };
            root.layout.border = meo_canvas_scene::Sides::all(1.0);
        }

        let fonts = test_fonts();
        let resolved = Resolved::new(&scene, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut surface = Surface::new(scene.size, 2.0, on_the_cpu())
            .unwrap_or_else(|error| unreachable!("{error}"));

        let page = scene.pages[0];
        let solved = crate::layout::solve(&scene, page, &mut measurer)
            .unwrap_or_else(|error| unreachable!("{error}"));
        draw(&mut surface, &resolved, &solved, &mut measurer)
            .unwrap_or_else(|error| unreachable!("{error}"));
    }
    /// Drives the traversal over the features the simple scene does not reach:
    /// transforms, all three gradient kinds, shadows, per-edge borders,
    /// overflow clipping, filters and background images.
    ///
    /// Like `a_scene_of_every_kind_draws_without_error`, this asserts only
    /// that the walk completes. It exists so those paths execute at all — an
    /// arm that panics or returns an error is caught here; an arm that draws
    /// the wrong pixels is caught by a fixture and by nothing in this file.
    #[test]
    fn the_decorated_paths_draw_without_error() {
        use meo_canvas_scene::style::paint::{Color, GradientStop};

        let stops = vec![
            GradientStop {
                offset: 0.0,
                color: Color::rgb(255, 0, 0),
            },
            GradientStop {
                offset: 1.0,
                color: Color::rgba(0, 0, 255, 128),
            },
        ];

        draw_each_gradient_geometry(&stops);
    }

    /// Every gradient geometry, drawn. Split from the decorated-path test above
    /// only because one function covering both runs past the line limit.
    fn draw_each_gradient_geometry(
        stops: &[meo_canvas_scene::style::paint::GradientStop],
    ) {
        use meo_canvas_scene::{
            Corners, Sides,
            style::{
                effect::{BoxShadow, Effects, Transform},
                layout::Overflow,
                paint::{BackgroundImage, BackgroundRepeat, Color, Gradient},
            },
        };

        let geometries = [
            GradientGeometry::Linear {
                direction: LinearDirection::Angle(45.0),
            },
            // The endpoint form, which no other test here exercises and which
            // is the reason the geometry moved onto the kinds.
            GradientGeometry::Linear {
                direction: LinearDirection::Between {
                    start: (Length::Percent(0.25), Length::ZERO),
                    end: (Length::Percent(0.75), Length::Percent(1.0)),
                },
            },
            GradientGeometry::Radial {
                at: GradientGeometry::CENTER,
            },
            GradientGeometry::Conic {
                at: (Length::Percent(0.25), Length::Percent(0.75)),
                from: 45.0,
            },
        ];

        for geometry in geometries {
            let mut scene = Scene::new(Size::new(80.0, 60.0));
            let child = scene
                .push(NodeId::ROOT, Node::container())
                .unwrap_or_else(|error| unreachable!("{error}"));

            if let Some(node) = scene.get_mut(child) {
                node.paint.gradient = Some(Gradient {
                    geometry,
                    stops: stops.to_vec(),
                });
                node.paint.border_radius = Corners::all(3.0);
                // Per-edge widths and colours, so the border takes the
                // edge-by-edge path rather than the single-stroke one.
                node.paint.border_color = Sides {
                    top: Some(Color::BLACK),
                    right: None,
                    bottom: Some(Color::rgb(1, 2, 3)),
                    left: None,
                };
                node.layout.border = Sides {
                    top: 2.0,
                    right: 1.0,
                    bottom: 3.0,
                    left: 0.0,
                };
                node.layout.overflow = (Overflow::Hidden, Overflow::Scroll);
                node.layout.size = (
                    meo_canvas_scene::Dimension::Points(40.0),
                    meo_canvas_scene::Dimension::Points(30.0),
                );
                node.effects = Effects {
                    transform: Some(Transform {
                        translate_x: Length::Points(2.0),
                        translate_y: Length::Percent(0.1),
                        rotate_degrees: 30.0,
                        scale_x: 1.2,
                        scale_y: 0.8,
                        origin: Transform::ORIGIN_CENTER,
                    }),
                    box_shadows: vec![
                        BoxShadow {
                            offset_x: 1.0,
                            offset_y: 2.0,
                            blur: 3.0,
                            spread: 1.0,
                            color: Color::rgba(0, 0, 0, 64),
                            ..BoxShadow::default()
                        },
                        // Inset is a documented no-op; running it proves the
                        // early return is taken rather than a wrong shadow.
                        BoxShadow {
                            inset: true,
                            ..BoxShadow::default()
                        },
                    ],
                    filter: Some("blur(1px)".to_owned()),
                    ..Effects::default()
                };
                node.paint.background_image = Some(BackgroundImage {
                    source: ImageSource::Bytes(RED_PNG.to_vec()),
                    repeat: BackgroundRepeat::NoRepeat,
                    size: meo_canvas_scene::style::paint::BackgroundSize::AUTO,
                    position: (Length::ZERO, Length::ZERO),
                });
            }

            let fonts = Fonts::new();
            let resolved = Resolved::new(&scene, &fonts)
                .unwrap_or_else(|error| unreachable!("{error}"));
            let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
                .unwrap_or_else(|error| unreachable!("{error}"));
            let mut surface = Surface::new(scene.size, 1.0, on_the_cpu())
                .unwrap_or_else(|error| unreachable!("{error}"));
            let solved =
                crate::layout::solve(&scene, scene.pages[0], &mut measurer)
                    .unwrap_or_else(|error| unreachable!("{error}"));
            draw(&mut surface, &resolved, &solved, &mut measurer)
                .unwrap_or_else(|error| unreachable!("{geometry:?}: {error}"));
        }
    }

    /// A uniform border takes the single-stroke path, and a path node with
    /// neither fill nor stroke draws nothing without erroring.
    #[test]
    fn the_uniform_border_and_empty_path_arms_run() {
        use meo_canvas_scene::{Corners, Sides, style::effect::FillRule};

        let mut scene = Scene::new(Size::new(40.0, 40.0));
        if let Some(root) = scene.get_mut(NodeId::ROOT) {
            root.layout.border = Sides::all(2.0);
            root.paint.border_radius = Corners::all(0.0);
        }
        scene
            .push(
                NodeId::ROOT,
                Node::new(NodeKind::Path {
                    data: "M0 0 L5 5".to_owned(),
                    fill: None,
                    stroke: None,
                    line_width: 1.0,
                    fill_rule: FillRule::NonZero,
                    line_cap: meo_canvas_scene::node::LineCap::Butt,
                    line_join: meo_canvas_scene::node::LineJoin::Miter,
                    line_dash: Vec::new(),
                    line_dash_offset: 0.0,
                }),
            )
            .unwrap_or_else(|error| unreachable!("{error}"));

        let fonts = Fonts::new();
        let resolved = Resolved::new(&scene, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut surface = Surface::new(scene.size, 1.0, on_the_cpu())
            .unwrap_or_else(|error| unreachable!("{error}"));
        let solved =
            crate::layout::solve(&scene, scene.pages[0], &mut measurer)
                .unwrap_or_else(|error| unreachable!("{error}"));
        draw(&mut surface, &resolved, &solved, &mut measurer)
            .unwrap_or_else(|error| unreachable!("{error}"));
    }

    /// A malformed SVG path is an error rather than a silent skip.
    #[test]
    fn a_path_that_is_not_svg_is_refused() {
        use meo_canvas_scene::style::effect::FillRule;

        let mut scene = Scene::new(Size::new(10.0, 10.0));
        scene
            .push(
                NodeId::ROOT,
                Node::new(NodeKind::Path {
                    data: "this is not path data".to_owned(),
                    fill: Some(meo_canvas_scene::node::PathPaint::Solid(
                        meo_canvas_scene::style::paint::Color::BLACK,
                    )),
                    stroke: None,
                    line_width: 1.0,
                    fill_rule: FillRule::NonZero,
                    line_cap: meo_canvas_scene::node::LineCap::Butt,
                    line_join: meo_canvas_scene::node::LineJoin::Miter,
                    line_dash: Vec::new(),
                    line_dash_offset: 0.0,
                }),
            )
            .unwrap_or_else(|error| unreachable!("{error}"));

        let fonts = Fonts::new();
        let resolved = Resolved::new(&scene, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut surface = Surface::new(scene.size, 1.0, on_the_cpu())
            .unwrap_or_else(|error| unreachable!("{error}"));
        let solved =
            crate::layout::solve(&scene, scene.pages[0], &mut measurer)
                .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(draw(&mut surface, &resolved, &solved, &mut measurer).is_err());
    }

    /// A gradient painted onto a path, which is the other `set_paint` arm.
    #[test]
    fn a_path_painted_with_a_gradient_draws() {
        use meo_canvas_scene::{
            node::PathPaint,
            style::{
                effect::FillRule,
                paint::{Color, Gradient, GradientStop},
            },
        };

        let gradient = Gradient {
            geometry: GradientGeometry::Linear {
                direction: LinearDirection::Angle(0.0),
            },
            stops: vec![
                GradientStop {
                    offset: 0.0,
                    color: Color::BLACK,
                },
                GradientStop {
                    offset: 1.0,
                    color: Color::rgb(255, 255, 255),
                },
            ],
        };

        let mut scene = Scene::new(Size::new(30.0, 30.0));
        scene
            .push(
                NodeId::ROOT,
                Node::new(NodeKind::Path {
                    data: "M0 0 L10 0 L10 10 Z".to_owned(),
                    fill: Some(PathPaint::Gradient(gradient.clone())),
                    stroke: Some(PathPaint::Gradient(gradient)),
                    line_width: 1.0,
                    fill_rule: FillRule::NonZero,
                    line_cap: meo_canvas_scene::node::LineCap::Butt,
                    line_join: meo_canvas_scene::node::LineJoin::Miter,
                    line_dash: Vec::new(),
                    line_dash_offset: 0.0,
                }),
            )
            .unwrap_or_else(|error| unreachable!("{error}"));

        let fonts = Fonts::new();
        let resolved = Resolved::new(&scene, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut surface = Surface::new(scene.size, 1.0, on_the_cpu())
            .unwrap_or_else(|error| unreachable!("{error}"));
        let solved =
            crate::layout::solve(&scene, scene.pages[0], &mut measurer)
                .unwrap_or_else(|error| unreachable!("{error}"));
        draw(&mut surface, &resolved, &solved, &mut measurer)
            .unwrap_or_else(|error| unreachable!("{error}"));
    }

    #[test]
    fn only_lengths_are_rewritten_for_the_device() {
        // The factor and the angle mean the same at any resolution; the two
        // radii do not.
        assert_eq!(
            scale_filter_lengths(
                "blur(4px) saturate(150%) hue-rotate(90deg)",
                2.0
            ),
            "blur(8px) saturate(150%) hue-rotate(90deg)"
        );
        // Every length in a `drop-shadow`, offsets included.
        assert_eq!(
            scale_filter_lengths("drop-shadow(2px -3px 4px black)", 2.0),
            "drop-shadow(4px -6px 8px black)"
        );
        // A nested colour is not a filter, so nothing inside it is a length
        // to scale -- and `50%` is not a `px` in any case.
        assert_eq!(
            scale_filter_lengths(
                "drop-shadow(2px 2px 4px rgb(0 0 0 / 50%))",
                2.0
            ),
            "drop-shadow(4px 4px 8px rgb(0 0 0 / 50%))"
        );
        // At the identity the string comes back untouched rather than
        // reformatted: `blur(4.0px)` for `blur(4px)` is the same filter and a
        // different string, and the string is what the binding echoes back.
        assert_eq!(scale_filter_lengths("blur(4px)", 1.0), "blur(4px)");
    }

    #[test]
    fn spill_covers_three_deviations_of_every_blur() {
        assert!((filter_spill("blur(4px)") - 12.0).abs() < f32::EPSILON);
        // A chain reaches as far as its parts together.
        assert!(
            (filter_spill("blur(2px) blur(3px)") - 15.0).abs() < f32::EPSILON
        );
        // The offset moves the shadow on top of what the blur spreads.
        assert!(
            (filter_spill("drop-shadow(6px 2px 1px black)") - 9.0).abs()
                < f32::EPSILON
        );
        // Nothing that carries no length reaches anywhere.
        assert!(
            filter_spill("grayscale(1) saturate(200%)").abs() < f32::EPSILON
        );
        assert!(filter_spill("none").abs() < f32::EPSILON);
    }

    #[test]
    fn a_rotation_is_bounded_by_all_four_corners() {
        // A quarter turn about the origin sends the top-left corner to the
        // top-right, so neither the left edge nor the top survives the map.
        let quarter = Affine {
            a: 0.0,
            b: 1.0,
            c: -1.0,
            d: 0.0,
            tx: 0.0,
            ty: 0.0,
        };
        let bounds = device_bounds(
            quarter,
            Rect::new(Point::new(10.0, 20.0), Size::new(30.0, 40.0)),
        );
        assert!((bounds.origin.x - -60.0).abs() < f32::EPSILON);
        assert!((bounds.origin.y - 10.0).abs() < f32::EPSILON);
        assert!((bounds.size.width - 40.0).abs() < f32::EPSILON);
        assert!((bounds.size.height - 30.0).abs() < f32::EPSILON);
    }

    #[test]
    fn the_page_scale_is_one_number_for_two_axes() {
        let stretched = Affine {
            a: 3.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: 0.0,
            ty: 0.0,
        };
        assert!((page_scale(stretched) - 2.0).abs() < f32::EPSILON);
        // A degenerate matrix would otherwise scale every length to nothing,
        // which turns a blur into a filter that draws the backdrop back
        // unchanged.
        assert!((page_scale(Affine::IDENTITY) - 1.0).abs() < f32::EPSILON);
        let collapsed = Affine {
            a: 0.0,
            b: 0.0,
            c: 0.0,
            d: 0.0,
            tx: 0.0,
            ty: 0.0,
        };
        assert!((page_scale(collapsed) - 1.0).abs() < f32::EPSILON);
    }
}
