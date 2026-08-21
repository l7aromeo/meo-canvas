import path from 'node:path'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  resolve: {
    alias: [
      { find: /^@\/(.+)\.js$/, replacement: path.resolve(__dirname, 'src/$1.ts') },
      { find: '@', replacement: path.resolve(__dirname, 'src') },
    ],
  },
  test: {
    environment: 'node',
    globals: true,
    include: ['**/__tests__/**/*.test.ts', '**/?(*.)+(spec|test).ts'],
    exclude: ['node_modules/**', 'dist/**'],
    testTimeout: 30_000,
    coverage: {
      provider: 'v8',
      reportsDirectory: './coverage',
      exclude: ['node_modules/**', 'dist/**', 'scripts/**', '**/__mocks__/**', 'tests/**'],
      thresholds: {
        'src/canvas/layout.canvas.ts': { lines: 90, statements: 90, functions: 90 },
        'src/canvas/canvas.helper.ts': { lines: 90, statements: 90, functions: 90 },
        'src/canvas/text.canvas.ts': { lines: 90, statements: 90, functions: 90 },
        'src/util/disk.cache.ts': { lines: 90, statements: 90, functions: 90 },
        'src/worker/comlink.pool.ts': { lines: 90, statements: 90, functions: 90 },
        'src/worker/canvas-handlers.ts': { lines: 90, statements: 90, functions: 90 },
      },
    },
    pool: 'forks',
  },
})
