//! Chrome's answers, one row per combination, put through this renderer: the
//! defects found here were combinational, and a row is cheaper than a fixture.
//! Chrome's side is measured in a browser, ours read from pixels; a row this
//! renderer cannot express is counted, never skipped.

use std::collections::BTreeMap;

use meo_canvas::{
    BorderStyle, Box, BoxSizing, Display, Element, Format, PositionType,
    Renderer, Root, Styled, hex_rgb, pct, px,
    scene::{Color, GridAutoFlow, GridPlacement, TrackSize, Transform},
    sides,
};

/// One row: its keys, and the text each value was written as.
type Row = BTreeMap<String, String>;

/// The page every paint-order case is drawn on.
const PAGE: (f32, f32) = (200.0, 140.0);

/// Where the parent sits on the page: not at the origin, since `fixed` resolves
/// against the page and everything else against the parent, and at 0,0 the two
/// are indistinguishable.
const PARENT_AT: (f32, f32) = (44.0, 44.0);

/// A's colour, B's colour, and the parent's.
const A_INK: Color = Color::rgb(220, 40, 40);
const B_INK: Color = Color::rgb(40, 80, 220);
const PARENT_INK: Color = Color::rgb(238, 238, 238);

/// Which box a render paints. Both are always in the tree -- one is hidden by
/// not painting it -- because B is pulled back over A by a negative margin and
/// lands elsewhere without it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Draw {
    /// A, with B transparent.
    A,
    /// B, with A transparent.
    B,
    /// Both, which is the question.
    Both,
}

/// A rendered page as raw `RGBA`, with its size.
struct Pixels {
    width: usize,
    height: usize,
    bytes: Vec<u8>,
}

impl Pixels {
    /// The colour at a point, or `None` outside the page.
    fn at(&self, x: usize, y: usize) -> Option<(u8, u8, u8)> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let at = (y * self.width + x) * 4;
        Some((self.bytes[at], self.bytes[at + 1], self.bytes[at + 2]))
    }

    /// The bounding box of an exact colour, as `(x0, y0, x1, y1)`.
    fn extent(&self, ink: Color) -> Option<(usize, usize, usize, usize)> {
        let want = (ink.r, ink.g, ink.b);
        let mut found: Option<(usize, usize, usize, usize)> = None;
        for y in 0..self.height {
            for x in 0..self.width {
                if self.at(x, y) != Some(want) {
                    continue;
                }
                found = Some(match found {
                    None => (x, y, x, y),
                    Some((x0, y0, x1, y1)) => {
                        (x0.min(x), y0.min(y), x1.max(x), y1.max(y))
                    }
                });
            }
        }
        found
    }
}

/// Renders one element tree on a page of `size`.
fn render(size: (f32, f32), child: Element) -> Pixels {
    render_on(size, child, hex_rgb(0xff_ff_ff))
}

/// The same, on a page of a stated colour.
fn render_on(size: (f32, f32), child: Element, page: Color) -> Pixels {
    let mut renderer = Renderer::new();
    // Off for the reason the fixture harness turns it off: two rasterisers do
    // not agree to the byte, and this reads exact colours.
    renderer.set_gpu(false);

    let mut canvas = Root::new(size.0)
        .height(size.1)
        .position_type(PositionType::Relative)
        .background_color(page)
        .children(child)
        .render(&renderer)
        .unwrap_or_else(|error| {
            unreachable!("the scene did not render: {error}")
        });
    let bytes = canvas.to_buffer(Format::Raw).unwrap_or_else(|error| {
        unreachable!("the canvas did not encode: {error}")
    });

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "every page here is a whole number of pixels, written above"
    )]
    Pixels {
        width: size.0 as usize,
        height: size.1 as usize,
        bytes,
    }
}

/// The `PositionType` a table's position name asks for.
fn position(name: &str) -> PositionType {
    match name {
        "static" => PositionType::Static,
        "relative" => PositionType::Relative,
        "absolute" => PositionType::Absolute,
        "fixed" => PositionType::Fixed,
        "sticky" => PositionType::Sticky,
        other => {
            unreachable!("the table names a position we do not read: {other}")
        }
    }
}

/// The `Display` a table's display name asks for, or `None` when we have no
/// variant for it.
fn display(name: &str) -> Option<Display> {
    match name {
        "block" => Some(Display::Block),
        "flex" => Some(Display::Flex),
        "grid" => Some(Display::Grid),
        _ => None,
    }
}

/// One of the two boxes, placed the way Chrome's probe placed it.
///
/// `painted` says whether this box shows its own colour or nothing at all.
fn box_of(row: &Row, is_b: bool, painted: bool) -> Element {
    let name = if is_b { "b" } else { "a" };
    let kind = position(&row[name]);
    let z = &row[if is_b { "zb" } else { "za" }];

    let ink = match (painted, is_b) {
        (false, _) => Color::rgba(0, 0, 0, 0),
        (true, true) => B_INK,
        (true, false) => A_INK,
    };
    let mut element = Box::new()
        .display(Display::Block)
        .size(px(50.0), px(34.0))
        .position_type(kind)
        .background_color(ink);

    if let Ok(value) = z.parse::<i32>() {
        element = element.z_index(value);
    }

    // `Fixed` takes an inset here though the table was measured without one.
    // Re-measuring all 281 cases with it changes no answer: of 45 `fixed` rows,
    // 34 agree and 11 do not overlap, and no other row moves. So the inset
    // stays, as the rule that drops it only for `Static` says.
    let out_of_flow =
        matches!(kind, PositionType::Absolute | PositionType::Fixed);
    if out_of_flow {
        let (left, top) = if is_b { (24.0, 14.0) } else { (0.0, 0.0) };
        return element.position(sides(
            Some(px(top)),
            None,
            None,
            Some(px(left)),
        ));
    }

    if row["display"] == "grid" {
        element = element
            .grid_row(GridPlacement::spanning(1, 1))
            .grid_column(GridPlacement::spanning(1, 1));
        if is_b {
            element =
                element.margin(sides(px(14.0), px(0.0), px(0.0), px(24.0)));
        }
        return element;
    }

    // In flow, B is pulled back over A along the parent's own axis.
    if is_b {
        element = if row["display"] == "flex" {
            element.margin(sides(px(0.0), px(0.0), px(0.0), px(-26.0)))
        } else {
            element.margin(sides(px(-20.0), px(0.0), px(0.0), px(0.0)))
        };
    }
    element
}

