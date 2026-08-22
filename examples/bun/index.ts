// A consumer, not a test. It reaches meo-canvas the way anyone else would —
// through the package's exports rather than into its source — so it fails if
// the exports map, the types field or the entry point is wrong, none of which
// the test suite touches.
//
// The output is something to look at. What the renderer draws correctly is
// settled by the golden fixtures; this answers the different question of
// whether a person can use the package to draw anything at all.

import { Column, Root, Row, Text } from 'meo-canvas'

const canvas = await Root({
  width: 520,
  height: 180,
  backgroundColor: '#101014',
  children: Row({
    gap: 20,
    padding: 24,
    children: Column({
      gap: 6,
      justifyContent: 'center',
      children: [
        Text('Ukasyah Rahmatullah Zada', {
          fontSize: 26,
          fontWeight: 'bold',
          color: '#f4f4f6',
        }),
        Text('meo-canvas — <b>declarative</b> scenes, rendered in Rust', {
          fontSize: 15,
          color: '#8a8a94',
        }),
      ],
    }),
  }),
})

await canvas.toFile('out.png')
console.log('wrote out.png')
