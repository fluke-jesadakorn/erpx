import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { commerceSettlementLineSql } from '../../../packages/server-commerce/src/adapters/read-tail-r25-dedupe-sql';

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

/** r25 family: the two settlement line-head literals in the Sales and Supply
 * withdrawal source witnesses. Both share the exact head through `WHERE ` and
 * differ only in their `parent.kind` tail. r18-r24 renderers cover different
 * heads (order-only, command_receipts, fulfillment guards, returns points) and
 * are deliberately not reused here. */
const SALES_TAIL = `parent.kind IN ('quote','sales')`;
const SUPPLY_TAIL = `parent.kind='purchase'`;
const HEAD = 'SELECT line.* FROM commerce_order_lines line JOIN commerce_orders parent ON parent.id=line.order_id WHERE ';

const FAMILY: Site[] = [
  {
    id: 'sales-settlement-lines',
    file: 'sales-package-sources.ts',
    before: `${HEAD}${SALES_TAIL}`,
    after: commerceSettlementLineSql(SALES_TAIL),
    call: `lines:commerceSettlementLineSql("${SALES_TAIL}")`,
    digest: '05653e1e64c254350a9d7cc17d3c2e936dbbf5a13abd29320f97b3aabf28fdbf',
  },
  {
    id: 'supply-settlement-lines',
    file: 'supply-package-sources.ts',
    before: `${HEAD}${SUPPLY_TAIL}`,
    after: commerceSettlementLineSql(SUPPLY_TAIL),
    call: `lines:commerceSettlementLineSql("${SUPPLY_TAIL}")`,
    digest: '0128d860e98f675172bf9ddaefe00c09204320b00f718fc09378a79b13dad7ec',
  },
];

