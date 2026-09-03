// Lifts the wire enums out of the scene crate into TypeScript.
//
// Every enum that crosses the arena is written as one number: the same
// discriminant the byte codec writes, because both sides read `from_wire`. The
// writer on this side has to know those numbers.
//
// Hand-copying them would be the fourth copy of each list -- the enum, the two
// halves of `wire_enum!`, and this -- and the drift is silent in the worst
// available way. A variant inserted upstream does not make a value fail to
// decode; it makes it decode as a *different variant*. That is precisely the
// failure `wire_enum!` exists to prevent within Rust, and copying its output by
// hand would reintroduce it at the language boundary.
//
// So the numbers are read from the declarations themselves. `wire_enum!` writes
// discriminants explicitly at the call site -- its own comment says why:
// position changes when a variant is inserted and the byte a variant is written
// as cannot -- which is what makes this parseable without evaluating Rust.
//
// The Rust module path is derived from the file, so a test can check every enum
// the arena tables name against what is actually declared. Two generated files
// reading two halves of one format is exactly where a set check earns its
// keep.

import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const HERE = dirname(fileURLToPath(import.meta.url))
const SCENE_SRC = resolve(HERE, '../../../crates/meo-canvas-scene/src')
const CHECKED_IN = resolve(HERE, '../src/generated/arena-enums.ts')

// An explicit destination lets the drift check emit somewhere disposable and
// diff the result, rather than asking git what changed -- git's answer depends
// on whether a file is untracked, written or staged.
const TARGET = process.argv[2] ? resolve(process.argv[2]) : CHECKED_IN

/** The crate the module paths are rooted at. */
const CRATE = 'meo_canvas_scene'

/** Fails with a message naming the file the parse gave up on. */
function fail(where, message) {
  throw new Error(`${where}: ${message}`)
}

/** Every `.rs` file under the scene crate, deepest last, in a stable order. */
async function sources(directory) {
  const entries = await readdir(directory, { withFileTypes: true })
  const found = []
  for (const entry of entries.sort((a, b) => (a.name < b.name ? -1 : 1))) {
    const path = join(directory, entry.name)
    if (entry.isDirectory()) found.push(...(await sources(path)))
    else if (entry.name.endsWith('.rs')) found.push(path)
  }
  return found
}

/**
 * The Rust module path a file declares its items in.
 *
 * `src/style/layout.rs` is `meo_canvas_scene::style::layout`, `src/lib.rs` is
 * the crate root. Mechanical because the crate uses one file per module and no
 * `#[path]`; a file that broke that assumption would produce a path no arena
 * table names, which the set check below turns into a failure.
 */
function modulePath(path) {
  const relative = path.slice(SCENE_SRC.length + 1).replace(/\.rs$/, '')
  if (relative === 'lib') return CRATE
  // Split on both separators, because this is a filesystem path becoming a
  // Rust module path and Windows spells the first one `\`. Splitting on `/`
  // alone emitted `meo_canvas_scene::style\layout` there -- not a crash, a
  // **wrong generated file**, which `arena-enums-check` reported as the table
  // being stale on Windows and nowhere else. Committing that file from a
  // Windows machine would have put backslashes into module paths for everyone.
  return [CRATE, ...relative.split(/[/\\]/)].join('::')
}

/**
 * Where the brace opened at `from` closes, or `-1`.
 *
 * Counted rather than matched with one expression, and comments are skipped:
 * a variant's doc comment may contain a brace, and a regular expression that
 * also had to survive that would be the harder thing to trust.
 */
function closes(text, from) {
  let depth = 0
  for (let index = from; index < text.length; index += 1) {
    if (text.startsWith('//', index)) {
      const line = text.indexOf('\n', index)
      if (line === -1) return -1
      index = line
      continue
    }
    if (text[index] === '{') depth += 1
    else if (text[index] === '}') {
      depth -= 1
      if (depth === 0) return index
    }
  }
  return -1
}

/** Every `wire_enum!` declaration in `text`. */
function declarations(path, text) {
  const found = []
  let at = 0

  for (;;) {
    const opened = text.indexOf('wire_enum! {', at)
    if (opened === -1) break

    const end = closes(text, text.indexOf('{', opened))
    if (end === -1) fail(path, 'a `wire_enum!` block is never closed')

    found.push(parse(path, text.slice(opened, end + 1)))
    at = end + 1
  }

  return found
}

