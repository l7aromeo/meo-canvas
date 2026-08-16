# Changelog

Every release, newest first. Generated from the commit messages merged into `main`, `beta`, and
`alpha` — see CONTRIBUTING.md for what a commit message decides.

# [6.0.0](https://github.com/l7aromeo/meo-canvas/compare/v5.0.0...v6.0.0) (2026-08-16)


* fix(build)!: drop the CommonJS build, which never worked ([75fa3a8](https://github.com/l7aromeo/meo-canvas/commit/75fa3a85d82107d0dd5eeb7f4ac5c44f342dd42a))


### Bug Fixes

* **animate:** bound the colour cache, and stop springDuration scanning past rest ([f67c9f6](https://github.com/l7aromeo/meo-canvas/commit/f67c9f6809a682af61722c6064b5d666ff05f4ab))
* **animate:** stop clipping colours to sRGB when blending them ([41f933d](https://github.com/l7aromeo/meo-canvas/commit/41f933df0391dcae765fde314757609810b9bf0c))
* **build:** declare types and package.json in the exports map ([435a776](https://github.com/l7aromeo/meo-canvas/commit/435a77652ad9c608c41a01bdaf4874537c35f833))
* **deps:** update meo-skia-canvas to ^5.1.0 ([a652bed](https://github.com/l7aromeo/meo-canvas/commit/a652bed7a86daf3f0984af2ce8523513a489c977))


### Features

* **animate:** add easing, interpolation, springs and tracks ([0bf9c75](https://github.com/l7aromeo/meo-canvas/commit/0bf9c7552209de38cd66c082f0e52819d56347dd))
* **animate:** add parallel(), and narrow exports on a non-worker canvas ([4617a46](https://github.com/l7aromeo/meo-canvas/commit/4617a464b28f3035eb647de61182cd990ff1d8b9))
* **animate:** add sequence() for multi-step animations ([d2df680](https://github.com/l7aromeo/meo-canvas/commit/d2df680f55d78010eccd2e029773bcc3d1b20a28))
* **Image:** animated sources play, at the rate the source declares ([2c5dca9](https://github.com/l7aromeo/meo-canvas/commit/2c5dca936c6771c44836655ab0628026e8f4298b))
* **Root:** render multi-page and animated output ([382a27d](https://github.com/l7aromeo/meo-canvas/commit/382a27d42fe5716facc4deda59f4329401ea8cc3))


### BREAKING CHANGES

* `main` and the `require` condition are gone. `require()` of
this package resolved before and threw; it now fails to resolve. Use `import`,
or `await import('meo-canvas')` from CommonJS.
