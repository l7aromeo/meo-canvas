//! The root's background covers the canvas as `<html>`'s does in Chrome 153: a
//! base of its colour no effect touches, then every layer again inside its
//! filter, opacity and mask (100x100, `margin: 20px`). A stated size is the
//! canvas, so no row is smaller; `Space`, `Round` and transforms are
//! unmeasured.

use meo_canvas_core::{ImageFormat, Renderer, encode::EncodeOptions};
use meo_canvas_scene::{
    Corners, Scene, Size,
    node::{ImageSource, Node, NodeId, NodeKind},
    style::{
        Dimension, Length,
        effect::Mask,
        layout::PositionType,
        paint::{
            BackgroundImage, BackgroundRepeat, BackgroundSize, BlendMode,
            Color, Gradient, GradientGeometry, GradientStop, LinearDirection,
        },
    },
};

/// The viewport, square.
const PAGE: u32 = 100;

/// Per-channel slack for two rasterisers drawing the same ramp.
const SLACK: u8 = 3;

const RED: Color = Color::rgb(204, 0, 0);
const BLUE: Color = Color::rgb(0, 0, 204);
const GREEN: Color = Color::rgb(0, 204, 0);

/// `linear-gradient(to right, #cc0000, #0000cc)`.
fn ramp() -> Gradient {
    Gradient {
        geometry: GradientGeometry::Linear {
            direction: LinearDirection::Angle(90.0),
        },
        stops: vec![
            GradientStop {
                offset: 0.0,
                color: RED,
            },
            GradientStop {
                offset: 1.0,
                color: BLUE,
            },
        ],
    }
}

/// A 10x10 tile, green on the left half and yellow on the right, drawn by this
/// renderer so its pixels are ones the suite can restate.
fn tile() -> Vec<u8> {
    let mut scene = Scene::new(Size::new(10.0, 10.0));
    for color in [GREEN, Color::rgb(204, 204, 0)] {
        let mut half = Node::new(NodeKind::Box);
        half.layout.size = (Dimension::Points(5.0), Dimension::Points(10.0));
        half.paint.background_color = color;
        scene
            .push(NodeId::ROOT, half)
            .unwrap_or_else(|error| unreachable!("{error}"));
    }
    if let Some(root) = scene.get_mut(NodeId::ROOT) {
        root.layout.display = meo_canvas_scene::style::layout::Display::Flex;
    }
    render(&scene, ImageFormat::Png)
}

fn render(scene: &Scene, format: ImageFormat) -> Vec<u8> {
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    renderer
        .render_to_buffer(scene, format, &EncodeOptions::default())
        .unwrap_or_else(|error| unreachable!("{error}"))
}

/// A 100x100 page whose root is Chrome's `<html>` at a 20px margin, which
/// leaves it 60x60 at 20,20.
fn root_with(setup: impl FnOnce(&mut Node)) -> Scene {
    let mut scene = Scene::new(Size::new(100.0, 100.0));
    if let Some(root) = scene.get_mut(NodeId::ROOT) {
        root.layout.margin =
            meo_canvas_scene::Sides::all(Dimension::Points(20.0));
        setup(root);
    }
    scene
}

/// What one row expects at each point, as Chrome's straight-alpha RGBA.
struct Row {
    name: &'static str,
    scene: Scene,
    points: Vec<((u32, u32), [u8; 4])>,
}

const R: [u8; 4] = [204, 0, 0, 255];
const CLEAR: [u8; 4] = [0, 0, 0, 0];

