// The box Chrome gives an `<img>` with no `alt` whose source fails (`l7aromeo/meo-canvas#183`):
// a local server answers HTTP 522, and the image sits in a block, flex, grid or absolute
// container with no size, a width, a height or both. Read as rectangles from
// `getBoundingClientRect`, beside a loaded 64x32 image in the same cells as a control.
import { createServer } from 'node:http'
import { writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { open, table } from './browser.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const DESTINATION = resolve(HERE, '../../../../crates/meo-canvas/tests/assets/chrome/failed-image.tsv')

/** The container's width, and a grid's and an absolute container's height. */
const CONTAINER = { width: 300, height: 200 }

/** The loaded control's intrinsic size, an SVG the server sends with a 200. */
const LOADED = { width: 64, height: 32 }

/** The status the failing source answers, a CDN's origin timeout. */
const STATUS = 522

/**
 * The containers, by the name the Rust and npm tests read. `item` is what the image
 * itself carries there: absolutely positioned, and between insets for one.
 */
const CONTAINERS = {
  block: { container: 'display:block;', item: 'display:block;' },
  'flex column': { container: 'display:flex;flex-direction:column;align-items:flex-start;', item: '' },
  'flex column stretch': { container: 'display:flex;flex-direction:column;', item: '' },
  'flex row': { container: 'display:flex;flex-direction:row;align-items:flex-start;', item: '' },
  grid: { container: `display:grid;grid-template-columns:${CONTAINER.width}px;height:${CONTAINER.height}px;`, item: '' },
  absolute: { container: `position:relative;height:${CONTAINER.height}px;`, item: 'position:absolute;' },
  'absolute insets': { container: `position:relative;height:${CONTAINER.height}px;`, item: 'position:absolute;left:10px;right:10px;' },
}

/** The sizes stated on the image, as CSS; `-` for an axis left `auto`. */
const SIZES = [
  ['-', '-'],
  ['100', '-'],
  ['-', '60'],
  ['80', '40'],
]

const server = createServer((request, response) => {
  if (request.url === '/loaded.svg') {
    response.writeHead(200, { 'content-type': 'image/svg+xml' })
    response.end(`<svg xmlns="http://www.w3.org/2000/svg" width="${LOADED.width}" height="${LOADED.height}"/>`)
    return
  }
  response.writeHead(STATUS, { 'content-type': 'text/html' })
  response.end(`<html><body>${STATUS}</body></html>`)
}).listen(0, '127.0.0.1')
await new Promise(ready => server.once('listening', ready))
const origin = `http://127.0.0.1:${server.address().port}`

const browser = await open()
try {
  const rows = []
  for (const [source, path] of [
    ['failed', '/failed.png'],
    ['loaded', '/loaded.svg'],
  ]) {
    for (const [name, { container, item }] of Object.entries(CONTAINERS)) {
      for (const [width, height] of SIZES) {
        const [w, h] = await browser.page.evaluate(
          async ({ wide, container, item, width, height, src }) => {
            document.body.innerHTML = ''
            const box = document.createElement('div')
            box.style.cssText = `width:${wide}px;${container}`
            const image = document.createElement('img')
            const sized = (width === '-' ? '' : `width:${width}px;`) + (height === '-' ? '' : `height:${height}px;`)
            image.style.cssText = item + sized
            box.append(image)
            document.body.append(box)
            await new Promise(settled => {
              image.onload = settled
              image.onerror = settled
              image.src = src
            })
            const rect = image.getBoundingClientRect()
            return [rect.width, rect.height]
          },
          { wide: CONTAINER.width, container, item, width, height, src: origin + path },
        )
        rows.push([source, name, width, height, w.toFixed(3), h.toFixed(3)].join('\t'))
      }
    }
  }

  const header = [
    '# Chrome, through `just conformance`: the box of an <img> with no alt whose source fails, by container and size.',
    `# A ${CONTAINER.width}-wide container; grid (one ${CONTAINER.width}px column) and absolute ones are ${CONTAINER.height} tall.`,
    `# \`failed\` answers HTTP ${STATUS}; \`loaded\` is a ${LOADED.width}x${LOADED.height} SVG, the control. width and height are CSS px, \`-\` for auto.`,
    '# Written by packages/meo-canvas/tools/conformance/failedimage.mjs.',
    '#',
    '# w and h are the image from getBoundingClientRect.',
    '#',
    '# source\tcontainer\twidth\theight\tw\th',
  ]
  const written = table([...header, ...rows])
  if (process.env['WRITE'] === '1') {
    await writeFile(DESTINATION, written, 'utf8')
    process.stderr.write(`failed image: ${rows.length} rows -> ${DESTINATION}\n`)
  } else {
    process.stdout.write(written)
  }
} finally {
  await browser.close()
  server.close()
}
