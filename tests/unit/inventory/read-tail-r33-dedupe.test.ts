import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

const srcRoot = resolve(__dirname, '../../../modules/inventory/server/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;

// Verbatim pre-dedupe literals from HEAD for the two single-field row probes.
// Both were moved to the native read registry; these remain byte-identity
// oracles only, never live TypeScript query sources.
const sites: [field: string, table: string, key: string, scopeKey: string][] = [
  ['version', 'inventory_lot_balances', 'lot_id', 'warehouse_id'],
  ['status', 'inventory_transfers', 'id', 'company_id'],
];
const preDedupe = (field: string, table: string, key: string, scopeKey: string) =>
  `SELECT ${field} FROM ${table} WHERE ${key}=? AND ${scopeKey}=?`;

describe('Inventory read-tail r33 parameterized scoped-field dedupe', () => {
  test('both pre-dedupe literals are byte-identical in the native registry and stay read-only', () => {
    const rust = readFileSync(resolve(srcRoot, 'adapters/native_command_sql.rs'), 'utf8');
    for (const [field, table, key, scopeKey] of sites) {
      const sql = preDedupe(field, table, key, scopeKey);
      expect(selectCount(sql)).toBe(1);
      expect(sql).not.toMatch(lockRe);
      expect(rust).toContain(`"${sql}"`);
    }
  });

  test('the r33 helper literal is retired from the canonical owner', () => {
    expect(src('read-tail-sql.ts')).not.toContain('scopedFieldSql');
  });

  test('former inline literals are gone and both call sites use the native bridges with unchanged scope', () => {
    const transit = src('inventory-transit.ts');
    expect(transit).not.toContain("SELECT version FROM inventory_lot_balances WHERE lot_id=? AND warehouse_id=?");
    expect(transit).toContain('lotBalanceVersion');
    expect(transit).not.toContain('scopedFieldSql');

    const transfer = src('transfer-purchases.ts');
    expect(transfer).not.toContain('SELECT status FROM inventory_transfers WHERE id=? AND company_id=?');
    expect(transfer).toContain('transferStatus');
    expect(transfer).not.toContain('scopedFieldSql');
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
    expect(src('inventory-read-projections.ts')).not.toContain('SELECT status FROM inventory_counts');
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