/** One declaration's name and variants. */
function parse(path, block) {
  const named = /\benum\s+(\w+)\s*\{/.exec(block)
  if (!named) fail(path, 'a `wire_enum!` block declares no enum')

  const brace = named.index + named[0].length - 1
  const end = closes(block, brace)
  if (end === -1) fail(path, `\`${named[1]}\` is never closed`)

  const body = block.slice(brace + 1, end)
  const variants = []
  const seen = new Map()

  for (const line of body.split('\n')) {
    const trimmed = line.trim()
    if (trimmed === '' || trimmed.startsWith('//') || trimmed.startsWith('#[')) continue

    const entry = /^(\w+)\s*=\s*(\d+)\s*,?$/.exec(trimmed)
    if (!entry) {
      fail(path, `\`${named[1]}\` has a line this generator cannot read: ${trimmed}`)
    }
    const [, variant, discriminant] = entry
    const value = Number(discriminant)

    // A repeated discriminant makes `from_wire` return whichever arm matched
    // first, so two variants would arrive as one. Rust would compile it.
    const collided = seen.get(value)
    if (collided !== undefined) {
      fail(path, `\`${named[1]}\` gives ${collided} and ${variant} the same discriminant ${value}`)
    }
    seen.set(value, variant)
    variants.push({ variant, value })
  }

  if (variants.length === 0) fail(path, `\`${named[1]}\` declares no variants`)
  return { name: named[1], path: modulePath(path), variants }
}

/** The constant name an enum's table is exported under. */
function constantName(name) {
  return name.replace(/([a-z0-9])([A-Z])/g, '$1_$2').toUpperCase()
}

/** The generated TypeScript. */
function emit(enums) {
  const lines = [
    '// Generated by `just arena-enums` from the `wire_enum!` declarations in',
    '// `crates/meo-canvas-scene/src`. Do not edit: `just ci` regenerates this',
    '// file and fails on a difference, so an edit here is a build failure',
    '// rather than a change.',
    '//',
    '// A variant crosses the arena as the same number the byte codec writes it',
    '// as. Copying these lists by hand would mean a variant inserted upstream',
    '// arrives as a *different variant* rather than as an error -- which is the',
    '// failure `wire_enum!` exists to prevent, so it is not one to reintroduce',
    '// by hand at the language boundary.',
    '',
    '/** One wire enum: where it is declared, and what each variant is written as. */',
    'export interface ArenaEnum {',
    '  /** The Rust path, as the arena property tables spell it. */',
    '  readonly path: string',
    '  /** Each variant name, and the number it crosses as. */',
    '  readonly variants: Readonly<Record<string, number>>',
    '}',
    '',
  ]

  for (const declared of enums) {
    const body = declared.variants.map(entry => `  ${entry.variant}: ${entry.value},`)
    lines.push(`/** \`${declared.path}::${declared.name}\`. */`)
    lines.push(`export const ${constantName(declared.name)} = {`, ...body, '} as const', '')
  }

  lines.push('/** Every wire enum, by the name Rust declares it under. */')
  lines.push('export const ENUMS: Readonly<Record<string, ArenaEnum>> = {')
  for (const declared of enums) {
    lines.push(`  ${declared.name}: { path: '${declared.path}', variants: ${constantName(declared.name)} },`)
  }
  lines.push('}', '')

  return lines.join('\n')
}

const enums = []
const byName = new Map()
for (const path of await sources(SCENE_SRC)) {
  const text = await readFile(path, 'utf8')
  for (const declared of declarations(path, text)) {
    const collided = byName.get(declared.name)
    if (collided !== undefined) {
      fail(path, `\`${declared.name}\` is also declared in ${collided}; one name cannot carry two tables`)
    }
    byName.set(declared.name, declared.path)
    enums.push(declared)
  }
}

if (enums.length === 0) {
  fail(SCENE_SRC, 'no `wire_enum!` declarations found; the generator would emit a table guarding nothing')
}

enums.sort((a, b) => (a.name < b.name ? -1 : 1))

await mkdir(dirname(TARGET), { recursive: true })
await writeFile(TARGET, emit(enums), 'utf8')

const variants = enums.reduce((total, declared) => total + declared.variants.length, 0)
process.stderr.write(`arena enums: ${enums.length} enums, ${variants} variants\n`)
