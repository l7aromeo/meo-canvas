# @meonode/canvas

A declarative, component-based library for server-side canvas image generation. Write complex visuals with simple
functions, similar to the composition style of @meonode/ui.
It uses `skia-canvas` for drawing and `yoga-layout` for flexbox-based layouts.

This library allows you to build complex image layouts using a familiar component-based approach. You can define your
image structure with components like `Box`, `Text`, `Image`, and `Grid`, and the library will handle the layout and
rendering to a canvas.

## Key Features

- **Declarative API:** Build images using a component tree, just like in React.
- **Flexbox Layout:** Powered by `yoga-layout`, it supports flexbox for powerful and flexible layouts.
- **Rich Text:** Render text with custom fonts and inline styling using simple HTML-like tags. Supported tags include
  `<color="value">`, `<weight="value">`, `<size="value">`, `<b>`, and `<i>`.
- **Image Support:** Render images from URLs, file paths, or buffers, with `object-fit` and `object-position` support.
- **Chart Support:** Render bar, line, pie, and doughnut charts with customizable data and options.
- **Styling:** Style your components with properties that mimic CSS, including borders, padding, margins, and more.
- **Grid Layout:** A `Grid` component is provided for easy grid-based layouts.
- **TypeScript Support:** Fully typed for a better development experience.
- **[Architecture →](./ARCHITECTURE.md)**

## Showcase

<table>
  <tr>
    <td width="50%"><img src="https://i.ibb.co/VpPZybzF/profile-card.webp" alt="Image 1"></td>
    <td width="50%"><img src="https://i.ibb.co/zTgrBWpT/profile-card-1.webp" alt="Image 2"></td>
  </tr>
  <tr>
    <td width="50%"><img src="https://i.ibb.co/F4xfHdBp/daily-notes.webp" alt="Image 3"></td>
    <td width="50%"><img src="https://i.ibb.co/Jj0x6khB/character-archive-base.webp" alt="Image 4"></td>
  </tr>
  <tr>
    <td width="50%"><img src="https://i.ibb.co.com/B24SZ0C9/charts.webp" alt="Image 5"/></td>
    <td width="50%"><img src="https://i.ibb.co.com/jkBJMRgT/sample-nested-grids.png" alt="Image 6"/></td>
  </tr>
</table>

## Installation

```bash
bun add @meonode/canvas
```

## Usage

### Simple Example

A minimal example that renders a title and description to a PNG file:

```typescript
import {Root, Box, Text} from '@meonode/canvas';

async function generateImage() {
  const canvas = await Root({
    width: 500,
    height: 300,
    scale: 1,          // increase to 2 for 2× retina resolution
    workerMode: true,  // renders in a worker thread — keeps the main thread free (default)
    fonts: [
      {
        family: 'Roboto',
        paths: ['./fonts/Roboto-Regular.ttf', './fonts/Roboto-Bold.ttf'],
      },
    ],
    children: [
      Box({
        width: '100%',
        height: '100%',
        backgroundColor: '#f0f0f0',
        padding: 20,
        children: [
          Text('Hello, World!', {
            fontSize: 32,
            fontWeight: 'bold',
            fontFamily: 'Roboto',
            color: '#333',
          }),
          Text('This is a basic example of using @meonode/canvas.', {
            fontSize: 18,
            fontFamily: 'Roboto',
            color: '#666',
            margin: {Top: 10},
          }),
        ],
      }),
    ],
  });

  await canvas.toFile('output.png');  // saves directly to disk
  canvas.release();                   // free worker memory after use
}

generateImage().catch(console.error);
```

### Complex Layout

A more complete example using `Column`, `Row`, `Image`, and advanced flexbox properties to build a structured page
layout with a header, content area, and footer:

