// Types for the notices tool, so `src/third-party-notices.test.ts` can drive its
// parts without the tool becoming TypeScript. See `stage-platform-package.d.mts`
// for why the tools are `.mjs`.

/** A third-party library a build can link, and where its licence text is read. */
export interface Component {
  readonly name: string
  /** Where the committed texts were read: a repository and the revision. */
  readonly origin?: string
  /** The licence files committed under the tool's `notices/`, relative to it. */
  readonly committed?: readonly string[]
  /** The package whose source holds the licence files, when they are not committed. */
  readonly pkg?: string
  /** The licence files, relative to that package's root. */
  readonly files?: readonly string[]
}

/** The Skia that skia-bindings builds, at which the committed Skia texts were read. */
export declare const SKIA_SOURCE: { readonly tag: string; readonly origin: string; readonly freetypeCreditYear: number }

/** Every C or C++ library a build can link. */
export declare const COMPONENTS: Readonly<Record<string, Component>>

/** What each GN target in a Skia archive holds, by component. */
export declare const GN_TARGETS: Readonly<Record<string, readonly string[]>>

/** Windows archive members under `third_party/externals/<dir>/`, by that directory. */
export declare const EXTERNALS: Readonly<Record<string, readonly string[]>>

/** The static libraries other packages' build scripts link, and what each holds. */
export declare const NATIVE_LIBS: Readonly<Record<string, Readonly<Record<string, readonly string[] | 'own'>>>>

/** Libraries linked by name that are each OS's own. */
export declare const SYSTEM_LIBS: Readonly<Record<string, readonly string[]>>

/** Libraries a Linux release image links by name from archives it built. */
export declare const IMAGE_LIBS: Readonly<Record<string, string>>

/** The source directory each image library is built from, as `containers/install-fontlibs.sh` pins it. */
export declare function imageVersions(script?: string): Readonly<Record<string, string>>

/** The skia-binaries release the Linux FreeType and zlib origins were read for. */
export declare const LINUX_ARCHIVE_RELEASE: string

/** One library a build script linked. */
export interface Linked {
  readonly pkg: string
  /** `static`, `dylib`, `framework`, or empty for a name alone. */
  readonly kind: string
  readonly name: string
  readonly dirs: readonly string[]
}

/** One cargo `--message-format=json` message, as far as the tool reads it. */
export type Message = Record<string, unknown>

/** The member names of a static archive. */
export declare function archiveMembers(bytes: Buffer): string[]

/** The components one Skia archive member holds. */
export declare function memberComponents(member: string): readonly string[]

/** Parses cargo's JSON messages, one per line. */
export declare function readMessages(text: string): Message[]

/** A package id's name. */
export declare function packageNameOf(id: string): string

/** Every library the build scripts linked, and the features the addon was built with. */
export declare function linkedLibraries(messages: readonly Message[], root?: string): { libraries: Linked[]; features: string[] }

/** The components the build's libraries hold, each with the libraries it was found in. */
export declare function nativeComponents(
  libraries: readonly Linked[],
  os: string,
  options?: { imageLicences?: string; archiveNotices?: boolean; readArchive?: (path: string) => readonly string[] },
): Map<string, Set<string>>

/** The file meo-skia-canvas puts in a skia-binaries archive with its notices. */
export declare const ARCHIVE_NOTICES: string

/** meo-skia-canvas's notices for one Skia archive, and where they were read. */
export interface ArchiveNotices {
  readonly text: string
  readonly source: string
}

/** A URL's body, or `undefined` when it answers 404. */
export declare function fetchText(url: string): Promise<string | undefined>

/** meo-skia-canvas's notices for the archive in `archiveDir`, from the archive or its release asset. */
export declare function archiveNotices(
  archiveDir: string,
  binariesUrl: string | undefined,
  fetchAsset?: (url: string) => Promise<string | undefined>,
): Promise<ArchiveNotices | undefined>

/** What cargo-about reports, as far as the tool reads it. */
export interface AboutReport {
  readonly licenses: readonly {
    readonly name: string
    readonly id: string
    readonly text: string
    readonly used_by: readonly { readonly crate: { readonly id: string; readonly name: string; readonly version: string } }[]
  }[]
  readonly crates: readonly { readonly package: { readonly id: string; readonly name: string; readonly version: string } }[]
}

/** The notices for one target. */
export declare function renderNotices(input: {
  suffix: string
  triple: string
  features: readonly string[]
  about: AboutReport
  native: Map<string, Set<string>>
  dirs: Map<string, string>
  workspace: Set<string>
  skiaRelease: string | undefined
  imageLicences?: string
  archive?: ArchiveNotices
}): string

/** A crate whose licence text is a committed file rather than what its source holds. */
export interface Clarified {
  readonly version: string
  readonly license: string
  /** The committed file, under the tool's `notices/`. */
  readonly file: string
  /** That file's SHA-256, which cargo-about checks before using it. */
  readonly checksum: string
  readonly origin: string
}

/** The crates whose licence text is committed, by name. */
export declare const CLARIFIED: Readonly<Record<string, Clarified>>

/** `about.toml`, or `base`, with a clarification for each of `CLARIFIED`. */
export declare function aboutConfig(base?: string): string

/** Refuses a report in which a clarified crate stands under any text but its committed one. */
export declare function checkClarified(about: AboutReport): void

/** cargo-about's report for a manifest, with `args` passed to `cargo about generate`. */
export declare function cargoAbout(manifest: string, args: readonly string[]): AboutReport
