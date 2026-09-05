// Whether Chrome clips a picture to the element it was placed in.
//
// **`objectfit.mjs` cannot answer this, and could not have.** It builds its
// cell as `overflow:hidden`, sets the viewport to the box, and clips the
// screenshot to the box as well — three separate reasons a pixel outside the
// element can never reach the measurement. So its table says where each rule
// puts the picture *given* a clip, and is silent on whether Chrome applies one.
// It is green and always would have been.
//
// This asks the other half: the element sits on a page larger than itself, with
// no `overflow` declared anywhere, and the shot covers the whole page. Anything
// outside the element's box is ink Chrome chose to paint there.
//
// Two rules can exceed their box and both are measured. `cover` scales by
// `max(sx, sy)`, so a source whose aspect differs from the box overflows on one
// axis. `none` draws at intrinsic size, so any source larger than its box
// overflows on both. Nobody has reported `none`, which is why it is here:
// the report is a sample and the class is "fits that can exceed".
//
// `contain` is the control. It scales by `min(sx, sy)` and cannot exceed, so a
// run that finds ink outside the box for `contain` is measuring something other
// than overflow — a stray margin, a scrollbar, a background that is not the
// colour this expects — and the table would be evidence about the harness.
//
// The source is generated in the page rather than read from `fit-marks.png`,
// which is 8x4: `none` needs a source larger than its box and `cover` needs an
// aspect that does not match it, and one asset cannot be both without boxes so
// small the answer is a rounding argument.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'
import { pixel, read } from './png.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/object-fit-overflow.tsv')

/** The page, larger than any box, so there is somewhere for spill to land. */
const PAGE = { width: 240, height: 240 }

/** Where the element's own box sits, with room on every side. */
const ORIGIN = { x: 80, y: 80 }

/** The page colour. Any pixel that is not this is ink. */
const PAPER = [0, 255, 0]

/** The picture's colour, which is neither the paper nor anything near it. */
const INK = '#e828c8'

const CASES = [
  // `cover` on a box whose aspect does not match the source: scale is
  // `max(40/40, 30/10) = 3`, so the picture is 120x30 in a 40x30 box and
  // overflows 40 pixels on each side.
  { fit: 'cover', box: { width: 40, height: 30 }, source: { width: 40, height: 10 } },
  // `none` at intrinsic size in a smaller box: 60x60 in 20x20, overflowing 20
  // on every side.
  { fit: 'none', box: { width: 20, height: 20 }, source: { width: 60, height: 60 } },
  // The control. `contain` scales by `min(sx, sy)` and cannot exceed its box.
  { fit: 'contain', box: { width: 40, height: 30 }, source: { width: 40, height: 10 } },
  // **The positive control, and the row that makes the other three mean
  // something.** Three `inside` verdicts are exactly what a harness that cannot
  // see outside the box would print, so one case forces `overflow:visible` and
  // must report `spills`. It also names the mechanism: the clip is the UA
  // stylesheet's `overflow: clip` on the replaced element, and it is
  // overridable -- which is why this row can exist at all.
  { fit: 'cover', box: { width: 40, height: 30 }, source: { width: 40, height: 10 }, overflow: 'visible' },
  // **The symptom users actually report.** Not "my image overflows its box" but
  // "my rounded image renders square": the element's radius is honoured, and
  // then the overflowing picture paints over the corners it cut. So the
  // interesting pixel is the box's own rectangular corner, which sits outside a
  // 14px curve -- paper there means the picture was clipped to the shape rather
  // than to its bounding rectangle.
  { fit: 'cover', box: { width: 60, height: 40 }, source: { width: 60, height: 10 }, radius: 14 },
  // **The case that separates two defects sharing one symptom, and it looks
  // trivially redundant.** A square source in a square box under `contain`
  // scales by `min(sx, sy)` with both equal, so the picture fills the box
  // exactly and nothing overflows. That is the point: every other case here
  // uses a fit that overflows or a source that letterboxes, and neither can
  // tell "clipped to the radius" from "clipped to the box" -- both mechanisms
  // predict the same pixels. Only a picture that reaches the corners without
  // exceeding them makes them disagree.
  //
  // Three people made four attempts on `l7aromeo/meo-canvas#37` and a fifth
  // hypothesis was written here, all of them missing it. The instruments were
  // fine; **the cases were not discriminating.**
  { fit: 'contain', box: { width: 80, height: 80 }, source: { width: 40, height: 40 }, radius: 20 },
  // **Where replaced content sits inside a decorated element.** CSS puts it in
  // the content box, which is inside the border *and* the padding. Both of
  // these inset it equally and the two add, which is the part worth measuring
  // rather than assuming: a fix built from the border case alone is right for
  // half the inputs and silently wrong for the other half.
  { fit: 'cover', box: { width: 80, height: 80 }, source: { width: 80, height: 20 }, border: 8 },
  // **The padding row carries a background and that is load-bearing.** A
  // padding band is only visible if something paints it, and for `padding` the
  // element's own `background` is what does -- a border has its own colour, a
  // padding band does not. The walker's scene sets the same background for the
  // same reason. Drop either and that row measures the element on one side and
  // the picture alone on the other, which reads as a placement defect and is
  // not one.
  { fit: 'cover', box: { width: 80, height: 80 }, source: { width: 80, height: 20 }, padding: 8 },
  { fit: 'cover', box: { width: 80, height: 80 }, source: { width: 80, height: 20 }, border: 8, padding: 8 },
  // And the corner the content box follows, which is tighter than the box's
  // own by the inset it sits inside.
  { fit: 'cover', box: { width: 80, height: 80 }, source: { width: 80, height: 20 }, border: 8, radius: 20 },
]

