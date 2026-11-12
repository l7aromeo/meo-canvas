// The notices tool's parts, each against inputs built here: archives in the three
// `ar` dialects the release archives use, cargo messages as a container records
// them, and package sources laid out in a temporary directory. The tool's run
// over a real build is `just notices-check` and `just pack`.

import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { createServer } from 'node:http'
import type { AddressInfo } from 'node:net'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

import { afterAll, describe, expect, it } from 'vitest'

import {
  ARCHIVE_NOTICES,
  COMPONENTS,
  EXTERNALS,
  GN_TARGETS,
  IMAGE_LIBS,
  LINUX_ARCHIVE_RELEASE,
  NATIVE_LIBS,
  SYSTEM_LIBS,
  archiveMembers,
  archiveNotices,
  fetchText,
  imageVersions,
  linkedLibraries,
  memberComponents,
  nativeComponents,
  renderNotices,
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
  const skia = join(scratch, 'registry', 'skia-bindings-0.156.0')
  const externals = join(skia, 'skia', 'third_party', 'externals')
  put(
    join(skia, 'skia', 'DEPS'),
    '  "third_party/externals/zlib"   : "https://example.test/zlib@456ae730",\n  "third_party/externals/freetype" : "https://example.test/freetype@891f904e",\n',
  )
  put(join(skia, 'skia', 'LICENSE'), 'Skia licence text\n')
  put(join(externals, 'zlib', 'LICENSE'), 'zlib text from skia-bindings\n')
  put(join(externals, 'freetype', 'LICENSE.TXT'), 'FreeType LICENSE.TXT\n')
  put(join(externals, 'freetype', 'docs', 'FTL.TXT'), 'The FreeType Project LICENSE\n')
  put(join(externals, 'freetype', 'include', 'freetype', 'freetype.h'), ' * Copyright (C) 1996-2024 by\n')
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

  it("reads skia-bindings' texts off Linux, with no credit line without FreeType", () => {
    const text = renderNotices({ ...base, features: ['default', 'metal'], suffix: 'darwin-arm64', native: native('skia', 'zlib') })
    expect(text).toContain('for darwin-arm64 (x, features metal).')
    expect(text).toContain('zlib text from skia-bindings')
    expect(text).toContain('Source: https://example.test/zlib at 456ae730')
    expect(text).toContain('Skia licence text')
    expect(text).not.toContain('FreeType Project (www.freetype.org)')
    expect(text).not.toContain('Used under the FreeType License')
    expect(text).toContain('Used by: a 2.0.0, b 1.0.0\n\nMIT text\n')
    expect(text).not.toContain('meo-canvas-node 1.0.0')
    expect(text).not.toContain('not listed')
  })

  it("reads Linux's zlib from the committed text and credits each FreeType by its own year", () => {
    const text = renderNotices({
      ...base,
      suffix: 'linux-x64-gnu',
      native: native('freetype', 'zlib', 'image:freetype', 'image:fontconfig'),
      imageLicences: image,
    })
    expect(text).toContain(readFileSync(resolve(HERE, '../tools/notices/chromium-zlib.LICENSE'), 'utf8').trimEnd())
    expect(text).not.toContain('zlib text from skia-bindings')
    expect(text).toContain('phyrondev/chromium-zlib at 646b7f56')
    expect(text).toContain('Portions of this software are copyright © 2025 The FreeType Project')
    expect(text).toContain('Portions of this software are copyright © 2024 The FreeType Project')
    expect(text).toContain('fontconfig COPYING')
    // Once for the archive's FreeType and once for the image's.
    expect(text.split('Used under the FreeType License (FTL)').length - 1).toBe(2)
    expect(text).not.toContain('credit-year')
  })

  it("takes skia-bindings' FreeType year where nothing overrides it", () => {
    const text = renderNotices({ ...base, suffix: 'win32-x64', native: native('freetype') })
    expect(text).toContain('Portions of this software are copyright © 2024 The FreeType Project')
  })

  it('refuses a Linux build against a binaries release the overrides were not read for', () => {
    expect(() => renderNotices({ ...base, suffix: 'linux-x64-gnu', skiaRelease: 'skia-binaries-9.9.9-0', native: native('zlib') })).toThrow(/read them again/)
    // Only the overridden components depend on the release.
    expect(() => renderNotices({ ...base, suffix: 'linux-x64-gnu', skiaRelease: 'skia-binaries-9.9.9-0', native: native('skia') })).not.toThrow()
  })

  it("refuses a licence file that is not there, and an image's missing licences or credit year", () => {
    expect(() => renderNotices({ ...base, suffix: 'darwin-arm64', native: native('skcms') })).toThrow(/does not exist/)
    expect(() => renderNotices({ ...base, suffix: 'darwin-arm64', native: native('libpng') })).toThrow(/DEPS pins no third_party\/externals\/libpng/)
    expect(() => renderNotices({ ...base, suffix: 'linux-x64-gnu', native: native('image:expat'), imageLicences: image })).toThrow(
      /expected one expat-<version>/,
    )
    expect(() => renderNotices({ ...base, suffix: 'linux-x64-gnu', native: native('image:fontconfig') })).toThrow(/licences are not at/)
    const noYear = join(scratch, 'no-year')
    put(join(noYear, pinned.freetype ?? '', 'FTL.TXT'), 'x\n')
    expect(() => renderNotices({ ...base, suffix: 'linux-x64-gnu', native: native('image:freetype'), imageLicences: noYear })).toThrow(/credit-year/)
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
    expect(() => renderNotices(input)).toThrow(/were not given/)
  })
})
