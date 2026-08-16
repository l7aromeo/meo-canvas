# Contributing to meo-canvas

First off, thank you for considering contributing! It's people like you that make open source such a great community.

Taking part means following the [Code of Conduct](CODE_OF_CONDUCT.md). Found a security problem rather than a bug? [SECURITY.md](SECURITY.md) says how to report it privately.

## Where do I go from here?

If you've noticed a bug or have a feature request, [make one](https://github.com/l7aromeo/meo-canvas/issues/new)! It's generally best if you get confirmation of your bug or approval for your feature request this way before starting to code.

### Fork & create a branch

If this is something you think you can fix, then [fork meo-canvas](https://github.com/l7aromeo/meo-canvas/fork) and create a branch with a descriptive name.

A good branch name would be (where issue #38 is the ticket you're working on):

```sh
git checkout -b 38-add-gaussian-blur-support
```

### Get the project running

At this point, you're ready to make your changes! Feel free to ask for help; everyone is a beginner at first :smile_cat:

1.  Install dependencies:

    ```sh
    bun install
    ```

2.  Run the linter to ensure your code follows the project's style guidelines:

    ```sh
    bun run lint
    ```

3.  Run the tests to make sure everything is working as expected:

    ```sh
    bun run test
    ```

    > **Note:** Use `bun run test` (Vitest), not bare `bun test`. The Bun test runner does not load Vitest globals (`describe`, `vi`, etc.).

4.  Build the project to generate the distribution files:

    ```sh
    bun run build
    ```

5.  Run the integration tests, which render through the real engine rather than mocks:

    ```sh
    bun run test:integration
    ```

    > **Note:** Run these after `bun run build`. The worker pool starts a worker by path (`render.worker.js`), which
    > only exists once the package has been built — the worker-mode cases skip themselves without it.

### Make your changes

Now, go to town on your feature or bug fix.

### Commit your changes

Commit messages here are release input, not prose. Releases are cut by
[semantic-release](https://semantic-release.gitbook.io/), which reads the messages merged into `main` and decides from
them whether to publish nothing, a patch, a minor, or a major. A message it cannot parse is not a style problem — it is
a version that never ships, found after the merge.

So they follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<optional scope>): <description>

<optional body>

<optional footer>
```

The type is what decides the version:

| Type                                                                | Release | Example                                                 |
| ------------------------------------------------------------------- | ------- | ------------------------------------------------------- |
| `fix`                                                               | patch   | `fix(Image): honour objectPosition on a cropped source` |
| `feat`                                                              | minor   | `feat(animate): add a spring solver`                    |
| any type with `!`, or a `BREAKING CHANGE:` footer                   | major   | `feat(Root)!: pages replace the children array`         |
| `docs`, `test`, `chore`, `build`, `ci`, `refactor`, `perf`, `style` | none    | `docs: document the frame prop`                         |

The scope is the thing you changed — a component (`Image`, `Root`, `Grid`), a module (`animate`, `build`), or nothing
at all when the change is broad.

Say why in the body, not just what. The diff already says what.

A `commit-msg` hook runs [commitlint](https://commitlint.js.org/) over the message, so a malformed one is rejected as
you write it rather than after review. It is installed by `bun install`.

### Push to your fork and submit a pull request

At this point, you should switch back to your main branch and make sure it's up to date with the latest upstream main.

```sh
git remote add upstream git@github.com:l7aromeo/meo-canvas.git
git checkout main
git pull upstream main
```

Then, update your feature branch from your local copy of main, and push it!

```sh
git checkout 38-add-gaussian-blur-support
git rebase main
git push --force-with-lease origin 38-add-gaussian-blur-support
```

Finally, go to GitHub and [open a Pull Request](https://github.com/l7aromeo/meo-canvas/compare) :D

We're happy to help you get your PR reviewed and merged.

---

_This contribution guide was adapted from the [React-Boilerplate guide](https://github.com/react-boilerplate/react-boilerplate)._
