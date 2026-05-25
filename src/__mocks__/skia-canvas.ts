import { vi } from 'vitest'

/** Default monospace-ish width — tests can replace via measureText.mockImplementation */
const defaultMeasureWidth = (text: string) => [...text].length * 8

export const mockCanvasContext = {
  scale: vi.fn(),
  drawImage: vi.fn(),
  fillText: vi.fn(),
  strokeText: vi.fn(),
  measureText: vi.fn((text: string) => ({
    width: defaultMeasureWidth(text),
    actualBoundingBoxAscent: 10,
    actualBoundingBoxDescent: 3,
  })),
  font: '',
  textAlign: 'left' as const,
  textBaseline: 'alphabetic' as const,
  letterSpacing: 'normal',
  wordSpacing: 'normal',
  fontVariant: 'normal',
  save: vi.fn(),
  restore: vi.fn(),
  filter: '',
  shadowOffsetX: 0,
  shadowOffsetY: 0,
  shadowBlur: 0,
  shadowColor: '',
  globalAlpha: 1,
  beginPath: vi.fn(),
  moveTo: vi.fn(),
  lineTo: vi.fn(),
  arc: vi.fn(),
  closePath: vi.fn(),
  rect: vi.fn(),
  clip: vi.fn(),
  fill: vi.fn(),
  stroke: vi.fn(),
  fillStyle: '',
  strokeStyle: '',
  lineWidth: 0,
  lineCap: 'butt' as const,
  lineJoin: 'miter' as const,
  setLineDash: vi.fn(),
  createLinearGradient: vi.fn(() => ({
    addColorStop: vi.fn(),
  })),
  createRadialGradient: vi.fn(() => ({
    addColorStop: vi.fn(),
  })),
  globalCompositeOperation: '',
  imageSmoothingEnabled: true,
  imageSmoothingQuality: 'high',
}

export const Canvas = vi.fn(function (this: any, width: number, height: number) {
  this.width = width
  this.height = height
  // Ensure getContext returns a fresh mockCanvasContext for each Canvas instance
  this.getContext = vi.fn(() => {
    // Reset mockCanvasContext before returning it to ensure a clean state for each test
    for (const key in mockCanvasContext) {
      if (vi.isMockFunction((mockCanvasContext as any)[key])) {
        ;(mockCanvasContext as any)[key].mockClear()
      }
    }
    return mockCanvasContext
  })
  this.toBuffer = vi.fn(() => Buffer.from(''))
})

export const loadImage = vi.fn()

export const FontLibrary = {
  use: vi.fn(),
}

export const __mocks__ = {
  Canvas,
  FontLibrary,
  loadImage,
  mockCanvasContext,
  reset: () => {
    // Reset Canvas and FontLibrary mocks (avoid importing root.canvas mock here — circular dep)
    Canvas.mockClear()
    FontLibrary.use.mockClear()
    loadImage.mockClear()
  },
}
