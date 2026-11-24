import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { quotedFromSource } from '../../../packages/server-procurement/src/adapters/read-tail-r35';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-procurement/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('Procurement read-tail r35 parameterized family dedupe', () => {
  test('quotedFromSource renders the three notification snapshot FROM clauses byte-identically', () => {
    expect(quotedFromSource('"id", "name"', 'procurement_rfqs', 'r'))
      .toBe('FROM (SELECT "id", "name" FROM procurement_rfqs) r');
    expect(quotedFromSource('"id", "version"', '(${src}) finance_supplier_batches', 'b'))
      .toBe('FROM (SELECT "id", "version" FROM (${src}) finance_supplier_batches) b');
    expect(quotedFromSource('"id", "status"', 'procurement_bill_matches', 'm'))
      .toBe('FROM (SELECT "id", "status" FROM procurement_bill_matches) m');
  });

  test('all three call sites use the helper and keep no inline FROM (SELECT', () => {
    const file = src('collaboration-notification-snapshot-sources.ts');
    expect(file).toContain('quotedFromSource(');
    expect(file.match(/quotedFromSource\(/g)?.length).toBe(3);
    expect(file).not.toContain('FROM (SELECT \\"id\\"');
    expect(file).not.toContain('FROM (SELECT "id"');
    expect(src('read-tail-r35.ts')).toContain('export const quotedFromSource =');
  });

  test('no write, lock, command_receipts or r17-r34 helper was touched', () => {
    expect(src('read-tail-sql.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(src('read-tail-sql.ts')).toContain('export const pointRead =');
    expect(src('read-tail-sql.ts')).toContain('export const orderedRead =');
    for (const [n, e] of [
      ['r19', 'tableRead'], ['r20', 'receiptCount'], ['r21', 'jsonbAggBy'], ['r22', 'jsonbAggJoinBy'],
      ['r23', 'billReceiptBinding'], ['r24', 'eligiblePurchaseCount'], ['r25', 'foreignBillReceiptWitness'],
      ['r26', 'fulfillmentLinesBy'], ['r27', 'settingsRead'], ['r28', 'attachmentVersionWitness'],
      ['r29', 'policyVersionGuard'], ['r30', 'orderScopedAggregate'], ['r31', 'sourceImage'],
      ['r32', 'progressOrderAggregate'], ['r33', 'quotedProjection'], ['r34', 'calendarScopedListRead'],
    ] as const) expect(src(`read-tail-${n}.ts`)).toContain(`export const ${e} =`);
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=114', () => {
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
    expect(total).toBeLessThan(114);
  });
});
