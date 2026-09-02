// Types for the staging tool, so `src/addon.test.ts` can assert against its
// target list without the tool becoming TypeScript.
//
// The tools here are `.mjs` because they are run by `just` and by the release
// workflow with plain `node`, and a build step between a script and running it
// is a step that can be stale. Only this one is imported by a typechecked file,
// so only this one needs declaring.

/** One target's shape: what a package manager selects on, and what builds it. */
export interface Target {
  readonly os: readonly string[]
  readonly cpu: readonly string[]
  /** Present only where the platform has more than one C library to choose between. */
  readonly libc?: readonly string[]
  /** The Rust target triple the addon is built for. */
  readonly rust: string
  /** The GitHub runner that builds it. */
  readonly runner: string
  /**
   * The ELF symbol floors this target's artefact currently has.
   *
   * Absent where there are none to check: darwin and win32 have no ELF, and a
   * musl build links no glibc at all.
   */
  readonly floors?: { readonly glibc?: string; readonly glibcxx?: string }
}

/** Every target a release carries, keyed by the package-name suffix. */
export declare const TARGETS: Readonly<Record<string, Target>>

/** The target suffix for the machine this is running on, matched against `TARGETS`. */
export declare function hostSuffix(): string

/** The manifest a platform package ships, derived from the main one. */
export declare function manifest(suffix: string, version: string): Record<string, unknown>

/** Writes one staged package, and reports where it went. */
export declare function stage(suffix: string, binary: string, outDir: string): { name: string; version: string; staged: string }
