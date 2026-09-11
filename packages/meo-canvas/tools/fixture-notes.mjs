// Every fixture directory carries a note, and the note carries three fields.
//
// A golden fixture is a committed image compared byte for byte. The image says
// what the renderer drew and nothing says what the scene was built to prove, so
// a fixture whose note is missing is a picture a later reader can only accept
// or re-derive. `notes.json` is where the three answers live: **`proves`**, what
// the scene pins; **`checkable`**, the geometry a person can read off the image
// with a pixel reader; and **`status`**, where the numbers came from and what a
// regression would look like.
//
// **What this sees is existence and three fields, and that is the whole of it.**
// It reads that `proves` is a non-empty string. It cannot read whether the
// sentence is true, whether it describes the scene beside it, or whether it
// still describes it after the scene changed. `checkable` is the sharper case:
// the field is named for a property no program here can test -- that a person
// with the image and a pixel reader could follow it and arrive at the numbers --
// and this tool tests that the key is present and says something. A note whose
// three fields are careful prose about a different fixture passes exactly as a
// correct one does.
//
// So it is worth what an empty-string check is worth and no more: it catches a
// fixture added with no note, a note edited into invalid JSON, and a field
// renamed or dropped. Those are the three failures nothing else in the tree
// reports -- `fixtures.rs` compares images and never opens a note.
//
// **The tail is not checked and must not be.** Twenty-three notes carry
// eighty-two distinct keys between them; seventy-two appear exactly once, with
// names like `why_the_cells_are_not_square` that answer a question only that
// fixture raises. A note that anticipates its reader's next question is the
// point, and a schema over the union of those keys would be a rule requiring
// every fixture to raise the same questions. The three below are the floor of
// what a note must answer; everything past them is the note doing its job.
import { readdirSync, readFileSync } from 'node:fs'
import { join } from 'node:path'

/** The three questions a note answers, and the only keys this reads. */
const REQUIRED = ['proves', 'checkable', 'status']

/** Where the golden fixtures live, relative to the repository root. */
const FIXTURES = 'fixtures'

/**
 * How few fixtures may be found before this asks whether it was meant.
 *
 * Two below today's twenty-three, for the reason the count exists at all: the
 * failure this cannot otherwise see is the enumeration going blind. A renamed
 * directory, a `withFileTypes` filter inverted, a path resolved against the
 * wrong root -- each of those finds nothing, checks nothing, and prints a line
 * saying every note it read was complete. A floor turns that sentence into a
 * failure.
 *
 * Two rather than more because the number is a tripwire and not a target:
 * retiring a fixture deliberately should not require editing a constant here,
 * and a number with room to drift stops describing the tree. Deleting a third
 * fixture is meant to require saying so.
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
