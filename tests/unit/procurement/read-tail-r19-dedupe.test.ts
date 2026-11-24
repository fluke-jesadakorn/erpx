import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  tableRead,
  requestOrderLink,
  pointVersionGuard,
  receiptImage,
} from '../../../packages/server-procurement/src/adapters/read-tail-r19';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-procurement/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('Procurement read-tail r19 byte-identical / parameterized dedupe', () => {
  test('package settlement table reads render the exact original literals', () => {
    const tables = [
      'procurement_rfqs', 'procurement_bill_matches', 'procurement_bill_match_receipts',
      'procurement_bill_match_lines', 'procurement_bill_match_events', 'procurement_delivery_promises',
    ];
    const s = src('package-obligations.ts');
    for (const table of tables) {
      expect(tableRead(table)).toBe(`SELECT * FROM ${table}`);
      expect(s).not.toContain(`"SELECT * FROM ${table}"`);
      expect(s).toContain(`tableRead("${table}")`);
    }
  });

  test('request-order link guards render the exact original literals with binds unchanged', () => {
    const families: [string, string][] = [
      ['l', 'o'], ['link', 'o'], ['link', 'covered_order'],
      ['link', 'matched_order'], ['link', 'paid_order'],
    ];
    const s = src('purchase-request-progress.ts');
    for (const [link, order] of families) {
      const literal = `EXISTS(SELECT 1 FROM procurement_request_orders ${link} WHERE ${link}.request_id=? AND ${link}.order_id=${order}.id)`;
      expect(requestOrderLink(link, order)).toBe(literal);
      expect(s).not.toContain(literal);
      expect(s).toContain(`requestOrderLink("${link}","${order}")`);
    }
    expect(s).toContain('.bind(actor.companyId,request.purchase_order_id,request.id,actor.companyId,request.purchase_order_id,request.id)');
  });

  test('point version guards render the exact original literals', () => {
    const op = src('attachments-operational-facts.ts');
    expect(pointVersionGuard('procurement_bill_matches')).toBe('EXISTS(SELECT 1 FROM procurement_bill_matches WHERE id=? AND company_id=? AND version=?)');
    expect(op).not.toContain('EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?)');
    expect(op).toContain('pointVersionGuard(table)');

    const svc = src('attachments-service-facts.ts');
    const tail = '${item.extra ? ` AND ${item.extra}` : ""}';
    expect(pointVersionGuard('${item.table}', '', tail)).toBe(`EXISTS(SELECT 1 FROM \${item.table} WHERE id=? AND company_id=? AND version=?${tail})`);
    expect(svc).toContain('pointVersionGuard(item.table,"",item.extra ? ` AND ${item.extra}` : "")');

    const rfq = src('rfqs.ts');
    const guards = [
      ['(${commercePlatformSources.commerce_warehouses}) commerce_warehouses', " AND status='active'", ''],
      ['(${commercePlatformSources.commerce_products}) commerce_products', '', " AND status='active'"],
      ['(${financePlatformSources.finance_parties}) finance_parties', '', " AND status='active' AND currency=?"],
    ] as const;
    for (const [source, before, after] of guards) {
      const literal = `EXISTS(SELECT 1 FROM ${source} WHERE id=? AND company_id=?${before} AND version=?${after})`;
      expect(pointVersionGuard(source, before, after)).toBe(literal);
      expect(rfq).not.toContain(literal);
      expect(rfq).toContain('pointVersionGuard(');
    }
  });

  test('receipt image aggregate renders the exact original literal', () => {
    const agg = (ref: string) =>
      `(SELECT jsonb_agg(jsonb_build_object('binding',to_jsonb(br),'receipt',to_jsonb(f),'order',to_jsonb(o)) ORDER BY br.id) FROM procurement_bill_match_receipts br JOIN commerce_fulfillments f ON f.id=br.fulfillment_id JOIN commerce_orders o ON o.id=f.order_id WHERE br.match_id=${ref})`;
    for (const [name, ref] of [['attachment-purchases.ts', 'r.id'], ['bill-purchases.ts', 'm.id']] as const) {
      const s = src(name);
      expect(receiptImage(ref)).toBe(agg(ref));
      expect(s).not.toContain(agg(ref));
      expect(s).toContain(`'receipts',\${receiptImage("${ref}")})::text image`);
    }
  });

  test('no write, lock or r17/r18 helper was touched', () => {
    const tail = src('read-tail-sql.ts');
    expect(tail).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(tail).toContain('export const pointRead =');
    expect(tail).toContain('export const orderedRead =');
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=181', () => {
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
    expect(total).toBeLessThan(181);
  });
});
