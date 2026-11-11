//! What `currentColor` paints as, against Chrome (`current-color.tsv`), by the
//! table's own method: each case is drawn with `currentColor` and again with
//! each colour it could stand for, and the colours whose render is identical
//! must be the ones Chrome named. The renders are compared with each other,
//! never with Chrome's pixels, so the two rasterisers never meet.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions, markup};
use meo_canvas_scene::{
    Scene, Sides, Size,
    node::{LineCap, LineJoin, Node, NodeId, NodeKind, PathPaint},
    style::{
        Dimension, Length,
        effect::{BoxShadow, FillRule, TextShadow},
        layout::PositionType,
        paint::{
            BorderStyle, Color, ColorValue, Gradient, GradientGeometry,
            GradientStop, LinearDirection,
        },
        text::{LineHeight, ParagraphStyle, TextStroke},
    },
};

/// The recorded Chrome rows, which `chrome_tables_are_read` requires be read.
const TABLE: &str =
    include_str!("../../meo-canvas/tests/assets/chrome/current-color.tsv");

/// The face the table's page loaded, registered under the name it used.
const FONT: (&str, &str) =
    ("Fixture", "tests/assets/fonts/Oswald-VariableFont_wght.ttf");

/// The element's own `color` where a context sets one, as the tool sets it.
const OWN: Color = Color::rgb(220, 40, 40);

/// Its parent's `color` where a context sets one.
const PARENT: Color = Color::rgb(40, 80, 220);

/// CSS's initial `color`.
const INITIAL: Color = Color::BLACK;

/// The rows this renderer answers differently, with why: a pinned list, so a
/// row that starts agreeing fails and is removed. `drop-shadow()` is drawn by
/// meo-skia-canvas from the filter string, where `currentColor` is black, and
/// `currentColor` inside `color-mix()` or a relative colour is refused; both
/// wait on the reader taking a node's colour (`l7aromeo/meo-skia-canvas#688`).
const KNOWN_APART: [(&str, &str); 10] = [
    ("drop-shadow()", "own"),
    ("drop-shadow()", "inherited"),
    ("drop-shadow() colour omitted", "own"),
    ("drop-shadow() colour omitted", "inherited"),
    ("color-mix()", "own"),
    ("color-mix()", "inherited"),
    ("color-mix()", "unset"),
    ("relative colour", "own"),
    ("relative colour", "inherited"),
    ("relative colour", "unset"),
];

/// The rows with no counterpart here, with why: a scene has no property for a
/// text decoration's colour, which is always the text's, so there is nothing
/// to write the colour into for the candidates.
const NO_COUNTERPART: [&str; 1] = ["text decoration colour omitted"];

/// One row of the table.
struct Row {
    case: String,
    context: String,
    same_as: Vec<String>,
}

fn rows() -> Vec<Row> {
    TABLE
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            assert_eq!(fields.len(), 4, "`{line}` is not four fields");
            Row {
                case: fields[0].to_owned(),
                context: fields[1].to_owned(),
                same_as: fields[3].split(',').map(str::to_owned).collect(),
            }
        })
        .collect()
}

/// A colour as CSS writes it, for the filter cases' strings.
fn css(color: Color) -> String {
    format!("rgb({}, {}, {})", color.r, color.g, color.b)
}

