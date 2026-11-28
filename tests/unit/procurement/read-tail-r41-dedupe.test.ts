import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { openPromiseByOrder } from '../../../modules/procurement/server/src/adapters/read-tail-r41';

const src = (name: string) => readFileSync(
  new URL(`../../../modules/procurement/server/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('Procurement read-tail r41 parameterized family dedupe', () => {
  test('openPromiseByOrder renders both workRows subqueries byte-identically', () => {
    expect(openPromiseByOrder('min(p.promised_date)', ' AND p.remaining_milli>0')).toBe(
      '(SELECT min(p.promised_date) FROM procurement_open_promise_lines p JOIN procurement_delivery_promises h ON h.id=p.promise_id WHERE h.order_id=o.id AND p.remaining_milli>0)',
    );
    expect(openPromiseByOrder('COALESCE(SUM(p.remaining_milli),0)')).toBe(
      '(SELECT COALESCE(SUM(p.remaining_milli),0) FROM procurement_open_promise_lines p JOIN procurement_delivery_promises h ON h.id=p.promise_id WHERE h.order_id=o.id)',
    );
  });

  test('supplier-promises delegates both witnesses and keeps no inline open-promise skeleton', () => {
    const file = src('supplier-promises.ts');
    expect(file).toContain("import {openPromiseByOrder} from './read-tail-r41';");
    expect(file.match(/openPromiseByOrder\(/g)?.length).toBe(2);
    expect(file).not.toContain('SELECT min(p.promised_date) FROM procurement_open_promise_lines p JOIN procurement_delivery_promises h');
    expect(file).not.toContain('SELECT COALESCE(SUM(p.remaining_milli),0) FROM procurement_open_promise_lines p JOIN procurement_delivery_promises h');
    expect(src('read-tail-r41.ts')).toContain('export const openPromiseByOrder =');
  });

  test('no write, lock, command_receipts or r17-r40 helper was touched', () => {
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
      ['r40', 'billMatchProjection'],
    ] as const) expect(src(`read-tail-${n}.ts`)).toContain(`export const ${e} =`);
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
    expect(src('attachment-purchases.ts')).toContain('FOR SHARE');
    expect(src('supplier-promises.ts')).toContain('replayCommand(await database(),a,key,operation,id,input)');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=107', () => {
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
    expect(total).toBeLessThan(107);
  });
});
