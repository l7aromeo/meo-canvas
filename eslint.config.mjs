// ESLint, flat config. Prettier owns formatting; this owns everything else.
//
// `eslint-config-prettier` goes last so that no rule here argues with the
// formatter about whitespace -- the two tools are run together by `bun run
// lint`, and a rule that fails on something prettier just wrote is a rule
// that trains people to skip the gate.
//
// Type-aware rules are on. They are the ones that find a promise nobody
// awaits or a `!` that hides a real `undefined`, which are the defects this
// codebase has actually shipped; the cost is that ESLint parses with the
// TypeScript program, which is why `parserOptions.projectService` is set and
// why the root TypeScript is pinned to a version typescript-eslint supports.

import eslint from '@eslint/js'
import prettier from 'eslint-config-prettier'
import globals from 'globals'
import tseslint from 'typescript-eslint'

export default tseslint.config(
  {
    // What is never linted: build output, installed packages, generated
    // tables, the vendored v1 layer kept in its own style so it can be diffed
    // against upstream, and the golden fixtures. Mirrors `.prettierignore`.
    ignores: [
      '**/dist/**',
      '**/node_modules/**',
      '**/target/**',
      'fixtures/**',
      'packages/*/vendor/**',
      'packages/*/src/generated/**',
      'release/**',
      'coverage/**',
      // Agent scratch: probes and reports, gitignored at `.gitignore`. They are
      // browser snippets run through a headless Chrome, so `document` and
      // `getComputedStyle` are undefined to a lint that reads them as Node.
      // eslint does not read `.gitignore`, so the two lists are kept in step by
      // hand.
      '.tmp/**',
      // Config files at the root belong to no TypeScript project.
      '*.config.mts',
      '*.config.mjs',
    ],
  },
  eslint.configs.recommended,
  ...tseslint.configs.recommendedTypeChecked,
  {
    languageOptions: {
      parserOptions: {
        // The test config is the one that includes everything under `src`,
        // tests included; the package's own tsconfig excludes them, and the
        // project service would otherwise refuse to parse any `.test.ts`.
        project: ['packages/meo-canvas/tsconfig.test.json', 'examples/bun/tsconfig.json'],
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      // Off, on purpose. `Canvas.toBuffer` and `toURL` are `async` with no
      // `await` because the contract is a rejection, not a throw -- AGENTS.md
      // "Throwing and rejecting are different failures". Dropping `async` to
      // satisfy this rule would turn a validation error into a synchronous
      // throw that a caller's `.catch` never sees.
      '@typescript-eslint/require-await': 'off',
      // The surface is TypeScript; a `.mjs` tool that reads JSON is allowed
      // to say `any` about it once. Elsewhere, a real type or `unknown`.
      // A parameter a signature forces on you may be named `_x` and left
      // unused. A variable may not: an unused variable is dead code, and
      // `const _ = x` to silence the rule is the thing the rule exists to stop.
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_', caughtErrorsIgnorePattern: '^_' }],
      '@typescript-eslint/no-explicit-any': 'error',
      // A floating promise is the failure mode of every `async` render call
      // in this package: the error goes nowhere and the process exits 0.
      '@typescript-eslint/no-floating-promises': 'error',
      // `${node}` where `node` is an object prints `[object Object]` and no
      // test notices, because the string is still truthy.
      '@typescript-eslint/restrict-template-expressions': ['error', { allowNumber: true }],
    },
  },
  {
    // Everything here runs under Node: the surface, the tests, the tools.
    languageOptions: { globals: globals.node },
  },
  {
    // Plain `.mjs` tools and configs are not part of a TypeScript project;
    // parse them as scripts without type information rather than failing on
    // "file not included in any tsconfig".
    files: ['**/*.mjs', '**/*.js'],
    ...tseslint.configs.disableTypeChecked,
  },
  {
    // The conformance scripts drive a browser through playwright and pass
    // functions that execute *in the page* -- `document`, `getComputedStyle`.
    // Those names are real there and undefined everywhere else.
    files: ['packages/meo-canvas/tools/conformance/**/*.mjs'],
    languageOptions: { globals: { ...globals.node, ...globals.browser } },
  },
  prettier,
)
