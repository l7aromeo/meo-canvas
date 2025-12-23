import typescript from '@rollup/plugin-typescript'
import { nodeResolve } from '@rollup/plugin-node-resolve'
import commonjs from '@rollup/plugin-commonjs'
import tsconfigPaths from 'rollup-plugin-tsconfig-paths'
import fs from 'fs'

const inputFiles = fs
  .readdirSync('src')
  .filter(file => file.endsWith('.ts'))
  .reduce((acc, file) => {
    const name = file.replace('.ts', '')
    acc[name] = `src/${file}`
    return acc
  }, {})

const common = {
  input: inputFiles,
  plugins: [tsconfigPaths(), nodeResolve({ exclude: 'node_modules/**' }), commonjs()],
  external: id => {
    return ['skia-canvas', 'yoga-layout', 'lodash-es', 'tinycolor2', 'file-type', 'node:fs', 'path'].includes(id) || id.startsWith('tslib')
  },
}

export default [
  // ESM build
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
  // CJS build
  {
    ...common,
    plugins: [...common.plugins, typescript({ tsconfig: './tsconfig.cjs.json' })],
    output: {
      dir: 'dist/cjs',
      format: 'cjs',
      sourcemap: true,
      preserveModules: true,
      exports: 'named',
      entryFileNames: chunkInfo => {
        return `${chunkInfo.name.replace('src/', '')}.js`
      },
      chunkFileNames: chunkInfo => {
        return `${chunkInfo.name.replace('src/', '')}.js`
      },
    },
  },
]
