//! Where an outer box-shadow's ink may fall. None may fall inside the box (CSS
//! Backgrounds 3 §7.1.1), checked over a translucent ground where a shadow
//! drawn beneath shows through; outside, `shadow-extent.tsv` holds Chrome's
//! reach, as spans, since two blurs never match to the byte.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Corners, Scene, Sides, Size,
    node::{Node, NodeId, NodeKind},
    style::{Dimension, effect::BoxShadow, layout::Overflow, paint::Color},
};

/// One scene: a page, a box inset from every edge, and the shadows it casts.
struct Cell {
    /// The page's size, which is also the buffer's stride in pixels.
    size: (f32, f32),
    /// How far the box sits from every edge.
    inset: f32,
    /// The box's own size.
    box_size: (f32, f32),
    /// What the page is filled with.
    page: Color,
    /// What the box is filled with.
    fill: Color,
    /// The box's corner radius, the same on all four.
    radius: f32,
    /// The shadows it casts, in the order CSS writes them.
    shadows: Vec<BoxShadow>,
    /// The box's own `overflow`, which clips its content and not its shadow.
    overflow: Overflow,
}

/// Renders a cell and returns its raw RGBA bytes. Built as a [`Scene`] because
/// both public surfaces reach the painter through it.
fn render(cell: &Cell) -> Vec<u8> {
    let mut scene = Scene::new(Size::new(cell.size.0, cell.size.1));
    if let Some(root) = scene.get_mut(NodeId::ROOT) {
        root.paint.background_color = cell.page;
    }

    let id = scene
        .push(NodeId::ROOT, Node::new(NodeKind::Box))
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(id) {
        node.layout.size = (
            Dimension::Points(cell.box_size.0),
            Dimension::Points(cell.box_size.1),
        );
        // A margin rather than a padding on the root, so the box's own edges
        // are the only ones in the picture.
        node.layout.margin = Sides {
            top: Dimension::Points(cell.inset),
            right: Dimension::Points(cell.inset),
            bottom: Dimension::Points(cell.inset),
            left: Dimension::Points(cell.inset),
        };
        node.paint.background_color = cell.fill;
        node.paint.border_radius = Corners {
            top_left: cell.radius,
            top_right: cell.radius,
            bottom_right: cell.radius,
            bottom_left: cell.radius,
        };
        node.effects.box_shadows.clone_from(&cell.shadows);
        node.layout.overflow = (cell.overflow, cell.overflow);
    }

    let mut renderer = Renderer::new();
    // Off for the reason every other pixel-reading test turns it off: two
    // rasterisers do not agree to the byte, and this reads exact colours.
    renderer.set_gpu(false);
    renderer
        .render_to_buffer(&scene, ImageFormat::Raw, &EncodeOptions::default())
        .unwrap_or_else(|error| {
            unreachable!("the scene did not render: {error}")
        })
}

/// The colour at a point of a raw RGBA buffer.
const fn at(bytes: &[u8], stride: f32, (x, y): (usize, usize)) -> (u8, u8, u8) {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "every cell here is a whole number of pixels"
    )]
    let index = (y * (stride as usize) + x) * 4;
    (bytes[index], bytes[index + 1], bytes[index + 2])
}

// ---------------------------------------------------------------------------
// `box-shadow.tsv`: colours at named points, inside the box and beside it.
// ---------------------------------------------------------------------------

/// The cell that table was measured in: `#b01020` behind a 40x40 box at 20,20.
const CLIP_CELL: (f32, f32) = (80.0, 80.0);
const CLIP_INSET: f32 = 20.0;
const CLIP_BOX: (f32, f32) = (40.0, 40.0);
const CLIP_PAGE: Color = Color::rgb(0xb0, 0x10, 0x20);

/// The two backgrounds it measures.
///
/// The opaque one is the colour half-alpha black composites to over the page,
/// so the two are the same picture wherever nothing is wrong.
const TRANSLUCENT: Color = Color::rgba(0, 0, 0, 0x80);
const OPAQUE: Color = Color::rgb(108, 15, 19);

/// The shadow that table casts, `0 1px 2px rgba(0, 0, 0, 0.5)`.
const fn clip_shadow(inset: bool) -> BoxShadow {
    BoxShadow {
        inset,
        offset_x: 0.0,
        offset_y: 1.0,
        blur: 2.0,
        spread: 0.0,
        color: Color::rgba(0, 0, 0, 0x80),
    }
}

