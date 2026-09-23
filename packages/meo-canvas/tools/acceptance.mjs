// Does the built addon load on a machine that did not build it? The release's ABI
// ceilings compare version tags and an unversioned symbol such as `_M_replace_cold`
// has none, so the ceilings diagnose and this decides. No font package is
// installed anywhere: a consumer on `node:22-slim` has no fontconfig.

import { execFile } from 'node:child_process'
import { existsSync, statSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { promisify } from 'node:util'

const run = promisify(execFile)
const HERE = dirname(fileURLToPath(import.meta.url))

/** The checked-in probe, copied into a staging directory beside the artefact. */
const PROBE = resolve(HERE, 'probe')

/** The staging directory a container mounts. Set once a Linux run starts. */
let mount

/** What the artefact needs that a bare image of its libc does not carry. */
let extra

/**
 * The libraries whose absence is the point. A row where either is present --
 * shipped by the image or pulled in by an install -- is `softened`, since it says
 * nothing about a machine without them.
 */
const MUST_BE_ABSENT = ['libfontconfig.so.1', 'libfreetype.so.6']

/**
 * The official Node build, mounted read-only rather than installed, so no package
 * manager runs in an image under test: `dnf install nodejs` gives Node 10 on
 * AlmaLinux 8, and one built against a newer glibc will not run on `almalinux:8`.
 */
const NODE_BIN = '/probe/node/bin'

/** The Node build mounted into every image. Pinned; nothing resolves "latest". */
const NODE_VERSION = process.env['MEO_CANVAS_ACCEPTANCE_NODE'] ?? '22.12.0'

/** Node's own name for an architecture, which is not always `process.arch`'s. */
const NODE_ARCH = { x64: 'x64', arm64: 'arm64' }

/**
 * The images each Linux target is answerable for: the `node:*` shapes people
 * deploy into and the bare ABI floors. Each list carries one control that ships
 * the font libraries, so a run where everything fails can be told from a broken harness.
 */
const LINUX_IMAGES = {
  gnu: [
    { image: 'node:22', why: 'the control — ships the font libraries', control: true },
    { image: 'node:22-slim', why: 'the commonest way to deploy a Node app' },
    { image: 'debian:12-slim', why: 'glibc 2.36, comfortably above the floor' },
    { image: 'rockylinux:9', why: 'glibc 2.34, GLIBCXX 3.4.29 — the RHEL 9 tier' },
    { image: 'amazonlinux:2023', why: 'glibc 2.34 — the AWS Lambda runtime' },
    { image: 'almalinux:8', why: 'glibc 2.28 — the oldest tier worth claiming' },
  ],
  // Musl rows show the addon loads on the Alpine image people deploy into, not on
  // bare musl: nodejs.org has no musl build and `node:22-alpine`'s needs a
  // `libstdc++` bare `alpine:3.20` lacks. `requirements` covers that; no control
  // row exists, since `node:22-alpine` ships neither font library.
  musl: [{ image: 'node:22-alpine', why: 'the Alpine image people deploy into, and it ships neither font library' }],
}

/** The docker `--platform` value for a target suffix's architecture. */
const PLATFORM = { x64: 'linux/amd64', arm64: 'linux/arm64' }

/** Parses `linux-x64-musl` into the parts that choose an image list. */
function parse(suffix) {
  const [os, arch, libc] = suffix.split('-')
  return { os, arch, libc }
}

async function docker(args, timeout = 600_000) {
  try {
    const { stdout, stderr } = await run('docker', args, { timeout, maxBuffer: 8 << 20 })
    return { ok: true, out: `${stdout}${stderr}`.trim() }
  } catch (error) {
    return { ok: false, out: `${error.stdout ?? ''}${error.stderr ?? error.message}`.trim() }
  }
}

/**
 * One image, pulled and probed. The library check never short-circuits the load,
 * so the control row still shows the binary is not inert.
 */
async function probe(addonName, { image }, platform) {
  // The pull is its own step so a registry failure is reported as one. Rolled
  // into `docker run`, it prints on the same stream as the load and reads
  // exactly like a binary that would not load.
  const pulled = await docker(['pull', '--platform', platform, '--quiet', image])
  if (!pulled.ok) return { kind: 'unasked', status: 'IMAGE_UNAVAILABLE', detail: pulled.out.split('\n').pop() }

  const present = MUST_BE_ABSENT.map(lib => `ls /usr/lib*/${lib} /usr/lib/*/${lib} /lib/*/${lib} /lib64/${lib} 2>/dev/null | head -1`).join('; ')
  const script = [
    `export PATH=${NODE_BIN}:$PATH; `,
    'command -v node >/dev/null || { echo NO_NODE; exit 0; }; ',
    // Braces around the group with the pipe outside: in `$(ls ...; ls ... | tr)`
    // the pipe binds to the last command only and the output splits across lines.
    `found=$({ ${present}; } | tr '\\n' ' '); `,
    'echo "PRESENT $found"; ',
    `node /probe/load.js /probe/${addonName}`,
  ].join('')
  const ran = await docker(['run', '--rm', '--platform', platform, '-v', `${mount}:/probe:ro`, image, 'sh', '-c', script])

  return classify(ran.out)
}

/**
 * What one container's output means: `answered` (the probe ran), `softened` (a
 * font library was present; never a pass), `unasked` (no probe ran) or `ambiguous`
 * (unreadable, failing the run -- a segfault in `dlopen` prints nothing either).
 */
export function classify(out) {
  const lines = out.split('\n').filter(Boolean)
  if (lines.includes('NO_NODE')) return { kind: 'unasked', status: 'NO_NODE', detail: 'the mounted node did not run in this image' }

  const found = (lines.find(line => line.startsWith('PRESENT')) ?? '').slice(8).trim()
  const verdict = lines[lines.length - 1] ?? ''

  const loaded = verdict.startsWith('LOADS')
  const outcome = loaded
    ? { status: 'LOADS', detail: `${verdict.split(' ')[1]} exports` }
    : verdict === 'REGISTERED_NOTHING'
      ? { status: 'FAILS', detail: 'loaded but registered no exports' }
      : verdict.startsWith('FAILS')
        ? { status: 'FAILS', detail: verdict.slice(6) }
        : { kind: 'ambiguous', status: 'UNREADABLE', detail: lines[lines.length - 1] || 'no output at all' }

  if (found !== '') {
    // Either the image ships them or the install pulled them in. The
    // distinction does not matter to the verdict: the machine under test is no
    // longer a machine without them, so the row says nothing about one.
    return {
      kind: 'softened',
      status: outcome.status,
      detail: `${outcome.detail} — but ${found} is present, so this row proves nothing about a machine without it`,
      loaded,
    }
  }
  return { kind: 'answered', ...outcome, loaded }
}

/**
 * Whether a set of rows passes, and why not. The exit code is this harness's whole
 * product: nothing answered must not pass, and a softened row never counts.
 */
export function decide(rows) {
  const answered = rows.filter(row => row.kind === 'answered')
  const broken = answered.filter(row => !row.loaded)
  const unasked = rows.filter(row => row.kind === 'unasked')
  const ambiguous = rows.filter(row => row.kind === 'ambiguous')
  const control = rows.find(row => row.control)

  if (answered.length === 0) return { ok: false, why: 'no image could be asked the question; this is a broken harness, not a passing binary' }
  if (control !== undefined && control.loaded !== true)
    return { ok: false, why: `the control ${control.image} did not load it, so every other row is uninformative` }
  if (broken.length > 0) return { ok: false, why: `${broken.length} image(s) cannot load this binary`, broken }
  if (ambiguous.length > 0)
    return { ok: false, why: `${ambiguous.length} image(s) answered something this could not read; that is not a verdict either way`, ambiguous }
  if (unasked.length > 0) return { ok: false, why: `${unasked.length} image(s) could not be asked`, unasked }
  return { ok: true, why: `${answered.length} image(s) load it with no font packages installed` }
}

/**
 * The sonames a bare `alpine:3.20` and the oldest glibc tier carry. Anything else
 * an artefact needs, the consumer has to install.
 */
const CARRIED = {
  musl: ['libc.musl-x86_64.so.1', 'libc.musl-aarch64.so.1', 'ld-musl-x86_64.so.1', 'ld-musl-aarch64.so.1'],
  gnu: [
    'libc.so.6',
    'libm.so.6',
    'libdl.so.2',
    'libpthread.so.0',
    'librt.so.1',
    'libgcc_s.so.1',
    'libstdc++.so.6',
    'libz.so.1',
    'ld-linux-x86-64.so.2',
    'ld-linux-aarch64.so.1',
  ],
}

/**
 * The shared libraries an artefact demands, read from its `NEEDED` list with no
 * container: what the musl rows cannot see, `libstdc++` included. A diagnostic,
 * as the ABI floors are; the load is still the gate.
 */
async function requirements(addon, libc) {
  // `objdump -p` over `ldd`: `ldd` reports what resolves on the machine running
  // it, so in any environment that has the libraries it reports success about a
  // question nobody asked. The `NEEDED` entries are in the file.
  const read = await run('objdump', ['-p', addon], { maxBuffer: 8 << 20 }).catch(error => ({ stdout: '', error }))
  if (read.error !== undefined) return { ok: undefined, detail: `objdump could not read it: ${read.error.message}` }

  const needed = [...read.stdout.matchAll(/^\s*NEEDED\s+(\S+)/gm)].map(match => match[1])
  if (needed.length === 0) return { ok: undefined, detail: 'no NEEDED entries; not an ELF this can read' }

  const carried = CARRIED[libc] ?? []
  const extra = needed.filter(name => !carried.includes(name))
  return {
    ok: extra.length === 0,
    detail:
      extra.length === 0
        ? `${needed.length} libraries, all carried by a bare ${libc} image`
        : `needs ${extra.join(', ')}, which a bare ${libc} image does not carry`,
    needed,
  }
}

/**
 * The official Node build for `arch`, unpacked and ready to mount, cached under
 * the system temp directory. Its digest is checked against `SHASUMS256.txt` from
 * the same release before anything is unpacked.
 */
async function stageNode(arch, into) {
  const { createHash } = await import('node:crypto')
  const { cpSync, existsSync: cached, mkdirSync, writeFileSync } = await import('node:fs')
  const { tmpdir } = await import('node:os')

  const name = NODE_ARCH[arch]
  if (name === undefined) throw new Error(`no Node build is named for ${arch}`)
  const archive = `node-v${NODE_VERSION}-linux-${name}.tar.xz`
  const base = `https://nodejs.org/dist/v${NODE_VERSION}`
  const cache = resolve(tmpdir(), `meo-canvas-node-${NODE_VERSION}-${name}`)

  if (!cached(resolve(cache, 'bin/node'))) {
    mkdirSync(cache, { recursive: true })
    const [tarball, sums] = await Promise.all([
      fetch(`${base}/${archive}`).then(async response => {
        if (!response.ok) throw new Error(`${base}/${archive} answered ${response.status}`)
        return Buffer.from(await response.arrayBuffer())
      }),
      fetch(`${base}/SHASUMS256.txt`).then(async response => {
        if (!response.ok) throw new Error(`${base}/SHASUMS256.txt answered ${response.status}`)
        return response.text()
      }),
    ])

    const expected = sums
      .split('\n')
      .map(line => line.trim().split(/\s+/))
      .find(([, file]) => file === archive)?.[0]
    if (expected === undefined) throw new Error(`SHASUMS256.txt for v${NODE_VERSION} does not name ${archive}`)
    const actual = createHash('sha256').update(tarball).digest('hex')
    if (actual !== expected) throw new Error(`${archive} hashed ${actual}, and its release says ${expected}`)

    const staged = resolve(cache, archive)
    writeFileSync(staged, tarball)
    // `--strip-components=1` because the tarball's top level is the versioned
    // directory name, and the mount path must not carry a version.
    await run('tar', ['-xJf', staged, '-C', cache, '--strip-components=1'], { timeout: 300_000 })
  }

  cpSync(cache, resolve(into, 'node'), { recursive: true })
}

/**
 * A target with no container to load it in: macOS and Windows, one runner each.
 * The OS version is recorded rather than asserted, so a table shows when GitHub
 * moves the runner image.
 */
async function inPlace(addon) {
  const { release, version } = await import('node:os')
  const where = `${process.platform} ${process.arch}, kernel ${release()}, ${version()}`
  try {
    const { stdout, stderr } = await run(process.execPath, [resolve(PROBE, 'load.js'), addon], { timeout: 120_000 })
    const verdict = `${stdout}${stderr}`.trim().split('\n').filter(Boolean).pop() ?? ''
    if (verdict.startsWith('LOADS')) return { kind: 'answered', status: 'LOADS', detail: `${verdict.split(' ')[1]} exports on ${where}`, loaded: true }
    if (verdict === 'REGISTERED_NOTHING') return { kind: 'answered', status: 'FAILS', detail: `loaded but registered no exports on ${where}`, loaded: false }
    if (verdict.startsWith('FAILS')) return { kind: 'answered', status: 'FAILS', detail: `${verdict.slice(6)} — on ${where}`, loaded: false }
    return { kind: 'ambiguous', status: 'UNREADABLE', detail: `${verdict || 'no output at all'} — on ${where}`, loaded: false }
  } catch (error) {
    // The probe could not be started at all, which is this harness failing to
    // ask rather than the binary failing to load.
    return { kind: 'unasked', status: 'PROBE_FAILED', detail: `${error.message} — on ${where}` }
  }
}

// The command-line half runs only when this file is the command, the way
// `stage-platform-package.mjs` guards its own. Without it, importing `classify`
// from a test would start a docker run.
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  await main()
}

