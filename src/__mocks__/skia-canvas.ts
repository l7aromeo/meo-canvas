import { vi } from 'vitest'
import { _clearRegisteredFonts } from '@/__mocks__/root.canvas.js'

export const mockCanvasContext = {
  scale: vi.fn(),
  drawImage: vi.fn(),
  save: vi.fn(),
  restore: vi.fn(),
  filter: '',
  shadowOffsetX: 0,
  shadowOffsetY: 0,
  shadowBlur: 0,
  shadowColor: '',
  globalAlpha: 1,
  beginPath: vi.fn(),
  rect: vi.fn(),
  clip: vi.fn(),
  fill: vi.fn(),
  stroke: vi.fn(),
  fillStyle: '',
  strokeStyle: '',
  lineWidth: 0,
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
    // Reset Canvas and FontLibrary mocks
    Canvas.mockClear()
    FontLibrary.use.mockClear()
    loadImage.mockClear()

    // Clear the registered fonts in the actual module
    _clearRegisteredFonts()
  },
}
