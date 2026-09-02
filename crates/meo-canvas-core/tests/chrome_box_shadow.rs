//! An outer box-shadow does not reach inside the box that casts it.
//!
//! # The rule, and why it needs a translucent background to be visible
//!
//! CSS Backgrounds and Borders 3 §7.1.1: an outer shadow is drawn outside the
//! border edge **only** — the border box is clipped out of it. So an element's
//! own background never composites over its own shadow.
//!
//! Painting the shadow beneath the box and letting the background cover it
//! looks identical wherever that background is opaque, which is every fixture
//! this project had. Under a translucent one the two coats show: half-alpha
//! black over half-alpha black is `1 - (1-0.5)^2 = 0.75`, and the box takes a
//! second dose of the shadow's colour.
//!
//! # What each case here can and cannot say
//!
//! The **invariant** is the discriminating claim, and it needs no agreement
//! about the ground: the same scene twice, once with the shadow and once
//! without, read well inside the border box, must give the same bytes. That is
//! true whatever the background's colour or alpha, so it survives a change of
//! scene.
//!
//! The **opaque pair** is the control that proves the instrument is pointed at
//! something. It also has to be unchanged — and it *was* unchanged before the
//! fix, which is exactly why it pins nothing on its own. A pair where both
//! rows already agree is measuring nothing; the translucent row carries the
//! claim.
//!
//! The `below` probe is the other control, guarding the direction nobody
//! watches: a renderer that satisfied every interior probe by **not drawing
//! the shadow at all** would pass all of them. Only a point outside the box
//! can tell a clipped shadow from an absent one.
//!
//! # Why Chrome's bytes are read for the interior and not for the ink
//!
//! Inside the box no blur reaches, so the colour is a compositing result and
//! two engines agree on it to the byte. In the ink they do not and never will
//! — that is a blur kernel, not a formula — so what crosses from
//! `box-shadow.tsv` for `below` is the **direction and rough depth** of
//! the darkening rather than its exact value. The same split the rest of this
//! suite makes.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Scene, Sides, Size,
    node::{Node, NodeId, NodeKind},
    style::{Dimension, effect::BoxShadow, paint::Color},
};

/// The cell, and the box inside it.
const CELL: (f32, f32) = (80.0, 80.0);
const INSET: f32 = 20.0;
const BOX: (f32, f32) = (40.0, 40.0);

/// The opaque page under everything, `#b01020`.
const PAGE: (u8, u8, u8) = (0xb0, 0x10, 0x20);

/// The two backgrounds the table measures, matched to its own rows.
///
/// The opaque one is the colour half-alpha black composites to over the page,
/// so the two backgrounds are the same picture wherever nothing is wrong.
const fn translucent() -> Color {
    Color::rgba(0, 0, 0, 0x80)
}

const fn opaque() -> Color {
    Color::rgb(108, 15, 19)
}

/// The shadow the table casts, `0 1px 2px rgba(0, 0, 0, 0.5)`.
const fn shadow(inset: bool) -> BoxShadow {
    BoxShadow {
        inset,
        offset_x: 0.0,
        offset_y: 1.0,
        blur: 2.0,
        spread: 0.0,
        color: Color::rgba(0, 0, 0, 0x80),
    }
}

/// One row of `box-shadow.tsv`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Row {
    /// `translucent` or `opaque`.
    background: &'static str,
    /// `none`, `outer` or `inset`.
    shadow: &'static str,
    /// `inside`, `inside top` or `below`.
    point: &'static str,
    /// Where it was read, in cell pixels.
    at: (usize, usize),
    /// What Chrome painted there.
    ink: (u8, u8, u8),
}

/// Chrome's table, parsed rather than transcribed.
///
/// A transcription reads identically and is not the same thing: it can drift
/// from the file in silence once the table is re-measured, which is a failure
/// this suite has already had once.
fn chrome() -> Vec<Row> {
    const TABLE: &str =
        include_str!("../../meo-canvas/tests/assets/chrome/box-shadow.tsv");
    let intern = |field: &str| -> &'static str {
        match field {
            "translucent" => "translucent",
            "opaque" => "opaque",
            "none" => "none",
            "outer" => "outer",
            "inset" => "inset",
            "inside" => "inside",
            "inside top" => "inside top",
            "below" => "below",
            other => unreachable!("{other:?} is not a field this table has"),
        }
    };
    TABLE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            assert_eq!(fields.len(), 8, "malformed row: {line:?}");
            let coordinate = |index: usize| -> usize {
                fields[index].parse().unwrap_or_else(|_| {
                    unreachable!("{:?} is not a coordinate", fields[index])
                })
            };
            let channel = |index: usize| -> u8 {
                fields[index].parse().unwrap_or_else(|_| {
                    unreachable!("{:?} is not a channel", fields[index])
                })
            };
            Row {
                background: intern(fields[0]),
                shadow: intern(fields[1]),
                point: intern(fields[2]),
                at: (coordinate(3), coordinate(4)),
                ink: (channel(5), channel(6), channel(7)),
            }
        })
        .collect()
}

