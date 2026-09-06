// Emits the arena property tables as TypeScript, read out of the Rust that
// defines them.
//
// The tables live in `arena_group!` invocations in
// `crates/meo-canvas-node/src/arena.rs` and carry their indices literally. A
// writer needs every index, name and type; transcribing them by hand would be a
// second table agreeing with the first by inspection, which is the failure this
// repository has removed twice already -- the format table that was
// `pub(crate)` upstream, and the node tags that were hand-written in the byte
// codec.
//
// Parsing the macro rather than exporting from the addon at runtime: the
// encoder runs per property per node and the standing constraint is that the
// path stays cheap, so the table has to be static. Parsing also needs no
// compiled addon, which means the generator runs on a checkout that has never
// built Skia.
//
// The parse is strict on purpose. A macro shape this does not recognise is an
// error naming the line, never a partial table -- a table missing entries would
// produce a writer that silently omits properties, which is exactly the failure
// generating it is meant to prevent.

import { readFile, writeFile, mkdir } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const HERE = dirname(fileURLToPath(import.meta.url))
const SOURCE = resolve(HERE, '../../../crates/meo-canvas-node/src/arena.rs')
const CHECKED_IN = resolve(HERE, '../src/generated/arena-tables.ts')

// An explicit destination lets the drift check emit somewhere disposable and
// `diff` the result, rather than asking git what changed. git's answer depends
// on whether a file is staged, committed or merely written, so a check built on
// it refuses the very workflow it exists to support: edit the Rust, regenerate,
// run the gate. A diff of two files is indifferent to all of that, which is
// what a check wants when CI's tree is clean and a developer's is not.
const TARGET = process.argv[2] ? resolve(process.argv[2]) : CHECKED_IN

/** Bits a mask slot holds. A double is exact on integers only to 2^53. */
const MASK_BITS = 53

/** Fails with a message naming where in the Rust the parse gave up. */
function fail(message) {
  throw new Error(`${SOURCE}: ${message}`)
}

/**
 * Every `arena_group!` invocation, as `{ name, sceneType, properties }`.
 *
 * Brace-counted rather than matched with one regular expression: the property
 * types nest angle brackets and parentheses, and a lazy match would stop at the
 * first `}` inside `Sides<Option<Length>>`.
 */
function parseGroups(source) {
  const groups = []
  const opener = /arena_group!\s*\{/g

  let found
  while ((found = opener.exec(source)) !== null) {
    const body = braced(source, found.index + found[0].length - 1)
    groups.push(parseGroup(source, body))
    opener.lastIndex = body.end
  }

  if (groups.length === 0) fail('no `arena_group!` invocation found')
  return groups
}

/** The span between a `{` at `open` and its matching `}`. */
function braced(source, open) {
  let depth = 0
  for (let index = open; index < source.length; index += 1) {
    if (source[index] === '{') depth += 1
    else if (source[index] === '}') {
      depth -= 1
      if (depth === 0) {
        return { text: source.slice(open + 1, index), end: index + 1 }
      }
    }
  }
  return fail(`unbalanced braces from offset ${open}`)
}

/** One group: its module name, the scene type it fills, and its properties. */
function parseGroup(source, body) {
  const header = /pub\(crate\)\s+mod\s+(\w+)\s+for\s+([\w:]+)\s*\{/.exec(body.text)
  if (!header) fail('an `arena_group!` body has no `mod NAME for TYPE {` header')

  const [, name, sceneType] = header
  const inner = braced(body.text, header.index + header[0].length - 1)

  const properties = []
  // One entry per `N => field as "caller": Type,`. The type runs to the comma
  // that closes the entry at depth zero, so a `Vec<(A, B)>` is not cut in half.
  //
  // The caller name is required rather than optional. An entry without one
  // does not match, so it is not collected, and the contiguity check below
  // then names the gap -- which is the strictness the module doc asks for: a
  // shape this does not recognise is an error, never a partial table.
  let cursor = 0
  const entry = /(\d+)\s*=>\s*(\w+)\s+as\s+"([^"]+)"\s*:/g
  entry.lastIndex = 0
  let match
  while ((match = entry.exec(inner.text)) !== null) {
    const [, index, field, caller] = match
    const type = readType(inner.text, entry.lastIndex)
    properties.push({
      index: Number(index),
      name: field,
      caller,
      type: type.text.trim().replace(/\s+/g, ' '),
    })
    entry.lastIndex = type.end
    cursor = type.end
  }
  void cursor

  if (properties.length === 0) fail(`group \`${name}\` declares no properties`)
  assertContiguous(name, properties)
  return { name, sceneType, properties }
}