describe('Commerce read-tail r25 settlement line-head retirement', () => {
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
    expect(sha(FAMILY.map((site) => site.after).join('\n'))).toBe('cae16cb8b6427cd8e4344925e0bfd8a0f404e7b24d16556524d1302e21478fb5');
    expect(commerceSettlementLineSql(SALES_TAIL)).not.toBe(commerceSettlementLineSql(SUPPLY_TAIL));
  });

  test('both call sites delegate with unchanged tails and no residual literals', () => {
    for (const site of FAMILY) {
      const text = read(site.file);
      expect(norm(text), `${site.id} delegation`).toContain(norm(site.call));
      expect(norm(text), `${site.id} import`).toContain(norm("import { commerceSettlementLineSql } from"));
      expect(text, `${site.id} retired literal`).not.toContain(site.before);
      expect(text, `${site.id} retired literal (quoted)`).not.toContain(`'${site.before}'`);
      expect(text, `${site.id} retired literal (quoted)`).not.toContain(`"${site.before}"`);
      expect(text.split('commerceSettlementLineSql(').length - 1).toBe(1);
    }
    // Source witnesses keep their own native ports; no bind was hoisted.
    expect(norm(read('sales-package-sources.ts'))).toContain(norm('orders:commerceOrdersReadSql(\'*\', "kind IN (\'quote\',\'sales\')")'));
    expect(norm(read('supply-package-sources.ts'))).toContain(norm('orders:commerceOrdersReadSql(\'*\', "kind=\'purchase\'")'));
  });

  test('one executable head; near-miss JOIN variants stay inline', () => {
    let heads = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) heads += (readFileSync(p, 'utf8').match(/SELECT line\.\* FROM commerce_order_lines line JOIN commerce_orders parent ON parent\.id=line\.order_id WHERE /g) ?? []).length;
      }
    };
    walk(src);
    expect(heads).toBe(1);
    const renderer = read('read-tail-r25-dedupe-sql.ts');
    expect(renderer.split('export const commerceSettlementLineSql =').length - 1).toBe(1);
    expect(renderer.split('`SELECT line.* FROM commerce_order_lines line JOIN commerce_orders parent ON parent.id=line.order_id WHERE ${tail}`').length - 1).toBe(1);
    // Aliased/JOINED siblings are not byte-identical to this head.
    expect(read('sales-package-sources.ts')).toContain('SELECT delivery.*,parent.company_id');
    expect(read('supply-package-sources.ts')).toContain('SELECT receipt.* FROM commerce_fulfillments receipt');
    expect(commerceSettlementLineSql('x')).not.toBe('SELECT line.* FROM commerce_order_lines line JOIN commerce_orders parent ON parent.id=line.order_id WHERE x LIMIT 1');
  });

  test('protected r18-r24 renderers, row locks, native registries and accounting locks stay untouched', () => {
    expect(read('read-tail-r21-dedupe-sql.ts')).toContain('`SELECT ${columns} FROM commerce_orders WHERE ${tail}`');
    expect(read('guarded-source-dedupe-sql.ts')).toContain('`SELECT 1 FROM command_receipts WHERE ${tail}`');
    expect(read('guarded-source-dedupe-sql.ts')).toContain('`SELECT 1 FROM commerce_fulfillments WHERE ${tail}`');
    expect(read('attachment-parent-guard-sql.ts')).toContain('`EXISTS(SELECT 1 FROM commerce_fulfillments f WHERE f.${parent}=child.${child} AND f.order_id=o.id)`');
    expect(read('commerce.ts')).toContain("'SELECT id FROM commerce_returns WHERE id=? FOR UPDATE'");
    expect(read('commerce-goods-accounting.ts')).toContain('"SELECT id FROM commerce_fulfillments WHERE id=? FOR UPDATE"');
    expect(read('commerce-goods-accounting.ts')).toContain('SELECT * FROM commerce_invoice_goods_carrying WHERE invoice_id=? AND company_id=?');
    expect(read('native_commerce_read_sql.rs')).toContain('assert_eq!(STATEMENTS.len(), 53);');
    // The two source witnesses keep their IN SHARE MODE lock statements byte-for-byte.
    expect(read('sales-package-sources.ts')).toContain('LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,commerce_fulfillment_evidence,commerce_returns,commerce_return_lines,commerce_return_lot_allocations,commerce_return_stock_moves,commerce_goods_valuations,commerce_invoice_goods_carrying,commerce_stock,commerce_stock_moves,commerce_warehouses,commerce_products,commerce_settings IN SHARE MODE');
    expect(read('supply-package-sources.ts')).toContain('LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,commerce_returns,commerce_return_lines,commerce_warehouses,commerce_products,commerce_goods_valuations,commerce_invoice_goods_carrying IN SHARE MODE');
  });

  test('retirement:ts-statements pins the owner TypeScript SELECT KPI at 364 (before 367)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(src);
    expect(total).toBe(364);
    expect((read('sales-package-sources.ts').match(/SELECT /g) ?? []).length).toBe(11);
    expect((read('supply-package-sources.ts').match(/SELECT /g) ?? []).length).toBe(7);
    expect((read('read-tail-r25-dedupe-sql.ts').match(/SELECT /g) ?? []).length).toBe(1);
  });

  test('native source fragments own the byte-exact r25 literals while cross-owner consumers embed the TS text', () => {
    const plans = read('native_commerce_read_sql.rs');
    const kinds = [
      { kind: 'RD_SALES_SETTLEMENT_LINES', wire: 'commerce_rd_sales_settlement_lines' },
      { kind: 'RD_SUPPLY_SETTLEMENT_LINES', wire: 'commerce_rd_supply_settlement_lines' },
    ];
    for (const { kind, wire } of kinds) {
      expect(plans).toContain(`pub const ${kind}: &str = "${wire}";`);
      expect(plans).toContain(`kind: ${kind},`);
      expect(read('native-commerce-read.ts')).toContain(`export const ${kind} = '${wire}';`);
    }
    for (const site of FAMILY) expect(plans).toContain(`"${site.before}"`);
    // Source fragments are composite-query text, not standalone scoped reads:
    // they stay outside the executable command-plan registry until the
    // Sales/Procurement/Finance package consumers outside this task cut over.
    expect(plans).toContain('pub const SOURCE_FRAGMENTS: &[NativeCommerceReadStatement] = &[');
    expect(plans).toContain('pub fn source_fragment(kind: &str)');
    expect(plans).toContain('assert_eq!(STATEMENTS.len(), 53);');
    expect(plans).toContain('fn settlement_line_source_fragments_keep_their_exact_literals()');
    expect(norm(read('sales-package-sources.ts'))).toContain(norm(`lines:commerceSettlementLineSql("${SALES_TAIL}")`));
    expect(norm(read('supply-package-sources.ts'))).toContain(norm(`lines:commerceSettlementLineSql("${SUPPLY_TAIL}")`));
  });
});
