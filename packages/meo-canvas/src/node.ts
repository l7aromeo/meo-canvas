/**
 * The node factories, and the object shape they all produce.
 *
 * A factory is a plain function returning a plain object. **Nothing crosses into
 * native code here** — the whole tree is built in JavaScript and `Root` encodes
 * it in one pass, because JavaScript evaluates arguments inside out: writing
 * opcodes as each factory ran would land them post-order where the arena is
 * pre-order.
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
 *
 * The style properties sit directly in the props, as v1's `BoxProps` carries
 * them, rather than under a `style` key. The **stored** node keeps them in one
 * object because that shape is about hidden classes rather than about how a
 * caller writes it — two different questions.
 *
 * @packageDocumentation
 */

import { PROPERTY_TABLES } from './arena.js'
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
 * Builds a node with every key present.
 *
 * The one place a `SceneNode` is constructed, so there is one key order in the
 * package rather than one per factory. A second literal elsewhere is a second
 * hidden class the moment its keys are written in another order.
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
 * `false` and `undefined` are members so `condition && Text('…')` reads the way
 * it does in JSX and renders nothing when the condition fails. v1 allows both
 * for that reason, and a conditional written that way is how its users write
 * one — dropping it would break the idiom rather than tidy it.
 *
 * **`null` and `''` are members for the same reason, measured against React
 * 19.2.8 rather than assumed.** `cond ? <X/> : null` is the other half of that
 * idiom and the spelling a ternary produces, and `''` is what an empty string
 * out of data looks like; React renders nothing for either. Both were type
 * errors that also threw at runtime, so the two spellings of one conditional
 * behaved differently — `&&` worked and `? :` did not.
 *
 * **`true` is a member too, and for a spelling rather than for its own sake.**
 * `cond || node` yields `true` when `cond` is a truthy boolean, which is the
 * same half-of-an-idiom asymmetry as `? :` — and React skips it. Admitting
 * `false` while rejecting `true` would leave one spelling of a conditional
 * working and its neighbour throwing, which is the shape this widening exists
 * to remove.
 *
 * **`0` is deliberately not a member.** React renders it as the text `0`, which
 * is a different decision from skipping it, and a caller who writes
 * `items.length && …` meaning "when there are items" would get a visible zero
 * rather than nothing. It stays an error, and the error is where to say so.
 * `NaN` and `0n` render there too — measured, not assumed — and are out for the
 * same reason. An empty array needs no member: it flattens to nothing already.
 * Skipping `0` silently would make this looser than React in the one
 * direction where looser means quieter.
 */
export type Child = SceneNode | string | number | bigint | boolean | null | undefined

/** One child, or many. */
export type Children = Child | readonly Child[]

/**
 * What every container factory accepts: its style, flat, plus its children.
 *
 * The style properties are the props, as v1 spells them. `children` and `name`
 * are not style properties and no style property is called either, so the
 * encoder — which looks up only the names in its own table — never reads them
 * and the props object is stored as the style without a copy.
 *
 * **A misspelt property is caught in a literal and not in a spread.** TypeScript
 * checks for excess properties only on a fresh object literal, so
 * `Box({ marginLeft: 4 })` is refused while `Box({ ...held })` and
 * `Box(held)` are not. And because the props object **is** the style, and the
 * encoder reads only the names in its own table, a key that reaches it is
 * **dropped rather than refused** — which renders as a plausible wrong picture
 * instead of an error.
 *
 * That is TypeScript's rule rather than this surface's, and the mitigation is
 * knowing it: name the type where a spread is unavoidable —
 * `const base: ContainerProps = { … }` is checked where `const base = { … }`
 * is not.
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
 * Whether an entry in a children list or a segment list renders nothing.
 *
 * **One predicate for both lists**, because it is the same question asked of a
 * container's children and of a paragraph's runs. They disagreed before this:
 * children ignored `false` and `undefined` and segments ignored neither, so one
 * caller building both from data met two behaviours for one mistake.
 *
 * `0` is absent on purpose — see {@link Child}.
 */
