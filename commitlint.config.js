/**
 * Commit messages are release input, not prose: semantic-release reads them to decide whether a
 * push publishes nothing, a patch, a minor, or a major. A message it cannot parse is not a style
 * problem — it is a version that never ships, discovered after the merge.
 */
export default {
  extends: ['@commitlint/config-conventional'],
  rules: {
    // The body carries the reasoning, and the reasoning is worth more than a column limit. Kept as
    // a warning so a long quoted error or stack trace does not block a commit.
    'body-max-line-length': [1, 'always', 100],
    'footer-max-line-length': [1, 'always', 100],
  },
}
