import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { fulfillmentLinesBy } from '../../../packages/server-procurement/src/adapters/read-tail-r26';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-procurement/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('Procurement read-tail r26 parameterized family dedupe', () => {
  test('fulfillmentLinesBy renders the original postable-lines statement', () => {
    const rendered = fulfillmentLinesBy('FL', 'OL', 'P', '?,?');
    expect(rendered).toBe(
      'SELECT fl.*,ol.product_id,ol.description,p.sku,p.unit,p.tracking_mode FROM (FL) fl JOIN (OL) ol ON ol.id=fl.order_line_id JOIN (P) p ON p.id=ol.product_id WHERE fl.fulfillment_id IN (?,?) ORDER BY fl.fulfillment_id,ol.ordinal',
    );
    expect(rendered).toContain('fl.fulfillment_id IN (?,?)');
    expect(rendered).toContain('ORDER BY fl.fulfillment_id,ol.ordinal');
  });

  test('both callers use the helper and keep no inline family fragment', () => {
    const s = src('supplier-bills.ts');
    expect(s.match(/fulfillmentLinesBy\(/g)?.length).toBe(2);
    expect(s).toContain('import {fulfillmentLinesBy} from \'./read-tail-r26\';');
    expect(s).not.toContain('SELECT fl.*,ol.product_id,ol.description,p.sku,p.unit,p.tracking_mode');
  });

  test('no write, lock, command_receipts or r17-r25 helper was touched', () => {
    const tail = src('read-tail-sql.ts');
    expect(tail).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(tail).toContain('export const pointRead =');
    expect(tail).toContain('export const orderedRead =');
    expect(src('read-tail-r19.ts')).toContain('export const tableRead =');
    expect(src('read-tail-r20.ts')).toContain('export const expenseAccountBy =');
    expect(src('read-tail-r21.ts')).toContain('export const jsonbAggBy =');
    expect(src('read-tail-r22.ts')).toContain('export const jsonbAggJoinBy =');
    expect(src('read-tail-r23.ts')).toContain('export const billReceiptBinding =');
    expect(src('read-tail-r24.ts')).toContain('export const eligiblePurchaseCount =');
    expect(src('read-tail-r25.ts')).toContain('export const foreignBillReceiptWitness =');
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=137', () => {
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
    expect(total).toBeLessThan(137);
  });
});
