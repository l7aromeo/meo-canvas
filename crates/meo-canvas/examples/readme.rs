//! The example the crate's README carries, compiled so it cannot rot.
//!
//! **A README example is the "docs that quote output" trap with nothing
//! gating it.** The version this replaced came from `AGENTS.md` and had four
//! faults: `Root::new` takes one argument, `Styled` has to be imported for the
//! setters, `to_file` takes only a path, and there was no renderer. It read
//! correctly and compiled nowhere.
//!
//! `cargo clippy --workspace --all-targets` builds this, so the README's
//! example is gated by the same run that gates everything else. Keep the two
//! identical: a divergence of one line is how it starts rotting again.
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
