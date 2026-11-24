import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { quotedProjection } from '../../../packages/server-procurement/src/adapters/read-tail-r33';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-procurement/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('Procurement read-tail r33 parameterized family dedupe', () => {
  test('quotedProjection renders the bill-match-receipts projection byte-identically', () => {
    expect(quotedProjection(
      '"id","match_id","fulfillment_id","captured_version","active"',
      'procurement_bill_match_receipts',
    )).toBe('SELECT "id","match_id","fulfillment_id","captured_version","active" FROM procurement_bill_match_receipts');
  });

  test('quotedProjection renders the bill-matches projection byte-identically', () => {
    expect(quotedProjection(
      '"id","company_id","branch_id","department_id","order_id","fulfillment_id","supplier_id","currency","chart_id","clearing_account_id","payable_account_id","tax_account_id","reference","invoice_date","due_date","evidence","expected_subtotal_minor","subtotal_minor","tax_minor","quantity_matches","variance_account_id","reviewed_by","review_reason","invoice_id","status","version","created_by","created_at","updated_at"',
      'procurement_bill_matches',
    )).toBe('SELECT "id","company_id","branch_id","department_id","order_id","fulfillment_id","supplier_id","currency","chart_id","clearing_account_id","payable_account_id","tax_account_id","reference","invoice_date","due_date","evidence","expected_subtotal_minor","subtotal_minor","tax_minor","quantity_matches","variance_account_id","reviewed_by","review_reason","invoice_id","status","version","created_by","created_at","updated_at" FROM procurement_bill_matches');
  });

  test('quotedProjection renders the request-orders projection byte-identically', () => {
    expect(quotedProjection(
      '"request_id","order_id","rfq_id","request_version","request_evidence_version","linked_by","created_at"',
      'procurement_request_orders',
    )).toBe('SELECT "request_id","order_id","rfq_id","request_version","request_evidence_version","linked_by","created_at" FROM procurement_request_orders');
  });

  test('caller uses the helper and keeps no inline quoted-projection statements', () => {
    const caller = src('commerce-final-owner-projection-sources.ts');
    expect(caller).toContain('quotedProjection(');
    expect(caller).not.toContain('SELECT \\"id\\"');
    expect(caller).not.toContain('SELECT \\"request_id\\"');
    expect(src('read-tail-r33.ts')).toContain('export const quotedProjection =');
  });

  test('no write, lock, command_receipts or r17-r32 helper was touched', () => {
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
    expect(src('read-tail-r32.ts')).toContain('export const progressOrderAggregate =');
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=122', () => {
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
    expect(total).toBeLessThan(122);
  });
});
