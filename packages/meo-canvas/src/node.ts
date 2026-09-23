/**
 * The node factories, and the one object shape they all produce. Nothing crosses
 * into native code here: `Root` encodes the finished tree in one pass.
 * @packageDocumentation
 */

import { PROPERTY_TABLES, render } from './arena.js'
import { STYLE_KEYS } from './style.js'
import type { Color, Gradient, Style } from './style.js'

/** What a node draws. */
export type NodeKind = 'box' | 'text' | 'image' | 'path'

/** Where an image's bytes come from. */
export type ImageSource =
  | {
      /** A path on the machine doing the rendering, read at render time. */
      readonly path: string
    }
  | {
      /**
       * A remote address, fetched at render time.
       *
       * **Refused unless the renderer was built to fetch.** The core is
       * fetch-free by default and answers a URL with an error rather than
       * reaching the network on an input's say-so; the CLI needs `--features
       * net` for the same reason.
       */
      readonly url: string
      /**
       * Merged over the scene-wide `httpOptions` on `Root` for this source alone.
       *
       * **Merged rather than replacing**, per key, and `headers` per header
       * name — so a scene-wide `Authorization` survives a source that only sets
       * an `Accept`.
       *
       * **`signal` is the exception: it composes** with the scene's, so a signal
       * set here can only make this fetch stop sooner, never let it outlive an
       * abort at the root.
       *
       * **On the source and not on the node**, because a URL can appear in three
       * places: an image's `src`, a `backgroundImage` and a `mask`.
       */
      readonly httpOptions?: RequestInit
    }
  | {
      /** The encoded image itself, when the caller already holds it. */
      readonly bytes: Uint8Array
    }

/** One run of a rich-text node, with a style of its own. */
export interface TextSegment {
  /** The run's text. */
  readonly text: string
  /**
   * Styling for this run alone, overriding the node's.
   *
   * Optional, because a caller writes these by hand and most runs in a rich
   * text are unstyled — requiring `style: undefined` on each of them is a word
   * per run that says nothing. {@link SceneNode} carries every key for the
   * opposite reason: it is built by this module and read by the encoder, and a
   * fixed shape is what lets that be a field read rather than a lookup.
   */
  readonly style?: Style | undefined
}

/**
 * One node of the tree.
 *
 * **Every field is present on every node**, `undefined` where it does not
 * apply, and always in this order. That is not tidiness: a node that sometimes
 * carries `src` and sometimes does not gives V8 two hidden classes for one
 * shape, and every property read in the encoder deoptimises to a megamorphic
 * lookup. One shape, one hidden class, and the encoder's reads stay inline.
 *
 * The factories are the only thing that should build one, for that reason —
 * an object literal written by hand is one key order away from a second class.
 */
export interface SceneNode {
  /** What this node draws. */
  readonly kind: NodeKind
  /** How it is styled, or `undefined` for a node that sets nothing. */
  readonly style: Style | undefined
  /** Its children, in paint order before `zIndex` applies. */
  readonly children: readonly SceneNode[] | undefined
  /** A name carried through for diagnostics, which the renderer never reads. */
  readonly name: string | undefined
  /**
   * A text node's paragraph properties, which are not style and do not inherit.
   *
   * Separate from {@link SceneNode.style} because the scene keeps them
   * separate: `maxLines` and `ellipsis` describe the block, not the glyphs, and
   * nothing inherits them from a parent.
   */
  readonly paragraph: ParagraphOptions | undefined
  /**
   * A text node's content as markup, to be parsed by the renderer.
   *
   * Set by {@link Text} and left `undefined` by {@link RichText}, which is how
   * the two are told apart on the wire: rich text of one run is otherwise
   * byte-identical to plain text of one run, and the decoder would have to
   * guess. Parse everything and `RichText` can no longer carry a literal `<`;
   * parse nothing and a caller gets no rich text at all.
   */
  readonly markup: string | undefined
  /** The runs of a text node, built by the caller and not interpreted. */
  readonly segments: readonly TextSegment[] | undefined
  /** Where an image node's bytes come from. */
  readonly src: ImageSource | undefined
  /** A path node's SVG `d` attribute. */
  readonly d: string | undefined
}

