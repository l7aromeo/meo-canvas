//! Chrome's answers, one row per combination, put through this renderer.
//!
//! # Why a table and not more fixtures
//!
//! A fixture is a scene, and every defect this project found in a week was
//! **combinational**: a zero-width edge beside a rounded corner, an absolute
//! child under a static clipper, a negative `z_index` under a parent that
//! establishes no context, a tile that divides its box evenly. Seventeen
//! scenes cannot cover a product of properties, and seventeen hundred images
//! would be unreadable. A row is cheap where a picture is not.
//!
//! Fixtures keep the job a table cannot do: saying that a picture *looks*
//! right. This says only that an answer matches Chrome's.
//!
//! # Where the answers came from
//!
//! `tests/assets/chrome/*.json`, measured in a browser and checked in. They
//! are the one thing here that is not downstream of our own arithmetic --
//! every other expectation in this project is measured from what we drew.
//!
//! # How a row is answered on our side
//!
//! By rendering and reading pixels, not by asking the layout engine. A layout
//! number compared against Chrome's shares nothing with what a caller sees; a
//! pixel is the same currency Chrome's `elementFromPoint` and
//! `getBoundingClientRect` were read in.
//!
//! Both walkers report **every** failing row rather than the first, for the
//! reason `property_effect.rs` does: a fix usually moves a family, and the
//! useful report is the family.
//!
//! # Rows this renderer cannot express
//!
//! Named, counted and excluded rather than skipped: a silent skip turns a
//! conformance table into a self-portrait. `display: inline-block` and
//! `table-cell` are Chrome's and have no variant in our `Display`, which is
//! flex, grid, block and none.

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

/// Where the parent sits on it.
///
/// Not at the origin, and that is the point: `fixed` resolves against the page
/// and every other position against the parent, so a parent at 0,0 would make
/// the two indistinguishable. Chrome's probe had the same offset for the same
/// reason -- its cases sat inside a padded body.
const PARENT_AT: (f32, f32) = (44.0, 44.0);

/// A's colour, B's colour, and the parent's.
const A_INK: Color = Color::rgb(220, 40, 40);
const B_INK: Color = Color::rgb(40, 80, 220);
const PARENT_INK: Color = Color::rgb(238, 238, 238);

/// Which box a render paints.
///
/// Both boxes are in the tree every time. Hiding one by **not painting it**
/// rather than by removing it is what keeps the layout identical: B is pulled
/// back over A by a negative margin, and a B without an A beside it lands
/// somewhere else entirely -- which is how this walker's first run reported
/// 134 disagreements that were all its own.
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

    // The parent's own background is painted only when both boxes are, and
    // that is load-bearing: a child at `z_index: -1` hoists out of a parent
    // that establishes no stacking context and paints *behind* that parent's
    // background, so a solo render with the grey drawn finds no such child at
    // all and cannot say where it landed. The layout is identical either way.
    let ink = if draw == Draw::Both {
        PARENT_INK
    } else {
        Color::rgba(0, 0, 0, 0)
    };
    let mut parent = Box::new()
        .size(px(90.0), px(60.0))
        // `Relative`, as Chrome's probe had it, and **not** `Absolute` even
        // though that would place it in one property: an absolutely positioned
        // box is a containing block either way, but the two differ on whether
        // the parent establishes a stacking context, which is the very thing
        // half these rows are about. The offset comes from a wrapper instead.
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

