// Installs the packed tarballs into a throwaway project and renders with them.
//
// **Packing is not publishing and publishing is not installing.** `npm pack`
// reports a file list, which says a file is in the tarball and nothing about
// whether a consumer can reach it: `exports` can point at a path the allowlist
// dropped, `main` in a platform package can name a binary that is not there,
// and `optionalDependencies` can pin a version nobody built. Every one of those
// packs cleanly and fails at the first `import`.
//
// So this does what a consumer does — a directory that is not this repository,
// `npm install` from the tarballs, `import` by package name, render, check the
// bytes are a PNG. The addon is found the way it will be found in the wild,
// through the platform package rather than through the copy sitting in this
// working tree, because the in-tree path does not exist inside `node_modules`.
//
// Run by `just verify-pack` and by the release workflow, before anything is
// published.

import { execFileSync } from 'node:child_process'
import { mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

/** The first eight bytes of every PNG, which is what proves a render happened. */
const PNG_MAGIC = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])

/**
 * Running npm, which takes two Windows-only accommodations rather than one.
 *
 * Both failed the `win32-x64` release job at this step, after the addon had
 * built and packed cleanly -- the artefact was fine and the tool checking it
 * was not, twice over.
 *
 * **The name.** npm on Windows is `npm.cmd`, and `execFileSync` does not
 * consult PATHEXT: `spawnSync npm ENOENT` on a machine where `npm` works in
 * any shell.
 *
 * **The shell.** Naming it correctly then fails differently --
 * `spawnSync npm.cmd EINVAL` -- because Node refuses to spawn a `.cmd` or
 * `.bat` without an explicit shell, which is the fix for CVE-2024-27980: a
 * batch file re-parses its command line, so arguments could escape into it.
 * Passing `shell: true` is the supported way, **and it hands the quoting back
 * to us** -- which is exactly the hazard that CVE describes, so the paths are
 * quoted here rather than trusted to contain nothing a shell reads. They are
 * `mkdtemp` and `readdir` output, not user input, but a temporary directory
 * under `C:\Users\Given Name\` needs no malice to break this.
 */
const WINDOWS = process.platform === 'win32'
const NPM = WINDOWS ? 'npm.cmd' : 'npm'

/** One argument, safe to hand a shell on the platform that needs one. */
function arg(value) {
  return WINDOWS ? `"${value}"` : value
}

const releaseDir = resolve(process.argv[2] ?? 'release')
const tarballs = readdirSync(releaseDir)
  .filter(name => name.endsWith('.tgz'))
  .map(name => join(releaseDir, name))

if (tarballs.length < 2) {
  process.stderr.write(`expected the main tarball and at least one platform tarball in ${releaseDir}, found ${tarballs.length}\n`)
  process.exit(1)
}

const project = mkdtempSync(join(tmpdir(), 'meo-canvas-verify-'))
process.stderr.write(`verifying ${tarballs.length} tarballs in ${project}\n`)

try {
  writeFileSync(join(project, 'package.json'), `${JSON.stringify({ name: 'verify', private: true, type: 'module', version: '0.0.0' }, null, 2)}\n`)

  // Both at once, so npm resolves the main package's `optionalDependencies`
  // against the platform tarball rather than reaching the registry for a
  // version that is not published yet.
  execFileSync(NPM, ['install', '--silent', '--no-audit', '--no-fund', ...tarballs.map(arg)], {
    cwd: project,
    stdio: 'inherit',
    shell: WINDOWS,
  })

  const script = `
import { Box, Root } from 'meo-canvas'
const canvas = await Root({
  width: 120,
  height: 60,
  backgroundColor: '#101820',
  padding: 12,
  children: [Box({ width: 40, height: 20, backgroundColor: '#f2aa4c' })],
})
const bytes = await canvas.toBuffer('png')
canvas.release()
process.stdout.write(String(bytes.length))
`
  writeFileSync(join(project, 'render.mjs'), script)
  const size = Number(execFileSync(process.execPath, ['render.mjs'], { cwd: project, encoding: 'utf8' }))

  // The length alone would pass on an empty buffer or an error string, so the
  // magic is checked too: this asserts a PNG, not a truthy value.
  const bytes = execFileSync(
    process.execPath,
    [
      '--input-type=module',
      '-e',
      `${script.replace('process.stdout.write(String(bytes.length))', 'process.stdout.write(bytes.subarray(0, 8).toString("base64"))')}`,
    ],
    { cwd: project, encoding: 'utf8' },
  )
  if (!Buffer.from(bytes, 'base64').equals(PNG_MAGIC)) {
    throw new Error(`the render produced ${size} bytes that are not a PNG`)
  }

  process.stderr.write(`rendered ${size} bytes of PNG through the installed package\n`)
} finally {
  rmSync(project, { recursive: true, force: true })
}