/// The parent, holding whichever boxes this render carries.
fn parent_of(row: &Row, draw: Draw) -> Element {
    let children = vec![
        box_of(row, false, draw != Draw::B),
        box_of(row, true, draw != Draw::A),
    ];

    // The parent's background is painted only when both boxes are: a `z_index:
    // -1` child under a parent with no stacking context paints behind that
    // background, so a solo render with it drawn cannot find the child.
    let ink = if draw == Draw::Both {
        PARENT_INK
    } else {
        Color::rgba(0, 0, 0, 0)
    };
    let mut parent = Box::new()
        .size(px(90.0), px(60.0))
        // `Relative`, as Chrome's probe had it, not `Absolute`: the two differ
        // on whether the parent establishes a stacking context, which is what
        // half these rows test. The offset comes from a wrapper.
        .position_type(PositionType::Relative)
        .background_color(ink)
        .display(display(&row["display"]).unwrap_or(Display::Block))
        .children(children);

    if let Ok(value) = row["parent_z"].parse::<i32>() {
        parent = parent.z_index(value);
    }

    // The wrapper exists only to put the parent somewhere other than the page
    // origin, so that a `fixed` box -- which resolves against the page -- and
    // an absolute one -- which resolves against the parent -- are not the same
    // thing. Chrome's probe had the same offset for the same reason.
    Box::new()
        .display(Display::Block)
        .padding(sides(px(PARENT_AT.1), px(0.0), px(0.0), px(PARENT_AT.0)))
        .children(parent)
}

/// What this renderer says is on top, or why it could not be asked.
fn top_of(row: &Row) -> Result<&'static str, String> {
    let alone_a = render(PAGE, parent_of(row, Draw::A));
    let alone_b = render(PAGE, parent_of(row, Draw::B));

    let (Some(a), Some(b)) = (alone_a.extent(A_INK), alone_b.extent(B_INK))
    else {
        return Err("one of the two boxes drew nothing at all".to_owned());
    };

    let x0 = a.0.max(b.0);
    let y0 = a.1.max(b.1);
    let x1 = a.2.min(b.2);
    let y1 = a.3.min(b.3);
    if x1 < x0 + 2 || y1 < y0 + 2 {
        return Err(format!(
            "the two boxes do not overlap here: A at {a:?}, B at {b:?}"
        ));
    }

    let both = render(PAGE, parent_of(row, Draw::Both));
    let point = (x0.midpoint(x1), y0.midpoint(y1));
    match both.at(point.0, point.1) {
        Some(seen) if seen == (A_INK.r, A_INK.g, A_INK.b) => Ok("A"),
        Some(seen) if seen == (B_INK.r, B_INK.g, B_INK.b) => Ok("B"),
        Some(seen) if seen == (PARENT_INK.r, PARENT_INK.g, PARENT_INK.b) => {
            Ok("P")
        }
        seen => Err(format!("the overlap centre is {seen:?}, which is nobody")),
    }
}

/// Whether either box in a row is `fixed`.
fn involves_fixed(row: &Row) -> bool {
    row["a"] == "fixed" || row["b"] == "fixed"
}

/// The rows this renderer answers differently from Chrome: pinned, so a row
/// that starts agreeing fails until it is deleted from here. Empty. A zero and
/// an auto `z_index` tie on positioned boxes, while a static flex or grid
/// item's zero outranks auto -- one index, two rules, split by position.
const KNOWN: &[&str] = &[];

/// How a row reads in a failure.
fn name(row: &Row) -> String {
    format!(
        "{} | {}:{} vs {}:{} | parent z {}",
        row["display"],
        row["a"],
        row["za"],
        row["b"],
        row["zb"],
        row["parent_z"]
    )
}

#[test]
fn paint_order_matches_chrome() {
    let table = read_columns(
        include_str!("assets/chrome/paint-order.tsv"),
        &[
            "section", "display", "a", "b", "za", "zb", "parent_z", "top",
        ],
    );
    let mut wrong = Vec::new();
    let mut excluded = 0_usize;
    let mut compared = 0_usize;
    let mut discriminating = 0_usize;
    let mut unreachable_geometry = 0_usize;

    for row in &table {
        if display(&row["display"]).is_none() {
            excluded += 1;
            continue;
        }
        // A row whose answer is the later child in document order is one every
        // implementation gets right by doing nothing. Counted, so a green run
        // says how much of it was earned.
        if row["top"] != "B" {
            discriminating += 1;
        }
        compared += 1;

        let known = KNOWN.contains(&name(row).as_str());
        match top_of(row) {
            Ok(ours) if (ours == row["top"]) != known => {}
            Ok(ours) if known => wrong.push(format!(
                "{}: now agrees with Chrome, drawing {ours}. That is a fix -- \
                 delete the row from KNOWN",
                name(row)
            )),
            Ok(ours) => wrong.push(format!(
                "{}: we draw {ours} on top, Chrome draws {}",
                name(row),
                row["top"]
            )),
            // A `fixed` box resolves against the page here and the viewport
            // there, so whether the pair overlaps depends on an offset we
            // cannot reproduce. Overlapping rows are compared, since stacking
            // does not depend on it; the rest are excluded and counted.
            Err(why) if involves_fixed(row) => {
                unreachable_geometry += 1;
                let _ = why;
            }
            Err(why) => wrong.push(format!("{}: {why}", name(row))),
        }
    }

    assert!(
        wrong.is_empty(),
        "{} rows changed their answer:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
    eprintln!(
        "paint order: {} rows compared, {discriminating} of them not answered \
         by document order alone; {excluded} excluded as inline-block or \
         table-cell, which this renderer has no variant for; \
         {unreachable_geometry} excluded as `fixed` cases whose boxes do not \
         overlap here; {} known disagreements pinned",
        compared - unreachable_geometry,
        KNOWN.len()
    );
}

/// The child every box-sizing case measures, and the grandchild that fills it.
fn sized(row: &Row, with_content: bool) -> Element {
    let border = row["border"].parse::<f32>().unwrap_or(0.0);
    let padding = row["padding"].parse::<f32>().unwrap_or(0.0);

    let mut child = Box::new()
        .display(Display::Block)
        .height(px(40.0))
        .box_sizing(if row["sizing"] == "border-box" {
            BoxSizing::BorderBox
        } else {
            BoxSizing::ContentBox
        })
        .border(sides(border, border, border, border))
        .border_style(BorderStyle::Solid)
        .border_color(Color::rgb(20, 20, 20))
        .padding(sides(px(padding), px(padding), px(padding), px(padding)))
        .background_color(B_INK);

    // `auto` is the absence of a width rather than a value: `Length` is points
    // or a percentage, and a node that says nothing about its width is sized by
    // its parent -- which is what CSS's `auto` means here.
    child = match row["width"].as_str() {
        "auto" => child,
        "50%" => child.width(pct(50.0)),
        _ => child.width(px(100.0)),
    };

    if with_content {
        // A grandchild filling the content box, so its painted span **is** the
        // content width. Drawn in a third colour so it is told from the
        // padding around it, which shares the child's background.
        child = child.children(
            Box::new()
                .display(Display::Block)
                .size(pct(100.0), pct(100.0))
                .background_color(Color::rgb(240, 200, 40)),
        );
    }
    child
}

/// The host the child sits in: 200 wide, and the display the row names.
fn host(row: &Row, with_content: bool) -> Element {
    Box::new()
        .width(px(200.0))
        .position_type(PositionType::Relative)
        .display(display(&row["parent"]).unwrap_or(Display::Block))
        .children(sized(row, with_content))
}

/// The width of an exact colour on the page, in whole pixels.
fn span(page: &Pixels, ink: Color) -> f32 {
    match page.extent(ink) {
        None => 0.0,
        #[expect(
            clippy::cast_precision_loss,
            reason = "a span of a 300-pixel page is exact in an f32"
        )]
        Some((x0, _, x1, _)) => (x1 - x0 + 1) as f32,
    }
}

