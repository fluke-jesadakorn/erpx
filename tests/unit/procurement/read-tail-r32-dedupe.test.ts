import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { requestOrderLink } from '../../../modules/procurement/server/src/adapters/read-tail-r19';
import { progressOrderAggregate } from '../../../modules/procurement/server/src/adapters/read-tail-r32';

const src = (name: string) => readFileSync(
  new URL(`../../../modules/procurement/server/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('Procurement read-tail r32 parameterized family dedupe', () => {
  test('progressOrderAggregate renders the original request-progress bill aggregate byte-identically', () => {
    expect(progressOrderAggregate(
      `COUNT(*) bill_count,
             COUNT(*) FILTER (WHERE m.status='posted') posted_count,
             COUNT(m.invoice_id) invoice_count,
             COUNT(*) FILTER (WHERE i.status='paid' OR COALESCE(i.outstanding_minor,0)=0) paid_count,
             (SELECT COUNT(DISTINCT br.fulfillment_id)
                FROM procurement_bill_match_receipts br
                JOIN procurement_bill_matches covered ON covered.id=br.match_id
               JOIN (O) covered_order ON covered_order.id=covered.order_id
               WHERE covered.company_id=? AND (covered_order.id=? OR ${requestOrderLink("link","covered_order")}) AND covered.status<>'returned' AND br.active=TRUE) matched_receipt_count`,
      `procurement_bill_matches m
           LEFT JOIN (I) i ON i.id=m.invoice_id
           JOIN (O) matched_order ON matched_order.id=m.order_id`,
      'm', 'matched_order', "m.status<>'returned'", requestOrderLink("link","matched_order"),
    )).toBe(`SELECT COUNT(*) bill_count,
             COUNT(*) FILTER (WHERE m.status='posted') posted_count,
             COUNT(m.invoice_id) invoice_count,
             COUNT(*) FILTER (WHERE i.status='paid' OR COALESCE(i.outstanding_minor,0)=0) paid_count,
             (SELECT COUNT(DISTINCT br.fulfillment_id)
                FROM procurement_bill_match_receipts br
                JOIN procurement_bill_matches covered ON covered.id=br.match_id
               JOIN (O) covered_order ON covered_order.id=covered.order_id
               WHERE covered.company_id=? AND (covered_order.id=? OR EXISTS(SELECT 1 FROM procurement_request_orders link WHERE link.request_id=? AND link.order_id=covered_order.id)) AND covered.status<>'returned' AND br.active=TRUE) matched_receipt_count
           FROM procurement_bill_matches m
           LEFT JOIN (I) i ON i.id=m.invoice_id
           JOIN (O) matched_order ON matched_order.id=m.order_id
           WHERE m.company_id=? AND (matched_order.id=? OR EXISTS(SELECT 1 FROM procurement_request_orders link WHERE link.request_id=? AND link.order_id=matched_order.id)) AND m.status<>'returned'`);
  });

  test('progressOrderAggregate renders the original request-progress payment aggregate byte-identically', () => {
    expect(progressOrderAggregate(
      `COUNT(*) instruction_count,
             COUNT(*) FILTER (WHERE s.status='confirmed') confirmed_count,
             MIN(s.batch_id) batch_id`,
      `(S) s
           JOIN (I) i ON i.id=s.invoice_id
           JOIN procurement_bill_matches m ON m.invoice_id=i.id
           JOIN (O) paid_order ON paid_order.id=m.order_id`,
      'm', 'paid_order', "s.status<>'cancelled'", requestOrderLink("link","paid_order"),
    )).toBe(`SELECT COUNT(*) instruction_count,
             COUNT(*) FILTER (WHERE s.status='confirmed') confirmed_count,
             MIN(s.batch_id) batch_id
           FROM (S) s
           JOIN (I) i ON i.id=s.invoice_id
           JOIN procurement_bill_matches m ON m.invoice_id=i.id
           JOIN (O) paid_order ON paid_order.id=m.order_id
           WHERE m.company_id=? AND (paid_order.id=? OR EXISTS(SELECT 1 FROM procurement_request_orders link WHERE link.request_id=? AND link.order_id=paid_order.id)) AND s.status<>'cancelled'`);
  });

  test('caller uses the helper and keeps no inline family statements', () => {
    expect(src('purchase-request-progress.ts')).toContain('progressOrderAggregate(');
    expect(src('purchase-request-progress.ts')).not.toContain('SELECT COUNT(*) bill_count,');
    expect(src('purchase-request-progress.ts')).not.toContain('SELECT COUNT(*) instruction_count,');
    expect(src('read-tail-r32.ts')).toContain('export const progressOrderAggregate =');
  });

  test('no write, lock, command_receipts or r17-r31 helper was touched', () => {
    expect(src('read-tail-sql.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(src('read-tail-sql.ts')).toContain('export const pointRead =');
    expect(src('read-tail-sql.ts')).toContain('export const orderedRead =');
    expect(src('read-tail-r19.ts')).toContain('export const tableRead =');
    expect(src('read-tail-r20.ts')).toContain('export const receiptCount =');
    expect(src('read-tail-r21.ts')).toContain('export const jsonbAggBy =');
    expect(src('read-tail-r22.ts')).toContain('export const jsonbAggJoinBy =');
    expect(src('read-tail-r23.ts')).toContain('export const billReceiptBinding =');
    expect(src('read-tail-r24.ts')).toContain('export const eligiblePurchaseCount =');
    expect(src('read-tail-r25.ts')).toContain('export const foreignBillReceiptWitness =');
    expect(src('read-tail-r26.ts')).toContain('export const fulfillmentLinesBy =');
    expect(src('read-tail-r27.ts')).toContain('export const settingsRead =');
    expect(src('read-tail-r28.ts')).toContain('export const attachmentVersionWitness =');
    expect(src('read-tail-r29.ts')).toContain('export const policyVersionGuard =');
    expect(src('read-tail-r30.ts')).toContain('export const orderScopedAggregate =');
    expect(src('read-tail-r31.ts')).toContain('export const sourceImage =');
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=123', () => {
    const root = new URL('../../../modules/procurement/server/src/', import.meta.url);
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
    expect(total).toBeLessThan(123);
  });
});
