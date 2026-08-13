import { Root, Column, Box, Row, Text, Style, Grid } from '../src/index.js'
import { GridItem } from '../src/canvas/grid.canvas.js'
import path from 'path'
import fs from 'fs'

function _darken(hex: string, amount: number): string {
  const r = Math.max(0, parseInt(hex.slice(1, 3), 16) - (amount / 100) * 255)
  const g = Math.max(0, parseInt(hex.slice(3, 5), 16) - (amount / 100) * 255)
  const b = Math.max(0, parseInt(hex.slice(5, 7), 16) - (amount / 100) * 255)
  return '#' + [r, g, b].map(v => Math.round(v).toString(16).padStart(2, '0')).join('')
}

const NAMED: Record<string, [number, number, number]> = {
  white: [255, 255, 255],
  black: [0, 0, 0],
}

/**
 * Applies an alpha to a colour, accepting `#rrggbb` or one of the names above.
 *
 * The name handling is not a convenience. This used to slice hex digits
 * unconditionally, so `_alpha('white', 0.7)` produced `rgba(NaN,NaN,NaN,0.7)`,
 * which is not a colour the renderer accepts — the text fell back to black and
 * sat there unreadable on a dark scrim. Silent, because an unparseable colour
 * still renders something. It throws now rather than returning a value that
 * looks like a colour and is not one.
 */
