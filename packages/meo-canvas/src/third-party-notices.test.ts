// The notices tool's parts, each against inputs built here: archives in the three
// `ar` dialects the release archives use, cargo messages as a container records
// them, and package sources laid out in a temporary directory. The tool's run
// over a real build is `just notices-check` and `just pack`.

import { execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { createServer } from 'node:http'
import type { AddressInfo } from 'node:net'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { afterAll, describe, expect, it } from 'vitest'

import {
  ARCHIVE_NOTICES,
  CLARIFIED,
  COMPONENTS,
  EXTERNALS,
  GN_TARGETS,
  IMAGE_LIBS,
  LINUX_ARCHIVE_RELEASE,
  NATIVE_LIBS,
  SKIA_SOURCE,
  SYSTEM_LIBS,
  aboutConfig,
  archiveMembers,
  archiveNotices,
  cargoAbout,
  checkClarified,
  fetchText,
  imageVersions,
  linkedLibraries,
  memberComponents,
  nativeComponents,
  renderNotices,
  skiaArchiveDir,
  type AboutReport,
  type Linked,
} from '../tools/third-party-notices.mjs'

const HERE = dirname(fileURLToPath(import.meta.url))

/** One `ar` member: a 60-byte header, the data, and a pad byte to an even offset. */
function member(name: string, data: Buffer): Buffer {
  const header = `${name.padEnd(16)}${'0'.padEnd(12)}${'0'.padEnd(6)}${'0'.padEnd(6)}${'644'.padEnd(8)}${String(data.length).padEnd(10)}\`\n`
  return Buffer.concat([Buffer.from(header, 'latin1'), data, Buffer.alloc(data.length % 2, '\n')])
}

const archive = (...members: Buffer[]) => Buffer.concat([Buffer.from('!<arch>\n'), ...members])
const bytes = (text: string) => Buffer.from(text, 'latin1')

/** A BSD member, whose name follows the header and counts in its size. */
const bsd = (name: string, data: Buffer) => member(`#1/${name.length + 3}`, Buffer.concat([bytes(name), Buffer.alloc(3), data]))

describe('archiveMembers', () => {
  it('reads a GNU archive, skipping the symbol table and resolving long names', () => {
    const names = 'libharfbuzz.hb-face.o/\nlibskia.SkCanvas.o/\n'
    const read = archiveMembers(
      archive(
        member('/', bytes('\0\0\0\0')),
        member('//', bytes(names)),
        member('/0', bytes('odd')),
        member('/23', bytes('xx')),
        member('core.a.o/', bytes('y')),
      ),
    )
    expect(read).toEqual(['libharfbuzz.hb-face.o', 'libskia.SkCanvas.o', 'core.a.o'])
  })

  it('reads a BSD archive, skipping `__.SYMDEF` and trimming the padded name', () => {
    const read = archiveMembers(archive(bsd('__.SYMDEF SORTED', bytes('ab')), bsd('libskia.SkCamera.o', bytes('abc')), member('short.o', bytes('z'))))
    expect(read).toEqual(['libskia.SkCamera.o', 'short.o'])
  })

  it('reads an MSVC archive, whose long names end with NUL', () => {
    const names = 'obj/third_party/externals/zlib/zlib.adler32.obj\0obj/src/core/core.SkCanvas.obj\0'
    const read = archiveMembers(
      archive(member('/', bytes('1')), member('/', bytes('2')), member('//', bytes(names)), member('/0', bytes('a')), member('/48', bytes('b'))),
    )
    expect(read).toEqual(['obj/third_party/externals/zlib/zlib.adler32.obj', 'obj/src/core/core.SkCanvas.obj'])
  })

  it('refuses what is not an archive, and one that ends inside a member', () => {
    expect(() => archiveMembers(bytes('!<thin>\n'))).toThrow(/no `!<arch>` magic/)
    const whole = archive(member('a.o/', bytes('abcd')))
    expect(() => archiveMembers(whole.subarray(0, whole.length - 2))).toThrow(/ends inside a member/)
  })
})

describe('memberComponents', () => {
  it('places Apple and Linux members by GN target, and Windows ones by path first', () => {
    expect(memberComponents('libharfbuzz.hb-face.o')).toEqual(['harfbuzz'])
    expect(memberComponents('core.SkCanvas.o')).toEqual(['skia'])
    expect(memberComponents('libfreetype2.ftbase.o')).toEqual(['freetype'])
    // Windows' `wuffs` target names both, so the path decides.
    expect(memberComponents('obj/third_party/externals/wuffs/release/c/wuffs.wuffs-v0.3.obj')).toEqual(['wuffs'])
    expect(memberComponents('obj/src/codec/wuffs.SkWuffsCodec.obj')).toEqual(['skia'])
    expect(memberComponents('obj\\third_party\\externals\\zlib\\zlib.adler32.obj')).toEqual(['zlib'])
  })

  it("holds skia-bindings' own glue to its crate licence", () => {
    expect(memberComponents('98b6e9e47a0de5e6-bindings.o')).toEqual([])
  })

  it('refuses a GN target or an external it has no licence for', () => {
    expect(() => memberComponents('libnewcodec.decode.o')).toThrow(/GN target `libnewcodec`/)
    expect(() => memberComponents('obj/third_party/externals/dawn/src/dawn.x.obj')).toThrow(/externals\/dawn/)
  })

  it('maps every target and external onto a component with a licence entry', () => {
    for (const held of [...Object.values(GN_TARGETS), ...Object.values(EXTERNALS)]) {
      for (const component of held) expect(COMPONENTS, component).toHaveProperty([component])
    }
    for (const libs of Object.values(NATIVE_LIBS)) {
      for (const held of Object.values(libs)) if (held !== 'own') for (const component of held) expect(COMPONENTS, component).toHaveProperty([component])
    }
  })
})

/** The cdylib artefact the tool finds the recorded checkout by. */
const cdylib = (root: string, features: string[] = ['vulkan']) => ({
  reason: 'compiler-artifact',
  package_id: `path+file://${root}crates/meo-canvas-node#meo-canvas-node@1.0.0`,
  manifest_path: `${root}crates/meo-canvas-node/Cargo.toml`,
  target: { kind: ['cdylib'] },
  features,
})

describe('linkedLibraries', () => {
  it('moves paths a container recorded onto this checkout, and reads each kind', () => {
    const messages = [
      cdylib('/src/'),
      {
        reason: 'build-script-executed',
        package_id: 'registry+https://github.com/rust-lang/crates.io-index#skia-bindings@0.156.0',
        linked_libs: ['static=skia', 'static:+whole-archive=skshaper', 'stdc++', 'dylib=GL'],
        linked_paths: ['native=/src/target/container/x/release/build/skia-bindings-1/out/skia', '/elsewhere/lib'],
      },
    ]
    const { libraries, features } = linkedLibraries(messages, '/here')
    expect(features).toEqual(['vulkan'])
    expect(libraries.map(({ kind, name }) => `${kind}:${name}`)).toEqual(['static:skia', 'static:skshaper', ':stdc++', 'dylib:GL'])
    expect(libraries[0]?.dirs).toEqual(['/here/target/container/x/release/build/skia-bindings-1/out/skia', '/elsewhere/lib'])
    expect(libraries[0]?.pkg).toBe('skia-bindings')
  })

  it('refuses messages from a build that made no addon', () => {
    expect(() => linkedLibraries([{ ...cdylib('/src/'), target: { kind: ['lib'] } }])).toThrow(/no cdylib artefact/)
  })
})

const scratch = mkdtempSync(join(tmpdir(), 'meo-canvas-notices-'))
afterAll(() => rmSync(scratch, { recursive: true, force: true }))

/** Writes a file, making its directories. */
function put(path: string, text: string): string {
  mkdirSync(dirname(path), { recursive: true })
  writeFileSync(path, text)
  return path
}

const lib = (pkg: string, name: string, kind = 'static', dirs: string[] = []): Linked => ({ pkg, kind, name, dirs })

describe('nativeComponents', () => {
  const skiaDir = join(scratch, 'out', 'skia')
  put(join(skiaDir, 'libskia.a'), '')
  put(join(skiaDir, 'skshaper.lib'), '')
  const readArchive = (path: string) => (path.endsWith('libskia.a') ? ['core.SkCanvas.o', 'libzlib.adler32.o'] : ['libharfbuzz.hb-face.o'])

  it("reads skia-bindings' archives member by member", () => {
    const found = nativeComponents([lib('skia-bindings', 'skia', 'static', [skiaDir]), lib('skia-bindings', 'skshaper', 'static', [skiaDir])], 'darwin', {
      readArchive,
    })
    expect(Object.fromEntries([...found].map(([id, libs]) => [id, [...libs]]))).toEqual({ skia: ['skia'], zlib: ['skia'], harfbuzz: ['skshaper'] })
  })

  // skia-bindings' build script links its own archives on Windows by name, with no
  // `static=`, which it says breaks MSVC linking (rust-skia#354).
  it("reads skia-bindings' archives linked by name, as its Windows build does", () => {
    const windows = [lib('skia-bindings', 'skia', '', [skiaDir]), lib('skia-bindings', 'skshaper', '', [skiaDir]), lib('skia-bindings', 'usp10', '', [skiaDir])]
    const found = nativeComponents(windows, 'win32', { readArchive })
    expect(Object.fromEntries([...found].map(([id, libs]) => [id, [...libs]]))).toEqual({ skia: ['skia'], zlib: ['skia'], harfbuzz: ['skshaper'] })
    expect(skiaArchiveDir(windows)).toBe(skiaDir)
    // A name with no archive beside it is still the platform's or refused, and
    // only skia-bindings' own archives are read this way.
    expect(() => nativeComponents([lib('skia-bindings', 'svg', '', [skiaDir])], 'win32', { readArchive })).toThrow(/links svg by name/)
    expect(() => nativeComponents([lib('libaom-sys', 'skia', '', [skiaDir])], 'win32', { readArchive })).toThrow(/libaom-sys links skia by name/)
  })

  it('refuses an archive that is not where its build script said', () => {
    expect(() => nativeComponents([lib('skia-bindings', 'svg', 'static', [skiaDir])], 'darwin', { readArchive })).toThrow(/no libsvg\.a or svg\.lib/)
  })

  it("maps other packages' static libraries through NATIVE_LIBS, and their own code to nothing", () => {
    const found = nativeComponents([lib('libaom-sys', 'aom'), lib('meo-skia-canvas', 'bdf_stub')], 'linux')
    expect([...found.keys()]).toEqual(['libaom'])
    expect(() => nativeComponents([lib('libaom-sys', 'aom_extra')], 'linux')).toThrow(/aom_extra statically/)
    expect(() => nativeComponents([lib('new-sys', 'thing')], 'linux')).toThrow(/new-sys links thing statically/)
  })

  it("passes the platform's own libraries and refuses any other linked by name", () => {
    expect(nativeComponents([lib('skia-bindings', 'c++', ''), lib('skia-bindings', 'Metal', 'framework')], 'darwin').size).toBe(0)
    expect(nativeComponents([lib('skia-bindings', 'usp10', '')], 'win32').size).toBe(0)
    expect(() => nativeComponents([lib('skia-bindings', 'stdc++', '')], 'darwin')).toThrow(/stdc\+\+ by name/)
    expect(() => nativeComponents([lib('x-sys', 'png16', '')], 'linux')).toThrow(/png16 by name/)
    expect(() => nativeComponents([lib('x-sys', 'odd', 'weird')], 'linux')).toThrow(/a kind this does not know/)
  })

  it("reads a release image's font libraries from its licences, and a host build's as the system's", () => {
    const names = Object.keys(IMAGE_LIBS)
    const image = nativeComponents(
      names.map(name => lib('skia-bindings', name, '')),
      'linux',
      { imageLicences: scratch },
    )
    expect([...image.keys()].sort()).toEqual(names.map(name => `image:${name}`).sort())
    for (const name of names) expect(SYSTEM_LIBS.linux).toContain(name)
    expect(
      nativeComponents(
        names.map(name => lib('skia-bindings', name, '')),
        'linux',
      ).size,
    ).toBe(0)
  })
})

describe('renderNotices', () => {
  // skia-bindings as crates.io ships it: build.rs, build_support, src and a
  // Cargo.toml, and no `skia/` tree, which only a Skia compile downloads.
  const skia = join(scratch, 'registry', 'skia-bindings-0.156.0')
  const manifest = (tag: string) => `[package]\nname = "skia-bindings"\nversion = "0.156.0"\n\n[package.metadata]\nskia = "${tag}"\n\n[features]\n`
  put(join(skia, 'Cargo.toml'), manifest(SKIA_SOURCE.tag))
  const committed = (path: string) => readFileSync(resolve(HERE, '../tools/notices', path), 'utf8').trimEnd()
  const image = join(scratch, 'image')
  const pinned = imageVersions()
  put(join(image, pinned.freetype ?? '', 'FTL.TXT'), 'image FTL\n')
  put(join(image, pinned.freetype ?? '', 'credit-year'), '2024\n')
  put(join(image, pinned.fontconfig ?? '', 'COPYING'), 'fontconfig COPYING\n')

  const skiaId = 'registry+https://github.com/rust-lang/crates.io-index#skia-bindings@0.156.0'
  const workspaceId = 'path+file:///src/crates/meo-canvas-node#meo-canvas-node@1.0.0'
  const about: AboutReport = {
    crates: [],
    licenses: [
      {
        name: 'MIT License',
        id: 'MIT',
        text: 'MIT text\r\n',
        used_by: [
          { crate: { id: 'registry+x#b@1.0.0', name: 'b', version: '1.0.0' } },
          { crate: { id: workspaceId, name: 'meo-canvas-node', version: '1.0.0' } },
          { crate: { id: 'registry+x#a@2.0.0', name: 'a', version: '2.0.0' } },
        ],
      },
      { name: 'Ours only', id: 'MIT', text: 'not listed', used_by: [{ crate: { id: workspaceId, name: 'meo-canvas-node', version: '1.0.0' } }] },
    ],
  }
  const base = {
    triple: 'x',
    features: ['vulkan'],
    about,
    dirs: new Map([[skiaId, skia]]),
    workspace: new Set([workspaceId]),
    skiaRelease: LINUX_ARCHIVE_RELEASE,
  }
  const native = (...ids: string[]) => new Map(ids.map(id => [id, new Set(['skia'])]))

  it('reads every component of Skia from the committed texts, naming the revision each was read at', () => {
    const fromSkia = Object.keys(COMPONENTS).filter(id => COMPONENTS[id]?.committed !== undefined)
    expect(fromSkia).toHaveLength(13)
    const text = renderNotices({ ...base, features: ['default', 'metal'], suffix: 'darwin-arm64', native: native(...fromSkia) })
    expect(text).toContain('for darwin-arm64 (x, features metal).')
    for (const id of fromSkia) {
      for (const file of COMPONENTS[id]?.committed ?? []) expect(text).toContain(`--- ${file}\n\n${committed(file)}\n`)
    }
    expect(text).toContain(`Skia\nSource: ${SKIA_SOURCE.origin}\n`)
    expect(text).toContain('Source: https://chromium.googlesource.com/chromium/src/third_party/zlib at 456ae730017b9b4bd3371927abc82627fd51feef\n')
    expect(text).toContain(`Portions of this software are copyright © ${SKIA_SOURCE.freetypeCreditYear} The FreeType Project`)
    expect(text.split('Used under the FreeType License (FTL)').length - 1).toBe(1)
    expect(text).toContain('Used by: a 2.0.0, b 1.0.0\n\nMIT text\n')
    expect(text).not.toContain('meo-canvas-node 1.0.0')
    expect(text).not.toContain('not listed')
  })

  it('carries no credit line without FreeType', () => {
    const text = renderNotices({ ...base, suffix: 'darwin-arm64', native: native('skia', 'zlib') })
    expect(text).not.toContain('FreeType Project (www.freetype.org)')
    expect(text).not.toContain('Used under the FreeType License')
  })

  it("reads Linux's zlib from the committed text and credits each FreeType by its own year", () => {
    const text = renderNotices({
      ...base,
      suffix: 'linux-x64-gnu',
      native: native('freetype', 'zlib', 'image:freetype', 'image:fontconfig'),
      imageLicences: image,
    })
    expect(committed('chromium-zlib.LICENSE')).not.toBe(committed('skia/third_party/externals/zlib/LICENSE'))
    expect(text).toContain(committed('chromium-zlib.LICENSE'))
    expect(text).not.toContain('--- skia/third_party/externals/zlib/LICENSE')
    expect(text).toContain('phyrondev/chromium-zlib at 646b7f56')
    expect(text).toContain('Portions of this software are copyright © 2025 The FreeType Project')
    expect(text).toContain('Portions of this software are copyright © 2024 The FreeType Project')
    expect(text).not.toContain(`copyright © ${SKIA_SOURCE.freetypeCreditYear} The FreeType Project`)
    expect(text).toContain('fontconfig COPYING')
    // Once for the archive's FreeType and once for the image's.
    expect(text.split('Used under the FreeType License (FTL)').length - 1).toBe(2)
    expect(text).not.toContain('credit-year')
  })

  it('refuses a Linux build against a binaries release the overrides were not read for', () => {
    expect(() => renderNotices({ ...base, suffix: 'linux-x64-gnu', skiaRelease: 'skia-binaries-9.9.9-0', native: native('zlib') })).toThrow(/read them again/)
    // Only the overridden components depend on the release.
    expect(() => renderNotices({ ...base, suffix: 'linux-x64-gnu', skiaRelease: 'skia-binaries-9.9.9-0', native: native('skia') })).not.toThrow()
  })

  it('refuses a skia-bindings that builds another Skia than the committed texts were read at', () => {
    const other = join(scratch, 'registry', 'skia-bindings-0.157.0')
    put(join(other, 'Cargo.toml'), manifest('m157.0'))
    const dirs = new Map([[skiaId, other]])
    expect(() => renderNotices({ ...base, dirs, suffix: 'darwin-arm64', native: native('skcms') })).toThrow(/builds Skia m157\.0, but .* read at m156\.5/)
    // A crate's own texts do not depend on the Skia it builds.
    const libaom = 'registry+https://github.com/rust-lang/crates.io-index#libaom-sys@0.17.2'
    put(join(scratch, 'libaom-sys', 'vendor', 'LICENSE'), 'aom\n')
    put(join(scratch, 'libaom-sys', 'vendor', 'PATENTS'), 'aom patents\n')
    expect(() =>
      renderNotices({ ...base, dirs: new Map([...dirs, [libaom, join(scratch, 'libaom-sys')]]), suffix: 'darwin-arm64', native: native('libaom') }),
    ).not.toThrow()
    const unnamed = join(scratch, 'registry', 'skia-bindings-unnamed')
    put(join(unnamed, 'Cargo.toml'), '[package]\nname = "skia-bindings"\n\n[features]\nskia = "m156.5"\n')
    expect(() => renderNotices({ ...base, dirs: new Map([[skiaId, unnamed]]), suffix: 'darwin-arm64', native: native('skia') })).toThrow(
      /names no `skia` under \[package\.metadata\]/,
    )
  })

  it("refuses a crate's licence file that is not there, and an image's missing licences or credit year", () => {
    const libaom = 'registry+https://github.com/rust-lang/crates.io-index#libaom-sys@0.17.2'
    expect(() => renderNotices({ ...base, dirs: new Map([[libaom, join(scratch, 'empty')]]), suffix: 'darwin-arm64', native: native('libaom') })).toThrow(
      /does not exist/,
    )
    expect(() => renderNotices({ ...base, suffix: 'linux-x64-gnu', native: native('image:expat'), imageLicences: image })).toThrow(
      /expected one expat-<version>/,
    )
    expect(() => renderNotices({ ...base, suffix: 'linux-x64-gnu', native: native('image:fontconfig') })).toThrow(/licences are not at/)
    const noYear = join(scratch, 'no-year')
    put(join(noYear, pinned.freetype ?? '', 'FTL.TXT'), 'x\n')
    expect(() => renderNotices({ ...base, suffix: 'linux-x64-gnu', native: native('image:freetype'), imageLicences: noYear })).toThrow(/credit-year/)
  })

  it("states that the software is based in part on the IJG's work wherever libjpeg-turbo is linked", () => {
    const statement = 'This software is based in part on the work of the Independent JPEG Group.'
    const text = renderNotices({ ...base, suffix: 'darwin-arm64', native: native('skia', 'libjpeg-turbo') })
    const head = text.slice(0, text.indexOf('C and C++ libraries'))
    expect(head.split('\n')).toContain(statement)
    // The licence quotes the sentence wrapped across two lines; that is not the statement.
    expect(committed('skia/third_party/externals/libjpeg-turbo/LICENSE.md').split('\n')).not.toContain(statement)
    expect(renderNotices({ ...base, suffix: 'darwin-arm64', native: native('skia') })).not.toContain(statement)
  })

  it('refuses licences an image kept for a version the script no longer builds', () => {
    const stale = join(scratch, 'stale')
    put(join(stale, 'fontconfig-2.14.0', 'COPYING'), 'old\n')
    expect(() => renderNotices({ ...base, suffix: 'linux-x64-gnu', native: native('image:fontconfig'), imageLicences: stale })).toThrow(
      /kept licences for fontconfig-2\.14\.0, but .* builds fontconfig-2\.15\.0/,
    )
  })

  it('reads each pinned version from the script, and refuses one it cannot find', () => {
    expect(imageVersions()).toEqual({ fontconfig: 'fontconfig-2.15.0', freetype: 'freetype-2.13.3', expat: 'expat-2.6.4' })
    expect(() => imageVersions('EXPAT=expat-2.6.4\nFREETYPE=freetype-2.13.3\n')).toThrow(/pins no FONTCONFIG=/)
  })
})

describe('crates whose licence text is committed', () => {
  const notices = resolve(HERE, '../tools/notices')
  const clarified = CLARIFIED['skia-bindings']
  const text = readFileSync(join(notices, clarified?.file ?? ''), 'utf8')
  const report = (version: string, texts: string[]): AboutReport => ({
    crates: [{ package: { id: `registry+x#skia-bindings@${version}`, name: 'skia-bindings', version } }],
    licenses: texts.map(t => ({
      name: 'MIT License',
      id: 'MIT',
      text: t,
      used_by: [{ crate: { id: `registry+x#skia-bindings@${version}`, name: 'skia-bindings', version } }],
    })),
  })

  it('clarifies skia-bindings with the committed file, whose checksum is the one recorded', () => {
    const config = aboutConfig('accepted = ["MIT"]\n')
    expect(config).toContain(`[skia-bindings.clarify]\nlicense = "MIT"\n\n[[skia-bindings.clarify.files]]\npath = '${join(notices, 'rust-skia.LICENSE')}'\n`)
    expect(createHash('sha256').update(text).digest('hex')).toBe(clarified?.checksum)
    expect(config).toContain(`checksum = "${clarified?.checksum}"`)
  })

  it('refuses a report where the clarification did not apply, or another version is in the graph', () => {
    expect(() => checkClarified(report('0.156.0', [text]))).not.toThrow()
    expect(() => checkClarified({ crates: [], licenses: [] })).not.toThrow()
    expect(() => checkClarified(report('0.156.0', [text, 'MIT text from a downloaded tree']))).toThrow(
      /2 licence texts rather than notices\/rust-skia\.LICENSE/,
    )
    expect(() => checkClarified(report('0.156.0', ['MIT License\n']))).toThrow(/did not apply/)
    expect(() => checkClarified(report('0.157.0', [text]))).toThrow(/skia-bindings 0\.157\.0 is in the graph, but .* read for 0\.156\.0/)
  })

  // cargo-about over a crate named skia-bindings whose directory holds a
  // downloaded tree, as a registry copy does after a Skia compile.
  it('credits skia-bindings with the committed text whatever its directory holds', () => {
    const root = join(scratch, 'about-fixture')
    put(
      join(root, 'Cargo.toml'),
      '[package]\nname = "app"\nversion = "0.1.0"\nedition = "2024"\nlicense = "MIT"\n\n[dependencies]\nskia-bindings = { path = "skia-bindings" }\n',
    )
    put(join(root, 'src', 'lib.rs'), '')
    put(join(root, 'skia-bindings', 'Cargo.toml'), '[package]\nname = "skia-bindings"\nversion = "0.156.0"\nedition = "2024"\nlicense = "MIT"\n')
    put(join(root, 'skia-bindings', 'src', 'lib.rs'), '')
    const downloaded = text.replace('2019 LongYinan & Armin Sander', '2024 A Downloaded External')
    put(join(root, 'skia-bindings', 'skia', 'third_party', 'externals', 'x', 'LICENSE'), downloaded)
    const manifest = join(root, 'Cargo.toml')
    execFileSync('cargo', ['generate-lockfile', '--offline', '--manifest-path', manifest], { stdio: 'ignore' })
    const textsOf = (about: AboutReport) => about.licenses.filter(l => l.used_by.some(use => use.crate.name === 'skia-bindings')).map(l => l.text)

    // The control: without the clarification cargo-about reads the tree.
    const plain = JSON.parse(
      execFileSync('cargo', ['about', 'generate', '--locked', '--offline', '-c', resolve(HERE, '../../../about.toml'), '-m', manifest, '--format', 'json'], {
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'ignore'],
      }),
    ) as AboutReport
    expect(textsOf(plain)).toContain(downloaded)

    expect(textsOf(cargoAbout(manifest, []))).toEqual([text])

    // A checksum that does not match makes cargo-about warn and scan instead.
    const entry = CLARIFIED['skia-bindings'] as { checksum: string }
    const checksum = entry.checksum
    entry.checksum = '0'.repeat(64)
    try {
      expect(() => cargoAbout(manifest, [])).toThrow(/did not apply/)
    } finally {
      entry.checksum = checksum
    }
  }, 120_000)
})

