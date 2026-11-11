// What `currentColor` paints as on this surface, against Chrome's
// `current-color.tsv`, by the table's own method: each case drawn with
// `currentColor`, again with each colour it could stand for, and the identical
// renders must be the ones Chrome named. `chrome_current_color.rs` is the
// crate's half, and the known-apart rows are the same.

import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

import { describe, expect, it } from 'vitest'

import { Box, Path, Root, Text } from './index.js'
import type { SceneNode, TextProps } from './node.js'

const TABLE = fileURLToPath(new URL('../../../crates/meo-canvas/tests/assets/chrome/current-color.tsv', import.meta.url))
const FONT = fileURLToPath(new URL('../../../crates/meo-canvas-core/tests/assets/fonts/Oswald-VariableFont_wght.ttf', import.meta.url))

/** The element's own `color` and its parent's, where a context sets them, as the tool does. */
const OWN = 'rgb(220, 40, 40)'
const PARENT = 'rgb(40, 80, 220)'
const INITIAL = 'rgb(0, 0, 0)'

/** As `chrome_current_color.rs`'s `KNOWN_APART`, and for its reason (`l7aromeo/meo-skia-canvas#688`). */
const KNOWN_APART = new Set([
  'drop-shadow() own',
  'drop-shadow() inherited',
  'drop-shadow() colour omitted own',
  'drop-shadow() colour omitted inherited',
  'color-mix() own',
  'color-mix() inherited',
  'color-mix() unset',
  'relative colour own',
  'relative colour inherited',
  'relative colour unset',
])

/** A text decoration has no colour property here; it is always the text's. */
const NO_COUNTERPART = new Set(['text decoration colour omitted'])

/** The props a case sets, given the colour to write, or `undefined` for the case's own spelling. */
const CASES: Record<string, (colour: string | undefined) => Partial<TextProps>> = {
  'background-color': c => ({ backgroundColor: c ?? 'currentColor' }),
  'background-color upper case': c => ({ backgroundColor: c ?? 'CURRENTCOLOR' }),
  'border-color': c => ({ border: 4, borderStyle: 'solid', borderColor: c ?? 'currentColor' }),
  'border-color omitted': c => ({ border: 4, borderStyle: 'solid', ...(c === undefined ? {} : { borderColor: c }) }),
  'box-shadow': c => ({ boxShadow: { offsetX: 4, offsetY: 4, color: c ?? 'currentColor' } }),
  'box-shadow colour omitted': c => ({ boxShadow: { offsetX: 4, offsetY: 4, ...(c === undefined ? {} : { color: c }) } }),
  'gradient stop': c => ({ gradient: { type: 'linear', direction: 'to-right', colors: [c ?? 'currentColor', 'white'] } }),
  'drop-shadow()': c => ({ filter: `drop-shadow(4px 4px 0 ${c ?? 'currentColor'})` }),
  'drop-shadow() colour omitted': c => ({ filter: c === undefined ? 'drop-shadow(4px 4px 0)' : `drop-shadow(4px 4px 0 ${c})` }),
  'text-shadow': c => ({ textShadow: { offsetX: 2, offsetY: 2, color: c ?? 'currentColor' } }),
  'text-shadow colour omitted': c => ({ textShadow: { offsetX: 2, offsetY: 2, ...(c === undefined ? {} : { color: c }) } }),
  'text stroke': c => ({ textStroke: { width: 1, color: c ?? 'currentColor' } }),
  'text stroke colour omitted': c => ({ textStroke: { width: 1, ...(c === undefined ? {} : { color: c }) } }),
  color: c => ({ color: c ?? 'currentColor' }),
  'color-mix()': c => ({ backgroundColor: `color-mix(in srgb, ${c ?? 'currentColor'} 50%, white)` }),
  'relative colour': c => ({ backgroundColor: `rgb(from ${c ?? 'currentColor'} r g b)` }),
  // Written by `element`, which builds the node these style.
  'path fill': () => ({}),
  'path fill omitted': () => ({}),
  'span color': () => ({}),
}