const browser = await open()
try {
  const rows = []
  await browser.page.setViewportSize(PAGE)

  for (const { fit, box, source, overflow, radius, border, padding } of CASES) {
    const computed = await browser.page.evaluate(
      ({ page, origin, box, source, fit, ink, paper, overflow, radius, border, padding }) => {
        document.body.innerHTML = ''
        document.body.style.cssText = `margin:0;width:${page.width}px;height:${page.height}px;background:rgb(${paper.join(',')});`

        // Drawn here rather than loaded, so each case gets the intrinsic size
        // its question needs. A flat rectangle is enough: this measures extent,
        // not which part of the picture survived.
        const canvas = document.createElement('canvas')
        canvas.width = source.width
        canvas.height = source.height
        const context = canvas.getContext('2d')
        context.fillStyle = ink
        context.fillRect(0, 0, source.width, source.height)

        const image = document.createElement('img')
        // No `overflow` on this element and none on any ancestor. That is the
        // whole point: whatever clipping happens is Chrome's own.
        image.style.cssText = `position:absolute;left:${origin.x}px;top:${origin.y}px;width:${box.width}px;height:${box.height}px;object-fit:${fit};image-rendering:pixelated;${overflow ? `overflow:${overflow};` : ''}${radius ? `border-radius:${radius}px;` : ''}${border ? `border:${border}px solid #0000ff;` : ''}${padding ? `padding:${padding}px;background:#0000ff;` : ''}box-sizing:border-box;`
        image.src = canvas.toDataURL('image/png')
        document.body.append(image)

        // What the UA stylesheet says, which is the mechanism behind whatever
        // the pixels show. Reported beside them so the two can be read
        // together rather than one being inferred from the other.
        return getComputedStyle(image).overflow
      },
      { page: PAGE, origin: ORIGIN, box, source, fit, ink: INK, paper: PAPER, overflow, radius, border, padding },
    )
    await settle(browser.page)

    const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...PAGE } }))
    let painted = null
    let ink_pixels = 0
    for (let y = 0; y < PAGE.height; y += 1) {
      for (let x = 0; x < PAGE.width; x += 1) {
        const [r, g, b] = pixel(shot, x, y)
        if (r === PAPER[0] && g === PAPER[1] && b === PAPER[2]) continue
        if (r === 232 && g === 40 && b === 200) ink_pixels += 1
        painted = painted === null ? [x, y, x, y] : [Math.min(painted[0], x), Math.min(painted[1], y), Math.max(painted[2], x), Math.max(painted[3], y)]
      }
    }

    // The box's own rectangular corner. Inside a radius it is outside the
    // rounded shape, so the paper colour there means the curve survived.
    const [cr, cg, cb] = pixel(shot, ORIGIN.x, ORIGIN.y)
    const corner = cr === PAPER[0] && cg === PAPER[1] && cb === PAPER[2] ? 'paper' : 'ink'
    const rect = painted === null ? 'absent' : `${painted[0]},${painted[1]},${painted[2] - painted[0] + 1},${painted[3] - painted[1] + 1}`
    const boxRect = `${ORIGIN.x},${ORIGIN.y},${box.width},${box.height}`
    const clipped =
      painted === null
        ? 'none-drawn'
        : painted[0] >= ORIGIN.x && painted[1] >= ORIGIN.y && painted[2] <= ORIGIN.x + box.width - 1 && painted[3] <= ORIGIN.y + box.height - 1
          ? 'inside'
          : 'spills'
    rows.push(
      [
        fit,
        `${box.width}x${box.height}`,
        `${source.width}x${source.height}`,
        boxRect,
        rect,
        clipped,
        computed,
        radius ?? 0,
        corner,
        border ?? 0,
        padding ?? 0,
        painted === null ? 0 : ink_pixels,
      ].join('\t'),
    )
  }

  const header = [
    '# Chrome, through `just conformance`. Whether a picture stays inside its element.',
    '#',
    '# `object-fit.tsv` cannot answer this: its cell is `overflow:hidden`, its',
    '# viewport is the box, and its screenshot is clipped to the box. Three reasons',
    '# a pixel outside the element cannot reach it, so it measures placement given a',
    '# clip and is silent about whether Chrome applies one.',
    '#',
    `# Here the element sits at ${ORIGIN.x},${ORIGIN.y} on a ${PAGE.width}x${PAGE.height} page with no`,
    '# `overflow` declared on it or on any ancestor, and the shot covers the page.',
    '#',
    '# `painted` is the bounding box of every pixel that is not the paper colour.',
    '# `verdict` compares it against `box`: `inside` means Chrome clipped, `spills`',
    '# means it did not. `contain` is the control -- it cannot exceed its box, so a',
    '# `spills` there is evidence about this harness rather than about Chrome.',
    '#',
    "# `overflow` is the element's computed value, which is the mechanism behind",
    '# whatever the pixels show. The last row forces it to `visible` and must read',
    '# `spills`: without a row that does, three `inside` verdicts are also what a',
    '# harness blind to everything outside the box would print.',
    '#',
    "# `corner` reads the box's own rectangular corner: `paper` means a radius",
    '# cut it and the picture did not paint back over it, which is the symptom',
    '# users report -- "my rounded image renders square" rather than "my image',
    '# overflows its box".',
    '#',
    '# `border` and `padding` are what the element carries; `pixels` counts the',
    '# picture alone, which is what reads a curve the bounding box cannot.',
    '#',
    '# fit\tbox\tsource\tbox-rect\tpainted\tverdict\toverflow\tradius\tcorner\tborder\tpadding\tpixels',
  ]
  await writeFile(DESTINATION, table([...header, ...rows]), 'utf8')
  process.stderr.write(`object fit overflow: ${rows.length} cases -> ${DESTINATION}\n`)
} finally {
  await browser.close()
}