/// A hard shadow offset ten to the right, which is what the order cases cast.
const fn hard(inset: bool, color: Color) -> BoxShadow {
    BoxShadow {
        inset,
        offset_x: 10.0,
        offset_y: 0.0,
        blur: 0.0,
        spread: 0.0,
        color,
    }
}

/// The colours the order cases use, matched to the table's own.
const RED: Color = Color::rgb(220, 40, 40);
const BLUE: Color = Color::rgb(40, 60, 220);

/// The cell `box-shadow.tsv` was measured in, carrying `shadows`.
const fn clip_cell(fill: Color, shadows: Vec<BoxShadow>) -> Cell {
    Cell {
        size: CLIP_CELL,
        inset: CLIP_INSET,
        box_size: CLIP_BOX,
        page: CLIP_PAGE,
        fill,
        radius: 0.0,
        shadows,
        overflow: Overflow::Visible,
    }
}

/// The same cell, clipping itself.
const fn clipping_cell(
    fill: Color,
    shadows: Vec<BoxShadow>,
    overflow: Overflow,
) -> Cell {
    Cell {
        size: CLIP_CELL,
        inset: CLIP_INSET,
        box_size: CLIP_BOX,
        page: CLIP_PAGE,
        fill,
        radius: 0.0,
        shadows,
        overflow,
    }
}

/// One row of `box-shadow.tsv`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    /// `translucent` or `opaque`.
    background: String,
    /// The case: `none`, `outer`, `inset`, or one of the order pairs.
    shadow: String,
    /// The named probe.
    point: String,
    /// Where it was read, in cell pixels.
    at: (usize, usize),
    /// What Chrome painted there.
    ink: (u8, u8, u8),
}

/// Chrome's colour table, parsed rather than transcribed so a re-measure cannot
/// leave a stale copy here.
fn chrome() -> Vec<Row> {
    const TABLE: &str =
        include_str!("../../meo-canvas/tests/assets/chrome/box-shadow.tsv");
    TABLE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            assert_eq!(fields.len(), 8, "malformed row: {line:?}");
            let number = |index: usize| -> u32 {
                fields[index].parse().unwrap_or_else(|_| {
                    unreachable!("{:?} is not a number", fields[index])
                })
            };
            let coordinate = |index: usize| -> usize {
                fields[index].parse().unwrap_or_else(|_| {
                    unreachable!("{:?} is not a coordinate", fields[index])
                })
            };
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a channel is written as 0..=255 and parsed as one"
            )]
            let channel = |index: usize| number(index) as u8;
            Row {
                background: fields[0].to_owned(),
                shadow: fields[1].to_owned(),
                point: fields[2].to_owned(),
                at: (coordinate(3), coordinate(4)),
                ink: (channel(5), channel(6), channel(7)),
            }
        })
        .collect()
}

/// The points that table reads **inside** the border box.
///
/// Derived from the table rather than written here, so a probe added to the
/// walker reaches the assertions without a second edit.
fn interior(rows: &[Row]) -> Vec<(String, (usize, usize))> {
    let mut points: Vec<(String, (usize, usize))> = rows
        .iter()
        .filter(|row| row.point.starts_with("inside"))
        .map(|row| (row.point.clone(), row.at))
        .collect();
    points.sort_unstable();
    points.dedup();
    assert!(!points.is_empty(), "the table has no interior probes");
    points
}

/// Chrome's byte at one cell of that table.
fn cell_ink(
    rows: &[Row],
    background: &str,
    shadow: &str,
    point: &str,
) -> (u8, u8, u8) {
    rows.iter()
        .find(|row| {
            row.background == background
                && row.shadow == shadow
                && row.point == point
        })
        .unwrap_or_else(|| {
            unreachable!("the table has no {background}/{shadow}/{point} row")
        })
        .ink
}

/// Where one named probe sits.
fn probe(rows: &[Row], point: &str) -> (usize, usize) {
    rows.iter()
        .find(|row| row.point == point)
        .unwrap_or_else(|| unreachable!("the table has no `{point}` probe"))
        .at
}