function ignorable(value: unknown): value is boolean | null | undefined | ((...args: never[]) => unknown) | symbol {
  // **`''` is deliberately absent.** A string child becomes a text node, so an
  // empty one becomes an empty text node — which renders byte-identically to no
  // children at all, measured. Skipping it as well would be a special case for
  // a value that already disappears.
  //
  // **A function or a symbol is skipped rather than refused**, which is what
  // React does. React also warns; the only warning channel here is typed
  // `ImageWarning`, so there is nowhere to put one that is not about an image.
  // The divergence is in the diagnostic, not in the render.
  return value === false || value === true || value === undefined || value === null || typeof value === 'function' || typeof value === 'symbol'
}

/**
 * A string, number or bigint child, as the text node React renders it as.
 *
 * **A single-segment `RichText`, never `Text`.** `Text`'s content is markup and
 * the renderer parses it, so a caller's angle brackets would be reinterpreted:
 * measured, `Text('<b>bold</b> rest')` draws bolded text at ink 565 where
 * `RichText([{ text: … }])` draws the tags literally at 820. React's text child
 * is literal, and a silent reinterpretation of somebody's data is exactly the
 * failure this change exists to remove.
 *
 * `String()` rather than a format, because that is what React uses -- so
 * `NaN` and `Infinity` render as their spellings and a bigint loses its `n`.
 */
function textChild(value: string | number | bigint): SceneNode {
  return RichText([{ text: String(value) }])
}

/**
 * One child as a node: a string or number becomes text, a node stays itself.
 *
 * **A plain object throws**, which is React's line and the one thing that must
 * not become permissive along with the rest. Functions and symbols never reach
 * here -- they are filtered as ignorable, because React skips them.
 */
function asNode(child: Child): SceneNode {
  if (typeof child === 'string' || typeof child === 'number' || typeof child === 'bigint') {
    return textChild(child)
  }
  if (isNode(child)) return child
  throw new TypeError(`a child is ${typeof child === 'object' ? 'an object' : String(child)}; it takes a node, a string or a number`)
}

/**
 * Whether a value is one of this package's nodes rather than a plain object.
 *
 * **`kind` is the discriminator** and every factory sets it. A plain object
 * reaching a children list is a mistake React refuses outright, and refusing it
 * here means the caller hears about it rather than meeting a decoder error four
 * layers down.
 */
function isNode(value: unknown): value is SceneNode {
  return typeof value === 'object' && value !== null && 'kind' in value
}

/**
 * The children a container actually has: one or many, with the falsy ones gone.
 *
 * Absent stays absent — a container that named no children has `undefined`,
 * not an empty array — and anything else becomes an array, because the node
 * field is one shape and the encoder should not have to ask which.
 *
 * The array a caller passed is handed straight through when every entry is a
 * node, so the common case allocates nothing. A filter runs only when there is
 * something to filter out.
 */
function toChildren(children: Children | undefined): readonly SceneNode[] | undefined {
  if (children === undefined) return undefined
  if (ignorable(children)) return NO_CHILDREN
  if (!Array.isArray(children)) return [asNode(children as Child)]

  const many = children as readonly Child[]
  // The fast path stays allocation-free for the common case: an array that is
  // already all nodes is handed straight through.
  // `every` with a type guard narrows the array, so neither branch needs an
  // assertion — the guard is doing the work the casts used to.
  const plain = many.every(child => isNode(child))
  if (plain) return many
  return many.filter(child => !ignorable(child)).map(child => asNode(child))
}

/**
 * Compiles only when `T` is `never`, and names what is left over when it is not.
 *
 * The `extends never` constraint is what does the work. An assertion written as
 * `const _: Leftover = undefined as never` **cannot fail**, because `never` is
 * assignable to everything.
 */
function noPropLeftOver<T extends never>(_leftOver?: T): void {}

/**
 * Every key {@link ContainerProps} accepts: `Style`'s, plus its own two.
 *
 * **The structural half is where drift lives now.** `STYLE_KEYS` is proved
 * against `keyof Style`, so the sixty-nine cannot go stale; `children` and
 * `name` are written here by hand and would, which is why this list carries a
 * proof of its own rather than borrowing the one next door.
 */
const CONTAINER_KEYS = [...STYLE_KEYS, 'children', 'name'] as const satisfies readonly (keyof ContainerProps)[]

