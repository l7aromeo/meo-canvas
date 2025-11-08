# @meonode/canvas

A declarative, component-based library for generating images on a canvas, inspired by the MeoNode UI engine for React. It uses `skia-canvas` for drawing and `yoga-layout` for flexbox-based layouts.

This library allows you to build complex image layouts using a familiar component-based approach. You can define your image structure with components like `Box`, `Text`, `Image`, and `Grid`, and the library will handle the layout and rendering to a canvas.

## Key Features

- **Declarative API:** Build images using a component tree, just like in React.
- **Flexbox Layout:** Powered by `yoga-layout`, it supports flexbox for powerful and flexible layouts.
- **Rich Text:** Render text with custom fonts and inline styling using simple HTML-like tags. Supported tags include `<color="value">`, `<weight="value">`, `<size="value">`, `<b>`, and `<i>`.
- **Image Support:** Render images from URLs, file paths, or buffers, with `object-fit` and `object-position` support.
- **Styling:** Style your components with properties that mimic CSS, including borders, padding, margins, and more.
- **Grid Layout:** A `Grid` component is provided for easy grid-based layouts.
- **TypeScript Support:** Fully typed for a better development experience.

## Installation

```bash
yarn add @meonode/canvas
```

## Basic Usage

Here's a simple example of how to create an image with a title and a description:

```typescript
import { Root, Box, Text } from '@meonode/canvas';
import { writeFile } from 'fs/promises';

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
            margin: { Top: 10 },
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
