import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  reservationsCteHeadSql,
  markedCountExistsSql,
  replayProbePrefixSql,
  reservedWarehouseProductQtySql,
} from '../../../packages/server-inventory/src/adapters/read-tail-sql';
import { inventoryPurchaseReplaySql, inventoryCommandReplaySql } from '../../../packages/server-inventory/src/adapters/inventory-read-projections';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const src = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const adapter = (name: string) => src(`adapters/${name}`);
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');
const tailsAfter = (text: string, marker: string) => {
  const out: string[] = [];
  let i = 0;
  while (true) {
    const at = text.indexOf(marker, i);
    if (at < 0) break;
    const start = at + marker.length;
    out.push(text.slice(start, text.indexOf('`;', start)));
    i = start;
  }
  return out;
};

// Verbatim SHA oracles captured from the pre-r47 source. They are identity
// checks only; the canonical helpers own the rendered text and every caller
// still prepares its own statement with its own binds.
const HEAD_METRIC_SHA = '2cd9dead701c4ddea85cc336c86a005bec08fd6b7f1a135f81ba16d2c291ce44';
const HEAD_EVIDENCE_SHA = '9fe6b6e5aab841bc1bf50667cfc09b3c4cd8526cab394307449baf4526c83d2b';
const HEAD_SHARED_SHA = 'eb1f7e1e3241b2fa45a95c84d7fb7009121d68722fab26a4a87ac9272209a8bd';
const TAIL_AVAILABLE_SHA = '4e02903e3fe1dd29ee2ce447101de02846b873fe29588370f47c6a53ce3a6e4c';
const TAIL_SHORTAGE_SHA = '19cfd90e4c45385bde88c326b40840d34a28bab0b7e5669799da2859e4fb3c83';
const TAIL_EVIDENCE_SHA = 'c700727f5d1ca681f441a3b5dd6ca3b6fe5dc1c28d433331e54f5b791a5e411c';
const MARKED_MFG_SHA = '33a65d51ee5a65e1e7ffb5f7ceda9caa67b0a8a1dafba838255303759d667ce0';
const MARKED_POS_SHA = '5929d7b5b9c20b4bf6abbc122bcec5253f5fe7081c5eff0610ca05a79a0d5823';
const MARKED_RENDERED_SHA = '4dd0f231652956c7e3a1fe8179318e15d8f453ccfa6d4e97f0fd4f7b2e3692d2';
const REPLAY_PREFIX_SHA = '0d3af8196e3cd76ddf478b2fdd254f5403670ef84295564111360b4ad5a6adc3';
const PURCHASE_REPLAY_SHA = '58c5e07caec8ad8d4cd9f965735c4d6a0e425dd60f57532d5984cc5362e8f27f';
const COMMAND_REPLAY_SHA = '7fd7a3cfb5c80b2423873745a84487b3021f64f4651a879334a49b1135561c7d';

const HEAD_METRIC = `WITH reservations AS (\n        ${reservedWarehouseProductQtySql}\n        FROM inventory_active_reservations GROUP BY warehouse_id,product_id\n      )`;
const HEAD_EVIDENCE = `WITH reservations AS (\n    ${reservedWarehouseProductQtySql}\n    FROM inventory_active_reservations GROUP BY warehouse_id,product_id\n  )`;
const HEAD_SHARED = `WITH reservations AS (\n  ${reservedWarehouseProductQtySql}\n  FROM inventory_active_reservations GROUP BY warehouse_id,product_id\n  )`;
const MARKED_MFG = 'c.company_id=scope.company_id AND EXISTS(SELECT 1 FROM (${manufacturingInitialPhysicalStockMarkedRows}) marker WHERE marker.id=c.id)';
const MARKED_POS = 'c.company_id=scope.company_id AND EXISTS(SELECT 1 FROM (${posInitialPhysicalStockMarkedRows}) marker WHERE marker.id=c.id)';