/// The outer width and the content width this renderer gives a row.
fn measure(row: &Row) -> (f32, f32) {
    let plain = render((300.0, 100.0), host(row, false));
    let border = row["border"].parse::<f32>().unwrap_or(0.0);
    // The outer box is the background plus the border painted around it, so a
    // bordered child is measured across both inks rather than one.
    let outer = if border > 0.0 {
        span(&plain, Color::rgb(20, 20, 20))
            .max(2.0_f32.mul_add(border, span(&plain, B_INK)))
    } else {
        span(&plain, B_INK)
    };

    let filled = render((300.0, 100.0), host(row, true));
    (outer, span(&filled, Color::rgb(240, 200, 40)))
}

#[test]
fn box_sizing_matches_chrome() {
    let table = read_columns(
        include_str!("assets/chrome/box-sizing.tsv"),
        &[
            "parent", "sizing", "width", "border", "padding", "outer",
            "content",
        ],
    );
    let mut wrong = Vec::new();
    let mut blind = 0_usize;

    for row in &table {
        let (outer, content) = measure(row);
        let want_outer = row["outer"].parse::<f32>().unwrap_or(f32::NAN);
        let want_content = row["content"].parse::<f32>().unwrap_or(f32::NAN);

        // A row whose outer box is the same under both sizings cannot tell the
        // two apart: `width: auto` is sized by the parent either way, and a
        // border and padding of zero leave nothing for the property to move.
        if row["width"] == "auto"
            || (row["border"] == "0" && row["padding"] == "0")
        {
            blind += 1;
        }

        if (outer - want_outer).abs() > 1.0
            || (content - want_content).abs() > 1.0
        {
            wrong.push(format!(
                "{} | {} | width {} | border {} | padding {}: we measure \
                 outer {outer} content {content}, Chrome {want_outer} and {want_content}",
                row["parent"], row["sizing"], row["width"], row["border"], row["padding"]
            ));
        }
    }

    assert!(
        wrong.is_empty(),
        "{} of {} rows disagree with Chrome:\n{}",
        wrong.len(),
        table.len(),
        wrong.join("\n")
    );
    eprintln!(
        "box sizing: {} rows compared, {} of them cannot tell content-box from \
         border-box at all -- `width: auto` is sized by the parent under either, \
         and a zero border with zero padding leaves the property nothing to move",
        table.len(),
        blind
    );
}

/// A tab-separated table, read through a header this asserts rather than skips,
/// so columns emitted in another order panic here instead of being read as
/// other fields. It cannot see fields swapped under an unchanged header. Panics
/// on a missing or different header, or a row of the wrong width.
fn read_columns(text: &str, want: &[&str]) -> Vec<Row> {
    let mut names: Option<Vec<&str>> = None;
    let mut rows = Vec::new();

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix('#') {
            // The column line is the last comment before the data, and is the
            // only one carrying tabs -- prose above it never does.
            if rest.contains('\t') {
                names = Some(rest.trim().split('\t').map(str::trim).collect());
            }
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }

        let Some(header) = names.as_ref() else {
            unreachable!(
                "a row before the commented line naming the columns: a \
                 malformed table is a broken checkout rather than a case to \
                 skip"
            )
        };
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(
            fields.len(),
            header.len(),
            "a row carries {} fields where the header names {}: {line}",
            fields.len(),
            header.len()
        );
        rows.push(
            header
                .iter()
                .zip(fields)
                .map(|(name, field)| {
                    ((*name).to_owned(), field.trim().to_owned())
                })
                .collect(),
        );
    }

    let Some(names) = names else {
        unreachable!(
            "no commented line naming the columns: a table with no header"
        )
    };
    assert_eq!(
        names, want,
        "the table names columns this reader does not expect; a positional \
         format read against the wrong names measures the wrong fields"
    );
    rows
}

/// The overflow rows answered differently from Chrome: empty, all 240 agree.
/// `outer` is an in-flow box behind a padded wrapper and found by its colour,
/// since an absolutely placed `outer` establishes a formatting context no
/// margin can collapse out of.
const KNOWN_OVERFLOW: &[&str] = &[];

/// The page the overflow cases are drawn on, with `outer` at Chrome's 40,40, so
/// a `fixed` box resolving against the viewport there and the page here lands
/// in the same place.
const OVERFLOW_PAGE: (f32, f32) = (280.0, 200.0);

/// Where `outer` sits on that page.
const OUTER_AT: (f32, f32) = (40.0, 40.0);

/// The three points Chrome's `elementFromPoint` was asked at, relative to
/// `outer`: inside, past the right edge, past the bottom edge.
const PROBES: [(f32, f32); 3] = [(60.0, 45.0), (88.0, 50.0), (60.0, 68.0)];

/// The clipper's grey and the child's red.
const CLIPPER_INK: Color = Color::rgb(238, 238, 238);
const CHILD_INK: Color = Color::rgb(220, 40, 40);

/// `outer`'s white, and the page behind it: `outer` is found by its colour,
/// since an escaping margin -- the behaviour under test -- moves it.
const OUTER_INK: Color = Color::rgb(255, 255, 255);
const PAGE_INK: Color = Color::rgb(247, 247, 251);

