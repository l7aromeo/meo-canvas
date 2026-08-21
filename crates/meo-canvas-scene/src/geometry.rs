//! Points, sizes, rectangles and the four-sided quantities layout works in.
//!
//! These duplicate types Skia and taffy both already have, and the duplication
//! is what keeps the crate dependency-free. Conversion in either direction is a
//! field-for-field move that `meo-canvas-core` writes once; carrying a
//! backend's geometry through the scene would carry the backend with it.
//!
//! Every value is `f32`, matching what Skia and taffy compute in, so the scene
//! introduces no rounding step of its own.
//!
//! [`Sides`] and [`Corners`] are generic over what they hold because margin,
//! padding, border and inset are the same shape carrying four different value
//! types -- `Sides<Dimension>` for margin, which may be `auto`, and
//! `Sides<f32>` for border, which may not. One type with four instantiations
//! keeps the field order identical everywhere, which is what the wire format
//! depends on.

/// A position, with `y` growing downward.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    /// Horizontal offset from the origin.
    pub x: f32,
    /// Vertical offset from the origin.
    pub y: f32,
}

/// A width and a height.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    /// Extent along the x axis.
    pub width: f32,
    /// Extent along the y axis.
    pub height: f32,
}

/// An axis-aligned rectangle, stored as an origin and an extent.
///
/// Origin-plus-extent rather than the four edges Skia uses: layout produces a
/// position and a size, and storing edges would mean recomputing one of them on
/// every read.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    /// Top-left corner.
    pub origin: Point,
    /// Extent from the origin.
    pub size: Size,
}

/// A value per edge, in CSS's `top right bottom left` order.
///
/// The order is CSS's rather than alphabetical or clockwise-from-left because
/// the wire format writes the fields in declaration order, and a reader
/// checking the format against the CSS shorthand it mirrors should find them in
/// the same sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Sides<T> {
    /// Top edge.
    pub top: T,
    /// Right edge.
    pub right: T,
    /// Bottom edge.
    pub bottom: T,
    /// Left edge.
    pub left: T,
}

/// A value per corner, in CSS's `top-left top-right bottom-right bottom-left`
/// order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Corners<T> {
    /// Top-left corner.
    pub top_left: T,
    /// Top-right corner.
    pub top_right: T,
    /// Bottom-right corner.
    pub bottom_right: T,
    /// Bottom-left corner.
    pub bottom_left: T,
}

impl Point {
    /// The origin.
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    /// Creates a point.
    #[must_use]
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

impl Size {
    /// A size with no extent.
    pub const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };

    /// Creates a size.
    #[must_use]
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

impl Rect {
    /// Creates a rectangle from an origin and an extent.
    #[must_use]
    pub const fn new(origin: Point, size: Size) -> Self {
        Self { origin, size }
    }

    /// The x coordinate of the right edge.
    #[must_use]
    pub fn right(&self) -> f32 {
        self.origin.x + self.size.width
    }

    /// The y coordinate of the bottom edge.
    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.origin.y + self.size.height
    }
}

impl<T: Copy> Sides<T> {
    /// The same value on all four edges, which is the CSS one-value shorthand.
    #[must_use]
    pub const fn all(value: T) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    /// One value for the horizontal edges and another for the vertical, which
    /// is the CSS two-value shorthand.
    #[must_use]
    pub const fn symmetric(vertical: T, horizontal: T) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }
}

impl<T: Copy> Corners<T> {
    /// The same value on all four corners.
    #[must_use]
    pub const fn all(value: T) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_right: value,
            bottom_left: value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Corners, Point, Rect, Sides, Size};

    #[test]
    fn rect_edges_are_origin_plus_extent() {
        let rect = Rect::new(Point::new(10.0, 20.0), Size::new(30.0, 40.0));
        assert!((rect.right() - 40.0).abs() < f32::EPSILON);
        assert!((rect.bottom() - 60.0).abs() < f32::EPSILON);
    }

    #[test]
    fn zero_constants_are_zero() {
        assert_eq!(Point::ZERO, Point::new(0.0, 0.0));
        assert_eq!(Size::ZERO, Size::new(0.0, 0.0));
        assert_eq!(Rect::default(), Rect::new(Point::ZERO, Size::ZERO));
    }

    #[test]
    fn sides_shorthands_match_css() {
        assert_eq!(
            Sides::all(4_u8),
            Sides {
                top: 4,
                right: 4,
                bottom: 4,
                left: 4
            }
        );
        assert_eq!(
            Sides::symmetric(1_u8, 2),
            Sides {
                top: 1,
                right: 2,
                bottom: 1,
                left: 2
            }
        );
        assert_eq!(Sides::<u8>::default(), Sides::all(0));
    }

    #[test]
    fn corners_shorthand_sets_every_corner() {
        assert_eq!(
            Corners::all(9_u8),
            Corners {
                top_left: 9,
                top_right: 9,
                bottom_right: 9,
                bottom_left: 9
            }
        );
        assert_eq!(Corners::<u8>::default(), Corners::all(0));
    }
    /// The derived traits are part of the surface -- a caller stores these in a
    /// `HashMap`, clones them and prints them in a test failure -- so they are
    /// exercised rather than assumed.
    #[test]
    fn the_derived_traits_work_on_every_shape() {
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher},
        };

        let sides = Sides::all(1_u8);
        let corners = Corners::all(2_u8);
        let rect = Rect::new(Point::new(1.0, 2.0), Size::new(3.0, 4.0));

        for rendered in [
            format!("{sides:?}"),
            format!("{corners:?}"),
            format!("{rect:?}"),
            format!("{:?}", rect.origin),
            format!("{:?}", rect.size),
        ] {
            assert!(!rendered.is_empty());
        }

        assert_eq!(sides.clone(), sides);
        assert_eq!(corners.clone(), corners);

        let mut hasher = DefaultHasher::new();
        sides.hash(&mut hasher);
        corners.hash(&mut hasher);
        assert_ne!(hasher.finish(), 0);
    }
}
