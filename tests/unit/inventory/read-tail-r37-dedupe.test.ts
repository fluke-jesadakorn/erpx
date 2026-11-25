import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { attachmentOwnerVersionGuardSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;

// Verbatim pre-dedupe literals from HEAD, used strictly as byte-identity oracles
// (never as live query sources). Only the relation and optional extra predicate
// vary; every caller still supplies its own real binds.
const headGuard = (table: string, extra = '') =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?${extra ? ` AND ${extra}` : ''})`;

describe('Inventory read-tail r37 dedupe', () => {
  test('f1: attachmentOwnerVersionGuardSql reproduces both guards byte-identically', () => {
    expect(attachmentOwnerVersionGuardSql('inventory_counts')).toBe(headGuard('inventory_counts'));
    expect(attachmentOwnerVersionGuardSql('inventory_transfers')).toBe(headGuard('inventory_transfers'));
    expect(attachmentOwnerVersionGuardSql('inventory_lots', 'status<>')).toBe(headGuard('inventory_lots', 'status<>'));
    expect(selectCount(attachmentOwnerVersionGuardSql('inventory_counts'))).toBe(1);
    expect(attachmentOwnerVersionGuardSql('inventory_counts')).not.toMatch(lockRe);
    expect(attachmentOwnerVersionGuardSql('inventory_lots', 'status<>')).not.toMatch(lockRe);
  });

  test('helper template carries exactly one SELECT token and is defined once', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('export const attachmentOwnerVersionGuardSql').length - 1).toBe(1);
    const from = shared.indexOf('export const attachmentOwnerVersionGuardSql');
    const rest = shared.slice(from);
    const end = rest.search(/\nexport const |\n\/\*\*/);
    const block = end === -1 ? rest : rest.slice(0, end);
    expect(selectCount(block)).toBe(1);
    expect(block).not.toMatch(lockRe);
  });

  test('former inline literals are gone and call sites keep unchanged binds', () => {
    const op = src('attachments-operational-facts.ts');
    expect(op).not.toContain('`EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?)`');
    expect(op).toContain('attachmentOwnerVersionGuardSql(table)');
    expect(op).toContain('[targetId, companyId, version]');
    // The operational version-step writer is untouched.
    expect(op).toContain('`UPDATE ${table} SET version=version+1,updated_at=? WHERE id=? AND version=?`');

    const svc = src('attachments-service-facts.ts');
    expect(svc).not.toContain('`EXISTS(SELECT 1 FROM ${item.table} WHERE id=? AND company_id=? AND version=?');
    expect(svc).toContain('attachmentOwnerVersionGuardSql(item.table, item.extra)');
    expect(svc).toContain('[targetId, companyId, version]');
    // The service version-step writer is untouched.
    expect(svc).toContain('`UPDATE ${descriptor.table} SET version=version+1');
  });

  test('r11-r36 helpers, locks, count sources/adapters and native registry stay untouched', () => {
    const shared = src('read-tail-sql.ts');
    for (const helper of [
      'reservedWarehouseProductQtySql', 'singleCountLineMarkerSql', 'initialPhysicalCountJournalSql',
      'childJsonAggSql', 'childJoinJsonAggSql', 'terminalReceiptMatchSql', 'reservedStockAggSql',
      'activeQualityHoldExistsSql', 'stockMoveSumSql', 'sourceScopeRowsSql', 'scopeExistsSql',
      'packageObligationDocumentRowsSql', 'evidencePositionRowsSql', 'recordIdExistsSql',
      'childLineRowsSql', 'scopeAllowedSql', 'disposalCountsSql',
      'clockScopedPurchaseExistsSql', 'lotBalanceGuardSql', 'stockPositionSourcesSql',
      'inventoryMovementEvidenceRowsSql', 'executivePositionPointSql', 'stockSourceColumnsSql',
    ]) expect(shared).toContain(`export const ${helper}`);
    for (const f of [
      'initial-physical-stock-count-source-sql.ts',
      'manufacturing-initial-physical-stock-count-source-sql.ts',
      'pos-initial-physical-stock-count-source-sql.ts',
      'service-initial-physical-stock-count-source-sql.ts',
    ]) expect(src(f).length).toBeGreaterThan(0);
    expect(src('count-record-purchases.ts')).toContain("clockCteSql('count_source_clock')");
    expect(src('inventory-read-projections.ts')).not.toContain('SELECT status FROM inventory_counts');
    expect(readFileSync(resolve(srcRoot, 'adapters', 'native-inventory-read.ts'), 'utf8').length).toBeGreaterThan(0);
    // Attachment and maintenance read locks are untouched.
    expect(src('attachment-read-sources.ts')).toContain('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE');
    expect(src('maintenance-settlement-sources.ts')).toContain(
      'LOCK TABLE operational_inventory_consumptions,operational_inventory_consumption_lots,inventory_lots,inventory_lot_moves,commerce_products,commerce_warehouses IN SHARE MODE',
    );
    expect(shared).not.toMatch(lockRe);
  });

  test('retirement:ts-statements stays strictly below the r36 BEFORE=343 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r36 ended at 343. r37 folds two byte-identical attachment owner version
    // guards into one helper (343 -> 342): the operational and service facts
    // EXISTS guards. Writer version-step statements, locks, count sources/
    // adapters and the native registry are not touched.
    expect(total).toBeLessThanOrEqual(342);
    expect(total).toBeLessThan(343);
  });
});
