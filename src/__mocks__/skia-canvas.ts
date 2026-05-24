import { jest } from '@jest/globals'
import { _clearRegisteredFonts } from '@/__mocks__/root.canvas.js'

export const mockCanvasContext = {
  scale: jest.fn(),
  drawImage: jest.fn(),
  save: jest.fn(),
  restore: jest.fn(),
  filter: '',
  shadowOffsetX: 0,
  shadowOffsetY: 0,
  shadowBlur: 0,
  shadowColor: '',
  globalAlpha: 1,
  beginPath: jest.fn(),
  rect: jest.fn(),
  clip: jest.fn(),
  fill: jest.fn(),
  stroke: jest.fn(),
  fillStyle: '',
  strokeStyle: '',
  lineWidth: 0,
  createLinearGradient: jest.fn(() => ({
    addColorStop: jest.fn(),
  })),
  createRadialGradient: jest.fn(() => ({
    addColorStop: jest.fn(),
  })),
  globalCompositeOperation: '',
  imageSmoothingEnabled: true,
  imageSmoothingQuality: 'high',
}

export const Canvas = jest.fn(function (this: any, width: number, height: number) {
  this.width = width
  this.height = height
  // Ensure getContext returns a fresh mockCanvasContext for each Canvas instance
  this.getContext = jest.fn(() => {
    // Reset mockCanvasContext before returning it to ensure a clean state for each test
    for (const key in mockCanvasContext) {
      if (jest.isMockFunction((mockCanvasContext as any)[key])) {
        ;(mockCanvasContext as any)[key].mockClear()
      }
    }
    return mockCanvasContext
  })
  this.toBuffer = jest.fn(() => Buffer.from(''))
})

export const loadImage = jest.fn()

export const FontLibrary = {
  use: jest.fn(),
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
