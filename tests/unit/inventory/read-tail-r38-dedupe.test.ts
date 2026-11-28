import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { attachmentLotIdsSql } from '../../../modules/inventory/server/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../modules/inventory/server/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;

// Verbatim pre-dedupe literals from HEAD, used strictly as byte-identity oracles
// (never as live query sources). Only the lot relation, line relation, child FK
// and parent FK vary; every caller still keeps its own bind shape and statement.
const headLotIds = (lots: string, lines: string, lineFk: string, parentFk: string) =>
  `ARRAY(SELECT lot_id FROM ${lots} cl JOIN ${lines} l ON l.id=cl.${lineFk} WHERE l.${parentFk}=d.id) lot_ids`;

const sites: [lots: string, lines: string, lineFk: string, parentFk: string][] = [
  ['inventory_landed_cost_lots', 'inventory_landed_cost_lines', 'landed_cost_line_id', 'landed_cost_id'],
  ['inventory_transit_lots', 'inventory_transit_lines', 'transit_line_id', 'transit_id'],
  ['inventory_transfer_lot_allocations', 'inventory_transfer_lines', 'transfer_line_id', 'transfer_id'],
];

describe('Inventory read-tail r38 dedupe', () => {
  test('f1: attachmentLotIdsSql reproduces all three pre-dedupe literals byte-identically', () => {
    for (const [lots, lines, lineFk, parentFk] of sites) {
      const sql = attachmentLotIdsSql(lots, lines, lineFk, parentFk);
      expect(sql).toBe(headLotIds(lots, lines, lineFk, parentFk));
      expect(selectCount(sql)).toBe(1);
      expect(sql).not.toMatch(lockRe);
    }
  });

  test('the three pre-dedupe literals share one skeleton after parameter masking', () => {
    const mask = (s: string, a: string, b: string, c: string, d: string) => s
      .split(a).join('<LOTS>').split(b).join('<LINES>')
      .split(c).join('<CHILD_FK>').split(d).join('<PARENT_FK>');
    const rendered = sites.map(([lots, lines, lineFk, parentFk]) =>
      mask(attachmentLotIdsSql(lots, lines, lineFk, parentFk), lots, lines, lineFk, parentFk));
    expect(rendered[0]).toBe(rendered[1]);
    expect(rendered[1]).toBe(rendered[2]);
  });

  test('helper template carries exactly one SELECT token and is defined once', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('export const attachmentLotIdsSql').length - 1).toBe(1);
    const from = shared.indexOf('export const attachmentLotIdsSql');
    const rest = shared.slice(from);
    const end = rest.search(/\nexport const |\n\/\*\*/);
    const block = end === -1 ? rest : rest.slice(0, end);
    expect(selectCount(block)).toBe(1);
    expect(block).not.toMatch(lockRe);
  });

  test('former inline literals are gone and call sites compose the helper with unchanged arguments', () => {
    const text = src('attachment-read-sources.ts');
    for (const [lots, lines, lineFk, parentFk] of sites) {
      expect(text).not.toContain(`ARRAY(SELECT lot_id FROM ${lots} cl JOIN ${lines} l ON l.id=cl.${lineFk} WHERE l.${parentFk}=d.id) lot_ids`);
      expect(text).toContain(`attachmentLotIdsSql('${lots}','${lines}','${lineFk}','${parentFk}')`);
    }
    // The read-source locks, child-line owner guards and products projection
    // in the same file are untouched.
    expect(text).toContain('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE');
    expect(text).toContain('childLineOwnerExistsSql(');
    expect(text).toContain('ARRAY(SELECT product_id FROM ${table} WHERE ${foreignKey}=d.id ORDER BY product_id)');
  });

  test('r11-r37 helpers, locks, count sources/adapters, clock CTE and native registry stay untouched', () => {
    const shared = src('read-tail-sql.ts');
    for (const helper of [
      'reservedWarehouseProductQtySql', 'singleCountLineMarkerSql', 'initialPhysicalCountJournalSql',
      'childJsonAggSql', 'childJoinJsonAggSql', 'terminalReceiptMatchSql', 'reservedStockAggSql',
      'activeQualityHoldExistsSql', 'stockMoveSumSql', 'sourceScopeRowsSql', 'scopeExistsSql',
      'packageObligationDocumentRowsSql', 'evidencePositionRowsSql', 'recordIdExistsSql',
      'childLineRowsSql', 'scopeAllowedSql', 'disposalCountsSql',
      'clockScopedPurchaseExistsSql', 'lotBalanceGuardSql', 'stockPositionSourcesSql',
      'inventoryMovementEvidenceRowsSql', 'executivePositionPointSql', 'stockSourceColumnsSql',
      'attachmentOwnerVersionGuardSql',
    ]) expect(shared).toContain(`export const ${helper}`);
    // The clock CTE shared by r34 and the count-source clock stay untouched.
    expect(shared).toContain('WITH ${alias}_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms)');
    expect(src('count-record-purchases.ts')).toContain("clockCteSql('count_source_clock')");
    for (const f of [
      'initial-physical-stock-count-source-sql.ts',
      'manufacturing-initial-physical-stock-count-source-sql.ts',
      'pos-initial-physical-stock-count-source-sql.ts',
      'service-initial-physical-stock-count-source-sql.ts',
    ]) expect(src(f).length).toBeGreaterThan(0);
    expect(src('inventory-read-projections.ts')).not.toContain('SELECT status FROM inventory_counts');
    expect(readFileSync(resolve(srcRoot, 'adapters', 'native-inventory-read.ts'), 'utf8').length).toBeGreaterThan(0);
    // Attachment and maintenance read locks are untouched.
    expect(src('attachment-read-sources.ts')).toContain('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE');
    expect(src('maintenance-settlement-sources.ts')).toContain(
      'LOCK TABLE operational_inventory_consumptions,operational_inventory_consumption_lots,inventory_lots,inventory_lot_moves,commerce_products,commerce_warehouses IN SHARE MODE',
    );
    expect(shared).not.toMatch(lockRe);
  });

  test('retirement:ts-statements stays strictly below the r37 BEFORE=342 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r37 ended at 342. r38 folds three byte-identical child-lot id ARRAY
    // projections into one helper (342 -> 340): three inline SELECT tokens
    // removed, one kept in the helper. Locks, count sources/adapters, clock
    // CTE and the native registry are not touched.
    expect(total).toBeLessThanOrEqual(340);
    expect(total).toBeLessThan(342);
  });
});
