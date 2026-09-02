// Loads one addon and says what happened, in one word the caller can parse.
//
// **A file rather than `node -e`.** The first version of this passed the script
// through the shell inside `node -e`, and every row of the resulting table came
// back unreadable -- including the image that was known to load. The escaping
// mangled the script, node died on a syntax error, and the harness reported six
// failures for a binary with one. A suite that is uniformly wrong looks exactly
// like a binary that works nowhere, and there were no agreeing rows to say
// otherwise. A mounted file has no quoting to get wrong.
//
// `process.dlopen` rather than `require`, because it is the load itself under
// test: `require` would add module resolution to the list of things that could
// fail, and a resolution failure and a link failure are the two things this
// whole harness exists to keep apart.

const addon = process.argv[2]
if (addon === undefined) {
  console.log('FAILS no addon path was passed to the probe')
  process.exit(0)
}

try {
  const module = { exports: {} }
  process.dlopen(module, addon)
  const exported = Object.keys(module.exports).length
  // Loading and registering nothing is a different failure from not loading,
  // and both print as success to anything that only checks for a throw.
  console.log(exported > 0 ? `LOADS ${exported}` : 'REGISTERED_NOTHING')
} catch (error) {
  console.log(`FAILS ${String(error.message).split('\n')[0]}`)
}
