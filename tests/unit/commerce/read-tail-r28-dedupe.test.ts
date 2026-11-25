import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';

const root = resolve(import.meta.dirname ?? process.cwd(), '../../..');
const adapters = resolve(root, 'packages/server-commerce/src/adapters');
const src = resolve(root, 'packages/server-commerce/src');
const read = (file: string) => readFileSync(resolve(adapters, file), 'utf8');
const norm = (s: string) => s.replace(/\s+/g, ' ');
const sha = (s: string) => createHash('sha256').update(s, 'utf8').digest('hex');
const count = (dir: string, pattern: RegExp): number => {
  let total = 0;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const p = resolve(dir, entry.name);
    if (entry.isDirectory()) total += count(p, pattern);
    else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(pattern) ?? []).length;
  }
  return total;
};

type Site = {
  id: string;
  file: string;
  sql: string;
  kind: string;
  call: string;
  digest: string;
};

/** r28 family: the active stock-products master selector head in the Inventory
 * initial-count source and the count-source `activeProducts` reader. Both now
 * delegate to mounted native `commerce_rd_count_*` command-plan kinds; the TS
 * renderer that prepared the shared head is deleted by this cutover. */
const HEAD = 'SELECT id,sku,name,unit,tracking_mode FROM commerce_products WHERE ';
const COUNT_TAIL = "company_id=? AND status='active' AND item_type='stock' ORDER BY sku,id";
const ACTIVE_TAIL = "company_id=? AND status='active' ORDER BY sku";

const FAMILY: Site[] = [
  {
    id: 'initial-count-active-products',
    file: 'initial-physical-stock-count-sources.ts',
    sql: `${HEAD}${COUNT_TAIL}`,
    kind: 'commerce_rd_count_active_stock_products',
    call: 'nativeCommerceReadAll<CountRow>(RD_COUNT_ACTIVE_STOCK_PRODUCTS,[actor.companyId],database)',
    digest: '9f7f17f9a4e004ad60a665eddaafa85c3ad159d6f1ee3c7cdba9a8579e4b9bb3',
  },
  {
    id: 'count-source-active-products',
    file: 'inventory-counts-sources.ts',
    sql: `${HEAD}${ACTIVE_TAIL}`,
    kind: 'commerce_rd_count_active_products',
    call: 'nativeCommerceReadAll<CountRow>(RD_COUNT_ACTIVE_PRODUCTS, [companyId], db)',
    digest: 'da2b441213678fbe35457faad9bdf0d1c059b652885bb54771c1419fea8d71a2',
  },
];

