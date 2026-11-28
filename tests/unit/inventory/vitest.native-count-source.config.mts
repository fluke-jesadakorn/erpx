import { defineConfig } from 'vitest/config';
import { fileURLToPath } from 'node:url';

/** Isolated config for the newly added count-purchase native-read suite; the
 * shared catalogue config cannot select a file before it is registered. */
export default defineConfig({
  resolve: { alias: { 'server-only': fileURLToPath(new URL('../../support/server-only.ts', import.meta.url)) } },
  test: { include: ['tests/unit/inventory/native-count-source-read.test.ts'], pool: 'forks' },
});
