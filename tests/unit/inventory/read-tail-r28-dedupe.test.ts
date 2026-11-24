import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { evidencePositionRowsSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;

// Verbatim pre-dedupe literals from HEAD for the three executive-evidence
// position page searches. Only the source relation and the row alias varied;
// the wrapper, condition slot, order and limit were byte-identical. These are
// byte-identity oracles only, never live query sources.
type PageCase = [source: string, alias: string];
const cases: PageCase[] = [
  ['${availableStockPositions}', 'stock'],
  ['${inventoryShortagePositions}', 'shortages'],
  ['${stockedLocationPositions}', 'stock'],
];
const conditionsExpr = '${conditions.join(" AND ")}';
const preDedupe = ([source, alias]: PageCase) =>
  `SELECT * FROM (${source}) ${alias} WHERE ${conditionsExpr} ORDER BY warehouse_id,product_id LIMIT ?`;

describe('Inventory read-tail r28 parameterized position-page wrapper dedupe', () => {
  test('helper reproduces all three pre-dedupe literals byte-identically and stays read-only', () => {
    for (const c of cases) {
      expect(evidencePositionRowsSql(c[0], c[1], conditionsExpr)).toBe(preDedupe(c));
      const sql = evidencePositionRowsSql(c[0], c[1], conditionsExpr);
      expect(sql).toContain('SELECT ');
      expect(sql).not.toMatch(lockRe);
      // the wrapper contributes exactly one SELECT token
      expect(selectCount(sql)).toBe(1);
    }
  });

  test('r28 helper literal is defined exactly once in the canonical owner', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('evidencePositionRowsSql =').length - 1).toBe(1);
  });

  test('former inline literals are gone and every call site composes the helper with unchanged binds', () => {
    const text = src('executive-main-evidence-sources.ts');
    for (const c of cases) expect(text).not.toContain(preDedupe(c));
    expect(text).toContain(`evidencePositionRowsSql(availableStockPositions, 'stock', conditions.join(" AND "))`);
    expect(text).toContain(`evidencePositionRowsSql(inventoryShortagePositions, 'shortages', conditions.join(" AND "))`);
    expect(text).toContain(`evidencePositionRowsSql(stockedLocationPositions, 'stock', conditions.join(" AND "))`);
    expect(text.match(/evidencePositionRowsSql\(/g) ?? []).toHaveLength(3); // three calls
    // bind order for each former site is unchanged
    expect((text.match(/\.bind\(\.\.\.values, input\.limit \+ 1\)/g) ?? []).length).toBeGreaterThanOrEqual(3);
    expect(text).not.toMatch(lockRe);
  });

  test('r11-r27 helpers, count source adapters, native registry and lock statements stay untouched', () => {
    const shared = src('read-tail-sql.ts');
    for (const helper of [
      'reservedWarehouseProductQtySql',
      'packageObligationDocumentRowsSql',
      'sourceScopeRowsSql',
      'scopeExistsSql',
      'journalAccountLineExistsSql',
      'childLineOwnerExistsSql',
      'nativeRecordExistsSql',
      'branchScopeTupleSql',
      'pendingLossSumSql',
      'transitLossDecisionPendingSql',
    ]) expect(shared).toContain(`export const ${helper}`);
    for (const f of [
      'initial-physical-stock-count-source-sql.ts',
      'manufacturing-initial-physical-stock-count-source-sql.ts',
      'pos-initial-physical-stock-count-source-sql.ts',
      'service-initial-physical-stock-count-source-sql.ts',
    ]) expect(src(f).length).toBeGreaterThan(0);
    const nativeRead = readFileSync(resolve(srcRoot, 'adapters', 'native-inventory-read.ts'), 'utf8');
    expect(nativeRead.length).toBeGreaterThan(0);
    expect(shared).not.toMatch(lockRe);
  });

  test('retirement:ts-statements stays strictly below the r28 BEFORE=363 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r28 deduped the 3x byte-identical position-page wrapper in
    // executive-main-evidence-sources.ts into one shared parameterized helper.
    expect(total).toBe(361);
    expect(total).toBeLessThan(363);
  });
});
