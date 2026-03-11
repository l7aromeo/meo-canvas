import { Root, Column, Box, Row, Text, Style, Grid } from '../src/index.js'
import { GridItem } from '../src/canvas/grid.canvas.util.js'
import tinycolor from 'tinycolor2'
import path from 'path'
import fs from 'fs'

// ─── Theme ──────────────────────────────────────────────────────────────────────
const Theme = {
  primaryColor: '#6C5CE7',
  secondaryColor: '#00B894',
  accentColor: '#FD79A8',
  paperColor: '#FAFAFA',
  darkColor: '#2D3436',
  surfaceColor: '#DFE6E9',
}

// ─── Helpers ────────────────────────────────────────────────────────────────────

/**
 * Creates a labeled section wrapper
 */
const Section = (title: string, subtitle: string, content: any) =>
  Column({
    gap: 12,
    padding: 24,
    backgroundColor: 'white',
    borderRadius: 12,
    boxShadow: { offsetY: 4, blur: 12, color: 'rgba(0,0,0,0.08)' },
    children: [
      Text(title, { fontSize: 22, fontWeight: 'bold', color: Theme.darkColor }),
      Text(subtitle, { fontSize: 14, color: tinycolor(Theme.darkColor).setAlpha(0.5).toString() }),
      content,
    ],
  })

/**
 * Creates a colored cell with label
 */
const Cell = (label: string, color: string, height: number = 60) =>
  Box({
    backgroundColor: color,
    height,
    borderRadius: 8,
    justifyContent: Style.Justify.Center,
    alignItems: Style.Align.Center,
    children: [Text(label, { color: 'white', fontSize: 13, fontWeight: 'bold', textAlign: 'center' })],
  })

/**
 * Creates a gradient cell
 */
const GradientCell = (label: string, colors: [string, string], height: number = 60) =>
  Box({
    gradient: { type: 'linear', colors, direction: 'to-right' },
    height,
    borderRadius: 8,
    justifyContent: Style.Justify.Center,
    alignItems: Style.Align.Center,
    children: [Text(label, { color: 'white', fontSize: 13, fontWeight: 'bold', textAlign: 'center' })],
  })

/**
 * Stat badge (small labeled value)
 */
const StatBadge = (label: string, value: string, color: string) =>
  Box({
    backgroundColor: color,
    borderRadius: 6,
    padding: { All: 8, Left: 12, Right: 12 },
    children: Column({
      alignItems: Style.Align.Center,
      gap: 2,
      children: [
        Text(value, { fontSize: 18, fontWeight: 'bold', color: 'white' }),
        Text(label, { fontSize: 10, color: tinycolor('white').setAlpha(0.7).toString() }),
      ],
    }),
  })

// ─── Sample 1: Dashboard Layout ────────────────────────────────────────────────
// Outer 2-column grid, each cell contains an inner grid of cards
const dashboardLayout = Section(
  '1. Dashboard Layout',
  'Outer 2-col grid with inner stat grids inside each panel',
  Grid({
    columns: 2,
    gap: 16,
    children: [
      // Left panel: stats grid
      Box({
        backgroundColor: tinycolor(Theme.primaryColor).setAlpha(0.08).toString(),
        borderRadius: 10,
        padding: 16,
        children: Column({
          gap: 12,
          children: [
            Text('User Analytics', { fontSize: 16, fontWeight: 'bold', color: Theme.darkColor }),
            Grid({
              columns: 2,
              gap: 10,
              children: [
                StatBadge('Active', '1,284', Theme.primaryColor),
                StatBadge('New', '342', Theme.secondaryColor),
                StatBadge('Bounce', '18%', Theme.accentColor),
                StatBadge('Avg Time', '4m32s', '#0984E3'),
              ],
            }),
          ],
        }),
      }),
      // Right panel: stats grid
      Box({
        backgroundColor: tinycolor(Theme.secondaryColor).setAlpha(0.08).toString(),
        borderRadius: 10,
        padding: 16,
        children: Column({
          gap: 12,
          children: [
            Text('Revenue Metrics', { fontSize: 16, fontWeight: 'bold', color: Theme.darkColor }),
            Grid({
              columns: 2,
              gap: 10,
              children: [
                StatBadge('Total', '$48.2K', Theme.secondaryColor),
                StatBadge('MRR', '$12.1K', '#0984E3'),
                StatBadge('Growth', '+24%', '#00B894'),
                StatBadge('Churn', '2.1%', '#D63031'),
              ],
            }),
          ],
        }),
      }),
    ],
  }),
)

