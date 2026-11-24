import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  sourceImagesHeadSql,
  lotBalanceRowsHeadSql,
  countAuthorityScopeSql,
} from '../../../packages/server-inventory/src/adapters/read-tail-sql';
import {
  posInitialPhysicalStockSourceImages,
} from '../../../packages/server-inventory/src/adapters/pos-initial-physical-stock-count-source-sql';
import {
  manufacturingInitialPhysicalStockSourceImages,
} from '../../../packages/server-inventory/src/adapters/manufacturing-initial-physical-stock-count-source-sql';
import {
  serviceInitialPhysicalStockSourceImages,
} from '../../../packages/server-inventory/src/adapters/service-initial-physical-stock-count-source-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const src = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const adapter = (name: string) => src(`adapters/${name}`);
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');

// Verbatim pre-dedupe head fragments from HEAD, used strictly as byte-identity
// oracles (never as a live query source). Each emitted statement is identical;
// only the literal head text is shared. Callers keep their own statements,
// binds, joins and tails.
const SOURCE_HEAD = 'SELECT c.id,c.company_id,c.journal_id,';
const LOT_ROWS_HEAD = 'SELECT l.*,b.warehouse_id,b.quantity_milli,b.value_minor,b.version balance_version,';
const SCOPE_HEAD = '(SELECT id,company_id,branch_id,NULL::text department_id FROM inventory_counts)';
const SOURCE_FILES = [
  'pos-initial-physical-stock-count-source-sql.ts',
  'manufacturing-initial-physical-stock-count-source-sql.ts',
  'service-initial-physical-stock-count-source-sql.ts',
];
const SOURCE_DIGESTS: Record<string, string> = {
  pos: '5826cb4b54462f93af393af574782bd3e566875922d74f7fdd4eee9badea590c',
  mfg: 'd599c7d516eb0cfc50d0e4c324ca7a5076d1a95b1c9dfb10cdbc1f81b15fa1ae',
  service: 'c713f1027447877a9c17796684b163c7e5aaec48e94f56b3b5f0d6e551ada6f5',
};

describe('Inventory read-tail r44 head dedupe', () => {
  test('f1: the three SourceImages heads share one byte-identical head helper', () => {
    expect(sourceImagesHeadSql).toBe(SOURCE_HEAD);
    expect(selectCount(sourceImagesHeadSql)).toBe(1);
    expect(sourceImagesHeadSql).not.toMatch(lockRe);
    const shared = adapter('read-tail-sql.ts');
    expect(shared.split('export const sourceImagesHeadSql').length - 1).toBe(1);
    for (const file of SOURCE_FILES) {
      const text = adapter(file);
      expect(text).toContain('${sourceImagesHeadSql}');
      expect(text).not.toContain('`' + SOURCE_HEAD);
    }
  });

  test('f2: every rendered SourceImages statement stays byte-identical to its pre-dedupe literal', () => {
    for (const [name, sql] of Object.entries({
      pos: posInitialPhysicalStockSourceImages,
      mfg: manufacturingInitialPhysicalStockSourceImages,
      service: serviceInitialPhysicalStockSourceImages,
    })) {
      expect(sql.startsWith(SOURCE_HEAD)).toBe(true);
      expect(sha(sql), name).toBe(SOURCE_DIGESTS[name]);
      expect(selectCount(sql)).toBeGreaterThan(1); // rendered child SELECTs are unchanged
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|LOCK\s+TABLE/i);
    }
  });

  test('f3: the lot-balance joined-read head dedupes two sites byte-identically', () => {
    expect(lotBalanceRowsHeadSql).toBe(LOT_ROWS_HEAD);
    expect(selectCount(lotBalanceRowsHeadSql)).toBe(1);
    expect(lotBalanceRowsHeadSql).not.toMatch(lockRe);
    const shared = adapter('read-tail-sql.ts');
    expect(shared.split('export const lotBalanceRowsHeadSql').length - 1).toBe(1);
    for (const file of ['commerce-facts.ts', 'inventory-transfers.ts']) {
      const text = adapter(file);
      expect(text).toContain('${lotBalanceRowsHeadSql}');
      expect(text).not.toContain(LOT_ROWS_HEAD);
    }
    // Binds and tails are unchanged.
    expect(adapter('commerce-facts.ts')).toContain('facts.id,facts.warehouse_id');
    expect(adapter('inventory-transfers.ts')).toContain('transfer.source_warehouse_id,id');
  });

  test('f4: the count-file authority scope head dedupes two sites byte-identically', () => {
    expect(countAuthorityScopeSql('count_source')).toBe(`${SCOPE_HEAD} count_source`);
    expect(countAuthorityScopeSql('count_file_scope')).toBe(`${SCOPE_HEAD} count_file_scope`);
    expect(selectCount(countAuthorityScopeSql('X'))).toBe(1);
    expect(countAuthorityScopeSql('X')).not.toMatch(lockRe);
    const shared = adapter('read-tail-sql.ts');
    expect(shared.split('export const countAuthorityScopeSql').length - 1).toBe(1);
    expect(adapter('count-attachment-purchases.ts')).toContain("${countAuthorityScopeSql('count_file_scope')}");
    expect(adapter('count-record-purchases.ts')).toContain("${countAuthorityScopeSql('count_source')}");
    expect(adapter('count-attachment-purchases.ts')).not.toContain(SCOPE_HEAD);
    expect(adapter('count-record-purchases.ts')).not.toContain(SCOPE_HEAD);
  });

  test('f5: locks, native, count adapters and r11-r43 helpers stay untouched', () => {
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
      'countEvidenceRowHeadSql',
    ]) expect(shared, helper).toContain(`export const ${helper}`);
    // Protected pinned literals are unchanged.
    expect(shared).toContain('NOT EXISTS(SELECT 1 FROM inventory_lot_balances WHERE lot_id=? AND warehouse_id=?)');
    expect(shared).toContain('SELECT image FROM (${images}) source WHERE id=? AND company_id=? AND EXISTS(SELECT 1 FROM inventory_counts c WHERE c.id=source.id AND (?::text IS NULL OR c.branch_id=?))');
    expect(src('adapters/native-inventory-read.ts').length).toBeGreaterThan(0);
    expect(shared).not.toMatch(lockRe);
  });

  test('retirement:ts-statements KPI is 321 (strictly below r43 325)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r43 ended at 325. sourceImagesHeadSql folds three inline heads into one
    // (325 -> 323), lotBalanceRowsHeadSql folds two into one (323 -> 322), and
    // countAuthorityScopeSql folds two into one (322 -> 321). Locks, native
    // transport, count adapters and r11-r43 helpers stay untouched.
    // r45 folds two transit-event quantity sums into one helper (321 -> 320);
    // r46 folds three families into shared helpers (320 -> 316).
    expect(total).toBeLessThanOrEqual(316);
    expect(total).toBeLessThan(325);
  });
});