/**
 * Builds a node with every key present: the one place a `SceneNode` is
 * constructed, so the package has one key order and one hidden class.
 */
function node(
  kind: NodeKind,
  style: Style | undefined,
  children: readonly SceneNode[] | undefined,
  name: string | undefined,
  paragraph: ParagraphOptions | undefined,
  markup: string | undefined,
  segments: readonly TextSegment[] | undefined,
  src: ImageSource | undefined,
  d: string | undefined,
): SceneNode {
  return { kind, style, children, name, paragraph, markup, segments, src, d }
}

/**
 * Anything that can sit inside a container.
 *
 * `false`, `true`, `null` and `undefined` render nothing, and `''` becomes an
 * empty text node that draws nothing, so every spelling of a conditional reads as
 * it does in JSX: `cond && Text('…')`, `cond ? Text('…') : null` and
 * `cond || node`. React 19.2.8 skips the same values, measured rather than assumed.
 *
 * **A number is text, `0` included.** React renders `0` as the text `0` rather
 * than skipping it, and so does this, so `items.length && …` shows a zero. `NaN`
 * and `0n` render their spellings too. An empty array flattens to nothing.
 */
export type Child = SceneNode | string | number | bigint | boolean | null | undefined

/** One child, or many. */
export type Children = Child | readonly Child[]

/**
 * What every container factory accepts: its style, flat, plus its children.
 *
 * The style properties are the props, and the props object is stored as the
 * style without a copy: `children` and `name` are not style properties, so the
 * encoder never reads them.
 *
 * **A misspelt property is caught at compile time only in a literal.**
 * TypeScript checks excess properties on a fresh object literal alone, so
 * `Box({ marginLeft: 4 })` is refused while `Box({ ...held })` is not; the
 * factory refuses the unknown key at run time instead. Naming the type where a
 * spread is unavoidable — `const base: ContainerProps = { … }` — catches it earlier.
 */
export type ContainerProps = Style & {
  /** Its children, drawn in order. A single child need not be wrapped. */
  readonly children?: Children
  /** A name carried through for diagnostics. */
  readonly name?: string
}

/**
 * The array every container with no surviving child shares.
 *
 * One frozen array rather than a fresh `[]` each time: a conditional subtree
 * that renders nothing is common, and none of them can tell the difference.
 */
const NO_CHILDREN: readonly SceneNode[] = Object.freeze([])

/**
 * Whether an entry in a children list or a segment list renders nothing: one
 * predicate, since it is the same question of both lists. `0` is not among them;
 * see {@link Child}.
 */
function ignorable(value: unknown): value is boolean | null | undefined | ((...args: never[]) => unknown) | symbol {
  // `''` is absent because an empty text node already renders like no child at
  // all. A function or symbol is skipped rather than refused, as React skips it;
  // React also warns, and the only warning channel here is about images.
  return value === false || value === true || value === undefined || value === null || typeof value === 'function' || typeof value === 'symbol'
}

/**
 * A string, number or bigint child, as React renders it: a one-segment
 * `RichText`, since `Text` parses markup and would reinterpret a caller's `<`
 * (`a text child is literal, not markup`). `String()` is React's conversion.
 */
function textChild(value: string | number | bigint): SceneNode {
  return RichText([{ text: String(value) }])
}

/**
 * One child as a node: a string or number becomes text, a node stays itself, and
 * a plain object throws, as React refuses one.
 */
function asNode(child: Child): SceneNode {
  if (typeof child === 'string' || typeof child === 'number' || typeof child === 'bigint') {
    return textChild(child)
  }
  if (isNode(child)) return child
  throw new TypeError(`a child is ${typeof child === 'object' ? 'an object' : String(child)}; it takes a node, a string or a number`)
}

/**
 * Whether a value is one of this package's nodes: `kind` is the discriminator,
 * and every factory sets it.
 */
