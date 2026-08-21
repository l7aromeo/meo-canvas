/**
 * Where a render spends its time, by path.
 *
 * Every case draws the same number of nodes on the same surface, so the only variable is which
 * code path the node takes. What matters is the ratio to `baseline`: a case at 3x is doing three
 * times the work of a plain box for the same picture, and that is where an optimisation pays.
 *
 * Run:
 *   bun run bench                     every case
 *   bun run bench text                cases whose name contains "text"
 *   bun run bench --json > out.json   for comparing two revisions
 *
 * Timings are medians of repeated runs after a warm-up, since the first render of a process pays
 * for module loading and the font cache.
 */
import { Root } from '@/canvas/root.canvas.js'
import { Box, Column, Row } from '@/canvas/layout.canvas.js'
import { Text } from '@/canvas/text.canvas.js'
import { Image } from '@/canvas/image.canvas.js'
import { Grid, GridItem } from '@/canvas/grid.canvas.js'
import { Chart } from '@/canvas/chart.canvas.js'
import { Style } from '@/constant/common.const.js'
import type { BoxProps, CanvasElement, RootProps } from '@/canvas/canvas.type.js'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..')
const FONT = join(ROOT, 'tests/fixtures/fonts/Roboto-Regular.ttf')
const IMAGE = join(ROOT, 'tests/fixtures/images/objectfit-40x20.png')
const FONT_FAMILY = 'BenchRoboto'

/** The page every case draws onto. Big enough that per-pixel work shows, small enough to iterate. */
const WIDTH = 900
const HEIGHT = 680
const SCALE = 2

/** Nodes per case. The same for every case, so counts never explain a difference. */
const NODES = 24

/** Thrown away, to pay for module loading, font registration and the first surface. */
const WARMUP_RUNS = 2

/**
 * Renders thrown away before any case is measured.
 *
 * Without these the first case measured carries the process's own start-up — which made a page of
 * plain boxes look more expensive than the same boxes with rounded corners, and every ratio after
 * it too small.
 */
const GLOBAL_WARMUP_RUNS = 3

/** Kept. The fastest is reported — see `measure`. */
const SAMPLED_RUNS = 7

interface Case {
  name: string
  /** Marks a case as its own baseline group, for cases that cannot share the common one. */
  group?: string
  build: () => Partial<RootProps>
}

const rootBase = {
  fonts: [{ family: FONT_FAMILY, paths: [FONT] }],
  workerMode: false,
  gpu: false,
  width: WIDTH,
  height: HEIGHT,
  scale: SCALE,
  backgroundColor: '#223344',
} satisfies Partial<RootProps>

/** A page of `NODES` panels, each carrying whatever the case is measuring. */
const panels = (props: Partial<BoxProps>, children?: (index: number) => CanvasElement[]): Partial<RootProps> => ({
  ...rootBase,
  children: [
    Box({
      width: WIDTH,
      height: HEIGHT,
      padding: 8,
      children: [
        Column({
          children: Array.from({ length: NODES }, (_, index) =>
            Box({ width: 380, height: 22, margin: 2, backgroundColor: 'rgba(255,255,255,0.35)', ...props, children: children?.(index) }),
          ),
        }),
      ],
    }),
  ],
})

/**
 * A small backdrop over a source of `ops` drawn boxes.
 *
 * The backdrop covers a sliver of the page, so the only variable is how much drawing sits beneath
 * it. That is the axis a clipped draw of a nested source is charged on, and no other case here
 * varies it — every one of them draws `NODES` panels, which holds source complexity fixed by
 * construction.
 *
 * Read each against its own control: the backdrop's cost is the difference, which leaves out the
 * page simply having more to draw. What matters is then how that difference moves with the source.
 * On 5.6.5 it is 7.6ms at both 40 and 2k ops and 11.3ms at 20k, so 3.7ms of it is charged on the
 * source rather than on the region. If the cost is the region, that excess is gone and all three
 * differences land together.
 *
 * The backdrop is large deliberately. At 180x24 the difference was near 1.3ms, which three runs
 * could not separate from noise; at this size the same three runs hold every figure inside 0.4ms.
 */
