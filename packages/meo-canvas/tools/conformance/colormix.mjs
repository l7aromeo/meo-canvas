// How Chrome mixes two colours partway through a CSS animation or transition of
// `background-color`: with the alpha multiplied in, so a transparent end adds no
// colour of its own. Each sample is held at a fixed progress and drawn over a
// white page and a black one, read from screenshot pixels.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'
import { pixel, read } from './png.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/color-mix.tsv')

/** The box: small, read from its middle. */
const BOX = { width: 4, height: 4 }

/** Long enough that the milliseconds a screenshot takes move nothing. */
const DURATION_S = 100

/** The pairs: each end translucent or not, a `transparent`, and an opaque control. */
const PAIRS = [
  ['rgba(255, 0, 0, 0)', 'rgb(255, 0, 0)'],
  ['rgba(255, 0, 0, 0)', 'rgb(0, 0, 255)'],
  ['rgb(255, 0, 0)', 'rgba(0, 0, 255, 0.2)'],
  ['transparent', 'rgb(0, 0, 255)'],
  ['rgb(255, 0, 0)', 'rgb(0, 0, 255)'],
]

/** The progress each pair is held at. */
const TIMES = [0.25, 0.5, 0.75]

/**
 * A page holding `from` to `to` at progress `t`: a paused `@keyframes` with a
 * negative delay, or a `transition` started and then paused and seeked through
 * the animation the page itself created.
 */
async function hold(page, kind, from, to, t, background) {
  const base = `html,body{margin:0;background:${background}}#m{position:absolute;inset:0}`
  if (kind === 'animation') {
    await page.setContent(
      `<!doctype html><style>${base}@keyframes k{from{background-color:${from}}to{background-color:${to}}}#m{animation:k ${DURATION_S}s linear ${-t * DURATION_S}s 1 paused both}</style><div id="m"></div>`,
    )
    return
  }
  await page.setContent(`<!doctype html><style>${base}#m{background-color:${from};transition:background-color ${DURATION_S}s linear}</style><div id="m"></div>`)
  await page.evaluate(
    ({ to, at }) => {
      const element = document.getElementById('m')
      if (element === null) throw new Error('no #m')
      // Read once so the starting colour is the computed one the transition
      // leaves from, then change it.
      void getComputedStyle(element).backgroundColor
      element.style.backgroundColor = to
      const [transition] = element.getAnimations()
      if (transition === undefined) throw new Error('the change started no transition')
      transition.pause()
      transition.currentTime = at
    },
    { to, at: t * DURATION_S * 1000 },
  )
}

const browser = await open()
try {
  await browser.page.setViewportSize(BOX)
  const rows = []
  for (const kind of ['animation', 'transition']) {
    for (const [from, to] of PAIRS) {
      for (const t of TIMES) {
        const over = []
        for (const background of ['#ffffff', '#000000']) {
          await hold(browser.page, kind, from, to, t, background)
          await settle(browser.page)
          const shot = read(await browser.page.screenshot({ clip: { x: 0, y: 0, ...BOX } }))
          over.push(pixel(shot, 2, 2).slice(0, 3))
        }
        rows.push([kind, JSON.stringify(from), JSON.stringify(to), t, ...over.flat()].join('\t'))
      }
    }
  }

  const written = table([
    '# Chrome, through `just conformance`. Two colours mixed partway through an animation.',
    '# Written by packages/meo-canvas/tools/conformance/colormix.mjs.',
    '#',
    '# Each row is `background-color` held at progress `t` between `from` and `to`, linear,',
    '# as a paused `@keyframes` with a negative delay or as a `transition` paused and seeked,',
    '# drawn over a white page and over a black one and read from screenshot pixels. Chrome',
    '# mixes with the alpha multiplied in (CSS Color 4, "Interpolating with Alpha"), so a',
    '# transparent end adds no colour: `rgba(255, 0, 0, 0)` to blue is blue fading in.',
    '#',
    '# kind\tfrom\tto\tt\twhite r\tg\tb\tblack r\tg\tb',
    ...rows,
  ])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`colour mix: ${rows.length} samples -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