/// The rows this renderer answers differently from Chrome today.
///
/// Pinned rather than left failing, for the reason `property_effect.rs` pins
/// its no-ops: a walker that fails is a walker nobody can run, and the useful
/// signal is a **change** in this set. A row that starts agreeing is a fix and
/// fails this test until it is deleted from here.
///
/// **Empty, and it was not.** Two families lived here and both are closed:
///
/// 1. `z_index: 0` against `auto` on a positioned box. CSS step 6 holds
///    positioned descendants with `auto` and child stacking contexts with `0`
///    **together**, in tree order, so the later box wins whichever spelling it
///    uses. The painter had ranked the explicit zero above the automatic one.
/// 2. `z_index` on a **static** flex or grid item. Flexbox §5.4 gives such an
///    item a stacking context although it is not positioned, which puts it at
///    step 6 above a static sibling with `auto` at step 5.
///
/// Those two pull in opposite directions -- the first says a zero does *not*
/// outrank an auto, the second says it does -- and what separates them is
/// whether the box is positioned. A painter that reads only the index gets one
/// family right and the other wrong, which is what this table caught.
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
    let table = read_rows(include_str!("assets/chrome/paint-order.json"));
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
            // A `fixed` box resolves against the page here and against the
            // viewport there, and Chrome's probe measured each case where the
            // flow happened to put it -- so whether the two boxes overlap at
            // all depends on an offset we cannot reproduce. Where they do
            // overlap the row is compared, because which box is on top does
            // not depend on that offset. Where they do not, the row is
            // excluded and counted: a silent skip would report the rows we
            // could answer as though they were the whole table.
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
    let table = read_rows(include_str!("assets/chrome/box-sizing.json"));
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

/// A reader for the tables, which are flat enough not to need a dependency.
///
/// An array of objects whose values are strings or numbers, one row per
/// combination. Every value is kept as the text it was written as: a row's
/// `border` is `"6"` and its `outer` is `"112"`, and the walker that reads
/// them decides which is a number. Stopping at the seam between JSON and
/// meaning is what keeps this short enough to be obviously right.
/// Every row of a table.
///
/// Panics naming the byte offset when the text is not the flat array of
/// flat objects this reads: a malformed table is a broken checkout rather
/// than a case to skip.
fn read_rows(text: &str) -> Vec<Row> {
    let bytes = text.as_bytes();
    let mut at = read_space(bytes, 0);
    assert_eq!(bytes.get(at), Some(&b'['), "a table is an array of rows");
    at += 1;

    let mut rows = Vec::new();
    loop {
        at = read_space(bytes, at);
        match bytes.get(at) {
            Some(&b']') | None => return rows,
            Some(&b',') => at += 1,
            Some(&b'{') => {
                let (row, next) = read_object(bytes, at);
                rows.push(row);
                at = next;
            }
            other => {
                unreachable!("byte {at}: expected a row, found {other:?}")
            }
        }
    }
}

/// One `{ "key": value, .. }`, and where it ended.
fn read_object(bytes: &[u8], from: usize) -> (Row, usize) {
    let mut row = Row::new();
    let mut at = from + 1;
    loop {
        at = read_space(bytes, at);
        match bytes.get(at) {
            Some(&b'}') => return (row, at + 1),
            Some(&b',') => at += 1,
            Some(&b'"') => {
                let (key, next) = read_string(bytes, at);
                at = read_space(bytes, next);
                assert_eq!(
                    bytes.get(at),
                    Some(&b':'),
                    "byte {at}: a key takes a colon"
                );
                let (value, next) =
                    read_value(bytes, read_space(bytes, at + 1));
                row.insert(key, value);
                at = next;
            }
            other => {
                unreachable!("byte {at}: expected a key, found {other:?}")
            }
        }
    }
}

/// A string or a number, as the text it was written as.
fn read_value(bytes: &[u8], from: usize) -> (String, usize) {
    if bytes.get(from) == Some(&b'"') {
        return read_string(bytes, from);
    }
    let mut at = from;
    while at < bytes.len()
        && !matches!(bytes[at], b',' | b'}' | b' ' | b'\n' | b'\r' | b'\t')
    {
        at += 1;
    }
    let text = String::from_utf8_lossy(&bytes[from..at]).into_owned();
    // `112.0` and `112` are the same answer, and a walker comparing text
    // would call them different. Only a number with a point is trimmed:
    // trimming zeros off every number turns `10` into `1` and `0` into
    // nothing, which is a defect this had for exactly one run.
    if !text.contains('.') {
        return (text, at);
    }
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    (trimmed.to_owned(), at)
}

