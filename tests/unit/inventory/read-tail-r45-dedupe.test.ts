import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { transitEventQuantitySumSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const src = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const adapter = (name: string) => src(`adapters/${name}`);
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');

// Verbatim pre-dedupe byte oracles from HEAD, used strictly as identity checks
// (never as a live query source). The two completed-transit guards differ only
// in the event-kind literal; both keep their own statement and binds.
const SUM_HEAD = "SELECT sum(event.quantity_milli) FROM inventory_transit_events event WHERE event.line_id=line.id AND event.kind='";
const SUM_KINDS = ['receipt', 'loss_approved'] as const;
const SUM_DIGESTS: Record<string, string> = {
  receipt: 'dd3bf714bf7fe270e7083bfb093c9159bfa1d0d15b7fa8b674047c6a26e12b96',
  loss_approved: '328e9da7a7e13b09442f5e8ca0617011ce8425c751b8124e06a9c33f3745a577',
};

describe('Inventory read-tail r45 parameterized transit-event sum dedupe', () => {
  test('f1: the parameterized helper reproduces both pre-dedupe statements byte-identically', () => {
    for (const kind of SUM_KINDS) {
      const rendered = `(${transitEventQuantitySumSql(kind)})`;
      expect(rendered, kind).toBe(`(${SUM_HEAD}${kind}')`);
      expect(sha(rendered), kind).toBe(SUM_DIGESTS[kind]);
      expect(selectCount(transitEventQuantitySumSql(kind))).toBe(1);
      expect(transitEventQuantitySumSql(kind)).not.toMatch(lockRe);
    }
  });

  test('f2: the helper owns one SELECT, and both former sites delegate to it', () => {
    const shared = adapter('read-tail-sql.ts');
    expect(shared.split('export const transitEventQuantitySumSql').length - 1).toBe(1);
    const from = shared.indexOf('export const transitEventQuantitySumSql');
    const block = shared.slice(from, shared.indexOf('\n\n', from));
    expect(selectCount(block)).toBe(1);
    expect(block).not.toMatch(lockRe);
    const site = adapter('supply-package-obligations.ts');
    for (const kind of SUM_KINDS) {
      expect(site, kind).toContain(`\${transitEventQuantitySumSql('${kind}')}`);
    }
    expect(site.split('transitEventQuantitySumSql(').length - 1).toBe(2);
    expect(site).not.toContain(`COALESCE((${SUM_HEAD}`);
  });

  test('f3: the enclosing completed-transit guard, binds and independent statements stay unchanged', () => {
    const site = adapter('supply-package-obligations.ts');
    expect(site).toContain("OR line.received_milli<>COALESCE((${transitEventQuantitySumSql('receipt')}),0)");
    expect(site).toContain("OR line.lost_milli<>COALESCE((${transitEventQuantitySumSql('loss_approved')}),0)))");
    expect(site).toContain("OR(t.status='completed' AND EXISTS(SELECT 1 FROM inventory_transit_lines line WHERE line.transit_id=t.id)");
    expect(site).toContain('AND NOT EXISTS(SELECT 1 FROM inventory_transit_events proposal WHERE proposal.transit_id=t.id AND proposal.kind=\'loss_requested\'');
    expect(site).toContain('${transitLossDecisionPendingSql(\'decision\',\'proposal.id\')}');
    expect(site).toContain('LOCK TABLE commerce_stock,commerce_warehouses,inventory_lots,inventory_lot_balances,');
  });

  test('f4: locks, count sources/adapters, native registry and r11-r44 helpers stay untouched', () => {
    const shared = adapter('read-tail-sql.ts');
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
      'decisionActorRowsSql', 'clockCteSql', 'packageObligationCountHeadSql', 'countScopeRowsSql',
      'countEvidenceRowHeadSql', 'sourceImagesHeadSql', 'lotBalanceRowsHeadSql', 'countAuthorityScopeSql',
    ]) expect(shared, helper).toContain(`export const ${helper}`);
    expect(shared).toContain('NOT EXISTS(SELECT 1 FROM inventory_lot_balances WHERE lot_id=? AND warehouse_id=?)');
    expect(shared).toContain('SELECT image FROM (${images}) source WHERE id=? AND company_id=? AND EXISTS(SELECT 1 FROM inventory_counts c WHERE c.id=source.id AND (?::text IS NULL OR c.branch_id=?))');
    expect(src('adapters/native-inventory-read.ts').length).toBeGreaterThan(0);
    expect(shared).not.toMatch(lockRe);
  });

  test('retirement:ts-statements KPI is 320 (strictly below r44 321)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r44 ended at 321. transitEventQuantitySumSql folds two inline transit
    // event quantity sums into one helper (321 -> 320). Locks, native
    // transport, count adapters and r11-r44 helpers stay untouched.
    expect(total).toBe(320);
    expect(total).toBeLessThan(321);
  });
});
