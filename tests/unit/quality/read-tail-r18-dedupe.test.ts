import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { qualityIdInSourceSql } from '../../../packages/server-quality/src/adapters/quality-id-source-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');

// Pre-dedupe id-source membership fragments copied byte-for-byte from HEAD.
const fragments = [
  ['${commerce.returns}', 'returned', 'IN (SELECT id FROM (${commerce.returns}) returned)'],
  ['${commerce.returnLines}', 'line', 'IN (SELECT id FROM (${commerce.returnLines}) line)'],
  ['${commerce.returnLots}', 'lot', 'IN (SELECT id FROM (${commerce.returnLots}) lot)'],
] as const;

describe('quality customer-return id-source family dedupe (r18)', () => {
  test.each(fragments)('helper reproduces fragment %s %s byte-for-byte', (source, alias, before) => {
    expect(qualityIdInSourceSql(source, alias)).toBe(before);
    expect(qualityIdInSourceSql(source, alias).match(/SELECT /g)).toHaveLength(1);
  });
  test('the settlement readers share the helper with no inline id-source SELECTs', () => {
    const text = read('adapters/ecommerce-package-sources.ts');
    expect(text.match(/qualityIdInSourceSql\(/g)).toHaveLength(3);
    expect(text).not.toContain('IN (SELECT id FROM (');
    expect(text).toContain("native.owner_id ${qualityIdInSourceSql(commerce.returns, 'returned')}");
    expect(text).toContain("hold.source_id ${qualityIdInSourceSql(commerce.returnLines, 'line')}");
    expect(text).toContain("hold.source_id ${qualityIdInSourceSql(commerce.returnLots, 'lot')}");
    expect(read('adapters/quality-id-source-sql.ts').match(/SELECT /g)).toHaveLength(1);
  });
  test('the helper is read-only; descriptor cutover, locks and r1-r17 helpers stay untouched', () => {
    const helper = read('adapters/quality-id-source-sql.ts');
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    expect(read('adapters/purchase-sources.ts')).toContain('qualityPurchaseSourceReadLocks');
    expect(read('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_specs');
    expect(read('adapters/sales-package-sources.ts')).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_inspections,inventory_quality_disposition_events,inventory_quality_cancellations');
    for (const name of [
      'quality-work-scope-sql', 'native-source-wrapper-sql', 'inspection-evidence-source-sql',
      'quality-record-source-sql', 'quality-lineage-recursion-sql', 'quality-child-agg-sql',
      'quality-receipt-guard-sql', 'quality-source-rows-sql', 'quality-scope-sql',
      'quality-alert-recipient-sql', 'quality-attachment-scope-read-sql',
    ]) {
      const expected = ['quality-lineage-recursion-sql', 'quality-scope-sql'].includes(name) ? 2 : 1;
      expect(read(`adapters/${name}.ts`).match(/SELECT /g)).toHaveLength(expected);
    }
  });
});

describe('quality read-tail r18 retirement check', () => {
  test('retirement:ts-statements is strictly lower than the BEFORE=75 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/INSERT INTO|DELETE FROM|UPDATE [a-z_]+ SET|SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(75);
    expect(total).toBe(59);
  });
});
