//! Arbitrary scenes through layout and paint.
//!
//! **The numbers are hostile rather than uniform.** A random `f32` is almost
//! never `NaN`, and `NaN` is what a caller's own arithmetic produces: a width
//! divided by a count that turned out to be zero, a ratio from an empty
//! measurement. So the generator draws from the set that breaks things.
//!
//! A refusal is a fine outcome and a panic is the finding. The seed is in the
//! source so a failure is reproducible.
use meo_canvas_core::Renderer;
use meo_canvas_scene::{
    Length, Scene, Size,
    node::{Node, NodeKind},
    style::{
        layout::{Display, FlexDirection},
        paint::{
            Color, Gradient, GradientGeometry, GradientStop, LinearDirection,
        },
        text::{ParagraphStyle, TextSegment, TextStyle},
    },
};

/// Scenes per run. Rendering is Skia work rather than arithmetic, so this is
/// two thousand rather than the hundred thousand the decoder gets.
const ITERATIONS: usize = 2_000;

/// Fixed, so a failure is reproducible.
const SEED: u64 = 0x9E37_79B9_7F4A_7C15;

/// **The floor that stops this measuring the gradient validator.** Most
/// hostile gradients are refused before the interesting half of paint runs, so
/// a run in which nothing draws is green and worthless. Measured at about two
/// thirds drawing; a quarter is a floor.
const MIN_DREW: usize = ITERATIONS / 4;

struct Rng(u64);

impl Rng {
    const fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    const fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }

    /// A float from the set that breaks things.
    #[expect(
        clippy::cast_precision_loss,
        reason = "the point is an arbitrary bit pattern, not a faithful one"
    )]
    fn hostile(&mut self) -> f32 {
        match self.below(10) {
            0 => f32::NAN,
            1 => f32::INFINITY,
            2 => f32::NEG_INFINITY,
            3 => 0.0,
            4 => -0.0,
            5 => -1.0,
            6 => 1e30,
            7 => -1e30,
            8 => f32::MIN_POSITIVE,
            _ => (self.next() as u32) as f32 / 1e6,
        }
    }

    fn byte(&mut self) -> u8 {
        u8::try_from(self.next() & 0xFF).unwrap_or(0)
    }
}

fn gradient(rng: &mut Rng) -> Gradient {
    // One in eight is hostile, so the validator is exercised without the
    // validator becoming the only thing exercised.
    let stops = if rng.below(8) == 0 {
        (0..rng.below(3))
            .map(|_| GradientStop {
                offset: rng.hostile(),
                color: Color::rgba(255, 0, 0, 255),
            })
            .collect()
    } else {
        vec![
            GradientStop {
                offset: 0.0,
                color: Color::rgba(255, 0, 0, 255),
            },
            GradientStop {
                offset: 1.0,
                color: Color::rgba(0, 0, 255, 128),
            },
        ]
    };
    let geometry = match rng.below(3) {
        0 => GradientGeometry::Linear {
            direction: LinearDirection::Angle(rng.hostile()),
        },
        1 => GradientGeometry::Radial {
            at: (Length::Points(rng.hostile()), Length::Points(rng.hostile())),
        },
        _ => GradientGeometry::Conic {
            at: (Length::Points(rng.hostile()), Length::Points(rng.hostile())),
            from: rng.hostile(),
        },
    };
    Gradient { geometry, stops }
}

fn scene(rng: &mut Rng) -> Scene {
    let mut scene = Scene::new(Size::new(
        rng.hostile().abs().clamp(1.0, 400.0),
        rng.hostile().abs().clamp(1.0, 400.0),
    ));
    let root = scene
        .root()
        .unwrap_or_else(|| unreachable!("a new scene has a root"));

    for _ in 0..=rng.below(12) {
        let mut node = match rng.below(3) {
            // A grapheme cluster, an emoji and a markup-looking string, since
            // the shaper is where text goes wrong.
            0 => Node::text("hi \u{1F600} \u{0301}\u{0301} <b>x</b>"),
            1 => Node::new(NodeKind::Box),
            _ => Node::new(NodeKind::Text {
                // A bidi override and a NUL, which are the characters that
                // reach a shaper differently from the rest.
                segments: vec![TextSegment {
                    text: "\u{202E}\u{0000}x".to_owned(),
                    style: TextStyle::default(),
                }],
                paragraph: ParagraphStyle::default(),
            }),
        };
        node.layout.display = match rng.below(3) {
            0 => Display::Flex,
            1 => Display::Grid,
            _ => Display::Block,
        };
        node.layout.flex_direction = if rng.below(2) == 0 {
            FlexDirection::Row
        } else {
            FlexDirection::Column
        };
        node.layout.gap =
            (Length::Points(rng.hostile()), Length::Points(rng.hostile()));
        node.paint.opacity = rng.hostile();
        node.paint.background_color =
            Color::rgba(rng.byte(), rng.byte(), rng.byte(), rng.byte());
        if rng.below(3) == 0 {
            node.paint.gradient = Some(gradient(rng));
        }
        let _ = scene.push(root, node);
    }
    scene
}

#[test]
fn rendering_an_arbitrary_scene_refuses_rather_than_panics() {
    let renderer = Renderer::new();
    let mut rng = Rng(SEED);
    let (mut drew, mut refused) = (0_usize, 0_usize);

    for _ in 0..ITERATIONS {
        if renderer.render(&scene(&mut rng)).is_ok() {
            drew += 1;
        } else {
            refused += 1;
        }
    }

    assert!(
        drew >= MIN_DREW,
        "only {drew} of {ITERATIONS} scenes reached the end of paint, which is \
         below the {MIN_DREW} that says this is measuring more than the \
         gradient validator; {refused} refused"
    );
}