```typescript
import {Root, Column, Row, Text, Image, Style} from '@meonode/canvas';

async function generateComplexImage() {
  const canvas = await Root({
    width: 800,
    height: 600,
    scale: 2,              // 2× resolution — canvas output will be 1600×1200px
    workerMode: true,      // renders off the main thread (default)
    useDiskCache: true,    // caches fetched remote images to disk for faster re-decode
    fonts: [
      {
        family: 'Roboto',
        paths: ['./fonts/Roboto-Regular.ttf', './fonts/Roboto-Bold.ttf'],
      },
      {
        family: 'Open Sans',
        paths: ['./fonts/OpenSans-Regular.ttf'],
      },
    ],
    children: [
      Column({
        width: '100%',
        height: '100%',
        backgroundColor: '#f0f0f0',
        padding: 20,
        justifyContent: Style.Justify.SpaceBetween, // evenly space header, body, footer
        children: [
          // Header: avatar + title side by side
          Row({
            width: '100%',
            alignItems: Style.Align.Center,
            marginBottom: 20,
            children: [
              Image({
                src: 'https://via.placeholder.com/80x80/FF0000/FFFFFF?text=Logo',
                width: 80,
                height: 80,
                borderRadius: 40, // circle crop
                marginRight: 20,
                objectFit: 'cover',
              }),
              Text('Welcome to MeoNode Canvas!', {
                fontSize: 40,
                fontWeight: 'bold',
                fontFamily: 'Roboto',
                color: '#333',
              }),
            ],
          }),

          // Body: grows to fill remaining vertical space
          Column({
            flexGrow: 1,
            width: '100%',
            backgroundColor: '#ffffff',
            borderRadius: 10,
            padding: 30,
            boxShadow: {blur: 10, color: 'rgba(0,0,0,0.1)'},
            children: [
              Text('A New Way to Render Graphics', {
                fontSize: 28,
                fontWeight: 'bold',
                fontFamily: 'Open Sans',
                color: '#555',
                marginBottom: 15,
              }),
              Text(
                `This example demonstrates a more complex layout using various components.
        We have a header with a logo and title, a main content area with text,
        and a footer. Notice how flexbox properties are used to arrange elements.`,
                {
                  fontSize: 18,
                  fontFamily: 'Open Sans',
                  color: '#777',
                  lineHeight: 24,
                },
              ),
              Image({
                src: 'https://via.placeholder.com/600x200/007bff/ffffff?text=Feature+Image',
                width: '100%',
                height: 200,
                marginTop: 20,
                borderRadius: 8,
                objectFit: 'contain',
                objectPosition: {Top: '50%', Left: '50%'}, // center within box
              }),
            ],
          }),

          // Footer: centered copyright line
          Row({
            width: '100%',
            marginTop: 20,
            justifyContent: Style.Justify.Center,
            children: [
              Text('© 2025 MeoNode Canvas. All rights reserved.', {
                fontSize: 14,
                fontFamily: 'Open Sans',
                color: '#999',
              }),
            ],
          }),
        ],
      }),
    ],
  });

  await canvas.toFile('complex_output.png');  // saves directly to disk
  canvas.release();                           // free worker memory after use
}

generateComplexImage().catch(console.error);
```

## Examples

### Charts

The `Chart` component supports `bar`, `line`, `pie`, and `doughnut` chart types.

#### Bar Chart

```typescript
import {Root, Chart} from '@meonode/canvas';

async function generateBarChart() {
  const canvas = await Root({
    width: 600,
    height: 400,
    workerMode: true, // default — render in a worker thread
    children: [
      Chart({
        type: 'bar',
        width: '100%',
        height: '100%',
        data: {
          labels: ['Jan', 'Feb', 'Mar', 'Apr', 'May'],
          datasets: [
            {
              label: 'Sales',
              data: [120, 150, 180, 90, 200],
              color: '#36A2EB',
            },
          ],
        },
        options: {
          grid: {show: true, style: 'dashed'},
          axisColor: '#333',
          labelColor: '#333',
          showValues: true,   // display value labels above each bar
          valueFontSize: 12,
          showYAxis: true,    // show Y-axis tick labels on the left
          yAxisColor: '#666',
        },
      }),
    ],
  });

  await canvas.toFile('bar_chart.png');
  canvas.release();
}

generateBarChart().catch(console.error);
```

