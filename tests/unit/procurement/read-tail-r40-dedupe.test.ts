import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { billMatchProjection } from '../../../packages/server-procurement/src/adapters/read-tail-r40';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-procurement/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('Procurement read-tail r40 parameterized family dedupe', () => {
  test('billMatchProjection renders the point-read and workspace prefixes byte-identically', () => {
    expect(billMatchProjection('f.version fulfillment_version', 'PARTIES', 'ORDERS', '\n    WHERE m.id=?'))
      .toBe('    SELECT m.*,p.name supplier_name,o.number order_number,f.version fulfillment_version'
        + '\n    FROM procurement_bill_matches m'
        + '\n    JOIN (PARTIES) p ON p.id=m.supplier_id'
        + '\n    JOIN (ORDERS) o ON o.id=m.order_id'
        + '\n    WHERE m.id=?');
    expect(billMatchProjection('\n      COALESCE((SELECT 1),\'[]\') receipt_ids_json', 'PARTIES', 'ORDERS', '\n    WHERE ${scope}'))
      .toBe('    SELECT m.*,p.name supplier_name,o.number order_number,'
        + '\n      COALESCE((SELECT 1),\'[]\') receipt_ids_json'
        + '\n    FROM procurement_bill_matches m'
        + '\n    JOIN (PARTIES) p ON p.id=m.supplier_id'
        + '\n    JOIN (ORDERS) o ON o.id=m.order_id'
        + '\n    WHERE ${scope}');
  });

  test('both supplier-bill reads delegate and keep no inline projection head', () => {
    const file = src('supplier-bills.ts');
    expect(file).toContain('billMatchProjection(');
    expect(file.match(/billMatchProjection\(/g)?.length).toBe(2);
    expect(file).not.toContain('SELECT m.*,p.name supplier_name,o.number order_number,f.version');
    expect(file).not.toContain('SELECT m.*,p.name supplier_name,o.number order_number,\n      COALESCE');
    expect(src('read-tail-r40.ts')).toContain('export const billMatchProjection =');
  });

  test('no write, lock, command_receipts or r17-r39 helper was touched', () => {
    expect(src('read-tail-sql.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(src('read-tail-sql.ts')).toContain('export const pointRead =');
    expect(src('read-tail-sql.ts')).toContain('export const orderedRead =');
    for (const [n, e] of [
      ['r19', 'tableRead'], ['r20', 'receiptCount'], ['r21', 'jsonbAggBy'], ['r22', 'jsonbAggJoinBy'],
      ['r23', 'billReceiptBinding'], ['r24', 'eligiblePurchaseCount'], ['r25', 'foreignBillReceiptWitness'],
      ['r26', 'fulfillmentLinesBy'], ['r27', 'settingsRead'], ['r28', 'attachmentVersionWitness'],
      ['r29', 'policyVersionGuard'], ['r30', 'orderScopedAggregate'], ['r31', 'sourceImage'],
      ['r32', 'progressOrderAggregate'], ['r33', 'quotedProjection'], ['r34', 'calendarScopedListRead'],
      ['r35', 'quotedFromSource'], ['r38', 'lineSourceWitness'], ['r39', 'actualSourceExists'],
    ] as const) expect(src(`read-tail-${n}.ts`)).toContain(`export const ${e} =`);
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=108', () => {
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
    expect(total).toBeLessThan(108);
  });
});
