# @meonode/canvas

A declarative, component-based library for generating images on a canvas, inspired by the MeoNode UI library for React.
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
- **Styling:** Style your components with properties that mimic CSS, including borders, padding, margins, and more.
- **Grid Layout:** A `Grid` component is provided for easy grid-based layouts.
- **TypeScript Support:** Fully typed for a better development experience.

## Showcase

<table>
  <tr>
    <td><img src="https://i.ibb.co/VpPZybzF/profile-card.webp" alt="Image 1"></td>
    <td><img src="https://i.ibb.co/zTgrBWpT/profile-card-1.webp" alt="Image 2"></td>
  </tr>
  <tr>
    <td><img src="https://i.ibb.co/F4xfHdBp/daily-notes.webp" alt="Image 3"></td>
    <td><img src="https://i.ibb.co/Jj0x6khB/character-archive-base.webp" alt="Image 4"></td>
  </tr>
</table>

## Installation

```bash
yarn add @meonode/canvas
```

## Basic Usage

Here's a simple example of how to create an image with a title and a description:

```typescript
import {Root, Box, Text} from '@meonode/canvas';
import {writeFile} from 'fs/promises';

async function generateImage() {
  const canvas = await Root({
    width: 500,
    height: 300,
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

  const buffer = await canvas.toBuffer('png');
  await writeFile('output.png', buffer);
}

generateImage().catch(console.error);
```

## Wider Usage Example

This example demonstrates a more complex layout using various components, including `Image` and advanced flexbox
properties.

```typescript
import {Root, Column, Row, Text, Image, Style} from '@meonode/canvas';
import {writeFile} from 'fs/promises';

async function generateComplexImage() {
  const canvas = await Root({
    width: 800,
    height: 600,
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
        justifyContent: Style.Justify.SpaceBetween,
        children: [
          // Header Section
          Row({
            width: '100%',
            alignItems: Style.Align.Center,
            marginBottom: 20,
            children: [
              Image({
                src: 'https://via.placeholder.com/80x80/FF0000/FFFFFF?text=Logo',
                width: 80,
                height: 80,
                borderRadius: 40,
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

          // Main Content Section
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
                objectPosition: {Top: '50%', Left: '50%'},
              }),
            ],
          }),

          // Footer Section
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

  const buffer = await canvas.toBuffer('png');
  await writeFile('complex_output.png', buffer);
}

generateComplexImage().catch(console.error);
```

## Using Yoga Layout Properties

This library leverages `yoga-layout` for its powerful flexbox engine. Many layout properties directly map to Yoga's
concepts. You can access Yoga-specific constants through the `Style` export from `@meonode/canvas`.

For example, to set `flexDirection` to `row` or `positionType` to `absolute`, you would use:

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