/* The proof that CONTAINER_KEYS is exactly `keyof ContainerProps`. */
noPropLeftOver<Exclude<keyof ContainerProps, (typeof CONTAINER_KEYS)[number]>>()

/**
 * Refuses a props key the factory does not have, naming it.
 *
 * **Checked at run time because the type system checks it almost nowhere.**
 * Excess property checking fires on a fresh object literal and on nothing else,
 * so a props object built from a variable, a spread or `JSON.parse` carries an
 * unknown key straight through and the property is silently ignored -- measured
 * on `Box`, `Text`, `Path` and the paragraph options alike.
 */
function checkProps(props: object, allowed: ReadonlySet<string>, what: string): void {
  for (const key of Object.keys(props)) {
    if (allowed.has(key)) continue
    throw new TypeError(`${what} has no property ${JSON.stringify(key)}`)
  }
}

/**
 * Every key {@link ParagraphProps} declares.
 *
 * Named separately rather than folded into {@link TEXT_KEYS} because
 * `TextProps` composes **three** sources, and a list built from one spread of
 * everything cannot say which source a missing key came from.
 */
const PARAGRAPH_KEYS = ['maxLines', 'ellipsis'] as const satisfies readonly (keyof ParagraphProps)[]

/* The proof that PARAGRAPH_KEYS is exactly `keyof ParagraphProps`. */
noPropLeftOver<Exclude<keyof ParagraphProps, (typeof PARAGRAPH_KEYS)[number]>>()

/** Every key {@link TextProps} accepts: `Style`, the paragraph options, and `name`. */
const TEXT_KEYS = [...STYLE_KEYS, ...PARAGRAPH_KEYS, 'name'] as const satisfies readonly (keyof TextProps)[]

/* The proof that TEXT_KEYS is exactly `keyof TextProps`. */
noPropLeftOver<Exclude<keyof TextProps, (typeof TEXT_KEYS)[number]>>()

/** {@link TEXT_KEYS} as a set, built once. */
const TEXT_KEY_SET: ReadonlySet<string> = new Set(TEXT_KEYS)

/** {@link CONTAINER_KEYS} as a set, built once. */
const CONTAINER_KEY_SET: ReadonlySet<string> = new Set(CONTAINER_KEYS)

/**
 * The container-shaped half of a wider props object.
 *
 * **For callers that legitimately hold more than a container takes.** `Root`'s
 * props carry the surface options — `fonts`, `gpu`, `pages`, `scale` and the
 * rest — alongside the page's own style, and it builds each page by handing
 * that object to {@link Box}. Spreading it whole put ten keys into a node's
 * style that the writer then ignored, which is the same defect this module now
 * refuses from a caller: it was simply ours.
 *
 * Filtering through {@link CONTAINER_KEYS} rather than by naming the surface
 * options is what keeps it correct — that list is proved equal to
 * `keyof ContainerProps`, so a style property added later is carried here
 * without anyone remembering to add it.
 */
export function containerPropsOf(props: object): ContainerProps {
  const kept: Record<string, unknown> = {}
  for (const [key, value] of Object.entries(props)) {
    if (CONTAINER_KEY_SET.has(key)) kept[key] = value
  }
  return kept
}

/**
 * A plain container.
 *
 * Lays its children out as a row, following CSS's `display: flex` rather than
 * Yoga's column.
 *
 * **The display is named rather than inherited.** The scene's default is
 * `block`, which is what a browser gives a `<div>`, so a `Box` that inherited
 * it would stop honouring `gap`, `alignItems` and `justifyContent` without
 * saying so. Naming it costs a spread per container: measured at 0.03 to 0.08
 * microseconds each, 3.1 ms across a hundred thousand, against a build of
 * 90 ms and a render of 8 to 22 ms for a tree of six thousand containers.
 * `Row` and `Column` have always spread for the same kind of reason.
 *
 * **Three orders of magnitude, which is why the number survives the machine it
 * was taken on.** Peers were building at the time; contention that voids a
 * benchmark measuring a few percent cannot reach a conclusion with this much
 * room in it.
 */