/// One row of the overflow table.
struct Overflow<'a> {
    /// The five axis letters, `OPCTI`.
    code: &'a str,
    /// The child's box, minus `outer`'s: `x, y, w, h`.
    rect: [f32; 4],
    /// What each probe found, as `c`, `l`, `o` or `b`.
    probes: &'a str,
}

/// The overflow a row's first letter names.
fn overflow_of(code: &str) -> meo_canvas::Overflow {
    match &code[0..1] {
        "v" => meo_canvas::Overflow::Visible,
        "h" => meo_canvas::Overflow::Hidden,
        _ => meo_canvas::Overflow::Scroll,
    }
}

/// The position a row's letter names.
fn position_letter(letter: &str) -> PositionType {
    match letter {
        "S" => PositionType::Static,
        "R" => PositionType::Relative,
        "A" => PositionType::Absolute,
        "K" => PositionType::Sticky,
        _ => PositionType::Fixed,
    }
}

/// The offsets a row's fifth letter names, as `(top, left)`, written whatever
/// the position type: that a `static` child ignores them is what the letter
/// measures, so withholding them would assume the answer.
fn offsets_of(code: &str) -> Option<(f32, f32)> {
    match &code[4..5] {
        "i" => Some((6.0, 8.0)),
        "n" => Some((-6.0, -8.0)),
        _ => None,
    }
}

/// The clipper and its child, from a row's five letters. `clip` is false for
/// the render that measures where the child is: Chrome reports a layout
/// rectangle, which a clip does not move.
fn clipper_of(
    row: &Overflow<'_>,
    clip: bool,
    paint_clipper: bool,
    paint_child: bool,
) -> Element {
    let out_of_flow =
        |kind| matches!(kind, PositionType::Absolute | PositionType::Fixed);

    let child_kind = position_letter(&row.code[2..3]);
    let mut child = Box::new()
        .display(Display::Block)
        .size(px(50.0), px(40.0))
        .position_type(child_kind)
        .background_color(if paint_child {
            CHILD_INK
        } else {
            Color::rgba(0, 0, 0, 0)
        });
    child = if out_of_flow(child_kind) {
        // Already placed by insets, so the table generates no offset rows for
        // it: an offset here would restate a scene the table has.
        child.position(sides(Some(px(20.0)), None, None, Some(px(30.0))))
    } else {
        let placed = child.margin(sides(px(20.0), px(0.0), px(0.0), px(30.0)));
        match offsets_of(row.code) {
            Some((top, left)) => placed.position(sides(
                Some(px(top)),
                None,
                None,
                Some(px(left)),
            )),
            None => placed,
        }
    };

    let clipper_kind = position_letter(&row.code[1..2]);
    let mut clipper = Box::new()
        .size(px(60.0), px(40.0))
        .position_type(clipper_kind)
        .display(Display::Block)
        .overflow(if clip {
            overflow_of(row.code)
        } else {
            meo_canvas::Overflow::Visible
        })
        .background_color(if paint_clipper {
            CLIPPER_INK
        } else {
            Color::rgba(0, 0, 0, 0)
        })
        .children(child);
    clipper = if out_of_flow(clipper_kind) {
        clipper.position(sides(Some(px(20.0)), None, None, Some(px(20.0))))
    } else {
        clipper.margin(sides(px(20.0), px(0.0), px(0.0), px(20.0)))
    };
    if &row.code[3..4] == "t" {
        // The identity, which is what `translateZ(0)` is in two dimensions:
        // the point is that a transform is *present*, since that is what makes
        // a box the containing block for a fixed descendant.
        clipper = clipper.transform(Transform::default());
    }

    // `outer` is in flow and relative, as Chrome's was, offset by padding on a
    // wrapper: an `outer` placed by insets establishes a formatting context, so
    // no margin escapes it and every in-flow row comes out twenty pixels low.
    let outer = Box::new()
        .size(px(200.0), px(120.0))
        .position_type(PositionType::Relative)
        .display(Display::Block)
        .background_color(OUTER_INK)
        .children(clipper);

    Box::new()
        .display(Display::Block)
        .padding(sides(px(OUTER_AT.1), px(0.0), px(0.0), px(OUTER_AT.0)))
        .children(outer)
}

/// The child's rectangle minus `outer`'s, as drawn, with the clip honoured:
/// turning `overflow` off to see the whole box also removes the formatting
/// context it establishes.
fn rect_of(row: &Overflow<'_>) -> Option<[f32; 4]> {
    let page =
        render_on(OVERFLOW_PAGE, clipper_of(row, true, true, true), PAGE_INK);
    let (ox, oy, _, _) = page.extent(OUTER_INK)?;

    #[expect(
        clippy::cast_precision_loss,
        reason = "a coordinate on a 280-pixel page is exact in an f32"
    )]
    let against = |found: (usize, usize, usize, usize)| {
        [
            found.0 as f32 - ox as f32,
            found.1 as f32 - oy as f32,
            (found.2 - found.0 + 1) as f32,
            (found.3 - found.1 + 1) as f32,
        ]
    };

    page.extent(CHILD_INK).map(against)
}

/// What the three probes find, as Chrome's letters.
fn probes_of(row: &Overflow<'_>) -> Option<String> {
    let page =
        render_on(OVERFLOW_PAGE, clipper_of(row, true, true, true), PAGE_INK);
    let (ox, oy, _, _) = page.extent(OUTER_INK)?;
    #[expect(
        clippy::cast_precision_loss,
        reason = "a coordinate on a 280-pixel page is exact in an f32"
    )]
    let origin = (ox as f32, oy as f32);
    Some(
        PROBES
            .iter()
            .map(|(x, y)| {
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "every probe is a whole number of pixels, written above"
            )]
            let seen =
                page.at((x + origin.0) as usize, (y + origin.1) as usize);
            match seen {
                Some(ink) if ink == (CHILD_INK.r, CHILD_INK.g, CHILD_INK.b) => 'c',
                Some(ink) if ink == (CLIPPER_INK.r, CLIPPER_INK.g, CLIPPER_INK.b) => 'l',
                Some(ink) if ink == (PAGE_INK.r, PAGE_INK.g, PAGE_INK.b) => 'b',
                _ => 'o',
            }
            })
            .collect(),
    )
}

