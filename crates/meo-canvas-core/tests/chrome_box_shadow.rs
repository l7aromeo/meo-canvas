//! Where an outer box-shadow's ink may fall, and where it may not.
//!
//! # The two halves, and why they are measured differently
//!
//! **Where it may not go is an invariant.** CSS Backgrounds and Borders 3
//! §7.1.1 draws an outer shadow outside the border edge only, so the same
//! scene rendered with and without the shadow has to read the same at every
//! point inside the box. That is true whatever the background's colour or
//! alpha, needs no agreement with anything, and is asserted directly.
//!
//! **Where it does go is a number**, and a browser is the authority on it.
//! Offset, blur and spread each move the edge of the ink, and a rounded corner
//! moves it again; `shadow-extent.tsv` holds Chrome's answers and this file
//! reproduces the scenes that produced them.
//!
//! # Why the invariant needs a translucent background
//!
//! Painting the shadow beneath the box and letting the background cover it
//! looks identical wherever that background is opaque, which is every fixture
//! this project had. Under a translucent one the two coats show: half-alpha
//! black over half-alpha black is `1 - (1-0.5)^2 = 0.75`, and the box takes a
//! second dose of the shadow's colour. The opaque pair beside it is the
//! control that says the instrument is pointed at something -- it was
//! unchanged before the fix too, which is exactly why it pins nothing on its
//! own.
//!
//! # The controls, and what each one caught
//!
//! [`the_shadow_is_still_drawn_outside_the_box`] guards the direction nobody
//! watches: a renderer that satisfied every interior probe by **drawing no
//! shadow at all** would pass them. It earned its keep -- a clip built on the
//! `Context2D` rather than as two subpaths came out empty, the shadow vanished
//! outright, and every interior probe stayed green.
//!
//! [`nothing_falls_where_the_shadow_does_not_point`] is the one that a clip
//! cannot satisfy. Clipping the border box out of a shadow drawn as a property
//! of a fill removes most of the fill's silhouette and not its antialiased
//! rim, which straddles the contour; measured, that left 14 units of 255 on
//! the top-left of a card whose shadow pointed down-right. Drawing the shadow
//! as its own blurred shape leaves nothing there to remove.
//!
//! # Why the extents are spans and not bytes
//!
//! Two rasterisers do not agree on a Gaussian's bytes and do agree closely on
//! where it has faded out. Each row of `shadow-extent.tsv` is the furthest
//! whole step outside the border edge that still carries ink at a stated
//! threshold, and the comparison carries a stated tolerance. Inside the box no
//! blur reaches, so `box-shadow.tsv`'s colours are compared exactly.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Corners, Scene, Sides, Size,
    node::{Node, NodeId, NodeKind},
    style::{Dimension, effect::BoxShadow, paint::Color},
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
}

/// Renders a cell and returns its raw RGBA bytes.
///
/// Built as a [`Scene`] rather than through the builder crate: this is the
/// renderer's own input, and both of the library's public surfaces reach the
/// painter through it. A test written on one surface would leave the other
/// asserting nothing.
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

/// Chrome's colour table, parsed rather than transcribed.
///
/// A transcription reads identically and is not the same thing: it can drift
/// from the file in silence once the table is re-measured, which is a failure
/// this suite has already had once.
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

        // **The shadow has to exist before its absence inside means
        // anything.** Every point below compares the two renders and passes
        // when they agree -- which two blank pages do, so with `draw`
        // returning before it read the scene this test passed unchanged. One
        // point just outside the border edge, where a shadow offset one down
        // and blurred by two must land, is what separates "the shadow stays
        // out of the box" from "there is no shadow".
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

/// The same interiors, against Chrome's own bytes.
///
/// Inside the box no blur reaches, so this is a compositing result two engines
/// agree on exactly. The invariant above would be satisfied by a renderer that
/// composited the background wrongly and did so consistently; this is what
/// says the colour is also the right one.
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

/// The control the interior probes need: the shadow is still drawn.
///
/// A renderer that satisfied every claim above by drawing no shadow at all
/// would pass them. Outside the box, with the shadow, the ink has to be
/// **darker** than without it, by roughly what Chrome darkens by -- roughly,
/// because this is a blur kernel rather than a formula.
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

/// The other arm: inset shadows are drawn after the background, deliberately.
///
/// Chrome darkens the point just inside the top edge and leaves the centre
/// alone; so must we. This is the rule the rewrite must not drag the outer arm
/// into, and the row that says it did not.
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

/// CSS Backgrounds and Borders 3 §7.1: a shadow list is painted front to back.
///
/// The **first** shadow written is the one on top, which is the opposite of
/// what a loop drawing them in sequence produces. Two hard shadows in the same
/// place, written in both orders, on both arms: `beside` reads the outer pair
/// and `inside left` reads the inset pair, whose ink lands along the edge
/// opposite the one it is offset towards.
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