// ─── Sample 2: Card Grid with Inner Details ─────────────────────────────────────
// 3-column outer grid, each card has an inner layout with stacked rows
const colors = ['#6C5CE7', '#00B894', '#FD79A8', '#0984E3', '#E17055', '#00CEC9']
const names = ['Alpha', 'Beta', 'Gamma', 'Delta', 'Epsilon', 'Zeta']

const cardGrid = Section(
  '2. Card Grid with Inner Details',
  'Outer 3-col grid, each card has its own inner vertical layout with nested rows',
  Grid({
    columns: 3,
    gap: 14,
    children: names.map((name, i) =>
      Box({
        backgroundColor: tinycolor(colors[i]).setAlpha(0.1).toString(),
        borderRadius: 10,
        padding: 14,
        children: Column({
          gap: 10,
          children: [
            // Header row
            Row({
              justifyContent: Style.Justify.SpaceBetween,
              alignItems: Style.Align.Center,
              children: [
                Text(name, { fontSize: 16, fontWeight: 'bold', color: colors[i] }),
                Box({
                  backgroundColor: colors[i],
                  borderRadius: 4,
                  padding: { All: 4, Left: 8, Right: 8 },
                  children: Text(`#${i + 1}`, { fontSize: 10, fontWeight: 'bold', color: 'white' }),
                }),
              ],
            }),
            // Inner 2-col stat row
            Grid({
              columns: 2,
              gap: 8,
              children: [
                GradientCell('Score: 92', [colors[i], tinycolor(colors[i]).darken(20).toString()], 40),
                GradientCell('Rank: A+', [tinycolor(colors[i]).darken(10).toString(), colors[i]], 40),
              ],
            }),
            // Description
            Text('Performance metrics for this module show steady growth with minimal variance.', {
              fontSize: 11,
              color: tinycolor(Theme.darkColor).setAlpha(0.6).toString(),
              maxLines: 2,
              ellipsis: true,
            }),
          ],
        }),
      }),
    ),
  }),
)

// ─── Sample 3: Holy Grail Layout (Spanning + Nested) ────────────────────────────
// Outer 4-col grid with spanning, sidebar contains its own vertical grid
const holyGrail = Section(
  '3. Holy Grail Layout with Nested Sidebar',
  'Outer 4-col grid with spanning items; the sidebar contains its own inner grid',
  Grid({
    templateColumns: ['1fr', '1fr', '1fr', '1fr'],
    gap: 12,
    children: [
      // Header spanning all 4 columns
      GridItem({
        gridColumn: 'span 4',
        children: [GradientCell('Header (Span 4)', [Theme.primaryColor, Theme.accentColor], 50)],
      }),
      // Main content spanning 3 columns
      GridItem({
        gridColumn: 'span 3',
        children: [
          Box({
            backgroundColor: tinycolor(Theme.primaryColor).setAlpha(0.06).toString(),
            borderRadius: 8,
            padding: 16,
            height: 200,
            children: Column({
              gap: 10,
              children: [
                Text('Main Content Area', { fontSize: 18, fontWeight: 'bold', color: Theme.darkColor }),
                Text('This area spans 3 columns and contains the primary page content.', {
                  fontSize: 13,
                  color: tinycolor(Theme.darkColor).setAlpha(0.6).toString(),
                }),
                Grid({
                  columns: 3,
                  gap: 8,
                  children: [Cell('Article 1', '#636E72', 50), Cell('Article 2', '#636E72', 50), Cell('Article 3', '#636E72', 50)],
                }),
              ],
            }),
          }),
        ],
      }),
      // Sidebar spanning 1 column with inner grid
      GridItem({
        gridColumn: 'span 1',
        children: [
          Box({
            backgroundColor: tinycolor(Theme.secondaryColor).setAlpha(0.08).toString(),
            borderRadius: 8,
            padding: 12,
            height: 200,
            children: Column({
              gap: 8,
              children: [
                Text('Sidebar', { fontSize: 14, fontWeight: 'bold', color: Theme.darkColor }),
                Grid({
                  columns: 1,
                  gap: 6,
                  children: [
                    Cell('Nav Item 1', Theme.secondaryColor, 36),
                    Cell('Nav Item 2', tinycolor(Theme.secondaryColor).darken(10).toString(), 36),
                    Cell('Nav Item 3', tinycolor(Theme.secondaryColor).darken(20).toString(), 36),
                  ],
                }),
              ],
            }),
          }),
        ],
      }),
      // Footer spanning all 4 columns
      GridItem({
        gridColumn: 'span 4',
        children: [GradientCell('Footer (Span 4)', [Theme.accentColor, Theme.primaryColor], 50)],
      }),
    ],
  }),
)