#[expect(
    clippy::too_many_lines,
    reason = "one row per Chrome measurement, kept together as the table it is"
)]
fn rows() -> Vec<Row> {
    let all = |rgba: [u8; 4]| {
        [(5, 5), (5, 50), (25, 50), (50, 50), (95, 95)]
            .map(|point| (point, rgba))
            .to_vec()
    };
    vec![
        Row {
            name: "plain root, the control",
            scene: {
                let mut scene = Scene::new(Size::new(100.0, 100.0));
                scene.nodes[0].paint.background_color = RED;
                scene
            },
            points: all(R),
        },
        Row {
            name: "colour at a margin",
            scene: root_with(|root| root.paint.background_color = RED),
            points: all(R),
        },
        Row {
            name: "colour on a relative root at an inset",
            scene: root_with(|root| {
                root.layout.margin =
                    meo_canvas_scene::Sides::all(Dimension::Points(0.0));
                root.layout.position_type = PositionType::Relative;
                root.layout.inset.top = Some(Length::Points(20.0));
                root.layout.inset.left = Some(Length::Points(20.0));
                root.paint.background_color = RED;
            }),
            points: all(R),
        },
        Row {
            name: "colour under a radius the canvas ignores",
            scene: root_with(|root| {
                root.paint.background_color = RED;
                root.paint.border_radius = Corners::all(30.0);
            }),
            points: all(R),
        },
        Row {
            name: "gradient, tiled from the root's box",
            scene: root_with(|root| root.paint.gradient = Some(ramp())),
            points: vec![
                ((5, 50), [50, 0, 155, 255]),
                ((15, 50), [16, 0, 189, 255]),
                ((25, 50), [186, 0, 19, 255]),
                ((50, 50), [100, 0, 103, 255]),
                ((85, 50), [186, 0, 19, 255]),
                ((95, 50), [152, 0, 53, 255]),
            ],
        },
        Row {
            name: "image, tiled from the root's box",
            scene: root_with(|root| {
                root.paint.background_image = Some(BackgroundImage {
                    source: ImageSource::Bytes(tile()),
                    repeat: BackgroundRepeat::Repeat,
                    size: BackgroundSize::PerAxis(
                        Dimension::Auto,
                        Dimension::Auto,
                    ),
                    position: (Length::ZERO, Length::ZERO),
                });
            }),
            points: vec![
                ((5, 5), [204, 204, 0, 255]),
                ((25, 50), [204, 204, 0, 255]),
                ((50, 50), [0, 204, 0, 255]),
                ((50, 5), [0, 204, 0, 255]),
            ],
        },
        Row {
            name: "image without repeat, one tile at the root's box",
            scene: root_with(|root| {
                root.paint.background_image = Some(BackgroundImage {
                    source: ImageSource::Bytes(tile()),
                    repeat: BackgroundRepeat::NoRepeat,
                    size: BackgroundSize::PerAxis(
                        Dimension::Auto,
                        Dimension::Auto,
                    ),
                    position: (Length::ZERO, Length::ZERO),
                });
            }),
            points: vec![
                ((22, 25), [0, 204, 0, 255]),
                ((27, 25), [204, 204, 0, 255]),
                ((32, 25), CLEAR),
                ((18, 25), CLEAR),
                ((5, 5), CLEAR),
            ],
        },
        Row {
            name: "no background, nothing propagates",
            scene: root_with(|_| {}),
            points: all(CLEAR),
        },
        Row {
            name: "opacity leaves the base whole",
            scene: root_with(|root| {
                root.paint.background_color = RED;
                root.paint.opacity = 0.5;
            }),
            points: all(R),
        },
        Row {
            name: "a gradient under opacity has no base",
            scene: root_with(|root| {
                root.paint.gradient = Some(ramp());
                root.paint.opacity = 0.5;
            }),
            points: vec![
                ((5, 50), [50, 0, 155, 128]),
                ((25, 50), [185, 0, 18, 128]),
                ((50, 50), [100, 0, 102, 128]),
            ],
        },
        Row {
            name: "a filter reaches the whole canvas",
            scene: root_with(|root| {
                root.paint.background_color = RED;
                root.effects.filter = Some("invert(1)".to_owned());
            }),
            points: all([51, 255, 255, 255]),
        },
        Row {
            name: "opacity and filter together",
            scene: root_with(|root| {
                root.paint.background_color = RED;
                root.paint.opacity = 0.5;
                root.effects.filter = Some("invert(1)".to_owned());
            }),
            points: all([127, 128, 128, 255]),
        },
        Row {
            name: "a blend mode has nothing to blend with",
            scene: root_with(|root| {
                root.paint.background_color = RED;
                root.paint.blend_mode = BlendMode::Multiply;
            }),
            points: all(R),
        },
        Row {
            name: "a backdrop filter leaves the canvas",
            scene: root_with(|root| {
                root.paint.background_color = RED;
                root.effects.backdrop_filter = Some("blur(4px)".to_owned());
            }),
            points: all(R),
        },
        Row {
            name: "a mask keeps the propagated layers to the root's box",
            scene: root_with(|root| {
                root.paint.gradient = Some(ramp());
                root.effects.mask = Some(Mask::Gradient(Gradient {
                    geometry: GradientGeometry::Linear {
                        direction: LinearDirection::Angle(180.0),
                    },
                    stops: vec![
                        GradientStop {
                            offset: 0.0,
                            color: Color::rgba(0, 0, 0, 255),
                        },
                        GradientStop {
                            offset: 1.0,
                            color: Color::rgba(0, 0, 0, 0),
                        },
                    ],
                }));
            }),
            points: vec![
                ((5, 5), CLEAR),
                ((5, 95), CLEAR),
                ((50, 25), [101, 0, 104, 232]),
                ((50, 75), [94, 0, 107, 19]),
            ],
        },
    ]
}

