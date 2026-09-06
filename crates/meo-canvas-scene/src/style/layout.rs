//! What the solver reads: the flexbox, grid and block properties.
//!
//! The field set is the part of `canvas.type.ts`'s `BoxProps` that taffy
//! answers to, so translation in `meo-canvas-core` is a move rather than an
//! interpretation. Anything CSS defines that taffy does not solve is absent
//! rather than accepted and ignored.

use crate::{
    geometry::Sides,
    style::{Dimension, Length},
    wire::wire_enum,
};

wire_enum! {
    /// How a node's children are arranged.
    pub enum Display {
        /// Children participate in flex layout.
        Flex = 0,
        /// Children are placed on a grid.
        Grid = 1,
        /// Children stack as blocks.
        Block = 2,
        /// The node and its subtree are neither laid out nor drawn.
        ///
        /// Distinct from an opacity of zero, which still occupies space.
        None = 3,
    }
}

wire_enum! {
    /// The axis children are placed along, and which end they start from.
    pub enum FlexDirection {
        /// Left to right, or right to left under [`Direction::Rtl`].
        Row = 0,
        /// The reverse of [`FlexDirection::Row`].
        RowReverse = 1,
        /// Top to bottom.
        Column = 2,
        /// Bottom to top.
        ColumnReverse = 3,
    }
}

wire_enum! {
    /// Whether children overflow onto further lines.
    pub enum FlexWrap {
        /// One line, children shrink to fit.
        NoWrap = 0,
        /// Further lines are added below, or after, the first.
        Wrap = 1,
        /// Further lines are added before the first.
        WrapReverse = 2,
    }
}

wire_enum! {
    /// Distribution of free space along the main axis.
    pub enum Justify {
        /// Packed at the start.
        FlexStart = 0,
        /// Packed at the end.
        FlexEnd = 1,
        /// Packed at the centre.
        Center = 2,
        /// Free space divided between the children.
        SpaceBetween = 3,
        /// Free space divided around each child, so edge gaps are half the
        /// inner ones.
        SpaceAround = 4,
        /// Free space divided evenly, so every gap including the edges is
        /// equal.
        SpaceEvenly = 5,
    }
}

wire_enum! {
    /// Placement along the cross axis.
    ///
    /// One enum carrying the *union* of what `align-items`, `align-self` and
    /// `align-content` accept, not their intersection. CSS does not give the
    /// three the same set -- `align-items` has no `space-*` value at all, and
    /// `align-content` has all four -- so each variant below names the
    /// properties that cannot use it. One enum rather than three because the
    /// overlap is most of the list and a caller reading a scene should not have
    /// to learn which of three near-identical types a field takes.
    ///
    /// A property handed a value it has no meaning for is not an error here.
    /// The solver resolves it the way CSS does, which for `align-items` given a
    /// `space-*` value is to behave as [`Align::FlexStart`].
    pub enum Align {
        /// At the start of the cross axis.
        FlexStart = 0,
        /// At the end.
        FlexEnd = 1,
        /// At the centre.
        Center = 2,
        /// Filling the cross axis.
        Stretch = 3,
        /// Aligned so the children's first baselines coincide.
        ///
        /// A measurer reports a baseline -- `meo-canvas-core`'s
        /// `MeasuredLeaf::first_baseline` carries one -- but taffy's
        /// high-level tree has nowhere to receive it: `compute_leaf_layout`
        /// returns `first_baselines: Point::NONE`
        /// (`taffy-0.13.0/src/compute/leaf.rs:102`) for every node sized by a
        /// measure function, and only the low-level `LayoutPartialTree` API
        /// lets a caller build the `LayoutOutput` that would carry it.
        ///
        /// What a measured leaf therefore aligns on is its bottom edge, not its
        /// text baseline: taffy reads a missing baseline as
        /// `baseline.unwrap_or(height)`
        /// (`taffy-0.13.0/src/compute/flexbox.rs:1524`). In a column direction
        /// it is treated as [`Align::FlexStart`] instead, because taffy
        /// supports baseline alignment only across a row.
        Baseline = 4,
        /// Free space divided between the lines.
        ///
        /// `align-content` only; `align-items` and `align-self` align one item
        /// and have no free space to divide.
        SpaceBetween = 5,
        /// Free space divided around each line, so the edge gaps are half the
        /// inner ones.
        ///
        /// `align-content` only.
        SpaceAround = 6,
        /// Free space divided evenly, so every gap including the edges is
        /// equal.
        ///
        /// `align-content` only.
        SpaceEvenly = 7,
    }
}

