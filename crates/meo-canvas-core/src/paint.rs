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

use meo_canvas_scene::{
    Rect, Size,
    node::{ImageSource, Node, NodeId, NodeKind, PathPaint},
    style::{
        Length,
        effect::{BoxShadow, Effects, FillRule, Mask, MaskShape, Transform},
        layout::Overflow,
        paint::{
            BlendMode, Color, Gradient, GradientKind, ObjectFit, PaintStyle,
        },
    },
};
use meo_skia_canvas::{
    BlendMode as SkiaBlendMode, Canvas, CanvasOptions, Context2D,
    FillRule as SkiaFillRule, GradientInterpolation,
    GradientStop as SkiaGradientStop, Path2D, Point, RgbaLinear, Shader,
    StrokeCap, StrokeJoin,
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
    pub fn new(size: Size, scale: f32, gpu: bool) -> Result<Self, Error> {
        let pixels = pixel_size(size, scale)?;
        let options = CanvasOptions {
            gpu,
            ..CanvasOptions::default()
        };
        let canvas = Canvas::with_options(pixels.width, pixels.height, options)
            .map_err(|error| Error::Paint(error.to_string()))?;
        Ok(Self { canvas, scale, gpu })
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
    let context = surface.context();
    context.save();
    context.scale(scale, scale);
    let result = walk(context, resolved, layout, measurer, page);
    context.restore();
    result
}

/// One entry in the traversal's own stack.
///
/// Iterative rather than recursive, for the reason `Scene::validate` is: a
/// scene is caller data, and a tree deeper than the thread's stack would abort
/// the process instead of returning an error. The explicit `Leave` is what
/// keeps `save`/`restore` balanced without the call stack to unwind it.
#[derive(Debug, Clone, Copy)]
enum Step {
    Enter(NodeId),
    Leave { layers: u8 },
}

fn walk(
    context: &mut Context2D,
    resolved: &Resolved<'_>,
    layout: &LayoutResult,
    measurer: &mut SceneMeasurer<'_>,
    page: NodeId,
) -> Result<(), Error> {
    let scene = resolved.scene;
    let mut stack = vec![Step::Enter(page)];

    while let Some(step) = stack.pop() {
        match step {
            Step::Leave { layers } => {
                for _ in 0..layers {
                    context.restore();
                }
                context.restore();
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
                let layers = enter_node(context, node, rect)?;
                paint_box(context, resolved, id, node, rect)?;
                paint_kind(
                    context, resolved, layout, measurer, id, node, rect,
                )?;

                stack.push(Step::Leave { layers });
                for &child in z_ordered(scene, node).iter().rev() {
                    stack.push(Step::Enter(child));
                }
            }
        }
    }
    Ok(())
}

/// Children in the order they are drawn.
///
/// `z_index` first, document order within it. The sort is stable, which is
/// what makes "document order within a z-index" true rather than incidental.
fn z_ordered(scene: &meo_canvas_scene::Scene, node: &Node) -> Vec<NodeId> {
    let mut children = node.children.clone();
    children.sort_by_key(|child| {
        scene.get(*child).map_or(0, |child| child.paint.z_index)
    });
    children
}

/// Applies the transform and opens whatever isolation layers the node needs.
///
/// Returns how many layers were opened, so the matching `Leave` closes exactly
/// those.
fn enter_node(
    context: &mut Context2D,
    node: &Node,
    rect: Rect,
) -> Result<u8, Error> {
    apply_transform(context, node.effects.transform.as_ref(), rect);

    if node.layout.overflow.0 != Overflow::Visible
        || node.layout.overflow.1 != Overflow::Visible
    {
        clip_to_box(context, &node.paint, rect)?;
    }

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
    for shadow in &node.effects.box_shadows {
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

    if paint.background_image.is_some()
        && let Some(image) = resolved.background(id).map(DecodedImage::inner)
    {
        // Drawn to the box rather than tiled: repetition needs a pattern
        // shader, and the tiling modes are a fixture-verified concern rather
        // than an arithmetic one.
        context.draw_image_sized(
            image,
            rect.origin.x,
            rect.origin.y,
            rect.size.width,
            rect.size.height,
        );
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
    }
    let top = baseline - paragraph.alphabetic_baseline();
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
    let radii = paint.border_radius;
    let corners = [
        radii.top_left,
        radii.top_right,
        radii.bottom_right,
        radii.bottom_left,
    ];
    if corners.iter().all(|radius| *radius <= 0.0) {
        context.rect(
            rect.origin.x,
            rect.origin.y,
            rect.size.width,
            rect.size.height,
        );
        return Ok(());
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

/// Strokes the border, one edge at a time where the edges differ.
///
/// A single stroked rounded rectangle would be wrong wherever two edges have
/// different widths or colours, which CSS allows and `Sides` carries.
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

    // The uniform case is the common one and is a single stroke, centred on
    // the box edge the way a canvas stroke is.
    //
    // Bit equality rather than an epsilon: these are four values a caller set,
    // not four results of arithmetic, and "the author wrote the same number on
    // every edge" is exactly the question. Two widths a hair apart are two
    // widths, and stroking them as one would be the wrong picture.
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

    if uniform && same_colour {
        context.set_stroke_style(to_skia_color(paint.border_color_all));
        context.set_line_width(widths.top);
        box_path(context, paint, rect)?;
        context.stroke();
        return Ok(());
    }

    for (width, colour, from, to) in [
        (
            widths.top,
            edge_colors.top,
            (rect.origin.x, rect.origin.y),
            (rect.right(), rect.origin.y),
        ),
        (
            widths.right,
            edge_colors.right,
            (rect.right(), rect.origin.y),
            (rect.right(), rect.bottom()),
        ),
        (
            widths.bottom,
            edge_colors.bottom,
            (rect.right(), rect.bottom()),
            (rect.origin.x, rect.bottom()),
        ),
        (
            widths.left,
            edge_colors.left,
            (rect.origin.x, rect.bottom()),
            (rect.origin.x, rect.origin.y),
        ),
    ] {
        if width <= 0.0 {
            continue;
        }
        context.set_stroke_style(to_skia_color(
            colour.unwrap_or(paint.border_color_all),
        ));
        context.set_line_width(width);
        context.begin_path();
        context.move_to(from.0, from.1);
        context.line_to(to.0, to.1);
        context.stroke();
    }
    Ok(())
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
    // An inset shadow falls inside the box and needs the inverse of this
    // path, which is a fixture-verified concern rather than an arithmetic one.
    if shadow.inset {
        return Ok(());
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

    let center = Point::new(
        rect.origin.x + resolve_length(gradient.center.0, rect.size.width),
        rect.origin.y + resolve_length(gradient.center.1, rect.size.height),
    );

    let shader = match gradient.kind {
        GradientKind::Linear => {
            let (start, end) = gradient_line(gradient.angle_degrees, rect);
            Shader::linear_gradient(
                start,
                end,
                &stops,
                GradientInterpolation::default(),
            )
        }
        GradientKind::Radial => {
            // The radius that reaches the furthest corner, which is CSS's
            // `farthest-corner` default for a radial gradient.
            let radius = rect.size.width.hypot(rect.size.height) / 2.0;
            Shader::radial_gradient(
                center,
                radius,
                &stops,
                GradientInterpolation::default(),
            )
        }
        GradientKind::Conic => Shader::sweep_gradient(
            center,
            gradient.angle_degrees,
            gradient.angle_degrees + DEGREES_PER_TURN,
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

/// Named so the mask vocabulary is reachable from this file's documentation
/// while masking itself is unimplemented.
const _: Option<(Mask, MaskShape, ImageSource, Effects)> = None;

#[cfg(test)]
mod tests {
    use meo_canvas_scene::{
        Length, Point, Rect, Scene, Size,
        node::{ImageSource, Node, NodeId, NodeKind},
        style::paint::{BlendMode, ObjectFit, PaintStyle},
    };

    use super::{
        Surface, draw, fit_image, gradient_line, pixel_size, resolve_length,
        to_skia_blend, z_ordered,
    };
    use crate::{
        layout::LayoutResult,
        measure::SceneMeasurer,
        resolve::{
            Fonts, Resolved,
            tests::{RED_PNG, TEST_FAMILY, test_fonts},
        },
    };

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
        let off = Surface::new(Size::new(8.0, 8.0), 1.0, false)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(!off.gpu());

        let on = Surface::new(Size::new(8.0, 8.0), 1.0, true)
            .unwrap_or_else(|error| unreachable!("{error}"));
        assert!(on.gpu(), "the request is recorded even where no backend is");
        assert!(format!("{on:?}").contains("gpu"));
    }

    #[test]
    fn a_surface_begins_a_page_per_call_after_the_first() {
        let mut surface = Surface::new(Size::new(20.0, 10.0), 2.0, false)
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
                node.paint.z_index = z;
            }
            ids.push(id);
        }
        let root = scene
            .get(NodeId::ROOT)
            .unwrap_or_else(|| unreachable!("a new scene has a root"));
        let ordered = z_ordered(&scene, root);

        // -1 first, then the two zeroes in the order they were added, then 2.
        assert_eq!(ordered, vec![ids[1], ids[2], ids[3], ids[0]]);
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
        let mut surface = Surface::new(scene.size, 1.0, false)
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
        let mut surface = Surface::new(scene.size, 2.0, false)
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
        use meo_canvas_scene::{
            Corners, Sides,
            style::{
                effect::{BoxShadow, Effects, Transform},
                layout::Overflow,
                paint::{
                    BackgroundImage, BackgroundRepeat, Color, Gradient,
                    GradientKind, GradientStop,
                },
            },
        };

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

        for kind in GradientKind::ALL {
            let mut scene = Scene::new(Size::new(80.0, 60.0));
            let child = scene
                .push(NodeId::ROOT, Node::container())
                .unwrap_or_else(|error| unreachable!("{error}"));

            if let Some(node) = scene.get_mut(child) {
                node.paint.gradient = Some(Gradient {
                    kind: *kind,
                    stops: stops.clone(),
                    angle_degrees: 45.0,
                    center: (Length::Percent(0.5), Length::Percent(0.5)),
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
                    size: (None, None),
                    position: (Length::ZERO, Length::ZERO),
                });
            }

            let fonts = Fonts::new();
            let resolved = Resolved::new(&scene, &fonts)
                .unwrap_or_else(|error| unreachable!("{error}"));
            let mut measurer = SceneMeasurer::prepare(&resolved, &fonts)
                .unwrap_or_else(|error| unreachable!("{error}"));
            let mut surface = Surface::new(scene.size, 1.0, false)
                .unwrap_or_else(|error| unreachable!("{error}"));
            let solved =
                crate::layout::solve(&scene, scene.pages[0], &mut measurer)
                    .unwrap_or_else(|error| unreachable!("{error}"));
            draw(&mut surface, &resolved, &solved, &mut measurer)
                .unwrap_or_else(|error| unreachable!("{kind:?}: {error}"));
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
        let mut surface = Surface::new(scene.size, 1.0, false)
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
        let mut surface = Surface::new(scene.size, 1.0, false)
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
                paint::{Color, Gradient, GradientKind, GradientStop},
            },
        };

        let gradient = Gradient {
            kind: GradientKind::Linear,
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
            angle_degrees: 0.0,
            center: (Length::Percent(0.5), Length::Percent(0.5)),
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
        let mut surface = Surface::new(scene.size, 1.0, false)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let solved =
            crate::layout::solve(&scene, scene.pages[0], &mut measurer)
                .unwrap_or_else(|error| unreachable!("{error}"));
        draw(&mut surface, &resolved, &solved, &mut measurer)
            .unwrap_or_else(|error| unreachable!("{error}"));
    }
}
