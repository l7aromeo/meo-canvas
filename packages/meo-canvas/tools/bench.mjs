// What a render costs in time and memory through the shipped surface, over thousands
// of renders in one process. RSS is a delta from a warm baseline; peak is sampled,
// a lower bound, with its sample count printed; idle, after a forced collection,
// answers "does it leak"; wall time is percentiles.

// The built package, not the source: this measures what a consumer installs,
// and `dist` is what `exports` points at.
import { Box, Root, Text } from '../dist/index.js'

/** How often the sampler reads RSS while renders are running. */
const SAMPLE_INTERVAL_MS = 20

/** How long the process sits idle before the settled reading. */
const IDLE_MS = 10_000

/** Renders taken before the baseline, so the first-call costs are not measured as steady state. */
const WARMUP = 20

/** Renders measured. */
const ITERATIONS = Number(process.env.MEO_BENCH_ITERATIONS ?? 500)

/** Bytes as MiB, to one decimal, signed so a delta reads as one. */
const mib = bytes => `${bytes < 0 ? '' : '+'}${(bytes / 1024 / 1024).toFixed(1)} MiB`

/** Bytes as MiB with no sign, for an absolute figure. */
const abs = bytes => `${(bytes / 1024 / 1024).toFixed(1)} MiB`

/**
 * A scene with text, a gradient and nested boxes, so every pass runs -- text is the
 * one that shapes, measures and caches.
 */
function scene(index) {
  return Root({
    width: 480,
    height: 320,
    backgroundColor: '#101820',
    padding: 16,
    flexDirection: 'column',
    gap: 8,
    children: [
      Box({
        width: 448,
        height: 120,
        gradient: {
          type: 'linear',
          direction: 135,
          stops: [
            { offset: 0, color: '#f2aa4c' },
            { offset: 0.5, color: '#ffffff' },
            { offset: 1, color: '#2850dc' },
          ],
        },
      }),
      Box({
        flexDirection: 'row',
        gap: 8,
        children: [
          Box({ width: 120, height: 80, backgroundColor: '#dc2828', borderRadius: 8 }),
          Box({ width: 120, height: 80, backgroundColor: '#288c3c', borderRadius: 8 }),
          Text(`render ${index}`, { fontSize: 18, color: '#eeeef2' }),
        ],
      }),
    ],
  })
}

/** One render, encoded, released, and how long it took in milliseconds. */
async function once(index) {
  const started = process.hrtime.bigint()
  const canvas = await scene(index)
  const bytes = await canvas.toBuffer('png')
  canvas.release()
  return { ms: Number(process.hrtime.bigint() - started) / 1e6, bytes: bytes.length }
}

/** The value at a percentile of a sorted list. */
function percentile(sorted, fraction) {
  if (sorted.length === 0) return 0
  const at = Math.min(sorted.length - 1, Math.floor(fraction * sorted.length))
  return sorted[at]
}

/** Collects, if the flag that allows it was passed. Reported rather than assumed. */
function collect() {
  if (typeof globalThis.gc === 'function') {
    globalThis.gc()
    return true
  }
  return false
}

for (let index = 0; index < WARMUP; index += 1) await once(index)

const collected = collect()
const baseline = process.memoryUsage()

let peakRss = baseline.rss
let samples = 0
const sample = () => {
  samples += 1
  const { rss } = process.memoryUsage()
  if (rss > peakRss) peakRss = rss
}
const sampler = setInterval(sample, SAMPLE_INTERVAL_MS)
// The sampler must not be what keeps the process alive.
sampler.unref()

const times = []
let encoded = 0
const wallStarted = process.hrtime.bigint()
for (let index = 0; index < ITERATIONS; index += 1) {
  const result = await once(index)
  times.push(result.ms)
  encoded += result.bytes
  // One guaranteed sample per render, whatever the timer managed.
  sample()
}
const wallMs = Number(process.hrtime.bigint() - wallStarted) / 1e6
clearInterval(sampler)

const after = process.memoryUsage()
const sorted = [...times].sort((a, b) => a - b)

process.stdout.write(
  [
    '',
    `renders            ${ITERATIONS} of a 480x320 scene (text, gradient, nested boxes), ${(encoded / ITERATIONS / 1024).toFixed(1)} KiB of png each`,
    `wall               ${(wallMs / 1000).toFixed(2)} s, ${(ITERATIONS / (wallMs / 1000)).toFixed(1)} renders/s`,
    '',
    `per render  p50    ${percentile(sorted, 0.5).toFixed(2)} ms`,
    `            p90    ${percentile(sorted, 0.9).toFixed(2)} ms`,
    `            p99    ${percentile(sorted, 0.99).toFixed(2)} ms`,
    `            max    ${sorted[sorted.length - 1].toFixed(2)} ms`,
    '',
    `baseline    rss    ${abs(baseline.rss)}   (after ${WARMUP} warm-up renders${collected ? ', collected' : ', NOT collected -- run with --expose-gc'})`,
    `            heap   ${abs(baseline.heapUsed)}`,
    '',
    `after       rss    ${mib(after.rss - baseline.rss)}`,
    `            heap   ${mib(after.heapUsed - baseline.heapUsed)}`,
    `            ext    ${mib(after.external - baseline.external)}`,
    `peak        rss    ${mib(peakRss - baseline.rss)}   (${samples} samples: every ${SAMPLE_INTERVAL_MS} ms and once per render, so a lower bound)`,
    '',
  ].join('\n') + '\n',
)

await new Promise(resolve => setTimeout(resolve, IDLE_MS))
const idleCollected = collect()
// A second pass: V8 frees some things only on the collection after the one that
// made them unreachable, so a single call can report memory that one more call
// releases -- which reads as a leak and is not one.
collect()
const idle = process.memoryUsage()

process.stdout.write(
  [
    `idle        rss    ${mib(idle.rss - baseline.rss)}   (after ${IDLE_MS / 1000} s${idleCollected ? ' and two collections' : ', NOT collected -- run with --expose-gc'})`,
    `            heap   ${mib(idle.heapUsed - baseline.heapUsed)}`,
    `            ext    ${mib(idle.external - baseline.external)}`,
    '',
    idleCollected
      ? 'Idle rss is what is retained. Heap and ext near zero with rss above it is the allocator holding pages, not the scene graph.'
      : 'Without --expose-gc the idle figures measure when V8 chose to collect, not what is held.',
    '',
  ].join('\n') + '\n',
)