// ─── Sample 4: Deeply Nested Grid (3 Levels) ────────────────────────────────────
// Level 1: 2-col → Level 2: 2-col → Level 3: 3-col cells
const deeplyNested = Section(
  '4. Deeply Nested Grid (3 Levels)',
  'Level 1 (2-col) → Level 2 (2-col) → Level 3 (3-col) demonstrating deep nesting',
  Grid({
    columns: 2,
    gap: 14,
    children: ['Panel A', 'Panel B'].map((panelName, pi) =>
      Box({
        backgroundColor: tinycolor(pi === 0 ? Theme.primaryColor : Theme.accentColor)
          .setAlpha(0.08)
          .toString(),
        borderRadius: 10,
        padding: 14,
        children: Column({
          gap: 10,
          children: [
            Text(panelName, { fontSize: 16, fontWeight: 'bold', color: pi === 0 ? Theme.primaryColor : Theme.accentColor }),
            // Level 2 grid inside each panel
            Grid({
              columns: 2,
              gap: 8,
              children: ['Sub 1', 'Sub 2'].map((subName, si) =>
                Box({
                  backgroundColor: tinycolor(pi === 0 ? Theme.primaryColor : Theme.accentColor)
                    .setAlpha(0.12)
                    .toString(),
                  borderRadius: 8,
                  padding: 10,
                  children: Column({
                    gap: 6,
                    children: [
                      Text(`${panelName} · ${subName}`, {
                        fontSize: 12,
                        fontWeight: 'bold',
                        color: Theme.darkColor,
                      }),
                      // Level 3 grid
                      Grid({
                        columns: 3,
                        gap: 4,
                        children: Array.from({ length: 3 }).map((_, ci) =>
                          Cell(
                            `${pi + 1}.${si + 1}.${ci + 1}`,
                            tinycolor(pi === 0 ? Theme.primaryColor : Theme.accentColor)
                              .darken(ci * 8)
                              .toString(),
                            32,
                          ),
                        ),
                      }),
                    ],
                  }),
                }),
              ),
            }),
          ],
        }),
      }),
    ),
  }),
)

// ─── Sample 5: Unstable / Asymmetric Content Size (3 Cells) ──────────────────────
// Mimics Genshin wish history card style: outer grid with 3 banner panels,
// each having different item counts for both 5★ and 4★

// Rarity color map (similar to RarityColorMapper in the example)
const RarityColor: Record<number, string> = {
  5: '#C99024',
  4: '#7B68C1',
}

