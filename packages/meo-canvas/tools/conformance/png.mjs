// A PNG reader, because a screenshot is the only way to ask Chrome what it
// actually painted.
//
// Eight-bit RGB or RGBA, no interlacing — which is what Playwright writes.
// Node's own `zlib` does the decompression, so this needs no dependency: the
// rest is the filter loop from the specification, and refusing anything it
// does not understand is what keeps it short enough to be obviously right.

import { inflateSync } from 'node:zlib'

/** The pixels of a PNG, with its size and how many channels each pixel has. */
export function read(bytes) {
  if (bytes.subarray(0, 8).toString('binary') !== '\x89PNG\r\n\x1a\n') {
    throw new TypeError('not a PNG')
  }

  let at = 8
  let width = 0
  let height = 0
  let channels = 0
  const parts = []

  while (at < bytes.length) {
    const length = bytes.readUInt32BE(at)
    const kind = bytes.subarray(at + 4, at + 8).toString('binary')
    const body = bytes.subarray(at + 8, at + 8 + length)
    at += length + 12

    if (kind === 'IHDR') {
      width = body.readUInt32BE(0)
      height = body.readUInt32BE(4)
      const depth = body[8]
      const colour = body[9]
      const interlace = body[12]
      if (depth !== 8 || interlace !== 0 || (colour !== 2 && colour !== 6)) {
        throw new TypeError(`this reader takes 8-bit RGB or RGBA without interlacing, not depth ${depth} colour ${colour} interlace ${interlace}`)
      }
      channels = colour === 2 ? 3 : 4
    } else if (kind === 'IDAT') {
      parts.push(body)
    } else if (kind === 'IEND') {
      break
    }
  }

  const raw = inflateSync(Buffer.concat(parts))
  const stride = width * channels
  const out = Buffer.alloc(height * stride)
  let previous = Buffer.alloc(stride)
  let read = 0

  for (let y = 0; y < height; y += 1) {
    const filter = raw[read]
    read += 1
    const line = Buffer.from(raw.subarray(read, read + stride))
    read += stride

    for (let i = 0; i < stride; i += 1) {
      const a = i >= channels ? line[i - channels] : 0
      const b = previous[i]
      const c = i >= channels ? previous[i - channels] : 0
      if (filter === 1) line[i] = (line[i] + a) & 0xff
      else if (filter === 2) line[i] = (line[i] + b) & 0xff
      else if (filter === 3) line[i] = (line[i] + ((a + b) >> 1)) & 0xff
      else if (filter === 4) {
        const p = a + b - c
        const pa = Math.abs(p - a)
        const pb = Math.abs(p - b)
        const pc = Math.abs(p - c)
        line[i] = (line[i] + (pa <= pb && pa <= pc ? a : pb <= pc ? b : c)) & 0xff
      }
    }

    line.copy(out, y * stride)
    previous = line
  }

  return { width, height, channels, bytes: out }
}

/** The colour at a point, as `[r, g, b]`. */
export function pixel(image, x, y) {
  const at = (y * image.width + x) * image.channels
  return [image.bytes[at], image.bytes[at + 1], image.bytes[at + 2]]
}