/// The claim: an outer shadow leaves the interior of its own box alone.
#[test]
fn an_outer_shadow_does_not_reach_inside_the_box() {
    let rows = chrome();
    let points = interior(&rows);
    let mut wrong = Vec::new();

    for (name, background) in [("translucent", TRANSLUCENT), ("opaque", OPAQUE)]
    {
        let plain = render(&clip_cell(background, Vec::new()));
        let cast = render(&clip_cell(background, vec![clip_shadow(false)]));

        // The shadow must exist before its absence inside means anything: two
        // blank renders agree everywhere. A shadow offset one down and
        // blurred by two must land on this point just outside the
        // border edge.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the cell is a whole number of pixels, like every other \
                      coordinate here"
        )]
        let below = (
            CLIP_INSET as usize + (CLIP_BOX.0 as usize) / 2,
            CLIP_INSET as usize + CLIP_BOX.1 as usize + 1,
        );
        assert_ne!(
            at(&plain, CLIP_CELL.0, below),
            at(&cast, CLIP_CELL.0, below),
            "{name}: the two renders agree just outside the box as well, so \
             nothing was cast and the interior points below are comparing two \
             identical pictures"
        );

        for (point_name, point) in &points {
            let bare = at(&plain, CLIP_CELL.0, *point);
            let shadowed = at(&cast, CLIP_CELL.0, *point);
            if bare != shadowed {
                wrong.push(format!(
                    "{name} at {point_name} {point:?}: {bare:?} without the \
                     shadow, {shadowed:?} with it -- an outer shadow is \
                     clipped out of the border box and cannot change what is \
                     inside it"
                ));
            }
        }
    }

    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The same interiors against Chrome's bytes. No blur reaches inside, so the