// Mock banner data with asymmetric item counts
const banners = [
  {
    type: 'Character Event',
    total: 487,
    pity5: 32,
    pity4: 7,
    fiveStars: Array.from({ length: 8 }).map((_, i) => ({
      name: ['Raiden', 'Hutao', 'Zhongli', 'Nahida', 'Furina', 'Neuvillette', 'Kazuha', 'Yelan'][i],
      item_type: 'Character',
      pity: [76, 45, 82, 61, 55, 78, 34, 90][i],
      time: `2025-${String((i % 12) + 1).padStart(2, '0')}-${String((i % 28) + 1).padStart(2, '0')}`,
    })),
    fourStars: Array.from({ length: 14 }).map((_, i) => ({
      name: [
        'Xiangling',
        'Bennett',
        'Xingqiu',
        'Fischl',
        'Sucrose',
        'Beidou',
        'Noelle',
        'Barbara',
        'Razor',
        'Chongyun',
        'Ningguang',
        'Diona',
        'Rosaria',
        'Yanfei',
      ][i],
      item_type: i % 3 === 0 ? 'Weapon' : 'Character',
      pity: [3, 8, 2, 5, 10, 1, 7, 4, 9, 6, 3, 8, 2, 5][i],
      time: `2025-${String((i % 12) + 1).padStart(2, '0')}-${String((i % 28) + 1).padStart(2, '0')}`,
    })),
    color: Theme.primaryColor,
  },
  {
    type: 'Weapon Event',
    total: 214,
    pity5: 58,
    pity4: 3,
    fiveStars: Array.from({ length: 5 }).map((_, i) => ({
      name: ['Engulfing', 'Homa', 'Jade Spear', 'Mistsplitter', 'Aqua Sim.'][i],
      item_type: 'Weapon',
      pity: [63, 71, 44, 80, 55][i],
      time: `2025-${String((i % 12) + 1).padStart(2, '0')}-${String(((i * 3) % 28) + 1).padStart(2, '0')}`,
    })),
    fourStars: Array.from({ length: 9 }).map((_, i) => ({
      name: ['Fav. Lance', 'Sacrificial', "Dragon's B.", 'Rainslasher', 'Rust', 'The Bell', 'Eye of Per.', 'Widsith', 'Stringless'][i],
      item_type: 'Weapon',
      pity: [5, 2, 8, 3, 10, 1, 6, 4, 7][i],
      time: `2025-${String((i % 12) + 1).padStart(2, '0')}-${String(((i * 2) % 28) + 1).padStart(2, '0')}`,
    })),
    color: Theme.accentColor,
  },
  {
    type: 'Standard',
    total: 326,
    pity5: 41,
    pity4: 9,
    fiveStars: Array.from({ length: 7 }).map((_, i) => ({
      name: ['Diluc', 'Jean', 'Mona', 'Keqing', 'Qiqi', 'Tighnari', 'Dehya'][i],
      item_type: 'Character',
      pity: [78, 52, 85, 63, 90, 47, 71][i],
      time: `2024-${String((i % 12) + 1).padStart(2, '0')}-${String(((i * 2) % 28) + 1).padStart(2, '0')}`,
    })),
    fourStars: Array.from({ length: 11 }).map((_, i) => ({
      name: ['Amber', 'Kaeya', 'Lisa', 'Barbara', 'Xiangling', 'Noelle', 'Bennett', 'Fischl', 'Sucrose', 'Chongyun', 'Razor'][i],
      item_type: i % 4 === 0 ? 'Weapon' : 'Character',
      pity: [4, 7, 2, 9, 5, 1, 8, 3, 10, 6, 4][i],
      time: `2024-${String((i % 12) + 1).padStart(2, '0')}-${String(((i * 3) % 28) + 1).padStart(2, '0')}`,
    })),
    color: Theme.secondaryColor,
  },
]

/**
 * Renders star history sections (5★ and/or 4★) — mimics _renderStarHistory from the example
 */
const renderStarHistory = (banner: (typeof banners)[number]) => {
  const sections: any[] = []

  const renderSection = (label: string, items: { name: string; item_type: string; pity: number; time: string }[], starRarity: number) => {
    if (items.length === 0) return
    const reversed = [...items].reverse()
    const bgColor = RarityColor[starRarity]

    sections.push(
      Text(label, {
        flexShrink: 0,
        fontSize: 16,
        fontWeight: 'bold',
        margin: { Top: 4 },
      }),
      Grid({
        flexShrink: 0,
        columns: 6,
        gap: 10,
        children: reversed.map(star =>
          Column({
            positionType: Style.PositionType.Relative,
            children: [
              Column({
                gradient: {
                  type: 'linear',
                  colors: [tinycolor(bgColor).darken(15).toString(), bgColor],
                  direction: 'to-bottom',
                },
                borderRadius: 10,
                overflow: Style.Overflow.Hidden,
                boxShadow: {
                  offsetY: 4,
                  color: tinycolor('black').setAlpha(0.3).toString(),
                },
                children: [
                  // Icon area
                  Column({
                    alignItems: Style.Align.Center,
                    justifyContent: Style.Justify.Center,
                    padding: 8,
                    height: 60,
                    children: Text(star.item_type === 'Character' ? '👤' : '⚔️', { fontSize: 32 }),
                  }),
                  // Label area
                  Column({
                    alignItems: Style.Align.Center,
                    backgroundColor: 'rgba(0, 0, 0, 0.5)',
                    padding: { All: 6, Left: 4, Right: 4 },
                    gap: 2,
                    children: [
                      Text(star.name, {
                        maxLines: 1,
                        ellipsis: true,
                        textAlign: 'center',
                        fontSize: 11,
                        fontWeight: '600',
                        color: 'white',
                      }),
                      Text(`${star.pity} pulls · ${star.time.slice(2)}`, {
                        fontSize: 9,
                        textAlign: 'center',
                        color: tinycolor('white').setAlpha(0.7).toString(),
                      }),
                    ],
                  }),
                ],
              }),
            ],
          }),
        ),
      }),
    )
  }

  renderSection('5★ History', banner.fiveStars, 5)
  renderSection('4★ History', banner.fourStars, 4)

  return sections
}

