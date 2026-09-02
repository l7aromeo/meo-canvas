// The acceptance harness's reading of a container's output, and its verdict.
//
// The harness itself needs docker and six images; these are the two pure
// functions inside it, exercised against fabricated output. That is deliberate
// rather than a convenience: the branches here are where a harness *misreports*,
// and the failures worth pinning are the ones where it would pass — a row that
// proves nothing counted as a row that passed, or a run where nothing could be
// asked exiting clean. Neither is reachable by running it against a good binary,
// which is exactly why they survive in harnesses nobody tests.

import { describe, expect, it } from 'vitest'

import { classify, decide, type Row } from '../tools/acceptance.mjs'

/** The output a container prints when both libraries are absent and it loaded. */
const CLEAN_LOAD = 'PRESENT \nLOADS 6'

describe('reading one container', () => {
  it('answers when the libraries are absent', () => {
    expect(classify(CLEAN_LOAD)).toMatchObject({ kind: 'answered', status: 'LOADS', loaded: true })
  })

  it('softens a row where a library is present, even though it loaded', () => {
    // The row that would otherwise be the most misleading in the table: it
    // loaded, and it says nothing about a machine without fontconfig.
    const result = classify('PRESENT /usr/lib/libfontconfig.so.1\nLOADS 6')
    expect(result.kind).toBe('softened')
    expect(result.loaded).toBe(true)
    expect(result.detail).toContain('proves nothing about a machine without it')
  })

  it('reports a load failure as an answer, not as a softening', () => {
    const result = classify('PRESENT \nFAILS libfontconfig.so.1: cannot open shared object file')
    expect(result).toMatchObject({ kind: 'answered', status: 'FAILS', loaded: false })
    // The loader's own words, not a paraphrase: it is the only evidence.
    expect(result.detail).toBe('libfontconfig.so.1: cannot open shared object file')
  })

  it('separates loading from registering nothing', () => {
    // Both print as success to anything that only checks for a throw.
    expect(classify('PRESENT \nREGISTERED_NOTHING')).toMatchObject({ kind: 'answered', status: 'FAILS', loaded: false })
  })

  it('treats a container with no node as unasked rather than as a failure', () => {
    expect(classify('NO_NODE')).toMatchObject({ kind: 'unasked', status: 'NO_NODE' })
  })

  it('treats output it cannot parse as ambiguous, not as a binary that failed', () => {
    // The shape that produced six false failures once: the probe was mangled
    // and every row came back like this. It must never read as `loaded` — and
    // it must not read as `answered` either, which would report a harness
    // fault as "this image cannot load the binary". A segfault inside `dlopen`
    // also prints nothing, so neither side owns this outcome.
    expect(classify('PRESENT \nsh: syntax error near unexpected token')).toMatchObject({
      kind: 'ambiguous',
      status: 'UNREADABLE',
      loaded: false,
    })
  })

  it('treats no output at all the same way', () => {
    expect(classify('')).toMatchObject({ kind: 'ambiguous', status: 'UNREADABLE' })
  })
})

describe('the verdict over a run', () => {
  const answeredPass: Row = { kind: 'answered', loaded: true, image: 'node:22-slim' }
  const answeredFail: Row = { kind: 'answered', loaded: false, image: 'debian:12-slim', detail: 'x' }
  const control: Row = { kind: 'softened', loaded: true, control: true, image: 'node:22' }

  it('passes when every answered row loaded', () => {
    expect(decide([control, answeredPass])).toMatchObject({ ok: true })
  })

  it('refuses a run where nothing could be answered', () => {
    // **The property this file exists for.** Every row softening or failing to
    // pull leaves nothing in the failure list, and a harness that then exits
    // clean reports a pass for a binary it never loaded anywhere.
    expect(decide([control])).toMatchObject({ ok: false })
    expect(decide([control]).why).toContain('broken harness')
  })

  it('never counts a softened row as one that passed', () => {
    const softenedFail: Row = { kind: 'softened', loaded: false, image: 'node:22-alpine' }
    expect(decide([softenedFail, answeredPass])).toMatchObject({ ok: true })
    // ...and it does not rescue a run either, which is the other direction.
    expect(decide([softenedFail])).toMatchObject({ ok: false })
  })

  it('fails the run when an answered row did not load', () => {
    expect(decide([control, answeredPass, answeredFail])).toMatchObject({ ok: false })
  })

  it('stops at a control that did not load, because the rest is uninformative', () => {
    // A binary that fails even on the image that ships every library is inert,
    // and every other row's failure says nothing beyond that.
    const dead: Row = { kind: 'softened', loaded: false, control: true, image: 'node:22' }
    expect(decide([dead, answeredFail]).why).toContain('uninformative')
  })

  it('fails an unreadable row without calling it a binary failure', () => {
    // Both halves matter. It must fail the run — an answer nobody could read
    // is not a pass — and the reason must not claim the binary is at fault,
    // because that is the sentence a reader acts on.
    const unreadable: Row = { kind: 'ambiguous', image: 'rockylinux:9', loaded: false }
    const verdict = decide([control, answeredPass, unreadable])
    expect(verdict.ok).toBe(false)
    expect(verdict.why).not.toMatch(/cannot load this binary/)
    expect(verdict.why).toMatch(/could not read/)
  })

  it('does not pass a run that left an image unasked', () => {
    const unasked: Row = { kind: 'unasked', image: 'rockylinux:9' }
    expect(decide([control, answeredPass, unasked])).toMatchObject({ ok: false })
  })
})
