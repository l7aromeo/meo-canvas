// Does the built addon load on a machine that is not the one that built it?
//
// This is the acceptance test for a release artefact, and for the Linux targets
// it is the only check that can pass it. The ABI ceilings the release workflow
// asserts compare version tags, and **an unversioned symbol has no tag to
// compare** -- a binary reporting `GLIBCXX_3.4.21`, under every ceiling, still
// failed to load on `undefined symbol: _M_replace_cold`, a GCC 12 symbol
// carrying no version. That is outside what a ceiling can measure rather than a
// gap in how carefully one is written. **The ceilings diagnose; this decides.**
//
// Adapted from the harness that produced the first real load table, which is
// where the two mistakes recorded in the comments below were made and found.
//
// # No font packages are installed anywhere here, on purpose
//
// The point is what a consumer gets, and a consumer running `node:22-slim` has
// no fontconfig. Installing one to make the test pass would be measuring our
// own setup script.
//
// # Three kinds of answer, and only one of them is about the binary
//
// A harness that reports "does not load" when it could not pull an image, or
// could not find the file, is worse than no harness: it fails in the same shape
// as the defect it exists to catch. So every precondition is established before
// the probe runs -- the binary is checked before any container starts, the
// image is pulled as its own step before anything is mounted -- and a failure
// before the probe cannot be a load failure, because the probe has not
// happened. Each row says which of the three it is:
//
//   answered    the probe ran and the binary either loaded or did not
//   softened    the probe ran on a host that is not the host we meant to test
//   unasked     the question could not be put at all
//
// Only `answered` rows can fail the run. A `softened` row is not a pass.

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

/**
 * The libraries whose absence is the point, checked before the load is read.
 *
 * **Installing node can undo the test.** Most of these images ship no node, and
 * a distribution's `nodejs` package may pull `fontconfig` in transitively --
 * which would install exactly what this exists to prove is absent, and report a
 * clean load on an image where a real consumer fails. Measured, that does not
 * happen on any of these images today, but "it does not happen today" is a fact
 * about package metadata nobody here controls.
 *
 * So the row is not trusted, it is checked. A row where either library is
 * present is `softened`: it says nothing about a machine without them, whether
 * they arrived with the image or with the install.
 */
const MUST_BE_ABSENT = ['libfontconfig.so.1', 'libfreetype.so.6']

/** How to get a `node` onto an image that has none. */
const INSTALL = {
  apt: 'apt-get update >/dev/null 2>&1 && apt-get install -y nodejs >/dev/null 2>&1',
  dnf: 'dnf -y install nodejs >/dev/null 2>&1 || yum -y install nodejs >/dev/null 2>&1',
  apk: 'apk add --no-cache nodejs >/dev/null 2>&1',
}

/**
 * The images each Linux target is answerable for.
 *
 * The `node:*` rows are the shapes people deploy into; the bare distributions
 * are the ABI floors the package name promises. Every list carries one
 * **control** — an image that ships the font libraries, so it can never be part
 * of the pass criterion, and is here only to show the binary is not inert. A
 * run where every row fails is usually a broken harness, and the control is
 * what tells that apart from a binary that works nowhere.
 */
