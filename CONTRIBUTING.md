# Contributing to @meonode/canvas

First off, thank you for considering contributing! It's people like you that make open source such a great community.

## Where do I go from here?

If you've noticed a bug or have a feature request, [make one](https://gitlab.com/meonode/canvas/issues/new)! It's generally best if you get confirmation of your bug or approval for your feature request this way before starting to code.

### Fork & create a branch

If this is something you think you can fix, then [fork @meonode/canvas](https://gitlab.com/meonode/canvas/-/forks/new) and create a branch with a descriptive name.

A good branch name would be (where issue #38 is the ticket you're working on):

```sh
git checkout -b 38-add-gaussian-blur-support
```

### Get the project running

At this point, you're ready to make your changes! Feel free to ask for help; everyone is a beginner at first :smile_cat:

1.  Install dependencies:
    ```sh
    yarn install
    ```

2.  Run the linter to ensure your code follows the project's style guidelines:
    ```sh
    yarn lint
    ```

3.  Run the tests to make sure everything is working as expected:
    ```sh
    yarn test
    ```

4.  Build the project to generate the distribution files:
    ```sh
    yarn build
    ```

### Make your changes

Now, go to town on your feature or bug fix.

### Commit your changes

Make sure your commit messages are clear and descriptive.

### Push to your fork and submit a pull request

At this point, you should switch back to your master branch and make sure it's up to date with the latest upstream master.

```sh
git remote add upstream git@gitlab.com:meonode/canvas.git
git checkout master
git pull upstream master
```

Then, update your feature branch from your local copy of master, and push it!

```sh
git checkout 38-add-gaussian-blur-support
git rebase master
git push --force-with-lease origin 38-add-gaussian-blur-support
```

Finally, go to GitLab and [make a Merge Request](https://gitlab.com/meonode/canvas/-/merge_requests/new) :D

We're happy to help you get your PR reviewed and merged.

---

*This contribution guide was adapted from the [React-Boilerplate guide](https://github.com/react-boilerplate/react-boilerplate).*