export function Box(props: ContainerProps = {}): SceneNode {
  checkProps(props, CONTAINER_KEY_SET, 'Box')
  return node('box', { display: 'flex', ...props }, toChildren(props.children), props.name, undefined, undefined, undefined, undefined, undefined)
}

/** A container whose children run horizontally. */
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
 * The caller's props with the display and flex direction the factory names.
 *
 * The one place this package copies a style, and it copies once per container
 * rather than once per property: `Row` and `Column` mean a direction, and a
 * caller who states one keeps it — spreading the props after the default is
 * what makes the caller's value win.
 *
 * **`display` is named here rather than inherited**, because the scene's
 * default is `block`, which is what a browser gives a `<div>`. A factory
 * called `Row` that laid its children out in a column would be a defect; one
 * that relied on the default for it would be a defect the day the default
 * moved. The same reason `Grid` has always named its own.
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
 * The marker a truncated line ends with when the caller writes `true`.
 *
 * U+2026 HORIZONTAL ELLIPSIS, one glyph rather than three full stops.
 *
 * **Measured rather than assumed.** Chrome's `text-overflow: ellipsis` was read
 * in Helvetica at 40px — deliberately not the repository's own Oswald, where
 * `…` and `...` rasterise to identical ink runs with advances 0.36px apart and
 * cannot tell the two answers apart. Chrome's marker has its three dots 10px
 * apart across a 31px span, which is exactly a literal `…`; three full stops
 * sit 7px apart across 26px. v1 draws the same character for `ellipsis: true`
 * (`src/canvas/text.canvas.ts:1244`), so the API reference and the behavioural
 * one agree.
 *
 * The Rust surface spells the same thing `scene::DEFAULT_ELLIPSIS`, which is
 * that language's idiom for it — Rust has no boolean-or-string union worth
 * having.
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
   *
   * The boolean is v1's spelling (`canvas.type.ts:1543`) and `false` is v1's
   * own applied default, so a ported script that wrote the default explicitly
   * keeps working. Both booleans threw before this took them: the value crossed
   * TypeScript unchecked and the arena refused it at the far end.
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
 * The marker `ellipsis` asks for, or `undefined` for no marker at all.
 *
 * An empty string is `undefined` rather than an empty marker because the two
 * draw the same picture, and because v1 reached that answer through a
 * truthiness guard — a caller who wrote `ellipsis: ''` there got no marker and
 * gets none here.
 */
function markerOf(ellipsis: boolean | string | undefined): string | undefined {
  if (ellipsis === true) return DEFAULT_ELLIPSIS
  if (ellipsis === false || ellipsis === undefined || ellipsis === '') return undefined
  return ellipsis
}

/**
 * The paragraph properties of `props`, or `undefined` when it sets neither.
 *
 * Each key is added only when it has a value, rather than written as
 * `undefined`: `exactOptionalPropertyTypes` is on, and an explicit `undefined`
 * is a different thing from an absent key to every reader here.
 *
 * `undefined` when nothing survives, which is not the same test as the one on
 * the way in: `ellipsis: false` is a value the caller wrote and resolves to no
 * marker, so a paragraph built from it alone would otherwise be an empty object
 * where an absent one is what every other path produces.
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
  checkProps(props, TEXT_KEY_SET, 'RichText')
  // **The same rule children get.** A segment list and a children list are the
  // same kind of list, and a caller building either from data hits the same
  // `null`. Before this, children ignored two of the four ignorable values and
  // segments ignored none, so `[seg, cond && other]` worked in one and threw in
  // the other.
  // **The same rule children get, including the conversion.** A string in a
  // children list becomes a text node; a string in a segment list becomes a
  // run with that text. Keeping only the *skipping* in step would have left
  // `''` ignorable in one list and a broken segment in the other -- which the
  // agreement test caught the moment `''` stopped being ignorable.
  // **The pass-through is deliberate and a test pins it**: a list needing
  // neither filtering nor conversion is handed on as it stands, so the common
  // case allocates nothing.
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
 * Keys the generated property tables carry, used **only** to decide whether a
 * suggestion is safe.
 *
 * **Deliberately not an allowlist.** Measured, the union is 66 keys where
 * `Style` declares 69: `objectFit`, `objectPosition` and `frame` are carried
 * in a node's payload rather than in a style group, so a check built on this
 * set would refuse three valid properties.
 *
 * **The direction matters and only one of them is sound.** A key *in* the
 * table is certainly a style property; a key *absent* from it may still be
 * one. So the set is safe for deciding whether to suggest a fix and unsafe for
 * deciding whether to refuse a key.
 *
 * **`STYLE_KEYS` in `style.ts` is the complete list and is not this.** That one
 * is proved equal to `keyof Style` and is what a *refusal* uses. This one stays
 * narrower on purpose: `objectFit`, `objectPosition` and `frame` are style keys
 * that mean nothing on a segment, so suggesting `style: { objectFit }` would
 * typecheck and do nothing — a confidently wrong suggestion, which is the thing
 * the gate on this set exists to prevent.
 */