function isNode(value: unknown): value is SceneNode {
  return typeof value === 'object' && value !== null && 'kind' in value
}

/**
 * The children a container actually has. Absent stays `undefined`; anything else
 * becomes an array, and an array of nodes is handed through without allocating.
 */
function toChildren(children: Children | undefined): readonly SceneNode[] | undefined {
  if (children === undefined) return undefined
  if (ignorable(children)) return NO_CHILDREN
  if (!Array.isArray(children)) return [asNode(children as Child)]

  const many = children as readonly Child[]
  // `every` with a type guard narrows the array, so neither branch needs a cast.
  const plain = many.every(child => isNode(child))
  if (plain) return many
  return many.filter(child => !ignorable(child)).map(child => asNode(child))
}

/**
 * Compiles only when `T` is `never`. The `extends never` constraint does the work:
 * `const _: Leftover = undefined as never` cannot fail, since `never` fits anything.
 */
function noPropLeftOver<T extends never>(_leftOver?: T): void {}

/**
 * Every key {@link ContainerProps} accepts: `Style`'s, proved against `keyof Style`
 * in `style.ts`, plus `children` and `name`, which are written by hand and so
 * carry a proof of their own below.
 */
const CONTAINER_KEYS = [...STYLE_KEYS, 'children', 'name'] as const satisfies readonly (keyof ContainerProps)[]

/* The proof that CONTAINER_KEYS is exactly `keyof ContainerProps`. */
noPropLeftOver<Exclude<keyof ContainerProps, (typeof CONTAINER_KEYS)[number]>>()

/**
 * Refuses a props key the factory does not have, naming it. Checked at run time
 * because excess-property checking fires only on a fresh object literal, not on a
 * variable, a spread or `JSON.parse`.
 */
function checkProps(props: object, allowed: ReadonlySet<string>, what: string): void {
  // The thing before its keys: `Object.keys` of a string, a number or an array
  // reports nonsense or nothing, and `Box(42)` would build a default box.
  if (typeof props !== 'object' || props === null || Array.isArray(props)) {
    throw new TypeError(`${what} takes a props object; it was given ${render(props)}`)
  }
  for (const key of Object.keys(props)) {
    if (allowed.has(key)) continue
    throw new TypeError(`${what} has no property ${JSON.stringify(key)}`)
  }
}

/**
 * Every key {@link ParagraphProps} declares, apart from {@link TEXT_KEYS} so a
 * missing key can be traced to the source it came from.
 */
const PARAGRAPH_KEYS = ['maxLines', 'ellipsis'] as const satisfies readonly (keyof ParagraphProps)[]

/* The proof that PARAGRAPH_KEYS is exactly `keyof ParagraphProps`. */
noPropLeftOver<Exclude<keyof ParagraphProps, (typeof PARAGRAPH_KEYS)[number]>>()

/** Every key {@link TextProps} accepts: `Style`, the paragraph options, and `name`. */
const TEXT_KEYS = [...STYLE_KEYS, ...PARAGRAPH_KEYS, 'name'] as const satisfies readonly (keyof TextProps)[]

/* The proof that TEXT_KEYS is exactly `keyof TextProps`. */
noPropLeftOver<Exclude<keyof TextProps, (typeof TEXT_KEYS)[number]>>()

/** Every key {@link PathProps} accepts. */
const PATH_KEYS = [
  ...STYLE_KEYS,
  'd',
  'viewBox',
  'preserveAspectRatio',
  'fill',
  'stroke',
  'lineWidth',
  'fillRule',
  'lineCap',
  'lineJoin',
  'lineDash',
  'lineDashOffset',
  'name',
] as const satisfies readonly (keyof PathProps)[]

/* The proof that PATH_KEYS is exactly `keyof PathProps`. */
noPropLeftOver<Exclude<keyof PathProps, (typeof PATH_KEYS)[number]>>()

/** Every key {@link ImageProps} accepts. */
const IMAGE_KEYS = [...STYLE_KEYS, 'src', 'name'] as const satisfies readonly (keyof ImageProps)[]

