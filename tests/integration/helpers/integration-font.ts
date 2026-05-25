import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import type { RootProps } from '@/canvas/canvas.type.js'

export const INTEGRATION_FONT_FAMILY = 'IntegrationRoboto'

const FIXTURES_DIR = join(dirname(fileURLToPath(import.meta.url)), '../../fixtures')
const ROBOTO_PATH = join(FIXTURES_DIR, 'fonts/Roboto-Regular.ttf')

/** Root options shared by integration renders — bundled font for cross-platform consistency. */
export const integrationRootBase: Pick<RootProps, 'fonts'> = {
  fonts: [{ family: INTEGRATION_FONT_FAMILY, paths: [ROBOTO_PATH] }],
}

/** Default text/chart fontFamily for integration tests. */
export const integrationFontFamily = INTEGRATION_FONT_FAMILY