/// The rows of the overflow table, which is a `.tsv` rather than JSON: 120
/// rows of four fields read better as columns, and the file is read by eye as
/// often as by this.
fn overflow_rows(text: &str) -> Vec<Overflow<'_>> {
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.split_whitespace();
            let code = fields.next().unwrap_or_default();
            let rect: Vec<f32> = fields
                .next()
                .unwrap_or_default()
                .split(',')
                .filter_map(|number| number.parse().ok())
                .collect();
            Overflow {
                code,
                rect: [
                    rect.first().copied().unwrap_or_default(),
                    rect.get(1).copied().unwrap_or_default(),
                    rect.get(2).copied().unwrap_or_default(),
                    rect.get(3).copied().unwrap_or_default(),
                ],
                probes: fields.next().unwrap_or_default(),
            }
        })
        .collect()
}

/// Whether the table can tell `relative` from `static`: with no offsets they
/// are the same, so a renderer ignoring `relative` passed every row until the
/// fifth letter. This asks the property rather than the row count.
fn separates_relative_from_static(rows: &[Overflow<'_>]) -> bool {
    rows.iter().any(|row| {
        &row.code[2..3] == "S"
            && rows.iter().any(|other| {
                other.code[0..2] == row.code[0..2]
                    && &other.code[2..3] == "R"
                    && other.code[3..] == row.code[3..]
                    && (other
                        .rect
                        .iter()
                        .zip(row.rect.iter())
                        // Both sides came out of the same file as decimal
                        // text, so equality is the question and the bits are
                        // how it is asked without a tolerance that would let a
                        // genuinely equal pair look different.
                        .any(|(theirs, ours)| {
                            theirs.to_bits() != ours.to_bits()
                        })
                        || other.probes != row.probes)
            })
    })
}

/// What a row that has started agreeing with Chrome should say.
fn stale(code: &str) -> String {
    format!(
        "{code}: now agrees with Chrome. That is a fix -- delete the row from \
         KNOWN_OVERFLOW"
    )
}

#[test]
fn overflow_against_position_matches_chrome() {
    let text = include_str!("assets/chrome/overflow-position.tsv");
    let rows = overflow_rows(text);

    assert!(
        separates_relative_from_static(&rows),
        "no pair of rows differing only in the child's position separates \
         `relative` from `static`, so the table cannot fail for `relative`: \
         without an inset the two are the same box in the same place"
    );

    let mut geometry = Vec::new();
    let mut painted = Vec::new();
    let mut off_the_page = 0_usize;
    let mut compared = 0_usize;
    let mut clipped = 0_usize;
    let mut uncomparable = 0_usize;

    for row in &rows {
        // A `fixed` child is placed against the viewport there and the page
        // here, at an offset that varies per row: `vFSn` fits `outer` at y=40
        // and `vSFn` only at y=60. Excluded and counted.
        let Some(ours) = rect_of(row) else {
            // A child with no pixels on the page: either it is off it entirely,
            // or the clip left nothing of it. Chrome reports a rectangle for
            // both, which is an answer; ours is only that it is not here.
            off_the_page += 1;
            continue;
        };

        // Rectangles are compared on `visible` rows only, where the painted box
        // is the layout box. Under a clip, knowing what should survive means
        // implementing the rule under test; those rows are answered by the
        // probes.
        if &row.code[0..1] == "v" {
            let apart = ours
                .iter()
                .zip(row.rect.iter())
                .any(|(ours, theirs)| (ours - theirs).abs() > 0.5);
            let known = KNOWN_OVERFLOW.contains(&row.code);
            if apart && !known {
                geometry.push(format!(
                    "{}: we place the child at {ours:?}, Chrome at {:?}",
                    row.code, row.rect
                ));
            }
            if !apart && known {
                geometry.push(stale(row.code));
            }
            if apart {
                uncomparable += 1;
                continue;
            }
        } else {
            clipped += 1;
        }

        let Some(seen) = probes_of(row) else { continue };
        compared += 1;
        let known = KNOWN_OVERFLOW.contains(&row.code);
        if seen != row.probes && !known {
            painted.push(format!(
                "{}: our probes read {seen}, Chrome's {}",
                row.code, row.probes
            ));
        }
        // A pinned row that has started agreeing says so: a list that
        // suppresses failures but cannot report a fix only ever grows.
        if seen == row.probes && known {
            painted.push(stale(row.code));
        }
    }

    eprintln!(
        "overflow against position: {} rows, {} placed off the page, \
         {uncomparable} whose geometry differs so the probes were not compared, \
         {clipped} whose rectangle is clipped and so answered by the probes \
         alone, {} known disagreements pinned",
        rows.len(),
        off_the_page,
        KNOWN_OVERFLOW.len()
    );
    // Every row has to have been compared: each way out of the loop is a
    // `continue`, so a renderer drawing nothing would skip all 240 and pass.
    // The three counts are exact, so pinning them costs nothing.
    assert_eq!(
        compared,
        rows.len(),
        "{} of {} rows were skipped rather than compared: {off_the_page} \
         placed off the page, {uncomparable} with geometry that differs. A row \
         nobody compares cannot fail",
        rows.len() - compared,
        rows.len()
    );
    assert_eq!(
        off_the_page, 0,
        "a child that draws nothing is a defect rather than an exclusion: \
         Chrome reports a rectangle for every row"
    );
    assert!(
        geometry.is_empty() && painted.is_empty(),
        "{} rows place the child differently and {} paint it differently:\n{}\n{}",
        geometry.len(),
        painted.len(),
        geometry.join("\n"),
        painted.join("\n")
    );
}

/// The truncation rows answered differently from Chrome, keyed by the string
/// Chrome keeps: empty. Truncation fires on width as well as line count, so an
/// unbreakable word is cut mid-word, and a trailing space survives only while
/// it fits with the marker on.
const KNOWN_ELLIPSIS: &[&str] = &[];

/// The font every ellipsis case is measured in, and the file behind it.
const ELLIPSIS_FONT: (&str, &str) = (
    "Fixture",
    "../meo-canvas-core/tests/assets/fonts/Oswald-VariableFont_wght.ttf",
);

/// How wide the ink of one line is, in whole pixels, or `None`. The comparison
/// is structural -- which glyphs were drawn -- since a word boundary and a
/// character cut differ by a whole word, far beyond antialiasing.
fn ink_width(text: &str, size: f32, width: Option<f32>) -> Option<f32> {
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    renderer
        .register_font(ELLIPSIS_FONT.0, ELLIPSIS_FONT.1)
        .unwrap_or_else(|error| {
            unreachable!("the font did not register: {error}")
        });

    let mut line =
        meo_canvas::Text::rich([(text.to_owned(), meo_canvas::Style::new())])
            .font_family(ELLIPSIS_FONT.0)
            .font_size(size)
            .color(Color::rgb(0, 0, 0));
    // A width only where the case truncates. Without one the reference line
    // needs room not to wrap -- a text node left to shrink wraps into the
    // column it is given, and the ink of three stacked lines is not the ink of
    // one. That mistake made every reference here read 52 pixels wide.
    line = match width {
        Some(width) => line.width(px(width)).max_lines(1).ellipsis("…"),
        None => line.width(px(380.0)),
    };

    let mut canvas = Root::new(400.0)
        .height(80.0)
        .background_color(hex_rgb(0xff_ff_ff))
        .children(line)
        .render(&renderer)
        .unwrap_or_else(|error| {
            unreachable!("the scene did not render: {error}")
        });
    let bytes = canvas.to_buffer(Format::Raw).unwrap_or_else(|error| {
        unreachable!("the canvas did not encode: {error}")
    });

    // 128 in the red channel, stated here because a threshold is part of a
    // measurement: 240 counts an off-white background as ink and 128 does not.
    let mut left = None;
    let mut right = 0_usize;
    for y in 0..80_usize {
        for x in 0..400_usize {
            if bytes[(y * 400 + x) * 4] < 128 {
                left = Some(left.map_or(x, |found: usize| found.min(x)));
                right = right.max(x);
            }
        }
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "an ink span of a 400-pixel page is exact in an f32"
    )]
    left.map(|left| (right - left + 1) as f32)
}