/* The proof that IMAGE_KEYS is exactly `keyof ImageProps`. */
noPropLeftOver<Exclude<keyof ImageProps, (typeof IMAGE_KEYS)[number]>>()

/** {@link IMAGE_KEYS} as a set, built once. */
const IMAGE_KEY_SET: ReadonlySet<string> = new Set(IMAGE_KEYS)

/** {@link PATH_KEYS} as a set, built once. */
const PATH_KEY_SET: ReadonlySet<string> = new Set(PATH_KEYS)

/** {@link TEXT_KEYS} as a set, built once. */
const TEXT_KEY_SET: ReadonlySet<string> = new Set(TEXT_KEYS)

/** {@link CONTAINER_KEYS} as a set, built once. */
const CONTAINER_KEY_SET: ReadonlySet<string> = new Set(CONTAINER_KEYS)

/**
 * The container-shaped half of a wider props object, for `Root`, whose props carry
 * surface options beside the page's style. Filtered through {@link CONTAINER_KEYS},
 * which is proved equal to `keyof ContainerProps`, so a new style property is
 * carried without anyone adding it here.
 */
export function containerPropsOf(props: object): ContainerProps {
  const kept: Record<string, unknown> = {}
  for (const [key, value] of Object.entries(props)) {
    if (CONTAINER_KEY_SET.has(key)) kept[key] = value
  }
  return kept
}

/**
 * A plain container, laying its children out as a row: CSS's `display: flex`
 * rather than Yoga's column.
 *
 * **The display is named rather than inherited.** The scene's default is `block`,
 * as a browser gives a `<div>`, and a `Box` inheriting it would silently stop
 * honouring `gap`, `alignItems` and `justifyContent`. Naming it costs a spread:
 * 0.03 to 0.08 microseconds per container, 3.1 ms across a hundred thousand,
 * against 90 ms to build and 8 to 22 ms to render six thousand.
 */
export function Box(props: ContainerProps = {}): SceneNode {
  checkProps(props, CONTAINER_KEY_SET, 'Box')
  return node('box', { display: 'flex', ...props }, toChildren(props.children), props.name, undefined, undefined, undefined, undefined, undefined)
}

/**
 * A container whose children run horizontally.
 *
 * ```ts
 * import { Row, Text } from 'meo-canvas'
 *
 * const card = Row({
 *   gap: 16,
 *   padding: 24,
 *   backgroundColor: '#101014',
 *   children: [Text('Ukasyah', { fontSize: 24, fontWeight: 'bold' })],
 * })
 * ```
 */
export function Row(props: ContainerProps = {}): SceneNode {
  checkProps(props, CONTAINER_KEY_SET, 'Row')
  return node('box', withDirection(props, 'row'), toChildren(props.children), props.name, undefined, undefined, undefined, undefined, undefined)
}

/** A container whose children run vertically. */
export function Column(props: ContainerProps = {}): SceneNode {
  checkProps(props, CONTAINER_KEY_SET, 'Column')
  return node('box', withDirection(props, 'column'), toChildren(props.children), props.name, undefined, undefined, undefined, undefined, undefined)
}

/** A container whose children are placed on a grid. */
export function Grid(props: ContainerProps = {}): SceneNode {
  checkProps(props, CONTAINER_KEY_SET, 'Grid')
  const style: Style = { display: 'grid', ...props }
  return node('box', style, toChildren(props.children), props.name, undefined, undefined, undefined, undefined, undefined)
}

/**
 * The caller's props with the display and direction the factory names, spread
 * first so the caller's own values win. `display` is named rather than inherited,
 * as in {@link Box}.
 */
function withDirection(props: ContainerProps, flexDirection: 'row' | 'column'): Style {
  return { display: 'flex', flexDirection, ...props }
}

/**
 * Properties of a paragraph as a whole, which do not inherit.
 *
 * Held apart from {@link Style} because the scene holds them apart: these
 * describe the block of text, and a child cannot inherit them from a parent the
 * way it inherits a font size.
 */