const backdropOverSource = (ops: number, backdrop = true): Partial<RootProps> => ({
  ...rootBase,
  children: [
    Box({
      width: WIDTH,
      height: HEIGHT,
      children: [
        Box({
          positionType: Style.PositionType.Absolute,
          position: { Top: 0, Left: 0 },
          width: WIDTH,
          height: HEIGHT,
          children: [
            Row({
              flexWrap: Style.Wrap.Wrap,
              children: Array.from({ length: ops }, (_, index) => Box({ width: 4, height: 4, backgroundColor: index % 2 ? '#e11d48' : '#0066cc' })),
            }),
          ],
        }),
        // Dropped for the control, so the pair differs by the backdrop and nothing else.
        ...(backdrop
          ? [
              Box({
                positionType: Style.PositionType.Absolute,
                position: { Top: 40, Left: 40 },
                width: 420,
                height: 140,
                borderRadius: 6,
                backdropFilter: 'blur(12px)',
                backgroundColor: 'rgba(255,255,255,0.2)',
              }),
            ]
          : []),
      ],
    }),
  ],
})

/**
 * The same picture on a page large enough for per-page cost to show.
 *
 * Every other case draws 900x680. A cost charged on the page rather than on the node is invisible
 * at that size: the v5.6.5 bump measured 21% faster on a 2000x1600 page and nothing at all here,
 * and the bench was what was wrong.
 */
const largePageBackdrops = (): Partial<RootProps> => ({
  ...rootBase,
  width: 2000,
  height: 1600,
  children: [
    Box({
      width: 2000,
      height: 1600,
      children: [
        Box({
          positionType: Style.PositionType.Absolute,
          position: { Top: 0, Left: 0 },
          width: 2000,
          height: 1600,
          gradient: { type: 'conic', colors: ['#e11d48', '#0066cc', '#00aa00', '#e11d48'] },
        }),
        ...Array.from({ length: 6 }, (_, index) =>
          Box({
            positionType: Style.PositionType.Absolute,
            position: { Top: 100 + index * 220, Left: 80 + index * 60 },
            width: 180,
            height: 90,
            borderRadius: 12,
            backdropFilter: 'blur(8px)',
            backgroundColor: 'rgba(255,255,255,0.2)',
          }),
        ),
      ],
    }),
  ],
})