async function main() {
  const suffix = process.argv[2]
  const addon = resolve(process.argv[3] ?? 'packages/meo-canvas/meo-canvas.node')

  if (suffix === undefined) {
    process.stderr.write('usage: node acceptance.mjs <target-suffix> [path to .node]\n')
    process.exit(2)
  }
  if (!existsSync(addon) || !statSync(addon).isFile()) {
    // Before any container starts, so a missing artefact can never be reported as
    // a binary that would not load.
    process.stderr.write(`no addon at ${addon}\nusage: node acceptance.mjs <target-suffix> [path to .node]\n`)
    process.exit(2)
  }

  const { os, arch, libc } = parse(suffix)
  const rows = []

  if (os === 'linux') {
    const images = LINUX_IMAGES[libc]
    const platform = PLATFORM[arch]
    if (images === undefined || platform === undefined) {
      process.stderr.write(`no image list for ${suffix}; known: ${Object.keys(LINUX_IMAGES).join(', ')} on ${Object.keys(PLATFORM).join(', ')}\n`)
      process.exit(2)
    }
    // A staging directory outside the repository, holding the probe and the
    // artefact together, mounted read-only. **Not the source tree**: the addon is
    // tens of megabytes, and a harness that writes a binary into
    // `packages/meo-canvas/tools/` leaves it there for someone to commit.
    const { copyFileSync, mkdtempSync } = await import('node:fs')
    const { tmpdir } = await import('node:os')
    const name = 'addon.node'
    mount = mkdtempSync(resolve(tmpdir(), 'meo-canvas-acceptance-'))
    copyFileSync(resolve(PROBE, 'load.js'), resolve(mount, 'load.js'))
    copyFileSync(addon, resolve(mount, name))
    // Before any container starts, so a Node that could not be fetched reads as
    // this harness failing to ask rather than as six images failing to load.
    try {
      await stageNode(arch, mount)
    } catch (error) {
      process.stderr.write(`could not stage a node to mount: ${error.message}\n`)
      process.stderr.write('this is the harness failing to ask the question, not a binary that does not load\n')
      process.exit(2)
    }

    process.stderr.write(`loading ${addon} for ${suffix}\n${'-'.repeat(78)}\n`)

    // Read before anything is loaded, because it is about the artefact rather
    // than about any image, and because a `NEEDED` a bare image lacks explains
    // a load failure that follows rather than being explained by it.
    const needs = await requirements(addon, libc)
    const verdict = needs.ok === undefined ? 'UNREADABLE' : needs.ok ? 'CARRIED' : 'EXTRA'
    process.stderr.write(`${'requirements'.padEnd(20)} ${verdict.padEnd(12)} ${''.padEnd(10)} ${needs.detail}\n`)
    // A finding rather than a failure of the run: the load rows decide, and
    // this says which library to look at when they fail.
    if (needs.ok === false) extra = needs.detail

    for (const target of LINUX_IMAGES[libc]) {
      const result = await probe(name, target, platform)
      rows.push({ ...target, ...result })
      process.stderr.write(`${target.image.padEnd(20)} ${result.status.padEnd(12)} ${result.kind.padEnd(10)} ${result.detail}\n`)
    }
  } else {
    process.stderr.write(`loading ${addon} for ${suffix}\n${'-'.repeat(78)}\n`)
    const result = await inPlace(addon)
    rows.push({ image: `${suffix} runner`, why: 'the runner itself; there is no container to load it in', ...result })
    process.stderr.write(`${suffix.padEnd(20)} ${result.status.padEnd(12)} ${result.kind.padEnd(10)} ${result.detail}\n`)
  }
  process.stderr.write(`${'-'.repeat(78)}\n`)

  const answered = rows.filter(row => row.kind === 'answered')
  const broken = answered.filter(row => !row.loaded)
  const unasked = rows.filter(row => row.kind === 'unasked')
  const softened = rows.filter(row => row.kind === 'softened')
  const control = rows.find(row => row.control)

  for (const row of unasked) process.stderr.write(`could not ask ${row.image}: ${row.status} — ${row.detail}\n`)
  for (const row of softened) process.stderr.write(`not a pass, ${row.image}: ${row.detail}\n`)

  // **A run with no answered rows is a broken harness, not a passing binary.**
  // Every row softening or failing to pull would otherwise leave nothing in
  // `broken` and exit clean, which is the exact shape this file exists to refuse.
  if (answered.length === 0) {
    process.stderr.write('no image could be asked the question; this is a broken harness, not a passing binary\n')
    process.exit(1)
  }

  // The control loads the binary like any other row, and is read only for whether
  // it loaded at all. It cannot pass the run — it ships the libraries — but a
  // control that does not load says the binary is inert and every other row's
  // failure is uninformative.
  if (control !== undefined && control.loaded !== true) {
    process.stderr.write(`the control ${control.image} did not load it: ${control.detail}\n`)
    process.stderr.write('every other row is uninformative until that is understood\n')
    process.exit(1)
  }

  if (broken.length > 0) {
    process.stderr.write(`${broken.length} image(s) cannot load this binary:\n`)
    for (const row of broken) process.stderr.write(`  ${row.image} (${row.why}): ${row.detail}\n`)
    process.exit(1)
  }

  // Said after the rows, so a green run still carries it: every image tested may
  // happen to have the library and the next consumer's may not.
  if (extra !== undefined) process.stderr.write(`the artefact ${extra}\n`)

  process.stderr.write(`${answered.length} image(s) load it with no font packages installed. This is what makes the package name honest.\n`)
  process.exit(unasked.length > 0 ? 1 : 0)
}