export interface ParagraphOptions {
  /** How many lines to draw before the text is truncated. Unset draws them all. */
  readonly maxLines?: number
  /**
   * What a truncated last line ends with, **resolved**.
   *
   * Only read when {@link ParagraphOptions.maxLines} truncates something. Unset
   * truncates without a marker.
   *
   * A caller writes {@link ParagraphProps.ellipsis}, which also takes a
   * boolean; by the time it reaches a node it is the marker itself or nothing.
   * The scene carries what will be drawn rather than which spelling asked for
   * it, because no measurer, line-breaker or painter reads the difference.
   */
  readonly ellipsis?: string
}

/**
 * The marker a truncated line ends with when the caller writes `true`: U+2026
 * HORIZONTAL ELLIPSIS, one glyph rather than three full stops.
 *
 * **Measured rather than assumed.** Chrome's `text-overflow: ellipsis`, read in
 * Helvetica at 40px, puts its three dots 10px apart across a 31px span, which is
 * exactly a literal `…`; three full stops sit 7px apart across 26px. The
 * repository's own Oswald draws the two identically, so it cannot tell them apart.
 *
 * The Rust surface spells the same thing `scene::DEFAULT_ELLIPSIS`.
 */
export const DEFAULT_ELLIPSIS = '\u2026'

/**
 * What a caller may write for a paragraph, before a boolean is resolved.
 *
 * Held apart from {@link ParagraphOptions} because the two are different
 * questions: this is the spelling a caller is allowed, that is what the node
 * ends up carrying.
 */
export interface ParagraphProps {
  /** How many lines to draw before the text is truncated. Unset draws them all. */
  readonly maxLines?: number
  /**
   * What a truncated last line ends with.
   *
   * `true` uses {@link DEFAULT_ELLIPSIS}, the character CSS uses. A string
   * replaces it — a longer one simply leaves the text less room. `false`, an
   * empty string and leaving it unset all truncate without a marker.
   */
  readonly ellipsis?: boolean | string
}

/** What a text node accepts beyond its content: its style, flat. */
export type TextProps = Style &
  ParagraphProps & {
    /** A name carried through for diagnostics. */
    readonly name?: string
  }

/**
 * The marker `ellipsis` asks for, or `undefined` for none. An empty string is no
 * marker rather than an empty one, since the two draw the same picture.
 */
function markerOf(ellipsis: boolean | string | undefined): string | undefined {
  if (ellipsis === true) return DEFAULT_ELLIPSIS
  if (ellipsis === false || ellipsis === undefined || ellipsis === '') return undefined
  return ellipsis
}

/**
 * The paragraph properties of `props`, or `undefined` when none survive. A key is
 * added only when it has a value, since `exactOptionalPropertyTypes` tells an
 * explicit `undefined` from an absent key; `ellipsis: false` resolves to none.
 */
function paragraphOf(props: TextProps): ParagraphOptions | undefined {
  const paragraph: { maxLines?: number; ellipsis?: string } = {}
  if (props.maxLines !== undefined) paragraph.maxLines = props.maxLines
  const marker = markerOf(props.ellipsis)
  if (marker !== undefined) paragraph.ellipsis = marker
  return paragraph.maxLines === undefined && paragraph.ellipsis === undefined ? undefined : paragraph
}

/**
 * A run of text.
 *
 * The content is the first argument rather than a key, so it cannot be
 * forgotten — a `Text` with no text is not a thing worth being able to write.
 *
 * ```ts
 * import { Text } from 'meo-canvas'
 *
 * const name = Text('Ukasyah', { fontSize: 24 })
 * ```
 */
export function Text(content: string, props: TextProps = {}): SceneNode {
  // Checked here: `Text` and `RichText` are the two factories whose first argument
  // is not props, so they are the two a caller hands an object to by mistake.
  if (typeof content !== 'string') {
    throw new TypeError(`Text takes its text first and its props second; it was given ${render(content)}`)
  }
  checkProps(props, TEXT_KEY_SET, 'Text')
  return node('text', props, undefined, props.name, paragraphOf(props), content, undefined, undefined, undefined)
}

