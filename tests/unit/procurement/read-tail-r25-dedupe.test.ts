import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { foreignBillReceiptWitness, sourceScopeExists, sourceVisible } from '../../../modules/procurement/server/src/adapters/read-tail-r25';

const src = (name: string) => readFileSync(
  new URL(`../../../modules/procurement/server/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('Procurement read-tail r25 parameterized family dedupe', () => {
  test('sourceScopeExists renders the original id-keyed source scope for both kinds', () => {
    expect(sourceScopeExists(false, 'SCOPE')).toBe(
      'SELECT 1 FROM procurement_rfqs r WHERE r.id=? AND SCOPE',
    );
    expect(sourceScopeExists(true, 'SCOPE')).toBe(
      'SELECT 1 FROM procurement_bill_matches r WHERE r.id=? AND SCOPE',
    );
  });

  test('sourceVisible renders both original company/scope visibility witnesses', () => {
    expect(sourceVisible('procurement_rfqs', 'procurement_source', '${id}', '${company}', '${rfq.where}')).toBe(
      'EXISTS(SELECT 1 FROM procurement_rfqs procurement_source WHERE procurement_source.id=${id} AND procurement_source.company_id=${company} AND ${rfq.where})',
    );
    expect(sourceVisible('procurement_bill_matches', 'procurement_bill_source', '${id}', '${company}', '${bill.where}')).toBe(
      'EXISTS(SELECT 1 FROM procurement_bill_matches procurement_bill_source WHERE procurement_bill_source.id=${id} AND procurement_bill_source.company_id=${company} AND ${bill.where})',
    );
  });

  test('foreignBillReceiptWitness renders the original non-foreign-receipt predicate', () => {
    const rendered = foreignBillReceiptWitness('m');
    expect(rendered).toBe(
      "NOT EXISTS(SELECT 1 FROM procurement_bill_match_receipts br LEFT JOIN commerce_fulfillments f ON f.id=br.fulfillment_id LEFT JOIN commerce_orders o ON o.id=f.order_id WHERE br.match_id=m.id AND (o.id IS NULL OR o.kind<>'purchase' OR o.company_id<>m.company_id OR o.party_id<>m.supplier_id OR o.currency<>m.currency))",
    );
    expect(rendered).toContain("o.kind<>'purchase'");
    expect(rendered).toContain('br.match_id=m.id');
  });

  test('callers use the helpers and keep no inline family fragments', () => {
    expect(src('record-purchases.ts')).toContain('sourceVisible("procurement_rfqs","procurement_source",id,company,rfq.where)');
    expect(src('record-purchases.ts')).toContain('sourceVisible("procurement_bill_matches","procurement_bill_source",id,company,bill.where)');
    expect(src('record-purchases.ts')).toContain('sourceScopeExists(bill,scope.where)');
    expect(src('attachment-purchases.ts')).toContain('sourceScopeExists(bill,authority.where)');
    expect(src('bill-source-sql.ts')).toContain('foreignBillReceiptWitness(alias)');
    expect(src('bill-purchases.ts')).toContain('foreignBillReceiptWitness("m")');
    expect(src('record-purchases.ts')).not.toContain('`SELECT 1 FROM ${bill?"procurement_bill_matches":"procurement_rfqs"} r WHERE r.id=? AND ${scope.where}`');
    expect(src('attachment-purchases.ts')).not.toContain('`SELECT 1 FROM ${bill?"procurement_bill_matches":"procurement_rfqs"} r WHERE r.id=? AND ${authority.where}`');
    expect(src('bill-source-sql.ts')).not.toContain('NOT EXISTS(SELECT 1 FROM procurement_bill_match_receipts br LEFT JOIN');
    expect(src('bill-purchases.ts')).not.toContain('NOT EXISTS(SELECT 1 FROM procurement_bill_match_receipts br LEFT JOIN');
  });

  test('no write, lock, command_receipts or r17-r24 helper was touched', () => {
    const tail = src('read-tail-sql.ts');
    expect(tail).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(tail).toContain('export const pointRead =');
    expect(tail).toContain('export const orderedRead =');
    expect(src('read-tail-r19.ts')).toContain('export const tableRead =');
    expect(src('read-tail-r20.ts')).toContain('export const receiptCount =');
    expect(src('read-tail-r21.ts')).toContain('export const jsonbAggBy =');
    expect(src('read-tail-r22.ts')).toContain('export const jsonbAggJoinBy =');
    expect(src('read-tail-r23.ts')).toContain('export const returnedWithoutReplacement =');
    expect(src('read-tail-r23.ts')).toContain('export const billReceiptBinding =');
    expect(src('read-tail-r24.ts')).toContain('export const memberScopeExclusion =');
    expect(src('read-tail-r24.ts')).toContain('export const eligiblePurchaseCount =');
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=140', () => {
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
    expect(total).toBeLessThan(140);
  });
});
