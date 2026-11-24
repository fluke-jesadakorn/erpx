import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { decisionActorRowsSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;

// Verbatim pre-dedupe interpolation text from HEAD, used strictly as a
// byte-identity oracle (never as a live query source). Only the decision table
// and its parent FK vary; every consumer keeps its own bind and independent
// statement.
const ACTOR = '${inventorypeople_payrollProjection.decisionActor}';
const head = (table: string, fk: string) =>
  `SELECT d.*,COALESCE(p.legal_name,d.actor_id) actor_name FROM ${table} d ${ACTOR} WHERE d.${fk}=? ORDER BY d.created_at DESC`;
const sites: [table: string, fk: string][] = [
  ['inventory_count_decisions', 'count_id'],
  ['inventory_transfer_decisions', 'transfer_id'],
];

describe('Inventory read-tail r40 dedupe', () => {
  test('f1: decisionActorRowsSql reproduces both pre-dedupe literals byte-identically', () => {
    for (const [table, fk] of sites) {
      const sql = decisionActorRowsSql(table, fk, ACTOR);
      expect(sql).toBe(head(table, fk));
      expect(selectCount(sql)).toBe(1);
      expect(sql).not.toMatch(lockRe);
    }
  });

  test('the two pre-dedupe literals share one skeleton after parameter masking', () => {
    const masked = sites.map(([table, fk]) =>
      decisionActorRowsSql(table, fk, ACTOR).split(table).join('<TABLE>').split(fk).join('<FK>'),
    );
    expect(masked[0]).toBe(masked[1]);
  });

  test('helper template carries exactly one SELECT token and is defined once', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('export const decisionActorRowsSql').length - 1).toBe(1);
    const from = shared.indexOf('export const decisionActorRowsSql');
    const rest = shared.slice(from);
    const end = rest.search(/\nexport const |\n\/\*\*/);
    const block = end === -1 ? rest : rest.slice(0, end);
    expect(selectCount(block)).toBe(1);
    expect(block).not.toMatch(lockRe);
  });

  test('former inline literals are gone and call sites compose the helper with unchanged arguments', () => {
    const counts = src('inventory-counts.ts');
    const transfers = src('inventory-transfers.ts');
    for (const [table, fk] of sites) expect(counts).not.toContain(head(table, fk));
    expect(counts).toContain(
      "decisionActorRowsSql('inventory_count_decisions','count_id',inventorypeople_payrollProjection.decisionActor)",
    );
    expect(transfers).toContain(
      "decisionActorRowsSql('inventory_transfer_decisions','transfer_id',inventorypeople_payrollProjection.decisionActor)",
    );
  });

  test('r11-r39 helpers, locks, count sources/adapters, clock CTE and native registry stay untouched', () => {
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
    ]) expect(shared).toContain(`export const ${helper}`);
    expect(shared).toContain('WITH ${alias}_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms)');
    expect(src('count-record-purchases.ts')).toContain('WITH count_source_clock AS MATERIALIZED');
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

  test('retirement:ts-statements stays strictly below the r39 BEFORE=339 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r39 ended at 339. r40 folds the stock-count and warehouse-transfer
    // decision-actor projections into one helper (339 -> 338): two inline
    // SELECT tokens removed, one kept in the helper. Locks, count
    // sources/adapters, clock CTE and native transport are not touched.
    expect(total).toBeLessThanOrEqual(338);
    expect(total).toBeLessThan(339);
  });
});
