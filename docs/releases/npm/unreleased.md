### Changed

- A chart given a negative value throws `[canvas] a chart cannot draw a negative value (got N)`; the message no longer compares the refusal with the previous major version.
- A `frame` past the last frame of an animated image is refused with `node N asks for frame F of an image with M frames`, naming the index and the frame count, rather than as an image whose bytes are in no format this decodes.

### Fixed

- An image URL whose host does not resolve is reported in `canvas.warnings` as `failure: 'host-not-found'` (do not retry) rather than `'transport'` (retry). A lookup that could not finish, because the resolver was unreachable or said to try again, stays `'transport'`.
- An SVG image given `frame: 1` or later draws, as a still raster does, rather than being refused.
