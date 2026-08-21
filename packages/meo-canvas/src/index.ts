/**
 * The type vocabulary a scene is described in.
 *
 * Every enumerated value is a string-literal union rather than a numeric enum or a
 * bare `string`. A union is what an editor completes as you type and what the
 * compiler rejects a misspelling against; `string` accepts anything and finds the
 * mistake at render time, and a numeric enum costs an import at every call site to
 * name a value that is already a word.
 *
 * The values are the CSS spellings, because that is the vocabulary the layout rules
 * come from and the one a caller already knows.
 *
 * @packageDocumentation
 */

/** Axis children are placed along, and the direction they run in. */
export type FlexDirection = 'row' | 'column' | 'row-reverse' | 'column-reverse'

/**
 * Distribution of children along the main axis.
 *
 * Carries `start`/`end` alongside `flex-start`/`flex-end`: the first pair follows the
 * writing direction and the second follows the flex direction, and a row-reverse
 * container puts them at opposite edges.
 */
export type JustifyContent = 'start' | 'end' | 'flex-start' | 'flex-end' | 'center' | 'stretch' | 'space-between' | 'space-around' | 'space-evenly'

/** Placement of children across the cross axis. */
export type AlignItems = 'start' | 'end' | 'flex-start' | 'flex-end' | 'center' | 'baseline' | 'stretch'

/**
 * One child's own cross-axis placement.
 *
 * `auto` is the extra value: it defers to the parent's `alignItems`, which is what a
 * child that states nothing does anyway, and naming it lets a computed style say so.
 */
export type AlignSelf = AlignItems | 'auto'

/** Distribution of wrapped lines across the cross axis. */
export type AlignContent = 'start' | 'end' | 'flex-start' | 'flex-end' | 'center' | 'stretch' | 'space-between' | 'space-around' | 'space-evenly'

/**
 * Whether a box is placed by the layout or on top of it.
 *
 * `relative` boxes take part in their parent's flow and are offset from where that
 * flow put them. `absolute` boxes are taken out of it and positioned against their
 * parent's padding box.
 */
export type Position = 'relative' | 'absolute'

/** Layout algorithm a container lays its children out with, or `none` to draw nothing. */
export type Display = 'block' | 'flex' | 'grid' | 'none'

/**
 * What happens to content that reaches past its box.
 *
 * `clip` and `hidden` both cut the content off; they differ in that `hidden` leaves
 * the box scrollable in a browser and `clip` does not. Neither scrolls here, so they
 * draw alike, and both are accepted so a style copied from CSS keeps its meaning.
 */
export type Overflow = 'visible' | 'clip' | 'hidden' | 'scroll'

/** Whether children overflow onto new lines, and which way those lines stack. */
export type FlexWrap = 'nowrap' | 'wrap' | 'wrap-reverse'

/** Whether a declared width and height measure the border box or the content box. */
export type BoxSizing = 'border-box' | 'content-box'

/** How an image fills the box it was given. */
export type ObjectFit = 'fill' | 'contain' | 'cover' | 'none' | 'scale-down'

/**
 * Horizontal alignment of text within its box.
 *
 * `start` and `end` follow the reading direction; `left` and `right` do not, and are
 * the pair to reach for when a label must sit on one particular side whatever the
 * text direction is.
 */
export type TextAlign = 'start' | 'end' | 'left' | 'right' | 'center' | 'justify'

/** Reading direction, which decides which edge a line starts at. */
export type Direction = 'ltr' | 'rtl'
