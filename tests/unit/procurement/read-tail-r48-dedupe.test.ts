import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { createHash } from 'node:crypto';
import { supplierDetailSelect } from '../../../packages/server-procurement/src/adapters/read-tail-r48';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-procurement/src/adapters/${name}`, import.meta.url), 'utf8',
);
const count = (s: string, needle: string) => s.split(needle).length - 1;
const sha = (name: string) => createHash('sha256').update(src(name)).digest('hex');

// Per-site byte oracles recorded verbatim from the BEFORE=90 rendered SQL at
// HEAD d45a9c7e001ccbc6c59bb482151002cf14070b43. `sql` is the exact string
// handed to write.prepare at each of the three detail sites.
const ORACLES = [
  { site: "bill-lines", prefix: "\n      ", head: "l.*,ol.description,fl.fulfillment_id,f.number receipt_number,f.fulfillment_date,o.number order_number", from: "procurement_bill_match_lines l\n      JOIN (SELECT \"id\",\"fulfillment_id\",\"order_line_id\",\"quantity_milli\",\"value_minor\",\"subtotal_minor\",\"tax_minor\",\"stock_currency\" FROM commerce_fulfillment_lines) fl ON fl.id=l.fulfillment_line_id\n      JOIN (SELECT \"id\",\"order_id\",\"number\",\"fulfillment_date\",\"created_by\",\"created_at\",\"journal_id\",\"invoice_id\",\"version\",\"evidence_note\" FROM commerce_fulfillments) f ON f.id=fl.fulfillment_id\n      JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders) o ON o.id=f.order_id\n      JOIN (SELECT \"id\",\"order_id\",\"product_id\",\"description\",\"quantity_milli\",\"fulfilled_milli\",\"price_minor\",\"tax_bps\",\"subtotal_minor\",\"tax_minor\",\"ordinal\",\"list_price_minor\",\"discount_bps\" FROM commerce_order_lines) ol ON ol.id=fl.order_line_id\n      WHERE l.match_id=? ORDER BY f.fulfillment_date,f.id,ol.ordinal\n    ", gap: "\n      ", sql: "\n      SELECT l.*,ol.description,fl.fulfillment_id,f.number receipt_number,f.fulfillment_date,o.number order_number\n      FROM procurement_bill_match_lines l\n      JOIN (SELECT \"id\",\"fulfillment_id\",\"order_line_id\",\"quantity_milli\",\"value_minor\",\"subtotal_minor\",\"tax_minor\",\"stock_currency\" FROM commerce_fulfillment_lines) fl ON fl.id=l.fulfillment_line_id\n      JOIN (SELECT \"id\",\"order_id\",\"number\",\"fulfillment_date\",\"created_by\",\"created_at\",\"journal_id\",\"invoice_id\",\"version\",\"evidence_note\" FROM commerce_fulfillments) f ON f.id=fl.fulfillment_id\n      JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders) o ON o.id=f.order_id\n      JOIN (SELECT \"id\",\"order_id\",\"product_id\",\"description\",\"quantity_milli\",\"fulfilled_milli\",\"price_minor\",\"tax_bps\",\"subtotal_minor\",\"tax_minor\",\"ordinal\",\"list_price_minor\",\"discount_bps\" FROM commerce_order_lines) ol ON ol.id=fl.order_line_id\n      WHERE l.match_id=? ORDER BY f.fulfillment_date,f.id,ol.ordinal\n    " },
  { site: "bill-receipts", prefix: "\n      ", head: "br.*,f.number receipt_number,f.fulfillment_date,f.version current_version,f.invoice_id,o.id order_id,o.number order_number", from: "procurement_bill_match_receipts br JOIN (SELECT \"id\",\"order_id\",\"number\",\"fulfillment_date\",\"created_by\",\"created_at\",\"journal_id\",\"invoice_id\",\"version\",\"evidence_note\" FROM commerce_fulfillments) f ON f.id=br.fulfillment_id\n      JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders) o ON o.id=f.order_id\n      WHERE br.match_id=? ORDER BY f.fulfillment_date,f.id\n    ", gap: "\n      ", sql: "\n      SELECT br.*,f.number receipt_number,f.fulfillment_date,f.version current_version,f.invoice_id,o.id order_id,o.number order_number\n      FROM procurement_bill_match_receipts br JOIN (SELECT \"id\",\"order_id\",\"number\",\"fulfillment_date\",\"created_by\",\"created_at\",\"journal_id\",\"invoice_id\",\"version\",\"evidence_note\" FROM commerce_fulfillments) f ON f.id=br.fulfillment_id\n      JOIN (SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"kind\",\"number\",\"party_id\",\"warehouse_id\",\"currency\",\"order_date\",\"due_date\",\"note\",\"state\",\"subtotal_minor\",\"tax_minor\",\"source_id\",\"version\",\"created_by\",\"approved_by\",\"created_at\",\"updated_at\",\"price_list_id\",\"price_list_version\",\"discount_reason\",\"discount_approval_required\",\"discount_approved_by\",\"discount_approved_at\" FROM commerce_orders) o ON o.id=f.order_id\n      WHERE br.match_id=? ORDER BY f.fulfillment_date,f.id\n    " },
  { site: "promise-lines", prefix: "", head: "l.*,p.sku", from: "(SELECT \"id\",\"order_id\",\"product_id\",\"description\",\"quantity_milli\",\"fulfilled_milli\",\"price_minor\",\"tax_bps\",\"subtotal_minor\",\"tax_minor\",\"ordinal\",\"list_price_minor\",\"discount_bps\" FROM commerce_order_lines) l JOIN (SELECT \"id\",\"company_id\",\"sku\",\"name\",\"unit\",\"price_minor\",\"tax_bps\",\"status\",\"version\",\"tracking_mode\" FROM commerce_products) p ON p.id=l.product_id WHERE l.order_id=? ORDER BY l.ordinal", gap: " ", sql: "SELECT l.*,p.sku FROM (SELECT \"id\",\"order_id\",\"product_id\",\"description\",\"quantity_milli\",\"fulfilled_milli\",\"price_minor\",\"tax_bps\",\"subtotal_minor\",\"tax_minor\",\"ordinal\",\"list_price_minor\",\"discount_bps\" FROM commerce_order_lines) l JOIN (SELECT \"id\",\"company_id\",\"sku\",\"name\",\"unit\",\"price_minor\",\"tax_bps\",\"status\",\"version\",\"tracking_mode\" FROM commerce_products) p ON p.id=l.product_id WHERE l.order_id=? ORDER BY l.ordinal" },
];

describe('Procurement read-tail r48 supplier detail head dedupe', () => {
  test('all three detail statements render byte-identically', () => {
    for (const { site, prefix, head, from, gap, sql } of ORACLES) {
      expect([site, prefix + supplierDetailSelect(head, from, gap)]).toEqual([site, sql]);
    }
  });

  test('the helper owns the single SELECT head and keeps the default gap', () => {
    const h = src('read-tail-r48.ts');
    expect(count(h, 'export const supplierDetailSelect =')).toBe(1);
    expect(h).toContain('`SELECT ${head}${gap}FROM ${from}`');
    expect(supplierDetailSelect('a', 't x')).toBe('SELECT a FROM t x');
  });

  test('bill and promise details delegate without moving binds', () => {
    const b = src('supplier-bills.ts');
    const p = src('supplier-promises.ts');
    expect(b).toContain("import {supplierDetailSelect} from './read-tail-r48';");
    expect(p).toContain("import {supplierDetailSelect} from './read-tail-r48';");
    expect(count(b, '${supplierDetailSelect(')).toBe(2);
    expect(count(p, 'write.prepare(supplierDetailSelect(')).toBe(1);
    expect(count(b, ').bind(id).all<Row>()')).toBe(3);
    expect(count(p, ').bind(id).all<Row>()')).toBe(1);
    expect(b).not.toContain('SELECT l.*,ol.description');
    expect(b).not.toContain('SELECT br.*,f.number receipt_number');
    expect(p).not.toContain('SELECT l.*,p.sku');
    for (const { head } of ORACLES) expect(b + p).toContain(head);
  });

  test('witnesses, locks, r45 frame and r47 settlement facts stay untouched', () => {
    expect(sha('read-tail-r28.ts')).toBe('d9a93d385847a379c5e470da3add3fd5e504558ec2a1e9c9fb94043efeab94d0');
    expect(sha('read-tail-r44.ts')).toBe('f5dc17c41403aded63b04458839a97a5db2ad0025139f88390478638f45239be');
    expect(sha('read-tail-r45.ts')).toBe('7e85a1f2555ab7d26ef18662380522a8db6d4f78cac5f5be0a27a9d1f3b120ec');
    expect(sha('read-tail-r47.ts')).toBe('7973cb62b2004263bc84593b15b1d5e5ce43dff50b7beea695fab3c6af4d2e6d');
    expect(sha('package-obligations.ts')).toBe('2c344b0529a049c1b03ded03a3a5df74ce5bf0d30d76eb7b8769b1210196b102');
    expect(src('read-tail-r28.ts')).toContain('export const attachmentVersionWitness =');
    expect(src('read-tail-r44.ts')).toContain('export const commandReceiptEventWitness =');
    expect(src('read-tail-r45.ts')).toContain('export const procurementAttachmentReadOuterFrame =');
    expect(src('package-obligations.ts')).toContain('LOCK TABLE procurement_rfqs,');
    expect(src('package-obligations.ts')).toContain('command_receipts IN SHARE MODE');
    expect(src('read-tail-r47.ts')).toContain('export const procurementSettlementFactSelect =');
  });

  test('worktree SELECT count is 90 - 3 + 1 - 3 = 85', () => {
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
    expect(total).toBeLessThan(90);
    expect(total).toBe(77); // r50: four bill-receipt heads (-4 +1 helper) 85 -> 82; r51: two jsonb_agg heads (-2 +1 helper) 82 -> 81; r52: two by-id EXISTS heads (-2 +1 helper) 81 -> 80; r53: two EXISTS/FROM/WHERE heads (-2 +1 helper) 80 -> 79; r54: two columns/FROM/WHERE heads (-2 +1 helper) 79 -> 78; r55: two ARRAY heads (-2 +1 helper) 78 -> 77.
  });
});
