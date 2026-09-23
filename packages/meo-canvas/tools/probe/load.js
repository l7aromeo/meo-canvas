// Loads one addon and prints one word the caller parses. A mounted file rather than
// `node -e`, which has shell quoting to get wrong; `process.dlopen` rather than
// `require`, so a resolution failure cannot pass for a link failure.

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