Refer to the [Yoga Layout documentation](https://yogalayout.dev/docs/) for a comprehensive understanding of these properties.

## Component API Reference

This section details the props available for each component.

### Box, Row, and Column

These are the fundamental layout components. `Row` and `Column` are wrappers around `Box` with a pre-set
`flexDirection`. They all share the same props.

#### Layout Props

| Prop                    | Type                         | Description                                                                    |
|-------------------------|------------------------------|--------------------------------------------------------------------------------|
| `width`, `height`       | `number \| string`           | Sets the size of the node in pixels or percentage.                             |
| `minWidth`, `minHeight` | `number \| string`           | Sets the minimum size of the node.                                             |
| `maxWidth`, `maxHeight` | `number \| string`           | Sets the maximum size of the node.                                             |
| `flexDirection`         | `Style.FlexDirection`        | Defines the direction of the main axis (`Row`, `Column`, etc.).                |
| `justifyContent`        | `Style.Justify`              | Defines how items are distributed along the main axis.                         |
| `alignItems`            | `Style.Align`                | Defines how items are aligned along the cross axis.                            |
| `alignSelf`             | `Style.Align`                | Overrides the parent's `alignItems` for a specific item.                       |
| `alignContent`          | `Style.Align`                | Defines how lines are distributed when content wraps.                          |
| `flexGrow`              | `number`                     | Defines the ability of an item to grow.                                        |
| `flexShrink`            | `number`                     | Defines the ability of an item to shrink.                                      |
| `flexBasis`             | `number \| 'auto' \| string` | Defines the default size of an item along the main axis.                       |
| `flexWrap`              | `Style.Wrap`                 | Controls whether flex items wrap to multiple lines.                            |
| `positionType`          | `Style.PositionType`         | Sets the positioning method (`Relative` or `Absolute`).                        |
| `position`              | `object \| number \| string` | Sets the offset for a positioned element.                                      |
| `margin`                | `object \| number \| string` | Sets the margin space on the outside of the node.                              |
| `padding`               | `object \| number \| string` | Sets the padding space on the inside of the node.                              |
| `border`                | `object \| number`           | Sets the width of the node's border.                                           |
| `aspectRatio`           | `number`                     | Locks the aspect ratio (width / height) of the node.                           |
| `overflow`              | `Style.Overflow`             | Defines how content that overflows is handled (`Visible`, `Hidden`).           |
| `display`               | `Style.Display`              | Controls if the node is included in layout (`Flex`, `None`).                   |
| `direction`             | `Style.Direction`            | Sets the primary layout direction (`LTR`, `RTL`).                              |
| `gap`                   | `object \| number \| string` | Defines the space between flex items.                                          |
| `boxSizing`             | `Style.BoxSizing`            | Defines how `width` and `height` are interpreted (`ContentBox`, `BorderBox`).  |
| `zIndex`                | `number`                     | Specifies the stack order of an element (only for `positionType: 'absolute'`). |

#### Styling Props

| Prop              | Type                                 | Description                                            |
|-------------------|--------------------------------------|--------------------------------------------------------|
| `backgroundColor` | `string`                             | Sets the background color of the node.                 |
| `borderColor`     | `string`                             | Sets the color of the node's border.                   |
| `borderStyle`     | `Style.Border`                       | Sets the style of the border (`Solid`, `Dashed`).      |
| `borderRadius`    | `object \| number`                   | Sets the radius of the node's corners.                 |
| `opacity`         | `number`                             | Sets the opacity of the node and its children (0-1).   |
| `gradient`        | `object`                             | Sets a linear or radial gradient as the background.    |
| `boxShadow`       | `BoxShadowProps \| BoxShadowProps[]` | Applies one or more box-shadow effects.                |
| `transform`       | `TransformProps`                     | Applies 2D transformations (translate, rotate, scale). |

#### Font & Text Props (Inheritable)

These props, when set on a `Box`, `Row`, or `Column`, are inherited by any descendant `Text` nodes.

| Prop            | Type                                                             | Description                                |
|-----------------|------------------------------------------------------------------|--------------------------------------------|
| `fontSize`      | `number`                                                         | Font size in pixels.                       |
| `fontFamily`    | `string`                                                         | Font family name.                          |
| `fontWeight`    | `string \| number`                                               | Font weight ('normal', 'bold', 400, etc.). |
| `fontStyle`     | `'normal' \| 'italic'`                                           | Font style.                                |
| `color`         | `string`                                                         | Text color.                                |
| `textAlign`     | `'start' \| 'end' \| 'left' \| 'center' \| 'right' \| 'justify'` | Horizontal text alignment.                 |
| `verticalAlign` | `'top' \| 'middle' \| 'bottom'`                                  | Vertical text alignment.                   |
| `lineHeight`    | `number`                                                         | Line height in pixels.                     |
| `lineGap`       | `number`                                                         | Additional vertical spacing between lines. |
| `letterSpacing` | `number \| string`                                               | Spacing between letters.                   |
| `wordSpacing`   | `number \| string`                                               | Spacing between words.                     |
| `fontVariant`   | `FontVariantSetting`                                             | Specifies font variation settings.         |

---

### Text

The `Text` component renders text content. It inherits all `BoxProps` except for `children`, `gap`, and flex container
properties.

#### Text-Specific Props

| Prop         | Type                                   | Description                                                                |
|--------------|----------------------------------------|----------------------------------------------------------------------------|
| `maxLines`   | `number`                               | Maximum number of lines to display before truncating.                      |
| `ellipsis`   | `boolean \| string`                    | If `true`, adds '...' when text is truncated. Can also be a custom string. |
| `textShadow` | `TextShadowProps \| TextShadowProps[]` | Applies one or more shadow effects to the text itself.                     |

---

### Image

The `Image` component renders an image. It inherits all `BoxProps` except for `children`.

#### Image-Specific Props

| Prop             | Type                                                       | Description                                                           |
|------------------|------------------------------------------------------------|-----------------------------------------------------------------------|
| `src`            | `string \| Buffer`                                         | The source URL, file path, or buffer of the image.                    |
| `objectFit`      | `'fill' \| 'contain' \| 'cover' \| 'none' \| 'scale-down'` | Specifies how the image should be resized to fit its container.       |
| `objectPosition` | `object`                                                   | Specifies the alignment of the image within its box.                  |
| `saturate`       | `number`                                                   | Adjusts the image's saturation level (0 is grayscale, 1 is original). |
| `dropShadow`     | `DropShadowProps`                                          | Applies a drop-shadow effect based on the image's alpha channel.      |
| `alt`            | `string`                                                   | Alternative text description (for accessibility).                     |
| `onLoad`         | `() => void`                                               | Callback function that executes when the image loads successfully.    |
| `onError`        | `(error: Error) => void`                                   | Callback function that executes when the image fails to load.         |

---

### Grid

The `Grid` component arranges its children in a grid layout. It is a specialized `RowNode` and inherits most `BoxProps`.

#### Grid-Specific Props

| Prop        | Type                                                     | Description                                         |
|-------------|----------------------------------------------------------|-----------------------------------------------------|
| `columns`   | `number`                                                 | The number of columns in the grid. Default is 1.    |
| `direction` | `'row' \| 'column' \| 'row-reverse' \| 'column-reverse'` | The direction of the grid layout. Default is 'row'. |

---

### Root

The `Root` component is the entry point for rendering. It is a specialized `ColumnNode`.

#### Root-Specific Props

| Prop     | Type                     | Description                                                              |
|----------|--------------------------|--------------------------------------------------------------------------|
| `width`  | `number`                 | **Required.** Width of the canvas in pixels.                             |
| `height` | `number`                 | Optional height of the canvas. If not set, it's calculated from content. |
| `scale`  | `number`                 | Scale factor for rendering (e.g., 2 for 2x resolution). Default is 1.    |
| `fonts`  | `FontRegistrationInfo[]` | An array of font files to register for use in the canvas.                |


## Contributing



Contributions are welcome! Please see the [Contributing Guidelines](CONTRIBUTING.md) for more details on how to get started.



## License



This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

