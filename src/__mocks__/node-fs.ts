import { jest } from '@jest/globals'

export const existsSync = jest.fn(() => true)

export const __mocks__ = {
  existsSync,
  reset: () => {
    existsSync.mockClear()
    existsSync.mockReturnValue(true) // Reset to default behavior
  },
}