/// A quoted string, with the escapes these tables actually use.
fn read_string(bytes: &[u8], from: usize) -> (String, usize) {
    let mut out = String::new();
    let mut at = from + 1;
    while at < bytes.len() {
        match bytes[at] {
            b'"' => return (out, at + 1),
            b'\\' => {
                at += 1;
                out.push(char::from(*bytes.get(at).unwrap_or(&b'"')));
                at += 1;
            }
            byte => {
                out.push(char::from(byte));
                at += 1;
            }
        }
    }
    unreachable!("byte {from}: a string is never closed");
}

/// Past any whitespace.
const fn read_space(bytes: &[u8], from: usize) -> usize {
    let mut at = from;
    while at < bytes.len() && bytes[at].is_ascii_whitespace() {
        at += 1;
    }
    at
}

/// The overflow rows this renderer answers differently from Chrome today.
///
/// **Empty: all 240 rows agree**, the 120 measured by hand and the 120 offset
/// rows added with the fifth letter. Kept with its history rather than deleted,
/// because an empty list with no history is a list nobody knows the shape of.
///
/// It has held two sets. The first was fifty-one rows and **every one of them
/// was this walker's own scene**: `outer` was placed by absolute insets, which
/// is the natural way to put a box at a known point and which establishes a
/// block formatting context, so nothing could collapse out of it while
/// everything collapses out of Chrome's `position: relative` one. It read as
/// fifty-one renderer defects and as a missing layout feature, and taffy had
/// implemented that feature in full. `outer` is now an in-flow box behind a
/// padded wrapper, and it is **found by its colour** rather than assumed to be
/// anywhere, because an escaping margin moves it.
///
/// The second was ten real rows: `hidden` or `scroll` on a clipper carrying a
/// transform, with an out-of-flow child. The transform captured the child for
/// positioning and the **clip** did not follow, because `escapes_clip` decided
/// by position type where the layout pass decides by
/// `layout::is_containing_block`. One predicate now answers both, and these
/// ten rows are what said so.
const KNOWN_OVERFLOW: &[&str] = &[];

/// The page the overflow cases are drawn on, and where `outer` sits on it.
///
/// Chrome's probe had `outer` at 40,40 -- its body carried that much padding --
/// and a `fixed` box resolves against the viewport there and against the page
/// here. Putting `outer` at the same offset makes those two the same thing, so
/// the `fixed` rows are comparable instead of being excluded: without it every
/// number in them is off by exactly the offset, which reads as a defect.
const OVERFLOW_PAGE: (f32, f32) = (280.0, 200.0);

/// Where `outer` sits on that page.
const OUTER_AT: (f32, f32) = (40.0, 40.0);

/// The three points Chrome's `elementFromPoint` was asked at, relative to
/// `outer`: inside, past the right edge, past the bottom edge.
const PROBES: [(f32, f32); 3] = [(60.0, 45.0), (88.0, 50.0), (60.0, 68.0)];

/// The clipper's grey and the child's red.
const CLIPPER_INK: Color = Color::rgb(238, 238, 238);
const CHILD_INK: Color = Color::rgb(220, 40, 40);

/// `outer`'s white, and the page behind it.
///
/// Two colours rather than one because `outer` no longer sits at a known
/// place: an escaping margin moves it, which is the behaviour under test, so
/// the walker finds it by its colour and reads every coordinate against what
/// it finds. It is also what tells Chrome's `o` from its `b`.
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