/// two engines agree exactly; the invariant alone would pass a background that
/// was composited wrongly but consistently.
#[test]
fn the_interior_is_the_colour_chrome_paints() {
    let rows = chrome();
    let points = interior(&rows);
    let mut wrong = Vec::new();

    for (name, background) in [("translucent", TRANSLUCENT), ("opaque", OPAQUE)]
    {
        for (kind, shadows) in
            [("none", Vec::new()), ("outer", vec![clip_shadow(false)])]
        {
            let cast = render(&clip_cell(background, shadows));
            for (point_name, point) in &points {
                let want = cell_ink(&rows, name, kind, point_name);
                let got = at(&cast, CLIP_CELL.0, *point);
                if got != want {
                    wrong.push(format!(
                        "{name}/{kind} at {point_name} {point:?}: \
                         Chrome {want:?}, ours {got:?}"
                    ));
                }
            }
        }
    }

    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The control: the shadow is still drawn. Outside the box the ink must be
/// darker with it than without, by roughly Chrome's amount -- roughly, since
/// this is a blur kernel.
#[test]
fn the_shadow_is_still_drawn_outside_the_box() {
    let rows = chrome();
    let point = probe(&rows, "below");

    let bare = at(
        &render(&clip_cell(TRANSLUCENT, Vec::new())),
        CLIP_CELL.0,
        point,
    );
    let cast = at(
        &render(&clip_cell(TRANSLUCENT, vec![clip_shadow(false)])),
        CLIP_CELL.0,
        point,
    );
    assert!(
        cast.0 < bare.0,
        "with the shadow the point below the box reads {cast:?}, without it \
         {bare:?} -- the shadow is not being drawn at all"
    );

    let chrome_bare = cell_ink(&rows, "translucent", "none", "below");
    let chrome_cast = cell_ink(&rows, "translucent", "outer", "below");
    let theirs = i32::from(chrome_bare.0) - i32::from(chrome_cast.0);
    let ours = i32::from(bare.0) - i32::from(cast.0);
    assert!(
        (ours - theirs).abs() <= 8,
        "Chrome darkens the point below the box by {theirs} in red and we \
         darken it by {ours}; the two rasterisers differ, but not by this much"
    );
}

/// Inset shadows are drawn after the background: Chrome darkens just inside the
/// top edge and leaves the centre alone.
#[test]
fn an_inset_shadow_still_lands_inside_the_box() {
    let rows = chrome();
    let bare = render(&clip_cell(TRANSLUCENT, Vec::new()));
    let cast = render(&clip_cell(TRANSLUCENT, vec![clip_shadow(true)]));
    let top = probe(&rows, "inside top");
    let centre = probe(&rows, "inside");

    assert!(
        at(&cast, CLIP_CELL.0, top).0 < at(&bare, CLIP_CELL.0, top).0,
        "an inset shadow reads {:?} just inside the top edge against {:?} \
         without it; it is being covered by the background again",
        at(&cast, CLIP_CELL.0, top),
        at(&bare, CLIP_CELL.0, top)
    );
    assert_eq!(
        at(&cast, CLIP_CELL.0, centre),
        at(&bare, CLIP_CELL.0, centre),
        "a 2px blur reached the centre of a 40px box"
    );
}

/// CSS Backgrounds 3 §7.1: the first shadow written is on top, the opposite of
/// drawing them in sequence. `beside` reads the outer pair and `inside left`
/// the inset pair, whose ink lands on the edge opposite its offset.
#[test]
fn the_first_shadow_written_is_the_one_on_top() {
    let rows = chrome();
    let mut wrong = Vec::new();

    for (inset, point_name) in [(false, "beside"), (true, "inside left")] {
        let point = probe(&rows, point_name);
        for (case, first, second) in
            [("red then blue", RED, BLUE), ("blue then red", BLUE, RED)]
        {
            let name = if inset {
                format!("inset {case}")
            } else {
                case.to_owned()
            };
            let bytes = render(&clip_cell(
                OPAQUE,
                vec![hard(inset, first), hard(inset, second)],
            ));
            let got = at(&bytes, CLIP_CELL.0, point);
            let want = cell_ink(&rows, "opaque", &name, point_name);
            if got != want {
                wrong.push(format!(
                    "{name} at {point_name} {point:?}: Chrome {want:?}, ours \
                     {got:?} -- the first shadow written has to end up on top"
                ));
            }
        }
    }

    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

// ---------------------------------------------------------------------------
// `shadow-extent.tsv`: how far the ink reaches, and in which directions.
// ---------------------------------------------------------------------------

/// The cell that table was measured in: a white 50x50 box on a white page.
///
/// The box is invisible, so every pixel that is not white is shadow ink and an
/// extent can be read by scanning rather than by knowing where the box was.
const EXTENT_CELL: (f32, f32) = (160.0, 160.0);
const EXTENT_INSET: f32 = 55.0;
const EXTENT_BOX: (f32, f32) = (50.0, 50.0);
const WHITE: Color = Color::rgb(0xff, 0xff, 0xff);

/// Ink is anything at least this far off white, matching the walker's own.
const THRESHOLD: u8 = 6;

/// Two steps, the smallest all agreeing rows fit inside: Gaussians with one
/// sigma still cross a threshold at different pixels. The cost is that a 2px
/// spread defect fails no row (3px fails eight of nine), so one that small
/// wants a case built for it rather than a tighter number.
const TOLERANCE: i32 = 2;

/// One row of `shadow-extent.tsv`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Span {
    /// The case's name.
    case: String,
    /// Which ray was scanned.
    ray: String,
    /// The furthest step outside the border edge that still carried ink, or
    /// `-1` where the first step out was already clear.
    steps: i32,
}

/// Chrome's extent table, parsed rather than transcribed.
fn chrome_extents() -> Vec<Span> {
    const TABLE: &str =
        include_str!("../../meo-canvas/tests/assets/chrome/shadow-extent.tsv");
    TABLE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            assert_eq!(fields.len(), 3, "malformed row: {line:?}");
            Span {
                case: fields[0].to_owned(),
                ray: fields[1].to_owned(),
                steps: fields[2].parse().unwrap_or_else(|_| {
                    unreachable!("{:?} is not a step count", fields[2])
                }),
            }
        })
        .collect()
}

/// A shadow written the way the walker writes it: offset, blur, spread.
const fn ink(offset: (f32, f32), blur: f32, spread: f32) -> BoxShadow {
    BoxShadow {
        inset: false,
        offset_x: offset.0,
        offset_y: offset.1,
        blur,
        spread,
        color: Color::rgb(0, 0, 0),
    }
}

