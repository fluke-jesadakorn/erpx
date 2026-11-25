import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  commandReceiptMissing,
  commandReceiptEventWitness,
} from '../../../packages/server-procurement/src/adapters/read-tail-r44';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-procurement/src/adapters/${name}`, import.meta.url), 'utf8',
);
const count = (s: string, needle: string) => s.split(needle).length - 1;
const STEPS = 'STEPS';
const WITNESS = 'WITNESS';

// Replaced literals, recorded verbatim from the pre-r44 (BEFORE=103) source.
const ORDER = "NOT EXISTS(SELECT 1 FROM command_receipts command WHERE command.record_id=o.id AND command.operation='commerce.order.cancel' AND command.authority_json::jsonb->>'companyId'=o.company_id AND command.result_json::jsonb->>'state'='cancelled')";
const REQUESTED = "NOT EXISTS(SELECT 1 FROM command_receipts requested WHERE requested.record_id=returned.id AND requested.operation='commerce.return.request' AND requested.actor_id=returned.requested_by AND requested.authority_json::jsonb->>'companyId'=returned.company_id AND requested.result_json::jsonb->>'orderId'=returned.order_id)";
const RETURNED = `NOT EXISTS(SELECT 1 FROM command_receipts command WHERE command.record_id=returned.id
 AND command.authority_json::jsonb->>'companyId'=returned.company_id AND command.result_json::jsonb->>'orderId'=returned.order_id AND command.result_json::jsonb->>'status'=returned.status
 AND ((returned.status='rejected' AND command.operation='commerce.return.reject' AND command.actor_id=returned.approved_by AND returned.approved_by<>returned.requested_by)
 OR(returned.status='posted' AND command.operation='commerce.return.complete' AND returned.approved_by IS NOT NULL AND returned.approved_by<>returned.requested_by))
 AND EXISTS(WITH RECURSIVE versions(version) AS(SELECT CASE WHEN command.result_json::jsonb->>'version' ~ '^[1-9][0-9]*$' THEN (command.result_json::jsonb->>'version')::bigint END UNION
 SELECT binding.next_version FROM versions JOIN (${STEPS}) binding ON binding.previous_version=versions.version AND binding.company_id=returned.company_id
 AND binding.target_type='commerce_return' AND binding.target_id=returned.id WHERE binding.next_version<=returned.version) SELECT 1 FROM versions WHERE version=returned.version))`;
const PROMISE = "NOT EXISTS(SELECT 1 FROM command_receipts command WHERE command.record_id=promise.order_id AND command.result_json::jsonb->>'promiseId'=promise.id AND command.operation='procurement.promise.record' AND command.actor_id=promise.recorded_by AND command.authority_json::jsonb->>'companyId'=promise.company_id)";
const RFQ = `(SELECT 1 FROM procurement_rfq_events event JOIN command_receipts command ON event.id='rfq-event-'||command.command_id
  WHERE event.rfq_id=r.id AND event.version<=r.version AND command.authority_json::jsonb->>'companyId'=r.company_id
${WITNESS}
  AND command.actor_id=event.actor_id AND ((r.status='cancelled' AND command.record_id=r.id AND event.action='cancel' AND command.operation='procurement.rfq.cancel') OR
   (r.status='ordered' AND command.record_id=r.order_id AND event.action='order-created' AND command.operation='commerce.order.create' AND command.result_json::jsonb->>'id'=r.order_id)))`;
const BILL = `(SELECT 1 FROM procurement_bill_match_events event JOIN command_receipts command ON event.id='bill-event-'||command.command_id
  WHERE event.match_id=bill.id AND event.version<=bill.version AND command.authority_json::jsonb->>'companyId'=bill.company_id AND command.actor_id=event.actor_id
${WITNESS}
  AND ((bill.status='returned' AND command.record_id=bill.id AND event.action IN ('return','return-ap-draft') AND command.operation='procurement.bill.return') OR (bill.status='posted' AND command.record_id=bill.invoice_id AND event.action='posted' AND command.operation='finance.invoice.post')))`

describe('Procurement read-tail r44 byte-exact helpers', () => {
  test('commandReceiptMissing renders all four NOT EXISTS frames byte-identically', () => {
    expect(commandReceiptMissing('command', "command.record_id=o.id AND command.operation='commerce.order.cancel' AND command.authority_json::jsonb->>'companyId'=o.company_id AND command.result_json::jsonb->>'state'='cancelled'")).toBe(ORDER);
    expect(commandReceiptMissing('requested', "requested.record_id=returned.id AND requested.operation='commerce.return.request' AND requested.actor_id=returned.requested_by AND requested.authority_json::jsonb->>'companyId'=returned.company_id AND requested.result_json::jsonb->>'orderId'=returned.order_id")).toBe(REQUESTED);
    expect(commandReceiptMissing('command', `command.record_id=returned.id
 AND command.authority_json::jsonb->>'companyId'=returned.company_id AND command.result_json::jsonb->>'orderId'=returned.order_id AND command.result_json::jsonb->>'status'=returned.status
 AND ((returned.status='rejected' AND command.operation='commerce.return.reject' AND command.actor_id=returned.approved_by AND returned.approved_by<>returned.requested_by)
 OR(returned.status='posted' AND command.operation='commerce.return.complete' AND returned.approved_by IS NOT NULL AND returned.approved_by<>returned.requested_by))
 AND EXISTS(WITH RECURSIVE versions(version) AS(SELECT CASE WHEN command.result_json::jsonb->>'version' ~ '^[1-9][0-9]*$' THEN (command.result_json::jsonb->>'version')::bigint END UNION
 SELECT binding.next_version FROM versions JOIN (${STEPS}) binding ON binding.previous_version=versions.version AND binding.company_id=returned.company_id
 AND binding.target_type='commerce_return' AND binding.target_id=returned.id WHERE binding.next_version<=returned.version) SELECT 1 FROM versions WHERE version=returned.version)`)).toBe(RETURNED);
    expect(commandReceiptMissing('command', "command.record_id=promise.order_id AND command.result_json::jsonb->>'promiseId'=promise.id AND command.operation='procurement.promise.record' AND command.actor_id=promise.recorded_by AND command.authority_json::jsonb->>'companyId'=promise.company_id")).toBe(PROMISE);
  });

  test('commandReceiptEventWitness renders both event/receipt joins byte-identically', () => {
    expect(commandReceiptEventWitness('rfq-event-', 'r', `command.actor_id=event.actor_id AND ((r.status='cancelled' AND command.record_id=r.id AND event.action='cancel' AND command.operation='procurement.rfq.cancel') OR
   (r.status='ordered' AND command.record_id=r.order_id AND event.action='order-created' AND command.operation='commerce.order.create' AND command.result_json::jsonb->>'id'=r.order_id))`, WITNESS)).toBe(RFQ);
    expect(commandReceiptEventWitness('bill-event-', 'bill', "((bill.status='returned' AND command.record_id=bill.id AND event.action IN ('return','return-ap-draft') AND command.operation='procurement.bill.return') OR (bill.status='posted' AND command.record_id=bill.invoice_id AND event.action='posted' AND command.operation='finance.invoice.post'))", WITNESS)).toBe(BILL);
  });

  test('every call site delegates and no inline family fragment remains', () => {
    const o = src('package-obligations.ts');
    expect(count(o, '${commandReceiptMissing(')).toBe(4);
    expect(count(o, '${commandReceiptEventWitness(')).toBe(2);
    expect(o).not.toContain('NOT EXISTS(SELECT 1 FROM command_receipts ');
    expect(o).not.toContain("(SELECT 1 FROM procurement_rfq_events event JOIN command_receipts command");
    expect(o).not.toContain("(SELECT 1 FROM procurement_bill_match_events event JOIN command_receipts command");
    expect(o).toContain("import {commandReceiptMissing,commandReceiptEventWitness} from './read-tail-r44';");
    // The r28 witness literals stay at the caller, rendered into the r44 frame.
    expect(o).toContain('${attachmentVersionWitness(recordAttachmentVersionSteps,"r","procurement_rfq")}');
    expect(o).toContain('${attachmentVersionWitness(recordAttachmentVersionSteps,"bill","supplier_bill_match")}');
  });

  test('r44 defines exactly two new helpers; r17-r43 helpers, locks and writes stay untouched', () => {
    const h = src('read-tail-r44.ts');
    expect(count(h, 'export const commandReceiptMissing =')).toBe(1);
    expect(count(h, 'export const commandReceiptEventWitness =')).toBe(1);
    for (const [n, e] of [
      ['r19', 'tableRead'], ['r20', 'receiptCount'], ['r21', 'jsonbAggBy'], ['r22', 'jsonbAggJoinBy'],
      ['r23', 'billReceiptBinding'], ['r24', 'eligiblePurchaseCount'], ['r25', 'foreignBillReceiptWitness'],
      ['r26', 'fulfillmentLinesBy'], ['r27', 'settingsRead'], ['r28', 'attachmentVersionWitness'],
      ['r29', 'policyVersionGuard'], ['r30', 'orderScopedAggregate'], ['r31', 'sourceImage'],
      ['r32', 'progressOrderAggregate'], ['r33', 'quotedProjection'], ['r34', 'calendarScopedListRead'],
      ['r35', 'quotedFromSource'], ['r38', 'lineSourceWitness'], ['r39', 'actualSourceExists'],
      ['r40', 'billMatchProjection'], ['r41', 'openPromiseByOrder'],
    ] as const) expect(src(`read-tail-${n}.ts`)).toContain(`export const ${e} =`);
    expect(src('read-tail-sql.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(src('package-obligations.ts')).toContain('command_receipts IN SHARE MODE');
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
  });

  test('worktree SELECT count is 99 - 4 = 95 after r45', () => {
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
    expect(total).toBeLessThan(103);
    expect(total).toBe(81); // r44: 103->99 (F1 -3, F6 -1); r45: 99->95; r47: 95->90; r48: three detail heads (-3 +1 helper) 90->88; r49: three read heads (-3) 88->85; r50: four bill-receipt heads (-4 +1 helper) 85->82; r51: two jsonb_agg heads (-2 +1 helper) 82->81.
  });
});
