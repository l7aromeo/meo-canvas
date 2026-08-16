import typescript from '@rollup/plugin-typescript'
import { nodeResolve } from '@rollup/plugin-node-resolve'
import commonjs from '@rollup/plugin-commonjs'
import tsconfigPaths from 'rollup-plugin-tsconfig-paths'
import fs from 'fs'

const inputFiles = {
  ...fs
    .readdirSync('src')
    .filter(file => file.endsWith('.ts'))
    .reduce((acc, file) => {
      const name = file.replace('.ts', '')
      acc[name] = `src/${file}`
      return acc
    }, {}),
  'worker/render.worker': 'src/worker/render.worker.ts',
}

const common = {
  input: inputFiles,
  plugins: [tsconfigPaths(), nodeResolve({ exclude: 'node_modules/**' }), commonjs()],
  external: id => {
    return (
      ['meo-skia-canvas', 'yoga-layout', 'lodash-es', 'tinycolor2', 'file-type', 'node:fs', 'path', 'node:worker_threads'].includes(id) ||
      id.startsWith('tslib') ||
      id.startsWith('comlink')
    )
  },
}

// ESM only, because `yoga-layout` is: its entry awaits the WebAssembly module at the top level,
// and `require()` refuses an ESM graph containing a top-level await on every version of Node. A
// CommonJS build here would resolve, load, and then throw at the first import of the layout
// engine, which is a worse answer than not offering one.
export default [
  {
    ...common,
    plugins: [...common.plugins, typescript({ tsconfig: './tsconfig.esm.json' })],
    output: {
      dir: 'dist/esm',
      preserveModules: true,
      entryFileNames: chunkInfo => {
        return `${chunkInfo.name.replace('src/', '')}.js`
      },
      chunkFileNames: chunkInfo => {
        return `${chunkInfo.name.replace('src/', '')}.js`
      },
    },
  },
]