function _alpha(color: string, a: number): string {
  const [r, g, b] = NAMED[color] ?? [parseInt(color.slice(1, 3), 16), parseInt(color.slice(3, 5), 16), parseInt(color.slice(5, 7), 16)]
  if ([r, g, b].some(Number.isNaN)) throw new Error(`_alpha: cannot parse colour ${JSON.stringify(color)}`)
  return `rgba(${r},${g},${b},${a})`
}

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
      Text(subtitle, { fontSize: 14, color: _alpha(Theme.darkColor, 0.5) }),
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
      children: [Text(value, { fontSize: 18, fontWeight: 'bold', color: 'white' }), Text(label, { fontSize: 10, color: _alpha('white', 0.7) })],
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
        backgroundColor: _alpha(Theme.primaryColor, 0.08),
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
        backgroundColor: _alpha(Theme.secondaryColor, 0.08),
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
        backgroundColor: _alpha(colors[i], 0.1),
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
              children: [GradientCell('Score: 92', [colors[i], _darken(colors[i], 20)], 40), GradientCell('Rank: A+', [_darken(colors[i], 10), colors[i]], 40)],
            }),
            // Description
            Text('Performance metrics for this module show steady growth with minimal variance.', {
              fontSize: 11,
              color: _alpha(Theme.darkColor, 0.6),
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
            backgroundColor: _alpha(Theme.primaryColor, 0.06),
            borderRadius: 8,
            padding: 16,
            height: 200,
            children: Column({
              gap: 10,
              children: [
                Text('Main Content Area', { fontSize: 18, fontWeight: 'bold', color: Theme.darkColor }),
                Text('This area spans 3 columns and contains the primary page content.', {
                  fontSize: 13,
                  color: _alpha(Theme.darkColor, 0.6),
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
            backgroundColor: _alpha(Theme.secondaryColor, 0.08),
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
                    Cell('Nav Item 2', _darken(Theme.secondaryColor, 10), 36),
                    Cell('Nav Item 3', _darken(Theme.secondaryColor, 20), 36),
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
        backgroundColor: _alpha(pi === 0 ? Theme.primaryColor : Theme.accentColor, 0.08),
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
                  backgroundColor: _alpha(pi === 0 ? Theme.primaryColor : Theme.accentColor, 0.12),
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
                          Cell(`${pi + 1}.${si + 1}.${ci + 1}`, _darken(pi === 0 ? Theme.primaryColor : Theme.accentColor, ci * 8), 32),
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
// Three panels whose two sub-sections hold different numbers of entries, so no
// two cells in the outer grid resolve to the same height. That asymmetry is the
// whole point: the outer grid has to lay out rows where the tallest cell is not
// the one that starts the row, and where a panel's height is only known after
// its nested grids have wrapped.

// Two tiers, so the nested grids differ in colour as well as in length.
const TierColor: Record<string, string> = {
  release: '#C99024',
  patch: '#7B68C1',
}

interface Entry {
  name: string
  kind: 'package' | 'service'
  minutes: number
  date: string
}

const stamp = (year: number, i: number, step = 1) => `${year}-${String((i % 12) + 1).padStart(2, '0')}-${String(((i * step) % 28) + 1).padStart(2, '0')}`

// Deliberately uneven: 8/14, 5/9 and 7/11 entries. One name per project is long
// enough to ellipsize, which keeps `maxLines: 1` under test — but the rest fit,
// because a shared prefix like `patch/` truncates to nothing but the prefix and
// every card ends up reading the same.
const projects = [
  {
    name: 'Web Platform',
    builds: 487,
    passRate: 96,
    median: 7,
    releases: Array.from({ length: 8 }).map((_, i) => ({
      name: ['v4.2.0', 'v4.1.0', 'v4.0.0', 'v3.9.0', 'v3.8.0', 'v3.7.0', 'v3.6.0', 'v3.5.0'][i],
      kind: 'package' as const,
      minutes: [12, 9, 21, 14, 8, 11, 17, 6][i],
      date: stamp(2025, i),
    })),
    patches: Array.from({ length: 14 }).map((_, i) => ({
      name: [
        'login-fix',
        'cache-warm',
        'virtualised-table',
        'i18n',
        'icons',
        'forms',
        'sorting',
        'modal',
        'toast',
        'theme',
        'search',
        'upload',
        'avatar',
        'footer',
      ][i],
      kind: (i % 3 === 0 ? 'service' : 'package') as Entry['kind'],
      minutes: [3, 8, 2, 5, 10, 1, 7, 4, 9, 6, 3, 8, 2, 5][i],
      date: stamp(2025, i),
    })),
    color: Theme.primaryColor,
  },
  {
    name: 'Mobile Client',
    builds: 214,
    passRate: 89,
    median: 19,
    releases: Array.from({ length: 5 }).map((_, i) => ({
      name: ['v2.8.0', 'v2.7.0', 'v2.6.0', 'v2.5.0', 'v2.4.0'][i],
      kind: 'package' as const,
      minutes: [24, 31, 18, 27, 22][i],
      date: stamp(2025, i, 3),
    })),
    patches: Array.from({ length: 9 }).map((_, i) => ({
      name: ['push', 'deeplink', 'offline-sync-retry', 'camera', 'perms', 'onboarding', 'crash', 'locale', 'badge'][i],
      kind: 'package' as const,
      minutes: [5, 2, 8, 3, 10, 1, 6, 4, 7][i],
      date: stamp(2025, i, 2),
    })),
    color: Theme.accentColor,
  },
  {
    name: 'Design System',
    builds: 326,
    passRate: 99,
    median: 4,
    releases: Array.from({ length: 7 }).map((_, i) => ({
      name: ['v6.1.0', 'v6.0.0', 'v5.4.0', 'v5.3.0', 'v5.2.0', 'v5.1.0', 'v5.0.0'][i],
      kind: 'package' as const,
      minutes: [5, 13, 4, 9, 3, 7, 11][i],
      date: stamp(2024, i, 2),
    })),
    patches: Array.from({ length: 11 }).map((_, i) => ({
      name: ['tokens', 'spacing', 'button', 'select', 'focus-ring-audit', 'tabs', 'chip', 'grid', 'motion', 'a11y', 'docs'][i],
      kind: (i % 4 === 0 ? 'service' : 'package') as Entry['kind'],
      minutes: [4, 7, 2, 9, 5, 1, 8, 3, 10, 6, 4][i],
      date: stamp(2024, i, 3),
    })),
    color: Theme.secondaryColor,
  },
]

/**
 * Renders the two history sections of a panel, each its own nested grid.
 */
const renderHistory = (project: (typeof projects)[number]) => {
  const sections: any[] = []

  const renderSection = (label: string, entries: Entry[], tier: keyof typeof TierColor) => {
    if (entries.length === 0) return
    const reversed = [...entries].reverse()
    const bgColor = TierColor[tier]

    sections.push(
      Text(label, {
        flexShrink: 0,
        fontSize: 16,
        fontWeight: 'bold',
        color: Theme.darkColor,
        margin: { Top: 4 },
      }),
      Grid({
        flexShrink: 0,
        columns: 6,
        gap: 10,
        children: reversed.map(entry =>
          Column({
            positionType: Style.PositionType.Relative,
            children: [
              Column({
                gradient: {
                  type: 'linear',
                  colors: [_darken(bgColor, 15), bgColor],
                  direction: 'to-bottom',
                },
                borderRadius: 10,
                overflow: Style.Overflow.Hidden,
                boxShadow: {
                  offsetY: 4,
                  color: _alpha('black', 0.3),
                },
                children: [
                  // Icon area
                  Column({
                    alignItems: Style.Align.Center,
                    justifyContent: Style.Justify.Center,
                    padding: 8,
                    height: 60,
                    children: Text(entry.kind === 'package' ? '📦' : '⚙️', { fontSize: 32 }),
                  }),
                  // Label area. The scrim was 0.5, where the secondary line measured
                  // 5.10:1 over the lightest end of the gold gradient — passing, but
                  // thin for 9px type. At 0.66 it is 9.98:1.
                  Column({
                    alignItems: Style.Align.Center,
                    backgroundColor: 'rgba(0, 0, 0, 0.66)',
                    padding: { All: 6, Left: 4, Right: 4 },
                    gap: 2,
                    children: [
                      Text(entry.name, {
                        maxLines: 1,
                        ellipsis: true,
                        textAlign: 'center',
                        fontSize: 11,
                        fontWeight: '600',
                        color: 'white',
                      }),
                      Text(`${entry.minutes} min · ${entry.date.slice(2)}`, {
                        fontSize: 9,
                        textAlign: 'center',
                        color: _alpha('white', 0.88),
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

  renderSection('Releases', project.releases, 'release')
  renderSection('Patches', project.patches, 'patch')

  return sections
}

const unstableGrid = Section(
  '5. Unstable Content Size (3 Cells)',
  'Outer 2-col grid — 3 project panels whose nested release and patch grids hold different numbers of entries',
  Grid({
    columns: 2,
    gap: 20,
    children: projects.map(project =>
      Box({
        flexShrink: 0,
        backgroundColor: _alpha(project.color, 0.15),
        borderRadius: 8,
        padding: 16,
        children: Column({
          flexShrink: 0,
          gap: 12,
          children: [
            // Header row: project name + total builds
            Row({
              justifyContent: Style.Justify.SpaceBetween,
              alignItems: Style.Align.Center,
              children: [
                Text(project.name, {
                  fontSize: 20,
                  fontWeight: 'bold',
                  color: Theme.darkColor,
                }),
                // Was black at 0.3, which measured 2.07:1 on the tinted panel and
                // read as disabled rather than secondary. At 0.72 of the dark it
                // is 4.74:1, the first of these to clear WCAG AA's 4.5:1.
                Text(`${project.builds} builds`, {
                  fontSize: 16,
                  fontWeight: '600',
                  color: _alpha(Theme.darkColor, 0.72),
                }),
              ],
            }),
            // Stat badges row
            Row({
              gap: 20,
              children: [
                Box({
                  backgroundColor: Theme.secondaryColor,
                  borderRadius: 6,
                  padding: { All: 8, Left: 12, Right: 12 },
                  children: Text(`Pass rate: ${project.passRate}%`, {
                    fontSize: 16,
                    fontWeight: 'bold',
                    color: Theme.paperColor,
                  }),
                }),
                Box({
                  backgroundColor: Theme.secondaryColor,
                  borderRadius: 6,
                  padding: { All: 8, Left: 12, Right: 12 },
                  children: Text(`Median: ${project.median} min`, {
                    fontSize: 16,
                    fontWeight: 'bold',
                    color: Theme.paperColor,
                  }),
                }),
              ],
            }),
            // Release and patch history, each a nested grid
            ...renderHistory(project),
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
      workerMode: false,
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
              color: _alpha(Theme.darkColor, 0.5),
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
