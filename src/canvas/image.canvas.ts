import type { BaseProps, ImageProps, CanvasElement } from '@/canvas/canvas.type.js'
import { type CanvasRenderingContext2D, Image as CanvasImage, loadImage } from 'meo-skia-canvas'
import { BoxNode } from '@/canvas/layout.canvas.js'
import { createCanvas, mirrorEngine } from '@/canvas/canvas.engine.js'
import { drawRoundedRectPath, parseBorderRadius } from '@/canvas/canvas.helper.js'
import { promises as fs } from 'fs'
import { Style } from '@/constant/common.const.js'
import { hashBuffer, readDiskCache, writeDiskCache } from '@/util/disk.cache.js'
import { frameAtTime } from '@/canvas/image.frames.js'
import { hashHttpOptions } from '@/util/http.options.js'

/**
 * Calculates pixel offset for image positioning based on percentage or pixel values.
 * This handles centering, edge alignment, and percentage-based positioning.
 */
function calculateOffsetFromValue(positionValue: number | `${number}%` | undefined, availableSpace: number): number {
  const value = positionValue ?? '50%'
  if (typeof value === 'number') {
    return value
  }
  if (typeof value === 'string' && value.endsWith('%')) {
    const percentage = parseFloat(value) / 100
    return availableSpace * percentage
  }
  console.warn(`[ImageNode] Invalid objectPosition value format: ${value}. Defaulting to 50%.`)
  return availableSpace * 0.5
}

/**
 * Per-render image cache — keyed by `src|color` for string sources.
 * Scoped to a single RootNode.render() call; discarded after rendering.
 * Deduplicates concurrent fetches when multiple ImageNodes share the same src.
 */
export type RenderImageCache = Map<string, Promise<CanvasImage>>

/**
 * Renders images with configurable sizing, positioning, and effects.
 * Supports object-fit modes, positioning, border radius, and saturation filters.
 */
export class ImageNode extends BoxNode {
  declare props: ImageProps & BaseProps
  private loadedImage: CanvasImage | null = null

  /**
   * When this node is being drawn, in seconds from the start of the render.
   *
   * Set by the render rather than passed as a prop: an animated source plays at its own rate, and
   * the page's clock is the only thing that knows how far along the sequence a page is. A still
   * render leaves it undefined, which is what keeps such a source on its first frame.
   */
  private pageTime: number | undefined
  private naturalWidth = 0
  private naturalHeight = 0
  private loadingPromise: Promise<void> | null = null

  constructor(props: ImageProps) {
    super({ name: 'Image', ...props, children: undefined })

    this.props = {
      objectFit: 'fill',
      overflow: Style.Overflow.Hidden,
      saturate: 1,
      objectPosition: { Left: '50%', Top: '50%' },
      ...props,
    }
  }

  /**
   * The aspect ratio Yoga should size this node by, or `undefined` to leave its dimensions alone.
   *
   * An intrinsic ratio only fills in a dimension the caller left out, which is what CSS does with
   * an image's own proportions. Handing Yoga a ratio when both `width` and `height` are given lets
   * it override one of them: a 40x20 image in a box declared 120x80 was laid out 160x80, and every
   * `objectFit` mode then measured itself against a box the caller never asked for — `contain` and
   * `cover` became indistinguishable from `fill`, because the node had been reshaped to the image's
   * own ratio before the fit was computed.
   */
  private sizingAspectRatio(natural: number | undefined): number | undefined {
    const asked = typeof this.props.aspectRatio === 'number' && this.props.aspectRatio > 0 ? this.props.aspectRatio : undefined
    if (asked !== undefined) return asked

    const hasWidth = this.props.width !== undefined
    const hasHeight = this.props.height !== undefined
    return hasWidth && hasHeight ? undefined : natural
  }

  /** Tells this node which moment of the render it is being drawn for. */
  setPageTime(seconds: number): void {
    this.pageTime = seconds
  }

