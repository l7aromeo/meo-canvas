// Writes the THIRD_PARTY_NOTICES a platform package ships: every third-party
// component in that target's addon with its licence text, the crates from
// cargo-about and the C libraries from what the build linked. Anything it cannot
// place fails the run; `docs/releasing.md` has the design.

import { execFileSync } from 'node:child_process'
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { TARGETS } from './stage-platform-package.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))
const ROOT = resolve(HERE, '../../..')

/** The crate the addon is built from, relative to the repository root. */
const NODE_CRATE = 'crates/meo-canvas-node/Cargo.toml'

/** The allowlist cargo-about holds every crate's licence to. */
const ABOUT_CONFIG = join(ROOT, 'about.toml')

/** Where a licence text lives: in a package's source, or committed beside this tool. */
const fromPackage = (pkg, ...files) => ({ pkg, files })
const committed = (...files) => ({ committed: files })

/**
 * The Skia skia-bindings builds from, which its `Cargo.toml` names under
 * `[package.metadata]`. The published crate holds no Skia source, so the texts
 * of Skia and its externals are committed under `notices/skia/`, read from
 * rust-skia/skia at this tag and each external at the revision its `DEPS` pins.
 */
export const SKIA_SOURCE = {
  tag: 'm156.5',
  origin: 'https://github.com/rust-skia/skia at 0746a1afaf750159e1a6aef29992980f4e293585 (m156.5)',
  /** The year in that FreeType's `freetype.h` copyright line, for the FTL's credit. */
  freetypeCreditYear: 2026,
}

/** A component of Skia's source, read at `origin`, its texts under `notices/skia/`. */
const fromSkia = (origin, ...files) => ({ origin, ...committed(...files.map(file => `skia/${file}`)) })

/** An external Skia's `DEPS` pins, as `<url> at <revision>`. */
const external = (url, revision, ...files) => fromSkia(`${url} at ${revision}`, ...files.map(file => `third_party/externals/${file}`))

/**
 * Every C or C++ library a build can link, and where its licence text is read.
 * The FreeType text is the FTL, the licence of the two it offers that this
 * project takes, and with it the credit line the FTL asks for.
 */
export const COMPONENTS = {
  skia: { name: 'Skia', ...fromSkia(SKIA_SOURCE.origin, 'LICENSE') },
  skcms: { name: 'skcms', ...fromSkia(SKIA_SOURCE.origin, 'modules/skcms/LICENSE') },
  expat: {
    name: 'Expat',
    ...external(
      'https://chromium.googlesource.com/external/github.com/libexpat/libexpat.git',
      'a76b1174c1ea52abb639a4c69d52b630a8a106a2',
      'expat/expat/COPYING',
    ),
  },
  harfbuzz: {
    name: 'HarfBuzz',
    ...external('https://chromium.googlesource.com/external/github.com/harfbuzz/harfbuzz.git', 'f5efbbef3841bdcdc6427bde7c2971f62ac8c8f1', 'harfbuzz/COPYING'),
  },
  icu: { name: 'ICU', ...external('https://chromium.googlesource.com/chromium/deps/icu.git', '2a51536dfb3b448bdf3dae70c782ea2879c90904', 'icu/LICENSE') },
  'libjpeg-turbo': {
    name: 'libjpeg-turbo',
    ...external(
      'https://chromium.googlesource.com/chromium/deps/libjpeg_turbo.git',
      '640f254ad0fa03f6b1f29f89b7dd9366f2f6e533',
      'libjpeg-turbo/LICENSE.md',
      'libjpeg-turbo/README.ijg',
    ),
  },
  libpng: { name: 'libpng', ...external('https://skia.googlesource.com/third_party/libpng.git', '3061454d980de7d53608f594194cfac722721d2a', 'libpng/LICENSE') },
  libwebp: {
    name: 'libwebp',
    ...external('https://chromium.googlesource.com/webm/libwebp.git', 'b43b2caa710c0c997c066cb32c7fea1391fad70a', 'libwebp/COPYING', 'libwebp/PATENTS'),
  },
  wuffs: {
    name: 'Wuffs',
    ...external(
      'https://skia.googlesource.com/external/github.com/google/wuffs-mirror-release-c.git',
      '7411f488fe2e2c205c3d3b3d28638b7356522930',
      'wuffs/LICENSE',
    ),
  },
  zlib: {
    name: 'zlib (Chromium)',
    ...external('https://chromium.googlesource.com/chromium/src/third_party/zlib', '456ae730017b9b4bd3371927abc82627fd51feef', 'zlib/LICENSE'),
  },
  freetype: {
    name: 'FreeType',
    ...external(
      'https://chromium.googlesource.com/chromium/src/third_party/freetype2.git',
      '891f904e2e7b4289f615598cd4b69832f87d2f3a',
      'freetype/LICENSE.TXT',
      'freetype/docs/FTL.TXT',
    ),
  },
  brotli: {
    name: 'Brotli',
    ...external('https://skia.googlesource.com/external/github.com/google/brotli.git', 'cfa184a997ac19454d5a3ae97f6d600aac2445a6', 'brotli/LICENSE'),
  },
  vulkanmemoryallocator: {
    name: 'Vulkan Memory Allocator',
    ...external(
      'https://chromium.googlesource.com/external/github.com/GPUOpen-LibrariesAndSDKs/VulkanMemoryAllocator',
      'eb744ea7a2b17040121b4bbb4d6f9e8a77e3cae7',
      'vulkanmemoryallocator/LICENSE.txt',
    ),
  },
  libaom: { name: 'libaom', ...fromPackage('libaom-sys', 'vendor/LICENSE', 'vendor/PATENTS') },
  'zlib-libz-sys': { name: 'zlib', ...fromPackage('libz-sys', 'src/zlib/LICENSE') },
  'brotli-vendored': { name: 'Brotli', ...fromPackage('meo-skia-canvas', 'vendor/brotli/LICENSE') },
}

