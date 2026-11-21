//! A filtered node and everything it paints, drawn onto a canvas of its own and
//! filtered as one picture, in the node's local space at device resolution.
//! The "Filters" section of `docs/design.md` has why and how it is sized;
//! `chrome_filter_whole.rs` pins it.

use meo_canvas_scene::{
    NodeId, Point, Rect, Size,
    node::{Node, NodeKind},
    style::effect::Transform,
};
use meo_skia_canvas::{Affine, Canvas, CanvasOptions, Context2D};

use super::{
    DEGREES_PER_TURN, device_bounds, filter_spill, in_property, resolve_length,
    scale_filter_lengths,
};
use crate::{Error, layout::LayoutResult, resolve::Resolved};

/// How far past its offset a blurred shadow reaches, per unit of CSS blur
/// radius: the backend's sigma is half the radius, and three sigmas hold all
/// but a thousandth of a Gaussian, which rounds to nothing in eight bits.
const BLUR_RADIUS_REACH: f32 = 1.5;

/// How far a glyph can ink past its text node's box, in ems. A line box at
/// `line_height` 0 still holds the baseline, and a face's ascent and descent
/// each stay inside one em in every face measured here; italic overhang is a
/// fraction of that.
const GLYPH_REACH_EMS: f32 = 1.0;

/// Extra room on every side for antialiasing, which inks the pixel an edge
/// crosses.
const ANTIALIAS: f32 = 1.0;

/// How close a transform's off-diagonal terms must be to zero, and its two
/// scales to each other, for it to count as a translation and a uniform scale.
/// Far below a pixel over any page Skia can allocate, and above the noise
/// `rotate(0)` composes into a matrix.
const AXIS_ALIGNED: f32 = 1e-5;

/// A filtered node's group while it is painted, and how it is drawn back.
pub(super) struct FilterGroup {
    /// Where the node and its descendants paint. Its transform maps the
    /// node's local space onto its pixels at `density`.
    content: Canvas,
    /// The chain with its lengths in the group's pixels.
    chain: String,
    /// Where the group's top-left pixel sits in the node's local space.
    origin: Point,
    /// What a second canvas is made with, for a group filtered on its own.
    options: CanvasOptions,
    /// How far the chain reaches past the group, in the group's pixels: the
    /// margin a second canvas needs to hold a shadow cast off its edge.
    margin: f32,
    /// Pixels per local unit.
    density: f32,
    /// Whether the transform below is a translation and a uniform scale, so
    /// the group draws back pixel for pixel with the chain set.
    aligned: bool,
}

impl FilterGroup {
    /// Opens a group under `context`'s transform for a subtree inking inside
    /// `ink` (local space; `None` is unbounded), cut to what can reach the
    /// `visible` pixels below. `None` where nothing can, so the node draws
    /// nothing; [`Error::Paint`] where the canvas cannot be made.
    pub(super) fn open(
        context: &Context2D,
        css: &str,
        ink: Option<Rect>,
        visible: Size,
        options: &CanvasOptions,
    ) -> Result<Option<Self>, Error> {
        let transform = context.get_transform();
        let Some(inverse) = transform.inverse() else {
            return Ok(None);
        };
        let aligned = is_aligned(transform);
        let density = if aligned {
            transform.a
        } else {
            transform
                .a
                .hypot(transform.b)
                .max(transform.c.hypot(transform.d))
        };
        if !(density > 0.0 && density.is_finite()) {
            return Ok(None);
        }

        // What reaches the visible part of the canvas below is what lies in it
        // grown by the chain's reach, in local units.
        let reach = filter_spill(css);
        let seen =
            device_bounds(inverse, Rect::new(Point::new(0.0, 0.0), visible));
        let wanted = grow(seen, reach);
        let Some(local) =
            ink.map_or(Some(wanted), |ink| intersect(ink, wanted))
        else {
            return Ok(None);
        };

        // Whole pixels in the group, and for an aligned group whole pixels of
        // the canvas below too, so drawing it back resamples nothing.
        let (origin, pixels) = if aligned {
            let device = device_bounds(transform, local);
            let left = device.origin.x.floor();
            let top = device.origin.y.floor();
            let right = (device.origin.x + device.size.width).ceil();
            let bottom = (device.origin.y + device.size.height).ceil();
            let origin = Point::new(
                (left - transform.tx) / density,
                (top - transform.ty) / density,
            );
            (origin, Size::new(right - left, bottom - top))
        } else {
            let left = (local.origin.x * density).floor();
            let top = (local.origin.y * density).floor();
            let right = ((local.origin.x + local.size.width) * density).ceil();
            let bottom =
                ((local.origin.y + local.size.height) * density).ceil();
            (
                Point::new(left / density, top / density),
                Size::new(right - left, bottom - top),
            )
        };
        if pixels.width < 1.0 || pixels.height < 1.0 {
            return Ok(None);
        }

        let mut content =
            Canvas::with_options(pixels.width, pixels.height, options.clone())
                .map_err(|error| Error::Paint(error.to_string()))?;
        let drawing = content.context();
        drawing.scale(density, density);
        drawing.translate(-origin.x, -origin.y);
        Ok(Some(Self {
            content,
            chain: scale_filter_lengths(css, density),
            origin,
            options: options.clone(),
            margin: (reach * density).ceil(),
            density,
            aligned,
        }))
    }