  /**
   * The frame of an animated source to draw, resolved for this page.
   *
   * An explicit `frame` wins and is handed to the renderer as given, so a negative index counts
   * from the end and an impossible one is refused there rather than quietly clamped here. With no
   * `frame`, an animated source plays: the page's own time is matched against the source's delays.
   */
  private currentFrame(image: CanvasImage): CanvasImage {
    if (image.frames <= 1) return image

    const { frame, loop } = this.props
    if (frame !== undefined) return image.frame(frame)
    if (this.pageTime === undefined) return image

    return image.frame(frameAtTime(image.delays, this.pageTime, { loop }))
  }

  /**
   * Fetches and decodes the source, then tells Yoga what proportions it has.
   *
   * `cache` is scoped to one render, so the same URL appearing on several nodes is fetched once.
   * Safe to call more than once: the first call's promise is returned again rather than a second
   * fetch being started.
   */
  public load(cache?: RenderImageCache, diskCacheKeys?: Set<string>): Promise<void> {
    if (!this.loadingPromise) {
      this.loadingPromise = this._loadImage(cache, diskCacheKeys)
    }
    return this.loadingPromise
  }

  /**
   * Fetches and processes the image source into a CanvasImage.
   * Does not touch node state — pure fetch logic.
   *
   * If `diskCacheKey` and `diskCacheKeys` are provided, the resolved image buffer
   * is written to disk and the key is recorded so the caller can clean it up later.
   */
  private async _fetchCanvasImage(diskCacheKey?: string, diskCacheKeys?: Set<string>): Promise<CanvasImage> {
    const { fileTypeFromBuffer, fileTypeFromFile } = await import('file-type')
    let finalSource: string | Buffer
    let isSvg: boolean
    let contentBuffer: Buffer | null = null
    let detectedMime: string | undefined

    if (typeof this.props.src === 'string') {
      if (this.props.src.startsWith('http')) {
        const response = await fetch(this.props.src, this.props.httpOptions)
        if (!response.ok) {
          throw new Error(`HTTP error ${response.status} fetching image: ${this.props.src}`)
        }
        const imageArrayBuffer = await response.arrayBuffer()
        contentBuffer = Buffer.from(imageArrayBuffer)
        finalSource = contentBuffer

        const fileTypeResult = await fileTypeFromBuffer(contentBuffer)
        detectedMime = fileTypeResult?.mime
        isSvg = detectedMime === 'image/svg+xml'

        if ((!detectedMime || detectedMime === 'application/xml') && contentBuffer.toString('utf-8').includes('<svg')) {
          isSvg = true
        }
      } else {
        finalSource = this.props.src
        const filePath = this.props.src

        try {
          const fileTypeResult = await fileTypeFromFile(filePath)
          detectedMime = fileTypeResult?.mime
          isSvg = detectedMime === 'image/svg+xml'

          if ((!detectedMime || detectedMime === 'application/xml') && filePath.toLowerCase().endsWith('.svg')) {
            isSvg = true
          }
        } catch {
          isSvg = filePath.toLowerCase().endsWith('.svg')
        }

        if (isSvg && this.props.color) {
          try {
            contentBuffer = await fs.readFile(filePath)
          } catch {
            isSvg = false
            contentBuffer = null
          }
        }
      }
    } else {
      contentBuffer = this.props.src
      finalSource = contentBuffer

      const fileTypeResult = await fileTypeFromBuffer(contentBuffer)
      detectedMime = fileTypeResult?.mime
      isSvg = detectedMime === 'image/svg+xml'
    }

    if (isSvg && this.props.color && contentBuffer) {
      const svgString = contentBuffer.toString('utf-8')
      const modifiedSvgString = svgString.replace(/fill="[^"]*"/g, `fill="${this.props.color}"`)
      finalSource = modifiedSvgString !== svgString ? Buffer.from(modifiedSvgString) : contentBuffer
    }

    // Write to disk and track the key so the render owner can clean it up
    if (diskCacheKey && diskCacheKeys) {
      const cacheBuffer = Buffer.isBuffer(finalSource) ? finalSource : contentBuffer
      if (cacheBuffer) {
        await writeDiskCache(diskCacheKey, cacheBuffer)
        diskCacheKeys.add(diskCacheKey)
      }
    }

    return loadImage(finalSource as Buffer)
  }

