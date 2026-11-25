import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { commerceOrderSourceImageSql } from '../../../packages/server-commerce/src/adapters/read-tail-r26-dedupe-sql';

const root = resolve(import.meta.dirname ?? process.cwd(), '../../..');
const adapters = resolve(root, 'packages/server-commerce/src/adapters');
const src = resolve(root, 'packages/server-commerce/src');
const read = (file: string) => readFileSync(resolve(adapters, file), 'utf8');
const norm = (s: string) => s.replace(/\s+/g, ' ');
const sha = (s: string) => createHash('sha256').update(s, 'utf8').digest('hex');

type Site = {
  id: string;
  file: string;
  before: string;
  after: string;
  call: string;
  digest: string;
};

/** r26 family: the two order-source image heads in the CRM and Support
 * attachment witnesses. Both share the exact head through `WHERE ` and differ
 * only in their tail (CRM pins the quote/sale kind; Support uses the ticket's
 * order FK alone). r18-r25 renderers cover different heads (order-only,
 * command_receipts, fulfillment/return guards, settlement lines and the noop
 * constant) and are deliberately not reused here. */
const HEAD = 'SELECT to_jsonb(source)::text image FROM commerce_orders source WHERE ';
const CRM_TAIL = "source.id=? AND source.company_id=? AND source.kind IN ('quote','sales')";
const SUPPORT_TAIL = 'source.id=? AND source.company_id=?';

const FAMILY: Site[] = [
  {
    id: 'crm-order-source-image',
    file: 'crm-attachment-sources.ts',
    before: `${HEAD}${CRM_TAIL}`,
    after: commerceOrderSourceImageSql(CRM_TAIL),
    call: `commerceOrderSourceImageSql("${CRM_TAIL}")`,
    digest: 'dfe055101af555a0e076492e57ab419e040bd7b6b16562844c2fe502a4de832b',
  },
  {
    id: 'support-order-source-image',
    file: 'support-attachment-sources.ts',
    before: `${HEAD}${SUPPORT_TAIL}`,
    after: commerceOrderSourceImageSql(SUPPORT_TAIL),
    call: `commerceOrderSourceImageSql('${SUPPORT_TAIL}')`,
    digest: '42f704656cd8a9fee53631be3924cf9d8a9bd3c040758b126a1199633232715e',
  },
];

