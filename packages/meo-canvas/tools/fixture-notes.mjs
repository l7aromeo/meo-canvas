// Every fixture directory has a `notes.json` with non-empty `proves`, `checkable` and
// `status`: what the scene pins, the geometry a person can read off the image, and
// where the numbers came from. It checks presence only, not truth; other keys are
// each note's own and are not checked.
import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'

/** The three questions a note answers, and the only keys this reads. */
const REQUIRED = ['proves', 'checkable', 'status']

/** Where the golden fixtures live, relative to the repository root. */
const FIXTURES = 'fixtures'

/**
 * How few fixtures may be found before this asks whether it was meant: a blind
 * enumeration checks nothing and reports every note complete. Two below the count,
 * so retiring one fixture needs no edit and a third needs saying so.
 */
const FLOOR = 21

const root = process.cwd()
const directories = readdirSync(join(root, FIXTURES), { withFileTypes: true })
  .filter(entry => entry.isDirectory())
  .map(entry => entry.name)
  .sort()

const faults = []
for (const directory of directories) {
  const path = join(FIXTURES, directory, 'notes.json')
  let source
  try {
    source = readFileSync(join(root, path), 'utf8')
  } catch {
    faults.push(`${path} is missing`)
    continue
  }
  let note
  try {
    note = JSON.parse(source)
  } catch (error) {
    faults.push(`${path} is not valid JSON: ${error.message}`)
    continue
  }
  // An array and a `null` both pass `typeof === 'object'`, and either would
  // reach the loop below and report three missing keys rather than the one
  // thing that is wrong with it.
  if (note === null || Array.isArray(note) || typeof note !== 'object') {
    faults.push(`${path} is not a JSON object`)
    continue
  }
  for (const key of REQUIRED) {
    const value = note[key]
    if (value === undefined) faults.push(`${path} has no \`${key}\``)
    else if (typeof value !== 'string') faults.push(`${path}'s \`${key}\` is ${typeof value}, not a string`)
    else if (value.trim() === '') faults.push(`${path}'s \`${key}\` is empty`)
  }
}

if (faults.length > 0) {
  for (const fault of faults) process.stdout.write(`  ${fault}\n`)
  process.stderr.write(
    `\n${faults.length} fault${faults.length === 1 ? '' : 's'} across ${directories.length} fixture ` +
      `director${directories.length === 1 ? 'y' : 'ies'}. A golden image says what was drawn and ` +
      `nothing else says what it was drawn to prove, so a fixture without \`${REQUIRED.join('`, `')}\` ` +
      'is one the next reader can only accept or re-derive.\n',
  )
  process.exit(1)
}

if (directories.length < FLOOR) {
  process.stderr.write(
    `\n${directories.length} fixture directories found under \`${FIXTURES}\`, below the floor of ` +
      `${FLOOR}. Either fixtures were retired -- in which case lower the floor in this file and say ` +
      'why -- or this enumeration is reading the wrong place, in which case every note above went ' +
      'unread and the line it would have printed is false.\n',
  )
  process.exit(1)
}

process.stdout.write(`${directories.length} fixture notes, each carrying ${REQUIRED.join(', ')}. ` + 'Whether they are true is not checked here.\n')