/** A property's type, from `start` to the comma that ends the entry. */
function readType(text, start) {
  let depth = 0
  for (let index = start; index < text.length; index += 1) {
    const character = text[index]
    if (character === '<' || character === '(' || character === '[') depth += 1
    else if (character === '>' || character === ')' || character === ']') depth -= 1
    else if (character === ',' && depth === 0) {
      return { text: text.slice(start, index), end: index + 1 }
    }
  }
  return fail(`a property type beginning at ${start} has no closing comma`)
}

/**
 * Indices must be `0..n` in ascending order.
 *
 * The Rust asserts the same thing at compile time, and for the reason the
 * module doc gives: a table out of order reads the right number of slots into
 * the wrong fields, which no length check catches. Asserting it here too means
 * a generator that mis-parses fails rather than emitting a plausible table.
 */
function assertContiguous(name, properties) {
  properties.forEach((property, position) => {
    if (property.index !== position) {
      fail(`group \`${name}\` is not in ascending index order: ` + `\`${property.name}\` is ${property.index} at position ${position}`)
    }
  })
}

/** Reads a `pub const NAME: f64 = VALUE;` out of the Rust. */
function constant(source, name) {
  const found = new RegExp(`pub const ${name}: f64 = ([0-9_]+(?:\\.[0-9]+)?);`).exec(source)
  if (!found) fail(`no \`pub const ${name}: f64\``)
  return Number(found[1].replaceAll('_', ''))
}

/** The TypeScript. */
function emit(groups, magic, version) {
  const lines = [
    '// Generated by `just arena-tables` from the `arena_group!` tables in',
    '// `crates/meo-canvas-node/src/arena.rs`. Do not edit: `just ci`',
    '// regenerates this file and fails on a difference, so an edit here is a',
    '// build failure rather than a change.',
    '',
    '/** The first slot of every arena. */',
    `export const MAGIC = ${magic}`,
    '',
    '/** The revision a writer of this table emits. */',
    `export const VERSION = ${version}`,
    '',
    '/**',
    ' * Bits one mask slot holds.',
    ' *',
    ' * A double is exact on integers only to 2^53, so the 54th bit of a mask',
    ' * packed into one slot is lost with no rounding a reader could detect.',
    ' */',
    `export const MASK_BITS = ${MASK_BITS}`,
    '',
    '/** One property of a style group: the bit that names it, and its type. */',
    'export interface ArenaProperty {',
    "  /** Bit index within the group's mask. */",
    '  readonly index: number',
    "  /** The field's name in the scene type. */",
    '  readonly name: string',
    '  /**',
    '   * The style properties that feed it, as a caller spells them.',
    '   *',
    "   * Not the field's name: `border_color_all` is written `borderColor`, and",
    '   * a slot several properties may feed names all of them -- `gridColumn or',
    '   * gridArea`. This is what a failure reading the slot reports, so it has',
    '   * to be the surface spelling rather than the scene one.',
    '   */',
    '  readonly caller: string',
    '  /** The Rust type, as the table spells it. */',
    '  readonly type: string',
    '}',
    '',
  ]

  for (const group of groups) {
    const slots = Math.ceil(group.properties.length / MASK_BITS)
    const upper = group.name.toUpperCase()
    lines.push(
      `/** \`${group.sceneType}\`, ${group.properties.length} properties in ${slots} mask slot${slots === 1 ? '' : 's'}. */`,
      `export const ${upper}: readonly ArenaProperty[] = [`,
      ...group.properties.map(
        property =>
          `  { index: ${property.index}, name: ${JSON.stringify(property.name)}, caller: ${JSON.stringify(property.caller)}, type: ${JSON.stringify(property.type)} },`,
      ),
      ']',
      '',
      `/** Mask slots \`${group.name}\` occupies. */`,
      `export const ${upper}_MASK_SLOTS = ${slots}`,
      '',
    )
  }

  return `${lines.join('\n')}`
}

const source = await readFile(SOURCE, 'utf8')
const groups = parseGroups(source)
const output = emit(groups, constant(source, 'MAGIC'), constant(source, 'VERSION'))

await mkdir(dirname(TARGET), { recursive: true })
await writeFile(TARGET, output, 'utf8')

const counts = groups.map(group => `${group.name} ${group.properties.length}`).join(', ')
process.stderr.write(`arena tables: ${counts}\n`)
