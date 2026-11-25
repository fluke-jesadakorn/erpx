import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { commerceProductsSelectSql } from '../../../packages/server-commerce/src/adapters/read-tail-r28-dedupe-sql';

const root = resolve(import.meta.dirname ?? process.cwd(), '../../..');
const adapters = resolve(root, 'packages/server-commerce/src/adapters');
const src = resolve(root, 'packages/server-commerce/src');
const read = (file: string) => readFileSync(resolve(adapters, file), 'utf8');
const norm = (s: string) => s.replace(/\s+/g, ' ');
const sha = (s: string) => createHash('sha256').update(s, 'utf8').digest('hex');

type Site = {
  id: string;
  file: string;
  tail: string;
  before: string;
  after: string;
  call: string;
  digest: string;
};

/** r28 family: the active stock-products master selector head in the Inventory
 * initial-count source and the count-source activeProducts reader. Both shared
 * the exact head through `WHERE ` and differed only in the predicate tail.
 * r18-r27 renderers cover different heads (order source, settlement lines,
 * receipt/guard frames, schema probes, settings image) and are not reused. */
const HEAD = 'SELECT id,sku,name,unit,tracking_mode FROM commerce_products WHERE ';
const COUNT_TAIL = "company_id=? AND status='active' AND item_type='stock' ORDER BY sku,id";
const ACTIVE_TAIL = "company_id=? AND status='active' ORDER BY sku";

const FAMILY: Site[] = [
  {
    id: 'initial-count-active-products',
    file: 'initial-physical-stock-count-sources.ts',
    tail: COUNT_TAIL,
    before: `${HEAD}${COUNT_TAIL}`,
    after: commerceProductsSelectSql(COUNT_TAIL),
    call: `database.prepare(commerceProductsSelectSql("${COUNT_TAIL}"))`,
    digest: '9f7f17f9a4e004ad60a665eddaafa85c3ad159d6f1ee3c7cdba9a8579e4b9bb3',
  },
  {
    id: 'count-source-active-products',
    file: 'inventory-counts-sources.ts',
    tail: ACTIVE_TAIL,
    before: `${HEAD}${ACTIVE_TAIL}`,
    after: commerceProductsSelectSql(ACTIVE_TAIL),
    call: `db.prepare(commerceProductsSelectSql("${ACTIVE_TAIL}"))`,
    digest: 'da2b441213678fbe35457faad9bdf0d1c059b652885bb54771c1419fea8d71a2',
  },
];

