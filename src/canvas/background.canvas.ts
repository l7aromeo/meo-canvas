import type { CanvasRenderingContext2D, Image as CanvasImage } from 'meo-skia-canvas'
import type { BoxProps } from '@/canvas/canvas.type.js'
import { drawRoundedRectPath } from '@/canvas/canvas.helper.js'

/** The node's box, which a background picture is placed and tiled against. */
interface BackgroundBox {
  x: number
  y: number
  width: number
  height: number
}

/** The size one tile is drawn at, once the picture's own size and the box are both known. */
interface TileSize {
  width: number
  height: number
}

type BackgroundImage = NonNullable<BoxProps['backgroundImage']>

/**
 * How big one tile is drawn.
 *
 * A number is a width, with the height following the picture's own proportions — the rule CSS uses
 * when `background-size` names one length. `cover` and `contain` scale to the box the way an
 * image's `objectFit` does.
 */
function tileSize(image: CanvasImage, size: BackgroundImage['size'], box: BackgroundBox): TileSize {
  const natural = { width: image.width, height: image.height }
  if (natural.width <= 0 || natural.height <= 0) return natural

  const ratio = natural.width / natural.height

  if (size === undefined) return natural
  if (typeof size === 'number') return { width: size, height: size / ratio }

  if (size === 'cover' || size === 'contain') {
    const boxRatio = box.width / box.height
    const matchWidth = size === 'cover' ? ratio < boxRatio : ratio > boxRatio
    return matchWidth ? { width: box.width, height: box.width / ratio } : { width: box.height * ratio, height: box.height }
  }

  const width = resolveLength(size.width, box.width)
  const height = resolveLength(size.height, box.height)

  // One edge given, the other follows the picture — again CSS's rule for a single length.
  if (width !== undefined && height === undefined) return { width, height: width / ratio }
  if (height !== undefined && width === undefined) return { width: height * ratio, height }
  return { width: width ?? natural.width, height: height ?? natural.height }
}

/** A length that may be a percentage of the edge it lies along. */
function resolveLength(value: number | `${number}%` | undefined, extent: number): number | undefined {
  if (value === undefined) return undefined
  return typeof value === 'string' ? (parseFloat(value) / 100) * extent : value
}

/**
 * Where the first tile's top-left corner sits.
 *
 * A percentage is not a distance from the edge but a share of the slack: CSS lines up the same
 * fraction of the picture with that fraction of the box, so `'100%'` puts the picture's far edge
 * against the box's far edge instead of pushing it out by a full width.
 */
function tileOrigin(value: number | `${number}%` | undefined, extent: number, tile: number): number {
  if (value === undefined) return 0
  if (typeof value === 'string') return ((parseFloat(value) / 100) * (extent - tile)) | 0
  return value
}

/**
 * Paints a node's background picture across its box.
 *
 * Tiling is done here rather than through a canvas pattern because CSS asks for more than a
 * pattern offers: `space` distributes the gaps between whole tiles and `round` stretches them to
 * fit a whole number, neither of which a repeating fill can express. Drawing the tiles directly
 * also keeps the placement rules — the size, the origin — in one place for every repeat mode.
 */
export function paintBackgroundImage(
  ctx: CanvasRenderingContext2D,
  image: CanvasImage,
  background: BackgroundImage,
  box: BackgroundBox,
  radii: { TopLeft: number; TopRight: number; BottomRight: number; BottomLeft: number },
): void {
  const size = tileSize(image, background.size, box)
  if (size.width <= 0 || size.height <= 0 || box.width <= 0 || box.height <= 0) return

  const repeat = background.repeat ?? 'repeat'
  const repeatsAcross = repeat === 'repeat' || repeat === 'repeat-x' || repeat === 'space' || repeat === 'round'
  const repeatsDown = repeat === 'repeat' || repeat === 'repeat-y' || repeat === 'space' || repeat === 'round'

  const across = layTilesOut(box.width, size.width, repeat, repeatsAcross, tileOrigin(background.position?.x, box.width, size.width))
  const down = layTilesOut(box.height, size.height, repeat, repeatsDown, tileOrigin(background.position?.y, box.height, size.height))

  ctx.save()
  try {
    // Clipped to the box, corners included: a background stops where its node does.
    drawRoundedRectPath(ctx, box.x, box.y, box.width, box.height, radii)
    ctx.clip()

    for (const left of across.offsets) {
      for (const top of down.offsets) {
        ctx.drawImage(image, box.x + left, box.y + top, across.extent, down.extent)
      }
    }
  } finally {
    ctx.restore()
  }
}

/**
 * Where the tiles go along one axis, and how long each is.
 *
 * `space` fits whole tiles and shares the remainder out as equal gaps between them, pinning the
 * first and last to the edges — so it ignores the origin, as CSS does. `round` scales the tile
 * instead, so a whole number of them fills the axis exactly. Every other mode leaves the tile at
 * its own length and steps by it.
 */
function layTilesOut(extent: number, tile: number, repeat: BackgroundImage['repeat'], repeats: boolean, origin: number): { offsets: number[]; extent: number } {
  if (!repeats) return { offsets: [origin], extent: tile }

  if (repeat === 'round') {
    const count = Math.max(1, Math.round(extent / tile))
    const rounded = extent / count
    return { offsets: Array.from({ length: count }, (_, index) => index * rounded), extent: rounded }
  }

  if (repeat === 'space') {
    const count = Math.floor(extent / tile)
    if (count <= 1) return { offsets: [0], extent: tile }
    const gap = (extent - count * tile) / (count - 1)
    return { offsets: Array.from({ length: count }, (_, index) => index * (tile + gap)), extent: tile }
  }

  // Start at the origin and step both ways, so a positive origin still covers the near edge.
  const offsets: number[] = []
  for (let start = origin % tile; start > -tile; start -= tile) {
    for (let position = start; position < extent; position += tile) offsets.push(position)
    break
  }
  const first = offsets[0] ?? origin
  if (first > 0) offsets.unshift(first - tile)
  return { offsets, extent: tile }
}
