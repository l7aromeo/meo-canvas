//! The example the crate's README carries, compiled by `cargo clippy
//! --workspace --all-targets` so it cannot rot. Keep the two identical.
use meo_canvas::{Renderer, Root, Row, Styled, Text, hex, px};

fn main() -> Result<(), meo_canvas::BuildError> {
    let renderer = Renderer::new();

    let mut canvas = Root::new(520.0)
        .height(180.0)
        .background_color(hex("#101014"))
        .children(
            Row::new()
                .gap(px(20.0))
                .padding(px(24.0))
                .children(Text::new("Ukasyah").font_size(26.0).bold()),
        )
        .render(&renderer)?;

    canvas.to_file("out.png")?;
    Ok(())
}