const SUGGESTIBLE_KEYS: ReadonlySet<string> = new Set(Object.values(PROPERTY_TABLES).flatMap(properties => properties.flatMap(property => property.keys)))

/**
 * Refuses a segment carrying a key `TextSegment` does not have.
 *
 * **Checked here because the type system checks it almost nowhere.** Excess
 * property checking fires on a *fresh object literal* and on nothing else, so
 * `RichText([{ text, fontSize }])` is caught and every other route is not —
 * a variable, a spread, `JSON.parse`, and most of all
 * `rows.map(r => ({ text: r.label, fontSize: r.size }))`, which is the case
 * `RichText` exists for. Measured: two of nine spellings rejected at compile
 * time, and the styling silently discarded at runtime for all nine.
 *
 * **The suggestion is worth the extra clause.** The mistake is almost always
 * flat-versus-nested, and unlike most bad input the correct spelling is
 * derivable from the wrong one — so the message can state the fix rather than
 * the rule.
 */
function checkSegment(segment: TextSegment, at: number): void {
  for (const key of Object.keys(segment)) {
    if (SEGMENT_KEYS.has(key)) continue
    // **The suggestion only where it is certainly right.** A key the generated
    // property tables carry is a style property, so `style: { key }` is the
    // fix. A key they do not carry might be a typo for anything, and a
    // confidently wrong suggestion is worse than none: a caller who follows it
    // writes a second broken call.
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
  const src = typeof props.src === 'string' ? { path: props.src } : props.src
  return node('image', props, undefined, props.name, undefined, undefined, undefined, src, undefined)
}

/** What a path node accepts: its data, and its style, flat. */
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
   * **Absent means absolute coordinates**, which is what every path did before
   * this existed. With a box, the path is scaled and centred into the node's
   * resolved size under SVG's default `preserveAspectRatio` — `xMidYMid meet`
   * — so it fits without distorting.
   *
   * **Equivalent to SVG's `viewBox` with
   * `vector-effect: non-scaling-stroke`.** The drawing scales; the pen does
   * not. In SVG a two-pixel stroke in a box scaled five times is drawn ten
   * pixels wide, and ours stays two — deliberately, because a caller authoring
   * a `d` in a unit square wants `lineWidth` to mean pixels. `vector-effect`
   * is the piece to add if something ever wants the other behaviour.
   *
   * **The node must have a size for this to mean anything.** A path node has
   * no intrinsic size, so one with neither a width nor a height gets an empty
   * box, and scaling a drawing into nothing draws nothing.
   *
   * It exists because a path in a percentage-sized box was otherwise
   * undrawable: `d` is absolute, `transform.scale` is a number rather than a
   * length, and a percentage-sized path node still draws `d` in absolute local
   * coordinates. **A rectangle can be a percentage and a path cannot** — which
   * is why a chart's bars needed nothing and its line, pie and doughnut need
   * this.
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
  return node('path', props, undefined, props.name, undefined, undefined, undefined, undefined, props.d)
}

/**
 * The keys every node carries, in the order the factories write them.
 *
 * Exported so a test can assert the shape rather than trusting it, since the
 * cost of a second hidden class is invisible until something is profiled.
 */
export const NODE_KEYS: readonly string[] = ['kind', 'style', 'children', 'name', 'paragraph', 'markup', 'segments', 'src', 'd']
