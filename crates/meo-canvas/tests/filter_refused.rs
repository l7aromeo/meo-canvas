//! A `filter` or `backdrop_filter` chain `meo-skia-canvas` refuses is dropped,
//! as Chrome drops an invalid declaration, and reported as a diagnostic: the
//! render goes on, drawn as though the property were not set.

use meo_canvas::{
    Box, Display, Element, Format, PositionType, Renderer, Root, Styled,
    hex_rgb, px, sides,
};

/// Chains meo-skia-canvas 0.17 refuses as Chrome 153 does: a space before a
/// unit, a negative blur, and a blank string, which 0.16 took as no filter.
const REFUSED: [&str; 3] = ["blur(4 px)", "blur(-1px)", ""];

fn page(child: Element) -> Root {
    Root::new(60.0)
        .height(40.0)
        .display(Display::Block)
        .background_color(hex_rgb(0xff_ff_ff))
        .children(child)
}

/// A red box with a blue child at `z_index: -1`.
fn parent() -> Element {
    Box::new()
        .position_type(PositionType::Relative)
        .size(px(40.0), px(30.0))
        .background_color(hex_rgb(0xdc_28_28))
        .children(
            Box::new()
                .position_type(PositionType::Absolute)
                .position(sides(Some(px(5.0)), None, None, Some(px(5.0))))
                .size(px(20.0), px(20.0))
                .background_color(hex_rgb(0x28_50_dc))
                .z_index(-1),
        )
}

fn drawn(root: Root) -> (Vec<u8>, Vec<(String, String)>) {
    let mut renderer = Renderer::new();
    renderer.set_gpu(false);
    let mut canvas = root
        .render(&renderer)
        .unwrap_or_else(|error| unreachable!("the render failed: {error}"));
    let found = canvas
        .diagnostics()
        .iter()
        .map(|found| (found.path.clone(), found.detail.clone()))
        .collect();
    let pixels = canvas
        .to_buffer(Format::Raw)
        .unwrap_or_else(|error| unreachable!("{error}"));
    (pixels, found)
}

#[test]
fn a_refused_filter_is_dropped_and_reported() {
    let (plain, none) = drawn(page(parent()));
    assert!(none.is_empty(), "{none:?}");
    for chain in REFUSED {
        let (pixels, found) = drawn(page(parent().filter(chain)));
        assert_eq!(found.len(), 1, "{chain:?}: {found:?}");
        assert_eq!(found[0].0, "filter", "{chain:?}");
        assert!(found[0].1.contains(&format!("{chain:?}")), "{found:?}");
        // Drawn as no filter draws: no stacking context either, so the child
        // at `z_index: -1` stays under its parent's red, as Chrome hoists it.
        assert!(pixels == plain, "{chain:?} drew differently from no filter");
    }
}

#[test]
fn a_drawable_filter_is_kept() {
    let (plain, _) = drawn(page(parent()));
    let (pixels, found) = drawn(page(parent().filter("blur(2px)")));
    assert!(found.is_empty(), "{found:?}");
    assert!(pixels != plain, "blur(2px) drew nothing");
}

#[test]
fn a_refused_backdrop_filter_is_dropped_and_reported() {
    let pane = |chain: Option<&str>| {
        let pane = Box::new().size(px(40.0), px(30.0));
        page(match chain {
            Some(chain) => pane.backdrop_filter(chain),
            None => pane,
        })
    };
    let (plain, _) = drawn(pane(None));
    for chain in REFUSED {
        let (pixels, found) = drawn(pane(Some(chain)));
        assert_eq!(found.len(), 1, "{chain:?}: {found:?}");
        assert_eq!(found[0].0, "backdrop-filter", "{chain:?}");
        assert!(pixels == plain, "{chain:?} drew differently from none");
    }
}
