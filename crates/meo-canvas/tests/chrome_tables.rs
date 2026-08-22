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
    Box as BoxNode, BoxSizing, Display, Element, Format, PositionType,
    Renderer, Root, Styled, hex_rgb, pct, px,
    scene::{Color, GridPlacement, Transform},
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

    let mut canvas = Root::new(size.0, size.1)
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
    let mut element = BoxNode::new()
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
    let mut parent = BoxNode::new()
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
    BoxNode::new()
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

    let mut child = BoxNode::new()
        .height(px(40.0))
        .box_sizing(if row["sizing"] == "border-box" {
            BoxSizing::BorderBox
        } else {
            BoxSizing::ContentBox
        })
        .border(sides(border, border, border, border))
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
            BoxNode::new()
                .size(pct(100.0), pct(100.0))
                .background_color(Color::rgb(240, 200, 40)),
        );
    }
    child
}

/// The host the child sits in: 200 wide, and the display the row names.
fn host(row: &Row, with_content: bool) -> Element {
    BoxNode::new()
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
/// **Empty: all 120 rows agree.** Kept with its history rather than deleted,
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
    /// The four axis letters, `OPCT`.
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

/// The clipper and its child, built from a row's four letters.
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
    let mut child = BoxNode::new()
        .size(px(50.0), px(40.0))
        .position_type(child_kind)
        .background_color(if paint_child {
            CHILD_INK
        } else {
            Color::rgba(0, 0, 0, 0)
        });
    child = if out_of_flow(child_kind) {
        child.position(sides(Some(px(20.0)), None, None, Some(px(30.0))))
    } else {
        child.margin(sides(px(20.0), px(0.0), px(0.0), px(30.0)))
    };

    let clipper_kind = position_letter(&row.code[1..2]);
    let mut clipper = BoxNode::new()
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
    let outer = BoxNode::new()
        .size(px(200.0), px(120.0))
        .position_type(PositionType::Relative)
        .display(Display::Block)
        .background_color(OUTER_INK)
        .children(clipper);

    BoxNode::new()
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
    let mut geometry = Vec::new();
    let mut painted = Vec::new();
    let mut off_the_page = 0_usize;
    let mut clipped = 0_usize;
    let mut uncomparable = 0_usize;

    for row in &overflow_rows(text) {
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
        overflow_rows(text).len(),
        off_the_page,
        KNOWN_OVERFLOW.len()
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