/// The offsets a row's fifth letter names, as `(top, left)`.
///
/// Written on the child whatever its position type is. That a `static` child
/// **ignores** them is the property the letter was added to measure -- the
/// `PositionType::Static` arm of `layout.rs` returns `Rect::auto()` and until
/// this letter existed nothing in the suite could see it do so -- and
/// withholding the offsets from the static rows would assume that answer
/// instead of measuring it.
fn offsets_of(code: &str) -> Option<(f32, f32)> {
    match &code[4..5] {
        "i" => Some((6.0, 8.0)),
        "n" => Some((-6.0, -8.0)),
        _ => None,
    }
}

/// The clipper and its child, built from a row's five letters.
///
/// `clip` is false for the render that measures where the child *is*: Chrome
/// reports a layout rectangle, which a clip does not move, and reading a
/// clipped child's pixels would report the intersection instead. Our own
/// `overflow` moves nothing, so turning it off costs no fidelity here -- and if
/// it ever does, the rect rows are what will say so.
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

    // `outer` is **in flow and relative**, as Chrome's was, and the offset
    // comes from padding on a wrapper rather than from insets on `outer`
    // itself.
    //
    // That distinction is the whole scene. An absolutely positioned box
    // establishes a block formatting context, which is one of the four things
    // that stop margin collapsing -- so an `outer` placed by insets cannot let
    // a child's margin escape through it, and every in-flow row comes out
    // twenty pixels low. Padding on the wrapper stops the margin escaping any
    // further, which is exactly what the body's padding did in the browser.
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

/// The child's rectangle, minus `outer`'s, as it is actually drawn.
///
/// Measured with the clip **honoured**. An earlier version forced `overflow`
/// to `Visible` here so the child's whole box would show, and that quietly
/// removed the block formatting context the property establishes -- so every
/// `hidden` row came back with a margin collapsed that Chrome had blocked.
/// The clip cannot be turned off in order to measure what it clips, which is
/// why only the `visible` rows compare a rectangle at all.
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

/// Whether the table can tell a `relative` child from a `static` one.
///
/// **For 120 rows it could not.** An in-flow child was placed by margins and
/// never given an inset, and `position: relative` with no offsets *is*
/// `position: static`, so a renderer that ignored `relative` entirely passed
/// every row. The fifth letter is what fixed that, and this asks the property
/// rather than the row count -- delete every offset row for tidiness and the
/// caller fails by name instead of the suite going quietly back to passing for
/// the wrong reason.
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
        // A `fixed` **child** is placed against the viewport there and the page
        // here, and Chrome measured every case where the flow happened to put
        // it -- so its rectangle minus `outer`'s carries an offset that varies
        // per row and cannot be reproduced. Two rows prove it: `vFSn` is only
        // consistent with `outer` at y=40 and `vSFn` only with y=60, because
        // the collapsing margin in the second case moved `outer` itself.
        // Excluded and counted rather than compared.
        let Some(ours) = rect_of(row) else {
            // A child with no pixels on the page: either it is off it entirely,
            // or the clip left nothing of it. Chrome reports a rectangle for
            // both, which is an answer; ours is only that it is not here.
            off_the_page += 1;
            continue;
        };

        // The rectangle is compared on the `visible` rows only. There the
        // painted box **is** the layout box, so the two numbers are the same
        // quantity. Where a clip is on they are not: Chrome reports the layout
        // rectangle and we can only see what survived the clip, and working
        // out what *should* have survived means implementing the rule under
        // test -- `overflow` does not clip a descendant whose containing block
        // is an ancestor of the clipper, which is half of what these rows are
        // about. Those rows are answered by the probes, which Chrome measured
        // directly and which are the observable fact either way.
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
        // A pinned row that has started agreeing says so, which is the half
        // this walker was missing: `KNOWN_OVERFLOW` suppressed a failure and
        // could not report a fix, so ten rows were silently correct for an
        // hour and only a hand-emptied list found out. A pinned list that
        // cannot tell you it is stale is a list that only grows.
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
    // **Every row has to have been compared.** Each of the three ways out of
    // the loop above is a `continue`, and a row that leaves that way is
    // reported in the summary and in nothing else -- so a renderer drawing
    // *nothing* takes all 240 of them and the walker passes. Measured, not
    // supposed: setting the child to `Display::None` gives `240 rows, 240
    // placed off the page` and a green test. That is a check that cannot fail,
    // and it was found by a control written for an unrelated survey.
    //
    // The three numbers are exact today, so pinning them costs nothing: a row
    // that starts being skipped is either a renderer that stopped drawing or a
    // scene that stopped being comparable, and both want a person.
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