wire_enum! {
    /// Whether a node is placed by the flow or by its own offsets.
    ///
    /// CSS's positioning schemes. `static` and `relative` differ in two observable
    /// ways -- whether `inset` moves the node, and whether `z_index` gives it a
    /// place in its parent's stack -- and both were measured in Chrome rather
    /// than read off the specification; see [`LayoutStyle::inset`] and
    /// `stacks_by_z_index` in `meo-canvas-core`.
    #[derive(Default)]
    pub enum PositionType {
        /// Placed by the flow, with `inset` shifting it from where it landed.
        Relative = 0,
        /// Taken out of the flow and placed by `inset` against its container.
        Absolute = 1,
        /// Placed by the flow, and `inset` does not apply.
        ///
        /// CSS's initial value, so this is the variant a node that says nothing
        /// about its position gets.
        ///
        /// Appended rather than given the zero it would have if the enum were
        /// written today:
        /// the discriminants are a published part of both wire formats, and
        /// renumbering `Relative` and `Absolute` to put `static` first would
        /// change what every already-encoded byte means to buy an ordering
        /// nothing reads.
        #[default]
        Static = 2,
        /// Placed by `inset` against the page rather than against an ancestor.
        ///
        /// CSS resolves `fixed` against the viewport, and a still render's
        /// viewport is its page — there is nothing to scroll relative to, so
        /// the page is the whole of it.
        Fixed = 3,
        /// Placed by the flow, and **identical to [`Relative`] here**.
        ///
        /// [`Relative`]: PositionType::Relative
        ///
        /// Not a gap: CSS's `sticky` is defined against a scroll position, and
        /// a still image has none. Chrome draws a sticky element exactly where
        /// it draws a relative one at scroll offset zero, which is the only
        /// offset a page ever has here. Carried as its own variant so a scene
        /// ported from a document keeps saying what it meant, and so a future
        /// paged or scrolling surface has the word already on the wire.
        Sticky = 4,
    }
}

wire_enum! {
    /// What happens to content larger than its box.
    pub enum Overflow {
        /// Drawn beyond the box.
        Visible = 0,
        /// Clipped to the box.
        Hidden = 1,
        /// Clipped, and the box reserves room for a scrollbar.
        ///
        /// Nothing scrolls in a still image. The variant exists because the
        /// reserved gutter changes layout, which a caller porting a web design
        /// depends on.
        Scroll = 2,
    }
}

wire_enum! {
    /// Whether `width` and `height` include padding and border.
    pub enum BoxSizing {
        /// They do. This is the default here, as it is in Yoga.
        BorderBox = 0,
        /// They do not. This is CSS's initial value.
        ContentBox = 1,
    }
}

wire_enum! {
    /// Inline direction, which decides which edge is the start.
    pub enum Direction {
        /// Left to right.
        Ltr = 0,
        /// Right to left.
        Rtl = 1,
    }
}

wire_enum! {
    /// The order the grid's auto-placement algorithm fills tracks in.
    pub enum GridAutoFlow {
        /// Fill a row, then move to the next.
        Row = 0,
        /// Fill a column, then move to the next.
        Column = 1,
        /// As [`GridAutoFlow::Row`], but backfill earlier holes.
        RowDense = 2,
        /// As [`GridAutoFlow::Column`], but backfill earlier holes.
        ColumnDense = 3,
    }
}

/// One track of a grid template.
///
/// `canvas.type.ts` spells this as ``number | 'auto' | `${number}px` |
/// `${number}fr` | `${number}%` ``, and a bare number there means pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[non_exhaustive]
pub enum TrackSize {
    /// Sized to its content.
    #[default]
    Auto,
    /// A fixed count of logical pixels.
    Points(f32),
    /// A fraction of the grid's extent, where `1.0` is 100%.
    Percent(f32),
    /// A share of the free space remaining after the fixed tracks are placed.
    Fraction(f32),
}

/// Where a grid item sits, as a track span.
///
/// Numeric lines rather than the `'1 / 3'` strings `canvas.type.ts` accepts:
/// the string is a surface convenience, and parsing it belongs to whichever
/// surface offered it rather than to a crate every surface links.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GridPlacement {
    /// The line the item starts at, counting from one. `None` means the
    /// auto-placement algorithm chooses.
    pub start: Option<i16>,
    /// How many tracks the item covers. `None` means one.
    pub span: Option<u16>,
}

