import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { commerceFulfillmentKindSql } from '../../../packages/server-commerce/src/adapters/read-tail-r29-dedupe-sql';

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

/** r29 family: the fulfillment/order kind projection head shared by the
 * `fulfillmentKind` and `invoiceKind` entrypoint readers in entrypoint-facts.ts.
 * Both share the exact head through `WHERE ` and differ only in the predicate
 * tail. r18-r28 renderers cover different heads (order source, settlement
 * lines, receipt/guard frames, schema probes, settings image, products master)
 * and are deliberately not reused. */
const HEAD = 'SELECT o.kind FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE ';
const FULFILLMENT_TAIL = 'f.id=? AND o.company_id=?';
const INVOICE_TAIL = 'f.invoice_id=? AND o.company_id=?';

const FAMILY: Site[] = [
  {
    id: 'fulfillment-kind',
    file: 'entrypoint-facts.ts',
    tail: FULFILLMENT_TAIL,
    before: `${HEAD}${FULFILLMENT_TAIL}`,
    after: commerceFulfillmentKindSql(FULFILLMENT_TAIL),
    call: `db.prepare(commerceFulfillmentKindSql('${FULFILLMENT_TAIL}'))`,
    digest: '8079026b02d02b095c7083b250d59a88c8039c505ea441b11a6972f66c28a7c0',
  },
  {
    id: 'invoice-kind',
    file: 'entrypoint-facts.ts',
    tail: INVOICE_TAIL,
    before: `${HEAD}${INVOICE_TAIL}`,
    after: commerceFulfillmentKindSql(INVOICE_TAIL),
    call: `db.prepare(commerceFulfillmentKindSql('${INVOICE_TAIL}'))`,
    digest: 'a1b585c657e5d1f7da79fcfc77ab31fee5308e262776d5ada3f64381b044ec84',
  },
];

describe('Commerce read-tail r29 fulfillment/order kind head retirement', () => {
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
      '8c28b6eb60894e8ec8c740e78b459c0789b119e3e8e3b7ebc852845a7391a72d',
    );
    expect(commerceFulfillmentKindSql(FULFILLMENT_TAIL)).not.toBe(commerceFulfillmentKindSql(INVOICE_TAIL));
    expect(sha(HEAD)).toBe('367eff609622433a0dc1c2090b7340bdbecbf00cd5ef2ff9ef50292a5a207618');
  });

  test('both call sites delegate with unchanged tails, binds and sibling reads', () => {
    for (const site of FAMILY) {
      const text = read(site.file);
      expect(norm(text), `${site.id} delegation`).toContain(norm(site.call));
      expect(norm(text), `${site.id} import`).toContain(norm("import {commerceFulfillmentKindSql} from \"./read-tail-r29-dedupe-sql\";"));
      expect(text, `${site.id} retired literal`).not.toContain(site.before);
      expect(text, `${site.id} retired literal (quoted)`).not.toContain(`'${site.before}'`);
      expect(text, `${site.id} retired literal (quoted)`).not.toContain(`"${site.before}"`);
      expect(text.split('commerceFulfillmentKindSql(').length - 1, `${site.id} calls`).toBe(2);
    }
    // Binds stay at the call site; the renderer never hoists a value.
    const text = read('entrypoint-facts.ts');
    expect(norm(text)).toContain(norm(`commerceFulfillmentKindSql('${FULFILLMENT_TAIL}')).bind(id,companyId).first<{kind:string}>()`));
    expect(norm(text)).toContain(norm(`commerceFulfillmentKindSql('${INVOICE_TAIL}')).bind(id,companyId).first<{kind:string}>()`));
    // Near-miss joins and other inline entrypoint reads stay byte-unchanged.
    expect(text).toContain("'SELECT o.kind FROM commerce_returns r JOIN commerce_orders o ON o.id=r.order_id WHERE r.id=? AND o.company_id=?'");
    expect(text).toContain("'SELECT kind FROM commerce_orders WHERE id=? AND company_id=?'");
    expect(text).toContain("'SELECT kind,state FROM commerce_orders WHERE id=? AND company_id=?'");
    expect(text).toContain("products:'SELECT id,sku,name FROM commerce_products'");
  });

  test('one executable head; near-miss return join stays inline', () => {
    let heads = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) heads += (readFileSync(p, 'utf8').match(/SELECT o\.kind FROM commerce_fulfillments f JOIN commerce_orders o ON o\.id=f\.order_id WHERE /g) ?? []).length;
      }
    };
    walk(src);
    expect(heads).toBe(1);
    const renderer = read('read-tail-r29-dedupe-sql.ts');
    expect(renderer.split('export const commerceFulfillmentKindSql =').length - 1).toBe(1);
    expect(renderer.split('`SELECT o.kind FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE ${tail}`').length - 1).toBe(1);
    expect(commerceFulfillmentKindSql('x')).not.toBe('SELECT o.kind FROM commerce_fulfillments f JOIN commerce_orders o ON o.id=f.order_id WHERE x ORDER BY f.id');
  });

  test('protected renderers, row locks, native registries and accounting locks stay untouched', () => {
    expect(read('read-tail-r21-dedupe-sql.ts')).toContain('`SELECT ${columns} FROM commerce_orders WHERE ${tail}`');
    expect(read('read-tail-r25-dedupe-sql.ts')).toContain('`SELECT line.* FROM commerce_order_lines line JOIN commerce_orders parent ON parent.id=line.order_id WHERE ${tail}`');
    expect(read('read-tail-r26-dedupe-sql.ts')).toContain('`SELECT to_jsonb(source)::text image FROM commerce_orders source WHERE ${tail}`');
    expect(read('read-tail-r27-dedupe-sql.ts')).toContain('`SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE ${tail}`');
    expect(read('read-tail-r28-dedupe-sql.ts')).toContain('`SELECT id,sku,name,unit,tracking_mode FROM commerce_products WHERE ${tail}`');
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

  test('retirement:ts-statements pins the owner TypeScript SELECT KPI at 406 (before 407)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(src);
    expect(total).toBe(406);
    expect((read('entrypoint-facts.ts').match(/SELECT /g) ?? []).length).toBe(6);
    expect((read('read-tail-r29-dedupe-sql.ts').match(/SELECT /g) ?? []).length).toBe(1);
  });
});