/// What a case writes onto the element, given the colour to write, or `None`
/// for the case's own spelling when it leaves the colour out. Returns `false`
/// where the scene cannot hold the case at all.
fn apply(case: &str, node: &mut Node, colour: Option<ColorValue>) -> bool {
    let named = colour.unwrap_or(ColorValue::Current);
    match case {
        "background-color" | "background-color upper case" => {
            node.paint.background_color = named;
        }
        "border-color" | "border-color omitted" => {
            node.layout.border = Sides::all(4.0);
            node.paint.border_style = BorderStyle::Solid;
            if let Some(colour) = colour {
                node.paint.border_color_all = colour;
            } else if case == "border-color" {
                node.paint.border_color_all = ColorValue::Current;
            }
        }
        "box-shadow" | "box-shadow colour omitted" => {
            let mut shadow = BoxShadow {
                offset_x: 4.0,
                offset_y: 4.0,
                ..BoxShadow::default()
            };
            if colour.is_some() || case == "box-shadow" {
                shadow.color = named;
            }
            node.effects.box_shadows = vec![shadow];
        }
        "gradient stop" => {
            node.paint.gradient = Some(Gradient {
                geometry: GradientGeometry::Linear {
                    direction: LinearDirection::Angle(90.0),
                },
                stops: vec![
                    GradientStop {
                        offset: 0.0,
                        color: named,
                    },
                    GradientStop {
                        offset: 1.0,
                        color: Color::rgb(255, 255, 255).into(),
                    },
                ],
            });
        }
        "text-shadow" | "text-shadow colour omitted" => {
            let mut shadow = TextShadow {
                offset_x: 2.0,
                offset_y: 2.0,
                ..TextShadow::default()
            };
            if colour.is_some() || case == "text-shadow" {
                shadow.color = named;
            }
            node.effects.text_shadows = vec![shadow];
        }
        "text stroke" | "text stroke colour omitted" => {
            node.text.text_stroke = Some(TextStroke {
                width: 1.0,
                color: named,
            });
        }
        "color" => node.text.color = Some(named),
        // Written by `element`, which builds the node these style.
        "path fill" | "path fill omitted" | "span color" => {}
        "drop-shadow()" => {
            let written = colour.map_or_else(
                || "currentColor".to_owned(),
                |c| css(c.resolve(INITIAL)),
            );
            node.effects.filter =
                Some(format!("drop-shadow(4px 4px 0 {written})"));
        }
        "drop-shadow() colour omitted" => {
            node.effects.filter = Some(colour.map_or_else(
                || "drop-shadow(4px 4px 0)".to_owned(),
                |c| {
                    format!(
                        "drop-shadow(4px 4px 0 {})",
                        css(c.resolve(INITIAL))
                    )
                },
            ));
        }
        _ => return false,
    }
    true
}

/// What one render writes where the case's colour goes.
#[derive(Clone, Copy)]
enum Written {
    /// The property left off the element.
    Absent,
    /// The case's own spelling: `currentColor`, or the colour left out.
    AsCase,
    /// A colour written out.
    Colour(ColorValue),
}

/// The element a case styles: a square path for the path cases, as the
/// tool's SVG path; text from markup for `span color`, whose tag is the span;
/// and `Hg` otherwise. A path left with no fill takes the black every surface
/// gives it, which is SVG's initial `fill`.
fn element(case: &str, written: Written) -> Node {
    match case {
        "path fill" | "path fill omitted" => {
            let fill = match (case, written) {
                (_, Written::Colour(colour)) => colour,
                ("path fill", Written::AsCase) => ColorValue::Current,
                _ => ColorValue::Rgba(Color::BLACK),
            };
            Node::new(NodeKind::Path {
                data: "M0 0H24V24H0Z".to_owned(),
                view_box: None,
                stretch: false,
                fill: Some(PathPaint::Solid(fill)),
                stroke: None,
                line_width: 1.0,
                fill_rule: FillRule::NonZero,
                line_cap: LineCap::Butt,
                line_join: LineJoin::Miter,
                line_dash: Vec::new(),
                line_dash_offset: 0.0,
            })
        }
        "span color" => {
            let markup = match written {
                Written::Colour(colour) => {
                    format!("<color=\"{}\">Hg", css(colour.resolve(INITIAL)))
                }
                Written::AsCase => "<color=currentColor>Hg".to_owned(),
                Written::Absent => "Hg".to_owned(),
            };
            Node::new(NodeKind::Text {
                segments: markup::parse(&markup),
                paragraph: ParagraphStyle::default(),
            })
        }
        _ => Node::text("Hg"),
    }
}

