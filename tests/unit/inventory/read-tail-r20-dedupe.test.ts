import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { countInitialPhysicalSourceMarkersSql, postedInitialPhysicalStockSourceSql } from '../../../modules/inventory/server/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../modules/inventory/server/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

const initial = '<initial>', mfg = '<mfg>', pos = '<pos>', service = '<service>';
const expectedMarkers =
  `EXISTS(SELECT 1 FROM (${initial}) actual_initial WHERE actual_initial.id=c.id AND actual_initial.company_id=c.company_id) initial_physical_source,` +
  `EXISTS(SELECT 1 FROM (${mfg}) actual_mfg_initial WHERE actual_mfg_initial.id=c.id AND actual_mfg_initial.company_id=c.company_id) manufacturing_initial_physical_source,` +
  `EXISTS(SELECT 1 FROM (${pos}) actual_pos_initial WHERE actual_pos_initial.id=c.id AND actual_pos_initial.company_id=c.company_id) pos_initial_physical_source,` +
  `EXISTS(SELECT 1 FROM (${service}) actual_service_initial WHERE actual_service_initial.id=c.id AND actual_service_initial.company_id=c.company_id) service_initial_physical_source`;
const expectedPosted = (source: string, alias: string) =>
  `EXISTS(SELECT 1 FROM (${source}) source JOIN inventory_counts ${alias} ON ${alias}.id=source.id AND ${alias}.company_id=source.company_id AND ${alias}.status='posted' WHERE source.company_id=scope.company_id AND source.warehouse_id=stock.warehouse_id AND source.product_id=stock.product_id)`;

describe('Inventory read-tail r20 parameterized EXISTS-family dedupe', () => {
  test('canonical helpers render byte-identical read-only SQL', () => {
    expect(countInitialPhysicalSourceMarkersSql(initial, mfg, pos, service)).toBe(expectedMarkers);
    expect(postedInitialPhysicalStockSourceSql('<rows>', 'posted')).toBe(expectedPosted('<rows>', 'posted'));
    expect(postedInitialPhysicalStockSourceSql('<rows>', 'posted_count')).toBe(expectedPosted('<rows>', 'posted_count'));
    for (const sql of [expectedMarkers, expectedPosted('<rows>', 'posted'), expectedPosted('<rows>', 'posted_count')]) readOnly(sql);
  });

  test('r20 helper literals are defined exactly once in the canonical owner', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('countInitialPhysicalSourceMarkersSql =')).toHaveLength(2);
    expect(shared.split('postedInitialPhysicalStockSourceSql =')).toHaveLength(2);
  });

  test('initial-physical source markers are composed, never inlined, in inventory-counts', () => {
    const text = src('inventory-counts.ts');
    expect(text).not.toContain('actual_initial WHERE actual_initial.id=c.id');
    expect(text.match(/countInitialPhysicalSourceMarkersSql\(/g) ?? []).toHaveLength(2);
  });

  test('posted source probe is composed, never inlined, in sales/mfg/pos obligations', () => {
    for (const [file, rows, alias] of [
      ['sales-package-obligations.ts', 'initialPhysicalStockCountSourceRows', 'posted_count'],
      ['manufacturing-package-obligations.ts', 'manufacturingInitialPhysicalStockCountSourceRows', 'posted'],
      ['pos-package-obligations.ts', 'posInitialPhysicalStockCountSourceRows', 'posted'],
    ] as const) {
      const text = src(file);
      expect(text).not.toContain(`JOIN inventory_counts ${alias} ON ${alias}.id=source.id`);
      expect(text).toContain(`postedInitialPhysicalStockSourceSql(${rows},'${alias}')`);
      expect(text.match(/postedInitialPhysicalStockSourceSql\(/g) ?? []).toHaveLength(1);
    }
  });

  test('retirement:ts-statements stays strictly below the r20 BEFORE=399 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(399);
  });
});
