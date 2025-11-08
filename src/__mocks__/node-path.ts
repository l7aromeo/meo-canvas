import { jest } from '@jest/globals'

jest.mock('node:path', () => ({
  resolve: jest.fn(p => p),
}))

export const __mocks__ = {
  resolve: jest.fn(),
  reset: jest.fn(),
}
