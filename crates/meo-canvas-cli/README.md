# `meo-canvas-cli`

Renders an encoded scene file to an image.

    meo-canvas scene.mcs --format png --output out.png

Build with `--features net` to resolve remote image URLs through a blocking
client. Without it, a URL in a scene is an error, the same as for a Rust caller.