/// Every case in that table, in its order, with the radius each carries.
fn extent_cases() -> Vec<(&'static str, f32, Vec<BoxShadow>)> {
    vec![
        ("none", 0.0, Vec::new()),
        // Offset, not 0,0: unoffset, the shadow sits behind its box and reads
        // what `none` reads. Equal axes here and unequal in `offset`,
        // the row that catches swapped axes.
        ("hard", 0.0, vec![ink((4.0, 4.0), 0.0, 0.0)]),
        ("offset", 0.0, vec![ink((8.0, 4.0), 0.0, 0.0)]),
        ("blur", 0.0, vec![ink((0.0, 0.0), 12.0, 0.0)]),
        ("spread", 0.0, vec![ink((0.0, 0.0), 0.0, 6.0)]),
        ("blur-spread", 0.0, vec![ink((0.0, 0.0), 8.0, 4.0)]),
        ("radius-spread", 16.0, vec![ink((0.0, 0.0), 0.0, 6.0)]),
        ("radius-blur", 16.0, vec![ink((0.0, 0.0), 10.0, 0.0)]),
        // Half-alpha, offset clear of the box so the band below is flat. Black
        // at half alpha over white is 128 by arithmetic; an alpha
        // applied twice reads 191.
        (
            "alpha",
            0.0,
            vec![BoxShadow {
                inset: false,
                offset_x: 0.0,
                offset_y: 20.0,
                blur: 0.0,
                spread: 0.0,
                color: Color::rgba(0, 0, 0, 128),
            }],
        ),
    ]
}

/// The rays the walker scans, as `(name, dx, dy)`.
const RAYS: [(&str, i32, i32); 6] = [
    ("left", -1, 0),
    ("right", 1, 0),
    ("up", 0, -1),
    ("down", 0, 1),
    ("corner up-left", -1, -1),
    ("corner down-right", 1, 1),
];

/// Scans one ray from the border edge and returns the furthest step still
/// inked. Extent rows start at 1, where the browser's walker starts; the rim
/// probe starts at 0, the partly covered boundary pixel where an antialiased
/// rim lives and which a scan from 1 cannot see.
fn span(bytes: &[u8], threshold: u8, from: i32, (dx, dy): (i32, i32)) -> i32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "every measurement here is a whole number of pixels"
    )]
    let (left, top, right, bottom, width, height) = (
        EXTENT_INSET as i32,
        EXTENT_INSET as i32,
        (EXTENT_INSET + EXTENT_BOX.0) as i32,
        (EXTENT_INSET + EXTENT_BOX.1) as i32,
        EXTENT_CELL.0 as i32,
        EXTENT_CELL.1 as i32,
    );
    let start_x = match dx {
        d if d < 0 => left,
        d if d > 0 => right - 1,
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the box is a whole number of pixels and even"
        )]
        _ => left + (EXTENT_BOX.0 / 2.0) as i32,
    };
    let start_y = match dy {
        d if d < 0 => top,
        d if d > 0 => bottom - 1,
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the box is a whole number of pixels and even"
        )]
        _ => top + (EXTENT_BOX.1 / 2.0) as i32,
    };

    let mut last = -1;
    for step in from..=40 {
        let across = start_x + dx * step;
        let down = start_y + dy * step;
        if across < 0 || down < 0 || across >= width || down >= height {
            break;
        }
        #[expect(
            clippy::cast_sign_loss,
            reason = "the bounds check above leaves both non-negative"
        )]
        let point = (across as usize, down as usize);
        let (red, green, blue) = at(bytes, EXTENT_CELL.0, point);
        if 255_u8.saturating_sub(red.min(green).min(blue)) >= threshold {
            last = step;
        }
    }
    last
}

/// The cell `shadow-extent.tsv` was measured in.
const fn extent_cell(radius: f32, shadows: Vec<BoxShadow>) -> Cell {
    Cell {
        size: EXTENT_CELL,
        inset: EXTENT_INSET,
        box_size: EXTENT_BOX,
        page: WHITE,
        fill: WHITE,
        radius,
        shadows,
        overflow: Overflow::Visible,
    }
}

