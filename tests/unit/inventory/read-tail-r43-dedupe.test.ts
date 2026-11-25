import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { countEvidenceRowHeadSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';
import {
  serviceCountMarkerPredicate,
  serviceCountOutboxEligible,
  serviceCountOutboxQuarantine,
  serviceCountOutboxAmbiguous,
} from '../../../packages/server-inventory/src/public/service-count-outbox-predicate.generated';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const src = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const adapter = (name: string) => src(`adapters/${name}`);
const pub = (name: string) => src(`public/${name}`);
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;

// Verbatim HEAD prefix for the five evidence-row projections. Only the tail
// varies; every emitted statement stays byte-identical to its original.
const HEAD_HEAD = 'SELECT c.id,c.company_id,c.branch_id,c.warehouse_id,';
const HEAD_TAILS = [
  `line.product_id,receipt.command_id source_command_id,receipt.result_json::jsonb->'\${initialPhysicalStockMarker}' source_proof`,
  `line.product_id,receipt.command_id source_command_id,receipt.result_json::jsonb->'\${manufacturingInitialPhysicalStockMarker}' source_proof`,
  `line.product_id,receipt.command_id source_command_id,receipt.result_json::jsonb->'\${posInitialPhysicalStockMarker}' source_proof`,
  `line.product_id,receipt.command_id source_command_id,receipt.result_json::jsonb->'\${serviceInitialPhysicalStockMarker}' source_proof`,
  `c.quality_hold_id,source.kind,source.stock_capability,source.finance_capability,source.witness source_witness,receipt.command_id source_command_id`,
];
const HEAD_FILES = [
  'initial-physical-stock-count-source-sql.ts',
  'manufacturing-initial-physical-stock-count-source-sql.ts',
  'pos-initial-physical-stock-count-source-sql.ts',
  'service-initial-physical-stock-count-source-sql.ts',
  'quality-count-purchases.ts',
];

describe('Inventory read-tail r43 dedupe', () => {
  test('f1: the shared evidence-row head reproduces all five HEAD projections byte-identically', () => {
    for (const tail of HEAD_TAILS) {
      expect(countEvidenceRowHeadSql(tail)).toBe(HEAD_HEAD + tail);
      expect(selectCount(countEvidenceRowHeadSql(tail))).toBe(1);
      expect(countEvidenceRowHeadSql(tail)).not.toMatch(lockRe);
    }
  });

  test('f2: the head helper owns one SELECT and no lock, and every former site delegates to it', () => {
    const shared = adapter('read-tail-sql.ts');
    expect(shared.split('export const countEvidenceRowHeadSql').length - 1).toBe(1);
    const from = shared.indexOf('export const countEvidenceRowHeadSql');
    const block = shared.slice(from, shared.indexOf('\n\n', from));
    expect(selectCount(block)).toBe(1);
    expect(block).not.toMatch(lockRe);
    for (const file of HEAD_FILES) {
      const text = adapter(file);
      expect(text).toContain('countEvidenceRowHeadSql(');
      expect(text).not.toContain('`' + HEAD_HEAD);
    }
  });

  test('f3: statement tails, binds and independent statements are unchanged', () => {
    const source = adapter('service-initial-physical-stock-count-source.ts');
    expect(source).toContain('nativeServiceCountPersistedMarker');
    expect(source).not.toContain('.bind(id,id).first<{marked:boolean}>()');
    expect(source).toContain('.bind(id,scope.companyId,scope.branchId,scope.branchId).first<string>');
    expect(adapter('quality-count-purchases.ts')).toContain(
      '.bind(id,scope.companyId,scope.branchId,scope.branchId).first<{id:string;quality_hold_id:string',
    );
    expect(adapter('count-record-purchases.ts')).toContain('initialPhysicalStockCountDestinations');
  });

  test('f4: the service-count marker is generated once and every predicate stays byte-identical', () => {
    expect(serviceCountOutboxEligible).toBe(pub('service_count_outbox_eligible.sql'));
    expect(serviceCountOutboxQuarantine).toBe(pub('service_count_outbox_quarantine.sql'));
    expect(serviceCountOutboxAmbiguous).toBe(pub('service_count_outbox_ambiguous.sql'));
    expect(serviceCountMarkerPredicate).toContain('_serviceInitialPhysicalStockCount');
    expect(selectCount(serviceCountMarkerPredicate)).toBe(2);
    expect(serviceCountOutboxQuarantine).toContain(serviceCountMarkerPredicate);
    expect(serviceCountOutboxAmbiguous).toContain(serviceCountMarkerPredicate);
    const generated = pub('service-count-outbox-predicate.generated.ts');
    expect(generated.split('export const serviceCountMarkerPredicate').length - 1).toBe(1);
    expect(generated).toContain('${serviceCountMarkerPredicate}');
    expect(generated.split('${serviceCountMarkerPredicate}').length - 1).toBe(2);
    const generator = readFileSync(
      resolve(__dirname, '../../../scripts/docs/generate-service-count-outbox-predicate.ts'),
      'utf8',
    );
    expect(generator).toContain("SERVICE_COUNT_MARKER_DRIFT");
    expect(generator).toContain('SERVICE_COUNT_OUTBOX_PREDICATE_DRIFT');
  });

  test('f5: locks, count sources/adapters, native registry and r11-r42 helpers stay untouched', () => {
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
      'clockScopedPurchaseExistsSql', 'stockPositionSourcesSql', 'inventoryMovementEvidenceRowsSql',
      'executivePositionPointSql', 'stockSourceColumnsSql', 'attachmentOwnerVersionGuardSql',
      'attachmentLotIdsSql', 'reservedAvailabilitySumSql', 'decisionActorRowsSql', 'clockCteSql',
      'packageObligationCountHeadSql', 'countScopeRowsSql',
    ]) expect(shared).toContain(`export const ${helper}`);
    for (const file of HEAD_FILES) expect(adapter(file)).toContain('JOIN command_receipts receipt');
    expect(src('adapters/native-inventory-read.ts').length).toBeGreaterThan(0);
    expect(shared).not.toMatch(lockRe);
  });

  test('retirement:ts-statements KPI is at or below r44 321 (r43 was 325)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r42 ended at 331. Family A folds the five evidence-row heads into one
    // helper (331 -> 327: five inline SELECT tokens removed, one kept). Family
    // B generates the service-count marker once (327 -> 325: two inline SELECT
    // tokens removed, none added to the generated exports). Locks, count
    // sources/adapters, native transport and r11-r42 helpers stay untouched.
    expect(total).toBeLessThanOrEqual(321);
    expect(total).toBeLessThan(331);
  });
});
