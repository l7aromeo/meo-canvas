import { loadavg, availableParallelism } from 'node:os'

/**
 * Says so when a failing run looks like a busy machine rather than a broken tree.
 *
 * **A gate cannot tell a slow box from a bad commit, and reports both as red
 * with a list of names attached.** On 2026-09-08 thirteen tests failed here,
 * every one a `Test timed out in 5000ms`, and the same tree passed on its own
 * minutes later. Establishing that took two more runs; this puts it in the
 * output of the run that failed.
 *
 * **The signature is a failure set that is entirely timeouts.** A genuine defect
 * produces an assertion diff, and a mixture means both are happening — so the
 * line is printed only when nothing failed for any other reason, and it says how
 * many did, which keeps it from claiming contention over a real break.
 *
 * **The load average is what makes it checkable.** Measured on this suite: on a
 * quiet box no test reaches a second — p95 is 57ms, the slowest is 824ms — while
 * under load the thirteen took 7.5 to 8.0 seconds each. Their quiet durations
 * span 118x and their loaded durations span 1.07x, so the time was not being
 * spent in the tests at all: every one of them acquired the same constant while
 * waiting for a core. **A per-test timeout under contention is bounding
 * scheduling latency rather than work**, which is why the number in the message
 * says nothing about the code.
 */
export default class ContentionReporter {
  /** The load average when the run began, for comparison with the end. */
  #started = []

  onTestRunStart() {
    this.#started = loadavg()
  }

  onTestRunEnd(testModules) {
    let timedOut = 0
    let otherwise = 0
    for (const module of testModules) {
      for (const test of module.children.allTests('failed')) {
        const result = test.result()
        const errors = result.state === 'failed' ? result.errors : []
        // The message rather than a type: vitest reports a timeout as an
        // ordinary error whose text names the limit it crossed.
        if (errors.some(error => (error.message ?? '').includes('Test timed out in'))) timedOut += 1
        else otherwise += 1
      }
    }

    if (timedOut === 0 || otherwise > 0) return

    const now = loadavg()
    const shape = averages => averages.map(one => one.toFixed(2)).join(' / ')
    process.stderr.write(
      `\n${timedOut} test${timedOut === 1 ? '' : 's'} failed, every one a timeout and none an assertion.\n` +
        `Load average ${shape(this.#started)} at the start and ${shape(now)} now, on ${availableParallelism()} cores.\n` +
        'On a quiet box no test in this suite reaches a second. A failure set that is entirely timeouts is\n' +
        'the signature of a contended machine rather than a broken tree -- run `just test-js` alone to tell\n' +
        'them apart, and see tools/contention-reporter.mts for the measurements behind this line.\n',
    )
  }
}
