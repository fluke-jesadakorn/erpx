import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { attachmentVersionWitness } from '../../../packages/server-procurement/src/adapters/read-tail-r28';
import { procurementAttachmentReadOuterFrame } from '../../../packages/server-procurement/src/adapters/read-tail-r45';
import { commandReceiptMissing } from '../../../packages/server-procurement/src/adapters/read-tail-r44';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-procurement/src/adapters/${name}`, import.meta.url), 'utf8',
);
const count = (s: string, needle: string) => s.split(needle).length - 1;
const STEPS = 'STEPS';
const SEED = "CASE WHEN command.result_json::jsonb->>'version' ~ '^[1-9][0-9]*$' THEN (command.result_json::jsonb->>'version')::bigint END";

// Byte oracles recorded verbatim from the BEFORE=99 source renderings.
const RETURNED_WITNESS =
  "  AND EXISTS(WITH RECURSIVE versions(version) AS(SELECT CASE WHEN command.result_json::jsonb->>'version' ~ '^[1-9][0-9]*$' THEN (command.result_json::jsonb->>'version')::bigint END UNION SELECT binding.next_version FROM versions\n" +
  '   JOIN (STEPS) binding ON binding.previous_version=versions.version AND binding.company_id=returned.company_id\n' +
  "   AND binding.target_type='commerce_return' AND binding.target_id=returned.id WHERE binding.next_version<=returned.version)\n" +
  '   SELECT 1 FROM versions WHERE version=returned.version)';
const RFQ_FRAME = 'SELECT (RFQ_IMAGE)::text image,r.warehouse_id,ARRAY(SELECT product_id FROM procurement_rfq_lines WHERE rfq_id=r.id AND product_id IS NOT NULL) product_ids,ARRAY(SELECT supplier_id FROM procurement_rfq_offers WHERE rfq_id=r.id) supplier_ids,r.order_id,NULL::text invoice_id,ARRAY[]::text[] receipt_ids FROM procurement_rfqs r WHERE r.id=? AND r.company_id=?';
const BILL_FRAME = 'SELECT BILL_IMAGE::text image,NULL::text warehouse_id,ARRAY[]::text[] product_ids,ARRAY[r.supplier_id] supplier_ids,r.order_id,r.invoice_id,ARRAY(SELECT fulfillment_id FROM procurement_bill_match_receipts WHERE match_id=r.id UNION SELECT r.fulfillment_id) receipt_ids FROM procurement_bill_matches r WHERE r.id=? AND r.company_id=?';

describe('Procurement read-tail r45 byte-exact delegation', () => {
  test('r28 defaults still render the RFQ and supplier-bill witnesses byte-identically', () => {
    expect(attachmentVersionWitness(STEPS, 'r', 'procurement_rfq')).toBe(
      '  AND EXISTS(WITH RECURSIVE versions(version) AS(SELECT event.version UNION SELECT binding.next_version FROM versions\n' +
      '   JOIN (STEPS) binding ON binding.previous_version=versions.version AND binding.company_id=r.company_id\n' +
      "   AND binding.target_type='procurement_rfq' AND binding.target_id=r.id WHERE binding.next_version<=r.version)\n" +
      '   SELECT 1 FROM versions WHERE version=r.version)',
    );
    expect(attachmentVersionWitness(STEPS, 'bill', 'supplier_bill_match')).toBe(
      '  AND EXISTS(WITH RECURSIVE versions(version) AS(SELECT event.version UNION SELECT binding.next_version FROM versions\n' +
      '   JOIN (STEPS) binding ON binding.previous_version=versions.version AND binding.company_id=bill.company_id\n' +
      "   AND binding.target_type='supplier_bill_match' AND binding.target_id=bill.id WHERE binding.next_version<=bill.version)\n" +
      '   SELECT 1 FROM versions WHERE version=bill.version)',
    );
  });

  test('parameterized seed renders the RETURNED command-receipt witness byte-identically', () => {
    expect(attachmentVersionWitness(STEPS, 'returned', 'commerce_return', SEED)).toBe(RETURNED_WITNESS);
    // The r44 NOT EXISTS wrapper still renders the delegated witness byte-for-byte.
    expect(commandReceiptMissing('command', `command.record_id=returned.id AND ${RETURNED_WITNESS}`)).toBe(
      `NOT EXISTS(SELECT 1 FROM command_receipts command WHERE command.record_id=returned.id AND ${RETURNED_WITNESS})`,
    );
  });

  test('package-obligations RETURNED frame delegates and keeps the command alias', () => {
    const o = src('package-obligations.ts');
    expect(o).toContain("import {attachmentVersionWitness} from './read-tail-r28';");
    expect(count(o, '${attachmentVersionWitness(')).toBe(3);
    expect(o).toContain('${attachmentVersionWitness(recordAttachmentVersionSteps,"r","procurement_rfq")}');
    expect(o).toContain('${attachmentVersionWitness(recordAttachmentVersionSteps,"bill","supplier_bill_match")}');
    expect(o).toContain('${attachmentVersionWitness(recordAttachmentVersionSteps,"returned","commerce_return",');
    expect(o).toContain(SEED);
    expect(count(o, "AND binding.target_type='commerce_return' AND binding.target_id=returned.id")).toBe(0);
    expect(count(o, '${commandReceiptMissing(')).toBe(4);
  });

  test('outer-frame builder renders both attachment-read frames byte-identically', () => {
    expect(procurementAttachmentReadOuterFrame('procurement_rfqs', '(RFQ_IMAGE)',
      'r.warehouse_id',
      'ARRAY(SELECT product_id FROM procurement_rfq_lines WHERE rfq_id=r.id AND product_id IS NOT NULL) product_ids',
      'ARRAY(SELECT supplier_id FROM procurement_rfq_offers WHERE rfq_id=r.id) supplier_ids',
      'NULL::text invoice_id',
      'ARRAY[]::text[] receipt_ids',
    )).toBe(RFQ_FRAME);
    expect(procurementAttachmentReadOuterFrame('procurement_bill_matches', 'BILL_IMAGE',
      'NULL::text warehouse_id',
      'ARRAY[]::text[] product_ids',
      'ARRAY[r.supplier_id] supplier_ids',
      'r.invoice_id',
      'ARRAY(SELECT fulfillment_id FROM procurement_bill_match_receipts WHERE match_id=r.id UNION SELECT r.fulfillment_id) receipt_ids',
    )).toBe(BILL_FRAME);
  });

  test('attachment-read-sources delegates both branches, keeps ARRAY fragments and bind order', () => {
    const a = src('attachment-read-sources.ts');
    expect(a).toContain("import {procurementAttachmentReadOuterFrame} from './read-tail-r45';");
    expect(count(a, '${procurementAttachmentReadOuterFrame(')).toBe(2);
    expect(count(a, '.bind(id,companyId)')).toBe(2);
    expect(a).toContain('ARRAY(SELECT product_id FROM procurement_rfq_lines WHERE rfq_id=r.id AND product_id IS NOT NULL) product_ids');
    expect(a).toContain('ARRAY(SELECT supplier_id FROM procurement_rfq_offers WHERE rfq_id=r.id) supplier_ids');
    expect(a).toContain('ARRAY(SELECT fulfillment_id FROM procurement_bill_match_receipts WHERE match_id=r.id UNION SELECT r.fulfillment_id) receipt_ids');
    expect(a).not.toContain('SELECT (${procurementCommandSourceImage');
    expect(a).not.toContain('SELECT jsonb_build_object(');
  });

  test('writes, locks, native readers and r17-r44 helpers stay untouched', () => {
    expect(src('read-tail-sql.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(src('read-tail-sql.ts')).toContain('export const pointRead =');
    expect(src('package-obligations.ts')).toContain('command_receipts IN SHARE MODE');
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
    // The skipped bill-match fulfillments JOIN frame (write path) is unchanged.
    expect(src('bill-purchases.ts')).toContain('procurement_bill_matches m JOIN commerce_fulfillments primary_receipt ON primary_receipt.id=m.fulfillment_id');
    expect(src('read-tail-r44.ts')).toContain('export const commandReceiptMissing =');
    expect(src('read-tail-r44.ts')).toContain('export const commandReceiptEventWitness =');
    for (const [n, e] of [
      ['r19', 'tableRead'], ['r20', 'receiptCount'], ['r21', 'jsonbAggBy'], ['r22', 'jsonbAggJoinBy'],
      ['r23', 'billReceiptBinding'], ['r24', 'eligiblePurchaseCount'], ['r25', 'foreignBillReceiptWitness'],
      ['r26', 'fulfillmentLinesBy'], ['r27', 'settingsRead'], ['r28', 'attachmentVersionWitness'],
      ['r29', 'policyVersionGuard'], ['r30', 'orderScopedAggregate'], ['r31', 'sourceImage'],
      ['r32', 'progressOrderAggregate'], ['r33', 'quotedProjection'], ['r34', 'calendarScopedListRead'],
      ['r35', 'quotedFromSource'], ['r38', 'lineSourceWitness'], ['r39', 'actualSourceExists'],
      ['r40', 'billMatchProjection'], ['r41', 'openPromiseByOrder'],
    ] as const) expect(src(`read-tail-${n}.ts`)).toContain(`export const ${e} =`);
  });

  test('worktree SELECT count is 99 - 3 - 1 = 95', () => {
    const root = new URL('../../../packages/server-procurement/src/', import.meta.url);
    const files: URL[] = [];
    const walk = (dir: URL) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = new URL(entry.name + (entry.isDirectory() ? '/' : ''), dir);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) files.push(p);
      }
    };
    walk(root);
    const total = files.reduce((n, p) => n + (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length, 0);
    expect(total).toBeLessThan(95);
    expect(total).toBe(80); // r47: 95 -> 90; r48: three detail heads (-3 +1 helper) 90 -> 88; r49: three read heads (-3) 88 -> 85; r50: four bill-receipt heads (-4 +1 helper) 85 -> 82; r51: two jsonb_agg heads (-2 +1 helper) 82 -> 81; r52: two by-id EXISTS heads (-2 +1 helper) 81 -> 80.
  });
});