describe('Inventory read-tail r47 dedupe', () => {
  test('f1: Rank1 reservations CTE head is shared with zero binds and byte-identical tails', () => {
    expect(reservationsCteHeadSql).toBe(HEAD_SHARED);
    expect(sha(reservationsCteHeadSql)).toBe(HEAD_SHARED_SHA);
    expect(selectCount(reservationsCteHeadSql)).toBe(1);
    expect(reservationsCteHeadSql).not.toContain('?');
    expect(reservationsCteHeadSql).not.toMatch(lockRe);
    // The two pre-dedupe head spellings differed only in indentation; both are
    // preserved as byte oracles, and neither is reused as a live query source.
    expect(sha(HEAD_METRIC)).toBe(HEAD_METRIC_SHA);
    expect(sha(HEAD_EVIDENCE)).toBe(HEAD_EVIDENCE_SHA);
    expect(HEAD_METRIC_SHA).not.toBe(HEAD_EVIDENCE_SHA);

    const em = adapter('executive-metric-projections.ts');
    const ee = adapter('executive-main-evidence-sources.ts');
    expect(em.split('${reservationsCteHeadSql}').length - 1).toBe(2);
    expect(ee.split('${reservationsCteHeadSql}').length - 1).toBe(1);
    expect(em).not.toContain('WITH reservations AS (');
    expect(ee).not.toContain('WITH reservations AS (');
    expect(em).not.toContain('reservedWarehouseProductQtySql');
    expect(ee).not.toContain('reservedWarehouseProductQtySql');
    const metricTails = tailsAfter(em, '${reservationsCteHeadSql}');
    expect(metricTails.map(sha)).toEqual([TAIL_AVAILABLE_SHA, TAIL_SHORTAGE_SHA]);
    expect(sha(tailsAfter(ee, '${reservationsCteHeadSql}')[0])).toBe(TAIL_EVIDENCE_SHA);
    // Binds stayed in their original order and arity.
    expect(em.split('values = [a.companyId, a.branchId, a.branchId]').length - 1).toBe(2);
    expect(ee).toContain('.bind(actor.companyId, actor.branchId, actor.branchId, ids.warehouseId, ids.productId)');
  });

  test('f2: Rank2 positive marked-count EXISTS guard keeps the scope.full prefix outside', () => {
    expect(markedCountExistsSql('<ROWS>')).toBe('c.company_id=scope.company_id AND EXISTS(SELECT 1 FROM (<ROWS>) marker WHERE marker.id=c.id)');
    expect(sha(markedCountExistsSql('<ROWS>'))).toBe(MARKED_RENDERED_SHA);
    expect(sha(markedCountExistsSql('MFG_ROWS'))).toBe('f4498b332e8314858743fa8b74e9463f5a97628e31b8a04832b040fbad258fff');
    expect(selectCount(markedCountExistsSql('ROWS'))).toBe(1);
    expect(markedCountExistsSql('ROWS')).not.toContain('?');
    expect(markedCountExistsSql('ROWS')).not.toMatch(lockRe);
    // Pre-dedupe literals stay as oracles only.
    expect(sha(MARKED_MFG)).toBe(MARKED_MFG_SHA);
    expect(sha(MARKED_POS)).toBe(MARKED_POS_SHA);

    const mfg = adapter('manufacturing-package-obligations.ts');
    const pos = adapter('pos-package-obligations.ts');
    expect(mfg).not.toContain(MARKED_MFG);
    expect(pos).not.toContain(MARKED_POS);
    expect(mfg).toContain('WHERE scope.full AND ${markedCountExistsSql(manufacturingInitialPhysicalStockMarkedRows)}');
    expect(pos).toContain('WHERE ${markedCountExistsSql(posInitialPhysicalStockMarkedRows)}');
    expect(pos).not.toContain('WHERE scope.full AND ${markedCountExistsSql');
    expect(mfg).toContain('.bind(input.companyId,input.packageId==="manufacturing").all<{kind:string;id:string;fact:string;pending:boolean}>()).results');
    expect(pos).toContain('.bind(input.companyId).all()).results');
  });

  test('f3: Rank3 replay probe prefix preserves the record_id distinction and bind order', () => {
    expect(replayProbePrefixSql).toBe('SELECT 1 FROM command_receipts WHERE command_id=? AND operation=?');
    expect(sha(replayProbePrefixSql)).toBe(REPLAY_PREFIX_SHA);
    expect(selectCount(replayProbePrefixSql)).toBe(1);
    expect(replayProbePrefixSql).not.toMatch(lockRe);
    // Rendered probes stay byte-identical to HEAD: only the literal source is shared.
    expect(sha(inventoryPurchaseReplaySql)).toBe(PURCHASE_REPLAY_SHA);
    expect(sha(inventoryCommandReplaySql)).toBe(COMMAND_REPLAY_SHA);
    expect(inventoryPurchaseReplaySql).toBe(`${replayProbePrefixSql} AND record_id=? AND actor_id=? AND authority_json::jsonb->>'companyId'=?`);
    expect(inventoryCommandReplaySql).toBe(`${replayProbePrefixSql} AND actor_id=? AND authority_json::jsonb->>'companyId'=?`);
    expect(inventoryPurchaseReplaySql).toContain(' AND record_id=? AND actor_id=?');
    expect(inventoryCommandReplaySql).not.toContain('record_id');

    const ir = adapter('inventory-read-projections.ts');
    expect(ir.split('${replayProbePrefixSql}').length - 1).toBe(2);
    expect(ir).not.toContain('= "SELECT 1 FROM command_receipts WHERE command_id=? AND operation=?');
    expect(ir).toContain("import {activeQualityHoldExistsSql,replayProbePrefixSql} from './read-tail-sql'");
  });

  test('f4: r11-r46 helpers, locks, count sources/adapters and native registry stay untouched', () => {
    const shared = adapter('read-tail-sql.ts');
    for (const helper of [
      'reservedWarehouseProductQtySql', 'singleCountLineMarkerSql', 'initialPhysicalCountJournalSql',
      'countMarkedExistsSql', 'countSourceProofSql', 'countCreateMarkedRowsSql', 'countMarkedJournalSql',
      'allInitialPhysicalStockMarkedRowsSql', 'countInitialMarkedExistsSql', 'childJsonAggSql',
      'childJoinJsonAggSql', 'terminalReceiptMatchSql', 'transitLossDecisionPendingSql', 'reservedStockAggSql',
      'qualityHeldExistsSql', 'initialPhysicalCountImageSql', 'activeQualityHoldExistsSql', 'pendingLossSumSql',
      'stockMoveSumSql', 'countInitialPhysicalSourceMarkersSql', 'postedInitialPhysicalStockSourceSql',
      'sourceRowsExistsSql', 'attachmentSourceProofExistsSql', 'sourceExistsSql', 'journalLineSumSql',
      'childLineOwnerExistsSql', 'nativeRecordExistsSql', 'branchScopeTupleSql', 'scopeExistsSql',
      'journalAccountLineExistsSql', 'sourceScopeRowsSql', 'packageObligationDocumentRowsSql',
      'evidencePositionRowsSql', 'recordIdExistsSql', 'childLineRowsSql', 'scopeAllowedSql', 'disposalCountsSql',
      'scopedFieldSql', 'clockScopedPurchaseExistsSql', 'lotBalanceGuardSql', 'stockPositionSourcesSql',
      'inventoryMovementEvidenceRowsSql', 'executivePositionPointSql', 'stockSourceColumnsSql',
      'attachmentOwnerVersionGuardSql', 'attachmentLotIdsSql', 'reservedAvailabilitySumSql',
      'decisionActorRowsSql', 'clockCteSql', 'packageObligationCountHeadSql', 'countScopeRowsSql',
      'countEvidenceRowHeadSql', 'sourceImagesHeadSql', 'lotBalanceRowsHeadSql', 'countAuthorityScopeSql',
      'transitEventQuantitySumSql', 'inList', 'schemaReadySql', 'posInitialPhysicalSchemaReadySql',
      'mfgInitialPhysicalSchemaReadySql',
    ]) expect(shared, helper).toContain(`export const ${helper}`);
    for (const helper of ['reservationsCteHeadSql', 'markedCountExistsSql', 'replayProbePrefixSql'])
      expect(shared.split(`export const ${helper}`).length - 1).toBe(1);
    expect(shared).not.toMatch(lockRe);
    expect(src('adapters/manufacturing-package-obligations.ts')).toContain('LOCK TABLE command_receipts,inventory_count_lot_lines');
    expect(src('adapters/pos-package-obligations.ts')).toContain('LOCK TABLE command_receipts,pos_receipts');
    expect(src('adapters/native-inventory-read.ts').length).toBeGreaterThan(0);
    for (const f of [
      'initial-physical-stock-count-source-sql.ts', 'manufacturing-initial-physical-stock-count-source-sql.ts',
      'pos-initial-physical-stock-count-source-sql.ts', 'service-initial-physical-stock-count-source-sql.ts',
    ]) expect(adapter(f).length).toBeGreaterThan(0);
  });

  test('retirement:ts-statements KPI is 303 after the r51 transit detail-lot read cutover (strictly below r50 305)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // 3 inline head copies -> 1 shared head, 2 marked guards -> 1 helper and
    // 2 replay probes -> 1 prefix: four duplicate SQL copies removed. The two
    // metric heads referenced reservedWarehouseProductQtySql, so Rank1 removes
    // 0 source SELECT tokens (3 references -> 1 reference); Rank2 and Rank3
    // each remove 1 literal token while adding 1 helper token: 316 -> 314.
    // The r49 native read cutover retires 7 owner-local SELECT tokens (2
    // commerce-facts, 5 inventory-transit): 314 -> 307.
    // r50 retires 2 landed-cost detail child reads (lots join, events): 307 -> 305.
    // r51 retires 2 transit detail-lot reads (transitLots join, selectableTransitLots): 305 -> 303.
    expect(total).toBe(303);
    expect(total).toBeLessThan(305);
  });
});