#### Doughnut Chart with Custom Legend

```typescript
import {Root, Chart, Row, Box, Text} from '@meonode/canvas';

async function generateDoughnutChart() {
  const canvas = await Root({
    width: 600,
    height: 400,
    workerMode: true, // default — render in a worker thread
    children: [
      Chart({
        type: 'doughnut',
        width: '100%',
        height: '100%',
        data: [
          {label: 'Red', value: 300, color: '#FF6384'},
          {label: 'Blue', value: 50, color: '#36A2EB'},
          {label: 'Yellow', value: 100, color: '#FFCE56'},
        ],
        options: {
          innerRadius: 0.7,      // 0 = full pie, 1 = empty ring; 0.7 gives a thick doughnut
          sliceBorderRadius: 5,  // rounded corners on each slice
          // custom legend item: colored dot + "Label: value" text
          renderLegendItem: ({item, color}) =>
            Row({
              alignItems: 'center',
              children: [
                Box({width: 12, height: 12, backgroundColor: color, borderRadius: 6}),
                Text(`${item.label}: ${item.value}`, {fontSize: 16, marginLeft: 8}),
              ],
            }),
        },
      }),
    ],
  });

  await canvas.toFile('doughnut_chart.png');
  canvas.release();
}

generateDoughnutChart().catch(console.error);
```

### Grid

The `Grid` component simplifies creating complex layouts. It mimics CSS Grid Layout.

#### Basic Grid

A simple grid with 3 columns, each 100 pixels wide.

```typescript
import {Root, Grid, Box, Text} from '@meonode/canvas';

async function generateBasicGrid() {
  const canvas = await Root({
    width: 400,
    height: 300,
    workerMode: true, // default — render in a worker thread
    children: [
      Grid({
        columns: 3,
        templateColumns: [100, 100, 100], // fixed widths; also accepts ['100px', '100px', '100px']
        gap: 10,
        children: [
          Box({backgroundColor: 'red', height: 50, children: [Text('1')]}),
          Box({backgroundColor: 'blue', height: 50, children: [Text('2')]}),
          Box({backgroundColor: 'green', height: 50, children: [Text('3')]}),
          Box({backgroundColor: 'yellow', height: 50, children: [Text('4')]}), // wraps to row 2
        ],
      }),
    ],
  });

  await canvas.toFile('grid_basic.png');
  canvas.release();
}

generateBasicGrid().catch(console.error);
```

#### Responsive Grid (Fractional Units)

Using fractional units (`fr`) allows columns to take up proportional space.

```typescript
Grid({
  // First column takes 1 part, second takes 2 parts, third takes 1 part
  templateColumns: ['1fr', '2fr', '1fr'],
  gap: 10,
  children: [
    Box({backgroundColor: 'red', height: 50, children: [Text('1fr')]}),
    Box({backgroundColor: 'blue', height: 50, children: [Text('2fr')]}),
    Box({backgroundColor: 'green', height: 50, children: [Text('1fr')]}),
  ],
});
```

#### Spanning Items

Use `GridItem` (or pass `gridColumn`/`gridRow` props directly to any child) to span multiple columns or rows.

```typescript
import {Grid, GridItem, Box, Text} from '@meonode/canvas';

Grid({
  templateColumns: ['1fr', '1fr', '1fr'],
  gap: 10,
  children: [
    // Spans all 3 columns
    GridItem({
      gridColumn: 'span 3',
      height: 50,
      backgroundColor: '#333',
      children: [Text('Header', {color: 'white'})],
    }),
    // Standard items
    Box({backgroundColor: '#eee', height: 100, children: [Text('Content')]}),
    Box({backgroundColor: '#ccc', height: 100, children: [Text('Sidebar')]}),
    // Spans 2 columns
    GridItem({
      gridColumn: 'span 2',
      height: 50,
      backgroundColor: '#555',
      children: [Text('Footer', {color: 'white'})],
    }),
  ],
});
```

