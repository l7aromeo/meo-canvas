import type { BaseProps, ImageProps, CanvasElement } from '@/canvas/canvas.type.js'
import { type CanvasRenderingContext2D, Image as CanvasImage, loadImage } from 'skia-canvas'
import { BoxNode } from '@/canvas/layout.canvas.util.js'
import { drawRoundedRectPath, parseBorderRadius } from '@/canvas/canvas.helper.js'
import { promises as fs } from 'fs'
import { Style } from '@/constant/common.const.js'

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
 */
export type RenderImageCache = Map<string, Promise<CanvasImage>>

/**
 * Renders images with configurable sizing, positioning, and effects.
 * Supports object-fit modes, positioning, border radius, and saturation filters.
 */
export class ImageNode extends BoxNode {
  declare props: ImageProps & BaseProps
  private loadedImage: CanvasImage | null = null
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

  public load(cache?: RenderImageCache): Promise<void> {
    if (!this.loadingPromise) {
      this.loadingPromise = this._loadImage(cache)
    }
    return this.loadingPromise
  }

  /**
   * Fetches and processes the image source into a CanvasImage.
   * Does not touch node state — pure fetch logic.
   */
  private async _fetchCanvasImage(): Promise<CanvasImage> {
    const { fileTypeFromBuffer, fileTypeFromFile } = await import('file-type')
    let finalSource: string | Buffer = this.props.src
    let isSvg = false
    let contentBuffer: Buffer | null = null
    let detectedMime: string | undefined

    if (typeof this.props.src === 'string') {
      if (this.props.src.startsWith('http')) {
        const response = await fetch(this.props.src)
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

    return loadImage(finalSource as never)
  }

  /**
   * Loads and processes an image, using the render-scoped cache to avoid
   * re-fetching the same source within a single render pass.
   */
  private _loadImage(cache?: RenderImageCache): Promise<void> {
    if (!this.props.src) {
      const aspectRatioFromProps = typeof this.props.aspectRatio === 'number' && this.props.aspectRatio > 0 ? this.props.aspectRatio : undefined
      this.node.setAspectRatio(aspectRatioFromProps)
      this.naturalWidth = 0
      this.naturalHeight = 0
      return Promise.resolve()
    }

    return new Promise(resolve => {
      const load = async () => {
        try {
          let imagePromise: Promise<CanvasImage>

          if (cache && typeof this.props.src === 'string') {
            const cacheKey = this.props.color ? `${this.props.src}|${this.props.color}` : this.props.src
            if (!cache.has(cacheKey)) {
              cache.set(cacheKey, this._fetchCanvasImage())
            }
            imagePromise = cache.get(cacheKey)!
          } else {
            imagePromise = this._fetchCanvasImage()
          }

          const img = await imagePromise
          this.loadedImage = img
          this.naturalWidth = img.width
          this.naturalHeight = img.height

          const calculatedAspectRatio = this.naturalWidth > 0 && this.naturalHeight > 0 ? this.naturalWidth / this.naturalHeight : undefined
          const finalAspectRatio = typeof this.props.aspectRatio === 'number' && this.props.aspectRatio > 0 ? this.props.aspectRatio : calculatedAspectRatio

          this.node.setAspectRatio(finalAspectRatio)
          this.props.onLoad?.()
          resolve()
        } catch (error: any) {
          this.naturalWidth = 0
          this.naturalHeight = 0
          const finalAspectRatioOnError = typeof this.props.aspectRatio === 'number' && this.props.aspectRatio > 0 ? this.props.aspectRatio : undefined
          this.node.setAspectRatio(finalAspectRatioOnError)
          this.props.onError?.(error)
          resolve()
        }
      }
      load()
    })
  }

  public getLoadingPromise(): Promise<void> {
    return this.loadingPromise ?? this.load()
  }

  /**
   * Renders the image with correct sizing, clipping, and positioning.
   * Handles object-fit, object-position, and visual effects like saturation.
   */
  protected override _renderContent(ctx: CanvasRenderingContext2D, x: number, y: number, width: number, height: number) {
    super._renderContent(ctx, x, y, width, height)

    if (!this.loadedImage || width <= 0 || height <= 0) return
    const img = this.loadedImage
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

    // Apply clipping for border radius
    ctx.save()
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
    drawRoundedRectPath(ctx, contentX, contentY, contentWidth, contentHeight, contentRadii)
    ctx.clip()

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

    // Draw image with filters
    ctx.save()
    try {
      if (this.props.dropShadow) {
        const shadow = this.props.dropShadow
        const shadowBlur = Math.max(shadow.offsetX ?? 0, shadow.offsetY ?? 0)
        ctx.shadowOffsetX = shadow.offsetX ?? 0
        ctx.shadowOffsetY = shadow.offsetY ?? 0
        ctx.shadowBlur = Math.max(0, shadow.blur ?? shadowBlur)
        ctx.shadowColor = shadow.color ?? 'black'
      }

      const saturateValue = this.props.saturate ?? 1
      let filterString = ''
      if (saturateValue !== 1) {
        filterString += `saturate(${saturateValue * 100}%) `
      }

      if (filterString) {
        const currentFilter = ctx.filter && ctx.filter !== 'none' ? ctx.filter + ' ' : ''
        ctx.filter = currentFilter + filterString.trim()
      }

      const finalDX = Math.floor(dx)
      const finalDY = Math.floor(dy)
      const finalDW = Math.ceil(dw + (dx - finalDX))
      const finalDH = Math.ceil(dh + (dy - finalDY))

      if (finalDW > 0 && finalDH > 0) {
        ctx.drawImage(img, sx, sy, sw, sh, finalDX, finalDY, finalDW, finalDH)
      }
    } catch (drawError) {
      console.error('[ImageNode] Error drawing image:', drawError)
    } finally {
      ctx.restore()
    }

    ctx.restore()
  }
}

/**
 * Factory function to create ImageNode instances
 */
export const Image = (props: ImageProps): CanvasElement => ({
  __type: 'Image',
  props: props as Omit<ImageProps, 'onLoad' | 'onError'>,
})