    /// Where the node and its descendants draw while the group is open.
    pub(super) fn context(&mut self) -> &mut Context2D {
        self.content.context()
    }

    /// The group's size in its own pixels, which is what a backdrop read inside
    /// it may cover.
    pub(super) fn pixels(&self) -> Size {
        Size::new(self.content.width(), self.content.height())
    }

    /// Filters the group as one picture and draws it onto `context`, which must
    /// still hold the transform the group was opened under. Fails where a
    /// canvas cannot be made, or on the chain's own refusal, which resolve
    /// rules out.
    pub(super) fn close(
        mut self,
        context: &mut Context2D,
    ) -> Result<(), Error> {
        context.save();
        context.translate(self.origin.x, self.origin.y);
        context.scale(1.0 / self.density, 1.0 / self.density);
        let result = if self.aligned {
            context
                .set_filter_css(&self.chain)
                .map(|()| context.draw_canvas(&mut self.content, 0.0, 0.0))
                .map_err(|error| in_property("filter", &error))
        } else {
            self.filtered().map(|mut filtered| {
                context.draw_canvas(&mut filtered, -self.margin, -self.margin);
            })
        };
        context.restore();
        result
    }

    /// The group drawn through the chain onto a canvas of the same pixels and
    /// `margin` more on every side, for a transform the chain cannot be set
    /// under.
    fn filtered(&mut self) -> Result<Canvas, Error> {
        let pixels = self.pixels();
        let mut filtered = Canvas::with_options(
            2.0f32.mul_add(self.margin, pixels.width),
            2.0f32.mul_add(self.margin, pixels.height),
            self.options.clone(),
        )
        .map_err(|error| Error::Paint(error.to_string()))?;
        let drawing = filtered.context();
        drawing
            .set_filter_css(&self.chain)
            .map_err(|error| in_property("filter", &error))?;
        drawing.draw_canvas(&mut self.content, self.margin, self.margin);
        Ok(filtered)
    }
}

/// Whether `transform` is a translation and a uniform positive scale.
fn is_aligned(transform: Affine) -> bool {
    transform.b.abs() < AXIS_ALIGNED
        && transform.c.abs() < AXIS_ALIGNED
        && transform.a > 0.0
        && (transform.a - transform.d).abs() < AXIS_ALIGNED * transform.a
}

/// `rect` grown by `by` on every side.
fn grow(rect: Rect, by: f32) -> Rect {
    Rect::new(
        Point::new(rect.origin.x - by, rect.origin.y - by),
        Size::new(
            2.0f32.mul_add(by, rect.size.width),
            2.0f32.mul_add(by, rect.size.height),
        ),
    )
}

/// The overlap of two rectangles, or `None` where they do not overlap.
fn intersect(one: Rect, other: Rect) -> Option<Rect> {
    let left = one.origin.x.max(other.origin.x);
    let top = one.origin.y.max(other.origin.y);
    let right =
        (one.origin.x + one.size.width).min(other.origin.x + other.size.width);
    let bottom = (one.origin.y + one.size.height)
        .min(other.origin.y + other.size.height);
    (right > left && bottom > top).then(|| {
        Rect::new(Point::new(left, top), Size::new(right - left, bottom - top))
    })
}

