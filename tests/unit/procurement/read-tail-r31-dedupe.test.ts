import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { receiptImage } from '../../../packages/server-procurement/src/adapters/read-tail-r19';
import { branchDepartmentScope } from '../../../packages/server-procurement/src/adapters/read-tail-sql';
import { billReceiptBinding } from '../../../packages/server-procurement/src/adapters/read-tail-r23';
import { foreignBillReceiptWitness } from '../../../packages/server-procurement/src/adapters/read-tail-r25';
import { supplierBillOwnerPredicate } from '../../../packages/server-procurement/src/adapters/bill-source-sql';
import { sourceImage } from '../../../packages/server-procurement/src/adapters/read-tail-r31';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-procurement/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('Procurement read-tail r31 parameterized family dedupe', () => {
  test('sourceImage renders the RFQ attachment image read byte-identically', () => {
    expect(sourceImage("(to_jsonb(r)-'version'-'updated_at')::text image", 'procurement_rfqs r', 'id=? AND company_id=?')).toBe(
      "SELECT (to_jsonb(r)-'version'-'updated_at')::text image FROM procurement_rfqs r WHERE id=? AND company_id=?",
    );
  });

  test('sourceImage renders the attachment bill image read byte-identically', () => {
    expect(sourceImage(
      `jsonb_build_object('bill',to_jsonb(r)-'version'-'updated_at','receipts',${receiptImage("r.id")})::text image`,
      'procurement_bill_matches r',
      `r.id=? AND r.company_id=? AND ${supplierBillOwnerPredicate("r")}`,
    )).toBe(
      `SELECT jsonb_build_object('bill',to_jsonb(r)-'version'-'updated_at','receipts',${receiptImage("r.id")})::text image FROM procurement_bill_matches r WHERE r.id=? AND r.company_id=? AND ${supplierBillOwnerPredicate("r")}`,
    );
  });

  test('sourceImage renders the supplier-bill image read byte-identically', () => {
    const scoped = branchDepartmentScope("m");
    expect(sourceImage(
      `jsonb_build_object('bill',to_jsonb(m),'receipts',${receiptImage("m.id")})::text image`,
      `procurement_bill_matches m JOIN commerce_fulfillments primary_receipt ON primary_receipt.id=m.fulfillment_id AND primary_receipt.order_id=m.order_id JOIN commerce_orders primary_order ON primary_order.id=m.order_id AND primary_order.company_id=m.company_id AND primary_order.kind='purchase'`,
      `m.id=? AND ${scoped} AND ${billReceiptBinding(false,"br","m")} AND ${foreignBillReceiptWitness("m")}`,
    )).toBe(
      `SELECT jsonb_build_object('bill',to_jsonb(m),'receipts',${receiptImage("m.id")})::text image FROM procurement_bill_matches m JOIN commerce_fulfillments primary_receipt ON primary_receipt.id=m.fulfillment_id AND primary_receipt.order_id=m.order_id JOIN commerce_orders primary_order ON primary_order.id=m.order_id AND primary_order.company_id=m.company_id AND primary_order.kind='purchase' WHERE m.id=? AND ${scoped} AND ${billReceiptBinding(false,"br","m")} AND ${foreignBillReceiptWitness("m")}`,
    );
  });

  test('callers use the helper and keep no inline family statements', () => {
    expect(src('attachment-purchases.ts')).toContain('sourceImage("(to_jsonb(r)-\'version\'-\'updated_at\')::text image"');
    expect(src('attachment-purchases.ts')).toContain('sourceImage(`jsonb_build_object(\'bill\',to_jsonb(r)');
    expect(src('bill-purchases.ts')).toContain('sourceImage(`jsonb_build_object(\'bill\',to_jsonb(m)');
    expect(src('attachment-purchases.ts')).not.toContain('SELECT (to_jsonb(r)-\'version\'-\'updated_at\')::text image');
    expect(src('attachment-purchases.ts')).not.toContain("SELECT jsonb_build_object('bill',to_jsonb(r)-'version'-'updated_at','receipts'");
    expect(src('bill-purchases.ts')).not.toContain("SELECT jsonb_build_object('bill',to_jsonb(m),'receipts'");
  });

  test('no write, lock, command_receipts or r17-r30 helper was touched', () => {
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
    expect(src('read-tail-r30.ts')).toContain('export const assignmentScopeProbe =');
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=125', () => {
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
    expect(total).toBeLessThan(125);
  });
});