/**
 * The release the Linux overrides below were read for. meo-skia-canvas's
 * `skia-binaries.yml` builds its Linux archives over its own `vendor/freetype`
 * and `vendor/zlib` submodules rather than the revisions Skia's `DEPS` pins. A
 * different release is refused until they are read again.
 */
export const LINUX_ARCHIVE_RELEASE = 'skia-binaries-0.156.0-3f566dd7198c'

/**
 * The Linux archives' FreeType and zlib, at the submodule commits that release
 * was built from (meo-skia-canvas 61d9536f). FreeType's LICENSE.TXT and FTL.TXT
 * there are byte-identical to the committed copies; chromium-zlib's LICENSE is
 * not, so it is committed too. The year is FreeType 2.14.1's, from `freetype.h`.
 */
const LINUX_OVERRIDES = {
  freetype: {
    origin: 'https://github.com/freetype/freetype at b91f75bd02db43b06d634591eb286d3eb0ce3b65 (FreeType 2.14.1)',
    creditYear: 2025,
  },
  zlib: {
    origin: 'https://github.com/phyrondev/chromium-zlib at 646b7f569718921d7d4b5b8e22572ff6c76f2596',
    ...committed('chromium-zlib.LICENSE'),
  },
}

/**
 * What each GN target in a Skia archive holds, by member name: `libharfbuzz.hb-face.o`,
 * or on Windows a path ending `harfbuzz.hb-face.obj`. A Windows path under
 * `third_party/externals/<dir>/` is placed by that directory instead, since its
 * `wuffs` target holds both Skia's codec and the library.
 */
export const GN_TARGETS = {
  // Skia's own sources.
  ...Object.fromEntries(
    [
      'clipstack_utils',
      'core',
      'gpu',
      'gpu_shared',
      'jpeg_decode',
      'jpeg_encode',
      'ml3',
      'ml4',
      'pathops',
      'pdf',
      'png_decode_common',
      'png_decode_libpng',
      'png_encode_common',
      'png_encode_libpng',
      'typeface_freetype',
      'typeface_proxy',
      'webp_decode',
      'webp_encode',
      'wuffs',
      'xml',
      'fontmgr_FontConfigInterface',
      'fontmgr_android',
      'fontmgr_android_parser',
      'fontmgr_custom',
      'fontmgr_custom_directory',
      'fontmgr_custom_embedded',
      'fontmgr_custom_empty',
      'fontmgr_fontconfig',
      'fontmgr_mac_ct',
      'fontmgr_win',
      'fontmgr_win_gdi',
      'libskia',
      'skia',
      'libskparagraph',
      'skparagraph',
      'libskresources',
      'skresources',
      'libskshaper',
      'skshaper',
      'libskunicode_core',
      'skunicode_core',
      'libskunicode_icu',
      'skunicode_icu',
      'libsvg',
      'svg',
    ].map(target => [target, ['skia']]),
  ),
  libskcms: ['skcms'],
  skcms: ['skcms'],
  skcms_TransformHsw: ['skcms'],
  skcms_TransformSkx: ['skcms'],
  libexpat: ['expat'],
  libharfbuzz: ['harfbuzz'],
  // `icu` on Windows holds Skia's `SkLoadICU` beside ICU itself.
  libicu: ['icu'],
  icu: ['skia', 'icu'],
  libjpeg: ['libjpeg-turbo'],
  libjpeg12: ['libjpeg-turbo'],
  libjpeg16: ['libjpeg-turbo'],
  libpng: ['libpng'],
  libwebp: ['libwebp'],
  libwebp_avx2: ['libwebp'],
  libwebp_sse41: ['libwebp'],
  libwuffs: ['wuffs'],
  libzlib: ['zlib'],
  zlib_adler32_simd: ['zlib'],
  zlib_arm_crc32: ['zlib'],
  zlib_crc32_simd: ['zlib'],
  zlib_inflate_chunk_simd: ['zlib'],
  libfreetype2: ['freetype'],
  libbrotli: ['brotli'],
  // Skia's wrapper, which compiles the allocator's implementation from its header.
  vulkanmemoryallocator: ['skia', 'vulkanmemoryallocator'],
}