/// Everything the layout pass reads off a node.
///
/// Two of the three interesting defaults are CSS's rather than Yoga's:
/// [`FlexDirection::Row`] and a `flex_shrink` of `1.0`, where Yoga's raw
/// defaults are a column direction and a shrink of `0`, so a bare box would
/// change meaning between the two. The TypeScript `Column` and `Row` factories
/// already override the shrink to `1`, which makes CSS's value the one their
/// own trees use; following CSS makes the bare case agree with the named ones
/// instead of inheriting an exception.
///
/// The third, [`Display::Flex`], is **Yoga's default and not CSS's** -- CSS's
/// initial `display` is `inline`, which this crate has no node for. It is kept
/// because a scene is a tree of boxes rather than a run of inline content, so
/// there is nothing for `inline` to mean here.
///
/// What that costs a reader is worth stating, because the shape of the tree
/// depends on it: a container with no `display` lays its children out as flex
/// items, so a text child shrink-wraps to its content where a web author's
/// `div` would fill the line. Measured, a text node under a default parent
/// draws the same ink at every `textAlign`, and only under [`Display::Block`]
/// does alignment move it. Reach for `Block` to get the familiar behaviour.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutStyle {
    /// Layout mode for this node's children.
    pub display: Display,
    /// Whether the node is placed by the flow or by `inset`.
    pub position_type: PositionType,
    /// Offsets from the container's edges.
    ///
    /// Read as a position for [`PositionType::Absolute`], as a shift that
    /// moves the node without moving its siblings for
    /// [`PositionType::Relative`], and **not at all** for
    /// [`PositionType::Static`] -- which is the default, so an inset on a
    /// node that was never positioned does nothing.
    ///
    /// Measured in Chrome, not assumed: a static child given
    /// `top: 30px; left: 30px` sits at its flow position in a block, a flex
    /// and a grid container alike, while the same child made relative
    /// moves by 30 on both axes and leaves its sibling exactly where it
    /// was.
    pub inset: Sides<Option<Length>>,
    /// Requested width and height.
    pub size: (Dimension, Dimension),
    /// Lower bound on the computed size.
    pub min_size: (Dimension, Dimension),
    /// Upper bound on the computed size.
    pub max_size: (Dimension, Dimension),
    /// Width divided by height, honoured when one axis is [`Dimension::Auto`].
    pub aspect_ratio: Option<f32>,
    /// Space outside the border. [`Dimension::Auto`] here is CSS's auto
    /// margin, which absorbs free space.
    pub margin: Sides<Dimension>,
    /// Space inside the border.
    pub padding: Sides<Length>,
    /// Border thickness, which occupies space whether or not it is painted.
    pub border: Sides<f32>,
    /// The axis children run along.
    pub flex_direction: FlexDirection,
    /// Whether children overflow onto further lines.
    pub flex_wrap: FlexWrap,
    /// Share of free space this node absorbs.
    pub flex_grow: f32,
    /// Share of overflow this node gives up.
    pub flex_shrink: f32,
    /// Size along the main axis before growing or shrinking.
    pub flex_basis: Dimension,
    /// Main-axis distribution. `None` leaves the solver's default.
    pub justify_content: Option<Justify>,
    /// Cross-axis placement of children.
    pub align_items: Option<Align>,
    /// Cross-axis placement of this node, overriding its parent's
    /// `align_items`.
    pub align_self: Option<Align>,
    /// Cross-axis distribution of wrapped lines.
    pub align_content: Option<Align>,
    /// Space between children as `(row, column)`.
    pub gap: (Length, Length),
    /// Clipping behaviour, per axis as `(x, y)`.
    pub overflow: (Overflow, Overflow),
    /// Whether `size` includes padding and border.
    pub box_sizing: BoxSizing,
    /// Inline direction.
    pub direction: Direction,
    /// Column tracks, when [`Display::Grid`].
    pub grid_template_columns: Vec<TrackSize>,
    /// Row tracks, when [`Display::Grid`].
    pub grid_template_rows: Vec<TrackSize>,
    /// Size given to rows the template does not name.
    pub grid_auto_rows: Option<TrackSize>,
    /// Size given to columns the template does not name.
    pub grid_auto_columns: Option<TrackSize>,
    /// The order auto-placement fills tracks in.
    pub grid_auto_flow: GridAutoFlow,
    /// This node's column span within its parent grid.
    pub grid_column: GridPlacement,
    /// This node's row span within its parent grid.
    pub grid_row: GridPlacement,
}