describe('Commerce r28 products master-selector family native cutover', () => {
  test('both call sites delegate to mounted native owner reads and TS SQL is deleted', () => {
    expect(existsSync(resolve(adapters, 'read-tail-r28-dedupe-sql.ts'))).toBe(false);
    expect(count(src, /SELECT id,sku,name,unit,tracking_mode FROM commerce_products WHERE /g)).toBe(0);
    for (const site of FAMILY) {
      const text = read(site.file);
      expect(norm(text), `${site.id} native call`).toContain(norm(site.call));
      expect(text, `${site.id} renderer import`).not.toContain('read-tail-r28-dedupe-sql');
      expect(text, `${site.id} renderer call`).not.toContain('commerceProductsSelectSql');
      expect(text, `${site.id} retired literal`).not.toContain(site.sql);
    }
    // Caller binds and returned row shape stay unchanged; the count source
    // still exposes `{ results }` to the Inventory application.
    expect(norm(read('initial-physical-stock-count-sources.ts'))).toContain(
      norm('return {warehouses,products};'),
    );
    expect(norm(read('inventory-counts-sources.ts'))).toContain(
      norm('return { results: await nativeCommerceReadAll<CountRow>'),
    );
    const bridge = read('native-commerce-read.ts');
    expect(bridge).toContain(
      "export const RD_COUNT_ACTIVE_STOCK_PRODUCTS = 'commerce_rd_count_active_stock_products';",
    );
    expect(bridge).toContain(
      "export const RD_COUNT_ACTIVE_PRODUCTS = 'commerce_rd_count_active_products';",
    );
    expect(bridge).not.toContain('SELECT ');
    expect(read('initial-physical-stock-count-sources.ts')).toContain('nativeCommerceReadFirst<CountRow>(RD_WAREHOUSE_ACTIVE');
    expect(read('inventory-counts-sources.ts')).toContain('nativeCommerceReadAll<CountRow>(RD_WAREHOUSES_ACTIVE');
    expect(read('inventory-counts-sources.ts')).toContain('nativeCommerceReadFirst<CountRow>(RD_PRODUCT_ACTIVE');
  });

  test('exact retired literals are owned by the native read registry', () => {
    const plans = read('native_commerce_read_sql.rs');
    for (const site of FAMILY) {
      expect(sha(site.sql), site.id).toBe(site.digest);
      expect(site.sql, site.id).toContain(HEAD);
      expect(site.sql, site.id).toContain(site.kind.replace('commerce_rd_count_', '') === 'active_stock_products' ? COUNT_TAIL : ACTIVE_TAIL);
      expect((site.sql.match(/SELECT /g) ?? []).length, site.id).toBe(1);
      expect(site.sql, site.id).not.toMatch(/FOR UPDATE|FOR SHARE|LOCK TABLE|;/);
      const constant = site.kind.replace('commerce_rd_', 'RD_').toUpperCase();
      expect(plans).toContain(`pub const ${constant}: &str = "${site.kind}";`);
      expect(plans).toContain(`"${site.sql}"`);
      expect(plans).toContain(`kind: ${constant},`);
    }
    expect(plans).toContain('assert_eq!(STATEMENTS.len(), 49);');
    expect(plans).toContain('fn inventory_count_product_selectors_keep_their_exact_literals()');
    expect(sha(FAMILY.map((site) => site.sql).join('\n'))).toBe(
      '8e79df210ce8ad27cffa3c548605370f0c5085579c6fb949db45574a0300f397',
    );
    expect(sha(HEAD)).toBe('e4d90ceba47e6622bcc29ac258264c8b7786d635d3080267d2c3ebb77af1816e');
  });

  test('protected renderers, row locks, native registries and accounting locks stay untouched', () => {
    expect(read('read-tail-r21-dedupe-sql.ts')).toContain('`SELECT ${columns} FROM commerce_orders WHERE ${tail}`');
    expect(read('read-tail-r25-dedupe-sql.ts')).toContain('`SELECT line.* FROM commerce_order_lines line JOIN commerce_orders parent ON parent.id=line.order_id WHERE ${tail}`');
    expect(existsSync(resolve(adapters, 'read-tail-r26-dedupe-sql.ts'))).toBe(false);
    expect(existsSync(resolve(adapters, 'read-tail-r27-dedupe-sql.ts'))).toBe(false);
    expect(read('guarded-source-dedupe-sql.ts')).toContain('`SELECT 1 FROM command_receipts WHERE ${tail}`');
    expect(read('guarded-source-dedupe-sql.ts')).toContain('`SELECT 1 FROM commerce_fulfillments WHERE ${tail}`');
    expect(read('attachment-parent-guard-sql.ts')).toContain('`EXISTS(SELECT 1 FROM commerce_fulfillments f WHERE f.${parent}=child.${child} AND f.order_id=o.id)`');
    expect(read('commerce.ts')).toContain("'SELECT id FROM commerce_returns WHERE id=? FOR UPDATE'");
    expect(read('commerce-goods-accounting.ts')).toContain('"SELECT id FROM commerce_fulfillments WHERE id=? FOR UPDATE"');
    expect(read('commerce-goods-accounting.ts')).toContain('SELECT * FROM commerce_invoice_goods_carrying WHERE invoice_id=? AND company_id=?');
    expect(read('sales-package-sources.ts')).toContain('LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,commerce_fulfillment_evidence,commerce_returns,commerce_return_lines,commerce_return_lot_allocations,commerce_return_stock_moves,commerce_goods_valuations,commerce_invoice_goods_carrying,commerce_stock,commerce_stock_moves,commerce_warehouses,commerce_products,commerce_settings IN SHARE MODE');
    expect(read('supply-package-sources.ts')).toContain('LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,commerce_returns,commerce_return_lines,commerce_warehouses,commerce_products,commerce_goods_valuations,commerce_invoice_goods_carrying IN SHARE MODE');
  });

  test('retirement:ts-statements pins the owner TypeScript SELECT KPI at 367 (before 368)', () => {
    expect(count(src, /SELECT /g)).toBe(367);
    expect((read('initial-physical-stock-count-sources.ts').match(/SELECT /g) ?? []).length).toBe(2);
    expect((read('inventory-counts-sources.ts').match(/SELECT /g) ?? []).length).toBe(3);
  });
});
