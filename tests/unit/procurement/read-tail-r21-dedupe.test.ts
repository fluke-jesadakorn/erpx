import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { jsonbAggBy } from '../../../modules/procurement/server/src/adapters/read-tail-r21';

const src = (name: string) => readFileSync(
  new URL(`../../../modules/procurement/server/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('Procurement read-tail r21 byte-identical / parameterized dedupe', () => {
  test('jsonbAggBy renders the exact original projection literals', () => {
    expect(jsonbAggBy('(SOURCE)', 'line', 'order_id', 'o.id', 'line.id')).toBe(
      '(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM (SOURCE) line WHERE line.order_id=o.id)',
    );
    expect(jsonbAggBy('procurement_bill_match_receipts', 'binding', 'match_id', 'bill.id', 'binding.fulfillment_id')).toBe(
      '(SELECT jsonb_agg(to_jsonb(binding) ORDER BY binding.fulfillment_id) FROM procurement_bill_match_receipts binding WHERE binding.match_id=bill.id)',
    );
    expect(jsonbAggBy('procurement_bill_match_events', 'e', 'match_id', 'r.id', 'e.id')).toBe(
      '(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM procurement_bill_match_events e WHERE e.match_id=r.id)',
    );
  });

  test('replaced files carry no inline jsonb_agg projection and use the helper', () => {
    const po = src('package-obligations.ts');
    const ar = src('attachment-read-sources.ts');
    expect(po).not.toContain('SELECT jsonb_agg(to_jsonb');
    expect(ar).not.toContain('SELECT jsonb_agg(to_jsonb');
    expect(po.match(/jsonbAggBy\(/g)?.length).toBe(7);
    expect(ar.match(/jsonbAggBy\(/g)?.length).toBe(3);
  });

  test('no write, lock, command_receipts or r17/r18/r19/r20 helper was touched', () => {
    const tail = src('read-tail-sql.ts');
    expect(tail).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(tail).toContain('export const pointRead =');
    expect(tail).toContain('export const orderedRead =');
    expect(src('read-tail-r19.ts')).toContain('export const tableRead =');
    expect(src('read-tail-r19.ts')).toContain('export const receiptImage =');
    expect(src('read-tail-r20.ts')).toContain('export const receiptCount =');
    expect(src('read-tail-r20.ts')).toContain('export const expenseAccountBy =');
    expect(src('read-tail-r20.ts')).toContain('export const hrPersonIn =');
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=163', () => {
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
    expect(total).toBeLessThan(163);
  });
});