const CASES: Case[] = [
  { name: 'baseline: plain boxes', build: () => panels({}) },
  { name: 'borderRadius', build: () => panels({ borderRadius: 10 }) },
  { name: 'border 2px', build: () => panels({ border: 2, borderColor: '#ffcc00' }) },
  { name: 'border per edge', build: () => panels({ border: 2, borderColor: { Top: '#f00', Right: '#0f0', Bottom: '#00f', Left: '#ff0' } }) },
  { name: 'borderStyle dashed', build: () => panels({ border: 2, borderColor: '#ffcc00', borderStyle: Style.Border.Dashed }) },
  { name: 'boxShadow outset', build: () => panels({ boxShadow: { offsetY: 4, blur: 10, color: 'rgba(0,0,0,0.6)' } }) },
  { name: 'boxShadow inset', build: () => panels({ boxShadow: { inset: true, offsetY: 4, blur: 10, color: 'rgba(0,0,0,0.6)' } }) },
  {
    name: 'boxShadow x3',
    build: () =>
      panels({
        boxShadow: [
          { offsetY: 2, blur: 4 },
          { offsetY: 6, blur: 12 },
          { offsetY: 10, blur: 20 },
        ],
      }),
  },
  { name: 'gradient linear', build: () => panels({ gradient: { type: 'linear', colors: ['#f80', '#08f'], direction: 'to-right' } }) },
  { name: 'gradient radial', build: () => panels({ gradient: { type: 'radial', colors: ['#f80', '#08f'] } }) },
  { name: 'gradient conic', build: () => panels({ gradient: { type: 'conic', colors: ['#f80', '#08f', '#f80'] } }) },
  { name: 'dither', build: () => ({ ...panels({ gradient: { type: 'linear', colors: ['#0b1220', '#1e2b4a'], direction: 'to-right' } }), dither: true }) },
  { name: 'opacity', build: () => panels({ opacity: 0.75 }) },
  { name: 'transform rotate', build: () => panels({ transform: { rotate: 4 } }) },
  { name: 'overflow hidden', build: () => panels({ overflow: Style.Overflow.Hidden }) },
  { name: 'mask circle', build: () => panels({ mask: { shape: 'circle' } }) },
  { name: 'filter grayscale', build: () => panels({ filter: 'grayscale(1)' }) },
  { name: 'filter blur', build: () => panels({ filter: 'blur(4px)' }) },
  { name: 'mixBlendMode', build: () => panels({ mixBlendMode: Style.BlendMode.Multiply }) },
  { name: 'backdropFilter blur', build: () => panels({ backdropFilter: 'blur(6px)' }) },
  { name: 'backdropFilter + filter', build: () => panels({ backdropFilter: 'blur(6px)', filter: 'grayscale(1)' }) },
  // Paired with a control at each size: the backdrop's own cost is the difference between the two,
  // which is what isolates it from the page simply having more to draw.
  { name: 'source 40 ops, no backdrop', group: 'backdrop-source', build: () => backdropOverSource(40, false) },
  { name: 'backdrop over source 40 ops', group: 'backdrop-source', build: () => backdropOverSource(40) },
  { name: 'source 2k ops, no backdrop', group: 'backdrop-source', build: () => backdropOverSource(2_000, false) },
  { name: 'backdrop over source 2k ops', group: 'backdrop-source', build: () => backdropOverSource(2_000) },
  { name: 'source 20k ops, no backdrop', group: 'backdrop-source', build: () => backdropOverSource(20_000, false) },
  { name: 'backdrop over source 20k ops', group: 'backdrop-source', build: () => backdropOverSource(20_000) },
  { name: 'large page backdrops', group: 'large-page', build: largePageBackdrops },
  { name: 'backgroundImage tiled', build: () => panels({ backgroundImage: { src: IMAGE, size: 20 } }) },
  {
    name: 'text plain',
    build: () => panels({}, index => [Text(`Panel number ${index}`, { fontFamily: FONT_FAMILY, fontSize: 14, color: '#fff' })]),
  },
  {
    name: 'text wrapping',
    build: () =>
      panels({ height: 22 }, () => [
        Text('a longer run of words that has to be wrapped across the panel', { fontFamily: FONT_FAMILY, fontSize: 12, color: '#fff' }),
      ]),
  },
  {
    name: 'text truncated',
    build: () =>
      panels({}, () => [
        Text('a longer run of words that will not fit and must be cut', {
          fontFamily: FONT_FAMILY,
          fontSize: 14,
          color: '#fff',
          maxLines: 1,
          ellipsis: true,
        }),
      ]),
  },
  {
    name: 'text rich tags',
    build: () =>
      panels({}, index => [
        Text(`Panel <b>${index}</b> with <color="#ffcc00">colour</color> and <size="18">size</size>`, {
          fontFamily: FONT_FAMILY,
          fontSize: 14,
          color: '#fff',
        }),
      ]),
  },
  {
    name: 'text decorated',
    build: () => panels({}, index => [Text(`Panel ${index}`, { fontFamily: FONT_FAMILY, fontSize: 14, color: '#fff', textDecoration: 'underline' })]),
  },
  {
    name: 'text stroked',
    build: () =>
      panels({}, index => [Text(`Panel ${index}`, { fontFamily: FONT_FAMILY, fontSize: 14, color: '#fff', textStroke: { width: 2, color: '#000' } })]),
  },
  {
    name: 'text shadowed',
    build: () =>
      panels({}, index => [
        Text(`Panel ${index}`, { fontFamily: FONT_FAMILY, fontSize: 14, color: '#fff', textShadow: { offsetY: 1, blur: 3, color: '#000' } }),
      ]),
  },
  { name: 'image local', build: () => panels({}, () => [Image({ src: IMAGE, width: 40, height: 20 })]) },
  { name: 'image objectFit cover', build: () => panels({}, () => [Image({ src: IMAGE, width: 40, height: 20, objectFit: 'cover' })]) },
  {
    name: 'image dropShadow',
    build: () => panels({}, () => [Image({ src: IMAGE, width: 40, height: 20, dropShadow: { offsetY: 3, blur: 6, color: '#000' } })]),
  },
  {
    name: 'grid',
    group: 'layout',
    build: () => ({
      ...rootBase,
      children: [
        Grid({
          width: WIDTH,
          templateColumns: ['1fr', '1fr', '1fr', '1fr', '1fr', '1fr'],
          gap: 6,
          children: Array.from({ length: NODES }, () => GridItem({ children: [Box({ height: 40, backgroundColor: '#89a' })] })),
        }),
      ],
    }),
  },
  {
    name: 'chart bar',
    group: 'layout',
    build: () => ({
      ...rootBase,
      children: [
        Chart({
          width: WIDTH - 40,
          height: 400,
          type: 'bar',
          fontFamily: FONT_FAMILY,
          data: {
            labels: Array.from({ length: 12 }, (_, index) => `d${index}`),
            datasets: [{ label: 'series', data: Array.from({ length: 12 }, (_, index) => (index % 7) + 1), color: '#4a90d9' }],
          },
        }),
      ],
    }),
  },
  {
    name: 'deep nesting (24 levels)',
    group: 'layout',
    build: () => {
      let node = Box({ width: 40, height: 40, backgroundColor: '#89a' })
      for (let depth = 0; depth < NODES; depth++) node = Box({ padding: 2, backgroundColor: 'rgba(255,255,255,0.04)', children: [node] })
      return { ...rootBase, children: [node] }
    },
  },
  {
    name: 'wide flex row',
    group: 'layout',
    build: () => ({
      ...rootBase,
      children: [
        Row({
          width: WIDTH,
          flexWrap: Style.Wrap.Wrap,
          children: Array.from({ length: NODES * 4 }, () => Box({ width: 60, height: 30, margin: 2, backgroundColor: '#89a' })),
        }),
      ],
    }),
  },
]