impl Default for LayoutStyle {
    fn default() -> Self {
        Self {
            display: Display::Flex,
            position_type: PositionType::Static,
            inset: Sides::all(None),
            size: (Dimension::Auto, Dimension::Auto),
            min_size: (Dimension::Auto, Dimension::Auto),
            max_size: (Dimension::Auto, Dimension::Auto),
            aspect_ratio: None,
            margin: Sides::all(Dimension::Points(0.0)),
            padding: Sides::all(Length::ZERO),
            border: Sides::all(0.0),
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::NoWrap,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Dimension::Auto,
            justify_content: None,
            align_items: None,
            align_self: None,
            align_content: None,
            gap: (Length::ZERO, Length::ZERO),
            overflow: (Overflow::Visible, Overflow::Visible),
            box_sizing: BoxSizing::BorderBox,
            direction: Direction::Ltr,
            grid_template_columns: Vec::new(),
            grid_template_rows: Vec::new(),
            grid_auto_rows: None,
            grid_auto_columns: None,
            grid_auto_flow: GridAutoFlow::Row,
            grid_column: GridPlacement::AUTO,
            grid_row: GridPlacement::AUTO,
        }
    }
}

impl GridPlacement {
    /// Placement left entirely to the auto-placement algorithm.
    pub const AUTO: Self = Self {
        start: None,
        span: None,
    };

    /// A span of `span` tracks beginning at `start`, counting from one.
    #[must_use]
    pub const fn spanning(start: i16, span: u16) -> Self {
        Self {
            start: Some(start),
            span: Some(span),
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn every_track_size_is_named_here_so_a_new_one_cannot_be_ignored_elsewhere()
    {
        // **The compile error `#[non_exhaustive]` moved out of the other
        // crates.** `meo-canvas-core` now has a wildcard arm for this enum, so
        // a variant added here would take that arm and draw nothing rather
        // than fail to build. This match has no wildcard and lives in the
        // crate that owns the type, which is where the attribute leaves
        // exhaustiveness intact: adding a variant fails to compile here, and
        // whoever adds it goes and looks at the arms that need it.
        //
        // `cargo test` rather than `cargo build`, which is the cost of putting
        // it in a test; the gate runs both.
        use super::TrackSize;
        let witness = |value: &TrackSize| match value {
            TrackSize::Auto => "auto",
            TrackSize::Points { .. } => "points",
            TrackSize::Percent { .. } => "percent",
            TrackSize::Fraction { .. } => "fraction",
        };
        let _ = witness(&TrackSize::Auto);
    }
    use super::{
        Align, BoxSizing, Direction, Display, FlexDirection, FlexWrap,
        GridAutoFlow, GridPlacement, Justify, LayoutStyle, Overflow,
        PositionType, TrackSize,
    };
    use crate::style::{Dimension, Length};

    #[test]
    fn defaults_follow_css_not_yoga() {
        let style = LayoutStyle::default();
        assert_eq!(style.display, Display::Flex);
        assert_eq!(style.flex_direction, FlexDirection::Row);
        assert!((style.flex_shrink - 1.0).abs() < f32::EPSILON);
        assert!((style.flex_grow - 0.0).abs() < f32::EPSILON);
        assert_eq!(style.box_sizing, BoxSizing::BorderBox);
        assert_eq!(style.direction, Direction::Ltr);
        assert_eq!(style.flex_basis, Dimension::Auto);
        assert_eq!(style.gap, (Length::ZERO, Length::ZERO));
        assert_eq!(style.overflow, (Overflow::Visible, Overflow::Visible));
        // CSS's initial value, which is `static` and not `relative`: an
        // `inset` on a node nobody positioned has to do nothing, and a
        // `z_index` on one in a block container has to name nothing.
        assert_eq!(style.position_type, PositionType::Static);
        assert_eq!(PositionType::default(), PositionType::Static);
        assert_eq!(style.grid_auto_flow, GridAutoFlow::Row);
        assert_eq!(style.grid_column, GridPlacement::AUTO);
        assert!(style.grid_template_rows.is_empty());
    }

    #[test]
    fn grid_placement_spanning_is_start_plus_span() {
        let placement = GridPlacement::spanning(2, 3);
        assert_eq!(placement.start, Some(2));
        assert_eq!(placement.span, Some(3));
        assert_eq!(GridPlacement::default(), GridPlacement::AUTO);
    }

    #[test]
    fn track_size_defaults_to_auto() {
        assert_eq!(TrackSize::default(), TrackSize::Auto);
    }

    #[test]
    fn every_layout_enum_lists_its_variants() {
        assert_eq!(Display::ALL.len(), 4);
        assert_eq!(FlexDirection::ALL.len(), 4);
        assert_eq!(FlexWrap::ALL.len(), 3);
        assert_eq!(Justify::ALL.len(), 6);
        assert_eq!(Align::ALL.len(), 8);
        assert_eq!(PositionType::ALL.len(), 5);
        assert_eq!(Overflow::ALL.len(), 3);
        assert_eq!(BoxSizing::ALL.len(), 2);
        assert_eq!(Direction::ALL.len(), 2);
        assert_eq!(GridAutoFlow::ALL.len(), 4);
    }
}