## Yoga Layout

This library leverages `yoga-layout` for its powerful flexbox engine. Many layout properties directly map to Yoga's
concepts. You can access Yoga-specific constants through the `Style` export from `@meonode/canvas`.

```typescript
import {Box, Style} from '@meonode/canvas';

Box({
  flexDirection: Style.FlexDirection.Row,
  justifyContent: Style.Justify.Center,
  alignItems: Style.Align.Center,
  children: [
    Box({
      width: 100,
      height: 100,
      backgroundColor: 'red',
      positionType: Style.PositionType.Absolute,
      position: {Top: 10, Left: 10},
    }),
    // ... other children
  ],
});
```

Refer to the [Yoga Layout documentation](https://yogalayout.dev/docs/) for a comprehensive understanding of these
properties.

## API Reference

### Root

The `Root` function is the entry point for rendering. It returns a `Canvas` object. It is a specialized `ColumnNode`
that inherits all `BoxProps`.

#### Root Props


| Prop           | Type                              | Default             | Description                                                                                           |
| -------------- | --------------------------------- | ------------------- | ----------------------------------------------------------------------------------------------------- |
| `width`        | `number`                          | -                   | **Required.** Width of the canvas in pixels.                                                          |
| `height`       | `number`                          | -                   | Optional height of the canvas. If not set, it's calculated from content.                              |
| `children`     | `CanvasElement | CanvasElement[]` | -                   | **Required.** The component tree to render.                                                           |
| `scale`        | `number`                          | `1`                 | Scale factor for rendering (e.g., 2 for 2x resolution).                                               |
| `fonts`        | `FontRegistrationInfo[]`          | -                   | An array of font files to register for use in the canvas.                                             |
| `useDiskCache` | `boolean`                         | `false`             | Write fetched images to disk during render for faster re-decode. Entries are cleaned up after render. |
| `imageConcurrency` | `number`                     | `5`                 | Maximum number of images to fetch concurrently during render.                                         |
| `workerMode`   | `boolean`                         | `true`              | Enable worker thread rendering for non-blocking operation.                                            |
| `workers`      | `number`                          | `cpus().length - 1` | Number of worker threads to use (only applies on first render with`workerMode: true`).                |

Since `Root` extends `BoxProps`, it also accepts `backgroundColor`, `padding`, `gradient`, `boxShadow`, and all other
layout props. See [Box, Row, and Column](#box-row-and-column) for the full list.

#### Canvas Methods

The `Root()` function returns a Canvas object with the following methods and properties.

##### Export Methods


| Method         | Signature                                                            | Description                                                                  |
| -------------- | -------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| `toBufferSync` | `(format?: ExportFormat, options?: ExportOptions) => Buffer`         | Synchronously returns a buffer. In worker mode, returns the pre-encoded PNG. |
| `toBuffer`     | `(format: ExportFormat, options?: ExportOptions) => Promise<Buffer>` | Asynchronously encodes to the specified format.                              |
| `toURL`        | `(format: ExportFormat, options?: ExportOptions) => Promise<string>` | Returns a data URL.                                                          |
| `toURLSync`    | `(format?: ExportFormat, options?: ExportOptions) => string`         | Synchronously returns a data URL (from pre-encoded PNG in worker mode).      |
| `toFile`       | `(filename: string, options?: SaveOptions) => Promise<void>`         | Saves the canvas to a file.                                                  |
| `toSharp`      | `(options?: RenderOptions) => Promise<Buffer>`                       | Returns a Sharp instance buffer for further processing.                      |
| `toSharpSync`  | `(options?: RenderOptions) => never`                                 | **Throws error.** Use `toSharp()` instead in worker mode.                    |

**Supported Export Formats:** `'png'`, `'jpg'` (or `'jpeg'`), `'webp'`, `'pdf'`, `'svg'`, `'raw'`

##### Convenience Getters


| Getter  | Returns           | Description                                     |
| ------- | ----------------- | ----------------------------------------------- |
| `.png`  | `Promise<Buffer>` | Shortcut for`toBuffer('png')`                   |
| `.jpg`  | `Promise<Buffer>` | Shortcut for`toBuffer('jpg')`                   |
| `.webp` | `Promise<Buffer>` | Shortcut for`toBuffer('webp')`                  |
| `.svg`  | `Promise<Buffer>` | Shortcut for`toBuffer('svg')`                   |
| `.pdf`  | `Promise<Buffer>` | Shortcut for`toBuffer('pdf')`                   |
| `.raw`  | `Promise<Buffer>` | Shortcut for`toBuffer('raw')` — raw pixel data |

##### Canvas Properties


| Property  | Type     | Description                            |
| --------- | -------- | -------------------------------------- |
| `.width`  | `number` | Canvas width in pixels (after scale).  |
| `.height` | `number` | Canvas height in pixels (after scale). |

##### Memory Management (Worker Mode)


| Method       | Description                                                                                                  |
| ------------ | ------------------------------------------------------------------------------------------------------------ |
| `.release()` | **Required in worker mode.** Releases the Canvas from worker memory. Call when done to prevent memory leaks. |

```typescript
import {Root} from '@meonode/canvas'

// Render with default worker mode (enabled)
const canvas = await Root({width: 400, height: 400, children: [...]})

// Or explicitly disable worker mode
const canvas = await Root({width: 400, height: 400, children: [...], workerMode: false})

// Use the canvas
const png = await canvas.png
const jpg = await canvas.jpg
await canvas.toFile('output.png')

// Release memory (worker mode only)
canvas.release()
```

> **Note:** A FinalizationRegistry provides automatic cleanup for forgotten `.release()` calls, but explicit cleanup is
> recommended for deterministic memory management in production.

---

### Box, Row, and Column

These are the fundamental layout components. `Row` and `Column` are wrappers around `Box` with a pre-set
`flexDirection`. They all share the same props.

#### Layout Props


| Prop                    | Type                       | Description                                                                   |
| ----------------------- | -------------------------- | ----------------------------------------------------------------------------- |
| `width`, `height`       | `number | string`          | Sets the size of the node in pixels or percentage.                            |
| `minWidth`, `minHeight` | `number | string`          | Sets the minimum size of the node.                                            |
| `maxWidth`, `maxHeight` | `number | string`          | Sets the maximum size of the node.                                            |
| `flexDirection`         | `Style.FlexDirection`      | Defines the direction of the main axis (`Row`, `Column`, etc.).               |
| `justifyContent`        | `Style.Justify`            | Defines how items are distributed along the main axis.                        |
| `alignItems`            | `Style.Align`              | Defines how items are aligned along the cross axis.                           |
| `alignSelf`             | `Style.Align`              | Overrides the parent's`alignItems` for a specific item.                       |
| `alignContent`          | `Style.Align`              | Defines how lines are distributed when content wraps.                         |
| `flexGrow`              | `number`                   | Defines the ability of an item to grow.                                       |
| `flexShrink`            | `number`                   | Defines the ability of an item to shrink.                                     |
| `flexBasis`             | `number | 'auto' | string` | Defines the default size of an item along the main axis.                      |
| `flexWrap`              | `Style.Wrap`               | Controls whether flex items wrap to multiple lines.                           |
| `positionType`          | `Style.PositionType`       | Sets the positioning method (`Relative` or `Absolute`).                       |
| `position`              | `object | number | string` | Sets the offset for a positioned element.                                     |
| `margin`                | `object | number | string` | Sets the margin space on the outside of the node.                             |
| `padding`               | `object | number | string` | Sets the padding space on the inside of the node.                             |
| `border`                | `object | number`          | Sets the width of the node's border.                                          |
| `aspectRatio`           | `number`                   | Locks the aspect ratio (width / height) of the node.                          |
| `overflow`              | `Style.Overflow`           | Defines how content that overflows is handled (`Visible`, `Hidden`).          |
| `display`               | `Style.Display`            | Controls if the node is included in layout (`Flex`, `None`).                  |
| `direction`             | `Style.Direction`          | Sets the primary layout direction (`LTR`, `RTL`).                             |
| `gap`                   | `object | number | string` | Defines the space between flex items.                                         |
| `boxSizing`             | `Style.BoxSizing`          | Defines how`width` and `height` are interpreted (`ContentBox`, `BorderBox`).  |
| `zIndex`                | `number`                   | Specifies the stack order of an element (only for`positionType: 'absolute'`). |
| `children`              | `CanvasElement \| CanvasElement[]` | Child nodes to render inside this node.                               |

#### Styling Props


| Prop              | Type                                | Description                                            |
| ----------------- | ----------------------------------- | ------------------------------------------------------ |
| `backgroundColor` | `string`                            | Sets the background color of the node.                 |
| `borderColor`     | `string`                            | Sets the color of the node's border.                   |
| `borderStyle`     | `Style.Border`                      | Sets the style of the border (`Solid`, `Dashed`, `Dotted`). |
| `borderRadius`    | `object | number`                   | Sets the radius of the node's corners.                 |
| `opacity`         | `number`                            | Sets the opacity of the node and its children (0-1).   |
| `gradient`        | `object`                            | Sets a linear or radial gradient as the background.    |
| `boxShadow`       | `BoxShadowProps | BoxShadowProps[]` | Applies one or more box-shadow effects.                |
| `transform`       | `TransformProps`                    | Applies 2D transformations (translate, rotate, scale). |

#### Font & Text Props (Inheritable)

These props, when set on a `Box`, `Row`, or `Column`, are inherited by any descendant `Text` nodes.


| Prop            | Type                                                        | Description                                |
| --------------- | ----------------------------------------------------------- | ------------------------------------------ |
| `fontSize`      | `number`                                                    | Font size in pixels.                       |
| `fontFamily`    | `string`                                                    | Font family name.                          |
| `fontWeight`    | `string | number`                                           | Font weight ('normal', 'bold', 400, etc.). |
| `fontStyle`     | `'normal' | 'italic'`                                       | Font style.                                |
| `color`         | `string`                                                    | Text color.                                |
| `textAlign`     | `'start' | 'end' | 'left' | 'center' | 'right' | 'justify'` | Horizontal text alignment.                 |
| `verticalAlign` | `'top' | 'middle' | 'bottom'`                               | Vertical text alignment.                   |
| `lineHeight`    | `number`                                                    | Line height in pixels.                     |
| `lineGap`       | `number`                                                    | Additional vertical spacing between lines. |
| `letterSpacing` | `number | string`                                           | Spacing between letters.                   |
| `wordSpacing`   | `number | string`                                           | Spacing between words.                     |
| `fontVariant`   | `FontVariantSetting`                                        | Specifies font variation settings.         |

---

### Text

The `Text` component renders text content. It inherits all `BoxProps` except for `children`, `gap`, and flex container
properties.

#### Text-Specific Props


| Prop         | Type                                  | Description                                                               |
| ------------ | ------------------------------------- | ------------------------------------------------------------------------- |
| `maxLines`   | `number`                              | Maximum number of lines to display before truncating.                     |
| `ellipsis`   | `boolean | string`                    | If`true`, adds '...' when text is truncated. Can also be a custom string. |
| `textShadow` | `TextShadowProps | TextShadowProps[]` | Applies one or more shadow effects to the text itself.                    |

---

### Image

The `Image` component renders an image. It inherits all `BoxProps` except for `children`.

#### Image-Specific Props


| Prop             | Type                                                   | Description                                                           |
| ---------------- | ------------------------------------------------------ | --------------------------------------------------------------------- |
| `src`            | `string | Buffer`                                      | The source URL, file path, or buffer of the image.                    |
| `objectFit`      | `'fill' | 'contain' | 'cover' | 'none' | 'scale-down'` | Specifies how the image should be resized to fit its container.       |
| `objectPosition` | `object`                                               | Specifies the alignment of the image within its box.                  |
| `saturate`       | `number`                                               | Adjusts the image's saturation level (0 is grayscale, 1 is original). |
| `dropShadow`     | `DropShadowProps`                                      | Applies a drop-shadow effect based on the image's alpha channel.      |
| `alt`            | `string`                                               | Alternative text description (for accessibility).                     |
| `onLoad`         | `() => void`                                           | Callback function that executes when the image loads successfully.    |
| `onError`        | `(error: Error) => void`                               | Callback function that executes when the image fails to load.         |

---

### Grid

The `Grid` component arranges its children in a grid layout. It is a specialized `RowNode` and inherits most `BoxProps`.

#### Grid-Specific Props


| Prop              | Type                                                  | Description                                            |
| ----------------- | ----------------------------------------------------- | ------------------------------------------------------ |
| `columns`         | `number`                                              | The number of columns in the grid. Default is 1.       |
| `templateColumns` | `GridTrackSize[]`                                     | Defines the columns of the grid (e.g.,`[100, '1fr']`). |
| `templateRows`    | `GridTrackSize[]`                                     | Defines the rows of the grid.                          |
| `autoRows`        | `GridTrackSize`                                       | Specifies the size of implicitly created rows.         |
| `autoFlow`        | `'row' \| 'column' \| 'row-dense' \| 'column-dense'` | Controls how the auto-placement algorithm works. Default is `'row'`. |

> **Gap control:** `gap` is inherited from `BoxProps`. Pass a number for uniform spacing, or an object for per-axis control: `gap: { Row: 10, Column: 20 }`.

---

### GridItem

The `GridItem` component represents a child item within a `Grid`. It inherits all `BoxProps` and adds grid placement
properties.

#### GridItem-Specific Props


| Prop         | Type     | Description                                                 |
| ------------ | -------- | ----------------------------------------------------------- |
| `gridColumn` | `string` | Specifies the column span (e.g.,`'span 2'`, `'1 / 3'`).     |
| `gridRow`    | `string` | Specifies the row span (e.g.,`'span 2'`, `'1 / 3'`).        |
| `gridArea`   | `string` | Shorthand for`gridRow` and `gridColumn` (e.g., `'header'`). |

> **Note:** You can also use `gridColumn` and `gridRow` props directly on any child component (`Box`, `Text`, etc.)
> without wrapping in `GridItem`.

---

### Chart

The `Chart` component renders various types of charts. It inherits all `BoxProps`.

#### Chart-Specific Props


| Prop      | Type                                       | Description                                                                   |
| --------- | ------------------------------------------ | ----------------------------------------------------------------------------- |
| `type`    | `'bar' | 'line' | 'pie' | 'doughnut'`      | The type of chart to render.                                                  |
| `data`    | `CartesianChartData | PieChartDataPoint[]` | The data for the chart, which varies based on the`type`.                      |
| `options` | `ChartOptions<T>`                          | An object containing rendering and style options, specific to the chart type. |

#### ChartOptions

The `options` prop is a conditional type that changes based on the chart `type`.

##### Common Options (All Chart Types)


| Prop               | Type                                         | Description                                       |
| ------------------ | -------------------------------------------- | ------------------------------------------------- |
| `showLabels`       | `boolean`                                    | If`true`, displays labels on the chart.           |
| `showLegend`       | `boolean`                                    | If`true`, displays the chart legend.              |
| `labelFontSize`    | `number`                                     | Font size for labels and legend text.             |
| `labelColor`       | `string`                                     | Color for labels and legend text.                 |
| `legendPosition`   | `'top' | 'bottom' | 'left' | 'right'`        | The position of the legend relative to the chart. |
| `renderLabelItem`  | `(props: { item, index }) => BoxNode`        | A custom render function for chart labels.        |
| `renderLegendItem` | `(props: { item, index, color }) => BoxNode` | A custom render function for legend items.        |

##### Cartesian Chart Options (`bar`, `line`)


| Prop                  | Type                                                                               | Description                                                                                       |
| --------------------- | ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| `grid`                | `GridOptions`                                                                      | Configures grid lines (`show`, `color`, `style: 'solid' \| 'dashed' \| 'dotted'`).               |
| `axisColor`           | `string`                                                                           | The color of the chart axes.                                                                      |
| `showValues`          | `boolean`                                                                          | If `true`, displays values on top of bars or points.                                              |
| `valueColor`          | `string`                                                                           | Color of the value labels.                                                                        |
| `valueFontSize`       | `number`                                                                           | Font size of the value labels.                                                                    |
| `renderValueItem`     | `(props: { item: number; index: number; datasetIndex: number }) => BoxNode`        | Custom render function for each value label above a bar or point.                                 |
| `showYAxis`           | `boolean`                                                                          | If `true`, displays the Y-axis labels on the left.                                                |
| `yAxisColor`          | `string`                                                                           | Color of the Y-axis labels.                                                                       |
| `yAxisFontSize`       | `number`                                                                           | Font size of the Y-axis labels.                                                                   |
| `yAxisLabelFormatter` | `(value: number) => string`                                                        | Custom formatter for Y-axis labels. Smart defaults adjust decimal precision based on value range. |
| `xAxisLabelFormatter` | `(value: string, index: number) => string`                                         | Custom formatter for X-axis labels. Useful for truncating or transforming labels.                 |

###### Y-Axis Label Formatter

The `yAxisLabelFormatter` enables custom Y-axis label formatting. By default, it uses smart formatting:

- **Values < 1**: 4 decimals (e.g., `0.0025`)
- **Values 1-100**: 2 decimals (e.g., `25.43`)
- **Values 100-1000**: 1 decimal (e.g., `250.5`)
- **Values 1000-1M**: 0 decimals (e.g., `50000`)
- **Values ≥ 1M**: 1 decimal with "M" suffix (e.g., `1.5M`)

Custom example:

```typescript
options: {
  yAxisLabelFormatter: (value) => `$${value.toFixed(2)}`,
}
```

###### X-Axis Label Formatter

The `xAxisLabelFormatter` allows custom X-axis label transformations. It receives the label string and its index.

Custom example:

```typescript
options: {
  xAxisLabelFormatter: (label, index) =>
    label.length > 5 ? label.substring(0, 5) + '...' : label,
}
```

##### Pie & Doughnut Chart Options (`pie`, `doughnut`)


| Prop                | Type     | Description                                                                  |
| ------------------- | -------- | ---------------------------------------------------------------------------- |
| `innerRadius`       | `number` | The radius of the inner circle in a doughnut chart (0 to 1). Default is 0.6. |
| `sliceBorderRadius` | `number` | The border radius for the corners of each slice. Default is 0.               |

---

### Cleanup Functions

#### `terminate()`

Terminate all worker pools and free worker thread resources. Call this when shutting down a long-running server.

```typescript
import {terminate} from '@meonode/canvas'

// Call on server shutdown
process.on('SIGTERM', () => {
  terminate()
  process.exit(0)
})
```

> After calling `terminate()`, the worker pool will be lazily re-initialized on the next `Root()` call.

#### `clearDiskCache()`

Manually clear the entire disk cache directory. Useful for debugging or forced cleanup.

```typescript
import {clearDiskCache} from '@meonode/canvas'

await clearDiskCache()
```

> **Note:** Disk cache is automatically cleaned up after each render when `useDiskCache: true`, and on process exit.

#### `setDiskCacheDir(dir)`

Override the default disk cache directory. Must be called before any cache read/write operations.

```typescript
import {setDiskCacheDir} from '@meonode/canvas'

setDiskCacheDir('/tmp/my-custom-cache')
```

---
## Contributing

Contributions are welcome! Please see the [Contributing Guidelines](CONTRIBUTING.md) for more details on how to get
started.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
