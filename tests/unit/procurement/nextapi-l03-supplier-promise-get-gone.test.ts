import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

const from = (path: string) => new URL(`../../../${path}`, import.meta.url);
const text = (path: string) => readFileSync(from(path), 'utf8');

const shared = 'packages/feature-supply/src/routes/api/v1/holdings/[holdingSlug]/supplier-promises/[id]/route.ts';
const appExports = [
  'apps/supply/src/app/api/v1/holdings/[holdingSlug]/supplier-promises/[id]/route.ts',
  'apps/compat/src/app/api/v1/holdings/[holdingSlug]/supplier-promises/[id]/route.ts',
];
const nativeRouter = 'packages/server-procurement/src/adapters/supplier_promise_command_http.rs';
const nativeHttpTest = 'tests/unit/procurement/rust-supplier-promise-detail-http.test.rs';

describe('nextapi L03: GET supplier-promises/{id} runs native only', () => {
  it('no longer exports a TypeScript GET from the shared Next route', () => {
    const source = text(shared);
    expect(source).not.toMatch(/export\s+async\s+function\s+GET\b/);
    expect(source).not.toMatch(/\bownedSupplierPromiseDetail\b|\bsupplierPromiseDetail\b/);
    expect(source).toMatch(/export\s+async\s+function\s+POST\b/);
  });

  it('every app re-export dropped GET but kept POST', () => {
    for (const file of appExports) {
      const source = text(file);
      expect(source).toMatch(/export\s*\{\s*POST\s*\}/);
      expect(source).not.toMatch(/\bGET\b/);
    }
  });

  it('native command router serves GET on the exact public path and executes real SQL', () => {
    const source = text(nativeRouter);
    expect(source).toContain('/api/v1/holdings/{holding_slug}/supplier-promises/{order_id}');
    expect(source).toMatch(/get\(read_detail\)\.post\(record_promise\)/);
    const httpTest = text(nativeHttpTest);
    expect(httpTest).toContain('supplier_promise_command_http_router');
    expect(httpTest).toMatch(/\bdata\b/);
  });
});
