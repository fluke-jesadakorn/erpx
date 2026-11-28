import { describe, expect, test } from 'vitest';
import { readFileSync } from 'node:fs';
import {
  noLaterLedgerMovementSql,
  noLotBalanceSql,
  jsonbImageSql,
  posInitialPhysicalSchemaReadySql,
  mfgInitialPhysicalSchemaReadySql,
} from '../../../modules/inventory/server/src/adapters/read-tail-sql';

const src = (name: string) => readFileSync(new URL(`../../../modules/inventory/server/src/adapters/${name}`, import.meta.url), 'utf8');

const ledgerGuard = 'NOT EXISTS(SELECT 1 FROM inventory_ledger WHERE warehouse_id=? AND product_id=? AND movement_date>?)';
const lotBalanceGuard = 'NOT EXISTS(SELECT 1 FROM inventory_lot_balances WHERE lot_id=? AND warehouse_id=?)';
const jsonbPrefix = 'SELECT jsonb_build_object(';
const posReady = "SELECT to_regclass('inventory_pos_initial_physical_sources') IS NOT NULL ready";
const mfgReady = "SELECT to_regclass('inventory_mfg_initial_physical_sources') IS NOT NULL ready";

describe('Inventory read-tail r12 byte-identical dedupe', () => {
  test('canonical literals own the exact shared text and stay read-only', () => {
    expect(noLaterLedgerMovementSql).toBe(ledgerGuard);
    expect(noLotBalanceSql).toBe(lotBalanceGuard);
    expect(jsonbImageSql).toBe(jsonbPrefix);
    expect(posInitialPhysicalSchemaReadySql).toBe(posReady);
    expect(mfgInitialPhysicalSchemaReadySql).toBe(mfgReady);
    for (const sql of [noLaterLedgerMovementSql, noLotBalanceSql, jsonbImageSql, posInitialPhysicalSchemaReadySql, mfgInitialPhysicalSchemaReadySql]) {
      expect(sql).toContain('SELECT');
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|LOCK\s+TABLE/i);
    }
  });

  test('shared owner literals are defined exactly once', () => {
    const shared = src('read-tail-sql.ts');
    for (const sql of [noLaterLedgerMovementSql, noLotBalanceSql, jsonbImageSql]) {
      expect(shared.split(sql), sql).toHaveLength(2);
    }
    // r46 folds the two to_regclass literals into one whitelisted builder.
    expect(shared.split("SELECT to_regclass('${table}') IS NOT NULL ready")).toHaveLength(2);
    expect(shared.split('export const schemaReadySql').length - 1).toBe(1);
  });

  test('guard literals are composed, never inlined, in the named adapters', () => {
    // Each identifier appears once in the import plus once per composed caller.
    const cases = [
      ['commerce-facts.ts', { noLaterLedgerMovementSql: 2, noLotBalanceSql: 2 }],
      ['inventory-transit.ts', { noLaterLedgerMovementSql: 3, noLotBalanceSql: 2 }],
      ['production-command-sources.ts', { noLaterLedgerMovementSql: 3, noLotBalanceSql: 0 }],
    ] as const;
    for (const [name, counts] of cases) {
      const text = src(name);
      expect(text, name).not.toContain(ledgerGuard);
      expect(text, name).not.toContain(lotBalanceGuard);
      expect(text.match(/noLaterLedgerMovementSql/g) ?? [], name).toHaveLength(counts.noLaterLedgerMovementSql);
      expect(text.match(/noLotBalanceSql/g) ?? [], name).toHaveLength(counts.noLotBalanceSql);
    }
  });

  test('jsonb image prefix has one owner literal and five composed callers', () => {
    const text = src('attachment-read-sources.ts');
    expect(text).not.toContain(jsonbPrefix);
    expect(text.match(/jsonbImageSql/g) ?? []).toHaveLength(6);
  });

  test('schema-readiness checks are composed twice each within their command', () => {
    const pos = src('pos-initial-physical-stock-count.ts');
    expect(pos).not.toContain(posReady);
    expect(pos.match(/posInitialPhysicalSchemaReadySql/g) ?? []).toHaveLength(3); // import + 2 callers
    const mfg = src('manufacturing-initial-physical-stock-count.ts');
    expect(mfg).not.toContain(mfgReady);
    expect(mfg.match(/mfgInitialPhysicalSchemaReadySql/g) ?? []).toHaveLength(3); // import + 2 callers
  });

  test('statements stay independent: every caller still prepares its own SQL', () => {
    expect(src('pos-initial-physical-stock-count.ts').match(/database\.prepare\(posInitialPhysicalSchemaReadySql\)/g) ?? []).toHaveLength(2);
    expect(src('manufacturing-initial-physical-stock-count.ts').match(/database\.prepare\(mfgInitialPhysicalSchemaReadySql\)/g) ?? []).toHaveLength(2);
  });
});
