import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { packageObligationCountHeadSql, countScopeRowsSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;

// Verbatim pre-dedupe fragments from HEAD, used strictly as byte-identity
// oracles (never as a live query source). Only the marked-row source varies in
// the count-scope family; every caller keeps its own statement, binds and tail.
const HEAD = `WITH scope AS (SELECT ?::text company_id)\n    SELECT 'count' kind,c.id,c.version,c.status,c.net_value_minor amount`;
const scopeRows = (rows: string) =>
  ` c.company_id=scope.company_id AND c.status NOT IN ('posted','cancelled','rejected') AND NOT EXISTS(SELECT 1 FROM (${rows}) marker WHERE marker.id=c.id)`;
const POS_ROWS = '${posInitialPhysicalStockMarkedRows} UNION ALL ${serviceInitialPhysicalStockMarkedRows}';
const MFG_ROWS = '${manufacturingInitialPhysicalStockMarkedRows} UNION ALL ${posInitialPhysicalStockMarkedRows} UNION ALL ${serviceInitialPhysicalStockMarkedRows}';

describe('Inventory read-tail r42 package-obligation dedupe', () => {
  test('f1: the count-projection head is byte-identical to the POS and PMS pre-dedupe literals', () => {
    expect(packageObligationCountHeadSql).toBe(HEAD);
    expect(selectCount(packageObligationCountHeadSql)).toBe(2);
    expect(packageObligationCountHeadSql).not.toMatch(lockRe);
  });

  test('f2: the count-scope rows helper reproduces both pre-dedupe fragments byte-identically', () => {
    for (const rows of [POS_ROWS, MFG_ROWS]) {
      expect(countScopeRowsSql(rows)).toBe(scopeRows(rows));
      expect(selectCount(countScopeRowsSql(rows))).toBe(1);
      expect(countScopeRowsSql(rows)).not.toMatch(lockRe);
    }
    const masked = [POS_ROWS, MFG_ROWS].map((rows) => countScopeRowsSql(rows).split(rows).join('<ROWS>'));
    expect(masked[0]).toBe(masked[1]);
  });

  test('both helpers are defined once with the expected SELECT-token count', () => {
    const shared = src('read-tail-sql.ts');
    for (const [name, sels] of [['packageObligationCountHeadSql', 2], ['countScopeRowsSql', 1]] as const) {
      expect(shared.split(`export const ${name}`).length - 1).toBe(1);
      const from = shared.indexOf(`export const ${name}`);
      const rest = shared.slice(from);
      const end = rest.search(/\nexport const |\n\/\*\*/);
      const block = end === -1 ? rest : rest.slice(0, end);
      expect(selectCount(block)).toBe(sels);
      expect(block).not.toMatch(lockRe);
    }
  });

  test('former inline fragments are gone and the call sites compose the helpers with unchanged arguments', () => {
    const pos = src('pos-package-obligations.ts');
    const pms = src('pms-package-obligations.ts');
    const mfg = src('manufacturing-package-obligations.ts');
    expect(pos).not.toContain(HEAD);
    expect(pms).not.toContain(HEAD);
    expect(pos).not.toContain(scopeRows(POS_ROWS));
    expect(mfg).not.toContain(scopeRows(MFG_ROWS));
    expect(pos).toContain(
      '${packageObligationCountHeadSql},true pending FROM inventory_counts c,scope WHERE${countScopeRowsSql(`${posInitialPhysicalStockMarkedRows} UNION ALL ${serviceInitialPhysicalStockMarkedRows}`)}',
    );
    expect(pms).toContain(
      "${packageObligationCountHeadSql} FROM inventory_counts c,scope WHERE c.company_id=scope.company_id AND c.status NOT IN ('posted','cancelled','rejected')",
    );
    expect(mfg).toContain(
      'WHERE scope.full AND${countScopeRowsSql(`${manufacturingInitialPhysicalStockMarkedRows} UNION ALL ${posInitialPhysicalStockMarkedRows} UNION ALL ${serviceInitialPhysicalStockMarkedRows}`)}',
    );
  });

  test('statement tails, binds and read-only scope remain unchanged', () => {
    expect(src('pos-package-obligations.ts')).toContain("${packageObligationDocumentRowsSql(',true')}");
    expect(src('pos-package-obligations.ts')).toContain('.bind(input.companyId).all()).results');
    expect(src('pms-package-obligations.ts')).toContain("${packageObligationDocumentRowsSql('')}");
    expect(src('pms-package-obligations.ts')).toContain('.bind(input.companyId).all()).results');
    expect(src('manufacturing-package-obligations.ts')).toContain(
      '.bind(input.companyId,input.packageId==="manufacturing").all<{kind:string;id:string;fact:string;pending:boolean}>()).results',
    );
    expect(src('pos-package-obligations.ts')).toMatch(/LOCK TABLE command_receipts,pos_receipts,pos_audit,commerce_stock/);
    expect(src('pms-package-obligations.ts')).toMatch(/LOCK TABLE commerce_stock,inventory_counts,inventory_count_lines/);
    expect(src('manufacturing-package-obligations.ts')).toMatch(/LOCK TABLE command_receipts,inventory_count_lot_lines/);
  });

  test('r11-r41 helpers, count sources/adapters and native registry stay untouched', () => {
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
      'recordIdExistsSql', 'childLineRowsSql', 'scopeAllowedSql', 'disposalCountsSql',
      'clockScopedPurchaseExistsSql', 'lotBalanceGuardSql', 'stockPositionSourcesSql',
      'inventoryMovementEvidenceRowsSql', 'executivePositionPointSql', 'stockSourceColumnsSql',
      'attachmentOwnerVersionGuardSql', 'attachmentLotIdsSql', 'reservedAvailabilitySumSql',
      'decisionActorRowsSql', 'clockCteSql',
    ]) expect(shared).toContain(`export const ${helper}`);
    expect(shared).toContain('WITH ${alias}_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms)');
    for (const f of [
      'initial-physical-stock-count-source-sql.ts', 'manufacturing-initial-physical-stock-count-source-sql.ts',
      'pos-initial-physical-stock-count-source-sql.ts', 'service-initial-physical-stock-count-source-sql.ts',
    ]) expect(src(f).length).toBeGreaterThan(0);
    expect(src('inventory-read-projections.ts')).not.toContain('SELECT status FROM inventory_counts');
    expect(readFileSync(resolve(srcRoot, 'adapters', 'native-inventory-read.ts'), 'utf8').length).toBeGreaterThan(0);
    expect(shared).not.toMatch(lockRe);
  });

  test('retirement:ts-statements stays strictly below the r41 BEFORE=334 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r41 ended at 334. r42 folds the POS/PMS count-projection heads into one
    // helper (334 -> 332: four inline SELECT tokens removed, two kept) and the
    // manufacturing/POS count-scope rows into one helper (332 -> 331: two
    // inline SELECT tokens removed, one kept). Locks, count sources/adapters,
    // native transport and r11-r41 helpers are untouched.
    expect(total).toBeLessThanOrEqual(331);
    expect(total).toBeLessThan(334);
  });
});
