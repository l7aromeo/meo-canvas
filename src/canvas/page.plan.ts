import type { Children, PageBuilder, PageInfo, RootNodeProps, RootProps } from '@/canvas/canvas.type.js'

/**
 * Frame rate assumed when a caller gives a `duration` without an `fps`.
 *
 * Matches the renderer's own default for `gif` and `apng`, so a render described purely as
 * `duration: 2` and encoded with a bare `toBuffer('gif')` plays for exactly the two seconds asked
 * for. Diverging here would produce an animation whose wall-clock length silently disagreed with
 * the number it was described by.
 */
export const DEFAULT_FPS = 30

/** A render always has at least one page; `pages: 0` describes nothing that can be drawn. */
const MIN_PAGES = 1

/**
 * `progress` for a render of a single page.
 *
 * The usual `index / (count - 1)` divides by zero here. Zero is the honest answer: a lone page is
 * the start of its own timeline, and it lets `pages: 1` behave exactly like a still render rather
 * than handing the builder a `NaN` that would surface later as an invalid layout value.
 */
const SINGLE_PAGE_PROGRESS = 0

/** The subset of {@link RootProps} that decides how many pages a render produces. */
type PagePlanProps = Pick<RootProps, 'children' | 'pages' | 'duration' | 'fps'>

/** Narrows `children` to the builder form. `Children` has no function member, so this is exact. */
export function isPageBuilder(children: PagePlanProps['children']): children is PageBuilder {
  return typeof children === 'function'
}

/**
 * Works out how many pages a render produces, rejecting every contradictory combination.
 *
 * The rules are enforced here rather than at the call site because the type system cannot reach
 * JavaScript callers, `as any`, or props that arrive over the worker boundary. Every rejection
 * below has a matching compile-time error in the `Root` overloads; this is the half that survives
 * an untyped caller.
 */
export function resolvePageCount(props: PagePlanProps): number {
  const { children, pages, duration, fps } = props
  const paged = isPageBuilder(children)

  if (!paged) {
    // `pages`, `duration` and `fps` describe a sequence, and there is nothing to sequence without
    // a builder: static children render once, however many pages were asked for.
    const named = (['pages', 'duration', 'fps'] as const).filter(key => props[key] !== undefined)
    if (named.length > 0) {
      throw new Error(`[canvas] ${named.join(', ')} require \`children\` to be a function of (page) — static children render a single page`)
    }
    return MIN_PAGES
  }

  if (pages !== undefined && duration !== undefined) {
    throw new Error('[canvas] `pages` and `duration` both set a page count — pass one, not both')
  }

  if (pages !== undefined) {
    if (!Number.isFinite(pages) || !Number.isInteger(pages)) {
      throw new Error(`[canvas] \`pages\` must be a whole number (got ${pages})`)
    }
    if (pages < MIN_PAGES) {
      throw new Error(`[canvas] \`pages\` must be at least ${MIN_PAGES} (got ${pages})`)
    }
    return pages
  }

  if (duration !== undefined) {
    if (!Number.isFinite(duration) || duration <= 0) {
      throw new Error(`[canvas] \`duration\` must be greater than 0 seconds (got ${duration})`)
    }
    const rate = resolveFps(fps)
    // Rounded up so the final, partially covered instant still gets a page: 0.55s at 10fps is six
    // pages, not five, and the animation lasts at least as long as it claims to.
    return Math.ceil(duration * rate)
  }

  throw new Error('[canvas] a `children` function needs `pages` or `duration` to say how many pages to render')
}

/** Validates and defaults the frame rate used to derive {@link PageInfo.time}. */
export function resolveFps(fps: number | undefined): number {
  if (fps === undefined) return DEFAULT_FPS
  if (!Number.isFinite(fps) || fps <= 0) {
    throw new Error(`[canvas] \`fps\` must be greater than 0 (got ${fps})`)
  }
  return fps
}

/**
 * Builds the descriptor handed to the page builder.
 *
 * `progress` and `time` answer different questions and neither replaces the other: `progress` is
 * position along the sequence, which is what interpolation and easing want, while `time` is
 * elapsed seconds, which is what physics integration needs. Both are derived here so a builder
 * never has to restate the frame rate it was configured with.
 */
export function pageInfoAt(index: number, count: number, fps: number): PageInfo {
  return {
    index,
    count,
    progress: count > MIN_PAGES ? index / (count - 1) : SINGLE_PAGE_PROGRESS,
    time: index / fps,
  }
}

/**
 * Reduces render props to what a single-page `RootNode` accepts.
 *
 * The page props are dropped because a node draws one page and has no use for them, and the guard
 * makes the narrowing honest rather than asserted: a builder reaching here would mean the sequence
 * was never resolved, and drawing it would put a function where children belong.
 */
export function asNodeProps(props: RootProps): RootNodeProps {
  const { children, pages: _pages, duration: _duration, fps: _fps, pagedChildren: _pagedChildren, ...rest } = props

  if (isPageBuilder(children)) {
    throw new Error('[canvas] a `children` function reached a single-page render — resolve it with planPages() first')
  }

  return { ...rest, children }
}

/**
 * Runs the page builder once per page, in order, and returns the resulting trees.
 *
 * Returns `null` for still content, which is how callers tell "one page, render the children as
 * given" apart from "one page, produced by a builder".
 *
 * The builder is awaited sequentially rather than through `Promise.all`. Concurrency would let a
 * builder that loads per-page data issue every request at once — a burst this library deliberately
 * avoids elsewhere — and the order of the returned array is load-bearing: it is the page order.
 */
export async function planPages(props: PagePlanProps): Promise<(Children | Children[])[] | null> {
  const { children } = props
  if (!isPageBuilder(children)) {
    // Still content: validated for contradictory page props, then left alone.
    resolvePageCount(props)
    return null
  }

  const count = resolvePageCount(props)
  const fps = resolveFps(props.fps)

  const pages: (Children | Children[])[] = []
  for (let index = 0; index < count; index++) {
    pages.push(await children(pageInfoAt(index, count, fps)))
  }
  return pages
}
