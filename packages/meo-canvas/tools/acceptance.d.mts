// Types for the acceptance harness, so `src/acceptance.test.ts` can exercise
// its two pure functions without the tool becoming TypeScript.
//
// Same reasoning as `stage-platform-package.d.mts`: the tools here are `.mjs`
// because `just` and the release workflow run them with plain `node`, and a
// build step between a script and running it is a step that can be stale. Only
// the parts a typechecked file imports are declared -- the container driving
// is not, because a test that needed docker would not be a test.

/** What one row of a run turned out to be. */
export interface Row {
  /**
   * Which of the three kinds of answer this is.
   *
   * `answered` is the only kind that can fail a run. `softened` means the probe
   * ran somewhere that is not the host we meant to test -- a machine that has
   * the font libraries says nothing about one that does not -- and `unasked`
   * means the question could not be put at all.
   */
  readonly kind: 'answered' | 'softened' | 'unasked'
  readonly status?: string
  readonly detail?: string
  /** Whether the binary loaded. Absent where the probe never ran. */
  readonly loaded?: boolean
  readonly image?: string
  /** The control row: it ships the libraries, so it can never be a pass. */
  readonly control?: boolean
}

/** What one container's output means, as a pure function of that output. */
export declare function classify(out: string): Row

/** Whether a set of rows passes, and why not when it does not. */
export declare function decide(rows: readonly Row[]): {
  ok: boolean
  why: string
  broken?: readonly Row[]
  unasked?: readonly Row[]
}