/// The page: a parent filling it, and the element at (10, 10) with `Hg` in
/// it, each with the context's colour where it sets one.
fn render(context: &str, case: &str, written: Written) -> Option<Vec<u8>> {
    let (parent, own) = match context {
        "own" => (Some(PARENT), Some(OWN)),
        "inherited" => (Some(PARENT), None),
        "unset" => (None, None),
        other => unreachable!("no context named {other}"),
    };
    let mut scene = Scene::new(Size::new(48.0, 48.0));
    let mut holder = Node::new(NodeKind::Box);
    holder.layout.size = (Dimension::Points(48.0), Dimension::Points(48.0));
    holder.paint.background_color = Color::rgb(255, 255, 255).into();
    holder.text.color = parent.map(ColorValue::Rgba);
    let holder = scene
        .push(NodeId::ROOT, holder)
        .unwrap_or_else(|error| unreachable!("{error}"));

    let mut node = element(case, written);
    node.layout.position_type = PositionType::Absolute;
    node.layout.inset = Sides {
        top: Some(Length::Points(10.0)),
        right: None,
        bottom: None,
        left: Some(Length::Points(10.0)),
    };
    node.layout.size = (Dimension::Points(24.0), Dimension::Points(24.0));
    node.text.font_family = Some(FONT.0.to_owned());
    node.text.font_size = Some(16.0);
    node.text.line_height = Some(LineHeight::Length(24.0));
    node.text.color = own.map(ColorValue::Rgba);
    let colour = match written {
        Written::Colour(colour) => Some(colour),
        Written::AsCase | Written::Absent => None,
    };
    if !matches!(written, Written::Absent) && !apply(case, &mut node, colour) {
        return None;
    }
    scene
        .push(holder, node)
        .unwrap_or_else(|error| unreachable!("{error}"));

    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    renderer
        .register_font(FONT.0, FONT.1)
        .unwrap_or_else(|error| unreachable!("{error}"));
    Some(
        renderer
            .render_to_buffer(
                &scene,
                ImageFormat::Raw,
                &EncodeOptions::default(),
            )
            .unwrap_or_else(|error| unreachable!("{case}: {error}")),
    )
}

/// Which colours this renderer draws the case as, by the table's method.
fn same_as(case: &str, context: &str) -> Option<Vec<String>> {
    let measured = render(context, case, Written::AsCase)?;
    let candidates = [
        ("own", (context == "own").then_some(OWN)),
        ("parent", (context != "unset").then_some(PARENT)),
        ("initial", Some(INITIAL)),
    ];
    let mut same = Vec::new();
    for (name, colour) in candidates {
        let Some(colour) = colour else { continue };
        if render(context, case, Written::Colour(colour.into()))? == measured {
            same.push(name.to_owned());
        }
    }
    if render(context, case, Written::Absent)? == measured {
        same.push("absent".to_owned());
    }
    Some(same)
}

#[test]
fn current_color_paints_as_chrome_paints_it() {
    let rows = rows();
    assert_eq!(rows.len(), 60, "the table holds {} rows", rows.len());
    let mut wrong = Vec::new();
    let mut apart = 0;
    for row in &rows {
        let known =
            KNOWN_APART.contains(&(row.case.as_str(), row.context.as_str()));
        let skipped = NO_COUNTERPART.contains(&row.case.as_str());
        let ours = same_as(&row.case, &row.context);
        assert_eq!(
            ours.is_none(),
            skipped
                || row.case == "color-mix()"
                || row.case == "relative colour",
            "{} {}: the scene {} hold it",
            row.case,
            row.context,
            if ours.is_none() { "cannot" } else { "can" }
        );
        let agrees = ours.as_ref() == Some(&row.same_as);
        apart += usize::from(known);
        match (agrees || skipped, known) {
            (true, false) | (false, true) => {}
            (true, true) => wrong.push(format!(
                "{} {}: agrees with Chrome now; remove it from KNOWN_APART",
                row.case, row.context
            )),
            (false, false) => wrong.push(format!(
                "{} {}: ours {ours:?}, Chrome {:?}",
                row.case, row.context, row.same_as
            )),
        }
    }
    assert_eq!(
        apart,
        KNOWN_APART.len(),
        "KNOWN_APART names a row the table lacks"
    );
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