#[test]
fn what_a_truncated_line_keeps_matches_chrome() {
    let text = include_str!("assets/chrome/ellipsis.tsv");
    let mut wrong = Vec::new();
    let mut compared = 0_usize;

    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        let (Some(size), Some(width)) = (
            fields.get(1).and_then(|f| f.parse::<f32>().ok()),
            fields.get(2).and_then(|f| f.parse::<f32>().ok()),
        ) else {
            continue;
        };
        let source = fields[0].trim_matches('"');
        let drawn = fields[4].trim_matches('"');

        // Chrome's answer, drawn whole, against ours drawn under the width
        // that truncates it. Equal ink means we kept the same glyphs.
        let Some(theirs) = ink_width(drawn, size, None) else {
            continue;
        };
        let Some(ours) = ink_width(source, size, Some(width)) else {
            wrong.push(format!("{drawn:?} at {width}: we drew nothing at all"));
            continue;
        };
        compared += 1;

        // Two pixels: the same glyphs shaped by two engines land within
        // rounding of each other, and one word of difference is twenty or
        // more. The tolerance is wide enough to ignore the first and far too
        // narrow to admit the second.
        let known = KNOWN_ELLIPSIS.contains(&drawn);
        let apart = (ours - theirs).abs() > 2.0;
        if apart && !known {
            wrong.push(format!(
                "at {size}px in {width}: Chrome keeps {drawn:?} at {theirs} wide, our ink is {ours}"
            ));
        }
        if !apart && known {
            wrong.push(format!(
                "{drawn:?} now agrees with Chrome. That is a fix -- delete the row from KNOWN_ELLIPSIS"
            ));
        }
    }

    assert!(compared > 0, "the ellipsis table has no rows to compare");
    assert!(
        wrong.is_empty(),
        "{} rows differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
    eprintln!("ellipsis: {compared} rows compared against Chrome");
}

/// The three children every flex case lays out, and their colours. Sized by a
/// spacer inside rather than a height of their own, or `Align::Stretch` has
/// nothing to change.
const FLEX_CHILDREN: [(f32, f32, Color); 3] = [
    (24.0, 20.0, Color::rgb(220, 40, 40)),
    (30.0, 32.0, Color::rgb(40, 80, 220)),
    (20.0, 44.0, Color::rgb(40, 140, 60)),
];

/// The container every flex case is laid out in.
const FLEX_BOX: (f32, f32) = (160.0, 80.0);

/// The flex rows answered differently from Chrome: empty. The `baseline` rows
/// cannot fail on a baseline -- a textless box's baseline is its bottom edge,
/// so they ask what `flex-end` asks. `fixtures/baseline-alignment` is the case
/// that discriminates.
const KNOWN_FLEX: &[&str] = &[];

/// The `Justify` a table's name asks for.
fn justify_of(name: &str) -> meo_canvas::Justify {
    match name {
        "flex-end" => meo_canvas::Justify::FlexEnd,
        "center" => meo_canvas::Justify::Center,
        "space-between" => meo_canvas::Justify::SpaceBetween,
        "space-around" => meo_canvas::Justify::SpaceAround,
        "space-evenly" => meo_canvas::Justify::SpaceEvenly,
        _ => meo_canvas::Justify::FlexStart,
    }
}

/// The `Align` a table's name asks for.
fn align_of(name: &str) -> meo_canvas::Align {
    match name {
        "flex-end" => meo_canvas::Align::FlexEnd,
        "center" => meo_canvas::Align::Center,
        "stretch" => meo_canvas::Align::Stretch,
        "baseline" => meo_canvas::Align::Baseline,
        _ => meo_canvas::Align::FlexStart,
    }
}

/// Each child's rectangle relative to the container, read from pixels: a child
/// drawn in its own colour has a bounding box, which equals Chrome's layout
/// rectangle when the layout agrees.
fn flex_rects(justify: &str, align: &str) -> Vec<[f32; 4]> {
    let children: Vec<Element> = FLEX_CHILDREN
        .iter()
        .map(|(width, content, ink)| {
            Box::new()
                .display(Display::Block)
                .width(px(*width))
                .background_color(*ink)
                // The spacer, which gives the child a height without setting
                // one.
                .children(
                    Box::new().display(Display::Block).height(px(*content)),
                )
        })
        .collect();

    let page = render(
        FLEX_BOX,
        Box::new()
            // Flex, and said rather than inherited from `Box::new`'s default:
            // Chrome's own markup sets `display:flex` here, so this is the
            // property being measured rather than a stand-in.
            .display(Display::Flex)
            .size(px(FLEX_BOX.0), px(FLEX_BOX.1))
            .justify_content(justify_of(justify))
            .align_items(align_of(align))
            .children(children),
    );

    FLEX_CHILDREN
        .iter()
        .map(|(_, _, ink)| {
            page.extent(*ink).map_or([-1.0, -1.0, -1.0, -1.0], |found| {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "a coordinate on a 160-pixel page is exact in an f32"
                )]
                [
                    found.0 as f32,
                    found.1 as f32,
                    (found.2 - found.0 + 1) as f32,
                    (found.3 - found.1 + 1) as f32,
                ]
            })
        })
        .collect()
}