/** Windows members under `third_party/externals/<dir>/`, by that directory. */
export const EXTERNALS = {
  expat: ['expat'],
  harfbuzz: ['harfbuzz'],
  icu: ['icu'],
  'libjpeg-turbo': ['libjpeg-turbo'],
  libpng: ['libpng'],
  libwebp: ['libwebp'],
  wuffs: ['wuffs'],
  zlib: ['zlib'],
  freetype: ['freetype'],
  brotli: ['brotli'],
}

/**
 * skia-bindings' own C++ glue, named `<16 hex digits>-<file>.o` by the `cc`
 * crate. It is skia-bindings' code, under the crate licence cargo-about lists.
 */
const CC_OBJECT = /^[0-9a-f]{16}-[^.]+\.o(bj)?$/

/**
 * The static libraries other packages' build scripts link, and what each
 * holds: a component above, or `OWN` for the package's own code, which
 * cargo-about already covers under the package's licence.
 */
const OWN = 'own'
export const NATIVE_LIBS = {
  'libaom-sys': { aom: ['libaom'] },
  'libz-sys': { z: ['zlib-libz-sys'] },
  'meo-skia-canvas': { brotli_command_table: ['brotli-vendored'], bdf_stub: OWN },
}

/**
 * Libraries a build script names without `static=`, by the OS `TARGETS` spells:
 * the platform's own, which the addon loads from the system and does not carry.
 * On Linux, fontconfig, FreeType and expat are shared objects on a host build;
 * a release image links its own static archives of them, `IMAGE_LIBS`.
 */
export const SYSTEM_LIBS = {
  darwin: ['c++'],
  // `dl` and `m` are glibc's, NEEDED by the linux-arm64-gnu release addon;
  // fontconfig, FreeType and expat are defined inside it, from `IMAGE_LIBS`.
  linux: ['stdc++', 'm', 'dl', 'fontconfig', 'freetype', 'expat'],
  win32: ['usp10', 'ole32', 'user32', 'gdi32', 'fontsub', 'advapi32'],
}

/**
 * What a Linux release image links by name from the static archives
 * `containers/install-fontlibs.sh` builds, with no shared object beside them,
 * so `-l<name>` reaches the archive. Each is read from the directory of
 * licence files the image kept for it, named for its source tarball.
 */
export const IMAGE_LIBS = { fontconfig: 'fontconfig', freetype: 'freetype', expat: 'expat' }

/** The script a release image builds its font libraries with, which pins each one's version. */
const FONTLIBS_SCRIPT = join(ROOT, 'containers', 'install-fontlibs.sh')

/**
 * The source directory each image library is built from, as the script pins
 * it: `FREETYPE=freetype-2.13.3` gives `freetype-2.13.3`. The licences an image
 * kept must be from these, so licences copied out of an image built from an
 * older script fail rather than describe the wrong version.
 */
export function imageVersions(script = readFileSync(FONTLIBS_SCRIPT, 'utf8')) {
  return Object.fromEntries(
    Object.values(IMAGE_LIBS).map(name => {
      const pinned = script.match(new RegExp(`^${name.toUpperCase()}=(${name}-[0-9.]+)$`, 'm'))
      return [name, pinned ? pinned[1] : refuse(`containers/install-fontlibs.sh pins no ${name.toUpperCase()}=${name}-<version>`)]
    }),
  )
}

/** The prefix of a component read from a release image's kept licences. */
const IMAGE = 'image:'

/** The component standing for every Skia archive, where meo-skia-canvas's notices describe them. */
const SKIA_ARCHIVE = 'skia-binaries'

/** The notices meo-skia-canvas puts in a skia-binaries archive, beside `key.txt` and `tag.txt`. */
export const ARCHIVE_NOTICES = 'THIRD-PARTY-NOTICES.md'

/** What a GitHub release download answers for an asset the release does not have. */
const HTTP_NOT_FOUND = 404

/** The FTL's credit line, which it asks for wherever FreeType is used. */
const FREETYPE_CREDIT = /Portions of this software are copyright \u00a9 \d{4} The FreeType Project \(www\.freetype\.org\)\. {1,2}All rights reserved\./g

/** Fails with a message naming what could not be placed. */
class NoticeError extends Error {}
const refuse = message => {
  throw new NoticeError(message)
}

/**
 * The member names of a static archive: the System V/GNU and BSD `ar` formats,
 * which also covers an MSVC `.lib`. Symbol tables and the long-name table are
 * not members, so they are skipped.
 */