/// Renders the cell and returns its pixels.
///
/// Built as a [`Scene`] rather than through the builder crate: this is the
/// renderer's own input, and both of the library's public surfaces reach the
/// painter through it. A test written on one surface would leave the other
/// asserting nothing.
fn render(background: Color, shadow: Option<BoxShadow>) -> Vec<u8> {
    let mut scene = Scene::new(Size::new(CELL.0, CELL.1));
    if let Some(root) = scene.get_mut(NodeId::ROOT) {
        root.paint.background_color = Color::rgb(PAGE.0, PAGE.1, PAGE.2);
    }

    let id = scene
        .push(NodeId::ROOT, Node::new(NodeKind::Box))
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(node) = scene.get_mut(id) {
        node.layout.size = (Dimension::Points(BOX.0), Dimension::Points(BOX.1));
        // A margin rather than a padding on the root, so the box's own edges
        // are the only ones in the picture.
        node.layout.margin = Sides {
            top: Dimension::Points(INSET),
            right: Dimension::Points(INSET),
            bottom: Dimension::Points(INSET),
            left: Dimension::Points(INSET),
        };
        node.paint.background_color = background;
        node.effects.box_shadows = shadow.into_iter().collect();
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
const fn at(bytes: &[u8], (x, y): (usize, usize)) -> (u8, u8, u8) {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the cell is a whole number of pixels, written above"
    )]
    let index = (y * (CELL.0 as usize) + x) * 4;
    (bytes[index], bytes[index + 1], bytes[index + 2])
}

/// The two points the table reads inside the border box.
fn interior(rows: &[Row]) -> Vec<(usize, usize)> {
    let mut points: Vec<(usize, usize)> = rows
        .iter()
        .filter(|row| row.point != "below")
        .map(|row| row.at)
        .collect();
    points.sort_unstable();
    points.dedup();
    assert_eq!(points.len(), 2, "the table's interior probes moved");
    points
}

/// The point outside it.
fn exterior(rows: &[Row]) -> (usize, usize) {
    let mut points: Vec<(usize, usize)> = rows
        .iter()
        .filter(|row| row.point == "below")
        .map(|row| row.at)
        .collect();
    points.sort_unstable();
    points.dedup();
    assert_eq!(points.len(), 1, "the table's exterior probe moved");
    points[0]
}

/// Chrome's byte at one cell of the table.
fn cell(
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

/// The claim: an outer shadow leaves the interior of its own box alone.
///
/// Read on a **translucent** background, which is the only one that can tell
/// the two implementations apart, and on an opaque one beside it as the
/// control that says the harness is pointed at something real.
#[test]
fn an_outer_shadow_does_not_reach_inside_the_box() {
    let rows = chrome();
    let points = interior(&rows);
    let mut wrong = Vec::new();

    for (name, background) in
        [("translucent", translucent()), ("opaque", opaque())]
    {
        let plain = render(background, None);
        let cast = render(background, Some(shadow(false)));
        for point in &points {
            let (bare, shadowed) = (at(&plain, *point), at(&cast, *point));
            if bare != shadowed {
                wrong.push(format!(
                    "{name} at {point:?}: {bare:?} without the shadow, \
                     {shadowed:?} with it -- an outer shadow is clipped out \
                     of the border box and cannot change what is inside it"
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

    for (name, background) in
        [("translucent", translucent()), ("opaque", opaque())]
    {
        for (kind, cast) in [
            ("none", render(background, None)),
            ("outer", render(background, Some(shadow(false)))),
        ] {
            for point in &points {
                let point_name = rows
                    .iter()
                    .find(|row| row.at == *point && row.point != "below")
                    .map_or_else(
                        || unreachable!("{point:?} is unnamed"),
                        |row| row.point,
                    );
                let want = cell(&rows, name, kind, point_name);
                let got = at(&cast, *point);
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
/// **darker** than without it, by roughly what Chrome darkens by — roughly,
/// because this is a blur kernel rather than a formula and two engines do not
/// agree on one to the byte.
#[test]
fn the_shadow_is_still_drawn_outside_the_box() {
    let rows = chrome();
    let point = exterior(&rows);

    let bare = at(&render(translucent(), None), point);
    let cast = at(&render(translucent(), Some(shadow(false))), point);
    assert!(
        cast.0 < bare.0,
        "with the shadow the point below the box reads {cast:?}, without it \
         {bare:?} -- the shadow is not being drawn at all"
    );

    let chrome_bare = cell(&rows, "translucent", "none", "below");
    let chrome_cast = cell(&rows, "translucent", "outer", "below");
    let theirs = i32::from(chrome_bare.0) - i32::from(chrome_cast.0);
    let ours = i32::from(bare.0) - i32::from(cast.0);
    assert!(
        (ours - theirs).abs() <= 8,
        "Chrome darkens the point below the box by {theirs} in red and we \
         darken it by {ours}; the two rasterisers differ, but not by this much"
    );
}

/// The other half of the fix: the inset path did not move.
///
/// Inset shadows are drawn **after** the background deliberately, which is the
/// opposite of the outer ones and is what makes them inset. Chrome darkens the
/// point just inside the top edge and leaves the centre alone; so must we.
#[test]
fn an_inset_shadow_still_lands_inside_the_box() {
    let rows = chrome();
    let bare = render(translucent(), None);
    let cast = render(translucent(), Some(shadow(true)));

    let probe = |name: &str| {
        rows.iter().find(|row| row.point == name).map_or_else(
            || unreachable!("the table has no `{name}` probe"),
            |row| row.at,
        )
    };
    let top = probe("inside top");
    let centre = probe("inside");

    assert!(
        at(&cast, top).0 < at(&bare, top).0,
        "an inset shadow reads {:?} just inside the top edge against {:?} \
         without it; it is being covered by the background again",
        at(&cast, top),
        at(&bare, top)
    );
    assert_eq!(
        at(&cast, centre),
        at(&bare, centre),
        "a 2px blur reached the centre of a 40px box"
    );
}
