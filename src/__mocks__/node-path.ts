import { jest } from '@jest/globals'

export const __mocks__ = {
  resolve: jest.fn(p => p), // Default to returning the path itself
  reset: jest.fn(() => {
    __mocks__.resolve.mockClear()
    __mocks__.resolve.mockImplementation(p => p) // Reset to default behavior
  }),
}