/**
 * Text made of runs that differ in style.
 *
 * The one case a single string cannot express: a sentence with one word bold.
 * Each segment's own style overrides the node's for that run.
 */
export function RichText(segments: readonly (TextSegment | string | number | bigint | boolean | null | undefined)[], props: TextProps = {}): SceneNode {
  // The same, one step earlier than `Text`'s: a non-list reached
  // `segments.every` and came back carrying that method's name.
  if (!Array.isArray(segments)) {
    throw new TypeError(`RichText takes its segments first and its props second; it was given ${render(segments)}`)
  }
  checkProps(props, TEXT_KEY_SET, 'RichText')
  // The same rule children get, conversion included: a string becomes a run with
  // that text. A list needing neither is handed on as it stands, so the common
  // case allocates nothing (`carries one segment per run when the runs differ`).
  const plain = segments.every(segment => typeof segment === 'object' && segment !== null)
  const runs = plain
    ? segments
    : segments
        .filter(segment => !ignorable(segment))
        .map(segment =>
          typeof segment === 'string' || typeof segment === 'number' || typeof segment === 'bigint' ? { text: String(segment) } : (segment as TextSegment),
        )
  runs.forEach(checkSegment)
  return node('text', props, undefined, props.name, paragraphOf(props), undefined, runs, undefined, undefined)
}

/**
 * The two keys a segment has. Anything else is a mistake with no other reading.
 */
const SEGMENT_KEYS: ReadonlySet<string> = new Set(['text', 'style'])

/**
 * Keys the generated property tables carry, used only to decide whether a
 * suggestion is safe: a key in the tables is certainly a style property that means
 * something on a segment. Not an allowlist, since `objectFit`, `objectPosition` and
 * `frame` are style keys it lacks; `STYLE_KEYS` is what a refusal uses.
 */
const SUGGESTIBLE_KEYS: ReadonlySet<string> = new Set(Object.values(PROPERTY_TABLES).flatMap(properties => properties.flatMap(property => property.keys)))

/**
 * Refuses a segment carrying a key `TextSegment` does not have. Excess-property
 * checking misses `rows.map(r => ({ text: r.label, fontSize: r.size }))`, the case
 * `RichText` exists for, and the mistake is nearly always flat-versus-nested, so the
 * message can state the fix.
 */
function checkSegment(segment: TextSegment, at: number): void {
  for (const key of Object.keys(segment)) {
    if (SEGMENT_KEYS.has(key)) continue
    // Suggested only where certainly right: a key the property tables carry is a
    // style property, and a confidently wrong suggestion is worse than none.
    const suggestion = SUGGESTIBLE_KEYS.has(key) ? ` — did you mean style: { ${key} }?` : ''
    throw new TypeError(`segments[${at}] has no property ${JSON.stringify(key)}; a segment takes text and style${suggestion}`)
  }
}

/** What an image node accepts: its source, and its style, flat. */
export type ImageProps = Style & {
  /**
   * Where the bytes come from.
   *
   * A bare string is a local path. A `{ url }` is fetched by the surface that
   * accepted it, never by the renderer.
   */
  readonly src: string | ImageSource
  /** A name carried through for diagnostics. */
  readonly name?: string
}

/**
 * A raster image.
 *
 * ```ts
 * import { Image } from 'meo-canvas'
 *
 * const avatar = Image({ src: 'avatar.png', width: 64, height: 64, objectFit: 'cover' })
 * ```
 */
export function Image(props: ImageProps): SceneNode {
  checkProps(props, IMAGE_KEY_SET, 'Image')
  const src = typeof props.src === 'string' ? { path: props.src } : props.src
  return node('image', props, undefined, props.name, undefined, undefined, undefined, src, undefined)
}

