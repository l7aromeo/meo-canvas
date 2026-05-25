/**
 * Rewrites TypeScript path aliases (@/*) to relative paths in emitted .d.ts files.
 * Bun-native replacement for tsc-alias — avoids globby CJS interop issues under Bun.
 */
import fs from 'node:fs'
import path from 'node:path'

const outDirs = process.argv.includes('--cjs-only') ? ['dist/cjs'] : process.argv.includes('--esm-only') ? ['dist/esm'] : ['dist/esm', 'dist/cjs']

const aliasPattern = /@\/([A-Za-z0-9_./-]+)/g

function collectDeclarationFiles(dir: string): string[] {
  const files: string[] = []

  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, entry.name)
    if (entry.isDirectory()) {
      files.push(...collectDeclarationFiles(fullPath))
    } else if (entry.name.endsWith('.d.ts')) {
      files.push(fullPath)
    }
  }

  return files
}

function rewriteAliases(content: string, filePath: string, outDir: string): string {
  const fileDir = path.dirname(filePath)

  return content.replace(aliasPattern, (_match, importPath: string) => {
    const target = path.join(outDir, importPath)
    let relative = path.relative(fileDir, target).replace(/\\/g, '/')
    if (!relative.startsWith('.')) {
      relative = `./${relative}`
    }
    return relative
  })
}

let rewrittenFiles = 0

for (const outDir of outDirs) {
  if (!fs.existsSync(outDir)) {
    console.warn(`[rewrite-dts-aliases] skipping missing directory: ${outDir}`)
    continue
  }

  for (const filePath of collectDeclarationFiles(outDir)) {
    const original = fs.readFileSync(filePath, 'utf8')

    if (!original.includes('@/')) {
      continue
    }

    const updated = rewriteAliases(original, filePath, outDir)
    if (updated !== original) {
      fs.writeFileSync(filePath, updated)
      rewrittenFiles++
    }
  }
}

console.log(`[rewrite-dts-aliases] updated ${rewrittenFiles} declaration file(s)`)
