import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { actualSourceExists } from '../../../packages/server-procurement/src/adapters/read-tail-r39';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-procurement/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('Procurement read-tail r39 parameterized family dedupe', () => {
  test('actualSourceExists renders the two record-source witnesses byte-identically', () => {
    expect(actualSourceExists('procurement_rfqs', 'actual_rfq', '${id}'))
      .toBe('(SELECT 1 FROM procurement_rfqs actual_rfq WHERE actual_rfq.id=${id})');
    expect(actualSourceExists('procurement_bill_matches', 'actual_bill', '${id}'))
      .toBe('(SELECT 1 FROM procurement_bill_matches actual_bill WHERE actual_bill.id=${id})');
  });

  test('both record-source call sites delegate and keep no inline witness', () => {
    const file = src('record-purchases.ts');
    expect(file).toContain('actualSourceExists(');
    expect(file.match(/actualSourceExists\(/g)?.length).toBe(2);
    expect(file).not.toContain('SELECT 1 FROM procurement_rfqs actual_rfq WHERE');
    expect(file).not.toContain('SELECT 1 FROM procurement_bill_matches actual_bill WHERE');
    expect(src('read-tail-r39.ts')).toContain('export const actualSourceExists =');
  });

  test('no write, lock, command_receipts or r17-r38 helper was touched', () => {
    expect(src('read-tail-sql.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(src('read-tail-sql.ts')).toContain('export const pointRead =');
    expect(src('read-tail-sql.ts')).toContain('export const orderedRead =');
    for (const [n, e] of [
      ['r19', 'tableRead'], ['r20', 'receiptCount'], ['r21', 'jsonbAggBy'], ['r22', 'jsonbAggJoinBy'],
      ['r23', 'billReceiptBinding'], ['r24', 'eligiblePurchaseCount'], ['r25', 'foreignBillReceiptWitness'],
      ['r26', 'fulfillmentLinesBy'], ['r27', 'settingsRead'], ['r28', 'attachmentVersionWitness'],
      ['r29', 'policyVersionGuard'], ['r30', 'orderScopedAggregate'], ['r31', 'sourceImage'],
      ['r32', 'progressOrderAggregate'], ['r33', 'quotedProjection'], ['r34', 'calendarScopedListRead'],
      ['r35', 'quotedFromSource'], ['r38', 'lineSourceWitness'],
    ] as const) expect(src(`read-tail-${n}.ts`)).toContain(`export const ${e} =`);
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=109', () => {
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
    expect(total).toBeLessThan(109);
  });
});
