import { Root, Column, Box, Text, Style } from '../src/index.js'
import { Grid, GridItem } from '../src/canvas/grid.canvas.js'
import path from 'path'
import fs from 'fs'

/**
 * Helper to render a labeled section
 */
const Section = (title: string, content: any) => {
  return Column({
    gap: 10,
    padding: 20,
    children: [
      Text(title, { fontSize: 24, fontWeight: 'bold', color: '#333' }),
      Box({
        border: 1,
        borderColor: '#ddd',
        padding: 10,
        children: [content],
      }),
    ],
  })
}

/**
 * Helper to create a visual box
 */
const ColorBox = (color: string, text: string, height: number = 50) => {
  return Box({
    backgroundColor: color,
    height,
    width: '100%',
    justifyContent: Style.Justify.Center,
    alignItems: Style.Align.Center,
    children: [Text(text, { color: 'white', fontSize: 14, fontWeight: 'bold' })],
  })
}

const basicGrid = Grid({
  templateColumns: [100, 100, 100],
  gap: 10,
  children: [
    ColorBox('#FF5252', '1'),
    ColorBox('#FF5252', '2'),
    ColorBox('#FF5252', '3'),
    ColorBox('#FF5252', '4'),
    ColorBox('#FF5252', '5'),
    ColorBox('#FF5252', '6'),
  ],
})

const fractionalGrid = Grid({
  templateColumns: ['1fr', '2fr', '1fr'],
  gap: 10,
  children: [ColorBox('#448AFF', '1fr'), ColorBox('#448AFF', '2fr'), ColorBox('#448AFF', '1fr')],
})

const spanningGrid = Grid({
  templateColumns: ['1fr', '1fr', '1fr', '1fr'],
  gap: 10,
  children: [
    GridItem({
      gridColumn: 'span 4',
      children: [ColorBox('#69F0AE', 'Header (Span 4)')],
    }),
    GridItem({
      gridColumn: 'span 3',
      children: [ColorBox('#69F0AE', 'Main (Span 3)', 150)],
    }),
    GridItem({
      gridColumn: 'span 1',
      children: [ColorBox('#00C853', 'Sidebar', 150)],
    }),
    GridItem({
      gridColumn: 'span 2',
      children: [ColorBox('#69F0AE', 'Footer L (Span 2)')],
    }),
    GridItem({
      gridColumn: 'span 2',
      children: [ColorBox('#69F0AE', 'Footer R (Span 2)')],
    }),
  ],
})

const implicitRowsGrid = Grid({
  templateColumns: ['1fr', '1fr'],
  autoRows: 80,
  gap: 10,
  children: Array.from({ length: 5 }).map((_, i) => ColorBox('#E040FB', `Auto Row Item ${i + 1}`)),
})

// Main execution
;(async () => {
  try {
    const canvas = await Root({
      workerMode: false,
      width: 1200, // Wide enough for all
      backgroundColor: 'white',
      fontFamily: 'Inter', // Assuming Inter is available or defaults safely
      padding: 40,
      children: [
        Column({
          gap: 30,
          children: [
            Text('Grid Component Variants', { fontSize: 32, fontWeight: 'bold', margin: { Bottom: 20 } }),

            Section('1. Basic Fixed Pixels (100px cols)', basicGrid),

            Section('2. Fractional Units (1fr 2fr 1fr)', fractionalGrid),

            Section('3. Spanning Items (Header/Sidebar Layout)', spanningGrid),

            Section('4. Implicit Rows (autoRows: 80)', implicitRowsGrid),
          ],
        }),
      ],
    })

    const outDir = path.join(process.cwd(), 'samples')
    if (!fs.existsSync(outDir)) {
      fs.mkdirSync(outDir)
    }
    const outFile = path.join(outDir, 'sample_grids.png')
    await canvas.toFile(outFile)
    console.log(`Sample grids generated at: ${outFile}`)
  } catch (e) {
    console.error(e)
  }
})()
