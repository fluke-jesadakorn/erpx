import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  resolve: {
    alias: {
      'server-only': fileURLToPath(new URL('../../support/server-only.ts', import.meta.url)),
    },
  },
  test: {
    include: [
      'tests/unit/identity_access/read-tail-r22-dedupe.test.ts',
      'tests/unit/identity_access/read-tail-r30-dedupe.test.ts',
      'tests/unit/identity_access/read-tail-r31-dedupe.test.ts',
      'tests/unit/identity_access/read-tail-r32-dedupe.test.ts',
      'tests/unit/identity_access/read-tail-r33-dedupe.test.ts',
      'tests/unit/identity_access/read-tail-r34-dedupe.test.ts',
      'tests/unit/identity_access/read-tail-r35-dedupe.test.ts',
      'tests/unit/identity_access/read-tail-r36-dedupe.test.ts',
      'tests/unit/identity_access/read-tail-r38-dedupe.test.ts',
      'tests/unit/identity_access/read-tail-r39-dedupe.test.ts',
      'tests/unit/identity_access/read-tail-r40-dedupe.test.ts',
      'tests/unit/identity_access/read-tail-r41-dedupe.test.ts',
      'tests/unit/identity_access/read-tail-r42-dedupe.test.ts',
    ],
    pool: 'forks',
  },
});
