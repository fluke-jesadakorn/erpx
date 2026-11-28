import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { jsonbAggBy } from '../../../modules/procurement/server/src/adapters/read-tail-r21';
import { jsonbAggJoinBy } from '../../../modules/procurement/server/src/adapters/read-tail-r22';

const src = (name: string) => readFileSync(
  new URL(`../../../modules/procurement/server/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('Procurement read-tail r22 byte-identical / parameterized dedupe', () => {
  test('jsonbAggJoinBy renders the exact original JOIN projection literals', () => {
    expect(jsonbAggJoinBy('procurement_rfq_offer_lines', 'line', 'procurement_rfq_offers', 'offer', 'offer.id=line.offer_id', 'rfq_id', 'r.id', 'line.id')).toBe(
      '(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM procurement_rfq_offer_lines line JOIN procurement_rfq_offers offer ON offer.id=line.offer_id WHERE offer.rfq_id=r.id)',
    );
    expect(jsonbAggJoinBy('procurement_delivery_promise_lines', 'line', 'procurement_delivery_promises', 'promise', 'promise.id=line.promise_id', 'order_id', 'r.id', 'line.id')).toBe(
      '(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM procurement_delivery_promise_lines line JOIN procurement_delivery_promises promise ON promise.id=line.promise_id WHERE promise.order_id=r.id)',
    );
  });

  test('jsonbAggBy renders the remaining command-source projection literals byte-identically', () => {
    expect(jsonbAggBy('procurement_rfq_lines', 'line', 'rfq_id', 'r.id', 'line.id')).toBe(
      '(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM procurement_rfq_lines line WHERE line.rfq_id=r.id)',
    );
    expect(jsonbAggBy('procurement_rfq_offers', 'offer', 'rfq_id', 'r.id', 'offer.id')).toBe(
      '(SELECT jsonb_agg(to_jsonb(offer) ORDER BY offer.id) FROM procurement_rfq_offers offer WHERE offer.rfq_id=r.id)',
    );
    expect(jsonbAggBy('procurement_rfq_events', 'event', 'rfq_id', 'r.id', 'event.id')).toBe(
      '(SELECT jsonb_agg(to_jsonb(event) ORDER BY event.id) FROM procurement_rfq_events event WHERE event.rfq_id=r.id)',
    );
    expect(jsonbAggBy('procurement_request_orders', 'binding', 'rfq_id', 'r.id', 'binding.request_id,binding.order_id')).toBe(
      '(SELECT jsonb_agg(to_jsonb(binding) ORDER BY binding.request_id,binding.order_id) FROM procurement_request_orders binding WHERE binding.rfq_id=r.id)',
    );
    expect(jsonbAggBy('(SOURCE)', 'line', 'order_id', 'r.id', 'line.id')).toBe(
      '(SELECT jsonb_agg(to_jsonb(line) ORDER BY line.id) FROM (SOURCE) line WHERE line.order_id=r.id)',
    );
    expect(jsonbAggBy('procurement_delivery_promises', 'promise', 'order_id', 'r.id', 'promise.id')).toBe(
      '(SELECT jsonb_agg(to_jsonb(promise) ORDER BY promise.id) FROM procurement_delivery_promises promise WHERE promise.order_id=r.id)',
    );
  });

  test('command-source projection has no inline jsonb_agg and uses both helpers', () => {
    const cs = src('command-source.ts');
    expect(cs).not.toContain('SELECT jsonb_agg(to_jsonb');
    expect(cs.match(/jsonbAggBy\(/g)?.length).toBe(8);
    expect(cs.match(/jsonbAggJoinBy\(/g)?.length).toBe(2);
  });

  test('no write, lock, command_receipts or r17/r18/r19/r20/r21 helper was touched', () => {
    const tail = src('read-tail-sql.ts');
    expect(tail).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(tail).toContain('export const pointRead =');
    expect(tail).toContain('export const orderedRead =');
    expect(src('read-tail-r19.ts')).toContain('export const tableRead =');
    expect(src('read-tail-r19.ts')).toContain('export const receiptImage =');
    expect(src('read-tail-r20.ts')).toContain('export const receiptCount =');
    expect(src('read-tail-r20.ts')).toContain('export const expenseAccountBy =');
    expect(src('read-tail-r20.ts')).toContain('export const hrPersonIn =');
    expect(src('read-tail-r21.ts')).toContain('export const jsonbAggBy =');
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=154', () => {
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
    expect(total).toBeLessThan(154);
  });
});
