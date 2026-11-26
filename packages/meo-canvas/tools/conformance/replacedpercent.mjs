// How wide a shrink-to-fit block grows around an image sized in percent: CSS
// resolves a percentage `width` or `max-width` against zero for a replaced
// element's min-content contribution, so the block shrinks below the picture's
// natural width when there is less room. Read from both boxes.

import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, settle, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/replaced-percentage.tsv')

/** The art: 80x40, intrinsic size declared on the SVG root. */
const ART = { width: 80, height: 40 }
const SRC = `data:image/svg+xml,${encodeURIComponent(
  `<svg xmlns="http://www.w3.org/2000/svg" width="${ART.width}" height="${ART.height}"><rect width="100%" height="100%" fill="#c00"/></svg>`,
)}`

/**
 * The shrink-to-fit block around the image, by how it is made one: placed
 * absolutely with `width: auto`, a flex item, or the item in an `auto` grid track.
 */
const WRAPPERS = ['absolute', 'flex-item', 'grid-auto']

/** The image's own sizing: `none` is the control, its natural size. */
const SIZINGS = {
  none: '',
  'width-100': 'width:100%;',
  'max-width-100': 'max-width:100%;',
  'width-50': 'width:50%;',
}

/** The room the wrapper has: less than the art's width, and more. */
const ROOMS = [40, 200]

const browser = await open()
try {
  const rows = []
  for (const wrapper of WRAPPERS) {
    for (const [sizing, css] of Object.entries(SIZINGS)) {
      for (const room of ROOMS) {
        await browser.page.evaluate(
          ({ src, wrapper, css, room }) => {
            document.body.innerHTML = ''
            const root = document.createElement('div')
            const layout = { absolute: 'position:relative;', 'flex-item': 'display:flex;', 'grid-auto': 'display:grid;grid-template-columns:auto;' }
            root.style.cssText = `position:absolute;left:0;top:0;width:${room}px;height:100px;${layout[wrapper]}`
            const block = document.createElement('div')
            block.id = 'block'
            block.style.cssText = wrapper === 'absolute' ? 'position:absolute;left:0;top:0;' : ''
            block.style.cssText += 'align-self:start;justify-self:start;'
            const image = document.createElement('img')
            image.id = 'image'
            image.src = src
            image.style.cssText = `display:block;${css}`
            block.append(image)
            root.append(block)
            document.body.append(root)
          },
          { src: SRC, wrapper, css, room },
        )
        await settle(browser.page)
        const [block, image] = await browser.page.evaluate(() =>
          ['block', 'image'].map(id => {
            const { left, top, width, height } = document.getElementById(id).getBoundingClientRect()
            return [left, top, width, height]
          }),
        )
        rows.push([wrapper, sizing, room, ...block, ...image].join('\t'))
      }
    }
  }

  const header = [
    '# Chrome, through `just conformance`. A shrink-to-fit block around an image sized in percent.',
    '#',
    `# The art is ${ART.width}x${ART.height}, a \`display: block\` image with the sizing named. Its block is`,
    '# shrink-to-fit: `absolute` with `width: auto` in a positioned root, a `flex-item`,',
    '# or the item in a `grid-auto` column, at the start on both axes. The root is `room`',
    '# wide and 100 tall at the origin. Both rectangles are border boxes from `getBoundingClientRect`.',
    '# Written by packages/meo-canvas/tools/conformance/replacedpercent.mjs.',
    '#',
    '# wrapper\tsizing\troom\tblock-x\tblock-y\tblock-w\tblock-h\timage-x\timage-y\timage-w\timage-h',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`replaced percentage: ${rows.length} rows -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
}