describe('Commerce read-tail r28 products master-selector head retirement', () => {
  test('both sites render byte-identically to the retired literals', () => {
    expect(FAMILY).toHaveLength(2);
    for (const site of FAMILY) {
      expect(site.after, site.id).toBe(site.before);
      expect(Buffer.from(site.after, 'utf8'), site.id).toEqual(Buffer.from(site.before, 'utf8'));
      expect(sha(site.after), site.id).toBe(site.digest);
      expect(site.after, site.id).toContain(HEAD);
      expect(site.after, site.id).toContain(site.tail);
      expect((site.after.match(/SELECT /g) ?? []).length, site.id).toBe(1);
      expect(site.after, site.id).not.toMatch(/FOR UPDATE|FOR SHARE|LOCK TABLE/);
    }
    expect(sha(FAMILY.map((site) => site.after).join('\n'))).toBe(
      '8e79df210ce8ad27cffa3c548605370f0c5085579c6fb949db45574a0300f397',
    );
    expect(commerceProductsSelectSql(COUNT_TAIL)).not.toBe(commerceProductsSelectSql(ACTIVE_TAIL));
    expect(sha(HEAD)).toBe('e4d90ceba47e6622bcc29ac258264c8b7786d635d3080267d2c3ebb77af1816e');
  });

  test('both call sites delegate with unchanged tails, binds and locks', () => {
    for (const site of FAMILY) {
      const text = read(site.file);
      expect(norm(text), `${site.id} delegation`).toContain(norm(site.call));
      expect(norm(text), `${site.id} import`).toContain(norm("import { commerceProductsSelectSql } from './read-tail-r28-dedupe-sql';"));
      expect(text, `${site.id} retired literal`).not.toContain(site.before);
      expect(text, `${site.id} retired literal (quoted)`).not.toContain(`'${site.before}'`);
      expect(text, `${site.id} retired literal (quoted)`).not.toContain(`"${site.before}"`);
      expect(text.split('commerceProductsSelectSql(').length - 1, `${site.id} calls`).toBe(1);
    }
    // Binds stay at the call site; the renderer never hoists a value.
    expect(norm(read('initial-physical-stock-count-sources.ts'))).toContain(
      norm(`commerceProductsSelectSql("${COUNT_TAIL}")).bind(actor.companyId).all<CountRow>()`),
    );
    expect(norm(read('inventory-counts-sources.ts'))).toContain(
      norm(`commerceProductsSelectSql("${ACTIVE_TAIL}")).bind(companyId).all<CountRow>()`),
    );
    // Sibling locks and native reads stay byte-unchanged (not part of this head).
    expect(read('initial-physical-stock-count-sources.ts')).toContain('LOCK TABLE commerce_stock IN SHARE ROW EXCLUSIVE MODE');
    expect(read('initial-physical-stock-count-sources.ts')).toContain('LOCK TABLE commerce_settings,commerce_warehouses,commerce_products IN SHARE MODE');
    expect(read('initial-physical-stock-count-sources.ts')).toContain('nativeCommerceReadFirst<CountRow>(RD_WAREHOUSE_ACTIVE');
    expect(read('inventory-counts-sources.ts')).toContain('nativeCommerceReadAll<CountRow>(RD_WAREHOUSES_ACTIVE');
    expect(read('inventory-counts-sources.ts')).toContain('nativeCommerceReadFirst<CountRow>(RD_PRODUCT_ACTIVE');
  });

  test('one executable head; near-miss product reads stay inline', () => {
    let heads = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) heads += (readFileSync(p, 'utf8').match(/SELECT id,sku,name,unit,tracking_mode FROM commerce_products WHERE /g) ?? []).length;
      }
    };
    walk(src);
    expect(heads).toBe(1);
    const renderer = read('read-tail-r28-dedupe-sql.ts');
    expect(renderer.split('export const commerceProductsSelectSql =').length - 1).toBe(1);
    expect(renderer.split('`SELECT id,sku,name,unit,tracking_mode FROM commerce_products WHERE ${tail}`').length - 1).toBe(1);
    // Same-table siblings with different columns/aliases are not this head.
    expect(read('sales-order-sources.ts')).toContain('SELECT id,sku,name,tax_bps FROM commerce_products WHERE company_id=?');
    expect(read('pms-command-facts.ts')).toContain('SELECT id,name FROM commerce_products WHERE company_id=?');
    expect(read('guarded-source-dedupe-sql.ts')).toContain('SELECT\t* FROM commerce_products WHERE id=? AND company_id=? AND status=\'active\'');
    expect(commerceProductsSelectSql('x')).not.toBe('SELECT id,sku,name,unit,tracking_mode FROM commerce_products WHERE x ORDER BY sku,id');
  });

  test('protected renderers, row locks, native registries and accounting locks stay untouched', () => {
    expect(read('read-tail-r21-dedupe-sql.ts')).toContain('`SELECT ${columns} FROM commerce_orders WHERE ${tail}`');
    expect(read('read-tail-r25-dedupe-sql.ts')).toContain('`SELECT line.* FROM commerce_order_lines line JOIN commerce_orders parent ON parent.id=line.order_id WHERE ${tail}`');
    expect(read('read-tail-r26-dedupe-sql.ts')).toContain('`SELECT to_jsonb(source)::text image FROM commerce_orders source WHERE ${tail}`');
    expect(read('read-tail-r27-dedupe-sql.ts')).toContain('`SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE ${tail}`');
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

  test('retirement:ts-statements pins the owner TypeScript SELECT KPI at 407 (before 408)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(src);
    expect(total).toBe(407);
    expect((read('initial-physical-stock-count-sources.ts').match(/SELECT /g) ?? []).length).toBe(4);
    expect((read('inventory-counts-sources.ts').match(/SELECT /g) ?? []).length).toBe(3);
    expect((read('read-tail-r28-dedupe-sql.ts').match(/SELECT /g) ?? []).length).toBe(1);
  });
});
