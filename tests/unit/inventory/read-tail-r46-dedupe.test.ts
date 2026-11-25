import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  initialPhysicalCountJournalSql,
  schemaReadySql,
  posInitialPhysicalSchemaReadySql,
  mfgInitialPhysicalSchemaReadySql,
  inList,
} from '../../../packages/server-inventory/src/adapters/read-tail-sql';
import { initialPhysicalStockCountSourceRows } from '../../../packages/server-inventory/src/adapters/initial-physical-stock-count-source-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const src = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const adapter = (name: string) => src(`adapters/${name}`);
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');

// Byte oracles captured verbatim from HEAD before the r46 dedupe. They are
// identity checks only, never live query sources: the canonical helpers own
// the rendered text and every caller still prepares its own statement.
const SALES_SHA = 'da5012d7668ad77528d80a1ae445a1fe08e300a4c571c05800cd7e2c2e1909aa';
const READY_SHAS: Record<string, string> = {
  inventory_initial_physical_stock_sources: '8e6bc21c6d5f1da7cdfae0f6f95f35f6e0940d1c27dc5f94cb5054d208851416',
  inventory_pos_initial_physical_sources: '514ac56a5e239321cf73674c65a3f6808d09eaa60b6a450e0b81e47101975a74',
  inventory_mfg_initial_physical_sources: '3f2a83929f0ba5927a4a2e27614a7ac3a2336f72c33594389f3660b4f274da06',
};
const CAL_SOURCE_SHA = 'dc9aec157c00386b237b16fb2b7a245259c0b0207cddb6a6f092e94e2f4e52aa';
const CAL_TARGET_SHA = '858d713765053d43caf804bccb34fcb0b3806e695599d1b6a0ff786acf66340d';
const ATT_CANON_SHA = 'bfda3ed62569fbcf84d9d55408523646e5b22c7979d5d916d000a191286b17e8';