/// The ink reaches where Chrome puts it on every ray: offset, blur, spread and
/// grown corners. `none` must read `-1` everywhere, or the scan is finding
/// something that is not a shadow.
#[test]
fn the_ink_reaches_where_chrome_puts_it() {
    let table = chrome_extents();
    let mut wrong = Vec::new();
    let mut checked = 0;

    for (case, radius, shadows) in extent_cases() {
        let bytes = render(&extent_cell(radius, shadows));
        for (ray, dx, dy) in RAYS {
            let theirs = table
                .iter()
                .find(|row| row.case == case && row.ray == ray)
                .unwrap_or_else(|| {
                    unreachable!("the table has no {case}/{ray} row")
                })
                .steps;
            let ours = span(&bytes, THRESHOLD, 1, (dx, dy));
            checked += 1;
            if case == "none" && ours != -1 {
                wrong.push(format!(
                    "none/{ray}: ours {ours}, and a scene with no shadow has \
                     to read -1 -- the scan is finding something that is not \
                     a shadow"
                ));
            } else if (ours - theirs).abs() > TOLERANCE {
                wrong.push(format!(
                    "{case}/{ray}: Chrome reaches {theirs} steps, ours \
                     {ours}"
                ));
            }
        }
    }

    assert_eq!(checked, table.len(), "a table row went unread");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Nothing falls where the shadow does not point. It is hard and offset (8, 4),
/// so every inked pixel must lie in the moved border box. The whole cell is
/// scanned because a clipped silhouette's rim follows the rounded contour
/// diagonally and misses every straight ray.
#[test]
fn nothing_falls_where_the_shadow_does_not_point() {
    const OFFSET: (f32, f32) = (8.0, 4.0);
    // One pixel of slack on every side, which is what an antialiased contour
    // costs. The residue this catches is several pixels clear of that.
    const SLACK: f32 = 1.0;

    let bytes = render(&extent_cell(16.0, vec![ink(OFFSET, 0.0, 0.0)]));

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "every measurement here is a whole number of pixels"
    )]
    let (left, top, right, bottom, width, height) = (
        (EXTENT_INSET + OFFSET.0 - SLACK) as usize,
        (EXTENT_INSET + OFFSET.1 - SLACK) as usize,
        (EXTENT_INSET + EXTENT_BOX.0 + OFFSET.0 + SLACK) as usize,
        (EXTENT_INSET + EXTENT_BOX.1 + OFFSET.1 + SLACK) as usize,
        EXTENT_CELL.0 as usize,
        EXTENT_CELL.1 as usize,
    );

    let mut worst: Option<((usize, usize), u8)> = None;
    let mut inked = 0_usize;
    for y in 0..height {
        for x in 0..width {
            if x >= left && x < right && y >= top && y < bottom {
                continue;
            }
            let (red, green, blue) = at(&bytes, EXTENT_CELL.0, (x, y));
            let depth = 255_u8.saturating_sub(red.min(green).min(blue));
            if depth == 0 {
                continue;
            }
            inked += 1;
            if worst.is_none_or(|(_, seen)| depth > seen) {
                worst = Some(((x, y), depth));
            }
        }
    }

    assert!(
        worst.is_none(),
        "{inked} pixels carry ink outside the shadow's own shape, the worst \
         {:?} deep at {:?}. The shadow is offset 8 right and 4 down with no \
         blur, so its shape is the border box moved by that and nothing may \
         fall anywhere else",
        worst.map(|(_, depth)| depth),
        worst.map(|(point, _)| point),
    );
}

/// The blur's falloff, not only its reach: a span says where a Gaussian fades
/// out, not its shape on the way. Reads the ramp straight down from the bottom
/// edge, where no corner or offset reaches, against a mask blur at sigma `blur
/// / 2`.
#[test]
fn the_blur_falls_off_the_way_chromes_does() {
    const TABLE: &str =
        include_str!("../../meo-canvas/tests/assets/chrome/shadow-profile.tsv");
    // Headroom, not slack: all 48 samples match Chrome to the byte, and a
    // rasteriser may drift a unit or two. Four still catches blurring the
    // rendered pixels rather than the shape, which misses by up to 115.
    const TOLERANCE: i32 = 4;

    let mut wanted: Vec<(String, i32, u8)> = Vec::new();
    for line in TABLE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
    {
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 6, "malformed row: {line:?}");
        let step = fields[2]
            .parse()
            .unwrap_or_else(|_| unreachable!("{:?} is not a step", fields[2]));
        let ink = fields[3].parse().unwrap_or_else(|_| {
            unreachable!("{:?} is not a channel", fields[3])
        });
        wanted.push((fields[0].to_owned(), step, ink));
    }
    assert!(!wanted.is_empty(), "the profile table is empty");

    let mut wrong = Vec::new();
    let mut checked = 0;
    for (case, radius, shadows) in extent_cases() {
        if !wanted.iter().any(|(name, _, _)| name == case) {
            continue;
        }
        let bytes = render(&extent_cell(radius, shadows));
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "every measurement here is a whole number of pixels"
        )]
        let (across, edge) = (
            (EXTENT_INSET + EXTENT_BOX.0 / 2.0) as usize,
            (EXTENT_INSET + EXTENT_BOX.1) as usize,
        );
        for (name, step, theirs) in
            wanted.iter().filter(|(name, _, _)| name == case)
        {
            #[expect(
                clippy::cast_sign_loss,
                reason = "the walker writes steps from 1 upward"
            )]
            let point = (across, edge - 1 + *step as usize);
            let ours = at(&bytes, EXTENT_CELL.0, point).0;
            checked += 1;
            if (i32::from(ours) - i32::from(*theirs)).abs() > TOLERANCE {
                wrong.push(format!(
                    "{name} at step {step}: Chrome {theirs}, ours {ours}"
                ));
            }
        }
    }

    assert_eq!(checked, wanted.len(), "a profile row went unread");
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

