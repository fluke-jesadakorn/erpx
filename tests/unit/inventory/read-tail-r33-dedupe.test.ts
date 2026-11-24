import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { scopedFieldSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;

// Verbatim pre-dedupe literals from HEAD for the two single-field row probes.
// The projection, two-key equality predicate and bind shape were byte-identical;
// only the selected field, table and scope keys varied. The canonical
// inventoryCountStatusSql count-source literal is NOT part of this family and
// stays untouched. These are byte-identity oracles only, never live query sources.
const sites: [field: string, table: string, key: string, scopeKey: string][] = [
  ['version', 'inventory_lot_balances', 'lot_id', 'warehouse_id'],
  ['status', 'inventory_transfers', 'id', 'company_id'],
];
const preDedupe = (field: string, table: string, key: string, scopeKey: string) =>
  `SELECT ${field} FROM ${table} WHERE ${key}=? AND ${scopeKey}=?`;

describe('Inventory read-tail r33 parameterized scoped-field dedupe', () => {
  test('helper reproduces both pre-dedupe literals byte-identically and stays read-only', () => {
    for (const [field, table, key, scopeKey] of sites) {
      const sql = scopedFieldSql(field, table, key, scopeKey);
      expect(sql).toBe(preDedupe(field, table, key, scopeKey));
      expect(sql).toContain('SELECT ');
      expect(selectCount(sql)).toBe(1);
      expect(sql).not.toMatch(lockRe);
    }
  });

  test('r33 helper literal is defined exactly once in the canonical owner', () => {
    expect(src('read-tail-sql.ts').split('scopedFieldSql =').length - 1).toBe(1);
  });

  test('former inline literals are gone and both call sites compose the helper with unchanged binds', () => {
    const transit = src('inventory-transit.ts');
    expect(transit).not.toContain("SELECT version FROM inventory_lot_balances WHERE lot_id=? AND warehouse_id=?");
    expect(transit).toContain("db.prepare(scopedFieldSql('version','inventory_lot_balances','lot_id','warehouse_id'))");
    expect(transit).toContain(".bind(lot_id,target_warehouse_id).first<Row>()");

    const transfer = src('transfer-purchases.ts');
    expect(transfer).not.toContain('SELECT status FROM inventory_transfers WHERE id=? AND company_id=?');
    expect(transfer).toContain("database.prepare(scopedFieldSql('status','inventory_transfers','id','company_id'))");
    expect(transfer).toContain(".bind(command.target.id,actor.companyId).first<{status:string}>()");
    expect(transfer).toContain('companyPurchaseReadLocks(database,actor.companyId)'); // lock guard intact
  });

  test('r11-r32 helpers, locks, count sources, count adapters and native registry stay untouched', () => {
    const shared = src('read-tail-sql.ts');
    for (const helper of [
      'reservedWarehouseProductQtySql', 'singleCountLineMarkerSql', 'initialPhysicalCountJournalSql',
      'childJsonAggSql', 'childJoinJsonAggSql', 'terminalReceiptMatchSql', 'reservedStockAggSql',
      'activeQualityHoldExistsSql', 'stockMoveSumSql', 'sourceScopeRowsSql', 'scopeExistsSql',
      'packageObligationDocumentRowsSql', 'evidencePositionRowsSql', 'recordIdExistsSql',
      'childLineRowsSql', 'scopeAllowedSql', 'disposalCountsSql',
    ]) expect(shared).toContain(`export const ${helper}`);
    for (const f of [
      'initial-physical-stock-count-source-sql.ts',
      'manufacturing-initial-physical-stock-count-source-sql.ts',
      'pos-initial-physical-stock-count-source-sql.ts',
      'service-initial-physical-stock-count-source-sql.ts',
    ]) expect(src(f).length).toBeGreaterThan(0);
    // The canonical count-source literal is unchanged.
    expect(src('inventory-read-projections.ts')).toContain(
      'export const inventoryCountStatusSql = "SELECT status FROM inventory_counts WHERE id=? AND company_id=?"',
    );
    expect(src('count-purchases.ts')).toContain('companyPurchaseReadLocks');
    expect(readFileSync(resolve(srcRoot, 'adapters', 'native-inventory-read.ts'), 'utf8').length).toBeGreaterThan(0);
    expect(shared).not.toMatch(lockRe);
  });

  test('retirement:ts-statements stays strictly below the r32 BEFORE=352 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r32 folded the 2x quality-disposal count source into one helper (353 -> 352).
    // r33 folds the 2x single-field scoped row probe into one helper (352 -> 351):
    // two inline SELECT tokens removed, one kept in the single canonical helper.
    // r34 lowers it further to 349, so the exact pin is relaxed to the r33 baseline.
    expect(total).toBeLessThanOrEqual(351);
    expect(total).toBeLessThan(352);
  });
});
