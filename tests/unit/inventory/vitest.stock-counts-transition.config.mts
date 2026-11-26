import {defineConfig} from 'vitest/config';
import {fileURLToPath} from 'node:url';

/** Isolated config for the stock-counts catch-all execution-retirement suite;
 * the shared catalogue config cannot select a file before it is registered. */
export default defineConfig({
  resolve: {alias: {'@': fileURLToPath(new URL('../../src', import.meta.url)), 'server-only': fileURLToPath(new URL('../../support/server-only.ts', import.meta.url))}},
  test: {include: ['tests/unit/inventory/stock-counts-transition-native-exec.test.ts'], pool: 'forks'},
});
