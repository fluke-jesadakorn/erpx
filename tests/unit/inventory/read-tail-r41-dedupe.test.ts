import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { packageClockSql } from '@erpx/server-commercial/public/purchased-capability-sql';
import { clockCteSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;

// Verbatim pre-dedupe clock-CTE fragments from HEAD, used strictly as a
// byte-identity oracle (never as a live query source). Only the alias varies;
// every consumer keeps its own statement, binds and capability/scope tail.
const head = (alias: string) => `${alias} AS MATERIALIZED (SELECT ${packageClockSql} now_ms)`;
const sites: [file: string, alias: string][] = [
  ['quality-product-entry.ts', 'count_entry_clock'],
  ['count-record-purchases.ts', 'count_source_clock'],
  ['inventory-counts.ts', 'count_list_clock'],
  ['finance-record-sources.ts', 'landed_finance_clock'],
  ['calendar-transit-facts.ts', 'transit_calendar_clock'],
];

describe('Inventory read-tail r41 clock-CTE dedupe', () => {
  test('f1: clockCteSql reproduces all five pre-dedupe literals byte-identically', () => {
    for (const [, alias] of sites) {
      const sql = clockCteSql(alias);
      expect(sql).toBe(head(alias));
      expect(selectCount(sql)).toBe(1);
      expect(sql).not.toMatch(lockRe);
    }
  });

  test('all five envelopes share one skeleton after alias masking', () => {
    const masked = sites.map(([, alias]) => clockCteSql(alias).split(alias).join('<ALIAS>'));
    for (const skeleton of masked) expect(skeleton).toBe(masked[0]);
  });

  test('helper template carries exactly one SELECT token and is defined once', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('export const clockCteSql').length - 1).toBe(1);
    const from = shared.indexOf('export const clockCteSql');
    const rest = shared.slice(from);
    const end = rest.search(/\nexport const |\n\/\*\*/);
    const block = end === -1 ? rest : rest.slice(0, end);
    expect(selectCount(block)).toBe(1);
    expect(block).not.toMatch(lockRe);
  });

  test('former inline literals are gone and all five call sites compose the helper with unchanged alias', () => {
    for (const [file, alias] of sites) {
      const text = src(file);
      expect(text).not.toContain(head(alias));
      expect(text).toContain(`clockCteSql('${alias}')`);
    }
    // The four sites whose only packageClockSql use was the folded clock CTE
    // drop the now-unused import; count-record-purchases keeps its second CTE.
    for (const file of ['quality-product-entry.ts', 'inventory-counts.ts', 'finance-record-sources.ts', 'calendar-transit-facts.ts']) {
      expect(src(file)).not.toContain('packageClockSql');
    }
    expect(src('count-record-purchases.ts')).toContain('packageClockSql');
    expect(src('count-record-purchases.ts')).toContain('WITH initial_destination_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms)');
  });

  test('binds and statement tails at each site are unchanged', () => {
    expect(src('quality-product-entry.ts')).toContain(
      '.bind(id,scope.companyId,scope.branchId,scope.branchId,scope.assignmentId,scope.userId,scope.role,scope.assignmentId,scope.assignmentVersion)',
    );
    expect(src('count-record-purchases.ts')).toContain(
      'args:[actor.branchId,actor.branchId,actor.assignmentId,actor.userId,actor.role]',
    );
    expect(src('inventory-counts.ts')).toContain('.bind(actor.companyId,actor.branchId,actor.branchId)');
    expect(src('finance-record-sources.ts')).toContain(
      'journal:`${countJournal} UNION ${transitLossJournal} UNION (${landedCostJournal})`',
    );
    expect(src('calendar-transit-facts.ts')).toContain('...(enforced?[actor.companyId,actor.branchId]:[])');
    expect(src('calendar-transit-facts.ts')).toContain('...(enforced?[actor.assignmentId,actor.userId,actor.role]:[])');
  });

  test('r11-r40 helpers, locks, count sources/adapters and native registry stay untouched', () => {
    const shared = src('read-tail-sql.ts');
    for (const helper of [
      'reservedWarehouseProductQtySql', 'singleCountLineMarkerSql', 'posInitialPhysicalSchemaReadySql',
      'mfgInitialPhysicalSchemaReadySql', 'countMarkedExistsSql', 'countSourceProofSql',
      'countCreateMarkedRowsSql', 'countMarkedJournalSql', 'allInitialPhysicalStockMarkedRowsSql',
      'countInitialMarkedExistsSql', 'initialPhysicalCountJournalSql', 'initialPhysicalCountFinanceImagesSql',
      'childJsonAggSql', 'childJoinJsonAggSql', 'terminalReceiptMatchSql', 'transitLossDecisionPendingSql',
      'reservedStockAggSql', 'qualityHeldExistsSql', 'initialPhysicalCountImageSql',
      'activeQualityHoldExistsSql', 'pendingLossSumSql', 'stockMoveSumSql',
      'countInitialPhysicalSourceMarkersSql', 'postedInitialPhysicalStockSourceSql', 'sourceRowsExistsSql',
      'attachmentSourceProofExistsSql', 'sourceExistsSql', 'journalLineSumSql', 'childLineOwnerExistsSql',
      'nativeRecordExistsSql', 'branchScopeTupleSql', 'scopeExistsSql', 'journalAccountLineExistsSql',
      'sourceScopeRowsSql', 'packageObligationDocumentRowsSql', 'evidencePositionRowsSql',
      'recordIdExistsSql', 'childLineRowsSql', 'scopeAllowedSql', 'disposalCountsSql', 'scopedFieldSql',
      'clockScopedPurchaseExistsSql', 'lotBalanceGuardSql', 'stockPositionSourcesSql',
      'inventoryMovementEvidenceRowsSql', 'executivePositionPointSql', 'stockSourceColumnsSql',
      'attachmentOwnerVersionGuardSql', 'attachmentLotIdsSql', 'reservedAvailabilitySumSql',
      'decisionActorRowsSql',
    ]) expect(shared).toContain(`export const ${helper}`);
    expect(shared).toContain('WITH ${alias}_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms)');
    for (const f of [
      'initial-physical-stock-count-source-sql.ts',
      'manufacturing-initial-physical-stock-count-source-sql.ts',
      'pos-initial-physical-stock-count-source-sql.ts',
      'service-initial-physical-stock-count-source-sql.ts',
    ]) expect(src(f).length).toBeGreaterThan(0);
    expect(src('inventory-read-projections.ts')).toContain(
      'export const inventoryCountStatusSql = "SELECT status FROM inventory_counts WHERE id=? AND company_id=?"',
    );
    expect(readFileSync(resolve(srcRoot, 'adapters', 'native-inventory-read.ts'), 'utf8').length).toBeGreaterThan(0);
    expect(src('transfer-attachment-purchases.ts')).toContain('FOR SHARE');
    expect(src('count-attachment-purchases.ts')).toContain('FOR SHARE');
    expect(shared).not.toMatch(lockRe);
  });

  test('retirement:ts-statements stays strictly below the r40 BEFORE=338 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r40 ended at 338. r41 folds the five inline clock CTEs into one helper
    // (338 -> 334): five inline SELECT tokens removed, one kept in the helper.
    // Locks, count sources/adapters, native transport and r11-r40 are untouched.
    expect(total).toBeLessThanOrEqual(334);
    expect(total).toBeLessThan(338);
  });
});