/// What the subtree under `id` can ink, in `id`'s local space: each laid-out
/// box under the transforms between, grown by what it draws past its box, and
/// `canvas` where `id` is the page root, whose background layers cover it.
/// `None` where a path, which draws where its data says, leaves no bound.
pub(super) fn subtree_ink(
    resolved: &Resolved<'_>,
    layout: &LayoutResult,
    id: NodeId,
    canvas: Option<Rect>,
) -> Option<Rect> {
    let scene = resolved.scene;
    let mut ink = canvas;
    let mut pending = vec![(id, Affine::IDENTITY)];
    while let Some((at, placed)) = pending.pop() {
        let (Some(node), Some(rect)) = (scene.get(at), layout.get(at)) else {
            continue;
        };
        if matches!(node.kind, NodeKind::Path { .. }) {
            return None;
        }
        let drawn = device_bounds(
            placed,
            grow(rect, reach(resolved, at, node, at == id)),
        );
        ink = Some(ink.map_or(drawn, |ink| union(ink, drawn)));
        for &child in &node.children {
            let Some(child_rect) = layout.get(child) else {
                continue;
            };
            let below = scene
                .get(child)
                .and_then(|child| child.effects.transform.as_ref())
                .map_or(placed, |transform| {
                    placed.multiply(&transform_matrix(transform, child_rect))
                });
            pending.push((child, below));
        }
    }
    ink
}

/// How far `node` draws past its box: its outer box shadows, its glyphs and
/// their shadows and outline, and a descendant's own filter.
fn reach(resolved: &Resolved<'_>, id: NodeId, node: &Node, root: bool) -> f32 {
    let shadow = |offset_x: f32, offset_y: f32, blur: f32, spread: f32| {
        blur.max(0.0).mul_add(
            BLUR_RADIUS_REACH,
            offset_x.abs().max(offset_y.abs()) + spread.max(0.0),
        )
    };
    let boxes = node
        .effects
        .box_shadows
        .iter()
        .filter(|shadow| !shadow.inset)
        .map(|s| shadow(s.offset_x, s.offset_y, s.blur, s.spread))
        .fold(0.0, f32::max);
    let glyphs = resolved.text(id).map_or(0.0, |text| {
        let outline = text.text_stroke.map_or(0.0, |stroke| stroke.width);
        let shadows = node
            .effects
            .text_shadows
            .iter()
            .map(|s| shadow(s.offset_x, s.offset_y, s.blur, 0.0))
            .fold(0.0, f32::max);
        text.size.mul_add(GLYPH_REACH_EMS, outline + shadows)
    });
    // The root's own chain is the group's; a descendant's grows what it adds.
    let filtered = if root {
        0.0
    } else {
        resolved
            .dropped()
            .filter(id, node)
            .map_or(0.0, filter_spill)
    };
    boxes.max(glyphs).max(filtered) + ANTIALIAS
}

/// The matrix `apply_transform` multiplies onto a context for `transform` on a
/// box at `rect`, in the same order.
fn transform_matrix(transform: &Transform, rect: Rect) -> Affine {
    let origin_x =
        rect.origin.x + resolve_length(transform.origin.0, rect.size.width);
    let origin_y =
        rect.origin.y + resolve_length(transform.origin.1, rect.size.height);
    let turn =
        transform.rotate_degrees / DEGREES_PER_TURN * core::f32::consts::TAU;
    Affine::translation(
        resolve_length(transform.translate_x, rect.size.width),
        resolve_length(transform.translate_y, rect.size.height),
    )
    .multiply(&Affine::translation(origin_x, origin_y))
    .multiply(&Affine::rotation_radians(turn))
    .multiply(&Affine::scale(transform.scale_x, transform.scale_y))
    .multiply(&Affine::translation(-origin_x, -origin_y))
}

/// The smallest rectangle holding both.
fn union(one: Rect, other: Rect) -> Rect {
    let left = one.origin.x.min(other.origin.x);
    let top = one.origin.y.min(other.origin.y);
    let right =
        (one.origin.x + one.size.width).max(other.origin.x + other.size.width);
    let bottom = (one.origin.y + one.size.height)
        .max(other.origin.y + other.size.height);
    Rect::new(Point::new(left, top), Size::new(right - left, bottom - top))
}