/// How far our span may differ from Chrome's before it is a defect.
///
/// Two Gaussians that agree on sigma still disagree on where their tails cross
/// a threshold, and a step is one pixel. Two is the smallest number that all
/// the agreeing rows fit inside.
///
/// **What that costs is stated rather than implied: a defect of two steps or
/// fewer is invisible here.** The comparison is `> TOLERANCE`, so a miss of
/// exactly two passes. Measured, not reasoned: a renderer given a spurious
/// **2px** spread fails no row in this table, and the same renderer at 3px
/// fails eight of the nine cases. This comment used to say that a wrong
/// offset, a wrong spread or a corner that failed to grow all miss by more
/// than the tolerance, which is the claim the 2px run refutes.
///
/// Narrowing it is a decision about how much rasteriser disagreement to allow
/// and moves every row, so a spread defect of exactly two steps wants a case
/// built to show it rather than a tighter number here.
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
        // Offset, not 0,0: with no offset the shadow sits entirely behind the
        // box that casts it and reads what `none` reads, so the row could not
        // fail for anything `none` did not already cover. The axes are equal
        // here and unequal in `offset`, which is the row that catches a
        // renderer swapping them.
        ("hard", 0.0, vec![ink((4.0, 4.0), 0.0, 0.0)]),
        ("offset", 0.0, vec![ink((8.0, 4.0), 0.0, 0.0)]),
        ("blur", 0.0, vec![ink((0.0, 0.0), 12.0, 0.0)]),
        ("spread", 0.0, vec![ink((0.0, 0.0), 0.0, 6.0)]),
        ("blur-spread", 0.0, vec![ink((0.0, 0.0), 8.0, 4.0)]),
        ("radius-spread", 16.0, vec![ink((0.0, 0.0), 0.0, 6.0)]),
        ("radius-blur", 16.0, vec![ink((0.0, 0.0), 10.0, 0.0)]),
        // Half-alpha, offset clear of the box so the band below it is flat.
        // Half-alpha black over white is 128, which is arithmetic rather than
        // a kernel: the previous implementation read 191, because riding on
        // Skia's `shadow_blur` applied the shadow's alpha twice -- once as the
        // fill it derived the shadow from, and again as the shadow's colour.
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

/// Scans one ray and returns the furthest step that still carried ink.
///
/// The same walk the browser-side walker makes, including where it starts: on
/// the border edge, at the midpoint of a side or at the corner point.
///
/// `from` is which step to begin at, and the two callers want different ones.
/// The extent rows begin at **1**, one whole pixel outside the box, because
/// that is where the walker begins and a span has to be compared against the
/// same walk. The rim probe begins at **0** -- the boundary pixel itself,
/// which the box only partly covers. That pixel is where a silhouette's
/// antialiased rim lives, so a scan starting past it cannot see the very thing
/// it exists to catch: measured, the shipped clip-only fix left 14 units of
/// 255 there and read clean from step 1 outward.
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
    }
}

/// The ink reaches where Chrome puts it, on every ray of every case.
///
/// This is where offset, blur, spread and a grown corner radius are pinned.
/// The `none` row is the instrument's own control: if it is not `-1`
/// everywhere then the scan is finding something that is not a shadow, and
/// every other row is worthless.
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

/// Nothing at all falls where the shadow does not point.
///
/// The probe a clip cannot satisfy, and the one that decided this rewrite.
///
/// The shadow is **hard** -- no blur, no spread -- and offset 8 right and 4
/// down, so its shape is exactly the border box moved by that offset and every
/// inked pixel in the whole cell has to lie inside it. Anything outside is ink
/// CSS does not put anywhere.
///
/// # Why the whole cell, and not a ray
///
/// Because the residue does not sit where a ray from a side midpoint or a
/// corner point passes. A silhouette drawn as a property of a fill and then
/// clipped away leaves its antialiased rim **along the box's own rounded
/// contour**, which on a 16px radius runs diagonally across the corner and
/// misses every straight ray. Measured against the shipped clip-only fix, the
/// residue sat at `(62, 57)` reading 219 of 255 and at `(58, 60)` reading 237,
/// both of them inside the box's bounding rectangle and outside the shadow's;
/// a scan of the four sides and two diagonals read perfectly clean.
///
/// The box is rounded for the same reason: a square one puts the rim on the
/// straight edges, where the shadow's own shape covers most of it and the
/// evidence is weakest.
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

/// The blur's falloff, not only its reach.
///
/// An extent says where a Gaussian has faded out and nothing about its shape
/// on the way there: a blur with the right reach and the wrong falloff passes
/// every span row. This reads the ramp itself, straight down from the bottom
/// edge where no corner and no offset reaches.
///
/// It exists because the blur changed hands. Drawing the shadow as its own
/// shape means blurring it ourselves -- a Gaussian mask blur at sigma
/// `blur / 2` -- where before it rode on Skia's `shadow_blur`, which halves
/// the radius the same way and then blurs the rendered pixels rather than the
/// coverage. Both are the same Gaussian on one flat colour, and this is what
/// says so rather than assuming it.
#[test]
fn the_blur_falls_off_the_way_chromes_does() {
    const TABLE: &str =
        include_str!("../../meo-canvas/tests/assets/chrome/shadow-profile.tsv");
    // Headroom, not slack. **Every one of these 48 samples agrees with Chrome
    // to the byte** as this is written, which is not something to pin at zero
    // -- a rasteriser is allowed a unit or two and a gate that forbids it
    // fails for the wrong reason. Four is small enough to catch the thing it
    // is for: the previous implementation, which rode on Skia's `shadow_blur`
    // rather than blurring the shape, missed by up to 7 through the middle of
    // the `blur` ramp and by 115 at the top of `blur-spread`, where its solid
    // silhouette covered the ramp entirely.
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
