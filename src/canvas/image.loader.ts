import { promises as fs } from 'fs'
import { loadImage, type Image as CanvasImage } from 'meo-skia-canvas'
import type { ImageProps } from '@/canvas/canvas.type.js'
import { hashBuffer, readDiskCache, writeDiskCache } from '@/util/disk.cache.js'
import { hashHttpOptions } from '@/util/http.options.js'

/**
 * One in-flight or finished load per source, for the length of a render.
 *
 * Keyed by content, so the same picture named by two nodes is fetched once. Lives in its own module
 * rather than beside `ImageNode`: a node's `backgroundImage` needs the same loading, and importing
 * it from the layout module would close a cycle with the class it extends.
 */
export type RenderImageCache = Map<string, Promise<CanvasImage>>

/** Everything the loader needs about a source, whichever kind of node asked for it. */
export interface ImageSource {
  /** A URL, a file path, or the bytes themselves. */
  src: string | Buffer
  /** Recolours an SVG's fills before it is rasterised. */
  color?: string
  /** Options for a remote fetch. Ignored for a local path or a buffer. */
  httpOptions?: ImageProps['httpOptions']
}

/**
 * The key a source is cached under, within a render and on disk.
 *
 * Content-addressed, so the same picture asked for twice is fetched once however it was named. A
 * tint is part of the key because it changes the pixels; request options are folded in for remote
 * sources only, since the same URL fetched with different headers is not the same picture.
 */
export function imageCacheKey(source: ImageSource): string {
  const srcHash = typeof source.src === 'string' ? hashBuffer(Buffer.from(source.src)) : hashBuffer(source.src)
  let key = source.color ? `${srcHash}|${source.color}` : srcHash

  const isHttpSrc = typeof source.src === 'string' && source.src.startsWith('http')
  if (isHttpSrc && source.httpOptions) {
    const optionsHash = hashHttpOptions(source.httpOptions)
    if (optionsHash) key += `|${optionsHash}`
  }

  return key
}

/**
 * Fetches and decodes a source, whether it is a URL, a path or a buffer.
 *
 * Touches no node state, so it serves an `Image` and a node's `backgroundImage` alike. If
 * `diskCacheKey` and `diskCacheKeys` are given the bytes are written to disk and the key recorded,
 * so the caller can clean up after the render.
 */
async function fetchCanvasImage(source: ImageSource, diskCacheKey?: string, diskCacheKeys?: Set<string>): Promise<CanvasImage> {
  const { fileTypeFromBuffer, fileTypeFromFile } = await import('file-type')
  let finalSource: string | Buffer
  let isSvg: boolean
  let contentBuffer: Buffer | null = null
  let detectedMime: string | undefined

  if (typeof source.src === 'string') {
    if (source.src.startsWith('http')) {
      const response = await fetch(source.src, source.httpOptions)
      if (!response.ok) {
        throw new Error(`HTTP error ${response.status} fetching image: ${source.src}`)
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
      finalSource = source.src
      const filePath = source.src

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

      if (isSvg && source.color) {
        try {
          contentBuffer = await fs.readFile(filePath)
        } catch {
          isSvg = false
          contentBuffer = null
        }
      }
    }
  } else {
    contentBuffer = source.src
    finalSource = contentBuffer

    const fileTypeResult = await fileTypeFromBuffer(contentBuffer)
    detectedMime = fileTypeResult?.mime
    isSvg = detectedMime === 'image/svg+xml'
  }

  if (isSvg && source.color && contentBuffer) {
    const svgString = contentBuffer.toString('utf-8')
    const modifiedSvgString = svgString.replace(/fill="[^"]*"/g, `fill="${source.color}"`)
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
 * Resolves a source to a decoded image, through the caches.
 *
 * Disk first, since that survives a restart; then the per-render map, which is what stops the same
 * picture being fetched once per node that wants it; then the network or the filesystem. The
 * promise goes into the map before it is awaited, so two nodes asking at once share one fetch
 * rather than starting two.
 */
export async function resolveCanvasImage(source: ImageSource, cache?: RenderImageCache, diskCacheKeys?: Set<string>): Promise<CanvasImage> {
  const cacheKey = imageCacheKey(source)

  if (diskCacheKeys) {
    const diskBuffer = await readDiskCache(cacheKey)
    if (diskBuffer) return loadImage(diskBuffer as Buffer)
  }

  if (!cache) return fetchCanvasImage(source, diskCacheKeys ? cacheKey : undefined, diskCacheKeys)

  if (!cache.has(cacheKey)) {
    cache.set(cacheKey, fetchCanvasImage(source, diskCacheKeys ? cacheKey : undefined, diskCacheKeys))
  }
  return cache.get(cacheKey)!
}
