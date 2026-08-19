import { vi } from 'vitest'
import type { CanvasRenderingContext2D, TextMetrics } from 'meo-skia-canvas'

export function createTestTextMetrics(overrides: Partial<TextMetrics> & Pick<TextMetrics, 'width'>): TextMetrics {
  const { width, ...rest } = overrides
  return {
    actualBoundingBoxLeft: 0,
    actualBoundingBoxRight: width,
    actualBoundingBoxAscent: 10,
    actualBoundingBoxDescent: 2,
    fontBoundingBoxAscent: 10,
    fontBoundingBoxDescent: 2,
    emHeightAscent: 10,
    emHeightDescent: 2,
    hangingBaseline: 0,
    alphabeticBaseline: 0,
    ideographicBaseline: 0,
    lines: [],
    width,
    ...rest,
  }
}

export function createMockCanvasContext(): CanvasRenderingContext2D {
  const ctx = {
    scale: vi.fn<CanvasRenderingContext2D['scale']>(),
    save: vi.fn<CanvasRenderingContext2D['save']>(),
    restore: vi.fn<CanvasRenderingContext2D['restore']>(),
    translate: vi.fn<CanvasRenderingContext2D['translate']>(),
    rotate: vi.fn<CanvasRenderingContext2D['rotate']>(),
    setTransform: vi.fn<CanvasRenderingContext2D['setTransform']>(),
    beginPath: vi.fn<CanvasRenderingContext2D['beginPath']>(),
    moveTo: vi.fn<CanvasRenderingContext2D['moveTo']>(),
    lineTo: vi.fn<CanvasRenderingContext2D['lineTo']>(),
    arc: vi.fn<CanvasRenderingContext2D['arc']>(),
    closePath: vi.fn<CanvasRenderingContext2D['closePath']>(),
    rect: vi.fn<CanvasRenderingContext2D['rect']>(),
    fill: vi.fn<CanvasRenderingContext2D['fill']>(),
    stroke: vi.fn<CanvasRenderingContext2D['stroke']>(),
    clip: vi.fn<CanvasRenderingContext2D['clip']>(),
    drawImage: vi.fn<CanvasRenderingContext2D['drawImage']>(),
    clearRect: vi.fn<CanvasRenderingContext2D['clearRect']>(),
    fillText: vi.fn<CanvasRenderingContext2D['fillText']>(),
    strokeText: vi.fn<CanvasRenderingContext2D['strokeText']>(),
    measureText: vi.fn<CanvasRenderingContext2D['measureText']>(_text =>
      createTestTextMetrics({ width: 0, actualBoundingBoxAscent: 10, actualBoundingBoxDescent: 2 }),
    ),
    fillStyle: '',
    strokeStyle: '',
    lineWidth: 0,
    lineCap: 'butt' as const,
    lineJoin: 'miter' as const,
    globalAlpha: 1,
    globalCompositeOperation: 'source-over',
    shadowOffsetX: 0,
    shadowOffsetY: 0,
    shadowBlur: 0,
    shadowColor: '',
    imageSmoothingEnabled: true,
    imageSmoothingQuality: 'high' as const,
    font: '',
    textAlign: 'left' as const,
    textBaseline: 'alphabetic' as const,
    letterSpacing: 'normal',
    wordSpacing: 'normal',
    textDecoration: 'none',
    fontVariant: 'normal',
    createLinearGradient: vi.fn<CanvasRenderingContext2D['createLinearGradient']>(() => ({
      addColorStop: vi.fn(),
      interpolation: 'srgb' as const,
      hueInterpolation: 'shorter' as const,
    })),
    createRadialGradient: vi.fn<CanvasRenderingContext2D['createRadialGradient']>(() => ({
      addColorStop: vi.fn(),
      interpolation: 'srgb' as const,
      hueInterpolation: 'shorter' as const,
    })),
    setLineDash: vi.fn<CanvasRenderingContext2D['setLineDash']>(),
  }

  return ctx as unknown as CanvasRenderingContext2D
}

export function createMockCanvas() {
  return vi.fn(function (
    this: { width: number; height: number; getContext: ReturnType<typeof vi.fn>; toBufferSync: ReturnType<typeof vi.fn> },
    w: number,
    h: number,
  ) {
    this.width = w
    this.height = h
    this.getContext = vi.fn(() => createMockCanvasContext())
    this.toBufferSync = vi.fn(() => Buffer.from(''))
  })
}
