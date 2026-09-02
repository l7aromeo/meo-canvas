// What a render costs in time and in memory, measured through the shipped
// surface rather than through the core.
//
// `cargo bench` already times the pipeline in Rust. This asks a different
// question that criterion cannot: what a long-lived Node process holding this
// addon looks like after a few thousand renders — whether it settles, and how
// far above where it started.
//
// # What each number is, and what it is not
//
// **RSS is not comparable between machines.** It counts the addon's own ~51 MB
// mapping, the Skia allocations behind it and V8's heap in one figure, and the
// mapping alone differs with the build. Every number here is therefore reported
// as a delta from a baseline taken after the addon is loaded and one warm-up
// render has run, so the constant part is subtracted rather than reported as if
// it were a cost per render.
//
// **Peak is sampled, so it is a lower bound**, and it is sampled two ways. A
// timer at {@link SAMPLE_INTERVAL_MS} catches a spike inside a render; a read
// after every render catches the rest. The timer alone was not enough — a
// native call holds the event loop for most of a render, so the first version
// of this took **zero** timer samples in a 2.7 s run and reported a peak of
// `+0.0 MiB` beside a final `+7.7 MiB`. A peak below the figure it bounds is
// the instrument saying it never looked, so the sample count is printed: read
// it before reading the peak.
//
// It is still not an allocator high-water mark. A peak equal to the final RSS
// means "nothing bigger was seen", not "nothing bigger happened".
//
// **Idle is the one that answers "does it leak".** Memory still held after an
// explicit collection and {@link IDLE_MS} of quiet is retained, not garbage
// awaiting a collector that had not run. Without the forced collection the
// reading measures V8's laziness — a process that allocated nothing at all
// would still show a high RSS if the last thing it did was allocate.
//
// **Wall time is reported as percentiles.** A mean over renders hides the shape
// that matters here: whether the slow tail is a few percent or a third of them.
//
// # A reading, so a later one has something to be read against
//
// On an M-series mac, release addon, this scene: 75 renders/s, p50 13.3 ms,
// p99 14.5 ms. Idle rss above baseline was **+7.0 MiB after 200 renders and
// +12.7 MiB after 1000** -- five times the work for 1.8 times the memory. That
// shape is what says it is a bounded cache filling rather than a leak; a leak
// would have been near +35 MiB, and the two points are what distinguish them.
// **One measurement could not have.**

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
 * A scene with something of each kind in it.
 *
 * Text, a gradient and nested boxes rather than one rectangle: a scene that
 * exercises one pass says nothing about the passes it skips, and text is the
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