export function archiveMembers(bytes) {
  if (bytes.toString('latin1', 0, 8) !== '!<arch>\n') refuse('not an ar archive (no `!<arch>` magic)')
  const members = []
  let longNames = ''
  let at = 8
  while (at + 60 <= bytes.length) {
    const header = bytes.toString('latin1', at, at + 60)
    if (header.slice(58, 60) !== '`\n') refuse(`malformed ar header at byte ${at}`)
    const size = Number(header.slice(48, 58).trim())
    if (!Number.isInteger(size) || size < 0) refuse(`malformed ar member size at byte ${at}`)
    const raw = header.slice(0, 16).trimEnd()
    const start = at + 60
    let name
    if (raw.startsWith('#1/')) {
      // BSD: the name follows the header and is counted in the size.
      const length = Number(raw.slice(3))
      name = bytes.toString('latin1', start, start + length).replace(/\0+$/, '')
    } else if (raw === '//') {
      longNames = bytes.toString('latin1', start, start + size)
      name = undefined
    } else if (raw === '/' || raw === '/SYM64/') {
      name = undefined
    } else if (/^\/\d+$/.test(raw)) {
      // GNU and MSVC: an offset into the long-name table, which GNU ends with
      // `/\n` and MSVC with a NUL.
      const offset = Number(raw.slice(1))
      const end = longNames.slice(offset).search(/\/\n|\0|\n/)
      name = longNames.slice(offset, end < 0 ? undefined : offset + end)
    } else {
      name = raw.endsWith('/') ? raw.slice(0, -1) : raw
    }
    if (name !== undefined && !name.startsWith('__.SYMDEF')) members.push(name)
    at = start + size + (size % 2)
  }
  if (at !== bytes.length) refuse(`ar archive ends inside a member header (byte ${at} of ${bytes.length})`)
  return members
}

