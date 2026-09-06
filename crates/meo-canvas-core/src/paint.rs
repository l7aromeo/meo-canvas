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
    ColorSpace, ColorType, Corners, OnImageError, Rect, Sides, Size,
    node::{Node, NodeId, NodeKind, PathPaint},
    style::{
        Dimension, Length, PaintOrder,
        effect::{BoxShadow, FillRule, Mask, MaskShape, Transform},
        layout::{Display, Overflow, PositionType},
        paint::{
            BackgroundImage, BackgroundRepeat, BackgroundSize, BlendMode,
            BorderStyle, Color, Gradient, GradientGeometry, LinearDirection,
            ObjectFit, PaintStyle,
        },
        text::{TextAlign, TextDecoration, VerticalAlign},
    },
};
use meo_skia_canvas::{
    Affine, BlendMode as SkiaBlendMode, Canvas, CanvasOptions, Context2D,
    FillRule as SkiaFillRule, GradientInterpolation,
    GradientStop as SkiaGradientStop, Image as SkiaImage, Path2D, PathBuilder,
    PixelColorSpace, PixelDepth, PixelExportOptions, PixelFormat, Point,
    RgbaLinear, Shader, StrokeCap, StrokeJoin, TextAlign as SkiaTextAlign,
    TextBaseline as SkiaTextBaseline, TextDecoration as SkiaTextDecoration,
    TextDecorationStyle as SkiaDecorationStyle,
    filter::{BlurStyle, MaskFilter},
};

use crate::{
    Error,
    layout::{LayoutResult, is_containing_block, used_border},
    lines::{Line, Metrics, Run, RunStyle, line_width},
    measure::SceneMeasurer,
    resolve::{DecodedImage, Resolved, ResolvedText},
};

/// Opacity at or above which a node needs no isolation layer.
///
/// Exactly one. Below it the node's children must be composited together and
/// then faded as a group, or overlapping siblings show through each other; at
/// it there is nothing to fade and the layer would cost an offscreen surface
/// for no visible difference.
const OPAQUE: f32 = 1.0;

/// Degrees in a full turn, for converting a scene's rotation to radians.
const DEGREES_PER_TURN: f32 = 360.0;

