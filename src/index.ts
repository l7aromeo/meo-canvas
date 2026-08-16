export * from '@/constant/common.const.js'
export * from '@/canvas/canvas.type.js'
export { Box, Column, Row, type BoxNode } from '@/canvas/layout.canvas.js'
export { Image } from '@/canvas/image.canvas.js'
export { Path } from '@/canvas/path.canvas.js'
export { Text } from '@/canvas/text.canvas.js'
export { Root, terminate } from '@/canvas/root.canvas.js'
export { GridItem } from '@/canvas/grid.canvas.js'
export { Grid } from '@/canvas/grid.canvas.js'
export { Chart } from '@/canvas/chart.canvas.js'
export { clearDiskCache, setDiskCacheDir } from '@/util/disk.cache.js'

// Animation utilities: pure, stateless, and safe to call from a page builder in any order.
export { easings, cubicBezier, steps, resolveEasing, type Easing, type EasingFn, type EasingName } from '@/animate/easing.js'
export { lerp, mapRange, interpolate, mix, type Animatable } from '@/animate/interpolate.js'
export { parseColor, formatColor, mixColor, isColor, type Rgba } from '@/animate/color.js'
export { spring, springDuration, type SpringConfig, type SpringDurationOptions } from '@/animate/spring.js'
export { track, type Track, type TrackConfig } from '@/animate/track.js'
export { sequence, type SequenceConfig, type SequenceStep } from '@/animate/sequence.js'
export { parallel, type Sampled, type GroupValue } from '@/animate/parallel.js'

/**
 * Re-exported from the renderer so consumers can name them without importing a transitive
 * dependency — a reach that breaks under pnpm's strict layout and Yarn PnP. These already appear in
 * this package's public signatures, so exporting them costs nothing and decouples callers from
 * where they happen to come from.
 */
export type { ExportFormat, ExportOptions, SaveOptions, RenderOptions } from 'meo-skia-canvas'