/** The components one Skia archive member holds, or a refusal naming it. */
export function memberComponents(member) {
  const path = member.replace(/\\/g, '/')
  const external = path.match(/(?:^|\/)third_party\/externals\/([^/]+)\//)
  if (external) {
    return EXTERNALS[external[1]] ?? refuse(`archive member ${member} is under third_party/externals/${external[1]}, which no licence is mapped for`)
  }
  const file = path.slice(path.lastIndexOf('/') + 1)
  if (CC_OBJECT.test(file)) return []
  const target = file.slice(0, file.indexOf('.'))
  return GN_TARGETS[target] ?? refuse(`archive member ${member} belongs to GN target \`${target}\`, which no licence is mapped for`)
}

/** Parses cargo's `--message-format=json` output, one message per line. */
export function readMessages(text) {
  return text
    .split('\n')
    .filter(line => line.startsWith('{'))
    .map(line => JSON.parse(line))
}

/** `static=x`, `static:+whole-archive=x`, `framework=x`, `x`: the kind, without modifiers, and the name. */
function linkEntry(entry) {
  const at = entry.indexOf('=')
  return at < 0 ? { kind: '', name: entry } : { kind: entry.slice(0, at).split(':')[0], name: entry.slice(at + 1) }
}

/** A package id's name: `registry+https://...#name@1.2.3` or `path+file:///...#name@1.2.3`. */
export function packageNameOf(id) {
  const fragment = id.slice(id.lastIndexOf('#') + 1)
  return fragment.slice(0, fragment.lastIndexOf('@'))
}

/**
 * Every library the build scripts linked, with its kind and the directories
 * its build script named. Paths recorded inside a container are moved onto
 * this checkout, found by where the container saw the addon's own manifest.
 */
export function linkedLibraries(messages, root = ROOT) {
  const node = messages.find(
    m => m.reason === 'compiler-artifact' && m.manifest_path.replace(/\\/g, '/').endsWith(NODE_CRATE) && m.target.kind.includes('cdylib'),
  )
  if (node === undefined) refuse('the build messages hold no cdylib artefact for meo-canvas-node; record them from the build that made the addon')
  const recorded = node.manifest_path.replace(/\\/g, '/').slice(0, -NODE_CRATE.length)
  const local = `${root.replace(/\\/g, '/')}/`
  const moved = path => {
    const normal = path.replace(/\\/g, '/')
    return normal.startsWith(recorded) ? local + normal.slice(recorded.length) : path
  }
  const libraries = []
  for (const message of messages) {
    if (message.reason !== 'build-script-executed') continue
    const dirs = message.linked_paths.map(entry => moved(entry.includes('=') ? entry.slice(entry.indexOf('=') + 1) : entry))
    for (const entry of message.linked_libs) libraries.push({ pkg: packageNameOf(message.package_id), ...linkEntry(entry), dirs })
  }
  return { libraries, features: node.features }
}

/** The archive a static library was linked from: `lib<name>.a`, or `<name>.lib` on Windows. */
function archivePath(library) {
  for (const dir of library.dirs) {
    for (const file of [`lib${library.name}.a`, `${library.name}.lib`]) {
      if (existsSync(join(dir, file))) return join(dir, file)
    }
  }
  return refuse(`${library.pkg} links ${library.name} statically, but no lib${library.name}.a or ${library.name}.lib is in ${library.dirs.join(', ')}`)
}

/**
 * The components the build's libraries hold, each with the libraries it was found
 * in: skia-bindings' archives member by member, or as one with `archiveNotices`;
 * other static libraries through `NATIVE_LIBS`; a library linked by name through
 * `SYSTEM_LIBS`, or `IMAGE_LIBS` given the image's `imageLicences`.
 */
export function nativeComponents(libraries, os, { imageLicences, archiveNotices = false, readArchive = path => archiveMembers(readFileSync(path)) } = {}) {
  const found = new Map()
  const add = (component, where) => {
    if (COMPONENTS[component] === undefined && !component.startsWith(IMAGE) && component !== SKIA_ARCHIVE) refuse(`component ${component} has no licence entry`)
    if (!found.has(component)) found.set(component, new Set())
    found.get(component).add(where)
  }
  for (const library of libraries) {
    if (library.kind === 'framework' || library.kind === 'dylib') continue
    if (library.kind === '') {
      if (imageLicences !== undefined && IMAGE_LIBS[library.name] !== undefined) {
        add(`${IMAGE}${IMAGE_LIBS[library.name]}`, library.name)
      } else if (!SYSTEM_LIBS[os].includes(library.name)) {
        refuse(`${library.pkg} links ${library.name} by name, which SYSTEM_LIBS does not list as ${os}'s own; if it is static, map its licence`)
      }
      continue
    }
    if (library.kind !== 'static') refuse(`${library.pkg} links ${library.name} as \`${library.kind}\`, a kind this does not know`)
    if (library.pkg === 'skia-bindings') {
      const archive = archivePath(library)
      if (archiveNotices) add(SKIA_ARCHIVE, library.name)
      else for (const member of readArchive(archive)) for (const component of memberComponents(member)) add(component, library.name)
      continue
    }
    const held = NATIVE_LIBS[library.pkg]?.[library.name]
    if (held === undefined) refuse(`${library.pkg} links ${library.name} statically, and no licence is mapped for it in NATIVE_LIBS`)
    if (held !== OWN) for (const component of held) add(component, library.name)
  }
  return found
}

/** A URL's body, or `undefined` when it answers 404; any other failure fails the run. */
export async function fetchText(url) {
  let response
  try {
    response = await fetch(url, { redirect: 'follow' })
  } catch (cause) {
    return refuse(`fetching ${url} failed: ${cause.message}`)
  }
  if (response.status === HTTP_NOT_FOUND) return undefined
  if (!response.ok) refuse(`${url} answered ${response.status}`)
  return response.text()
}

/**
 * meo-skia-canvas's own notices for the Skia archive a build linked: the file
 * inside the archive, else the release asset named for it with `.tar.gz`
 * replaced by `.notices.md`. `undefined` when neither exists, which leaves the
 * archive to be read member by member; any other failure fails the run.
 */
export async function archiveNotices(archiveDir, binariesUrl, fetchAsset = fetchText) {
  const inside = join(archiveDir, ARCHIVE_NOTICES)
  if (existsSync(inside)) return { text: readFileSync(inside, 'utf8'), source: `${ARCHIVE_NOTICES} in the skia-binaries archive` }
  const keyFile = join(archiveDir, 'key.txt')
  if (!existsSync(keyFile)) refuse(`${archiveDir} holds no key.txt, so it is not a downloaded skia-binaries archive`)
  if (binariesUrl === undefined || !binariesUrl.includes('{key}') || !binariesUrl.endsWith('.tar.gz')) {
    refuse(`SKIA_BINARIES_URL (${binariesUrl}) does not name an archive as \`...{key}.tar.gz\`, so its notices asset cannot be named`)
  }
  const url = binariesUrl.replace('{key}', readFileSync(keyFile, 'utf8').trim()).replace(/\.tar\.gz$/, '.notices.md')
  const text = await fetchAsset(url)
  return text === undefined ? undefined : { text, source: url }
}

/** The directory skia-bindings' archives were linked from, one for all of them. */
function skiaArchiveDir(libraries) {
  const dirs = new Set(libraries.filter(l => l.pkg === 'skia-bindings' && l.kind === 'static').map(l => dirname(archivePath(l))))
  if (dirs.size !== 1) refuse(`expected skia-bindings' archives in one directory, found ${dirs.size}`)
  return [...dirs][0]
}

/** Plain error output from cargo and cargo-about, which colour it by default. */
const PLAIN = { ...process.env, CARGO_TERM_COLOR: 'never', NO_COLOR: '1' }

/** Runs a command and returns its standard output, or fails with its error output. */
function run(file, args) {
  try {
    return execFileSync(file, args, { cwd: ROOT, encoding: 'utf8', env: PLAIN, maxBuffer: 1 << 28, stdio: ['ignore', 'pipe', 'pipe'] })
  } catch (cause) {
    return refuse(`\`${file} ${args.join(' ')}\` failed:\n${`${cause.stderr ?? ''}`.trim()}`)
  }
}

/** Every package in the build graph for a target, by id, with its source directory. */
function packageDirs(triple) {
  const metadata = JSON.parse(run('cargo', ['metadata', '--locked', '--format-version', '1', '--filter-platform', triple, '--manifest-path', NODE_CRATE]))
  return {
    dirs: new Map(metadata.packages.map(p => [p.id, dirname(p.manifest_path)])),
    workspace: new Set(metadata.workspace_members),
  }
}

/** The committed licence texts, by their path under this directory. */
const NOTICES_DIR = join(HERE, 'notices')

/**
 * Crates whose licence text is a committed file, not what cargo-about finds in
 * their source. skia-bindings ships none, and a Skia compile downloads Skia's
 * licence files into its registry copy; its text is rust-skia's LICENSE at the
 * commit `.cargo_vcs_info.json` names for that version.
 */
export const CLARIFIED = {
  'skia-bindings': {
    version: '0.156.0',
    license: 'MIT',
    file: 'rust-skia.LICENSE',
    checksum: '4b33ed5b329a3a79eb1243fd91373c68b0ae0b9df7503bfc50d0c5253adf1b9a',
    origin: 'https://github.com/rust-skia/rust-skia at ce6727ef251518df37bcfdde4865b71f7e3a4352',
  },
}

/**
 * `about.toml` with a clarification for each of `CLARIFIED`. The path is
 * absolute, which cargo-about joins onto the crate's directory unchanged, so
 * it is written per run rather than committed.
 */
export function aboutConfig(base = readFileSync(ABOUT_CONFIG, 'utf8')) {
  const clarifications = Object.entries(CLARIFIED).map(
    ([name, { license, file, checksum }]) =>
      `[${name}.clarify]\nlicense = "${license}"\n\n[[${name}.clarify.files]]\npath = '${join(NOTICES_DIR, file)}'\nchecksum = "${checksum}"\n`,
  )
  return [base.trimEnd(), ...clarifications].join('\n\n')
}

/**
 * Refuses a report in which a clarified crate stands under any text but its
 * committed one: cargo-about only warns when a clarification fails to apply,
 * and then reads the crate's directory instead.
 */
export function checkClarified(about) {
  for (const [name, { version, file }] of Object.entries(CLARIFIED)) {
    const present = about.crates.filter(c => c.package.name === name)
    if (present.length === 0) continue
    const other = present.find(c => c.package.version !== version)
    if (other !== undefined) {
      refuse(
        `${name} ${other.package.version} is in the graph, but notices/${file} was read for ${version}; read its licence at the commit that version was published from`,
      )
    }
    const text = readFileSync(join(NOTICES_DIR, file), 'utf8')
    const texts = about.licenses.filter(l => l.used_by.some(use => use.crate.name === name)).map(l => l.text)
    if (texts.length !== 1 || texts[0] !== text) {
      refuse(`cargo-about gave ${name} ${texts.length} licence texts rather than notices/${file}, so its clarification did not apply`)
    }
  }
}

/**
 * cargo-about's report for a manifest, held to `about.toml` and `CLARIFIED`.
 * `--offline` keeps it to the license files in the crates themselves: without
 * it cargo-about also asks clearlydefined.io, whose answer can change between
 * two runs of one lockfile.
 */
export function cargoAbout(manifest, args) {
  const dir = mkdtempSync(join(tmpdir(), 'meo-canvas-about-'))
  try {
    const config = join(dir, 'about.toml')
    writeFileSync(config, aboutConfig())
    const about = JSON.parse(run('cargo', ['about', 'generate', '--locked', '--offline', '--fail', '-c', config, '-m', manifest, ...args, '--format', 'json']))
    checkClarified(about)
    return about
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
}

/** cargo-about's report for the addon on one target. */
export function rustLicences(triple, features) {
  run('cargo', ['fetch', '--locked', '--target', triple, '--manifest-path', NODE_CRATE])
  const featureArgs = features === 'all' ? ['--all-features'] : features.length === 0 ? [] : ['--features', features.join(' ')]
  return cargoAbout(NODE_CRATE, ['--target', triple, ...featureArgs])
}

/** The Skia tag skia-bindings' `Cargo.toml` names under `[package.metadata]`. */
function skiaTagOf(bindingsDir) {
  const manifest = readFileSync(join(bindingsDir, 'Cargo.toml'), 'utf8')
  const section = manifest.split(/^(?=\[)/m).find(part => part.startsWith('[package.metadata]\n'))
  const found = section?.match(/^skia = "([^"]+)"$/m)
  return found ? found[1] : refuse(`${bindingsDir}/Cargo.toml names no \`skia\` under [package.metadata]`)
}

/** The width of the rules between sections. */
const RULE = '='.repeat(80)

/** One licence text, under the path it was read from. */
const block = (label, text) => `--- ${label}\n\n${text.replace(/\r\n/g, '\n').trimEnd()}\n`

/**
 * The notices for one target. `skiaRelease` is the release `SKIA_BINARIES_URL`
 * names, which decides whether the Linux overrides apply.
 */
export function renderNotices({ suffix, triple, features, about, native, dirs, workspace, skiaRelease, imageLicences, archive }) {
  const linux = TARGETS[suffix].os.includes('linux')
  if (linux && Object.keys(LINUX_OVERRIDES).some(id => native.has(id)) && skiaRelease !== LINUX_ARCHIVE_RELEASE) {
    refuse(
      `SKIA_BINARIES_URL names ${skiaRelease}, but the Linux FreeType and zlib origins were read for ${LINUX_ARCHIVE_RELEASE}; ` +
        'read them again from the vendor submodules at the commit that release was built from, then update LINUX_ARCHIVE_RELEASE',
    )
  }
  const dirOf = name => {
    const matches = [...dirs.entries()].filter(([id]) => packageNameOf(id) === name)
    if (matches.length !== 1) refuse(`expected one ${name} in the build graph for ${triple}, found ${matches.length}`)
    return matches[0][1]
  }
  if ([...native.keys()].some(id => COMPONENTS[id]?.committed?.some(file => file.startsWith('skia/')))) {
    const tag = skiaTagOf(dirOf('skia-bindings'))
    if (tag !== SKIA_SOURCE.tag) {
      refuse(
        `skia-bindings builds Skia ${tag}, but the committed texts under notices/skia/ were read at ${SKIA_SOURCE.tag}; ` +
          'read them again from rust-skia/skia at that tag and each external at its DEPS revision, then update SKIA_SOURCE and COMPONENTS',
      )
    }
  }

  const sections = []
  const credits = []
  const freetypeCredit = year => `Portions of this software are copyright © ${year} The FreeType Project (www.freetype.org). All rights reserved.`
  for (const id of [...native.keys()].sort()) {
    const libs = [...native.get(id)].sort().join(', ')
    if (id === SKIA_ARCHIVE) {
      if (archive === undefined) refuse("the Skia archives were taken as one component, but meo-skia-canvas's notices for them were not given")
      // The credit leads the file wherever FreeType is in it, so it is lifted
      // from meo-skia-canvas's text rather than trusted to be read there.
      const lifted = archive.text.match(FREETYPE_CREDIT) ?? []
      if (/FreeType/.test(archive.text) && lifted.length === 0) refuse(`${archive.source} names FreeType but carries no FTL credit line`)
      credits.push(...lifted.map(line => line.replace(/ {2}/, ' ')))
      sections.push(
        `Skia and the libraries its archives link\nSource: ${archive.source}\nLinked through: ${libs}\n\n${archive.text.replace(/\r\n/g, '\n').trimEnd()}\n`,
      )
      continue
    }
    if (id.startsWith(IMAGE)) {
      const { name, texts, year } = imageComponent(imageLicences, id.slice(IMAGE.length))
      if (year !== undefined) credits.push(freetypeCredit(year))
      const note = year === undefined ? '' : `${FTL_CHOICE}\n`
      sections.push(`${name}\nSource: built from its release tarball by containers/install-fontlibs.sh\nLinked through: ${libs}\n${note}\n${texts.join('\n')}`)
      continue
    }
    const component = COMPONENTS[id]
    const override = linux ? LINUX_OVERRIDES[id] : undefined
    const origin = override?.origin ?? component.origin ?? `the ${dirOf(component.pkg).split(/[\\/]/).pop()} crate`
    const texts =
      (override?.committed ?? component.committed)?.map(file => block(file, readFileSync(join(NOTICES_DIR, file), 'utf8'))) ??
      component.files.map(file => {
        const path = join(dirOf(component.pkg), file)
        if (!existsSync(path)) refuse(`${component.name}'s licence is expected at ${path}, which does not exist`)
        return block(`${component.pkg}: ${file}`, readFileSync(path, 'utf8'))
      })
    if (id === 'freetype') credits.push(freetypeCredit(override?.creditYear ?? SKIA_SOURCE.freetypeCreditYear))
    const note = id === 'freetype' ? `${FTL_CHOICE}\n` : ''
    sections.push(`${component.name}\nSource: ${origin}\nLinked through: ${libs}\n${note}\n${texts.join('\n')}`)
  }

  const crates = []
  for (const licence of about.licenses) {
    const users = licence.used_by.filter(use => !workspace.has(use.crate.id)).map(use => `${use.crate.name} ${use.crate.version}`)
    if (users.length === 0) continue
    crates.push({
      key: `${licence.id}\0${users[0]}`,
      text: `${licence.name} (${licence.id})\nUsed by: ${[...new Set(users)].sort().join(', ')}\n\n${licence.text.replace(/\r\n/g, '\n').trimEnd()}\n`,
    })
  }
  crates.sort((a, b) => (a.key < b.key ? -1 : a.key > b.key ? 1 : 0))

  // `default` is cargo's name for the empty default set, not a feature chosen.
  const chosen = features.filter(feature => feature !== 'default')
  const head = [
    'THIRD-PARTY SOFTWARE NOTICES',
    '',
    `The meo-canvas addon for ${suffix} (${triple}${chosen.length > 0 ? `, features ${chosen.join(' ')}` : ''}).`,
    'meo-canvas itself is under the MIT licence in LICENSE. The addon also contains',
    'the third-party software below, each under the licence given with it.',
  ]
  if (credits.length > 0) head.push('', ...[...new Set(credits)].sort())
  return [head.join('\n'), `${RULE}\nC and C++ libraries\n${RULE}`, ...sections, `${RULE}\nRust crates\n${RULE}`, ...crates.map(crate => crate.text)].join(
    '\n\n',
  )
}

/**
 * A library a release image built, from the directory of licence files it
 * kept: `<name>-<version>/`, holding the texts and, for FreeType, `credit-year`.
 */
function imageComponent(root, name, versions = imageVersions()) {
  if (root === undefined || !existsSync(root)) refuse(`the release image's licences are not at ${root}; \`addon-container\` copies them beside its build`)
  const dirs = readdirSync(root).filter(dir => dir.startsWith(`${name}-`))
  if (dirs.length !== 1) refuse(`expected one ${name}-<version> in ${root}, found ${dirs.length}`)
  if (dirs[0] !== versions[name]) {
    refuse(`the image kept licences for ${dirs[0]}, but containers/install-fontlibs.sh builds ${versions[name]}; rebuild the image with \`addon-container\``)
  }
  const dir = join(root, dirs[0])
  const files = readdirSync(dir)
    .filter(file => file !== CREDIT_YEAR)
    .sort()
  if (files.length === 0) refuse(`${dir} holds no licence text`)
  const texts = files.map(file => block(`${dirs[0]}: ${file}`, readFileSync(join(dir, file), 'utf8')))
  let year
  if (name === 'freetype') {
    const path = join(dir, CREDIT_YEAR)
    year = existsSync(path) ? Number(readFileSync(path, 'utf8').trim()) : NaN
    if (!Number.isInteger(year)) refuse(`${path} must hold the year of FreeType's copyright line, for the FTL's credit`)
  }
  return { name: dirs[0], texts, year }
}

/**
 * FreeType is offered under the FTL or the GPLv2, and is used here under the
 * FTL, whose text follows and whose credit line heads the file.
 */
const FTL_CHOICE = 'Used under the FreeType License (FTL), one of the two licences FreeType offers; the other, the GPLv2, is not taken.'

/** The file beside FreeType's licences holding the year its credit line names. */
const CREDIT_YEAR = 'credit-year'

/** The release tag in `SKIA_BINARIES_URL`, which every recipe exports from the justfile. */
function skiaReleaseOf(url) {
  const found = url?.match(/\/releases\/download\/([^/]+)\//)
  return found ? found[1] : undefined
}

/**
 * The notices for one built target, from the build's recorded messages and,
 * for a release image, the licences it kept for the libraries it built.
 */
export async function generate(suffix, messagesFile, imageLicences, fetchAsset = fetchText) {
  const target = TARGETS[suffix] ?? refuse(`no target named ${suffix}; known: ${Object.keys(TARGETS).join(', ')}`)
  const messages = readMessages(readFileSync(messagesFile, 'utf8'))
  const { libraries, features } = linkedLibraries(messages)
  const binariesUrl = process.env.SKIA_BINARIES_URL
  const skiaRelease = skiaReleaseOf(binariesUrl)
  if (skiaRelease === undefined) refuse('SKIA_BINARIES_URL is not set; run this through `just`, which exports it')
  const archive = await archiveNotices(skiaArchiveDir(libraries), binariesUrl, fetchAsset)
  const native = nativeComponents(libraries, target.os[0], { imageLicences, archiveNotices: archive !== undefined })
  if (!native.has('skia') && !native.has(SKIA_ARCHIVE)) refuse(`the build in ${messagesFile} linked no Skia archive, so it is not the addon's build`)
  const about = rustLicences(target.rust, features)
  const { dirs, workspace } = packageDirs(target.rust)

  // cargo-about reads the lockfile; the build is what shipped. A crate it lists
  // that the build never compiled means the two were asked about different
  // targets or features.
  const compiled = new Set(messages.filter(m => m.reason === 'compiler-artifact').map(m => m.package_id))
  const extra = about.crates.map(c => c.package.id).filter(id => !workspace.has(id) && !compiled.has(id))
  if (extra.length > 0) refuse(`cargo-about lists crates the build did not compile: ${extra.map(packageNameOf).sort().join(', ')}`)

  return renderNotices({ suffix, triple: target.rust, features, about, native, dirs, workspace, skiaRelease, imageLicences, archive })
}

/** Holds every target's crate graph, with every feature, to the allowlist. */
export function checkAllTargets() {
  for (const [suffix, target] of Object.entries(TARGETS)) {
    const about = rustLicences(target.rust, 'all')
    process.stderr.write(`${suffix}: ${about.crates.length} crates, every licence accepted\n`)
  }
}

if (process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [mode, ...rest] = process.argv.slice(2)
  try {
    if (mode === '--check-all') {
      checkAllTargets()
    } else if (mode !== undefined && (rest.length === 2 || rest.length === 3)) {
      const [messagesFile, out, imageLicences] = rest
      writeFileSync(out, await generate(mode, messagesFile, imageLicences))
      process.stderr.write(`wrote ${out}\n`)
    } else {
      process.stderr.write(
        'usage: third-party-notices.mjs <suffix> <cargo build messages> <output file> [<release image licences>]\n' +
          '       third-party-notices.mjs --check-all\n',
      )
      process.exit(2)
    }
  } catch (error) {
    if (!(error instanceof NoticeError)) throw error
    process.stderr.write(`error: ${error.message}\n`)
    process.exit(1)
  }
}