const unstableGrid = Section(
  '5. Unstable Content Size (3 Cells)',
  'Outer 2-col grid — 3 banner panels with varying 5★ and 4★ item counts (mimics wish history cards)',
  Grid({
    columns: 2,
    gap: 20,
    children: banners.map(banner =>
      Box({
        flexShrink: 0,
        backgroundColor: tinycolor(banner.color).setAlpha(0.15).toString(),
        borderRadius: 8,
        padding: 16,
        children: Column({
          flexShrink: 0,
          gap: 12,
          children: [
            // Header row: banner name + total wishes
            Row({
              justifyContent: Style.Justify.SpaceBetween,
              alignItems: Style.Align.Center,
              children: [
                Text(banner.type, {
                  fontSize: 20,
                  fontWeight: 'bold',
                }),
                Text(`${banner.total} wishes`, {
                  fontSize: 16,
                  fontWeight: '600',
                  color: tinycolor('black').setAlpha(0.3).toString(),
                }),
              ],
            }),
            // Pity badges row
            Row({
              gap: 20,
              children: [
                Box({
                  backgroundColor: Theme.secondaryColor,
                  borderRadius: 6,
                  padding: { All: 8, Left: 12, Right: 12 },
                  children: Text(`5★ Pity: ${banner.pity5}`, {
                    fontSize: 16,
                    fontWeight: 'bold',
                    color: Theme.paperColor,
                  }),
                }),
                Box({
                  backgroundColor: Theme.secondaryColor,
                  borderRadius: 6,
                  padding: { All: 8, Left: 12, Right: 12 },
                  children: Text(`4★ Pity: ${banner.pity4}`, {
                    fontSize: 16,
                    fontWeight: 'bold',
                    color: Theme.paperColor,
                  }),
                }),
              ],
            }),
            // 5★ and 4★ History sections
            ...renderStarHistory(banner),
          ],
        }),
      }),
    ),
  }),
)

// ─── Main Execution ─────────────────────────────────────────────────────────────
;(async () => {
  try {
    const canvas = await Root({
      width: 1200,
      backgroundColor: '#F0F0F5',
      fontFamily: 'Inter',
      padding: 40,
      children: [
        Column({
          gap: 30,
          children: [
            Text('Nested Grid Samples', {
              fontSize: 32,
              fontWeight: 'bold',
              color: Theme.darkColor,
              margin: { Bottom: 10 },
            }),
            Text('Demonstrating Grid nesting patterns: grids inside grids with spanning and deep hierarchies', {
              fontSize: 14,
              color: tinycolor(Theme.darkColor).setAlpha(0.5).toString(),
              margin: { Bottom: 10 },
            }),
            dashboardLayout,
            cardGrid,
            holyGrail,
            deeplyNested,
            unstableGrid,
          ],
        }),
      ],
    })

    const outDir = path.join(process.cwd(), 'samples')
    if (!fs.existsSync(outDir)) {
      fs.mkdirSync(outDir)
    }
    const outFile = path.join(outDir, 'sample_nested_grids.png')
    await canvas.toFile(outFile)
    console.log(`Nested grid samples generated at: ${outFile}`)
  } catch (e) {
    console.error(e)
  }
})()