describe("meo-skia-canvas's notices for the Skia archive", () => {
  const KEY = 'ce6727ef251518df37bc-aarch64-apple-darwin-ganesh'
  /** A fixture of the asset meo-skia-canvas publishes, with the FTL's credit as the FTL words it. */
  const FIXTURE =
    '# Third-party notices\n\nPortions of this software are copyright \u00a9 2025 The FreeType Project (www.freetype.org).  All rights reserved.\n\n## FreeType\n'

  /** A release server: the fixture at the archive's `.notices.md`, 500 for one name, 404 for the rest. */
  async function release<T>(use: (base: string, asked: string[]) => Promise<T>): Promise<T> {
    const asked: string[] = []
    const server = createServer((request, response) => {
      asked.push(request.url ?? '')
      if (request.url === `/download/skia-binaries-${KEY}.notices.md`) response.writeHead(200).end(FIXTURE)
      else if (request.url?.includes('broken')) response.writeHead(500).end()
      else response.writeHead(404).end()
    })
    await new Promise<void>(done => server.listen(0, '127.0.0.1', done))
    try {
      return await use(`http://127.0.0.1:${(server.address() as AddressInfo).port}/download`, asked)
    } finally {
      await new Promise(done => server.close(done))
    }
  }

  const archiveDir = (name: string, key = KEY) => {
    const dir = join(scratch, 'archives', name)
    put(join(dir, 'key.txt'), `${key}\n`)
    return dir
  }

  it('reads the asset named for the archive when the release has one', async () => {
    await release(async (base, asked) => {
      const found = await archiveNotices(archiveDir('asset'), `${base}/skia-binaries-{key}.tar.gz`)
      expect(asked).toEqual([`/download/skia-binaries-${KEY}.notices.md`])
      expect(found).toEqual({ text: FIXTURE, source: `${base}/skia-binaries-${KEY}.notices.md` })
    })
  })

  it('prefers the file inside the archive, asking the release nothing', async () => {
    const dir = archiveDir('inside')
    put(join(dir, ARCHIVE_NOTICES), 'inside text\n')
    const found = await archiveNotices(dir, 'https://example.test/skia-binaries-{key}.tar.gz', () => {
      throw new Error('asked the release')
    })
    expect(found).toEqual({ text: 'inside text\n', source: `${ARCHIVE_NOTICES} in the skia-binaries archive` })
  })

  it('falls back when the release has no asset, and fails on any other answer', async () => {
    await release(async base => {
      expect(await archiveNotices(archiveDir('absent', 'other-key'), `${base}/skia-binaries-{key}.tar.gz`)).toBeUndefined()
      await expect(fetchText(`${base}/broken`)).rejects.toThrow(/answered 500/)
    })
    await expect(fetchText('http://127.0.0.1:1/unreachable')).rejects.toThrow(/fetching .* failed/)
    await expect(archiveNotices(join(scratch, 'archives', 'nokey'), 'https://example.test/{key}.tar.gz')).rejects.toThrow(/no key\.txt/)
    await expect(archiveNotices(archiveDir('badurl'), 'https://example.test/fixed.tar.gz')).rejects.toThrow(/cannot be named/)
  })

  it('takes the archives as one component when the notices exist, and reads members when they do not', () => {
    const dir = join(scratch, 'one')
    put(join(dir, 'libskia.a'), '')
    const libraries = [lib('skia-bindings', 'skia', 'static', [dir])]
    const readArchive = () => ['libfreetype2.ftbase.o']
    expect([...nativeComponents(libraries, 'linux', { archiveNotices: true, readArchive }).keys()]).toEqual(['skia-binaries'])
    expect([...nativeComponents(libraries, 'linux', { readArchive }).keys()]).toEqual(['freetype'])
  })

  it('renders the notices whole, lifting the FTL credit to the head and skipping the Linux overrides', () => {
    const about: AboutReport = { crates: [], licenses: [] }
    const input = {
      suffix: 'linux-x64-gnu',
      triple: 'x',
      features: [],
      about,
      native: new Map([['skia-binaries', new Set(['skia'])]]),
      dirs: new Map(),
      workspace: new Set<string>(),
      skiaRelease: 'skia-binaries-9.9.9-0',
    }
    const text = renderNotices({ ...input, archive: { text: FIXTURE, source: 'the asset' } })
    const head = text.slice(0, text.indexOf('C and C++ libraries'))
    expect(head).toContain('Portions of this software are copyright \u00a9 2025 The FreeType Project (www.freetype.org). All rights reserved.')
    expect(text).toContain('Source: the asset')
    expect(text).toContain('## FreeType')
    expect(() => renderNotices({ ...input, archive: { text: 'FreeType, uncredited', source: 'the asset' } })).toThrow(/no FTL credit line/)
    const ijg = 'This software is based in part on the work of the Independent JPEG Group.'
    const jpeg = `${FIXTURE}\n## libjpeg-turbo\n\nthe Independent JPEG Group's licence\n`
    expect(() => renderNotices({ ...input, archive: { text: jpeg, source: 'the asset' } })).toThrow(
      /carries the IJG's licence but not the statement it requires/,
    )
    const stated = renderNotices({ ...input, archive: { text: `${jpeg}\n${ijg}\n`, source: 'the asset' } })
    expect(stated.slice(0, stated.indexOf('C and C++ libraries')).split('\n')).toContain(ijg)
    expect(() => renderNotices(input)).toThrow(/were not given/)
  })
})
