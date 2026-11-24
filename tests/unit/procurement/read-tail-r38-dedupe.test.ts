import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { lineSourceWitness } from '../../../packages/server-procurement/src/adapters/read-tail-r38';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-procurement/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('Procurement read-tail r38 parameterized family dedupe', () => {
  test('lineSourceWitness renders the three package-obligation line witnesses byte-identically', () => {
    expect(lineSourceWitness('${commerce.lines}', 'order_id', 'o.id'))
      .toBe('(SELECT 1 FROM (${commerce.lines}) line WHERE line.order_id=o.id)');
    expect(lineSourceWitness('${commerce.receiptLines}', 'fulfillment_id', 'receipt.id'))
      .toBe('(SELECT 1 FROM (${commerce.receiptLines}) line WHERE line.fulfillment_id=receipt.id)');
    expect(lineSourceWitness('${commerce.returnLines}', 'return_id', 'returned.id'))
      .toBe('(SELECT 1 FROM (${commerce.returnLines}) line WHERE line.return_id=returned.id)');
  });

  test('all three call sites delegate and keep no inline line witness', () => {
    const file = src('package-obligations.ts');
    expect(file).toContain('lineSourceWitness(');
    expect(file.match(/lineSourceWitness\(/g)?.length).toBe(3);
    expect(file).not.toContain('EXISTS(SELECT 1 FROM (${commerce.lines}) line WHERE line.order_id=o.id)');
    expect(file).not.toContain('NOT EXISTS(SELECT 1 FROM (${commerce.receiptLines}) line WHERE line.fulfillment_id=receipt.id)');
    expect(file).not.toContain('NOT EXISTS(SELECT 1 FROM (${commerce.returnLines}) line WHERE line.return_id=returned.id)');
    expect(src('read-tail-r38.ts')).toContain('export const lineSourceWitness =');
  });

  test('no write, lock, command_receipts or r17-r37 helper was touched', () => {
    expect(src('read-tail-sql.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(src('read-tail-sql.ts')).toContain('export const pointRead =');
    expect(src('read-tail-sql.ts')).toContain('export const orderedRead =');
    for (const [n, e] of [
      ['r19', 'tableRead'], ['r20', 'receiptCount'], ['r21', 'jsonbAggBy'], ['r22', 'jsonbAggJoinBy'],
      ['r23', 'billReceiptBinding'], ['r24', 'eligiblePurchaseCount'], ['r25', 'foreignBillReceiptWitness'],
      ['r26', 'fulfillmentLinesBy'], ['r27', 'settingsRead'], ['r28', 'attachmentVersionWitness'],
      ['r29', 'policyVersionGuard'], ['r30', 'orderScopedAggregate'], ['r31', 'sourceImage'],
      ['r32', 'progressOrderAggregate'], ['r33', 'quotedProjection'], ['r34', 'calendarScopedListRead'],
      ['r35', 'quotedFromSource'],
    ] as const) expect(src(`read-tail-${n}.ts`)).toContain(`export const ${e} =`);
    const obligation = src('package-obligations.ts');
    expect(obligation).toContain('command_receipts command WHERE command.record_id=o.id');
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=111', () => {
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
    expect(total).toBeLessThan(111);
  });
});
