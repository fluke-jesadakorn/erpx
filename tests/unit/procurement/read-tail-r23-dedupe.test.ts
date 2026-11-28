import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { billReceiptBinding, returnedWithoutReplacement } from '../../../modules/procurement/server/src/adapters/read-tail-r23';

const src = (name: string) => readFileSync(
  new URL(`../../../modules/procurement/server/src/adapters/${name}`, import.meta.url), 'utf8',
);
const norm = (s: string) => s.replace(/\s+/g, ' ');

describe('Procurement read-tail r23 parameterized family dedupe', () => {
  test('returnedWithoutReplacement renders the exact original WHERE text', () => {
    expect(returnedWithoutReplacement('latest')).toBe(
      "(latest.status='returned' AND NOT EXISTS( SELECT 1 FROM procurement_bill_match_receipts returned_receipt JOIN procurement_bill_match_receipts replacement_receipt ON replacement_receipt.fulfillment_id=returned_receipt.fulfillment_id AND replacement_receipt.active=TRUE WHERE returned_receipt.match_id=latest.id AND replacement_receipt.match_id<>latest.id))",
    );
  });

  test('billReceiptBinding renders each original EXISTS/NOT EXISTS literal byte-identically', () => {
    expect(billReceiptBinding(false, 'br', 'm')).toBe(
      'EXISTS(SELECT 1 FROM procurement_bill_match_receipts br WHERE br.match_id=m.id AND br.fulfillment_id=m.fulfillment_id)',
    );
    expect(billReceiptBinding(false, 'br', '${alias}')).toBe(
      'EXISTS(SELECT 1 FROM procurement_bill_match_receipts br WHERE br.match_id=${alias}.id AND br.fulfillment_id=${alias}.fulfillment_id)',
    );
    expect(billReceiptBinding(true, 'binding', 'bill')).toBe(
      'NOT EXISTS(SELECT 1 FROM procurement_bill_match_receipts binding WHERE binding.match_id=bill.id AND binding.fulfillment_id=bill.fulfillment_id)',
    );
  });

  test('callers use the helpers and keep no inline family fragments', () => {
    expect(src('executive-main-evidence-sources.ts')).toContain('returnedWithoutReplacement("latest")');
    expect(src('executive-metric-projections.ts')).toContain('returnedWithoutReplacement("latest")');
    expect(src('bill-source-sql.ts')).toContain('billReceiptBinding(false,"br",alias)');
    expect(src('bill-purchases.ts')).toContain('billReceiptBinding(false,"br","m")');
    expect(src('package-obligations.ts')).toContain('billReceiptBinding(true,"binding","bill")');
    expect(norm(src('executive-main-evidence-sources.ts'))).not.toContain(norm('returned_receipt JOIN procurement_bill_match_receipts replacement_receipt'));
    expect(norm(src('executive-metric-projections.ts'))).not.toContain(norm('returned_receipt JOIN procurement_bill_match_receipts replacement_receipt'));
    expect(src('bill-source-sql.ts')).not.toContain('br.match_id=${alias}.id AND br.fulfillment_id=${alias}.fulfillment_id)');
    expect(src('bill-purchases.ts')).not.toContain('br.match_id=m.id AND br.fulfillment_id=m.fulfillment_id)');
    expect(src('package-obligations.ts')).not.toContain('binding.match_id=bill.id AND binding.fulfillment_id=bill.fulfillment_id');
  });

  test('no write, lock, command_receipts or r17-r22 helper was touched', () => {
    const tail = src('read-tail-sql.ts');
    expect(tail).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(tail).toContain('export const pointRead =');
    expect(tail).toContain('export const orderedRead =');
    expect(src('read-tail-r19.ts')).toContain('export const receiptImage =');
    expect(src('read-tail-r20.ts')).toContain('export const hrPersonIn =');
    expect(src('read-tail-r21.ts')).toContain('export const jsonbAggBy =');
    expect(src('read-tail-r22.ts')).toContain('export const jsonbAggJoinBy =');
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
    expect(src('command-source.ts')).toContain('jsonbAggJoinBy');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=145', () => {
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
    expect(total).toBeLessThan(145);
  });
});