  /**
   * Loads and processes an image.
   *
   * Resolution order:
   *   1. Disk cache at `.cache/files/<hash>` — survives process restarts.
   *   2. Per-render dedup cache — avoids duplicate in-flight fetches when
   *      multiple ImageNodes share the same src within one render pass.
   *   3. Fresh fetch via `_fetchCanvasImage()` — writes buffer to disk cache.
   *
   * Buffer sources use a SHA-256 hash as their cache key (same as string sources).
   * All resolved images are released when the render completes (no cross-render retention).
   */
  private _loadImage(cache?: RenderImageCache, diskCacheKeys?: Set<string>): Promise<void> {
    if (!this.props.src) {
      this.node.setAspectRatio(this.sizingAspectRatio(undefined))
      this.naturalWidth = 0
      this.naturalHeight = 0
      return Promise.resolve()
    }

    return new Promise(resolve => {
      const load = async () => {
        try {
          const srcHash = typeof this.props.src === 'string' ? hashBuffer(Buffer.from(this.props.src)) : hashBuffer(this.props.src)
          let cacheKey = this.props.color ? `${srcHash}|${this.props.color}` : srcHash

          // httpOptions only affect remote fetches, so fold them into the key
          // for http(s) sources only — same URL + different headers/body must
          // not share a cached image.
          const isHttpSrc = typeof this.props.src === 'string' && this.props.src.startsWith('http')
          if (isHttpSrc && this.props.httpOptions) {
            const optionsHash = hashHttpOptions(this.props.httpOptions)
            if (optionsHash) cacheKey += `|${optionsHash}`
          }

          // 1. Disk cache read — only when disk caching is enabled for this render
          if (diskCacheKeys) {
            const diskBuffer = await readDiskCache(cacheKey)
            if (diskBuffer) {
              const img = await loadImage(diskBuffer as Buffer)
              this.loadedImage = img
              this.naturalWidth = img.width
              this.naturalHeight = img.height
              const calculatedAspectRatio = img.width > 0 && img.height > 0 ? img.width / img.height : undefined
              this.node.setAspectRatio(this.sizingAspectRatio(calculatedAspectRatio))
              this.props.onLoad?.()
              resolve()
              return
            }
          }

          // 2. Per-render memory dedup cache or fresh fetch
          let imagePromise: Promise<CanvasImage>
          if (cache) {
            if (!cache.has(cacheKey)) {
              cache.set(cacheKey, this._fetchCanvasImage(diskCacheKeys ? cacheKey : undefined, diskCacheKeys))
            }
            imagePromise = cache.get(cacheKey)!
          } else {
            imagePromise = this._fetchCanvasImage(diskCacheKeys ? cacheKey : undefined, diskCacheKeys)
          }

          const img = await imagePromise

          this.loadedImage = img
          this.naturalWidth = img.width
          this.naturalHeight = img.height

          const calculatedAspectRatio = this.naturalWidth > 0 && this.naturalHeight > 0 ? this.naturalWidth / this.naturalHeight : undefined
          this.node.setAspectRatio(this.sizingAspectRatio(calculatedAspectRatio))
          this.props.onLoad?.()
          resolve()
        } catch (error: any) {
          this.naturalWidth = 0
          this.naturalHeight = 0
          this.node.setAspectRatio(this.sizingAspectRatio(undefined))
          this.props.onError?.(error)
          resolve()
        }
      }
      load()
    })
  }