/**
 * The element a case styles: a square path for the path cases, as the tool's SVG
 * path, left at the default fill when its colour is omitted; markup for `span
 * color`, whose tag is the span; `Hg` otherwise. `colour` is the colour written,
 * `null` for the case's own spelling, `undefined` for the property absent.
 */
function element(name: string, colour: string | null | undefined, props: Partial<TextProps>): SceneNode {
  if (name === 'path fill' || name === 'path fill omitted') {
    const fill = colour ?? (name === 'path fill' && colour === null ? 'currentColor' : undefined)
    return Path({ d: 'M0 0H24V24H0Z', ...props, ...(fill === undefined ? {} : { fill }) })
  }
  if (name === 'span color') {
    const markup = colour === undefined ? 'Hg' : `<color="${colour ?? 'currentColor'}">Hg`
    return Text(markup, props)
  }
  return Text('Hg', props)
}

interface Row {
  readonly name: string
  readonly context: string
  readonly sameAs: string
}

function rows(): Row[] {
  return readFileSync(TABLE, 'utf8')
    .split('\n')
    .filter(line => line !== '' && !line.startsWith('#'))
    .map(line => {
      const [name, context, , sameAs] = line.split('\t')
      if (name === undefined || context === undefined || sameAs === undefined) throw new Error(`\`${line}\` is not four fields`)
      return { name, context, sameAs }
    })
}

/** The page, as the tool's: a parent, and the element at (10, 10) with `Hg` in it. */
async function render(context: string, name: string, colour: string | null | undefined, props: Partial<TextProps>): Promise<Buffer | 'refused'> {
  const parent = context === 'unset' ? undefined : PARENT
  const own = context === 'own' ? OWN : undefined
  try {
    const canvas = await Root({
      width: 48,
      height: 48,
      gpu: false,
      fonts: [{ family: 'Fixture', paths: [FONT] }],
      children: Box({
        width: 48,
        height: 48,
        backgroundColor: '#ffffff',
        ...(parent === undefined ? {} : { color: parent }),
        children: element(name, colour, {
          positionType: 'absolute',
          position: { top: 10, left: 10 },
          width: 24,
          height: 24,
          fontFamily: 'Fixture',
          fontSize: 16,
          lineHeight: '24px',
          ...(own === undefined ? {} : { color: own }),
          ...props,
        }),
      }),
    })
    const raw = Buffer.from(await canvas.toBuffer('raw'))
    canvas.release()
    return raw
  } catch {
    return 'refused'
  }
}

async function sameAs(row: Row, write: (colour: string | undefined) => Partial<TextProps>): Promise<string> {
  const measured = await render(row.context, row.name, null, write(undefined))
  if (measured === 'refused') return 'refused'
  const candidates: [string, string | undefined][] = [
    ['own', row.context === 'own' ? OWN : undefined],
    ['parent', row.context === 'unset' ? undefined : PARENT],
    ['initial', INITIAL],
  ]
  const same: string[] = []
  for (const [name, colour] of candidates) {
    if (colour === undefined) continue
    const drawn = await render(row.context, row.name, colour, write(colour))
    if (drawn !== 'refused' && drawn.equals(measured)) same.push(name)
  }
  const absent = await render(row.context, row.name, undefined, {})
  if (absent !== 'refused' && absent.equals(measured)) same.push('absent')
  return same.join(',')
}

describe('currentColor', () => {
  it('paints as Chrome paints it, on every case the table holds', async () => {
    const table = rows()
    expect(table).toHaveLength(60)
    const wrong: string[] = []
    let apart = 0
    for (const row of table) {
      const key = `${row.name} ${row.context}`
      if (NO_COUNTERPART.has(row.name)) continue
      const write = CASES[row.name]
      if (write === undefined) throw new Error(`no case written for \`${row.name}\``)
      const ours = await sameAs(row, write)
      const known = KNOWN_APART.has(key)
      if (known) apart += 1
      if ((ours === row.sameAs) === known) {
        wrong.push(known ? `${key}: agrees with Chrome now; remove it from KNOWN_APART` : `${key}: ours ${ours}, Chrome ${row.sameAs}`)
      }
    }
    expect(apart).toBe(KNOWN_APART.size)
    expect(wrong).toEqual([])
  })
})
