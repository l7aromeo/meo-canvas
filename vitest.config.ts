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
    coverage: {
      provider: 'v8',
      reportsDirectory: './coverage',
      exclude: ['node_modules/**', 'dist/**'],
    },
    pool: 'forks',
  },
})
