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

/** r26 family: the order-source image witness read in the CRM and Support
 * attachment sources. Both now delegate to mounted native
 * `commerce_rd_*_order_source_image` command-plan kinds; the TS renderer that
 * prepared the shared head is deleted by this cutover. */
const HEAD = 'SELECT to_jsonb(source)::text image FROM commerce_orders source WHERE ';
const CRM_TAIL = "source.id=? AND source.company_id=? AND source.kind IN ('quote','sales')";
const SUPPORT_TAIL = 'source.id=? AND source.company_id=?';

const FAMILY: Site[] = [
  {
    id: 'crm-order-source-image',
    file: 'crm-attachment-sources.ts',
    sql: `${HEAD}${CRM_TAIL}`,
    kind: 'commerce_rd_crm_order_source_image',
    call: 'nativeCommerceReadFirst<string>(RD_CRM_ORDER_SOURCE_IMAGE,[id,companyId],database)',
    digest: 'dfe055101af555a0e076492e57ab419e040bd7b6b16562844c2fe502a4de832b',
  },
  {
    id: 'support-order-source-image',
    file: 'support-attachment-sources.ts',
    sql: `${HEAD}${SUPPORT_TAIL}`,
    kind: 'commerce_rd_support_order_source_image',
    call: 'nativeCommerceReadFirst<string>(RD_SUPPORT_ORDER_SOURCE_IMAGE,[id,companyId],database)',
    digest: '42f704656cd8a9fee53631be3924cf9d8a9bd3c040758b126a1199633232715e',
  },
];

describe('Commerce r26 order-source image family native cutover', () => {
  test('both attachment witnesses delegate to mounted native owner reads', () => {
    expect(existsSync(resolve(adapters, 'read-tail-r26-dedupe-sql.ts'))).toBe(false);
    expect(count(src, /SELECT to_jsonb\(source\)::text image FROM commerce_orders source WHERE /g)).toBe(0);
    for (const site of FAMILY) {
      const text = read(site.file);
      expect(norm(text), `${site.id} native call`).toContain(norm(site.call));
      expect(text, `${site.id} renderer import`).not.toContain('read-tail-r26-dedupe-sql');
      expect(text, `${site.id} renderer call`).not.toContain('commerceOrderSourceImageSql');
      expect(text, `${site.id} retired literal`).not.toContain(site.sql);
      // The sibling SHARE ROW EXCLUSIVE lock is byte-unchanged (not FOR SHARE).
      expect(text, `${site.id} lock`).toContain('LOCK TABLE commerce_orders IN SHARE ROW EXCLUSIVE MODE');
      expect(text, `${site.id} inline SELECT`).not.toContain('SELECT ');
    }
    const bridge = read('native-commerce-read.ts');
    expect(bridge).toContain(
      "export const RD_SUPPORT_ORDER_SOURCE_IMAGE = 'commerce_rd_support_order_source_image';",
    );
    expect(bridge).toContain(
      "export const RD_CRM_ORDER_SOURCE_IMAGE = 'commerce_rd_crm_order_source_image';",
    );
    expect(bridge).not.toContain('SELECT ');
  });

  test('exact retired literals are byte-identical and owned by the native registry', () => {
    const plans = read('native_commerce_read_sql.rs');
    for (const site of FAMILY) {
      expect(sha(site.sql), site.id).toBe(site.digest);
      expect(Buffer.from(site.sql, 'utf8').toString('utf8'), site.id).toContain(HEAD);
      expect(site.sql, site.id).toContain(HEAD);
      expect((site.sql.match(/SELECT /g) ?? []).length, site.id).toBe(1);
      expect((site.sql.match(/\?/g) ?? []).length, site.id).toBe(2);
      expect(site.sql, site.id).not.toMatch(/FOR UPDATE|FOR SHARE|LOCK TABLE|;/);
      const constant = site.kind.replace('commerce_rd_', 'RD_').toUpperCase();
      expect(plans).toContain(`pub const ${constant}: &str = "${site.kind}";`);
      expect(plans).toContain(`"${site.sql}"`);
      expect(plans).toContain(`kind: ${constant},`);
    }
    expect(sha(FAMILY.map((site) => site.sql).join('\n'))).toBe('5649861c056454b2df0d2d659ce6588aff74ff279566966d4db525ae194981c7');
    expect(plans).toContain('assert_eq!(STATEMENTS.len(), 31);');
    expect(plans).toContain('fn attachment_order_source_images_keep_their_exact_literals()');
    // Near-miss image heads are not this family and stay untouched.
    expect(read('expense-posting-setup.ts')).not.toContain('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE s.company_id=?');
    expect(read('manufacturing-existing-posting-setup.ts')).not.toContain('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE company_id=?');
  });

  test('protected r21/r25 renderers, row locks, native registries and accounting locks stay untouched', () => {
    expect(read('read-tail-r21-dedupe-sql.ts')).toContain('`SELECT ${columns} FROM commerce_orders WHERE ${tail}`');
    expect(read('read-tail-r25-dedupe-sql.ts')).toContain('`SELECT line.* FROM commerce_order_lines line JOIN commerce_orders parent ON parent.id=line.order_id WHERE ${tail}`');
    expect(read('guarded-source-dedupe-sql.ts')).toContain('`SELECT 1 FROM command_receipts WHERE ${tail}`');
    expect(read('guarded-source-dedupe-sql.ts')).toContain('`SELECT 1 FROM commerce_fulfillments WHERE ${tail}`');
    expect(read('attachment-parent-guard-sql.ts')).toContain('`EXISTS(SELECT 1 FROM commerce_fulfillments f WHERE f.${parent}=child.${child} AND f.order_id=o.id)`');
    expect(read('commerce.ts')).toContain("'SELECT id FROM commerce_returns WHERE id=? FOR UPDATE'");
    expect(read('commerce-goods-accounting.ts')).toContain('"SELECT id FROM commerce_fulfillments WHERE id=? FOR UPDATE"');
    expect(read('commerce-goods-accounting.ts')).toContain('SELECT * FROM commerce_invoice_goods_carrying WHERE invoice_id=? AND company_id=?');
    expect(read('sales-package-sources.ts')).toContain('LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,commerce_fulfillment_evidence,commerce_returns,commerce_return_lines,commerce_return_lot_allocations,commerce_return_stock_moves,commerce_goods_valuations,commerce_invoice_goods_carrying,commerce_stock,commerce_stock_moves,commerce_warehouses,commerce_products,commerce_settings IN SHARE MODE');
    expect(read('supply-package-sources.ts')).toContain('LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,commerce_returns,commerce_return_lines,commerce_warehouses,commerce_products,commerce_goods_valuations,commerce_invoice_goods_carrying IN SHARE MODE');
  });

  test('retirement:ts-statements pins the owner TypeScript SELECT KPI at 385 (before 386)', () => {
    expect(count(src, /SELECT /g)).toBe(385);
    expect((read('crm-attachment-sources.ts').match(/SELECT /g) ?? []).length).toBe(0);
    expect((read('support-attachment-sources.ts').match(/SELECT /g) ?? []).length).toBe(0);
    expect((read('sales-package-sources.ts').match(/SELECT /g) ?? []).length).toBe(11);
    expect((read('supply-package-sources.ts').match(/SELECT /g) ?? []).length).toBe(7);
  });
});
