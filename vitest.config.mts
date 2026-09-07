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
    /*
     * A hang detector, not a performance budget.
     *
     * **The number this replaces was never chosen.** It was vitest's default,
     * and this file had an opinion about coverage and none about time. Five
     * seconds is a performance budget wearing a hang detector's clothes, which
     * is why it fired on a busy machine: a busy machine is a performance
     * question and a hang is not.
     *
     * **It is deliberately not chosen to survive contention.** That is a
     * treadmill -- whatever number outlasts today's load loses to tomorrow's,
     * and the load line in `tools/contention-reporter.mjs` is what handles a
     * contended run. The question this answers is the other one: how long must
     * a test run before it is broken rather than slow?
     *
     * **Measured, so the multiple is a fact rather than a feeling.** Across 662
     * tests on a quiet box, p95 is 57ms, p99 is 336ms and the slowest single
     * test is 1616ms once an addon build has run in front of it -- 824ms on a
     * fifth consecutive warm run, which is a condition the gate never has. No test in this
     * repository has ever asked for a longer limit. Under the worst contention
     * observed -- three project gates on fourteen cores -- the same tests took
     * 7.5 to 8.8 seconds, and that is the ceiling on *legitimate* slowness ever
     * seen here.
     *
     * Thirty seconds is 18.6x the slowest legitimate test and 3.4x the worst a
     * contended one has ever measured. A test still running there is not slow.
     *
     * **What it costs is twenty-five extra seconds to notice a hang**, against
     * thirteen fake failures and two diagnostic runs every time two projects
     * build at once.
     */
    testTimeout: 30_000,
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
