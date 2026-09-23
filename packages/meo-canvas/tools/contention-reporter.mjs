import { loadavg, availableParallelism } from 'node:os'

/**
 * Says so when every failure in a run is a timeout, with the load average, since a
 * busy machine fails tests that pass alone. Printed only when nothing failed another
 * way; under load a per-test timeout bounds scheduling latency, not work.
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
        'them apart, and see tools/contention-reporter.mjs for the measurements behind this line.\n',
    )
  }
}