describe('Commerce read-tail r26 order-source image-head retirement', () => {
  test('both sites render byte-identically to the retired literals', () => {
    expect(FAMILY).toHaveLength(2);
    for (const site of FAMILY) {
      expect(site.after, site.id).toBe(site.before);
      expect(Buffer.from(site.after, 'utf8'), site.id).toEqual(Buffer.from(site.before, 'utf8'));
      expect(sha(site.after), site.id).toBe(site.digest);
      expect(site.after, site.id).toContain(HEAD);
      expect((site.after.match(/SELECT /g) ?? []).length, site.id).toBe(1);
      expect(site.after, site.id).not.toMatch(/FOR UPDATE|FOR SHARE|LOCK TABLE/);
    }
    expect(sha(FAMILY.map((site) => site.after).join('\n'))).toBe('5649861c056454b2df0d2d659ce6588aff74ff279566966d4db525ae194981c7');
    expect(commerceOrderSourceImageSql(CRM_TAIL)).not.toBe(commerceOrderSourceImageSql(SUPPORT_TAIL));
  });

  test('both call sites delegate with unchanged tails, binds and locks', () => {
    for (const site of FAMILY) {
      const text = read(site.file);
      expect(norm(text), `${site.id} delegation`).toContain(norm(site.call));
      expect(norm(text), `${site.id} import`).toContain(norm("import {commerceOrderSourceImageSql} from './read-tail-r26-dedupe-sql';"));
      expect(text, `${site.id} retired literal`).not.toContain(site.before);
      expect(text, `${site.id} retired literal (quoted)`).not.toContain(`'${site.before}'`);
      expect(text, `${site.id} retired literal (quoted)`).not.toContain(`"${site.before}"`);
      expect(text.split('commerceOrderSourceImageSql(').length - 1, `${site.id} calls`).toBe(1);
      // Binds stay at the call site; the renderer never hoists a value.
      expect(norm(text), `${site.id} binds`).toContain(norm('.bind(id,companyId).first<string>(\'image\')'));
      // The sibling SHARE ROW EXCLUSIVE lock is byte-unchanged (not FOR SHARE).
      expect(text, `${site.id} lock`).toContain("LOCK TABLE commerce_orders IN SHARE ROW EXCLUSIVE MODE");
    }
  });

  test('one executable head; near-miss image heads stay inline', () => {
    let heads = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) heads += (readFileSync(p, 'utf8').match(/SELECT to_jsonb\(source\)::text image FROM commerce_orders source WHERE /g) ?? []).length;
      }
    };
    walk(src);
    expect(heads).toBe(1);
    const renderer = read('read-tail-r26-dedupe-sql.ts');
    expect(renderer.split('export const commerceOrderSourceImageSql =').length - 1).toBe(1);
    expect(renderer.split('`SELECT to_jsonb(source)::text image FROM commerce_orders source WHERE ${tail}`').length - 1).toBe(1);
    // Aliased image reads over other tables are not byte-identical to this head;
    // r27 retired the two commerce_settings image heads into their own renderer.
    expect(read('expense-posting-setup.ts')).not.toContain('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE s.company_id=?');
    expect(read('manufacturing-existing-posting-setup.ts')).not.toContain('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE company_id=?');
    expect(commerceOrderSourceImageSql('x')).not.toBe('SELECT to_jsonb(source)::text image FROM commerce_orders source WHERE x ORDER BY source.id');
  });

  test('protected r18-r25 renderers, row locks, native registries and accounting locks stay untouched', () => {
    expect(read('read-tail-r21-dedupe-sql.ts')).toContain('`SELECT ${columns} FROM commerce_orders WHERE ${tail}`');
    expect(read('read-tail-r25-dedupe-sql.ts')).toContain('`SELECT line.* FROM commerce_order_lines line JOIN commerce_orders parent ON parent.id=line.order_id WHERE ${tail}`');
    expect(read('guarded-source-dedupe-sql.ts')).toContain('`SELECT 1 FROM command_receipts WHERE ${tail}`');
    expect(read('guarded-source-dedupe-sql.ts')).toContain('`SELECT 1 FROM commerce_fulfillments WHERE ${tail}`');
    expect(read('attachment-parent-guard-sql.ts')).toContain('`EXISTS(SELECT 1 FROM commerce_fulfillments f WHERE f.${parent}=child.${child} AND f.order_id=o.id)`');
    expect(read('commerce.ts')).toContain("'SELECT id FROM commerce_returns WHERE id=? FOR UPDATE'");
    expect(read('commerce-goods-accounting.ts')).toContain('"SELECT id FROM commerce_fulfillments WHERE id=? FOR UPDATE"');
    expect(read('commerce-goods-accounting.ts')).toContain('SELECT * FROM commerce_invoice_goods_carrying WHERE invoice_id=? AND company_id=?');
    expect(read('native_commerce_read_sql.rs')).toContain('assert_eq!(STATEMENTS.len(), 20);');
    expect(read('sales-package-sources.ts')).toContain('LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,commerce_fulfillment_evidence,commerce_returns,commerce_return_lines,commerce_return_lot_allocations,commerce_return_stock_moves,commerce_goods_valuations,commerce_invoice_goods_carrying,commerce_stock,commerce_stock_moves,commerce_warehouses,commerce_products,commerce_settings IN SHARE MODE');
    expect(read('supply-package-sources.ts')).toContain('LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,commerce_returns,commerce_return_lines,commerce_warehouses,commerce_products,commerce_goods_valuations,commerce_invoice_goods_carrying IN SHARE MODE');
  });

  test('retirement:ts-statements pins the owner TypeScript SELECT KPI at 392 (before 393)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(src);
    expect(total).toBe(392);
    expect((read('crm-attachment-sources.ts').match(/SELECT /g) ?? []).length).toBe(0);
    expect((read('support-attachment-sources.ts').match(/SELECT /g) ?? []).length).toBe(0);
    expect((read('read-tail-r26-dedupe-sql.ts').match(/SELECT /g) ?? []).length).toBe(1);
    expect((read('sales-package-sources.ts').match(/SELECT /g) ?? []).length).toBe(11);
    expect((read('supply-package-sources.ts').match(/SELECT /g) ?? []).length).toBe(7);
  });
});