/// The truncation rows this renderer answers differently from Chrome today.
///
/// Keyed by the string Chrome keeps, which is the answer under test.
///
/// **Empty, and it held two rows that were two different defects.**
///
/// `Antidisestabli…` was a word with no break opportunity in it: Chrome cuts
/// mid-word rather than overflow and we drew the whole word, 171 pixels of ink
/// in a box 90 wide. The character-level refill was ported and correct; what
/// was missing was **when it ran**. Truncation fired on the line count, and a
/// word placed whole however wide it is never overflows a count. `lines.rs`
/// now triggers on the width as well: once `max_lines` has had its say, a
/// marked line still wider than its box is rebuilt.
///
/// `Flower of …` was one **space**. v1 pops trailing whitespace before the
/// marker so it is not pushed away from the text it belongs to; Chrome keeps
/// the longest prefix that fits and a space is part of the string. It survives
/// only while it fits, which is the same rule and not a second one -- the line
/// is measured with the marker on it, so the 22px row in 90 keeps its space at
/// 89.98 wide and the 16px row in 60 does not.
///
/// A width comparison would have called the second a rounding argument. It is
/// a content difference, which is why this table is measured as a string.
const KNOWN_ELLIPSIS: &[&str] = &[];

/// The font every ellipsis case is measured in, and the file behind it.
const ELLIPSIS_FONT: (&str, &str) = (
    "Fixture",
    "../meo-canvas-core/tests/assets/fonts/Oswald-VariableFont_wght.ttf",
);

/// How wide the ink of one line is, in whole pixels, or `None` if there is
/// none.
///
/// The comparison this feeds is **structural**: Chrome's rasteriser is not
/// ours, so what is compared is which glyphs were drawn rather than which
/// pixels. A line truncated by a word boundary and one truncated by character
/// differ by a whole word, which is far outside any antialiasing margin.
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

/// The three children every flex case lays out, and the colour each is drawn
/// in.
///
/// Sized by a **spacer inside them** rather than by a height of their own,
/// which is what gives `Align::Stretch` something to change: an item with its
/// own height stretches to the height it already had, and a matrix built that
/// way reports one of its five alignments as a duplicate of another.
const FLEX_CHILDREN: [(f32, f32, Color); 3] = [
    (24.0, 20.0, Color::rgb(220, 40, 40)),
    (30.0, 32.0, Color::rgb(40, 80, 220)),
    (20.0, 44.0, Color::rgb(40, 140, 60)),
];

/// The container every flex case is laid out in.
const FLEX_BOX: (f32, f32) = (160.0, 80.0);

/// The rows we answer differently from Chrome today.
///
/// **Empty: all thirty cases agree**, baseline included — and that last part
/// is worth reading carefully rather than as good news.
///
/// **The `baseline` rows here cannot fail on a baseline.** These children are
/// boxes with no text in them, and a box's baseline *is* its bottom margin
/// edge — in Chrome as much as here — so `baseline` and `flex-end` ask this
/// matrix the same question and get one answer. The rows agreeing says our
/// flex alignment is right; it says nothing about baselines, and a reader who
/// took thirty green rows as covering `Align::Baseline` would be wrong.
///
/// The case that does discriminate is `fixtures/baseline-alignment`, where a
/// measured text leaf reports a baseline of its own.
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

/// Each child's rectangle, relative to the container, as we lay them out.
///
/// Read from the pixels because that is the currency both sides can be asked
/// in: Chrome reports a layout rectangle and we have no such API, but a child
/// drawn in a colour of its own has a bounding box, and the two are the same
/// number when the layout agrees.
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

