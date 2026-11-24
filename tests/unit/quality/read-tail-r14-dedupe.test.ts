import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { qualityLineageSeedSql, qualityLineageStepSql } from '../../../packages/server-quality/src/adapters/quality-lineage-recursion-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-quality/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const purchase = read('adapters/purchase-sources.ts');
const sales = read('adapters/sales-package-sources.ts');
const ecommerce = read('adapters/ecommerce-package-sources.ts');
const helper = read('adapters/quality-lineage-recursion-sql.ts');

// Pre-dedupe SQL fragments copied from HEAD, used only as byte-identity oracles.
const seeds = [
  ['h.*', 'inventory_quality_holds h', 'SELECT h.*,0 depth FROM inventory_quality_holds h'],
  ['id hold_id,id root_id', 'inventory_quality_holds', 'SELECT id hold_id,id root_id,0 depth FROM inventory_quality_holds'],
  ['anchors.id,anchors.source_company', 'anchors', 'SELECT anchors.id,anchors.source_company,0 depth FROM anchors'],
  ['id', 'anchors', 'SELECT id,0 depth FROM anchors'],
] as const;
const steps = [
  ['parent.*', 'child', 'SELECT parent.*,child.depth+1 FROM lineage child'],
  ['walk.hold_id,parent.id', 'walk', 'SELECT walk.hold_id,parent.id,walk.depth+1 FROM lineage walk'],
  ['child.id,parent.source_company', 'parent', 'SELECT child.id,parent.source_company,parent.depth+1 FROM lineage parent'],
  ['child.id', 'parent', 'SELECT child.id,parent.depth+1 FROM lineage parent'],
] as const;

describe('quality recursive lineage recursion family dedupe (r14)', () => {
  test.each(seeds)('seed %s reproduces the inline anchor byte-for-byte', (columns, source, before) => {
    expect(qualityLineageSeedSql(columns, source)).toBe(before);
  });
  test.each(steps)('step %s reproduces the inline recursive member byte-for-byte', (columns, alias, before) => {
    expect(qualityLineageStepSql(columns, alias)).toBe(before);
  });

  test('the eight callers share the two helpers with no inline copies', () => {
    const consumers = [purchase, sales, ecommerce].join('\n');
    expect(consumers.match(/qualityLineageSeedSql\(/g)).toHaveLength(4);
    expect(consumers.match(/qualityLineageStepSql\(/g)).toHaveLength(4);
    for (const [, , before] of [...seeds, ...steps]) expect(consumers).not.toContain(before);
    expect(helper.match(/SELECT /g)).toHaveLength(2);
  });

  test('the family is read-only, locks untouched and r10-r13 helpers stay independent', () => {
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    expect(purchase).toContain('qualityPurchaseSourceReadLocks');
    expect(sales).toContain('LOCK TABLE inventory_quality_holds,inventory_quality_inspections,inventory_quality_disposition_events,inventory_quality_cancellations');
    for (const name of ['quality-child-agg-sql', 'quality-receipt-guard-sql', 'quality-source-rows-sql', 'quality-record-source-sql']) {
      expect(read(`adapters/${name}.ts`).match(/SELECT /g)).toHaveLength(1);
    }
  });
});

describe('quality read-tail r14 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r17 BEFORE=75 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(75);
    expect(total).toBe(70);
  });
});