// ---------------------------------------------------------------------------
// Issue l7aromeo/meo-canvas#83: a node's own `overflow` against its own outer
// shadow.
// ---------------------------------------------------------------------------

/// An outer shadow survives the node's own `overflow`: it is painted outside
/// the border edge and is neither content nor a descendant (CSS Backgrounds 3
/// §7.1.1). The probe reads ink outside the box, since a style assertion sees
/// the shadow it was given; Chrome draws it under all three clipping values.
#[test]
fn an_outer_shadow_survives_the_nodes_own_overflow() {
    let rows = chrome();
    let point = probe(&rows, "below");

    let bare = at(
        &render(&clip_cell(TRANSLUCENT, Vec::new())),
        CLIP_CELL.0,
        point,
    );
    let unclipped = at(
        &render(&clip_cell(TRANSLUCENT, vec![clip_shadow(false)])),
        CLIP_CELL.0,
        point,
    );

    // This scene's `Overflow` has no `Auto`, so Chrome's `auto` row, which
    // reads as `hidden` and `scroll` do, has no input here to compare.
    for overflow in [Overflow::Hidden, Overflow::Scroll] {
        let clipped = at(
            &render(&clipping_cell(
                TRANSLUCENT,
                vec![clip_shadow(false)],
                overflow,
            )),
            CLIP_CELL.0,
            point,
        );
        assert!(
            clipped.0 < bare.0,
            "under `overflow: {overflow:?}` the point below the box reads              {clipped:?} against {bare:?} with no shadow at all -- the node's              own clip is eating its own outer shadow"
        );
        assert_eq!(
            clipped, unclipped,
            "`overflow: {overflow:?}` changed the outer shadow's ink below the              box: {clipped:?} against {unclipped:?} unclipped. The clip is not              the shadow's to obey"
        );
    }
}

/// The control: an inset shadow is still clipped by the same `overflow`.
/// Lifting both kinds above the clip is the one-line mistake, and it leaks an
/// inset shadow outside the box.
#[test]
fn an_inset_shadow_is_still_clipped_by_the_nodes_own_overflow() {
    let rows = chrome();
    let top = probe(&rows, "inside top");
    let below = probe(&rows, "below");

    let bare =
        render(&clipping_cell(TRANSLUCENT, Vec::new(), Overflow::Hidden));
    let cast = render(&clipping_cell(
        TRANSLUCENT,
        vec![clip_shadow(true)],
        Overflow::Hidden,
    ));

    assert!(
        at(&cast, CLIP_CELL.0, top).0 < at(&bare, CLIP_CELL.0, top).0,
        "an inset shadow under `overflow: hidden` reads {:?} inside the top          edge against {:?} without it; the clip has taken the inset arm too",
        at(&cast, CLIP_CELL.0, top),
        at(&bare, CLIP_CELL.0, top)
    );
    assert_eq!(
        at(&cast, CLIP_CELL.0, below),
        at(&bare, CLIP_CELL.0, below),
        "an inset shadow put ink BELOW the box under `overflow: hidden`;          moving the outer arm above the clip has dragged the inset arm with it"
    );
}