/// The six children the wrapping cases lay out, each in a colour of its own.
///
/// Six colours rather than the matrix's three repeated: a child is located by
/// its ink, and two children sharing a colour would report one bounding box
/// covering both. The widths and contents are the matrix's three, twice.
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

/// The page that box is drawn on.
///
/// **Taller than the box**, because a wrapped line can fall outside its
/// container: Chrome puts `wrap`'s second line at `y = 44` in a box 56 tall
/// and `wrap-reverse`'s at `y = -32`, so both overflow. Measured on a page the
/// size of the box, the overflowing halves are not there to find and read as
/// children we failed to place -- which is what this walker reported before
/// the page grew.
const FLEX_WRAP_PAGE: (f32, f32) = (88.0, 200.0);

/// Where the container sits on that page, so a line above it is still drawn.
const FLEX_WRAP_AT: f32 = 72.0;

/// Which wrapping cases we answer differently from Chrome today.
///
/// **One, and it is where the two lines sit rather than whether they exist.**
/// `wrap` agrees exactly: line one at `y = 0` and line two at `y = 44`, both
/// 44 tall in a box 56 tall, so the second overflows in Chrome and here alike.
/// `wrap-reverse` reverses the stack in both, and the two disagree about where
/// the pair is placed: Chrome puts it at `y = -32` and `y = 12`, bottom-
/// aligned so the *last* line ends at the box's bottom edge; we put it at
/// `y = 44` and `y = 0`, which is the same reversal packed from the top.
/// Thirty-two pixels, one property -- how a reversed line stack is aligned in
/// a container taller than it.
///
/// **This list held both cases an hour ago and the first was my measurement.**
/// The page was the size of the box, so a line at `y = 44` in a box 56 tall
/// had 12 of its 44 rows on the page and the other 32 nowhere -- and a child
/// two thirds missing reads as a child that was never placed. The page is
/// taller than the box now and the container is offset down it, so a line
/// above the box is drawn rather than lost.
/// **Empty, and it held `wrap-reverse` until the alignment was fixed.** taffy
/// applies css-align-3's *safe* fallback when a distributed alignment
/// overflows, which throws the reversal away at exactly the moment it would
/// push content out of the box; Chrome keeps it. `layout.rs` shifts the stack
/// after the solve, in `bottom_align_reversed_wraps`, and all three cases now
/// agree.
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

/// The page it sits on, and where.
///
/// **Bigger than the grid, and offset into it**, for the reason the flex
/// wrapping page is: an item the auto-placement algorithm pushes outside the
/// explicit tracks is still drawn, and a page the size of the grid would lose
/// it and report a placement failure that is really a measurement failure.
/// Chrome puts `column` flow's fifth item at `x = 120`, one whole grid width
/// to the right of the container.
const GRID_PAGE: (f32, f32) = (240.0, 200.0);

/// Where the grid sits on that page.
const GRID_AT: f32 = 40.0;

/// The six items, each in a colour of its own.
///
/// A colour each rather than a shared one, for the reason the wrapping cases
/// have six: an item is located by its ink, and two items sharing a colour
/// report one bounding box covering both.
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

/// Where each item lands, for one auto-placement flow.
///
/// The second item spans all three columns and the fifth spans two rows,
/// which is the whole point of the table: uniform single-cell items are placed
/// identically by all four flows, so a grid without a spanning item reports
/// `dense` and its plain counterpart as the same keyword. `dense` exists only
/// to go back for a hole, and an item that spans is what leaves one.
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
                // Chrome placed it in an implicit track of zero size. A
                // rectangle with no area paints nothing, so **where** it went
                // is not a question a pixel can answer -- but *that* it went
                // nowhere is, and that is what this arm checks. Named and
                // counted rather than skipped, because a silent skip turns a
                // conformance table into a self-portrait.
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