/**
 * How a path is painted.
 *
 * A CSS colour, a {@link Gradient}, or `'none'` for an unpainted fill or
 * stroke. The three are told apart by shape rather than by a tag the caller
 * writes: a colour is a string, a gradient is an object, and `'none'` is the
 * one string that is neither.
 *
 * `'none'` is the *absent* paint rather than a transparent colour. A
 * transparent fill is a paint that draws nothing, which is a different thing
 * from a path that is not filled at all — only the second leaves the stroke to
 * decide the shape's edge.
 */
export type PathPaint = Color | 'none' | Gradient

/**
 * A path node's own properties, on top of everything a node can be styled with.
 *
 * The geometry lives here rather than in {@link Style} because it is what the
 * node *is* rather than how it looks: a path without `d` draws nothing, where a
 * path without a fill is still a shape.
 */
export type PathProps = Style & {
  /** The SVG `d` attribute, in the node's own coordinates — or in
   * the space of {@link PathProps}'s `viewBox` when one is given. */
  readonly d: string
  /**
   * The coordinate space `d` is written in, as SVG's `viewBox`:
   * `[minX, minY, width, height]`.
   *
   * **Absent means absolute coordinates.** With a box, the path is scaled and
   * centred into the node's resolved size under SVG's default
   * `preserveAspectRatio`, `xMidYMid meet`, so it fits without distorting. This is
   * what lets a path follow a percentage-sized box, which `d` alone cannot.
   *
   * **The drawing scales; the pen does not**, as SVG's `viewBox` with
   * `vector-effect: non-scaling-stroke`: a caller authoring `d` in a unit square
   * wants `lineWidth` to mean pixels.
   *
   * **The node must have a size.** A path node has no intrinsic size, so one with
   * neither a width nor a height gets an empty box and draws nothing.
   */
  readonly viewBox?: readonly [number, number, number, number]
  /**
   * Whether the drawing may be stretched to fill the node.
   *
   * SVG's `preserveAspectRatio`, and **only its `none` value**: absent is the
   * default `xMidYMid meet`, which fits the drawing without distorting it, and
   * `'none'` scales each axis independently so it fills the node exactly.
   *
   * A subset rather than a private spelling, so the other eight alignments
   * stay addable without breaking a caller. `none` is here because a line
   * chart needs it — a plot must fill its box, `meet` preserves aspect, and no
   * viewBox fixes that, since the box's aspect would have to match the node's
   * and that is exactly what is unknown when the drawing is authored.
   *
   * It does **not** distort the pen — see {@link PathProps}'s `viewBox`.
   */
  readonly preserveAspectRatio?: 'none'
  /** How the interior is painted. Defaults to black, as SVG does. */
  readonly fill?: PathPaint
  /** How the outline is painted. Unset draws no stroke. */
  readonly stroke?: PathPaint
  /** How wide the stroke is drawn, in logical pixels. */
  readonly lineWidth?: number
  /** Which side of the winding counts as inside. */
  readonly fillRule?: 'nonzero' | 'evenodd'
  /** How the stroke's ends are drawn. */
  readonly lineCap?: 'butt' | 'round' | 'square'
  /** How the stroke's corners are drawn. */
  readonly lineJoin?: 'bevel' | 'round' | 'miter'
  /** Alternating dash and gap lengths. Empty or unset draws a solid line. */
  readonly lineDash?: readonly number[]
  /** How far into the dash pattern the stroke begins. */
  readonly lineDashOffset?: number
  /** A name carried through for diagnostics. */
  readonly name?: string
}

/**
 * An arbitrary shape from SVG path data.
 *
 * ```ts
 * import { Path } from 'meo-canvas'
 *
 * const tick = Path({ d: 'M2 8 L6 12 L14 3' })
 * ```
 */
export function Path(props: PathProps): SceneNode {
  checkProps(props, PATH_KEY_SET, 'Path')
  return node('path', props, undefined, props.name, undefined, undefined, undefined, undefined, props.d)
}

/**
 * The keys every node carries, in factory order; exported so a test asserts the
 * shape, since a second hidden class is invisible until something is profiled.
 */
export const NODE_KEYS: readonly string[] = ['kind', 'style', 'children', 'name', 'paragraph', 'markup', 'segments', 'src', 'd']
