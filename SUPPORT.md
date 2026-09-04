# Support

Where to take a thing, depending on what it is.

**A security issue does not go in an issue.** Report it privately — see
[SECURITY.md](SECURITY.md) for the advisory form and the address. Please do not open a public issue
for a vulnerability, and please do not include a working exploit in one.

Everything else:

| What you have                           | Where it goes                                     |
| --------------------------------------- | ------------------------------------------------- |
| A bug, or a picture that came out wrong | An issue, using the bug report template           |
| A question, or "how do I…"              | An issue — there is no discussions board          |
| A missing capability                    | An issue, using the feature request template      |
| Something wrong in the documentation    | An issue, or a pull request if the fix is obvious |

There is no chat, no mailing list and no discussions board, so an issue is the only channel and a
question is a perfectly good reason to open one. Questions that turn out to be documentation gaps
are the most useful reports this project gets.

## What helps in a bug report

The smallest scene that shows it, and the image it produced. A rendered PNG says more than a
description of one, and a scene of five nodes is worth more than the real one of five hundred.

Say which surface — the npm package or the Rust crate — and which version. For the npm package,
`npm ls meo-canvas` also names the platform package that carried the binary, which matters whenever
the answer is "it draws differently on my machine".

If it is a rendering difference rather than a crash, say what you expected and where that
expectation comes from. This project targets Chrome's behaviour, so "Chrome does X" is the strongest
form of that argument and is usually acted on.

## What to expect

One maintainer, so answers are not immediate. A bug with a reproduction gets looked at sooner than
one without, not as a rule but because it can be.

If you are contributing rather than reporting, [CONTRIBUTING.md](CONTRIBUTING.md) has the build and
the checks.