/** Cases that change how the whole page is produced rather than what is on it. */
const PAGE_CASES: Array<{ name: string; props: Partial<RootProps>; format?: 'png' | 'webp' | 'jpeg' }> = [
  { name: 'export png', props: {} },
  { name: 'export webp', props: {}, format: 'webp' },
  { name: 'export jpeg', props: {}, format: 'jpeg' },
  { name: 'scale 1', props: { scale: 1 } },
  { name: 'scale 3', props: { scale: 3 } },
  { name: 'gpu', props: { gpu: true } },
]

/**
 * The fastest run, not the average or the median.
 *
 * Every disturbance a benchmark meets — a garbage collection, the scheduler moving the process,
 * another core waking — makes a run slower and none makes it faster, so the minimum is the closest
 * reading to the work itself. Averaging measures the machine's mood as much as the code, and it
 * showed here as rounded corners looking half the price of square ones.
 */
const fastest = (values: number[]) => Math.min(...values)

interface Timing {
  /** Layout, and recording what to draw. This is the library's own work. */
  build: number
  /** Turning that into pixels and encoding them. Mostly the renderer and the codec. */
  raster: number
  total: number
}

/**
 * Times a case, separating the library's work from the renderer's.
 *
 * Drawing is deferred: the calls a render makes cost almost nothing until something asks for
 * pixels, and then the whole picture is rasterised at once. Timing only the total therefore says
 * where the cost *appeared*, not what caused it — a page of plain boxes and one of rounded boxes
 * differ by 2x in PNG encoding alone, with identical drawing.
 */