/// The quarter turn between where CSS starts a conic sweep and where a canvas
/// does -- twelve o'clock against three.
const QUARTER_TURN_DEGREES: f32 = 90.0;

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
                    context,
                    resolved,
                    measurer,
                    id,
                    node,
                    rect,
                    layout.content(id).unwrap_or(rect),
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
                    context,
                    resolved,
                    measurer,
                    id,
                    node,
                    rect,
                    layout.content(id).unwrap_or(rect),
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
    /// One participant, the key it sorts by, and what it was hoisted past.
    struct Ranked {
        id: NodeId,
        /// One `(z, layer, order)` triple per step from the context root down
        /// to this node, compared in order. See the sort at the end of this
        /// function.
        key: Key,
        /// Clipping ancestors between this node and the context root,
        /// outermost first. See [`Step::EnterClipped`].
        clips: Vec<NodeId>,
    }

    /// A node's place in its context's order: one `(z, layer, order)` triple
    /// per step from the context root down to it.
    type Key = Vec<(i32, u8, u32)>;

    /// A node still to be walked: where it is, whose child it is, the clips it
    /// owes, and the key of the ancestor it hangs from.
    type Pending = (NodeId, NodeId, Vec<NodeId>, Key);

    /// Which of CSS's painting steps a node belongs to within its parent.
    ///
    /// Appendix E orders a stacking context's contents: in-flow
    /// non-positioned descendants at steps 3 and 5, then at step 6
    /// **everything positioned and every child stacking context with a
    /// `z_index` of zero**. So the upper band is not "positioned" alone.
    ///
    /// The case that separates the two readings is a **static flex or grid
    /// item with `z_index: 0`**. Flexbox §5.4 gives such an item a stacking
    /// context even though it is not positioned, which puts it at step 6,
    /// while a static item with `auto` paints as an inline block at step 5 --
    /// so the indexed one is above whatever the document order. Measured
    /// against Chrome, which disagreed on exactly those two rows when this
    /// asked about position alone.
    const fn layer(node: &Node, indexed: bool) -> u8 {
        let positioned =
            !matches!(node.layout.position_type, PositionType::Static);
        if positioned || (indexed && node.paint.z_index.is_some()) {
            1
        } else {
            0
        }
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
    let mut pending: Vec<Pending> = node
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
            (*child, root, owed, Vec::new())
        })
        .collect();

    while let Some((id, parent_id, clips, ancestry)) = pending.pop() {
        let (Some(source), Some(parent)) =
            (scene.get(id), scene.get(parent_id))
        else {
            continue;
        };

        // **A non-zero `z_index` joins the context root's own ordering**
        // rather than its ancestor's place in it. CSS puts such a child
        // stacking context at step 2 or step 7 of the *context* -- before
        // every in-flow descendant or after every positioned one -- so a
        // `z_index: -1` child of a plain block paints beneath that block's
        // background, which is the hoist `fixtures/stacking-hoist` exists for.
        // Starting its key afresh is what lets it overtake its own ancestor.
        //
        // **Zero is not one of those, and that is the whole of this
        // distinction.** Step 6 holds positioned descendants with `auto` *and*
        // child stacking contexts with `0`, together, in tree order -- so
        // `z_index: 0` and `z_index: auto` do not rank against each other at
        // all and the later box wins. Measured against Chrome, where ranking
        // the explicit zero above the automatic one disagreed in three rows,
        // one per container kind. The two differ in whether a stacking
        // context is established, which is [`establishes_stacking_context`]'s
        // question and not this one.
        let indexed = stacks_by_z_index(parent, source);
        let explicit =
            indexed && source.paint.z_index.is_some_and(|index| index != 0);
        // The third component is the node's place in the pre-order walk,
        // which is document order. Without it a *sibling's* descendant
        // compares against a shorter key as though it were an ancestor: three
        // absolutely-positioned panels and the twelve stripes behind them all
        // key as `(0, 1)`, and the stripes' children would sort after the
        // panels and paint over them.
        //
        // The cast is exact: the arena is bounded by `MAX_NODES`, a `u32`.
        let own = (
            if indexed {
                source.paint.z_index.unwrap_or(0)
            } else {
                0
            },
            layer(source, indexed),
            found.len() as u32,
        );
        let key = if explicit {
            vec![own]
        } else {
            let mut key = ancestry.clone();
            key.push(own);
            key
        };

        found.push(Ranked {
            id,
            key: key.clone(),
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
                pending.push((*child, id, inherited, key.clone()));
            }
        }
    }

    // **By the path, not by a flat key.** Each node's key is one `(z, layer)`
    // pair per step from the context root down to it, compared in order, and
    // the sort is stable so tree order decides the rest.
    //
    // A flat key was tried first and is wrong twice over. Ranking by `z` alone
    // loses CSS's rule that a positioned box paints above an in-flow one
    // whatever the document order — 66 of 231 rows of the paint-order table
    // disagreed with Chrome on exactly that, every one of them
    // `relative`, `absolute` or `sticky` against `static`. And adding a
    // "positioned" key *flat* sorts a static grandchild before its own
    // positioned parent, whose background then covers it: `display: block`
    // below the page root painted no children at all.
    //
    // The path key has both properties by construction. An ancestor's key is a
    // strict prefix of its descendant's, and a prefix sorts first, so a
    // descendant can never overtake the box it sits in; and within one parent
    // the last pair decides, which is where CSS's ordering belongs.
    found.sort_by(|left, right| left.key.cmp(&right.key));
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
/// A [`PositionType::Fixed`] node escapes every clipper **but the one that
/// captures it**. Its containing block is not any positioned ancestor — it is
/// the transform that captures it, or nothing at all — so neither a static nor
/// a relative box cuts one where either would cut an absolute node.
///
/// Ported from v1's `4f542d8`. Measured before: a 50-wide fixed child in a
/// 20-wide clipper painted 20 columns under a static clipper and 20 under a
/// relative one, where both should be 50.
///
/// # Capture and clip are one rule
///
/// Both arms ask [`is_containing_block`], which is the same predicate
/// [`crate::layout`] attaches an out-of-flow box with — deliberately, because
/// a box is clipped by its containing block's `overflow` and by nothing it was
/// merely written inside. They were two rules for an hour and Chrome found it
/// in ten rows: a transformed clipper with `overflow: hidden` placed an
/// out-of-flow child exactly where the browser does and then **drew it whole**,
/// because layout knew the transform had captured it and paint still thought
/// every fixed box escapes everything.
const fn escapes_clip(clipper: &Node, child: &Node) -> bool {
    match child.layout.position_type {
        // Only a transform captures a fixed box, so only a transform clips
        // one. A positioned clipper is not its containing block and does not
        // cut it.
        PositionType::Fixed => clipper.effects.transform.is_none(),
        PositionType::Absolute => !is_containing_block(clipper),
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
    measurer: &mut SceneMeasurer<'_>,
    id: NodeId,
    node: &Node,
    rect: Rect,
    content: Rect,
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
        paint_kind(context, resolved, measurer, id, node, rect, content)
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
    // Outer shadows first, and **clipped out of the border box** rather than
    // merely covered by it -- see `draw_box_shadow`.
    //
    // Reversed, because CSS Backgrounds and Borders 3 §7.1 paints a shadow
    // list **front to back**: the first one written is the one on top, so it
    // has to be drawn last. Drawn in list order the last one won instead.
    // Measured: `10px 0 0 red, 10px 0 0 blue` reads red beside the box in
    // Chrome and read blue here.
    for shadow in node.effects.box_shadows.iter().rev().filter(|s| !s.inset) {
        draw_box_shadow(context, paint, rect, shadow)?;
    }

    if !paint.background_color.is_invisible() {
        context.set_fill_style(to_skia_color(paint.background_color));
        fill_box(context, paint, rect)?;
    }

    if let Some(gradient) = paint.gradient.as_ref() {
        let (shader, squash) = build_gradient(gradient, rect)?;
        context.set_fill_shader(&shader);
        fill_with_gradient(context, squash, rect, |context| {
            box_path(context, paint.border_radius, rect)
        })?;
    }

    // Inset shadows after the background and before the border, which is where
    // CSS puts them. Drawn with the outer ones they were painted **and then
    // covered by the very background they fall on**, which is why the arm
    // looked unimplemented from the outside.
    // Reversed for the same reason as the outer ones above, and measured the
    // same way: two inset shadows offset right land their ink on the left
    // inner edge, and Chrome shows the first-written colour there.
    for shadow in node.effects.box_shadows.iter().rev().filter(|s| s.inset) {
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
#[expect(
    clippy::match_same_arms,
    reason = "`Box` draws nothing because a box with no paint is nothing to \
              draw; the `#[non_exhaustive]` arm draws nothing because it has \
              no idea what to draw. Same body, different claims."
)]
fn paint_kind(
    context: &mut Context2D,
    resolved: &Resolved<'_>,
    measurer: &mut SceneMeasurer<'_>,
    id: NodeId,
    node: &Node,
    rect: Rect,
    content: Rect,
) -> Result<(), Error> {
    match &node.kind {
        NodeKind::Box => Ok(()),
        NodeKind::Text { .. } => draw_text(context, measurer, id, node, rect),
        NodeKind::Image { fit, position, .. } => {
            // **The arm an unresolved image already took.** This returned
            // `Ok(())` before the placeholder existed and still does under
            // `Ignore`, so an image that decoded does not reach a single new
            // branch: the `Option` match below is the one every image node has
            // always paid for, and everything the placeholder costs is inside
            // the arm a resolved image never enters.
            let Some(image) = resolved.image(id).map(DecodedImage::inner)
            else {
                if resolved.scene().on_image_error == OnImageError::Placeholder
                {
                    draw_missing(
                        context,
                        inner_radii(node, rect, content),
                        content,
                    )?;
                }
                return Ok(());
            };
            let intrinsic =
                Size::new(image.width() as f32, image.height() as f32);
            // **Clipped, because CSS clips replaced content and this did not.**
            // `fit_image`'s own note says the destination may be larger than
            // the box and "the caller crops"; this caller did not, so a
            // `cover` whose aspect did not match its box painted outside the
            // element -- reported from a real consumer as a 152x186 avatar in
            // a 26x26 frame painting 26x32.
            //
            // **Not only `cover`.** `None` draws at intrinsic size, so any
            // source larger than its box overflows too. Nobody reported that
            // one because `cover` is the common case; the clip is
            // unconditional because the rule is about the element rather than
            // about the fit, which is how Chrome applies it.
            //
            // Measured rather than assumed, on an `<img>` with no `overflow`
            // declared on it or any ancestor and a page larger than the box:
            // Chrome paints `cover` and `none` inside the box and its computed
            // `overflow` is `clip`. Forcing `overflow: visible` makes the same
            // picture spill exactly as this used to.
            // `tests/assets/chrome/object-fit-overflow.tsv` carries the run,
            // including that last row -- without a case that spills, three
            // rows saying "inside" are also what a harness blind to everything
            // outside the box would print.
            //
            // **Placed in the content box, not the box.** CSS puts replaced
            // content inside the border *and* the padding, and Chrome does
            // both and adds them: an 80x80 `<img>` with an 8px border paints
            // its picture at `68,68,64,64`, with 8px of padding at exactly the
            // same rectangle, and with both at `76,76,48,48`. Fitting to the
            // box instead put the picture over the element's own border --
            // measured as every one of a ring's pixels gone, where Chrome
            // keeps all of them.
            //
            // Text and child boxes already land here; this arm was the one
            // drawing into the box itself, so this is one path brought into
            // line with the other two rather than a change to what a box is.
            //
            // The corners follow the inner curve, tighter than the box's own
            // by the inset it sits inside -- see `clip_to_rounded`, which
            // carries the measurement.
            let placed = fit_image(intrinsic, content, *fit, *position);
            context.save();
            let result = (|context: &mut Context2D| {
                clip_to_rounded(
                    context,
                    inner_radii(node, rect, content),
                    content,
                )?;
                context.draw_image_sized(
                    image,
                    placed.origin.x,
                    placed.origin.y,
                    placed.size.width,
                    placed.size.height,
                );
                Ok(())
            })(context);
            context.restore();
            result
        }
        NodeKind::Path {
            data,
            view_box,
            stretch,
            fill,
            stroke,
            line_width,
            fill_rule,
            line_cap,
            line_join,
            line_dash,
            line_dash_offset,
        } => {
            let drawn = Path2D::from_svg(data, to_skia_rule(*fill_rule))
                .map_err(|error| Error::Paint(error.to_string()))?;
            // No box is the behaviour every path had before one existed:
            // absolute coordinates, shifted to where the node sits.
            let path = view_box.map_or_else(
                || drawn.offset(rect.origin.x, rect.origin.y),
                |view| {
                    drawn.transform(view_box_transform(view, rect, *stretch))
                },
            );

            if let Some(fill) = fill {
                let squash = set_paint(context, fill, rect, true)?;
                let rule = to_skia_rule(*fill_rule);
                if let Some(squash) = squash {
                    context.save();
                    context.clip_path(&path, rule);
                    context.translate(squash.centre.x, squash.centre.y);
                    context.scale(1.0, squash.vertical);
                    context.translate(-squash.centre.x, -squash.centre.y);
                    let reach = (rect.size.height + rect.size.width)
                        / squash.vertical.max(f32::EPSILON);
                    context.fill_rect(
                        rect.origin.x - 1.0,
                        squash.centre.y - reach,
                        rect.size.width + 2.0,
                        reach * 2.0,
                    );
                    context.restore();
                } else {
                    context.fill_path(&path, rule);
                }
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
        // `NodeKind` is `#[non_exhaustive]`: a kind this build cannot draw
        // draws nothing rather than refusing the page. Adding one still fails
        // to compile in `meo-canvas-scene`, where `NodeKind::tag` matches every
        // variant -- the guarantee moved rather than went.
        _ => Ok(()),
    }
}

/// Draws a text node, line box by line box.
///
/// # The content box, not the border box
///
/// Text lays out inside the border **and** the padding, which is v1's rule and
/// CSS's. The rectangle handed down here is the border box -- the same one the
/// background and the border are drawn on -- so a node that drew its text from
/// it put the first glyph under its own border and wrapped against a width
/// that included it.
///
/// # Why the wrap happens again here
///
/// Layout settles a width; the width it last *asked* about is not always that
/// one, because a flex pass narrows an item and then re-offers it. v1 re-wraps
/// in its render pass for exactly this reason and says so. The shaping is
/// cached, so what this costs is the wrap arithmetic and not the shaping.
fn draw_text(
    context: &mut Context2D,
    measurer: &mut SceneMeasurer<'_>,
    id: NodeId,
    node: &Node,
    rect: Rect,
) -> Result<(), Error> {
    let Some(style) = measurer.resolved().text(id).cloned() else {
        return Ok(());
    };
    let content = content_box(node, rect);
    if content.size.width <= 0.0 || content.size.height <= 0.0 {
        return Ok(());
    }

    let Some(block) = measurer.block(id, content.size.width) else {
        return Ok(());
    };
    if block.lines.is_empty() {
        return Ok(());
    }

    let metrics = Metrics::of(&style);
    let base = RunStyle::base(&style);
    let space = measurer.space(&base, metrics.letter_spacing);
    let gap = space + metrics.word_spacing;

    // **The block within the node's box, not a line within its line box.**
    // CSS's `vertical-align` places one inline box on its line, which a scene
    // with one paragraph per node cannot ask for; v1 places the whole
    // paragraph in the box that holds it, and where the two disagree v1 wins.
    //
    // Not clamped at zero, also v1: a paragraph taller than its box hangs out
    // of it rather than being pinned to the top. A node sized to its own text
    // has nothing left over, so all three alignments agree there.
    let free = content.size.height - block.height;
    let mut top = content.origin.y
        + match style.vertical_align {
            VerticalAlign::Top => 0.0,
            VerticalAlign::Middle => free / 2.0,
            VerticalAlign::Bottom => free,
        };

    context.save();
    context.set_text_baseline(SkiaTextBaseline::Alphabetic);
    context.set_text_align(SkiaTextAlign::Left);
    context.set_letter_spacing(metrics.letter_spacing);
    // Held at zero and added by hand: a space is a run of no width here, and
    // the gap between two words is arithmetic the alignment can redistribute.
    context.set_word_spacing(0.0);
    set_text_decoration(context, style.decoration);

    let last = block.lines.len().saturating_sub(1);
    let mut draw = |context: &mut Context2D| {
        for (index, line) in block.lines.iter().enumerate() {
            let width = line_width(line, space, metrics.word_spacing);
            // **Justification skips the last line**, which is CSS's rule and
            // v1's: stretching a line that ends a paragraph spaces out a few
            // words across the whole measure.
            let justify = matches!(style.align, TextAlign::Justify)
                && index != last
                && width < content.size.width;
            let mut x = content.origin.x
                + match style.align {
                    TextAlign::Start | TextAlign::Left | TextAlign::Justify => {
                        0.0
                    }
                    TextAlign::Center => (content.size.width - width) / 2.0,
                    TextAlign::End | TextAlign::Right => {
                        content.size.width - width
                    }
                }
                .max(0.0);
            let gap = if justify {
                let gaps = gap_count(line);
                if gaps > 0 {
                    gap + (content.size.width - width) / gaps as f32
                } else {
                    gap
                }
            } else {
                gap
            };
            let baseline = top + line.baseline_from_top();

            // The gap belongs to a space run that is actually there. Two
            // runs can meet with nothing between them -- `<b>a</b><b>b</b>`
            // is one word in two styles -- and a gap inserted between every
            // pair would draw a space the text does not contain.
            let mut pending = false;
            let mut started = false;
            for run in &line.runs {
                if run.is_space() {
                    pending = started;
                    continue;
                }
                if pending {
                    x += gap;
                }
                draw_run(context, node, &style, run, x, baseline);
                x += run.width;
                pending = false;
                started = true;
            }
            top += line.height + metrics.line_gap;
        }
        Ok(())
    };
    let result = draw(context);

    context.restore();
    result
}

/// Draws one run: its shadows, then the glyphs themselves.
///
/// Every shadow is a full pass over the run before the real one, which is v1's
/// shape and the reason a shadow is cast by the **outlined** glyph rather than
/// by the fill alone.
fn draw_run(
    context: &mut Context2D,
    node: &Node,
    style: &ResolvedText,
    run: &Run,
    x: f32,
    baseline: f32,
) {
    context.set_font(&run.style.to_font());
    // After the font, which resets the variant axes as assigning the CSS
    // `font` shorthand does -- so setting these first would undo them, and
    // the run would be drawn in a face the measurer did not measure.
    let (caps, features) = run.style.to_variant();
    context.set_font_variant(caps, &features);
    context.set_fill_style(to_skia_color(style.color));

    for shadow in &node.effects.text_shadows {
        context.save();
        context.set_shadow_color(to_skia_color(shadow.color));
        // CSS gives a blur *radius* and the backend takes a Gaussian sigma.
        // Half is the conversion every CSS engine uses.
        context.set_shadow_blur(shadow.blur / 2.0);
        context.set_shadow_offset(shadow.offset_x, shadow.offset_y);
        paint_run(context, style, run, x, baseline);
        context.restore();
    }

    paint_run(context, style, run, x, baseline);
}

/// Puts one run down, with its outline if it has one.
///
/// CSS centres a text stroke on the glyph's outline and paints it **over** the
/// fill, so half the width falls inside the letter and a thick stroke visibly
/// thins it. `paint_order` swaps the two, which is the only way to have a
/// heavy outline and whole letterforms at once.
///
/// A round join rather than the canvas default of a mitre: a mitre throws a
/// spike off every sharp corner of a glyph, which is not what a browser draws
/// for `-webkit-text-stroke`. v1's reasoning, and v1's `miterLimit` with it.
fn paint_run(
    context: &mut Context2D,
    style: &ResolvedText,
    run: &Run,
    x: f32,
    baseline: f32,
) {
    let Some(stroke) = style.text_stroke.filter(|stroke| stroke.width > 0.0)
    else {
        context.fill_text(&run.text, x, baseline, None);
        return;
    };

    context.save();
    context.set_line_width(stroke.width);
    context.set_stroke_style(to_skia_color(stroke.color));
    context.set_line_join(StrokeJoin::Round);
    context.set_miter_limit(2.0);
    match style.paint_order {
        PaintOrder::Stroke => {
            context.stroke_text(&run.text, x, baseline, None);
            context.fill_text(&run.text, x, baseline, None);
        }
        PaintOrder::Fill => {
            context.fill_text(&run.text, x, baseline, None);
            context.stroke_text(&run.text, x, baseline, None);
        }
    }
    context.restore();
}

/// How many inter-word gaps a line has to spread justification across.
///
/// A gap for each space run that separates two words, which is not the same as
/// one per pair of runs: two runs can meet with nothing between them.
fn gap_count(line: &Line) -> usize {
    let mut gaps = 0;
    let mut pending = false;
    let mut started = false;
    for run in &line.runs {
        if run.is_space() {
            pending = started;
            continue;
        }
        if pending {
            gaps += 1;
        }
        pending = false;
        started = true;
    }
    gaps
}

/// The rectangle a node's own content sits in: inside its border and padding.
fn content_box(node: &Node, rect: Rect) -> Rect {
    // The used width, not the declared one, so the content box starts where
    // layout reserved room for it.
    let border = used_border(node.layout.border);
    let padding = &node.layout.padding;
    let left = border.left + resolve_length(padding.left, rect.size.width);
    let right = border.right + resolve_length(padding.right, rect.size.width);
    let top = border.top + resolve_length(padding.top, rect.size.width);
    let bottom =
        border.bottom + resolve_length(padding.bottom, rect.size.width);
    Rect::new(
        meo_canvas_scene::Point::new(rect.origin.x + left, rect.origin.y + top),
        Size::new(
            (rect.size.width - left - right).max(0.0),
            (rect.size.height - top - bottom).max(0.0),
        ),
    )
}

/// Sets the rule drawn under, over or through a run.
///
/// One flag set at a time: the scene carries a single keyword where the
/// backend takes three independent lines, which is CSS's own shape --
/// `text-decoration-line` is a set — narrowed to what the wire can say.
fn set_text_decoration(context: &mut Context2D, decoration: TextDecoration) {
    let lines = match decoration {
        TextDecoration::None => SkiaTextDecoration::default(),
        TextDecoration::Underline => SkiaTextDecoration::underline(),
        TextDecoration::Overline => SkiaTextDecoration::overline(),
        TextDecoration::LineThrough => SkiaTextDecoration::line_through(),
    };
    context.set_text_decoration(
        lines,
        SkiaDecorationStyle::default(),
        None,
        None,
    );
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
/// One corner radius, with a value Skia cannot use dropped to a square corner.
const fn usable_radius(radius: f32) -> f32 {
    if radius.is_finite() && radius > 0.0 {
        radius
    } else {
        0.0
    }
}

fn box_path(
    context: &mut Context2D,
    radii: Corners<f32>,
    rect: Rect,
) -> Result<(), Error> {
    context.begin_path();
    box_path_continuing(context, radii, rect)
}

/// The same contour, added to whatever path is already open.
///
/// Split from [`box_path`] for the callers that need two contours in one path —
/// a ring, and an inset shadow's surround-with-a-hole — where a second
/// `begin_path` would discard the first.
fn box_path_continuing(
    context: &mut Context2D,
    radii: Corners<f32>,
    rect: Rect,
) -> Result<(), Error> {
    // **A radius that is not a usable number is dropped here, where it is
    // used.** Skia refuses the whole rectangle for a non-finite radius --
    // `invalid rect: Rect { .. }`, thrown out of a paint that was going to
    // succeed -- and a negative radius is invalid CSS that Chrome drops to
    // zero. Both become a square corner, which is what the browser draws.
    //
    // Layout normalises the same way at `to_taffy_style`, and this is the
    // second door rather than a duplicate: a corner radius is never a layout
    // input, so nothing in that pass sees it.
    let corners = [
        usable_radius(radii.top_left),
        usable_radius(radii.top_right),
        usable_radius(radii.bottom_right),
        usable_radius(radii.bottom_left),
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
    box_path(context, paint.border_radius, rect)?;
    context.fill(SkiaFillRule::NonZero);
    Ok(())
}

fn clip_to_box(
    context: &mut Context2D,
    paint: &PaintStyle,
    rect: Rect,
) -> Result<(), Error> {
    clip_to_rounded(context, paint.border_radius, rect)
}

/// The corner radii of a node's content box.
///
/// Each is the node's own radius less the inset it sits inside, floored at
/// zero. The **larger** of a corner's two adjacent insets is subtracted: our
/// radii are scalar where CSS's inner corner is an ellipse with a different
/// reduction per axis, and rounding a little more than CSS clips a little more
/// of the picture, where rounding less would let it paint outside the curve.
/// With uniform insets -- which is every case anyone has reported -- the two
/// agree exactly.
fn inner_radii(node: &Node, rect: Rect, content: Rect) -> Corners<f32> {
    let left = content.origin.x - rect.origin.x;
    let top = content.origin.y - rect.origin.y;
    let right = (rect.origin.x + rect.size.width)
        - (content.origin.x + content.size.width);
    let bottom = (rect.origin.y + rect.size.height)
        - (content.origin.y + content.size.height);
    let radii = node.paint.border_radius;
    Corners {
        top_left: (radii.top_left - left.max(top)).max(0.0),
        top_right: (radii.top_right - right.max(top)).max(0.0),
        bottom_right: (radii.bottom_right - right.max(bottom)).max(0.0),
        bottom_left: (radii.bottom_left - left.max(bottom)).max(0.0),
    }
}

/// Clips to a rectangle with the corner radii given rather than the ones a
/// node declares.
///
/// **Replaced content needs this and nothing else does.** It is clipped to the
/// *content* box, whose corners follow a tighter curve than the box's own: CSS
/// reduces each radius by the border it sits inside, and Chrome does the same
/// for padding. Measured on an 80x80 `<img>` with a 20px radius and an 8px
/// border, `object-fit: cover` paints 3922 pixels -- against about 3972 for a
/// 12px inner curve, 3753 for the outer 20px curve applied to the smaller
/// rectangle, and 4096 for no curve at all.
/// Draws the mark that stands in for a picture that never arrived.
///
/// # What it looks like and why
///
/// A hairline frame, a wash, and one short diagonal, all in **one mid grey at
/// three alphas**. `rgb(128,128,128)` is equidistant from a white card and a
/// near-black one, so the same three numbers read on both and the renderer
/// needs no idea what is behind the box -- which it could not have. A
/// theme-conditional palette here would be a guess dressed as a feature.
///
/// # What Chrome does, measured
///
/// Chrome paints a **one-pixel border and nothing else** for a broken `<img>`
/// with a box to paint in: the non-background pixel count is exactly `4n-4` at
/// 24, 80 and 200 square, and a loaded image of the same size has no border at
/// all. So the frame here is conformant and the wash and mark are a deliberate
/// departure -- a hairline alone is too easy to read as a styled empty box on a
/// busy card, and the case this exists for is a card a person glances at.
///
/// # The two clamps, which are the whole design
///
/// **The stroke does not scale linearly.** `min(w,h)/32` bounded to 1..=2.5: a
/// linear stroke is invisible at 24 pixels and a cartoon at 400.
///
/// **The mark is size-capped and centred** rather than drawn corner to corner.
/// A diagonal across the box looks right on a square and smears across a
/// 300x84 strip; one small centred mark reads the same at every aspect.
fn draw_missing(
    context: &mut Context2D,
    radii: Corners<f32>,
    content: Rect,
) -> Result<(), Error> {
    // Chrome draws nothing in a box with no area, and neither does this: an
    // `auto` axis with no picture to size it contributes zero, so a collapsed
    // node has nowhere to put a mark and drawing one would invent an extent
    // layout did not give it.
    if content.size.width <= 0.0 || content.size.height <= 0.0 {
        return Ok(());
    }

    let short = content.size.width.min(content.size.height);
    let stroke = (short / 32.0).clamp(1.0, 2.5);

    context.save();
    let result = (|context: &mut Context2D| {
        // Everything is inside the element: the clip is the same rounded inner
        // rectangle the picture would have been clipped to, so a radius is
        // honoured and nothing reaches the border.
        clip_to_rounded(context, radii, content)?;

        context.set_fill_style(to_skia_color(Color::rgba(128, 128, 128, 26)));
        context.fill_rect(
            content.origin.x,
            content.origin.y,
            content.size.width,
            content.size.height,
        );

        context
            .set_stroke_style(to_skia_color(Color::rgba(128, 128, 128, 107)));
        context.set_line_width(stroke);
        // Inset by half the stroke so the frame lands inside the content box
        // rather than straddling its edge, half of which the clip would eat.
        let half = stroke / 2.0;
        box_path(
            context,
            radii,
            Rect::new(
                meo_canvas_scene::Point::new(
                    content.origin.x + half,
                    content.origin.y + half,
                ),
                Size::new(
                    (content.size.width - stroke).max(0.0),
                    (content.size.height - stroke).max(0.0),
                ),
            ),
        )?;
        context.stroke();

        let mark = (short * 0.38).clamp(10.0, 64.0);
        let cx = content.origin.x + content.size.width / 2.0;
        let cy = content.origin.y + content.size.height / 2.0;
        let arm = mark / 2.0;
        context
            .set_stroke_style(to_skia_color(Color::rgba(128, 128, 128, 140)));
        context.set_line_width(stroke);
        context.set_line_cap(StrokeCap::Round);
        context.begin_path();
        context.move_to(cx - arm, cy - arm);
        context.line_to(cx + arm, cy + arm);
        context.stroke();
        Ok(())
    })(context);
    context.restore();
    result
}

fn clip_to_rounded(
    context: &mut Context2D,
    radii: Corners<f32>,
    rect: Rect,
) -> Result<(), Error> {
    box_path(context, radii, rect)?;
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
    // As `content_box`: the painter draws the width layout reserved.
    let widths = used_border(node.layout.border);
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

    // Dashes and dots are strokes, not a ring: a fill has no rhythm to break.
    if !matches!(paint.border_style, BorderStyle::Solid) {
        return stroke_broken_border(context, node, rect, widths);
    }

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
        context.save();
        clip_to_edge(
            context,
            edge,
            outer_corners,
            divisions,
            radii_at(paint),
            widths,
        );

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

/// Narrows the clip to one edge's share of the ring.
///
/// The wedge between the two division lines at that edge's corners, extended
/// far enough to clear the ring there and never past where the two meet. Both
/// border paths use it, so a solid border and a dashed one divide their
/// corners the same way by construction rather than by two implementations
/// agreeing.
fn clip_to_edge(
    context: &mut Context2D,
    edge: usize,
    outer: [(f32, f32); 4],
    divisions: [(f32, f32); 4],
    radii: [f32; 4],
    widths: Sides<f32>,
) {
    let next = (edge + 1) % outer.len();
    let clearance = radii[edge].max(radii[next])
        + widths
            .top
            .max(widths.right)
            .max(widths.bottom)
            .max(widths.left)
        + 1.0;
    let limit = meeting_point(
        outer[edge],
        divisions[edge],
        outer[next],
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
    let far_edge = far(outer[edge], divisions[edge]);
    let far_next = far(outer[next], divisions[next]);

    context.begin_path();
    context.move_to(outer[edge].0, outer[edge].1);
    context.line_to(outer[next].0, outer[next].1);
    context.line_to(far_next.0, far_next.1);
    context.line_to(far_edge.0, far_edge.1);
    context.close_path();
    context.clip(SkiaFillRule::NonZero);
}

/// Clips to one edge's **territory** rather than to its wedge.
///
/// # What differs from [`clip_to_edge`], and why it is a second function
///
/// The wedge splits every corner down its mitre so each edge paints its own
/// half. **This gives each corner to exactly one edge**: an edge owns the
/// corner it starts at and gives away the one it ends at, so a corner mark is
/// stroked once rather than as two halves that overlap.
///
/// The two are separate because the divided case must not move. Chrome's
/// two-colour corner reads `0.753` on the diagonal -- two half-covered
/// antialiased halves -- and that is the right answer wherever the edges
/// differ. Only a corner between edges that agree reaches this.
///
/// # The shape
///
/// A rectangle rather than a wedge: from the **outer** line of the previous
/// side, across this side's own run, to the **inner** line of the next side.
///
/// ```text
/// wedge                     territory
/// +--------------+          +--------------+
/// |\            /|          |              |
/// | \          / |          |              |
/// +--+--------+--+          +-----------+--+
/// ```
///
/// Drawn once, the mark is exact at any opacity: two halves composite to
/// `1 - (1 - a)^2` and one mark is `a`. Chrome draws it once at every alpha,
/// measured at `0.502` for `rgba(0, 0, 0, .5)`.
fn clip_to_owned_edge(
    context: &mut Context2D,
    edge: usize,
    outer: [(f32, f32); 4],
    widths: Sides<f32>,
) {
    let next = (edge + 1) % outer.len();
    let (start, end) = (outer[edge], outer[next]);
    let span = (end.0 - start.0).hypot(end.1 - start.1);
    if span <= 0.0 {
        return;
    }
    let along = ((end.0 - start.0) / span, (end.1 - start.1) / span);
    // Into the box, which for a rectangle in this rotation is the previous
    // side's own direction reversed -- so the owned corner's boundary lies on
    // that side's outer line and the whole corner square falls inside.
    let inward = (-along.1, along.0);
    let widest = widths
        .top
        .max(widths.right)
        .max(widths.bottom)
        .max(widths.left);
    // Past the inner edge of the widest side, so the clip never cuts the
    // stroke it is meant to contain.
    let depth = widest + 1.0;
    // The corner this edge gives away: pulled back along its own run by the
    // next side's width, which is where that side's territory begins.
    //
    // **Unless that side draws nothing.** Chrome fills a corner square from
    // whichever edge is drawn -- measured both ways round, `border-top` alone
    // and `border-left` alone each fill the whole square with no diagonal --
    // so handing the corner to an edge of zero width would leave it to
    // nobody. The start-corner convention only decides between two edges that
    // both draw.
    let given = [widths.top, widths.right, widths.bottom, widths.left][next];
    let handover = if given > 0.0 {
        (
            along.0.mul_add(-given, end.0),
            along.1.mul_add(-given, end.1),
        )
    } else {
        (along.0.mul_add(depth, end.0), along.1.mul_add(depth, end.1))
    };
    let deep = |point: (f32, f32)| {
        (
            inward.0.mul_add(depth, point.0),
            inward.1.mul_add(depth, point.1),
        )
    };
    // **The only boundary that may cut anything is the handover.** A clip is
    // antialiased, so an edge of it lying exactly on the mark's tangent eats
    // the rim: bounding the territory at the box's outer line dropped the
    // corner's first row from `5` to `3` against Chrome. So the other three
    // sides are pushed clear -- outward past the outer line, and back past
    // the owned corner -- and the rectangle cuts only where this edge's
    // territory actually ends.
    let clear = |point: (f32, f32), back: f32| {
        (
            along.0.mul_add(back, inward.0.mul_add(-depth, point.0)),
            along.1.mul_add(back, inward.1.mul_add(-depth, point.1)),
        )
    };
    let outside_start = clear(start, -depth);
    let outside_handover = clear(handover, 0.0);
    let far_handover = deep(handover);
    let far_start = deep((
        along.0.mul_add(-depth, start.0),
        along.1.mul_add(-depth, start.1),
    ));

    context.begin_path();
    context.move_to(outside_start.0, outside_start.1);
    context.line_to(outside_handover.0, outside_handover.1);
    context.line_to(far_handover.0, far_handover.1);
    context.line_to(far_start.0, far_start.1);
    context.close_path();
    context.clip(SkiaFillRule::NonZero);
}

/// The width at and above which a dotted mark is a circle.
///
/// **Below it Chrome draws a square**, measured by MC Main at every width
/// from one to seven, as total ink over one window:
///
/// ```text
/// width 1   3.000   three 1x1 marks, each exactly 1.000   square
/// width 2   4.000   2x2, no rim                           square
/// width 3   9.000   3x3, no rim                           square
/// width 4  11.988   rimmed, pi r^2 = 12.57                circle
/// width 5  19.831   pi r^2 = 19.63                        circle
/// width 7  39.604   pi r^2 = 38.48                        circle
/// ```
///
/// **The squares are exact integers with no antialiasing anywhere.** A circle
/// cannot produce an integer at any subpixel position, so `9.000` over a 3x3
/// with no rim settles the shape without an argument about it. Ours drew a
/// disc at every width -- `2.984` at width 2, which is π short a little
/// antialiasing, and exactly what CSS Backgrounds 3 describes. **Chrome does
/// not do what the specification says below four pixels**, and this follows
/// Chrome, as everything else here does.
///
/// **Fractional widths are unmeasured.** Whether the rule is *below four* or
/// *below some fractional threshold* is not known, and this constant assumes
/// the first: a 3.5-pixel border squares here. A single Chrome reading at
/// 3.5 settles it, and `crates/meo-canvas-core/src/paint.rs` is where the
/// answer goes when it exists.
const ROUND_DOT_WIDTH: f32 = 4.0;

/// The dash and the gap a dashed border of this width is drawn with.
///
/// **Two regimes, measured in Chrome rather than derived**, at widths 1, 2, 4
/// and 8 along a 240-pixel edge:
///
/// ```text
/// width  ink  gap   period    as a multiple of the width
///   1     3    2      5       dash 3w, gap 2w
///   2     6    4     10       dash 3w, gap 2w
///   3     6    3      9       dash 2w, gap 1w
///   4     8    4     12       dash 2w, gap 1w
///   8    16    8     24       dash 2w, gap 1w
/// ```
///
/// So a thin border gets a longer dash relative to its width -- a minimum
/// dash asserting itself, which is what stops a one-pixel dashed line reading
/// as a dotted one.
///
/// **Width 3 was measured after this was written and sits in the upper
/// regime**: `on:6 off:3`, which is `2w` and `1w`. So the step is at 3 rather
/// than after it, and the boundary here is a row of the table rather than the
/// guess it started as.
///
/// This replaced `max(2, w * 1.5)` on and `max(1, w)` off, which was v1's and
/// wrong at every width: v1's rhythm is a decision made without a browser to
/// check against, and the browser is the baseline for behaviour.
///
/// `crates/meo-canvas/tests/assets/chrome/border-rhythm.tsv`.
///
/// Public so the Chrome-truth test can assert the ratio directly rather than
/// counting ink runs off a render: the rhythm is arithmetic, and a test that
/// re-derives it from pixels would be measuring the rasteriser as well.
#[must_use]
pub fn dash_pattern(width: f32) -> (f32, f32) {
    if width < 3.0 {
        (width * 3.0, width * 2.0)
    } else {
        (width * 2.0, width)
    }
}

/// The line a broken border is stroked along, and the radii it curves by.
///
/// Half of each edge's width in from the box, so a stroke of that width lands
/// inside the border box where CSS puts a border. The radii shrink with the
/// inset and are floored at zero, as CSS floors them.
fn centre_line(
    paint: &PaintStyle,
    rect: Rect,
    widths: Sides<f32>,
) -> (Rect, PaintStyle) {
    let centre = Rect::new(
        meo_canvas_scene::Point::new(
            widths.left.mul_add(0.5, rect.origin.x),
            widths.top.mul_add(0.5, rect.origin.y),
        ),
        Size::new(
            widths
                .left
                .midpoint(widths.right)
                .mul_add(-1.0, rect.size.width)
                .max(0.0),
            widths
                .top
                .midpoint(widths.bottom)
                .mul_add(-1.0, rect.size.height)
                .max(0.0),
        ),
    );
    let shrink = |radius: f32, by: f32| (radius - by).max(0.0);
    let radii = paint.border_radius;
    let curved = PaintStyle {
        border_radius: Corners {
            top_left: shrink(radii.top_left, widths.left.min(widths.top) / 2.0),
            top_right: shrink(
                radii.top_right,
                widths.right.min(widths.top) / 2.0,
            ),
            bottom_right: shrink(
                radii.bottom_right,
                widths.right.min(widths.bottom) / 2.0,
            ),
            bottom_left: shrink(
                radii.bottom_left,
                widths.left.min(widths.bottom) / 2.0,
            ),
        },
        ..paint.clone()
    };
    (centre, curved)
}

/// Strokes a dashed or dotted border, edge by edge.
///
/// # Why this is not the ring
///
/// A solid border is the region between the border box and the padding box,
/// filled. A dashed one is that region **interrupted**, and a fill has no
/// rhythm to break — so the broken styles are strokes of the box's centre
/// line, at the border's own width, with a dash pattern.
///
/// # The pattern
///
/// Chrome's, measured. Dashed takes its lengths from [`dash_pattern`], which
/// carries the numbers and the two regimes they fall into. Dotted is a
/// zero-length dash with round caps at a period of twice the width, which
/// draws circles of the border's own diameter — that one was v1's and turns
/// out to be Chrome's as well, on and off both exactly the width at all four
/// measured sizes.
///
/// # The fitting, and the two shapes a border can take
///
/// A side is fitted to a whole number of dashes, the dash keeping its nominal
/// length and the slack going into the gaps — but **only while `radius <=
/// width`**, where the inner corner is square. Above that the inner corner is
/// genuinely round and the whole border becomes one continuous run, fitted as
/// a loop. [`fits_per_side`] carries the measurement.
///
/// The length fitted is the **border box's** straight run and not the centre
/// line's; [`straight_run`] carries why, and `chrome_border_rhythm.rs` has the
/// row that reads it back out of our own render.
///
/// # Per edge, through the same wedges the solid path uses
///
/// Each edge is clipped to its own corner-divided wedge and strokes the whole
/// centre line in its own colour and width, so per-edge colours and the corner
/// division behave exactly as they do for a solid border. Where two edges
/// differ in width the centre line is a compromise — it is inset by half of
/// each side's own width, and the stroke of the wider edge is centred a little
/// off its own middle.
fn stroke_broken_border(
    context: &mut Context2D,
    node: &Node,
    rect: Rect,
    widths: Sides<f32>,
) -> Result<(), Error> {
    let paint = &node.paint;
    let (centre, centre_paint) = centre_line(paint, rect, widths);

    let outer_corners = [
        (rect.origin.x, rect.origin.y),
        (rect.right(), rect.origin.y),
        (rect.right(), rect.bottom()),
        (rect.origin.x, rect.bottom()),
    ];
    let inner = inner_box(rect, widths);
    let inner_corners = [
        (inner.origin.x, inner.origin.y),
        (inner.right(), inner.origin.y),
        (inner.right(), inner.bottom()),
        (inner.origin.x, inner.bottom()),
    ];
    let divisions = divisions_at(outer_corners, inner_corners);
    let edge_colors = paint.border_color;
    let curves = fitted_radii(paint, rect);
    let per_side = fits_per_side(curves, widths);
    // **Only a square corner.** Where the corner is a straight mitre the
    // division buys nothing between edges that agree and costs a seam down
    // the diagonal, so it goes. Where the corner is a curve it is not a seam
    // at all: the band crosses the diagonal obliquely and Chrome reads
    // `0.325 0.412 0.439` there, partial by geometry. Removing the division
    // on the curve drove ours from `0.753` to `1.000` -- **further from
    // Chrome, not nearer** -- so the loop branch keeps it.
    //
    // Opaque only: undivided, both sides draw the corner mark and it lands on
    // itself, which is exact at full opacity and doubles through a
    // translucent colour. Chrome draws it once at every opacity, so a
    // translucent square corner is still wrong -- see `#27`, where the mark
    // becomes owned by one edge rather than drawn by both.
    let undivided = per_side && uniform_edges(paint, widths);

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
        context.save();
        // **A corner between two matching edges is not divided**, because
        // dividing it is what puts a seam down its diagonal: both edges draw
        // the mark, each clipped to its own half, and two antialiased halves
        // composite to `1 - (1 - 0.5)^2` rather than to one -- measured at
        // `0.753` here against Chrome's `1.000`.
        //
        // **Where the edges differ the division stays, and the seam with
        // it.** Chrome's own two-colour corner reads `0.753` on the same
        // diagonal, so there the seam is the right answer and removing it
        // would be a second defect rather than a fix.
        if undivided {
            clip_to_owned_edge(context, edge, outer_corners, widths);
        } else {
            clip_to_edge(
                context,
                edge,
                outer_corners,
                divisions,
                radii_at(paint),
                widths,
            );
        }

        context.set_line_width(width);
        context.set_stroke_style(to_skia_color(
            colour.unwrap_or(paint.border_color_all),
        ));

        let dotted = matches!(paint.border_style, BorderStyle::Dotted);
        if dotted {
            // The pattern itself is set per side or per loop below, because
            // it is fitted to the length it will run along.
            //
            // **A dot below `ROUND_DOT_WIDTH` is a square, not a circle**,
            // because Chrome's is. The dash is zero-length, so the cap *is*
            // the mark: a round cap draws a disc of the border's width and a
            // square cap draws a square of it, at the same place and with the
            // same rhythm.
            context.set_line_cap(if width < ROUND_DOT_WIDTH {
                StrokeCap::Square
            } else {
                StrokeCap::Round
            });
        } else {
            context.set_line_cap(StrokeCap::Butt);
        }

        let path = if per_side {
            stroke_fitted_side(
                context,
                &SideRun {
                    paint: &centre_paint,
                    rect,
                    centre,
                    outer: outer_corners,
                    curves,
                    widths,
                    edge,
                    width,
                    dotted,
                },
            )
        } else {
            // Above the threshold the border is one continuous run round the
            // whole path, **fitted to the perimeter rather than to a side**.
            // Chrome's loop has no seam: the slack is spread all the way
            // round, so the corner the run starts at is unobservable and this
            // may start wherever the path does.
            let around = perimeter(&centre_paint, centre);
            let loop_fit: [f32; 2] = if dotted {
                fitted_dot_loop(around, width)
            } else {
                fitted_loop(around, width)
            }
            .into();
            context.set_line_dash(&loop_fit);
            let drawn = anchored_loop(context, &centre_paint, centre);
            if drawn.is_ok() {
                context.stroke();
            }
            drawn
        };
        context.restore();
        path?;
    }
    Ok(())
}

/// Whether every edge that is drawn shares one colour and one width.
///
/// **The condition for leaving a corner undivided.** A division exists to give
/// each edge its own paint over its own half of the corner; where the two
/// halves would be painted identically it buys nothing and costs the seam.
///
/// A zero-width edge is skipped by the caller and so cannot disagree: a box
/// with a border on two sides is uniform if those two match.
fn uniform_edges(paint: &PaintStyle, widths: Sides<f32>) -> bool {
    let sides = [
        (widths.top, paint.border_color.top),
        (widths.right, paint.border_color.right),
        (widths.bottom, paint.border_color.bottom),
        (widths.left, paint.border_color.left),
    ];
    let mut drawn = sides.into_iter().filter(|(width, _)| *width > 0.0);
    let Some((first_width, first_colour)) = drawn.next() else {
        return true;
    };
    let first = first_colour.unwrap_or(paint.border_color_all);
    drawn.all(|(width, colour)| {
        width.to_bits() == first_width.to_bits()
            && colour.unwrap_or(paint.border_color_all) == first
    })
}

/// One side of a box that is dashed side by side, and the geometry the two
/// passes need: the run itself, and the corners that close it.
struct SideRun<'a> {
    /// The centre line's own paint, carrying the radii the path curves by.
    paint: &'a PaintStyle,
    /// The border box.
    rect: Rect,
    /// The line the stroke is centred on.
    centre: Rect,
    /// The border box's four corners, in the wedge rotation.
    outer: [(f32, f32); 4],
    /// The outer radii, scaled.
    curves: [f32; 4],
    /// Every side's width.
    widths: Sides<f32>,
    /// Which side this is.
    edge: usize,
    /// This side's width.
    width: f32,
    /// Whether the style is dotted, which has no fitting.
    dotted: bool,
}

/// Strokes one side of a box below the fitting threshold.
///
/// Two passes: the straight run, dashed to its own fitted pattern, and the
/// corners, filled. Both are already clipped to this side's wedge.
fn stroke_fitted_side(
    context: &mut Context2D,
    side: &SideRun<'_>,
) -> Result<(), Error> {
    let (from, to) =
        straight_run(side.rect, side.centre, side.edge, side.curves);
    // **A dot is drawn by a round cap, so its ink reaches half a width past
    // the point the path names.** A dashed run is butt-capped and ends where
    // it says; a dotted one centred on the corner would put half its first
    // dot outside the box. So the dotted run is inset by half a width at each
    // end, which is what makes the ink flush at both -- Chrome reads
    // `first@0 last@136` on a 137 edge at every measured width.
    let (from, to) = if side.dotted {
        inset_ends(from, to, side.width / 2.0)
    } else {
        (from, to)
    };
    let straight = (to.0 - from.0).hypot(to.1 - from.1);
    if straight > 0.0 {
        let fitted: [f32; 2] = if side.dotted {
            fitted_dot(straight, side.width)
        } else {
            fitted_dash(straight, side.width)
        }
        .into();
        context.set_line_dash(&fitted);
        context.begin_path();
        context.move_to(from.0, from.1);
        // **The last dot sits at exactly the path's length, and a dash walker
        // emits at offsets strictly inside it** -- so the final dot of every
        // dotted run was dropped. Each corner then carried one dot instead of
        // two: the edge that *starts* there drew, the edge that ends there did
        // not. It read as flush only because the neighbouring edge's first dot
        // stood in for the missing last one.
        //
        // A four-thousandth of a pixel is enough to make the offset strictly
        // interior, and moves no dot anywhere: the positions are multiples of
        // the period and the period is unchanged.
        let reach = if side.dotted { straight * 1e-4 } else { 0.0 };
        context.line_to(
            (to.0 - from.0).mul_add(1.0 + reach / straight, from.0),
            (to.1 - from.1).mul_add(1.0 + reach / straight, from.1),
        );
        context.stroke();
    }
    fill_corner_arcs(context, side)
}

/// Fills the two corners at the ends of a fitted side.
///
/// **A corner below the threshold is filled rather than dashed, and no gap
/// falls inside it.** Whether Chrome fills it by rule or the adjoining dash
/// simply covers it is not separable at the radii where this branch applies —
/// an arc that short is under one dash long — so this claims the behaviour
/// and not the reason.
fn fill_corner_arcs(
    context: &mut Context2D,
    side: &SideRun<'_>,
) -> Result<(), Error> {
    let along = side_line(side.centre, side.edge);
    let span = (along.1.0 - along.0.0).hypot(along.1.1 - along.0.1);
    if span <= 0.0 {
        return Ok(());
    }
    let forward = (
        (along.1.0 - along.0.0) / span,
        (along.1.1 - along.0.1) / span,
    );
    let reach = span
        + side.widths.top
        + side.widths.right
        + side.widths.bottom
        + side.widths.left;
    let next = (side.edge + 1) % side.outer.len();
    for (corner, direction, radius) in [
        (side.outer[side.edge], forward, side.curves[side.edge]),
        (
            side.outer[next],
            (-forward.0, -forward.1),
            side.curves[next],
        ),
    ] {
        if radius <= 0.0 {
            continue;
        }
        context.save();
        clip_to_corner(context, corner, direction, radius, reach);
        context.set_line_dash(&[]);
        let path = box_path(context, side.paint.border_radius, side.centre);
        if path.is_ok() {
            context.stroke();
        }
        context.restore();
        path?;
    }
    Ok(())
}

/// Pulls both ends of a segment in along its own direction.
///
/// Returns it unchanged when it is shorter than twice the inset: there would
/// be nothing left to draw along, and crossing the ends over would stroke it
/// backwards.
fn inset_ends(
    from: (f32, f32),
    to: (f32, f32),
    by: f32,
) -> ((f32, f32), (f32, f32)) {
    let span = (to.0 - from.0).hypot(to.1 - from.1);
    if span <= by * 2.0 {
        return (from, to);
    }
    let step = (by * (to.0 - from.0) / span, by * (to.1 - from.1) / span);
    (
        (from.0 + step.0, from.1 + step.1),
        (to.0 - step.0, to.1 - step.1),
    )
}

/// The dot pattern for one side, fitted to that side's own length.
///
/// # A dot is drawn by a cap, not by a dash
///
/// Dotted is a **zero-length** dash with round caps, so the pattern element
/// carries no length and the ink is a circle of the border's own diameter.
/// That is Chrome's, measured: on and off are both exactly `w` at every width
/// in the table. It also means the fitting arithmetic is not
/// [`fitted_dash`]'s -- the ink extends half a cap past each end of the run,
/// so a side of `length` holding `n` dots spans `(n - 1) * period + w`, and
/// flushness at both ends wants `period = (length - w) / (n - 1)`.
///
/// # The count is the general rule, not a dotted one
///
/// Chrome takes `(length / w + 1) / 2` to the nearest whole number, measured
/// across seven edge lengths at five widths. **That is
/// [`fitted_dash`]'s own count** -- `round((length + gap) / (dash + gap))`
/// with a nominal pattern of `w` on and `w` off -- so the dashed and dotted
/// tables were confirming one rule while each was taken to be measuring its
/// own. Two instruments, two patterns, one answer neither was looking for.
///
/// A tie is where that rule is undetermined and Chrome's own answers disagree
/// with each other, so the fixture for this is a 131- or 137-wide box rather
/// than the 240 every measured width ties on.
///
/// # What is still not Chrome's, measured -- and the mechanism is not known
///
/// A corner carries a whole dot: both edges place one there and the two
/// coincide. **Chrome's corner is fuller, and it is fuller in one direction.**
/// At width 8, ours against Chrome's, and an ordinary dot for scale:
///
/// ```text
/// ours corner    chrome corner    an ordinary dot
/// .+####+..      .######..        .+####+..
/// +######+.      #######+.        +######+.
/// ########.      ########.        ########.
/// ```
///
/// **Ours is a symmetric disc. Chrome's leans toward the corner diagonal.**
///
/// The mechanism offered for it was *two overlapping discs, one from each
/// edge* -- and **our own render refutes that on its own terms**: we place a
/// dot from each edge at the corner too, and two discs sharing a centre are
/// one disc, which is exactly the symmetric shape we draw. **Whatever leans
/// Chrome's corner into the diagonal is not two coincident discs**, and it has
/// not been measured. The difference is a handful of part-covered pixels per
/// corner.
///
/// Worth knowing before chasing it: **at width 4 the two shapes are
/// indistinguishable** -- a disc of diameter 4 saturates its own 4x4 box, so
/// the shoulders that separate them do not exist to read. The case only
/// discriminates from width 8 up, which is why it went unnoticed.
///
/// `crates/meo-canvas/tests/assets/chrome/dotted-rhythm.tsv`.
#[must_use]
pub fn fitted_dot(length: f32, width: f32) -> (f32, f32) {
    let nominal = (0.0, width * 2.0);
    if width <= 0.0 || length <= width {
        return nominal;
    }
    // Written as the general count with both terms `w` rather than as
    // `(length / w + 1) / 2`: the two are the same number and this spelling
    // says which rule it is.
    let period = 2.0 * width;
    let count = ((length + period) / period).round();
    if count < 2.0 {
        return nominal;
    }
    (0.0, (length / (count - 1.0)).max(0.0))
}

/// The dot pattern for a closed path, fitted to the whole of it.
///
/// **A loop has as many gaps as dots**, because the last gap closes onto the
/// first dot rather than stopping at a corner -- the same term that separates
/// [`fitted_loop`] from [`fitted_dash`]. So the count is `round(L / 2w)` and
/// the period divides the length exactly.
///
/// The round cap does not need subtracting here: on a closed path every dot
/// has a neighbour on both sides, so there is no end for the ink to overhang.
#[must_use]
pub fn fitted_dot_loop(length: f32, width: f32) -> (f32, f32) {
    let nominal = (0.0, width * 2.0);
    if width <= 0.0 || length <= width * 2.0 {
        return nominal;
    }
    let count = (length / (width * 2.0)).round().max(1.0);
    (0.0, (length / count).max(0.0))
}

/// The dash and gap for a closed path, fitted to the whole of it.
///
/// **The same arithmetic as [`fitted_dash`] with one term changed: a loop has
/// as many gaps as dashes**, because the last gap closes onto the first dash
/// rather than stopping at a corner. An open run of `n` dashes has `n - 1`
/// gaps; getting that wrong here leaves the gaps a shade too wide and the
/// fitting invisible -- on a 240x48 box at radius 8 the open form fits 46
/// dashes with a gap of 4.04 and draws what an unfitted period draws, where
/// the closed form fits 46 with 3.95 and scatters the threes Chrome scatters.
///
/// **The length is the centre path's**, measured on both renderers: a 240x48
/// box at radius 8 holds 46 marks where the outer perimeter predicts 47, and
/// Chrome's own 137x120 row holds 41 where the outer predicts 42. So a
/// dashed border fits the **outer** straight run per side and the **centre**
/// path round a loop -- two mechanisms, which is not what either of us
/// expected and is measured on both sides.
///
/// Chrome's loop has no seam, so this says nothing about where the run starts.
#[must_use]
pub fn fitted_loop(length: f32, width: f32) -> (f32, f32) {
    let (dash, nominal) = dash_pattern(width);
    if dash <= 0.0 || length <= dash + nominal {
        return (dash, nominal);
    }
    let count = (length / (dash + nominal)).round().max(1.0);
    (dash, (count.mul_add(-dash, length) / count).max(0.0))
}

/// The same contour [`box_path`] draws, opened at the top-left tangent.
///
/// # Why this exists rather than a dash offset
///
/// A dashed loop's phase begins where its path begins. Chrome's begins at a
/// tangent -- its dashes fall on `x = 8` of a 240x48 box at radius 8, which is
/// where the top edge's straight part starts -- and ours fell on `x = 3`,
/// which is inside the arc and is not a landmark at all. **Neither our own
/// start point nor the offset from it to a tangent is derivable**: the two
/// candidate starts a rounded rectangle might open at predict 8 and 10.5, and
/// the phase we actually got is neither, so `round_rect` is opening somewhere
/// unstated or Skia's measured length differs from the geometric one by the
/// conic approximation of the arcs. An offset tuned until the picture agreed
/// would be a number nobody could derive, and the first box with a different
/// radius would move it.
///
/// So the path is traced from the tangent instead, and the phase starts there
/// **because the path does** -- the same reason each side of a square box is
/// stroked as its own line rather than given a computed offset.
///
/// **Which tangent is not a choice**: the outer contour's is at `r` from the
/// box's edge, and the centre line's is at `w / 2 + (r - w / 2)`, the same
/// point. They separate only where the centre radius floors at zero, `r < w /
/// 2`, and a box on this branch has `r > w`. On the branch that uses an
/// anchor, there is one point to mean.
///
/// # The hazard
///
/// Skia adds a rectangle and a rounded rectangle to a path by different
/// mechanisms, and mixing them in one path joins the contours instead of
/// leaving them separate -- which painted a triangle over half a box once
/// already; [`box_path_continuing`] carries that story. This traces lines and
/// arcs only, so it never takes the rectangle route, and **it is used for the
/// dashed loop alone**. Every filling caller stays on [`box_path`], where the
/// contour's start point does not matter and the even-odd ring does.
fn anchored_loop(
    context: &mut Context2D,
    paint: &PaintStyle,
    rect: Rect,
) -> Result<(), Error> {
    let [top_left, top_right, bottom_right, bottom_left] =
        fitted_radii(paint, rect);
    let (left, top) = (rect.origin.x, rect.origin.y);
    let (right, bottom) = (rect.right(), rect.bottom());

    context.begin_path();
    context.move_to(left + top_left, top);
    // Clockwise from the top-left tangent: each side, then the corner it runs
    // into. A zero radius is a corner rather than an arc of no length, because
    // `arc_to` through coincident points has no circle to fit.
    for (side, corner, next, radius) in [
        (
            (right - top_right, top),
            (right, top),
            (right, top + top_right),
            top_right,
        ),
        (
            (right, bottom - bottom_right),
            (right, bottom),
            (right - bottom_right, bottom),
            bottom_right,
        ),
        (
            (left + bottom_left, bottom),
            (left, bottom),
            (left, bottom - bottom_left),
            bottom_left,
        ),
        (
            (left, top + top_left),
            (left, top),
            (left + top_left, top),
            top_left,
        ),
    ] {
        context.line_to(side.0, side.1);
        if radius > 0.0 {
            context
                .arc_to(corner.0, corner.1, next.0, next.1, radius)
                .map_err(|error| Error::Paint(error.to_string()))?;
        } else {
            context.line_to(corner.0, corner.1);
        }
    }
    context.close_path();
    Ok(())
}

/// The length of the closed path a rounded box is stroked along.
///
/// Each corner takes its radius off both of the sides it joins and gives back
/// a quarter arc, so a radius costs `2r` of straight and returns `pi * r / 2`.
///
/// This is what a continuous border is fitted to. **Not a side**: above the
/// threshold Chrome fits the loop, spreads the remainder round the whole of
/// it, and leaves no seam — which is why a side of such a box is neither
/// flush at its corners nor a whole number of periods long. Ours was flush at
/// both ends of every side for the accidental reason that each stroke began
/// at a corner, and flushness is the *per-side* signature.
fn perimeter(paint: &PaintStyle, rect: Rect) -> f32 {
    let curved: f32 = radii_at(paint).iter().sum();
    let straight = 2.0f32.mul_add(rect.size.width + rect.size.height, 0.0);
    (std::f32::consts::FRAC_PI_2 - 2.0).mul_add(curved, straight)
}

/// The same ramp, rotated round the sweep by a fraction of a turn.
///
/// # Why the stops move and not the angle
///
/// **CSS starts a conic sweep at twelve o'clock and a canvas starts it at
/// three**, so a `from` handed straight to the shader draws the ramp a quarter
/// turn late -- measured against Chrome as a uniform 270 degrees across every
/// sample of every conic case. v1 converts the angle
/// (`gradient.canvas.ts:42`) and this crate did not, so it is a port defect
/// rather than one we invented.
///
/// **The angle cannot carry the correction here.** Skia's sweep takes a start
/// and an end and **clamps outside them rather than wrapping**: moving the
/// range to `[from - 90, from + 270]` leaves every pixel past the end reading
/// the last stop, and moving it to `[from + 270, from + 630]` leaves every
/// pixel before the start reading the first. Both were measured -- white at
/// twelve o'clock, then black everywhere. The range has to stay the full turn
/// it already is.
///
/// The local-matrix slot that would rotate the shader is passed `None` by the
/// binding (`meo-skia-canvas-0.11.0/src/shader.rs:428`) and is not exposed, so
/// that route is closed too.
///
/// So the ramp moves instead of the frame. A stop at `p` is read where the
/// sweep is at `p`, and we want the colour CSS puts a quarter turn earlier, so
/// every position shifts by `turns` and wraps.
///
/// # The seam
///
/// Wrapping splits the ramp, and the pair that straddles `0` would otherwise
/// interpolate the long way round the circle. A stop is planted at each end
/// carrying the colour the ramp actually has there, so the seam is a join
/// rather than a jump.
fn turned(stops: &[SkiaGradientStop], turns: f32) -> Vec<SkiaGradientStop> {
    if stops.is_empty() {
        return Vec::new();
    }
    // The position in the original ramp that becomes the new origin.
    let origin = (-turns).rem_euclid(1.0);
    let seam = seam_color(stops, turns);

    // Walked from the new origin rather than shifted in place. **Shifting
    // collapses the ends**: a ramp's stops at `0` and `1` are the same point
    // on a circle, so moving both by the same amount lands them together and
    // destroys the order the ramp is read in -- measured as a picture mirrored
    // about the vertical, matching Chrome at twelve and six o'clock and
    // reversed at three and nine.
    let mut out = Vec::with_capacity(stops.len() + 2);
    out.push(SkiaGradientStop {
        position: 0.0,
        color: seam,
    });
    for stop in stops.iter().filter(|stop| stop.position > origin) {
        out.push(SkiaGradientStop {
            position: stop.position - origin,
            color: stop.color,
        });
    }
    for stop in stops.iter().filter(|stop| stop.position <= origin) {
        out.push(SkiaGradientStop {
            position: stop.position - origin + 1.0,
            color: stop.color,
        });
    }
    out.push(SkiaGradientStop {
        position: 1.0,
        color: seam,
    });
    out
}

/// The ramp's colour where the rotation wraps it.
///
/// `-turns` is the position in the original ramp that lands on the seam, so
/// this is the ramp read at that point: the two stops it falls between, mixed
/// by how far along it sits.
///
/// **Mixed in the encoded space and not in linear light.** The stops are
/// stored premultiplied and linear, but the gradient interpolates the way CSS
/// does, so a seam blended linearly lands in the wrong place -- a quarter of
/// the way from black to white is `64` encoded and `137` linear, and the
/// second is what we drew before this converted. The blend has to happen in
/// whichever space the ramp either side of it is being drawn in.
/// # Panics
///
/// **On an empty `stops`, and the guard against that is the caller's.** The
/// first and last stop are read before anything else, so there is nothing
/// sensible to return for a gradient with no colours in it. `conic_shader`
/// refuses that case five lines above the call, and the render fuzz reaches
/// this arm often enough that the refusal is exercised rather than merely
/// present -- 1,509 of one 2,000-scene run were empty-gradient refusals.
///
/// The assertion below is what makes that caller's check load-bearing instead
/// of incidental: a second caller added later fails here in a debug build
/// rather than indexing past the end in a release one.
fn seam_color(stops: &[SkiaGradientStop], turns: f32) -> RgbaLinear {
    debug_assert!(
        !stops.is_empty(),
        "seam_color needs at least one stop; the caller checks `is_empty` \
         before calling and a new caller must do the same"
    );
    let at = (-turns).rem_euclid(1.0);
    let (mut before, mut after) = (stops[0], stops[stops.len() - 1]);
    for stop in stops {
        if stop.position <= at {
            before = *stop;
        }
    }
    for stop in stops.iter().rev() {
        if stop.position >= at {
            after = *stop;
        }
    }
    let span = after.position - before.position;
    if span <= f32::EPSILON {
        return before.color;
    }
    let t = (at - before.position) / span;
    let blend = |a: f32, b: f32| {
        encoded((to_encoded(b) - to_encoded(a)).mul_add(t, to_encoded(a)))
    };
    RgbaLinear {
        r: blend(before.color.r, after.color.r),
        g: blend(before.color.g, after.color.g),
        b: blend(before.color.b, after.color.b),
        a: (after.color.a - before.color.a).mul_add(t, before.color.a),
    }
}

/// A linear-light channel as sRGB writes it.
fn to_encoded(linear: f32) -> f32 {
    if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055_f32.mul_add(linear.powf(1.0 / 2.4), -0.055)
    }
}

/// The inverse: an sRGB channel back to linear light.
fn encoded(value: f32) -> f32 {
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

/// The four outer radii, scaled the way CSS scales them when two on one side
/// overrun it.
///
/// The border box's own radii, not the centre line's: Chrome fits a side to
/// its **outer** straight run, and below the fitting threshold the centre
/// line's radius has floored at zero while the outer one has not.
fn fitted_radii(paint: &PaintStyle, rect: Rect) -> [f32; 4] {
    let [top_left, top_right, bottom_right, bottom_left] = radii_at(paint);
    let ratio = |sum: f32, length: f32| {
        if sum > length && sum > 0.0 {
            length / sum
        } else {
            1.0
        }
    };
    let scale = ratio(top_left + top_right, rect.size.width)
        .min(ratio(bottom_left + bottom_right, rect.size.width))
        .min(ratio(top_left + bottom_left, rect.size.height))
        .min(ratio(top_right + bottom_right, rect.size.height));
    [
        top_left * scale,
        top_right * scale,
        bottom_right * scale,
        bottom_left * scale,
    ]
}

/// Whether this box is dashed side by side rather than round its path.
///
/// **The threshold is a degeneracy rather than a margin.** The inner edge of a
/// border curves by `radius - width`; where that is zero or negative **the
/// inner corner is square**, and Chrome fits each side on its own exactly as
/// it does for a square box. Above it the inner corner is genuinely round and
/// the border becomes one continuous run round the whole path.
///
/// # The signature, which is a length and not a presence
///
/// What separates the two is **a mark longer than that side's own dash**,
/// which only two per-side runs butting at a corner can produce. Ink spanning
/// the tangent is *not* the signature: a continuous run crosses the corner
/// with one ordinary dash, and reading presence rather than length inverts the
/// answer on exactly the case in question.
///
/// ```text
/// w 4/4    on:8.1   one dash at that width   crossing, not butting
/// w 8/8    on:26.8  against a 16 dash        butting
/// w 12/12  on:64.8  against a 24 dash        butting
/// ```
///
/// # Uniform widths: `radius > width`
///
/// ```text
/// width 4   r 0, 4 -> 3 butting marks     r 5, 6, 8, 12, 24 -> none
/// width 8   r 4, 6, 7, 8 -> 3 butting     r 9, 10, 12       -> none
/// ```
///
/// Both turn at `r > w`. Flushness at a tangent does **not** measure this and
/// contradicted it three times: it recurs in bands as the arithmetic comes
/// round -- at width 2, flush at radii 1-4, not 5-7, flush 8-9 -- which is a
/// coincidence with a period rather than a branch. An earlier reading of this
/// threshold as `5 < r <= 6` came from exactly that, and `w + 2` and `1.5w`
/// were both fitted to it.
///
/// # Unequal widths: the **thinner** side decides, and it is measured
///
/// Two pairs, both walked by Agent Zero at `r = 6`, and neither side butts:
///
/// ```text
/// w_top 4 / w_left 8    off:9.8  over the corner    continuous
/// w_top 4 / w_left 12   off:13.1 over the corner    continuous
/// ```
///
/// So a corner is degenerate up to `min(w_a, w_b)`. The second pair is what
/// makes it a rule rather than a fit: at `4/8` the two candidate thresholds
/// are 4 and 8 with the radius between them, and at `4/12` they are far apart
/// and the answer still follows the smaller.
///
/// **What this displaced**: that Chrome asks each *side* about its own width,
/// which a corner would see as `min`. That framing accounts for each side
/// keeping its own dash length -- `2w` at 12 on one side of a corner and `2w`
/// at 4 on the other -- but so does this one, because a continuous border is
/// still stroked edge by edge in each edge's own width. It fails on the bit
/// that does separate them: it predicts the thicker side butts at a corner its
/// own width calls degenerate, and at both pairs it does not.
///
/// **A consequence worth meeting here rather than in a render**: a thin edge
/// beside a thick one sends the whole corner continuous early. Widths 1 and 20
/// at a radius of 2 is continuous, though the 20-wide side's own geometry is
/// nowhere near its threshold. That follows from both rows rather than adding
/// to them, and it is where this rule would be wrong if it is wrong.
///
/// # Why the arc is safe to fill from whichever wedge is painting
///
/// **Dash length is per side; whether the corner is filled is a corner
/// decision.** So a corner's two sides never branch differently, and the arc
/// between them never has two answers to choose from. That is a reason rather
/// than a construction -- the code would happily paint a corner twice if the
/// sides disagreed -- so it is written here: if a measurement ever shows one
/// side of a corner fitted and the other continuous, [`fill_corner_arcs`]
/// becomes a third case and not a detail.
fn fits_per_side(curves: [f32; 4], widths: Sides<f32>) -> bool {
    let pairs = [
        (widths.top, widths.left),
        (widths.top, widths.right),
        (widths.bottom, widths.right),
        (widths.bottom, widths.left),
    ];
    curves
        .iter()
        .zip(pairs)
        .all(|(radius, (one, other))| *radius <= one.min(other))
}

/// The straight part of one side: where its ink begins and ends.
///
/// **Taken from the border box and not from the line it is drawn on.** Chrome
/// fits a side to `outer - r_start - r_end`, which the centre line cannot
/// give: inset by half a width, its own radius floors at zero, and at width 8
/// with a 1px radius the two lengths differ by 6. Three radii at that width
/// track the outer run exactly.
///
/// So the run is positioned across the side by the centre line -- a stroke of
/// the border's width lands where CSS puts a border -- and along it by the
/// border box.
fn straight_run(
    rect: Rect,
    centre: Rect,
    edge: usize,
    curves: [f32; 4],
) -> ((f32, f32), (f32, f32)) {
    let (left, top) = (rect.origin.x, rect.origin.y);
    let (right, bottom) = (rect.right(), rect.bottom());
    let (near, far) = (centre.origin.x, centre.origin.y);
    let across = (near + centre.size.width, far + centre.size.height);
    let [top_left, top_right, bottom_right, bottom_left] = curves;
    match edge {
        0 => ((left + top_left, far), (right - top_right, far)),
        1 => (
            (across.0, top + top_right),
            (across.0, bottom - bottom_right),
        ),
        2 => (
            (right - bottom_right, across.1),
            (left + bottom_left, across.1),
        ),
        _ => ((near, bottom - bottom_left), (near, top + top_left)),
    }
}

/// Narrows the clip to one corner's share of a side.
///
/// Everything within `distance` of `corner` along `direction`, which for a
/// rounded box is exactly the part of the side the arc occupies. `reach`
/// carries the polygon far enough out to cover the ring in both directions;
/// the wedge this sits inside does the real cutting.
fn clip_to_corner(
    context: &mut Context2D,
    corner: (f32, f32),
    direction: (f32, f32),
    distance: f32,
    reach: f32,
) {
    let normal = (-direction.1, direction.0);
    let point = |along: f32, across: f32| {
        (
            normal
                .0
                .mul_add(across, direction.0.mul_add(along, corner.0)),
            normal
                .1
                .mul_add(across, direction.1.mul_add(along, corner.1)),
        )
    };
    let corners = [
        point(-reach, -reach),
        point(distance, -reach),
        point(distance, reach),
        point(-reach, reach),
    ];
    context.begin_path();
    context.move_to(corners[0].0, corners[0].1);
    for step in &corners[1..] {
        context.line_to(step.0, step.1);
    }
    context.close_path();
    context.clip(SkiaFillRule::NonZero);
}

/// The centre line of one side, corner to corner.
///
/// Ordered so that it starts at the side's first corner in the same rotation
/// the wedges use — top, right, bottom, left — because the dash starts where
/// the line does and Chrome anchors it at the corner.
const fn side_line(centre: Rect, edge: usize) -> ((f32, f32), (f32, f32)) {
    let (left, top) = (centre.origin.x, centre.origin.y);
    let right = left + centre.size.width;
    let bottom = top + centre.size.height;
    match edge {
        0 => ((left, top), (right, top)),
        1 => ((right, top), (right, bottom)),
        2 => ((right, bottom), (left, bottom)),
        _ => ((left, bottom), (left, top)),
    }
}

/// The dash and gap for one side, fitted to that side's own length.
///
/// # What Chrome does
///
/// **The dash keeps its nominal length and the slack goes into the gaps**, and
/// a side begins and ends flush with a whole dash. On a 48-pixel edge at width
/// 4 the runs are `8, 5, 8, 6, 8, 5, 8` -- four dashes of exactly `2w`, three
/// gaps, summing to exactly 48.
///
/// So the count is chosen and the gap follows: `n` dashes leave `n - 1` gaps,
/// and the gap that makes them fit is `(length - n * dash) / (n - 1)`.
///
/// # Choosing the count
///
/// The **nearest** fit, not the largest that fits. Chrome's gaps go both ways
/// around the nominal: `5, 6, 5` on that 48-pixel edge are all wider than the
/// nominal 4, and `4, 3, 4, 4` on a 137-pixel one include a narrower. A rule
/// that only ever padded would be right on the first edge and wrong on the
/// second.
///
/// Rounding `(length + gap) / (dash + gap)` is that rule: 52/12 rounds to four
/// dashes on the 48 edge, and 141/12 rounds to twelve on the 137 one, which
/// are the counts measured in both.
///
/// # What this does not reproduce
///
/// Chrome distributes the remainder **symmetrically** -- `5, 6, 5` rather than
/// `6, 5, 5` -- and a single dash array cannot say that: every gap here is the
/// same fractional length and the rasteriser rounds each one where it falls.
/// The symmetry is measured on the 48-pixel edge alone, where three gaps make
/// it plain; the longer edge was read through a sixty-pixel window that never
/// reached its middle. Reproducing it needs a per-gap path rather than a dash
/// pattern, and a row that reads a long edge whole to check it against.
#[must_use]
pub fn fitted_dash(length: f32, width: f32) -> (f32, f32) {
    let (dash, nominal) = dash_pattern(width);
    if dash <= 0.0 || length <= dash {
        return (dash, nominal);
    }
    let count = ((length + nominal) / (dash + nominal)).round();
    if count < 2.0 {
        return (dash, nominal);
    }
    (
        dash,
        (count.mul_add(-dash, length) / (count - 1.0)).max(0.0),
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
        let margin = shadow_reach(rect, shadow);
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
        box_path_continuing(context, paint.border_radius, hole)?;
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
/// # Drawn as a shape, not as a property of a fill
///
/// The obvious way is Skia's own shadow: set `shadow_blur`, `shadow_color` and
/// `shadow_offset`, then fill the box. It draws the blurred copy correctly and
/// then draws **the box itself** in whatever the fill style is -- and the fill
/// cannot be transparent, because the shadow is derived from the drawn shape's
/// own alpha and a transparent shape casts nothing. So that route always
/// leaves a solid silhouette of the box on the canvas in the shadow's colour.
///
/// While every background was opaque the silhouette was invisible, covered by
/// the background painted next. A translucent one showed it: a
/// `rgba(0,0,0,0.5)` box over `#b01020` read `33, 3, 6` where Chrome reads
/// `88, 8, 16`, the second coat of half-alpha black that
/// `1 - (1-0.5)^2 = 0.75` describes.
///
/// Clipping the silhouette away gets most of it and cannot get all of it: its
/// antialiased rim straddles the border contour, the clip feathers across the
/// same pixels, and the two coverages multiply instead of cancelling. Measured
/// on the `box-shadow` fixture, on the top-left contour of a card whose shadow
/// is offset 6,6 with no blur -- where CSS puts nothing at all -- that left 14
/// units of 255 behind, down from 23 with no clip.
///
/// Nor can the silhouette be moved out of the way. Drawing the source past the
/// clip and paying the distance back through the shadow's offset is the
/// standard trick and it fails here: **Skia clips the source before it blurs
/// it**, so a source outside the clip blurs from almost nothing. Measured,
/// that turned a 10px-blur card's ink from 137 to 226 against a 250 page.
///
/// So the shadow is drawn as what it is: the border box, moved by the offset
/// and grown by the spread, filled in the shadow's colour through a Gaussian
/// mask blur. There is no silhouette to remove because none is made, and the
/// rim probe reads zero rather than fourteen.
///
/// The clip stays, and now says only what CSS Backgrounds and Borders 3 §7.1.1
/// says: an outer shadow is drawn outside the border edge only, so a
/// translucent background cannot reveal the part that falls beneath it.
fn draw_box_shadow(
    context: &mut Context2D,
    paint: &PaintStyle,
    rect: Rect,
    shadow: &BoxShadow,
) -> Result<(), Error> {
    if shadow.inset {
        return draw_inset_box_shadow(context, paint, rect, shadow);
    }
    if shadow.color.is_invisible() {
        return Ok(());
    }

    let spread = shadow.spread;
    let shape = Rect::new(
        meo_canvas_scene::Point::new(
            rect.origin.x - spread + shadow.offset_x,
            rect.origin.y - spread + shadow.offset_y,
        ),
        Size::new(
            spread.mul_add(2.0, rect.size.width).max(0.0),
            spread.mul_add(2.0, rect.size.height).max(0.0),
        ),
    );
    if shape.size.width <= 0.0 || shape.size.height <= 0.0 {
        // Shrunk to nothing by a negative spread. Skia would draw an empty
        // path harmlessly, but a blur of nothing is still a blur pass.
        return Ok(());
    }

    context.save();
    let result = (|| -> Result<(), Error> {
        clip_outside_box(context, paint, rect, shadow_reach(rect, shadow))?;

        // Sigma is exactly half the blur radius -- CSS Backgrounds and Borders
        // 3 §7.1.1, and the same halving `meo-skia-canvas` applies to its own
        // `shadow_blur` (`context/mod.rs:1634`). Taking the same route to the
        // same number is what keeps this rewrite from moving the blur while it
        // moves the silhouette.
        //
        // A mask blur rather than an image filter: the shape is one flat
        // colour, so blurring its coverage and blurring its pixels give the
        // same answer, and the mask is the cheaper of the two. Skipped
        // entirely at zero, where Skia declines to build one.
        if shadow.blur > 0.0 {
            let blur =
                MaskFilter::blur(BlurStyle::Normal, shadow.blur * 0.5, true)
                    .map_err(|error| Error::Paint(error.to_string()))?;
            context.set_mask_filter(Some(&blur));
        }
        context.set_fill_style(to_skia_color(shadow.color));

        let path = contour(shape, spread_radii(paint, spread))?;
        context.fill_path(
            &path.build(SkiaFillRule::NonZero),
            SkiaFillRule::NonZero,
        );
        Ok(())
    })();
    context.restore();
    result
}

/// The corner radii of an outer shadow's shape, once the spread is applied.
///
/// CSS Backgrounds and Borders 3 §7.1.1 grows each radius by the spread and
/// floors it at zero -- **except that a square corner stays square**, which is
/// the part a reading of "grow every radius" gets wrong.
///
/// Measured, and the two rules are told apart by one ray. A square 50x50 box
/// with `0 0 0 6px` carries ink 6 steps out along the corner diagonal in
/// Chrome, which is a right angle at the spread box's own corner; a radius
/// grown to 6 would have curved that corner away and read less. A box with
/// `border-radius: 16px` and the same spread reads `-1` on that ray -- no ink
/// even one step out -- which only a radius of 22 produces. Both rows are in
/// `shadow-extent.tsv`.
fn spread_radii(paint: &PaintStyle, spread: f32) -> [(f32, f32); 4] {
    let grow = |radius: f32| {
        if radius > 0.0 {
            let grown = (radius + spread).max(0.0);
            (grown, grown)
        } else {
            (0.0, 0.0)
        }
    };
    let radii = paint.border_radius;
    [
        grow(radii.top_left),
        grow(radii.top_right),
        grow(radii.bottom_right),
        grow(radii.bottom_left),
    ]
}

/// Clips to everything **outside** `rect`'s box, out to `margin`.
///
/// The complement of [`clip_to_box`], and built the way
/// [`draw_inset_box_shadow`] builds its hole: a surround rectangle and the
/// box's own contour in one path, filled by the even-odd rule, so the box is
/// the hole. There is no difference-clip on this binding, and this is the
/// shape that stands in for one.
///
/// `margin` is how far past the box the surround reaches, and has to clear
/// everything the shadow can paint -- [`shadow_reach`] is that distance. A
/// clip that stopped nearer would cut the ink rather than the box.
fn clip_outside_box(
    context: &mut Context2D,
    paint: &PaintStyle,
    rect: Rect,
    margin: f32,
) -> Result<(), Error> {
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

    let mut outside = contour(surround, [(0.0, 0.0); 4])?;
    let radii = paint.border_radius;
    let hole = contour(
        rect,
        [
            (radii.top_left, radii.top_left),
            (radii.top_right, radii.top_right),
            (radii.bottom_right, radii.bottom_right),
            (radii.bottom_left, radii.bottom_left),
        ],
    )?;
    // `Path2D::add_path` appends a subpath; building the second contour on the
    // *context* extends the first instead, and the joined self-intersecting
    // shape is what `ring_path` documents at length. Built that way this clip
    // came out empty and the shadow vanished outright -- which the `below`
    // probe caught, and no interior probe could have.
    outside.add_path(&hole.build(SkiaFillRule::EvenOdd));
    context.clip_path(
        &outside.build(SkiaFillRule::EvenOdd),
        SkiaFillRule::EvenOdd,
    );
    Ok(())
}

/// How far from `rect`'s edge a shadow's ink can reach.
///
/// Blur, spread and offset, plus the box itself so the figure is an
/// over-estimate rather than an exact bound -- the callers want a margin they
/// cannot be caught short by, not a tight one. Three times the blur is where a
/// Gaussian is spent, which is what the inset path already used and is kept
/// here so the two agree by construction rather than by two edits landing
/// together.
fn shadow_reach(rect: Rect, shadow: &BoxShadow) -> f32 {
    shadow.blur.mul_add(3.0, shadow.spread.abs())
        + shadow.offset_x.abs()
        + shadow.offset_y.abs()
        + rect.size.width
        + rect.size.height
}

/// Sets the fill or stroke source for a painted path.
/// Sets a path's fill or stroke, and reports how a radial gradient among them
/// wants its space squashed.
///
/// `None` for every paint but an elliptical radial, and **`None` for a
/// stroke** whatever the gradient: the squash is a non-uniform scale of the
/// space, which would squash the stroke's own width with it. A radial gradient
/// stroking a path stays a circle, and that is the one place this renderer
/// still draws v1's shape.
fn set_paint(
    context: &mut Context2D,
    paint: &PathPaint,
    rect: Rect,
    fill: bool,
) -> Result<Option<Squash>, Error> {
    match paint {
        PathPaint::Solid(color) => {
            if fill {
                context.set_fill_style(to_skia_color(*color));
            } else {
                context.set_stroke_style(to_skia_color(*color));
            }
        }
        PathPaint::Gradient(gradient) => {
            let (shader, squash) = build_gradient(gradient, rect)?;
            if fill {
                context.set_fill_shader(&shader);
                return Ok(squash);
            }
            context.set_stroke_shader(&shader);
        }
    }
    Ok(None)
}

/// A radial gradient's two radii, from its own centre.
///
/// CSS's default for `radial-gradient` is **`farthest-corner ellipse`**: an
/// ellipse with the aspect ratio of the farthest *sides* that passes through
/// the farthest *corner*. With `dx` and `dy` the distances to the farthest
/// side on each axis, the corner sits at `(dx, dy)`, so a ratio-preserving
/// ellipse through it has `rx = dx * sqrt(2)` and `ry = dy * sqrt(2)`.
///
/// # What this replaced, and why the old comment read as true
///
/// It was half the box's diagonal, from the box's centre, which is
/// `farthest-corner` **only** for a circle at the centre -- so the comment
/// claiming `farthest-corner` was right about the intent and wrong about both
/// the shape and the point. Measured in a 120x60 box with `at 25% 75%`, the
/// old radius fell 31 pixels short of the far corner and the ramp held its
/// last stop flat across everything past it.
///
/// # Measured against Chrome, at the mid-edges
///
/// ```text
///                         left  right  top  bottom
/// ellipse, CSS's default  0.68  0.68   0.67  0.67
/// circle                  0.82  0.81   0.51  0.50
/// ours before this        0.87  0.87   0.42  0.42
/// ```
///
/// **The corners cannot tell the two apart** -- they are equidistant from the
/// centre of a rectangle whichever shape is drawn -- so the mid-edges are the
/// sample, and an ellipse is the one that reads the same at all four.
fn radial_radii(centre: Point, rect: Rect) -> (f32, f32) {
    let right = rect.origin.x + rect.size.width;
    let bottom = rect.origin.y + rect.size.height;
    let dx = (centre.x - rect.origin.x)
        .abs()
        .max((right - centre.x).abs());
    let dy = (centre.y - rect.origin.y)
        .abs()
        .max((bottom - centre.y).abs());
    (
        dx * core::f32::consts::SQRT_2,
        dy * core::f32::consts::SQRT_2,
    )
}

/// How a radial gradient's circle is squashed into its ellipse.
///
/// Skia's radial shader is a circle and this binding exposes no local matrix
/// for it, so the ellipse is made by squashing the **space** the circle is
/// drawn in: clip to the shape, scale about the gradient's centre, fill.
#[derive(Debug, Clone, Copy)]
struct Squash {
    /// The point the scale is about: the gradient's own centre.
    centre: Point,
    /// How much the vertical axis is compressed, `ry / rx`.
    vertical: f32,
}

/// Builds a shader for a gradient placed against a node's box.
fn build_gradient(
    gradient: &Gradient,
    rect: Rect,
) -> Result<(Shader, Option<Squash>), Error> {
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

    let mut squash = None;
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
            let centre = place(at);
            let (horizontal, vertical) = radial_radii(centre, rect);
            // Drawn as the circle of the wider radius and squashed to the
            // narrower one. A degenerate axis leaves it a circle rather than
            // collapsing the fill to a line.
            squash = (horizontal > 0.0 && vertical > 0.0).then_some(Squash {
                centre,
                vertical: vertical / horizontal,
            });
            Shader::radial_gradient(
                centre,
                horizontal,
                &stops,
                GradientInterpolation::default(),
            )
        }
        GradientGeometry::Conic { at, from } => {
            // **CSS starts a conic sweep at twelve o'clock and a canvas
            // starts it at three**, so a `from` handed straight to the shader
            // draws the ramp a quarter turn late -- measured against Chrome as
            // a uniform 270 degrees across every sample of every conic case,
            // with a spread of two bytes that is quantisation rather than
            // variation.
            //
            // v1 converts (`gradient.canvas.ts:42`, `degreesToCanvasAngle`)
            // and this crate did not, so it is a port defect rather than one
            // we invented. The turn is applied to the angle handed to the
            // shader and **not** to `from` itself: a caller's `from` is CSS's,
            // and reinterpreting it would move every angle they wrote.
            // **The whole angle goes into the ramp, including `from`.** The
            // sweep always covers one full turn from Skia's own zero, because
            // Skia clamps outside its range rather than wrapping: a range of
            // `[from, from + 360]` leaves every pixel below `from` reading the
            // first stop, which at `from: 90deg` painted the entire box one
            // flat colour.
            Shader::sweep_gradient(
                place(at),
                0.0,
                DEGREES_PER_TURN,
                &turned(
                    &stops,
                    (from - QUARTER_TURN_DEGREES) / DEGREES_PER_TURN,
                ),
                GradientInterpolation::default(),
            )
        }
    };
    let shader = shader.map_err(|error| Error::Paint(error.to_string()))?;
    Ok((shader, squash))
}

/// Fills a shape with a gradient, squashing the space for an elliptical one.
///
/// A radial gradient is drawn as a circle and made elliptical by scaling the
/// space about its centre — Skia's radial shader is a circle and this binding
/// exposes no local matrix for it. So the shape is clipped first, in its own
/// coordinates, and the fill that follows happens in the squashed space where
/// the circle reads as the ellipse CSS asks for.
///
/// The rectangle filled under that scale is the clip's own bounds stretched by
/// the inverse of it, which is what covers the clip however tall the squash
/// makes it.
fn fill_with_gradient(
    context: &mut Context2D,
    squash: Option<Squash>,
    bounds: Rect,
    shape: impl FnOnce(&mut Context2D) -> Result<(), Error>,
) -> Result<(), Error> {
    let Some(squash) = squash else {
        shape(context)?;
        context.fill(SkiaFillRule::NonZero);
        return Ok(());
    };

    context.save();
    let result = (|context: &mut Context2D| {
        shape(context)?;
        context.clip(SkiaFillRule::NonZero);
        context.translate(squash.centre.x, squash.centre.y);
        context.scale(1.0, squash.vertical);
        context.translate(-squash.centre.x, -squash.centre.y);
        let reach = (bounds.size.height + bounds.size.width)
            / squash.vertical.max(f32::EPSILON);
        context.fill_rect(
            bounds.origin.x - 1.0,
            squash.centre.y - reach,
            bounds.size.width + 2.0,
            reach * 2.0,
        );
        Ok(())
    })(context);
    context.restore();
    result
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
/// The transform that fits a path's `viewBox` into the box it was given.
///
/// **SVG's `xMidYMid meet`, which is its default and the only one here.** One
/// scale for both axes — the smaller, so the whole box fits — and the
/// remainder split evenly, which is what centres it. A per-axis scale would
/// fill the node exactly and distort the drawing; `meet` keeps the shape and
/// leaves letterboxing.
///
/// A zero or negative extent has no scale that means anything, so the path is
/// placed unscaled at the node's origin rather than multiplied by infinity —
/// the same choice as a zero `maxValue` in a chart, and for the same reason: a
/// degenerate input should draw something explicable rather than nothing.
fn view_box_transform(
    view: (f32, f32, f32, f32),
    rect: Rect,
    stretch: bool,
) -> Affine {
    let (min_x, min_y, width, height) = view;
    if width <= 0.0 || height <= 0.0 {
        return Affine {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            tx: rect.origin.x - min_x,
            ty: rect.origin.y - min_y,
        };
    }

    // `none` scales each axis on its own so the drawing fills the node;
    // `meet` takes the smaller for both, which fits it without distorting.
    let (scale_x, scale_y) = if stretch {
        (rect.size.width / width, rect.size.height / height)
    } else {
        let scale = (rect.size.width / width).min(rect.size.height / height);
        (scale, scale)
    };
    Affine {
        a: scale_x,
        b: 0.0,
        c: 0.0,
        d: scale_y,
        // `mul_add` because clippy asks for it and it is right here: this
        // crate is the single implementation both surfaces render through, and
        // nothing compares these numbers bit-for-bit against another engine.
        // The opposite rule holds in `animate`, where the comparison is exact
        // against v1's own output — the boundary is the comparison, not the
        // file.
        // Under `none` the remainder is zero on both axes, so the centring
        // term vanishes and this is the same expression for both cases.
        tx: width
            .mul_add(-scale_x, rect.size.width)
            .mul_add(0.5, min_x.mul_add(-scale_x, rect.origin.x)),
        ty: height
            .mul_add(-scale_y, rect.size.height)
            .mul_add(0.5, min_y.mul_add(-scale_y, rect.origin.y)),
    }
}

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
    // The wildcard is `Mask` being `#[non_exhaustive]`: a mask this build
    // cannot describe clips nothing, which shows the subtree whole rather than
    // hiding it. A mask that silently removed its own content would be a
    // missing picture with nothing to point at.
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
            let (shader, squash) = build_gradient(gradient, rect)?;
            context.set_fill_shader(&shader);
            fill_with_gradient(context, squash, rect, |context| {
                context.begin_path();
                context.rect(
                    rect.origin.x,
                    rect.origin.y,
                    rect.size.width,
                    rect.size.height,
                );
                Ok(())
            })
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
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {

    /// The corner a broken border turns, against Chrome's own alphas.
    ///
    /// # The seam, and when it is correct
    ///
    /// Each edge of a dashed or dotted border is clipped to its half of the
    /// corner and strokes through it, so **both edges draw the corner mark and
    /// each draws half of it.** Two antialiased halves composite source-over
    /// to `1 - (1 - 0.5)^2`, not to one: a light diagonal down the corner.
    ///
    /// **Chrome has that seam too -- exactly when the two edges differ.** Its
    /// two-colour corner reads `0.753` on the diagonal, ours reads `0.753`,
    /// and there the seam is the right answer. Its one-colour corner reads
    /// `1.000` and ours read `0.753`, which was the defect: **a division that
    /// buys nothing, because both halves would be painted identically.**
    ///
    /// So the rule is not *remove the seam* but **do not divide a corner whose
    /// two edges agree**. Both readings are pinned here, because a fix that
    /// removed the seam everywhere would pass a test that only checked the
    /// first.
    ///
    /// Chrome measured by MC Main: dotted border, width 8, box 60x60,
    /// `border-top-color: #ff0000; border-left-color: #0000ff` for the
    /// two-colour case, through `foreignObject` to canvas and `getImageData`.
    /// Ours are read from a transparent page, so the alpha channel is the
    /// coverage with nothing composited under it.
    /// A corner radius Skia cannot use is a square corner, not a failed paint.
    ///
    /// **This is the one row of the bad-value grid that threw rather than
    /// drawing the wrong thing.** `border-radius: NaN` reached Skia, which
    /// refused the whole rectangle -- `invalid rect: Rect { .. }` -- so a
    /// render that was going to succeed returned an error instead. Chrome
    /// drops the declaration and computes `0px`.
    ///
    /// The radius is normalised in `box_path_continuing` rather than beside
    /// the layout normalisation, because a corner radius is never a layout
    /// input: the two are the same rule at the two places values are used.
    mod unusable_radius {
        use meo_canvas_scene::{
            Corners, Scene, Size,
            node::{Node, NodeId, NodeKind},
            style::{Dimension, paint::Color},
        };

        use crate::{ImageFormat, Renderer, encode::EncodeOptions};

        fn paints(radius: f32) -> bool {
            let mut scene = Scene::new(Size::new(40.0, 30.0));
            let id = scene
                .push(NodeId::ROOT, Node::new(NodeKind::Box))
                .unwrap_or_else(|error| unreachable!("{error}"));
            if let Some(node) = scene.get_mut(id) {
                node.layout.size =
                    (Dimension::Points(40.0), Dimension::Points(30.0));
                node.paint.background_color = Color::rgb(255, 0, 0);
                node.paint.border_radius = Corners::all(radius);
            }
            let mut renderer = Renderer::new();
            renderer.set_gpu(false);
            renderer
                .render_to_buffer(
                    &scene,
                    ImageFormat::Png,
                    &EncodeOptions::default(),
                )
                .is_ok()
        }

        #[test]
        fn a_non_finite_or_negative_radius_paints_a_square_corner() {
            for radius in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -20.0] {
                assert!(paints(radius), "radius {radius} refused the paint");
            }
            // The control: a radius that is usable still paints, so the test
            // cannot be satisfied by a renderer that succeeds at everything
            // because it draws nothing.
            assert!(paints(8.0));
        }
    }

    mod corner_seam {
        use meo_canvas_scene::{
            Corners, Scene, Sides, Size,
            node::{Node, NodeId, NodeKind},
            style::{
                Dimension,
                paint::{BorderStyle, Color},
            },
        };

        use crate::{ImageFormat, Renderer, encode::EncodeOptions};

        /// One ordinary dot from the middle of a straight top run.
        ///
        /// Returns a window two pixels clear of the mark on every side, so
        /// its rim and the blank beside it are both in the reading.
        fn dot(width: f32) -> (usize, f64, Vec<String>) {
            let mut scene = Scene::new(Size::new(240.0, 48.0));
            let id = scene
                .push(NodeId::ROOT, Node::new(NodeKind::Box))
                .unwrap_or_else(|error| unreachable!("{error}"));
            if let Some(node) = scene.get_mut(id) {
                node.layout.size =
                    (Dimension::Points(240.0), Dimension::Points(48.0));
                node.layout.border = Sides::all(width);
                node.paint.border_style = BorderStyle::Dotted;
                node.paint.border_color_all = Color::rgb(0, 0, 0);
            }
            let (stride, pixels) = render(&scene);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a border width of two to sixteen pixels"
            )]
            let band = width as usize;
            let alpha = |x: usize, y: usize| {
                u32::from(pixels[(((y * stride) + x) * 4) + 3])
            };
            // The first mark whose left edge is past the middle, so the
            // window is far from both corners whatever the fitted rhythm is.
            // The middle row of the band, not its far edge: `band` itself is
            // the first row *outside* a border of that width, where every
            // scan reads blank and the search finds nothing it is looking
            // for.
            let middle = (band / 2).max(1);
            // **At Chrome's threshold, not at any ink at all.** A run
            // pattern is read at alpha 0.5; scanning for `> 0` stops on the
            // antialiased fringe a column early, which shifts the whole
            // window and pulls the previous mark's tail into the sum. That
            // cost a `6.008` against Chrome's `4.000` and read as a rhythm
            // disagreement.
            let lit = |x: usize| alpha(x, middle) >= 128;
            let mut left = 120;
            while left < 200 && lit(left) {
                left += 1;
            }
            while left < 200 && !lit(left) {
                left += 1;
            }
            let start = left.saturating_sub(2);
            let mut ink = 0.0;
            for y in 0..band + 3 {
                // `start - 2` through `start + w + 1` inclusive, which is the
                // window the grids this is compared against use.
                for x in start..start + band + 4 {
                    ink += f64::from(alpha(x, y)) / 255.0;
                }
            }
            let rows: Vec<String> = (0..band + 3)
                .map(|y| {
                    (start..start + band + 4)
                        .map(|x| {
                            let value = alpha(x, y);
                            if value == 0 {
                                '.'
                            } else {
                                char::from(
                                    b'0' + u8::try_from(
                                        (value * 9 + 127) / 255,
                                    )
                                    .unwrap_or(9),
                                )
                            }
                        })
                        .collect()
                })
                .collect();
            (left, ink, rows)
        }

        /// The corner's top-left 8x8, as alpha digits.
        fn grid(colour: Color) -> Vec<String> {
            let mut scene = Scene::new(Size::new(60.0, 60.0));
            let id = scene
                .push(NodeId::ROOT, Node::new(NodeKind::Box))
                .unwrap_or_else(|error| unreachable!("{error}"));
            if let Some(node) = scene.get_mut(id) {
                node.layout.size =
                    (Dimension::Points(60.0), Dimension::Points(60.0));
                node.layout.border = Sides::all(8.0);
                node.paint.border_style = BorderStyle::Dotted;
                node.paint.border_color_all = colour;
            }
            let (stride, pixels) = render(&scene);
            (0..8)
                .map(|y| {
                    (0..8)
                        .map(|x| {
                            let alpha =
                                u32::from(pixels[(((y * stride) + x) * 4) + 3]);
                            // **Rounded, not truncated.** The grid this is
                            // compared against rounds, and a digit scale read
                            // one way against a grid written the other differs
                            // by one at the rim for no reason at all -- two
                            // conventions, read as two renderers.
                            if alpha == 0 {
                                '.'
                            } else {
                                char::from(
                                    b'0' + u8::try_from(
                                        (alpha * 9 + 127) / 255,
                                    )
                                    .unwrap_or(9),
                                )
                            }
                        })
                        .collect()
                })
                .collect()
        }

        /// Coverage down the corner's diagonal for one colour.
        fn diagonal_at(colour: Color) -> Vec<f64> {
            let mut scene = Scene::new(Size::new(60.0, 60.0));
            let id = scene
                .push(NodeId::ROOT, Node::new(NodeKind::Box))
                .unwrap_or_else(|error| unreachable!("{error}"));
            if let Some(node) = scene.get_mut(id) {
                node.layout.size =
                    (Dimension::Points(60.0), Dimension::Points(60.0));
                node.layout.border = Sides::all(8.0);
                node.paint.border_style = BorderStyle::Dotted;
                node.paint.border_color_all = colour;
            }
            read_diagonal(&scene)
        }

        /// Coverage down the corner's diagonal, `(1, 1)` to `(5, 5)`.
        fn diagonal(two_colour: bool, style: BorderStyle) -> Vec<f64> {
            let mut scene = Scene::new(Size::new(60.0, 60.0));
            let id = scene
                .push(NodeId::ROOT, Node::new(NodeKind::Box))
                .unwrap_or_else(|error| unreachable!("{error}"));
            if let Some(node) = scene.get_mut(id) {
                node.layout.size =
                    (Dimension::Points(60.0), Dimension::Points(60.0));
                node.layout.border = Sides::all(8.0);
                node.paint.border_style = style;
                if two_colour {
                    node.paint.border_color = Sides {
                        top: Some(Color::rgb(255, 0, 0)),
                        right: Some(Color::rgb(255, 0, 0)),
                        bottom: Some(Color::rgb(0, 0, 255)),
                        left: Some(Color::rgb(0, 0, 255)),
                    };
                } else {
                    node.paint.border_color_all = Color::rgb(0, 0, 0);
                }
            }
            read_diagonal(&scene)
        }

        /// Renders one scene and reads the corner diagonal out of it.
        fn read_diagonal(scene: &Scene) -> Vec<f64> {
            let (stride, pixels) = render(scene);
            (1..6)
                .map(|n| {
                    f64::from(pixels[(((n * stride) + n) * 4) + 3]) / 255.0
                })
                .collect()
        }

        /// One render, as `(stride, RGBA bytes)`.
        fn render(scene: &Scene) -> (usize, Vec<u8>) {
            let mut renderer = Renderer::new();
            // The two rasterisers do not agree to the byte, and this reads
            // bytes.
            renderer.set_gpu(false);
            let png = renderer
                .render_to_buffer(
                    scene,
                    ImageFormat::Png,
                    &EncodeOptions::default(),
                )
                .unwrap_or_else(|error| unreachable!("{error}"));
            let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
            decoder.set_transformations(
                png::Transformations::normalize_to_color8()
                    | png::Transformations::ALPHA,
            );
            let mut reader = decoder
                .read_info()
                .unwrap_or_else(|error| unreachable!("{error}"));
            let mut pixels = vec![0; reader.output_buffer_size().unwrap_or(0)];
            let info = reader
                .next_frame(&mut pixels)
                .unwrap_or_else(|error| unreachable!("{error}"));
            pixels.truncate(info.buffer_size());
            (info.width as usize, pixels)
        }

        /// Prints the corner as a grid of alpha digits, for eyes.
        ///
        /// **A single cell cannot tell a mark drawn in the right dash phase
        /// from one drawn in the wrong phase at the same coverage.** The
        /// assertions below read the diagonal; this reads the whole 8x8, and
        /// it is what a change to the corner's geometry should be compared
        /// against before and after.
        ///
        /// Chrome's own, measured by MC Main -- 60x60, `border: 8px dotted`,
        /// opaque beside `rgba(0, 0, 0, 0.5)`:
        ///
        /// ```text
        /// .599995.        .245542.
        /// 59999994        25555552
        /// 99999999        45555554
        /// 99999999        45555554
        /// 99999999        45555554
        /// 99999999        45555554
        /// 59999994        25555552
        /// .599994.        .245442.
        /// ```
        ///
        /// **The translucent grid is the opaque one at half alpha, cell for
        /// cell** -- no cell reaches 7, which is what a doubled composite
        /// would give. One mark at one alpha, not two halves.
        ///
        /// # The digit scale, because two conventions read as two renderers
        ///
        /// **Every grid here is `round(alpha * 9 / 255)`, and so is this
        /// printer.** It was `alpha * 9 / 255` truncated for one afternoon,
        /// and against the same rounded grids ours read `.489984.` where
        /// Chrome read `.599995.` -- a whole cell out at every rim pixel,
        /// from a renderer that agreed exactly. **Truncation loses a digit
        /// wherever coverage is just under a step**, which at a mark's
        /// antialiased rim is most of it.
        ///
        /// So: compare a grid only against one written in the same
        /// convention, and say which convention it is. `254` is `9` here and
        /// `8` under truncation, and nothing in the picture says which you
        /// are looking at.
        ///
        /// `cargo test -p meo-canvas-core --lib corner_grid -- --ignored
        /// --nocapture`
        #[test]
        fn a_dots_ink_is_chromes_from_four_pixels_up() {
            // **The reading that killed "our dot is smaller".** Total ink over
            // one window, against Chrome measured by MC Main at the same
            // anchor: two columns before the mark, ending `w + 1` after it
            // starts.
            //
            // The inference that our mark was undersized came from the corner
            // grid, whose rim was two digits light -- **and a corner mark
            // carries corner geometry.** On a straight run the marks are the
            // same size to within four hundredths of an ink unit out of two
            // hundred. **That inference is refuted, and it is pinned here so
            // it cannot be reopened from the corner grid**, which is still
            // there and still suggests it.
            //
            // What remains is a sub-pixel horizontal offset: same size, same
            // ink, coverage distributed a little differently between
            // neighbouring pixels.
            for (width, chrome) in
                [(4.0_f32, 11.988_f64), (8.0, 51.996), (16.0, 198.722)]
            {
                let (_, ink, _) = dot(width);
                assert!(
                    (ink - chrome).abs() < 0.1,
                    "width {width}: our dot is {ink:.3} of ink against \
                     Chrome's {chrome:.3}"
                );
            }
        }

        #[test]
        fn a_dot_squares_below_four_pixels_and_rounds_at_four() {
            // **Both sides of the boundary, because a fix that squares
            // everything passes a test that only checks the small side.**
            //
            // Discriminated by area rather than by looking: a square of width
            // `w` is `w^2` of ink and a circle inscribed in it is `pi/4` of
            // that, about `0.785 w^2`. At three those are `9` and `7.07`; at
            // four, `16` and `12.57`. **Nothing else about the mark has to be
            // agreed for the two to be told apart.**
            //
            // Chrome measured by MC Main at every width from one to seven:
            // exact integers with no antialiasing at 1, 2 and 3 -- `3.000`,
            // `4.000`, `9.000` -- and rimmed circles from 4. **An integer is
            // the proof: a circle cannot produce one at any subpixel
            // position.**
            let (_, three, _) = dot(3.0);
            assert!(
                three > 8.5,
                "a 3-pixel dot is {three:.3} of ink, nearer a circle's 7.07 \
                 than a square's 9 -- Chrome squares below four"
            );
            let (_, four, _) = dot(4.0);
            assert!(
                four < 13.5,
                "a 4-pixel dot is {four:.3} of ink, nearer a square's 16 \
                 than a circle's 12.57 -- Chrome rounds from four"
            );
        }

        /// Walks our top-left quarter arc the way the Chrome table walks it.
        ///
        /// **Zero is the LEFT tangent**, sweeping through the diagonal to the
        /// top -- `walkArc(cx, cy, Math.PI)` in
        /// `packages/meo-canvas/tools/conformance/borders.mjs`, which starts
        /// at angle π and adds a quarter turn. Read the other way round every
        /// run below is mirrored and the comparison is of two different
        /// walks.
        ///
        /// Radius, sampling and threshold all follow that generator: the
        /// centre path's radius is `radius - width / 2`, six hundred steps,
        /// each sample **floored** to a pixel, ink is red below 128 on white.
        ///
        /// Chrome at `radius 8, width 4`, `border-rhythm.tsv` line 71:
        ///
        /// ```text
        /// quarter=9.4  first-ink@0  on:4.4 off:4.1 on:1.0
        /// ```
        ///
        /// `cargo test -p meo-canvas-core --lib arc_walk -- --ignored
        /// --nocapture`
        #[test]
        #[ignore = "prints a walk rather than asserting one"]
        fn arc_walk() {
            for (radius, width) in [(8.0_f32, 4.0_f32), (6.0, 4.0), (5.0, 4.0)]
            {
                let (stride, pixels) = dashed_box(radius, width);
                let (quarter, first, runs) =
                    walk_quarter(&pixels, stride, radius, width);
                eprintln!(
                    "r{radius} w{width}  quarter={quarter:.1} \
                     first-ink@{first}  {}",
                    runs.join(" ")
                );
                eprintln!(
                    "        straight  {}",
                    straight_runs(&pixels, stride, radius, width).join(" ")
                );
            }
        }

        /// A dashed box with a radius, on white, as `(stride, RGBA)`.
        fn dashed_box(radius: f32, width: f32) -> (usize, Vec<u8>) {
            let mut scene = Scene::new(Size::new(240.0, 48.0));
            if let Some(root) = scene.get_mut(NodeId::ROOT) {
                root.paint.background_color = Color::rgb(255, 255, 255);
            }
            let id = scene
                .push(NodeId::ROOT, Node::new(NodeKind::Box))
                .unwrap_or_else(|error| unreachable!("{error}"));
            if let Some(node) = scene.get_mut(id) {
                node.layout.size =
                    (Dimension::Points(240.0), Dimension::Points(48.0));
                node.layout.border = Sides::all(width);
                node.paint.background_color = Color::rgb(255, 255, 255);
                node.paint.border_color_all = Color::rgb(0, 0, 0);
                node.paint.border_style = BorderStyle::Dashed;
                node.paint.border_radius = Corners::all(radius);
            }
            render(&scene)
        }

        /// The top-left quarter's runs, from the LEFT tangent to the top.
        ///
        /// Returns `(quarter length, where ink starts, the runs)`.
        fn walk_quarter(
            pixels: &[u8],
            stride: usize,
            radius: f32,
            width: f32,
        ) -> (f32, String, Vec<String>) {
            let along = (radius - width / 2.0).max(0.0);
            let quarter = std::f32::consts::FRAC_PI_2 * along;
            let steps = 600_i32;
            let mut runs: Vec<String> = Vec::new();
            let mut ink: Option<bool> = None;
            let mut start = 0_i32;
            let mut first: Option<String> = None;
            let length = |from: i32, to: i32| {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "six hundred steps"
                )]
                let span = (to - from) as f32 / steps as f32;
                span * quarter
            };
            for step in 0..=steps {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "six hundred steps"
                )]
                let fraction = step as f32 / steps as f32;
                let angle = fraction
                    .mul_add(std::f32::consts::FRAC_PI_2, std::f32::consts::PI);
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "inside a 240x48 page"
                )]
                let (px, py) = (
                    along.mul_add(angle.cos(), radius).floor() as usize,
                    along.mul_add(angle.sin(), radius).floor() as usize,
                );
                let here = pixels[((py * stride) + px) * 4] < 128;
                let Some(was) = ink else {
                    ink = Some(here);
                    if here {
                        first = Some("0".to_owned());
                    }
                    continue;
                };
                if here != was {
                    runs.push(format!(
                        "{}:{:.1}",
                        if was { "on" } else { "off" },
                        length(start, step)
                    ));
                    if here && first.is_none() {
                        first = Some(format!("{:.1}", length(0, step)));
                    }
                    ink = Some(here);
                    start = step;
                }
            }
            runs.push(format!(
                "{}:{:.1}",
                if ink == Some(true) { "on" } else { "off" },
                length(start, steps)
            ));
            (quarter, first.unwrap_or_else(|| "-".to_owned()), runs)
        }

        /// The same box's straight run, for the comparison that needs no
        /// browser.
        ///
        /// If the arc and the side disagree here, the disagreement is inside
        /// one renderer and cannot be a window, a threshold or a convention.
        fn straight_runs(
            pixels: &[u8],
            stride: usize,
            radius: f32,
            width: f32,
        ) -> Vec<String> {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "a radius and width of a few pixels"
            )]
            let (from, to, row) = (
                radius as usize,
                240 - radius as usize,
                (width / 2.0) as usize,
            );
            let mut runs: Vec<String> = Vec::new();
            let mut lit = pixels[((row * stride) + from) * 4] < 128;
            let mut start = from;
            for x in from..to {
                let here = pixels[((row * stride) + x) * 4] < 128;
                if here != lit {
                    runs.push(format!(
                        "{}:{}",
                        if lit { "on" } else { "off" },
                        x - start
                    ));
                    lit = here;
                    start = x;
                }
            }
            runs.push(format!(
                "{}:{}",
                if lit { "on" } else { "off" },
                to - start
            ));
            runs.truncate(9);
            runs
        }

        /// Prints the curved corner of a dashed border, against Chrome's.
        ///
        /// The `borders-dashed-radius` case: `240x48`, `border: 4px dashed`,
        /// `border-radius: 8px`, top-left. **Chrome, measured by MC Main:**
        ///
        /// ```text
        /// .......79
        /// .......79
        /// .23....69
        /// .594...59
        /// 29994..0.
        /// 59996....
        /// 79992....
        /// 89991....
        /// 9999.....
        /// ```
        ///
        /// **The diagonal numbers that opened this -- `0.635 0.753 0.753`
        /// against Chrome's `0.325 0.412 0.439` -- are not a coverage
        /// difference.** Widened to sixteen columns the grids show why:
        /// Chrome's arc carries a **gap** across the diagonal, blank from
        /// column 0 to 6 in its first four rows, and ours carries a **mark**
        /// there. Comparing alphas at those cells reads our ink against the
        /// fringe of their gap.
        ///
        /// So the question is the dash phase around the arc, not the band's
        /// thickness, and *twice the coverage* was an artefact of sampling a
        /// mark against a gap. **Both engines put marks on the vertical part
        /// of the corner and only one puts one on the diagonal.**
        ///
        /// `cargo test -p meo-canvas-core --lib curve_grid -- --ignored
        /// --nocapture`
        #[test]
        #[ignore = "prints a grid rather than asserting one"]
        fn curve_grid() {
            let mut scene = Scene::new(Size::new(240.0, 48.0));
            let id = scene
                .push(NodeId::ROOT, Node::new(NodeKind::Box))
                .unwrap_or_else(|error| unreachable!("{error}"));
            if let Some(node) = scene.get_mut(id) {
                node.layout.size =
                    (Dimension::Points(240.0), Dimension::Points(48.0));
                node.layout.border = Sides::all(4.0);
                node.paint.border_style = BorderStyle::Dashed;
                node.paint.border_color_all = Color::rgb(0, 0, 0);
                node.paint.border_radius = Corners::all(8.0);
            }
            let (stride, pixels) = render(&scene);
            let mut ink = 0.0;
            for y in 0..16_usize {
                let row: String = (0..16)
                    .map(|x| {
                        let alpha =
                            u32::from(pixels[(((y * stride) + x) * 4) + 3]);
                        ink += f64::from(alpha) / 255.0;
                        if alpha == 0 {
                            '.'
                        } else {
                            char::from(
                                b'0' + u8::try_from((alpha * 9 + 127) / 255)
                                    .unwrap_or(9),
                            )
                        }
                    })
                    .collect();
                eprintln!("  {row}");
            }
            eprintln!("  total ink {ink:.3}");
        }

        /// Prints an ordinary dot from a straight run, at four widths.
        ///
        /// **Away from every corner**, so no corner rule is in the reading:
        /// the dot nearest the middle of a 240-wide top edge. The corner
        /// grids answer where a mark goes; this answers **how big it is**.
        ///
        /// Why four widths rather than one: **a diameter wrong by a constant
        /// and one wrong by a ratio are the same picture at a single width.**
        /// CSS Backgrounds 3 makes a dot a circle of the border's width, so
        /// the candidates are an inset of a fixed fraction of a pixel against
        /// a scale just under one, and only a spread of widths separates
        /// them.
        ///
        /// Digits are `round(alpha * 9 / 255)`, as everything else here.
        ///
        /// `cargo test -p meo-canvas-core --lib dot_grid -- --ignored
        /// --nocapture`
        #[test]
        #[ignore = "prints a grid rather than asserting one"]
        fn dot_grid() {
            for width in [2.0_f32, 4.0, 8.0, 16.0] {
                let (start, ink, rows) = dot(width);
                // **Total ink separates a displaced mark from a smaller
                // one.** An offset moves coverage between pixels and keeps
                // the sum; a diameter error changes it. Neither grid answers
                // that by eye.
                eprintln!(
                    "--- width {width}, starts x={start}, total ink {ink:.3}"
                );
                for row in rows {
                    eprintln!("  {row}");
                }
            }
        }

        #[test]
        #[ignore = "prints a grid rather than asserting one"]
        fn corner_grid() {
            for (name, colour) in [
                ("opaque", Color::rgb(0, 0, 0)),
                ("half", Color::rgba(0, 0, 0, 128)),
            ] {
                eprintln!("--- {name} ---");
                for row in grid(colour) {
                    eprintln!("  {row}");
                }
            }
        }

        #[test]
        fn a_corner_between_matching_edges_is_solid() {
            for style in [BorderStyle::Dotted, BorderStyle::Dashed] {
                for (index, alpha) in
                    diagonal(false, style).into_iter().enumerate()
                {
                    assert!(
                        (alpha - 1.0).abs() < 0.02,
                        "{style:?}: the diagonal is {alpha:.3} at {index}, \
                         where Chrome is 1.000 -- the corner is being \
                         divided between two edges that agree"
                    );
                }
            }
        }

        #[test]
        fn a_translucent_corner_is_one_mark_at_one_alpha() {
            // **The reading that ownership exists for.** Divided, both edges
            // draw the corner mark and it composites with itself:
            // `1 - (1 - a)^2`, so a half-alpha border came out at three
            // quarters down the middle of its own mark. Owned by one edge it
            // is drawn once and the mark is the source alpha.
            //
            // Chrome measured by MC Main: `0.502` for `rgba(0, 0, 0, .5)` on
            // a square corner, and its whole 8x8 grid is the opaque grid at
            // half alpha, cell for cell -- **no cell reaches the doubled
            // value anywhere.**
            //
            // Read inside the mark rather than at its rim: the rim is where
            // our dot and Chrome's differ in size, which is a separate
            // difference this test is not about.
            let alpha = diagonal_at(Color::rgba(0, 0, 0, 128));
            for (index, value) in alpha.into_iter().enumerate().take(4).skip(2)
            {
                assert!(
                    (value - 0.502).abs() < 0.03,
                    "the diagonal is {value:.3} at {index}, where Chrome is \
                     0.502 -- a translucent corner is being drawn twice"
                );
            }
        }

        #[test]
        fn one_drawn_edge_takes_the_whole_corner_square() {
            // **Both orientations, because a rule that names the drawn edge
            // in one and not the other would pass a single test.** With only
            // the top border drawn the corner belongs to no neighbour; with
            // only the left border drawn the same square belongs to no
            // neighbour from the other side. Chrome fills it solid either
            // way, measured by MC Main at `240x48`: ten rows of `9` under
            // `border-top` alone, ten columns of `9` beside `border-left`
            // alone, and **no diagonal in either.**
            //
            // **This pins agreement, not a fix.** The start-corner
            // convention would hand such a corner to an edge that draws
            // nothing -- but the handover is pulled back by *the neighbour's
            // width*, so a neighbour of zero width pulls it back by nothing
            // and the corner stays inside the drawn edge's territory. It was
            // already right, measured before the branch below it existed:
            // replacing that branch with the unconditional form leaves this
            // test green.
            //
            // So the branch states the rule and this states the behaviour.
            // **Neither is the other's guard**, and a reader should not read
            // a passing run here as evidence that the branch works.
            for (top, left) in [(8.0_f32, 0.0_f32), (0.0, 8.0)] {
                let mut scene = Scene::new(Size::new(60.0, 60.0));
                let id = scene
                    .push(NodeId::ROOT, Node::new(NodeKind::Box))
                    .unwrap_or_else(|error| unreachable!("{error}"));
                if let Some(node) = scene.get_mut(id) {
                    node.layout.size =
                        (Dimension::Points(60.0), Dimension::Points(60.0));
                    node.layout.border = Sides {
                        top,
                        right: 0.0,
                        bottom: 0.0,
                        left,
                    };
                    node.paint.border_style = BorderStyle::Dashed;
                    node.paint.border_color_all = Color::rgb(0, 0, 0);
                }
                let (stride, pixels) = render(&scene);
                let inside = |x: usize, y: usize| {
                    f64::from(pixels[(((y * stride) + x) * 4) + 3]) / 255.0
                };
                for (x, y) in [(2_usize, 2_usize), (5, 5), (2, 5), (5, 2)] {
                    let alpha = inside(x, y);
                    assert!(
                        alpha > 0.97,
                        "top {top}, left {left}: ({x}, {y}) is {alpha:.3} \
                         where Chrome is solid -- the corner was handed to an \
                         edge that draws nothing"
                    );
                }
            }
        }

        #[test]
        fn a_corner_between_differing_edges_keeps_chromes_seam() {
            // **The half of this that a naive fix breaks.** Removing the
            // division everywhere would make this 1.000 and disagree with
            // Chrome in the other direction.
            for style in [BorderStyle::Dotted, BorderStyle::Dashed] {
                for (index, alpha) in
                    diagonal(true, style).into_iter().enumerate().skip(1)
                {
                    assert!(
                        (alpha - 0.753).abs() < 0.02,
                        "{style:?}: the diagonal is {alpha:.3} at {index}, \
                         where Chrome is 0.753 -- two differing edges each \
                         paint half of this corner and the seam is real"
                    );
                }
            }
        }
    }

    /// The transform a `viewBox` produces, checked against SVG's own rule.
    ///
    /// Verified against a render before these were written: a `0 0 10 10` box
    /// in a 100x50 node at (20, 10) draws its unit square at `x 45..94,
    /// y 10..59` — scale 5, fifty wide, centred horizontally with twenty-five
    /// pixels either side and flush vertically. The numbers below are that
    /// reading turned into arithmetic.
    mod view_box {
        use meo_canvas_scene::{Point, Rect, Size};

        use super::super::view_box_transform;

        fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
            Rect {
                origin: Point { x, y },
                size: Size { width, height },
            }
        }

        #[test]
        fn meet_takes_the_smaller_scale_so_the_whole_box_fits() {
            // 100/10 is 10 and 50/10 is 5; `meet` takes 5, which is what keeps
            // the drawing's shape. A per-axis scale would fill the node and
            // distort it.
            let transform = view_box_transform(
                (0.0, 0.0, 10.0, 10.0),
                rect(20.0, 10.0, 100.0, 50.0),
                false,
            );
            assert!((transform.a - 5.0).abs() < f32::EPSILON);
            assert!((transform.d - 5.0).abs() < f32::EPSILON);
        }

        #[test]
        fn the_remainder_is_split_evenly_which_is_what_centres_it() {
            // Fifty wide inside a hundred leaves fifty, so twenty-five each
            // side: 20 + 25 = 45. Vertically it is flush, so the origin is
            // untouched.
            let transform = view_box_transform(
                (0.0, 0.0, 10.0, 10.0),
                rect(20.0, 10.0, 100.0, 50.0),
                false,
            );
            assert!((transform.tx - 45.0).abs() < f32::EPSILON);
            assert!((transform.ty - 10.0).abs() < f32::EPSILON);
        }

        #[test]
        fn a_min_corner_shifts_the_drawing_rather_than_scaling_it() {
            // `min-x` and `min-y` say where the drawing's own origin is, so a
            // box starting at (2, 2) moves the picture by two scaled units.
            let plain = view_box_transform(
                (0.0, 0.0, 10.0, 10.0),
                rect(0.0, 0.0, 50.0, 50.0),
                false,
            );
            let shifted = view_box_transform(
                (2.0, 2.0, 10.0, 10.0),
                rect(0.0, 0.0, 50.0, 50.0),
                false,
            );
            assert!(
                (shifted.a - plain.a).abs() < f32::EPSILON,
                "the scale is unchanged"
            );
            assert!((plain.tx - shifted.tx - 10.0).abs() < f32::EPSILON);
        }

        #[test]
        fn a_degenerate_box_places_the_path_rather_than_multiplying_by_infinity()
         {
            // A zero extent has no scale that means anything. Drawing at the
            // node's origin unscaled is explicable; NaN coordinates are not,
            // and would draw nothing while looking like a renderer fault.
            let transform = view_box_transform(
                (0.0, 0.0, 0.0, 10.0),
                rect(7.0, 9.0, 50.0, 50.0),
                false,
            );
            assert!((transform.a - 1.0).abs() < f32::EPSILON);
            assert!((transform.tx - 7.0).abs() < f32::EPSILON);
            assert!((transform.ty - 9.0).abs() < f32::EPSILON);
        }
    }

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
        radial_radii, resolve_length, ring_path, scale_filter_lengths,
        to_skia_blend, to_skia_color,
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
    fn a_positioned_box_paints_above_an_in_flow_one_whatever_the_order() {
        // Measured against Chrome across 231 combinations of position and
        // container: 66 disagreed, every one of them `relative`, `absolute`
        // or `sticky` against `static`, and every one because this list was
        // sorted by `z` alone and so kept document order. CSS 2.1 Appendix E
        // paints in-flow non-positioned descendants at steps 3 and 5 and
        // everything positioned at step 6.
        let mut scene = Scene::new(Size::new(40.0, 40.0));
        let mut positioned = Node::container();
        positioned.layout.position_type = PositionType::Relative;
        let first = scene
            .push(NodeId::ROOT, positioned)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let second = scene
            .push(NodeId::ROOT, Node::container())
            .unwrap_or_else(|error| unreachable!("{error}"));

        // The static box is written second and paints first, so the
        // positioned one is on top.
        assert_eq!(
            ordered_ids(&scene, NodeId::ROOT),
            vec![second, first],
            "a relative box should paint above a static one written after it"
        );
    }

    #[test]
    fn an_explicit_zero_ranks_with_auto_and_not_above_it() {
        // CSS step 6 holds positioned descendants with `auto` and child
        // stacking contexts with `0` **together**, in tree order. So these two
        // do not rank against each other and the later one wins, however the
        // index is spelled -- measured against Chrome in three rows, one per
        // container kind, where ranking the explicit zero above the automatic
        // one put the wrong box on top.
        //
        // Nested one level down, because that is where the two spellings came
        // apart: an explicit index starts its key afresh at the context root,
        // and a zero doing that would overtake an `auto` sibling written after
        // it.
        let mut scene = Scene::new(Size::new(40.0, 40.0));
        let container = scene
            .push(NodeId::ROOT, Node::container())
            .unwrap_or_else(|error| unreachable!("{error}"));

        let mut zero = Node::container();
        zero.layout.position_type = PositionType::Relative;
        zero.paint.z_index = Some(0);
        let zero = scene
            .push(container, zero)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let mut auto = Node::container();
        auto.layout.position_type = PositionType::Relative;
        let auto = scene
            .push(container, auto)
            .unwrap_or_else(|error| unreachable!("{error}"));

        assert_eq!(
            ordered_ids(&scene, NodeId::ROOT),
            vec![container, zero, auto],
            "`z_index: 0` written first should paint below an `auto` sibling \
             written after it"
        );
    }

    #[test]
    fn a_descendant_still_paints_after_the_box_it_sits_in() {
        // The invariant the positioned rule must not break, and did once: a
        // flat "is it positioned" key sorted a static grandchild *before* its
        // own positioned parent, whose background then covered it, and
        // `display: block` below the page root painted no children at all.
        let mut scene = Scene::new(Size::new(40.0, 40.0));
        let mut positioned = Node::container();
        positioned.layout.position_type = PositionType::Relative;
        let parent = scene
            .push(NodeId::ROOT, positioned)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let child = scene
            .push(parent, Node::container())
            .unwrap_or_else(|error| unreachable!("{error}"));
        let sibling = scene
            .push(NodeId::ROOT, Node::container())
            .unwrap_or_else(|error| unreachable!("{error}"));

        // The static sibling first, then the positioned parent, then its own
        // child -- which is inside it and cannot overtake it.
        assert_eq!(
            ordered_ids(&scene, NodeId::ROOT),
            vec![sibling, parent, child]
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
    fn a_radial_gradient_is_an_ellipse_measured_from_its_own_centre() {
        use meo_canvas_scene::Point as ScenePoint;
        use meo_skia_canvas::Point;

        let rect = Rect::new(ScenePoint::new(0.0, 0.0), Size::new(120.0, 60.0));

        // Centred: CSS's farthest-corner ellipse has the farthest-side ratio
        // and passes through the farthest corner, so each radius is that
        // side's distance times the square root of two.
        let (rx, ry) = radial_radii(Point::new(60.0, 30.0), rect);
        assert!(core::f32::consts::SQRT_2.mul_add(-60.0, rx).abs() < 0.001);
        assert!(core::f32::consts::SQRT_2.mul_add(-30.0, ry).abs() < 0.001);

        // **The property that separates an ellipse from a circle**: every
        // mid-edge sits at the same fraction along the ramp. A circle reads
        // 0.82 and 0.51 at the same points; the four corners cannot tell them
        // apart at all, which is why they are the wrong sample.
        assert!(((60.0 / rx) - (30.0 / ry)).abs() < 0.001);

        // Off-centre, both radii are measured from the point given. At
        // 25% 75% of this box the centre is (30, 45), whose farthest sides
        // are 90 across and 45 down.
        let (rx, ry) = radial_radii(Point::new(30.0, 45.0), rect);
        assert!(core::f32::consts::SQRT_2.mul_add(-90.0, rx).abs() < 0.001);
        assert!(core::f32::consts::SQRT_2.mul_add(-45.0, ry).abs() < 0.001);

        // A centre outside the box still measures every side: `at` is a
        // length, not a fraction clamped to the box.
        let (rx, ry) = radial_radii(Point::new(-40.0, -40.0), rect);
        assert!(core::f32::consts::SQRT_2.mul_add(-160.0, rx).abs() < 0.001);
        assert!(core::f32::consts::SQRT_2.mul_add(-100.0, ry).abs() < 0.001);
    }

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
                    view_box: None,
                    stretch: false,
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
                    view_box: None,
                    stretch: false,
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
                    view_box: None,
                    stretch: false,
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
                    view_box: None,
                    stretch: false,
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