const LINUX_IMAGES = {
  gnu: [
    { image: 'node:22', why: 'the control — ships the font libraries', control: true },
    { image: 'node:22-slim', why: 'the commonest way to deploy a Node app' },
    { image: 'debian:12-slim', why: 'glibc 2.36, comfortably above the floor', install: 'apt' },
    { image: 'rockylinux:9', why: 'glibc 2.34, GLIBCXX 3.4.29 — the RHEL 9 tier', install: 'dnf' },
    { image: 'amazonlinux:2023', why: 'glibc 2.34 — the AWS Lambda runtime', install: 'dnf' },
    { image: 'almalinux:8', why: 'glibc 2.28 — the oldest tier worth claiming', install: 'dnf' },
  ],
  musl: [
    { image: 'node:22-alpine', why: 'the control — ships the font libraries', control: true },
    { image: 'alpine:3.20', why: 'the musl baseline the package name promises', install: 'apk' },
  ],
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
 * One image, pulled and probed.
 *
 * **The library check does not short-circuit the load.** An earlier shape
 * returned `SOFTENED` and exited before running the probe, which made the
 * control row unable to do the one job it exists for: an image that ships the
 * libraries never actually loaded the binary, so it could not show the binary
 * was not inert. Both facts are gathered every time and combined afterwards.
 */
async function probe(addonName, { image, install }, platform) {
  // The pull is its own step so a registry failure is reported as one. Rolled
  // into `docker run`, it prints on the same stream as the load and reads
  // exactly like a binary that would not load.
  const pulled = await docker(['pull', '--platform', platform, '--quiet', image])
  if (!pulled.ok) return { kind: 'unasked', status: 'IMAGE_UNAVAILABLE', detail: pulled.out.split('\n').pop() }

  const present = MUST_BE_ABSENT.map(lib => `ls /usr/lib*/${lib} /usr/lib/*/${lib} /lib/*/${lib} /lib64/${lib} 2>/dev/null | head -1`).join('; ')
  const script = [
    install ? `${INSTALL[install]}; ` : '',
    'command -v node >/dev/null || { echo NO_NODE; exit 0; }; ',
    // Braces around the group with the pipe outside them. Written as
    // `$(ls ...; ls ... | tr)` the pipe binds to the LAST command only, so one
    // path was collapsed and the other was not — two lines out, and a reader
    // taking the last got a bare path with the word that gave it meaning on the
    // line above. It reported as unreadable rather than as softened.
    `found=$({ ${present}; } | tr '\\n' ' '); `,
    'echo "PRESENT $found"; ',
    `node /probe/load.js /probe/${addonName}`,
  ].join('')
  const ran = await docker(['run', '--rm', '--platform', platform, '-v', `${mount}:/probe:ro`, image, 'sh', '-c', script])

  return classify(ran.out)
}

/**
 * What one container's output means, as a pure function of that output.
 *
 * Separated from the container so it can be tested without one: the branches
 * here are where a harness misreports, and they are the part worth exercising
 * against fabricated output rather than against six real images.
 */
export function classify(out) {
  const lines = out.split('\n').filter(Boolean)
  if (lines.includes('NO_NODE')) return { kind: 'unasked', status: 'NO_NODE', detail: 'no node, and installing one did not work' }

  const found = (lines.find(line => line.startsWith('PRESENT')) ?? '').slice(8).trim()
  const verdict = lines[lines.length - 1] ?? ''

  const loaded = verdict.startsWith('LOADS')
  const outcome = loaded
    ? { status: 'LOADS', detail: `${verdict.split(' ')[1]} exports` }
    : verdict === 'REGISTERED_NOTHING'
      ? { status: 'FAILS', detail: 'loaded but registered no exports' }
      : verdict.startsWith('FAILS')
        ? { status: 'FAILS', detail: verdict.slice(6) }
        : { status: 'UNREADABLE', detail: lines[lines.length - 1] || 'no output at all' }

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
 * Whether a set of rows passes, and why not when it does not.
 *
 * Pure, and separate from printing, because the exit code is the whole product
 * of this harness and the rules behind it are the thing most worth pinning: a
 * run with nothing answered must not pass, and a softened row must never count
 * as one that did.
 */
export function decide(rows) {
  const answered = rows.filter(row => row.kind === 'answered')
  const broken = answered.filter(row => !row.loaded)
  const unasked = rows.filter(row => row.kind === 'unasked')
  const control = rows.find(row => row.control)

  if (answered.length === 0) return { ok: false, why: 'no image could be asked the question; this is a broken harness, not a passing binary' }
  if (control !== undefined && control.loaded !== true)
    return { ok: false, why: `the control ${control.image} did not load it, so every other row is uninformative` }
  if (broken.length > 0) return { ok: false, why: `${broken.length} image(s) cannot load this binary`, broken }
  if (unasked.length > 0) return { ok: false, why: `${unasked.length} image(s) could not be asked`, unasked }
  return { ok: true, why: `${answered.length} image(s) load it with no font packages installed` }
}

/**
 * A target with no container to load it in: macOS and Windows.
 *
 * There is one runner and one OS version per target, so "load in place" is the
 * whole test and no image matrix exists to invent. **The OS version is recorded
 * rather than asserted**, because it is the thing that moves silently when
 * GitHub updates a runner image, and a table that does not say which version it
 * was verified on cannot show that it moved.
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
    return { kind: 'answered', status: 'UNREADABLE', detail: `${verdict || 'no output at all'} — on ${where}`, loaded: false }
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

    process.stderr.write(`loading ${addon} for ${suffix}\n${'-'.repeat(78)}\n`)
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

  process.stderr.write(`${answered.length} image(s) load it with no font packages installed. This is what makes the package name honest.\n`)
  process.exit(unasked.length > 0 ? 1 : 0)
}