#[test]
fn flex_alignment_matches_chrome() {
    let text = include_str!("assets/chrome/flex-alignment.tsv");
    let mut wrong = Vec::new();
    let mut compared = 0_usize;
    let mut cases: BTreeMap<(String, String), Vec<[f32; 4]>> = BTreeMap::new();

    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        // The wrapping rows live in the same file under a second shape, and
        // this walker lays out three children where they have six: without
        // this it reads `six-children` as an alignment and reports three
        // rectangles against Chrome's six.
        if fields.len() < 7 || fields[1] == "six-children" {
            continue;
        }
        let numbers: Vec<f32> = fields[3..7]
            .iter()
            .filter_map(|field| field.parse().ok())
            .collect();
        if numbers.len() != 4 {
            continue;
        }
        cases
            .entry((fields[0].to_owned(), fields[1].to_owned()))
            .or_default()
            .push([numbers[0], numbers[1], numbers[2], numbers[3]]);
    }

    for ((justify, align), theirs) in &cases {
        let ours = flex_rects(justify, align);
        let known = KNOWN_FLEX.contains(&align.as_str());
        // A pixel of tolerance: a bounding box read from ink is the box the
        // colour covers, and a layout rectangle is where the box was put.
        let apart = ours.iter().zip(theirs.iter()).any(|(ours, theirs)| {
            ours.iter()
                .zip(theirs.iter())
                .any(|(ours, theirs)| (ours - theirs).abs() > 1.0)
        });
        compared += 1;

        if apart && !known {
            wrong.push(format!(
                "{justify} | {align}: we lay the children out at {ours:?}, Chrome at {theirs:?}"
            ));
        }
        if !apart && known {
            wrong.push(format!(
                "{justify} | {align}: now agrees with Chrome. That is a fix -- delete the row from KNOWN_FLEX"
            ));
        }
    }

    assert!(compared > 0, "the flex table has no cases to compare");
    assert!(
        wrong.is_empty(),
        "{} cases differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
    eprintln!(
        "flex alignment: {compared} cases compared, {} pinned",
        KNOWN_FLEX.len()
    );
}

/// The six children the wrapping cases lay out, each in its own colour: a child
/// is located by its ink, and two sharing a colour would report one box
/// covering both.
const FLEX_SIX: [(f32, f32, Color); 6] = [
    (24.0, 20.0, Color::rgb(220, 40, 40)),
    (30.0, 32.0, Color::rgb(40, 80, 220)),
    (20.0, 44.0, Color::rgb(40, 140, 60)),
    (24.0, 20.0, Color::rgb(230, 160, 30)),
    (30.0, 32.0, Color::rgb(150, 60, 190)),
    (20.0, 44.0, Color::rgb(30, 170, 180)),
];

/// The box the wrapping cases use: narrow enough that six children cannot fit.
const FLEX_WRAP_BOX: (f32, f32) = (88.0, 56.0);

/// The page that box is drawn on, taller than the box: Chrome puts `wrap`'s
/// second line at y=44 in a box 56 tall and `wrap-reverse`'s at y=-32, and a
/// line off the page reads as a child never placed.
const FLEX_WRAP_PAGE: (f32, f32) = (88.0, 200.0);

/// Where the container sits on that page, so a line above it is still drawn.
const FLEX_WRAP_AT: f32 = 72.0;

/// The wrapping cases answered differently from Chrome: empty. taffy's safe
/// fallback for an overflowing distributed alignment throws the reversal away,
/// where Chrome keeps it; `bottom_align_reversed_wraps` in `layout.rs` shifts
/// the stack after the solve.
const KNOWN_WRAP: &[&str] = &[];

/// Each child's rectangle when six of them are wrapped in a narrow box.
fn wrap_rects(wrap: &str) -> Vec<[f32; 4]> {
    let children: Vec<Element> = FLEX_SIX
        .iter()
        .map(|(width, content, ink)| {
            Box::new()
                .display(Display::Block)
                .width(px(*width))
                .background_color(*ink)
                .children(
                    Box::new().display(Display::Block).height(px(*content)),
                )
        })
        .collect();

    let page = render(
        FLEX_WRAP_PAGE,
        Box::new()
            // As above: Chrome sets `display:flex` on the wrap container, so
            // this states the property rather than inheriting a default that
            // happens to match.
            .display(Display::Flex)
            .position_type(PositionType::Absolute)
            .position(sides(Some(px(FLEX_WRAP_AT)), None, None, Some(px(0.0))))
            .size(px(FLEX_WRAP_BOX.0), px(FLEX_WRAP_BOX.1))
            .flex_wrap(match wrap {
                "wrap" => meo_canvas::FlexWrap::Wrap,
                "wrap-reverse" => meo_canvas::FlexWrap::WrapReverse,
                _ => meo_canvas::FlexWrap::NoWrap,
            })
            .children(children),
    );

    FLEX_SIX
        .iter()
        .map(|(_, _, ink)| {
            page.extent(*ink).map_or([-1.0, -1.0, -1.0, -1.0], |found| {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "a coordinate on an 88-pixel page is exact in an f32"
                )]
                [
                    found.0 as f32,
                    found.1 as f32 - FLEX_WRAP_AT,
                    (found.2 - found.0 + 1) as f32,
                    (found.3 - found.1 + 1) as f32,
                ]
            })
        })
        .collect()
}