async function measure(build: () => Partial<RootProps>, format: 'png' | 'webp' | 'jpeg' = 'png'): Promise<Timing> {
  const once = async (): Promise<Timing> => {
    const startedBuild = performance.now()
    const canvas = await Root(build() as Parameters<typeof Root>[0])
    const built = performance.now()
    canvas.toBufferSync(format as 'png')
    const rastered = performance.now()
    if ('release' in canvas) (canvas as { release: () => void }).release()
    return { build: built - startedBuild, raster: rastered - built, total: rastered - startedBuild }
  }

  for (let run = 0; run < WARMUP_RUNS; run++) await once()

  const samples: Timing[] = []
  for (let run = 0; run < SAMPLED_RUNS; run++) samples.push(await once())
  return {
    build: fastest(samples.map(sample => sample.build)),
    raster: fastest(samples.map(sample => sample.raster)),
    total: fastest(samples.map(sample => sample.total)),
  }
}

async function main() {
  const args = process.argv.slice(2)
  const asJson = args.includes('--json')
  const filter = args.find(arg => !arg.startsWith('--'))

  const results: Array<{ name: string; group: string; timing: Timing; ratio: number }> = []

  for (let run = 0; run < GLOBAL_WARMUP_RUNS; run++) await measure(() => panels({}))

  // Measured in the same conditions as everything else, and after the warm-up, or the case that
  // happens to run first is charged for the process starting.
  const baseline = await measure(CASES[0].build)
  const pageBaseline = baseline

  for (const testCase of CASES) {
    if (filter && !testCase.name.includes(filter)) continue
    const timing = testCase.name === CASES[0].name ? baseline : await measure(testCase.build)
    results.push({ name: testCase.name, group: testCase.group ?? 'node', timing, ratio: timing.total / baseline.total })
  }

  for (const pageCase of PAGE_CASES) {
    if (filter && !pageCase.name.includes(filter)) continue
    const timing = await measure(() => ({ ...panels({}), ...pageCase.props }), pageCase.format)
    results.push({ name: pageCase.name, group: 'page', timing, ratio: timing.total / pageBaseline.total })
  }

  if (asJson) {
    console.log(JSON.stringify({ nodes: NODES, width: WIDTH, height: HEIGHT, scale: SCALE, results }, null, 2))
    return
  }

  // The baseline again, last: if it has moved, the machine drifted under the run and every ratio
  // in between should be read with that in mind.
  const baselineAgain = await measure(CASES[0].build)
  const drift = Math.abs(baselineAgain.total - baseline.total) / baseline.total

  console.log(`\n${NODES} nodes on ${WIDTH}x${HEIGHT} @${SCALE}, fastest of ${SAMPLED_RUNS} runs after ${WARMUP_RUNS} warm-ups`)
  console.log(`baseline ${baseline.total.toFixed(1)}ms at the start, ${baselineAgain.total.toFixed(1)}ms at the end — ${(drift * 100).toFixed(0)}% drift`)
  console.log('build = layout and recording the draw; raster = pixels and encoding\n')
  let group = ''
  for (const result of results) {
    if (result.group !== group) {
      group = result.group
      console.log(`  ${group}:`)
    }
    const bar = '#'.repeat(Math.min(30, Math.round(result.ratio * 4)))
    console.log(
      `    ${result.name.padEnd(26)} build ${result.timing.build.toFixed(1).padStart(6)}ms  raster ${result.timing.raster
        .toFixed(1)
        .padStart(6)}ms  total ${result.timing.total.toFixed(1).padStart(6)}ms  x${result.ratio.toFixed(2).padStart(5)}  ${bar}`,
    )
  }
  console.log('\n  x is the cost against a page of plain boxes drawing the same shapes.\n')
}

await main()