describe('Inventory read-tail r46 dedupe', () => {
  test('f1: Rank1 sales count journal reuses the r14 helper byte-identically with zero new binds', () => {
    const rendered = initialPhysicalCountJournalSql(initialPhysicalStockCountSourceRows, '');
    expect(sha(rendered)).toBe(SALES_SHA);
    expect(rendered).toContain("j.source_type='stock_count' AND j.source_id=c.id AND j.status='posted' AND j.is_opening=0");
    expect(selectCount(initialPhysicalCountJournalSql('ROWS', ''))).toBe(1);
    expect(initialPhysicalCountJournalSql('ROWS', '')).not.toContain('?');
    const finance = adapter('finance-record-sources.ts');
    expect(finance).toContain("const salesInitialPhysicalCountJournal=initialPhysicalCountJournalSql(initialPhysicalStockCountSourceRows,'');");
    expect(finance).not.toContain('`SELECT source.company_id,c.journal_id id FROM (${initialPhysicalStockCountSourceRows})');
    expect(finance.match(/initialPhysicalCountJournalSql/g) ?? []).toHaveLength(5); // import + 4 callers
  });

  test('f2: Rank2 schema readiness probes render from one fixed-table whitelist', () => {
    const shared = adapter('read-tail-sql.ts');
    expect(shared.split('export const schemaReadySql').length - 1).toBe(1);
    expect(shared).toContain("new Set(['inventory_initial_physical_stock_sources','inventory_pos_initial_physical_sources','inventory_mfg_initial_physical_sources'])");
    expect(selectCount(schemaReadySql('inventory_pos_initial_physical_sources'))).toBe(1);
    for (const [table, digest] of Object.entries(READY_SHAS)) {
      const rendered = schemaReadySql(table);
      expect(rendered, table).toBe(`SELECT to_regclass('${table}') IS NOT NULL ready`);
      expect(sha(rendered), table).toBe(digest);
      expect(rendered, table).not.toMatch(lockRe);
    }
    expect(posInitialPhysicalSchemaReadySql).toBe(schemaReadySql('inventory_pos_initial_physical_sources'));
    expect(mfgInitialPhysicalSchemaReadySql).toBe(schemaReadySql('inventory_mfg_initial_physical_sources'));
    expect(() => schemaReadySql('evil')).toThrow('INVENTORY_SCHEMA_READY_TABLE_NOT_ALLOWED:evil');
    const ips = adapter('initial-physical-stock-count.ts');
    expect(ips).toContain("schemaReadySql('inventory_initial_physical_stock_sources')");
    expect(ips).not.toContain("database.prepare(\"SELECT to_regclass('inventory_initial_physical_stock_sources') IS NOT NULL ready\")");
    expect(shared).not.toContain('"SELECT to_regclass(\'inventory_pos_initial_physical_sources\') IS NOT NULL ready"');
  });

  test('f3: Rank3 jsonb inList fragment is shared and the native mirror is untouched', () => {
    const shared = adapter('read-tail-sql.ts');
    expect(shared.split('export const inList').length - 1).toBe(1);
    expect(selectCount(inList('x'))).toBe(1);
    expect(inList('d.source_branch_id')).toBe('d.source_branch_id IN (SELECT jsonb_array_elements_text(?::jsonb))');
    expect(sha(inList('d.source_branch_id'))).toBe(CAL_SOURCE_SHA);
    expect(sha(inList('d.target_branch_id'))).toBe(CAL_TARGET_SHA);
    expect(sha(inList('lot.id'))).toBe(ATT_CANON_SHA);
    const cal = adapter('calendar-transit-facts.ts');
    expect(cal).not.toContain('const inList =');
    expect(cal.match(/inList\(/g) ?? []).toHaveLength(2);
    expect(cal).toContain("import {clockCteSql,inList} from './read-tail-sql'");
    const att = adapter('attachment-read-sources.ts');
    expect(att).toContain("${inList('lot.id')}");
    expect(att).not.toContain('lot.id IN(SELECT jsonb_array_elements_text(?::jsonb))');
    expect(att).toContain('.bind(companyId,JSON.stringify(native.lot_ids))');
    const native = readFileSync(resolve(srcRoot, 'adapters/native_command_sql.rs'), 'utf8');
    expect(native).toContain('l.id IN(SELECT jsonb_array_elements_text(?::jsonb))');
  });

  test('f4: locks, count sources/adapters, native registry and r11-r45 helpers stay untouched', () => {
    const shared = adapter('read-tail-sql.ts');
    for (const helper of [
      'reservedWarehouseProductQtySql', 'singleCountLineMarkerSql', 'posInitialPhysicalSchemaReadySql',
      'mfgInitialPhysicalSchemaReadySql', 'schemaReadySql', 'countMarkedExistsSql', 'countSourceProofSql',
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
      'transitEventQuantitySumSql', 'inList',
    ]) expect(shared, helper).toContain(`export const ${helper}`);
    expect(shared).toContain('NOT EXISTS(SELECT 1 FROM inventory_lot_balances WHERE lot_id=? AND warehouse_id=?)');
    expect(shared).toContain('SELECT image FROM (${images}) source WHERE id=? AND company_id=? AND EXISTS(SELECT 1 FROM inventory_counts c WHERE c.id=source.id AND (?::text IS NULL OR c.branch_id=?))');
    expect(src('adapters/native-inventory-read.ts').length).toBeGreaterThan(0);
    expect(adapter('finance-record-sources.ts')).toContain('LOCK TABLE inventory_transfers,inventory_transit,inventory_transit_lines,inventory_transit_events IN SHARE MODE');
    expect(shared).not.toMatch(lockRe);
  });

  test('retirement:ts-statements KPI is 300 after the r53 count-list head fold (strictly below r52 302)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // Rank1 (-1), Rank2 (-2), Rank3 (-1) fold into shared helpers: 320 -> 316.
    // Locks, native transport, count adapters and r11-r45 helpers stay untouched.
    // r47 folds 3 head references into 1 shared head (0 token delta), 2 marked
    // guards and 2 replay probes into one helper each (-1 each): 316 -> 314.
    // The r49 native read cutover retires 7 owner-local SELECT tokens: 2 in
    // commerce-facts (order reservations by line, lot balance by lot/warehouse)
    // and 5 in inventory-transit (event resolution subquery, posted transfer,
    // transfer by id, dispatch-lines join): 314 -> 307.
    // r50 retires 2 landed-cost detail child reads (lots join, events): 307 -> 305.
    // r51 retires 2 transit detail-lot reads (transitLots join, selectableTransitLots): 305 -> 303.
    // r52 retires 1 landed-cost detail-lines read: 303 -> 302.
    // r53 folds 3 byte-identical count-list projection heads into one helper: 302 -> 300.
    // r54 folds 3 package-obligation stock heads into one kind-parameterized renderer: 300 -> 298.
    expect(total).toBe(284);
    expect(total).toBeLessThan(299);
  });
});
