import { defineConfig } from 'vitest/config'

/**
 * The JavaScript suite, and the floor it has to clear.
 *
 * The same 90% the Rust half is held to, and the same rule about the
 * denominator: nothing is excluded for being hard to reach. A file earns an
 * exclusion by being **generated** rather than written, and each one is named
 * here a path at a time so the list is reviewable in a diff.
 *
 * The three generated files are the arena property tables, the wire-enum
 * tables and the lifted doc examples. None of them is code anyone wrote, and a
 * generated table would otherwise be counted as a hundred uncovered lines that
 * no test could honestly cover — or, worse, be covered by a test written to
 * cover it rather than to check anything.
 */
export default defineConfig({
  test: {
    include: ['packages/meo-canvas/src/**/*.test.ts'],
    // The default reporter, plus one that says so when a failing run looks like
    // a busy machine. It prints nothing unless every failure is a timeout, so a
    // real break is never explained away as load.
    reporters: ['default', new URL('packages/meo-canvas/tools/contention-reporter.mjs', import.meta.url).pathname],
    coverage: {
      provider: 'v8',
      // Everything the package ships, whether a test reached it or not. Without
      // this the denominator is the files the tests happened to import, and a
      // module with no test at all would raise the percentage by being absent.
      all: true,
      include: ['packages/meo-canvas/src/**/*.ts'],
      exclude: ['packages/meo-canvas/src/generated/**'],
      thresholds: {
        lines: 90,
        branches: 90,
        functions: 90,
        statements: 90,
      },
    },
  },
})