  /** The in-flight load, starting one if nothing has asked yet. */
  public getLoadingPromise(): Promise<void> {
    return this.loadingPromise ?? this.load()
  }

  /**
   * Renders the image with correct sizing, clipping, and positioning.
   * Handles object-fit, object-position, and visual effects like saturation.
   */
  protected override async _renderContent(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
    await super._renderContent(ctx, x, y, width, height)

    if (!this.loadedImage || width <= 0 || height <= 0) return
    const img = this.currentFrame(this.loadedImage)
    const imgW = this.naturalWidth
    const imgH = this.naturalHeight
    if (imgW <= 0 || imgH <= 0) return

    // Calculate content box accounting for padding and borders
    const paddingLeft = this.node.getComputedPadding(Style.Edge.Left)
    const paddingTop = this.node.getComputedPadding(Style.Edge.Top)
    const paddingRight = this.node.getComputedPadding(Style.Edge.Right)
    const paddingBottom = this.node.getComputedPadding(Style.Edge.Bottom)
    const borderLeft = this.node.getComputedBorder(Style.Edge.Left)
    const borderTop = this.node.getComputedBorder(Style.Edge.Top)
    const borderRight = this.node.getComputedBorder(Style.Edge.Right)
    const borderBottom = this.node.getComputedBorder(Style.Edge.Bottom)
    const contentX = x + borderLeft + paddingLeft
    const contentY = y + borderTop + paddingTop
    const contentWidth = Math.max(0, width - borderLeft - paddingLeft - borderRight - paddingRight)
    const contentHeight = Math.max(0, height - borderTop - paddingTop - borderBottom - paddingBottom)

    if (contentWidth <= 0 || contentHeight <= 0) return

    const outerRadii = parseBorderRadius(this.props.borderRadius)
    const innerBorderRadii = {
      TopLeft: Math.max(0, outerRadii.TopLeft - borderTop),
      TopRight: Math.max(0, outerRadii.TopRight - borderTop),
      BottomRight: Math.max(0, outerRadii.BottomRight - borderBottom),
      BottomLeft: Math.max(0, outerRadii.BottomLeft - borderBottom),
    }
    const contentRadii = {
      TopLeft: Math.max(0, innerBorderRadii.TopLeft - Math.max(paddingLeft, paddingTop)),
      TopRight: Math.max(0, innerBorderRadii.TopRight - Math.max(paddingRight, paddingTop)),
      BottomRight: Math.max(0, innerBorderRadii.BottomRight - Math.max(paddingRight, paddingBottom)),
      BottomLeft: Math.max(0, innerBorderRadii.BottomLeft - Math.max(paddingLeft, paddingBottom)),
    }
    // Calculate image dimensions based on object-fit
    const nodeRatio = contentWidth / contentHeight
    const imgRatio = imgW / imgH
    const objectFit = this.props.objectFit
    let dw = contentWidth
    let dh = contentHeight

    if (objectFit === 'contain') {
      if (imgRatio > nodeRatio) {
        dw = contentWidth
        dh = contentWidth / imgRatio
      } else {
        dh = contentHeight
        dw = contentHeight * imgRatio
      }
    } else if (objectFit === 'cover') {
      if (imgRatio > nodeRatio) {
        dh = contentHeight
        dw = contentHeight * imgRatio
      } else {
        dw = contentWidth
        dh = contentWidth / imgRatio
      }
    } else if (objectFit === 'none') {
      dw = imgW
      dh = imgH
    } else if (objectFit === 'scale-down') {
      if (imgW <= contentWidth && imgH <= contentHeight) {
        dw = imgW
        dh = imgH
      } else {
        if (imgRatio > nodeRatio) {
          dw = contentWidth
          dh = contentWidth / imgRatio
        } else {
          dh = contentHeight
          dw = contentHeight * imgRatio
        }
      }
    }

    // Calculate image position based on object-position
    const sx = 0
    const sy = 0
    const sw = imgW
    const sh = imgH

    const availableWidth = contentWidth - dw
    const availableHeight = contentHeight - dh
    const posProps = this.props.objectPosition || {}
    const horizontalValue = posProps.Left !== undefined ? posProps.Left : posProps.Right !== undefined ? posProps.Right : '50%'
    const verticalValue = posProps.Top !== undefined ? posProps.Top : posProps.Bottom !== undefined ? posProps.Bottom : '50%'

    let offsetX = calculateOffsetFromValue(horizontalValue, availableWidth)
    let offsetY = calculateOffsetFromValue(verticalValue, availableHeight)

    if (posProps.Left === undefined && posProps.Right !== undefined) {
      offsetX = availableWidth - offsetX
    }
    if (posProps.Top === undefined && posProps.Bottom !== undefined) {
      offsetY = availableHeight - offsetY
    }

    const dx = contentX + offsetX
    const dy = contentY + offsetY

    // Where the image lands, rounded to whole pixels so the sampler is not asked to filter a
    // fractional destination.
    const finalDX = Math.floor(dx)
    const finalDY = Math.floor(dy)
    const finalDW = Math.ceil(dw + (dx - finalDX))
    const finalDH = Math.ceil(dh + (dy - finalDY))

    /** Draws the image as this node paints it: clipped to the content box, corners and all. */
    const paint = (target: CanvasRenderingContext2D) => {
      target.save()
      try {
        drawRoundedRectPath(target, contentX, contentY, contentWidth, contentHeight, contentRadii)
        target.clip()
        if (finalDW > 0 && finalDH > 0) {
          target.drawImage(img, sx, sy, sw, sh, finalDX, finalDY, finalDW, finalDH)
        }
      } catch (drawError) {
        console.error('[ImageNode] Error drawing image:', drawError)
      } finally {
        target.restore()
      }
    }

    // A drop shadow falls outside the node's box by definition, and the clip inside `paint` exists
    // to keep the image within it — so a shadow cast inside that clip was clipped away and nothing
    // appeared at all. CSS has the same order: `overflow` clips an element's content, and a
    // `drop-shadow` filter applies to what is left afterwards.
    //
    // So the drawing is built once on an offscreen and then composited in a single call with the
    // shadow set, which both places the image and casts the shadow from the pixels actually drawn —
    // following a rounded corner or a transparent edge rather than outlining the box.
    const shadow = this.props.dropShadow
    if (shadow && width > 0 && height > 0) {
      const silhouette = createCanvas(Math.ceil(width), Math.ceil(height), mirrorEngine(ctx))
      const silhouetteCtx = silhouette.getContext('2d')
      silhouetteCtx.translate(-x, -y)
      paint(silhouetteCtx)

      ctx.save()
      ctx.shadowOffsetX = shadow.offsetX ?? 0
      ctx.shadowOffsetY = shadow.offsetY ?? 0
      ctx.shadowBlur = Math.max(0, shadow.blur ?? 0)
      ctx.shadowColor = shadow.color ?? 'black'
      ctx.drawImage(silhouette, x, y)
      ctx.restore()
    } else {
      paint(ctx)
    }
  }
}

/**
 * Draws an image from a URL, a file path or a `Buffer`.
 *
 * Remote sources are fetched during the render, so a paged render shares one cache across pages and
 * loads a repeated source once.
 * @example
 * ```ts
 * Image({
 *   src: 'https://example.com/avatar.png',
 *   width: 64,
 *   height: 64,
 *   objectFit: 'cover',
 *   borderRadius: 32,
 *   httpOptions: { headers: { Authorization: 'Bearer …' } },
 * })
 * ```
 */
export const Image = (props: ImageProps): CanvasElement => ({
  __type: 'Image',
  props: props as Omit<ImageProps, 'onLoad' | 'onError'>,
})