#[test]
fn flex_wrapping_matches_chrome() {
    let text = include_str!("assets/chrome/flex-alignment.tsv");
    let mut wrong = Vec::new();
    let mut cases: BTreeMap<String, Vec<[f32; 4]>> = BTreeMap::new();

    for line in text.lines() {
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 7 || fields.get(1) != Some(&"six-children") {
            continue;
        }
        let numbers: Vec<f32> = fields[3..7]
            .iter()
            .filter_map(|field| field.parse().ok())
            .collect();
        if numbers.len() == 4 {
            cases
                .entry(fields[0].to_owned())
                .or_default()
                .push([numbers[0], numbers[1], numbers[2], numbers[3]]);
        }
    }

    for (wrap, theirs) in &cases {
        let ours = wrap_rects(wrap);
        let known = KNOWN_WRAP.contains(&wrap.as_str());
        let apart = ours.iter().zip(theirs.iter()).any(|(ours, theirs)| {
            ours.iter()
                .zip(theirs.iter())
                .any(|(ours, theirs)| (ours - theirs).abs() > 1.0)
        });

        if apart && !known {
            wrong.push(format!(
                "{wrap}: we lay the six children out at {ours:?}, Chrome at {theirs:?}"
            ));
        }
        if !apart && known {
            wrong.push(format!(
                "{wrap}: now agrees with Chrome. That is a fix -- delete the row from KNOWN_WRAP"
            ));
        }
    }

    assert!(!cases.is_empty(), "the flex table has no wrapping cases");
    assert!(
        wrong.is_empty(),
        "{} cases differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
    eprintln!(
        "flex wrapping: {} cases compared, {} pinned",
        cases.len(),
        KNOWN_WRAP.len()
    );
}

/// The grid every placement case is laid out in: three columns, three rows.
const GRID: (f32, f32) = (120.0, 90.0);

/// The page the grid sits on, larger than the grid and offset into it: an item
/// placed outside the explicit tracks is still drawn -- Chrome puts `column`
/// flow's fifth item at x=120 -- and must be on the page to be found.
const GRID_PAGE: (f32, f32) = (240.0, 200.0);

/// Where the grid sits on that page.
const GRID_AT: f32 = 40.0;

/// The six items, each in its own colour, so no two report one bounding box.
const GRID_INK: [Color; 6] = [
    Color::rgb(220, 40, 40),
    Color::rgb(40, 80, 220),
    Color::rgb(40, 140, 60),
    Color::rgb(230, 160, 30),
    Color::rgb(150, 60, 190),
    Color::rgb(30, 170, 180),
];

/// Which flows we place differently from Chrome today.
const KNOWN_GRID: &[&str] = &[];

/// Where each item lands for one auto-placement flow. The second item spans
/// three columns and the fifth two rows, since uniform single cells place
/// identically under all four flows and `dense` would read as its plain
/// counterpart.
fn grid_rects(flow: GridAutoFlow) -> Vec<Option<[f32; 4]>> {
    let children: Vec<Element> = GRID_INK
        .iter()
        .enumerate()
        .map(|(index, ink)| {
            let item =
                Box::new().display(Display::Block).background_color(*ink);
            match index {
                // `span 3` and `span 2` with no start line: the placement is
                // still the algorithm's, and only the size is ours.
                1 => item.grid_column(GridPlacement {
                    start: None,
                    span: Some(3),
                }),
                4 => item.grid_row(GridPlacement {
                    start: None,
                    span: Some(2),
                }),
                _ => item,
            }
        })
        .collect();

    let page = render(
        GRID_PAGE,
        Box::new()
            .position_type(PositionType::Absolute)
            .position(sides(Some(px(GRID_AT)), None, None, Some(px(GRID_AT))))
            .size(px(GRID.0), px(GRID.1))
            .display(Display::Grid)
            // The track lists spelt out rather than through `Style::columns`:
            // that sugar is on `Style` alone, and this builds an `Element`.
            .grid_template_columns(vec![TrackSize::Fraction(1.0); 3])
            .grid_template_rows(vec![TrackSize::Fraction(1.0); 3])
            .grid_auto_flow(flow)
            .children(children),
    );

    GRID_INK
        .iter()
        .map(|ink| {
            page.extent(*ink).map(|found| {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "a coordinate on a 240-pixel page is exact in an f32"
                )]
                [
                    found.0 as f32 - GRID_AT,
                    found.1 as f32 - GRID_AT,
                    (found.2 - found.0 + 1) as f32,
                    (found.3 - found.1 + 1) as f32,
                ]
            })
        })
        .collect()
}

#[test]
fn grid_placement_matches_chrome() {
    let text = include_str!("assets/chrome/grid-placement.tsv");
    let mut wrong = Vec::new();
    let mut compared = 0_usize;
    let mut unobservable = 0_usize;
    let mut cases: BTreeMap<String, Vec<[f32; 4]>> = BTreeMap::new();

    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() < 6 {
            continue;
        }
        let numbers: Vec<f32> = fields[2..6]
            .iter()
            .filter_map(|field| field.parse().ok())
            .collect();
        if numbers.len() != 4 {
            continue;
        }
        cases
            .entry(fields[0].to_owned())
            .or_default()
            .push([numbers[0], numbers[1], numbers[2], numbers[3]]);
    }

    for (flow, theirs) in &cases {
        let ours = grid_rects(match flow.as_str() {
            "column" => GridAutoFlow::Column,
            "row-dense" => GridAutoFlow::RowDense,
            "column-dense" => GridAutoFlow::ColumnDense,
            _ => GridAutoFlow::Row,
        });
        let known = KNOWN_GRID.contains(&flow.as_str());
        let mut apart = false;

        for (index, theirs) in theirs.iter().enumerate() {
            let empty = theirs[2] == 0.0 || theirs[3] == 0.0;
            match (ours.get(index).copied().flatten(), empty) {
                // Chrome placed it in a zero-size implicit track, which paints
                // nothing: where it went is not a pixel question, but that it
                // went nowhere is, and that is what this arm counts.
                (None, true) => unobservable += 1,
                (Some(ours), true) => {
                    apart = true;
                    wrong.push(format!(
                        "{flow} item {index}: Chrome gives it no area at {:?} and we paint it at {ours:?}",
                        [theirs[0], theirs[1]]
                    ));
                }
                (None, false) => {
                    apart = true;
                    wrong.push(format!(
                        "{flow} item {index}: Chrome puts it at {theirs:?} and we paint nothing"
                    ));
                }
                // A pixel of tolerance, as the flex matrix takes: a bounding
                // box read from ink is the box the colour covers, and a
                // layout rectangle is where the box was put.
                (Some(ours), false) => {
                    if ours
                        .iter()
                        .zip(theirs.iter())
                        .any(|(ours, theirs)| (ours - theirs).abs() > 1.0)
                    {
                        apart = true;
                        wrong.push(format!(
                            "{flow} item {index}: we place it at {ours:?}, Chrome at {theirs:?}"
                        ));
                    }
                }
            }
            compared += 1;
        }

        if apart && known {
            wrong.retain(|line| !line.starts_with(flow.as_str()));
        }
        if !apart && known {
            wrong.push(format!(
                "{flow}: now agrees with Chrome. That is a fix -- delete the row from KNOWN_GRID"
            ));
        }
    }

    assert_eq!(cases.len(), 4, "all four flows have to be in the table");
    assert!(compared > 0, "the grid table has no cases to compare");
    assert!(
        wrong.is_empty(),
        "{} placements differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
    eprintln!(
        "grid placement: {compared} placements compared across {} flows, \
         {unobservable} of them zero-area and checked only for absence, \
         {} pinned",
        cases.len(),
        KNOWN_GRID.len()
    );
}
