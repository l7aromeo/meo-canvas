### Changed

- A chart given a negative value throws `[canvas] a chart cannot draw a negative value (got N)`; the message no longer compares the refusal with the previous major version.

### Fixed

- An image URL whose host does not resolve is reported in `canvas.warnings` as `failure: 'host-not-found'` (do not retry) rather than `'transport'` (retry). A lookup that could not finish, because the resolver was unreachable or said to try again, stays `'transport'`.