/// Whether two straight-alpha pixels agree within [`SLACK`]; colour is
/// ignored where both are fully transparent, since it is then undefined.
fn agrees(ours: [u8; 4], chrome: [u8; 4]) -> bool {
    if ours[3] == 0 && chrome[3] == 0 {
        return true;
    }
    ours.iter()
        .zip(chrome.iter())
        .all(|(ours, chrome)| ours.abs_diff(*chrome) <= SLACK)
}

#[test]
fn the_root_background_covers_the_canvas_as_chrome_paints_it() {
    let mut wrong = Vec::new();
    for row in rows() {
        let raw = render(&row.scene, ImageFormat::Raw);
        for ((x, y), chrome) in &row.points {
            let at = ((y * PAGE + x) * 4) as usize;
            let ours = [raw[at], raw[at + 1], raw[at + 2], raw[at + 3]];
            if !agrees(ours, *chrome) {
                wrong.push(format!(
                    "{}: at ({x}, {y}) we paint {ours:?}, Chrome {chrome:?}",
                    row.name
                ));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The control the propagation must not reach: a gradient on a box that is
/// not the root fills that box once, and tiles nowhere.
#[test]
fn a_gradient_below_the_root_does_not_tile() {
    let mut scene = Scene::new(Size::new(100.0, 100.0));
    let mut child = Node::new(NodeKind::Box);
    child.layout.size = (Dimension::Points(60.0), Dimension::Points(60.0));
    child.layout.margin = meo_canvas_scene::Sides::all(Dimension::Points(20.0));
    child.paint.gradient = Some(ramp());
    scene
        .push(NodeId::ROOT, child)
        .unwrap_or_else(|error| unreachable!("{error}"));
    if let Some(root) = scene.get_mut(NodeId::ROOT) {
        root.layout.display = meo_canvas_scene::style::layout::Display::Block;
    }
    let raw = render(&scene, ImageFormat::Raw);
    for (x, y) in [(5, 50), (15, 50), (85, 50), (95, 50), (50, 5)] {
        let at = ((y * PAGE + x) * 4) as usize;
        assert_eq!(
            raw[at + 3],
            0,
            "({x}, {y}) is outside the child, which a non-root gradient never \
             reaches"
        );
    }
    let at = ((50 * PAGE + 25) * 4) as usize;
    assert!(
        raw[at] > 150 && raw[at + 3] == 255,
        "inside the child the ramp still starts red"
    );
}

/// One geometry row: its name, the canvas width, what is done to the root, and
/// the child's inked `(left, top, right, bottom)`.
type Geometry = (
    &'static str,
    f32,
    Box<dyn Fn(&mut Node)>,
    (u32, u32, u32, u32),
);

/// The page root's box, read from the ink of a child at 75% of it: margins
/// inside the canvas as Chrome's `<html>` takes them, and an inset that moves
/// it without resizing it. Chrome's numbers, `getBoundingClientRect` on
/// `<html>`, are the left edge and width; the height is the page's own rule.
#[test]
fn the_page_root_sits_inside_the_canvas_at_its_margins() {
    let scene_of = |width: f32, setup: &dyn Fn(&mut Node)| {
        let mut scene = Scene::new(Size::new(width, 100.0));
        let mut child = Node::new(NodeKind::Box);
        child.layout.size =
            (Dimension::Percent(0.75), Dimension::Percent(0.75));
        child.paint.background_color = BLUE;
        scene
            .push(NodeId::ROOT, child)
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(root) = scene.get_mut(NodeId::ROOT) {
            setup(root);
        }
        scene
    };
    let all = |value| meo_canvas_scene::Sides::all(value);
    let rows: [Geometry; 6] = [
        (
            "no margin, the control",
            100.0,
            Box::new(|_| {}),
            (0, 0, 74, 74),
        ),
        // Chrome: 20,20, 60 wide.
        (
            "margin 20",
            100.0,
            Box::new(move |root| {
                root.layout.margin = all(Dimension::Points(20.0));
            }),
            (20, 20, 64, 64),
        ),
        // Chrome: 0,0, 100 wide -- `auto` is no margin at all.
        (
            "margin auto",
            100.0,
            Box::new(move |root| root.layout.margin = all(Dimension::Auto)),
            (0, 0, 74, 74),
        ),
        // Chrome: -10,-10, 120 wide; 90 of child from -10 inks to 79.
        (
            "margin -10",
            100.0,
            Box::new(move |root| {
                root.layout.margin = all(Dimension::Points(-10.0));
            }),
            (0, 0, 79, 79),
        ),
        // Chrome on 200x100: 20,20, 160 wide -- 10% of the width on both axes,
        // where 10% of the height would put it at 10,10.
        (
            "margin 10% on a wide canvas",
            200.0,
            Box::new(move |root| {
                root.layout.margin = all(Dimension::Percent(0.1));
            }),
            (20, 20, 139, 64),
        ),
        // Chrome: 20,20, 100 wide -- moved, not resized.
        (
            "relative at an inset of 20",
            100.0,
            Box::new(|root| {
                root.layout.position_type = PositionType::Relative;
                root.layout.inset.top = Some(Length::Points(20.0));
                root.layout.inset.left = Some(Length::Points(20.0));
            }),
            (20, 20, 94, 94),
        ),
    ];
    let mut wrong = Vec::new();
    for (name, width, setup, want) in &rows {
        let raw = render(&scene_of(*width, setup.as_ref()), ImageFormat::Raw);
        let side = *width as u32;
        let (mut l, mut t, mut r, mut b) = (side, 100, 0, 0);
        for y in 0..100 {
            for x in 0..side {
                let at = ((y * side + x) * 4) as usize;
                if raw[at..at + 4] == [0, 0, 204, 255] {
                    (l, t, r, b) = (l.min(x), t.min(y), r.max(x), b.max(y));
                }
            }
        }
        if (l, t, r, b) != *want {
            wrong.push(format!(
                "{name}: the child inks {:?}, want {want:?}",
                (l, t, r, b)
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// A stated size is the canvas, margin or not: the root at a 20px margin is
/// placed inside a canvas that stays 60x60.
#[test]
fn a_stated_root_size_stays_the_canvas_size() {
    let mut scene = Scene::new(Size::new(100.0, 100.0));
    if let Some(root) = scene.get_mut(NodeId::ROOT) {
        root.layout.size = (Dimension::Points(60.0), Dimension::Points(60.0));
        root.layout.margin =
            meo_canvas_scene::Sides::all(Dimension::Points(20.0));
    }
    assert_eq!(render(&scene, ImageFormat::Raw).len(), 60 * 60 * 4);
}

/// A page as tall as its content is as tall as the root's margin box, Chrome's
/// `scrollHeight` on a 100x40 viewport over 50px of content: 90 at a 20px
/// margin, and the 40 floor at -10, where the margin box is 30.
#[test]
fn a_content_height_page_counts_the_root_s_margins() {
    for (margin, chrome) in [(20.0, 90), (-10.0, 40), (0.0, 50)] {
        let mut scene = Scene::new(Size::new(100.0, 40.0));
        scene.content_height = true;
        let mut child = Node::new(NodeKind::Box);
        child.layout.size = (Dimension::Points(10.0), Dimension::Points(50.0));
        scene
            .push(NodeId::ROOT, child)
            .unwrap_or_else(|error| unreachable!("{error}"));
        if let Some(root) = scene.get_mut(NodeId::ROOT) {
            root.layout.margin =
                meo_canvas_scene::Sides::all(Dimension::Points(margin));
        }
        let height = render(&scene, ImageFormat::Raw).len() / (100 * 4);
        assert_eq!(
            height, chrome,
            "margin {margin}: the page is {height} tall, Chrome's document {chrome}"
        );
    }
}

/// A gradient tiled 67 times each way, 4000x4000 with the root a 60x60 box at
/// a 1970px margin: the whole canvas, as Chrome 153.0.8010.12 paints it at the
/// far corner and down the left edge.
#[test]
fn a_root_gradient_tiles_the_whole_of_a_large_canvas() {
    const SIDE: u32 = 4000;
    let mut scene = Scene::new(Size::new(4000.0, 4000.0));
    if let Some(root) = scene.get_mut(NodeId::ROOT) {
        root.layout.margin =
            meo_canvas_scene::Sides::all(Dimension::Points(1970.0));
        root.paint.gradient = Some(ramp());
    }
    let raw = render(&scene, ImageFormat::Raw);
    let mut wrong = Vec::new();
    for ((x, y), chrome) in [
        ((5, 5), [151, 0, 53, 255]),
        ((5, 3705), [151, 0, 53, 255]),
        ((5, 3995), [151, 0, 53, 255]),
        ((3995, 3995), [49, 0, 154, 255]),
        ((1975, 1990), [186, 0, 19, 255]),
        ((2000, 2000), [100, 0, 103, 255]),
    ] {
        let at = ((y * SIDE + x) * 4) as usize;
        let ours = [raw[at], raw[at + 1], raw[at + 2], raw[at + 3]];
        if !agrees(ours, chrome) {
            wrong.push(format!(
                "at ({x}, {y}) we paint {ours:?}, Chrome {chrome:?}"
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
