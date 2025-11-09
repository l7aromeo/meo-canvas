import { jest } from '@jest/globals'

export const existsSync = jest.fn<(val: any) => boolean>(_val => true)

export const __mocks__ = {
  existsSync,
  reset: () => {
    existsSync.mockClear()
    existsSync.mockReturnValue(true) // Reset to default behavior
  },
}
