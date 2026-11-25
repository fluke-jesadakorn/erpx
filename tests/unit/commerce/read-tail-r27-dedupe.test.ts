import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { commerceSettingsImageSql } from '../../../packages/server-commerce/src/adapters/read-tail-r27-dedupe-sql';

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

/** r27 family: the two settings-image reads in the Expense and existing
 * Manufacturing posting-setup state witnesses. Both share the exact head
 * through `WHERE ` and differ only in their company predicate (one aliases
 * `s`, one does not). r18-r26 renderers cover different heads (order source,
 * settlement lines, receipt/guard frames, schema-ready probes) and are
 * deliberately not reused here. */
const HEAD = 'SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE ';
const EXPENSE_TAIL = 's.company_id=?';
const MFG_TAIL = 'company_id=?';

const FAMILY: Site[] = [
  {
    id: 'expense-settings-image',
    file: 'expense-posting-setup.ts',
    tail: EXPENSE_TAIL,
    before: `${HEAD}${EXPENSE_TAIL}`,
    after: commerceSettingsImageSql(EXPENSE_TAIL),
    call: `database.prepare(commerceSettingsImageSql('${EXPENSE_TAIL}'))`,
    digest: '69b010d0774d6c20ab35d2b3b4c99355cda2b6c86fbb4782c276f69741e8d1c6',
  },
  {
    id: 'mfg-existing-settings-image',
    file: 'manufacturing-existing-posting-setup.ts',
    tail: MFG_TAIL,
    before: `${HEAD}${MFG_TAIL}`,
    after: commerceSettingsImageSql(MFG_TAIL),
    call: `database.prepare(commerceSettingsImageSql('${MFG_TAIL}'))`,
    digest: '23756a35df50c65ba2ff2e5fbc0015335037358942968c7c0985994d926c09aa',
  },
];

describe('Commerce read-tail r27 settings-image head retirement', () => {
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
    expect(sha(FAMILY.map((site) => site.after).join('\n'))).toBe('b41fa29dda25eb56b97ddf900caf59512e1d12211e5bca16954a5f844d2706a8');
    expect(commerceSettingsImageSql(EXPENSE_TAIL)).not.toBe(commerceSettingsImageSql(MFG_TAIL));
  });

  test('both call sites delegate with unchanged tails, binds and locks', () => {
    for (const site of FAMILY) {
      const text = read(site.file);
      expect(norm(text), `${site.id} delegation`).toContain(norm(site.call));
      expect(norm(text), `${site.id} import`).toContain(norm("import { commerceSettingsImageSql } from './read-tail-r27-dedupe-sql';"));
      expect(text, `${site.id} retired literal`).not.toContain(site.before);
      expect(text, `${site.id} retired literal (quoted)`).not.toContain(`'${site.before}'`);
      expect(text, `${site.id} retired literal (quoted)`).not.toContain(`"${site.before}"`);
      expect(text.split('commerceSettingsImageSql(').length - 1, `${site.id} calls`).toBe(1);
    }
    // Binds stay at the call site; the renderer never hoists a value.
    expect(norm(read('expense-posting-setup.ts'))).toContain(norm(`commerceSettingsImageSql('${EXPENSE_TAIL}')).bind(companyId).first<string>('image')`));
    expect(norm(read('manufacturing-existing-posting-setup.ts'))).toContain(norm(`commerceSettingsImageSql('${MFG_TAIL}')).bind(actor.companyId).first<string>('image')`));
    // Sibling SHARE ROW EXCLUSIVE locks are byte-unchanged (not FOR SHARE).
    expect(read('expense-posting-setup.ts')).toContain('LOCK TABLE command_receipts,commerce_settings IN SHARE ROW EXCLUSIVE MODE');
    expect(read('manufacturing-existing-posting-setup.ts')).toContain('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE');
  });

  test('one executable head; near-miss settings reads stay inline', () => {
    let heads = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) heads += (readFileSync(p, 'utf8').match(/SELECT to_jsonb\(s\)::text image FROM commerce_settings s WHERE /g) ?? []).length;
      }
    };
    walk(src);
    expect(heads).toBe(1);
    const renderer = read('read-tail-r27-dedupe-sql.ts');
    expect(renderer.split('export const commerceSettingsImageSql =').length - 1).toBe(1);
    expect(renderer.split('`SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE ${tail}`').length - 1).toBe(1);
    // Same-table siblings with different columns/aliases are not this head.
    expect(read('settings-dedupe-sql.ts')).toContain('SELECT\t* FROM commerce_settings WHERE company_id=?');
    expect(read('guarded-source-dedupe-sql.ts')).toContain("NOT EXISTS(SELECT 1 FROM commerce_settings WHERE company_id=?)");
    expect(commerceSettingsImageSql('x')).not.toBe('SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE x ORDER BY s.company_id');
  });

  test('protected renderers, row locks, native registries and accounting locks stay untouched', () => {
    expect(read('read-tail-r21-dedupe-sql.ts')).toContain('`SELECT ${columns} FROM commerce_orders WHERE ${tail}`');
    expect(read('read-tail-r25-dedupe-sql.ts')).toContain('`SELECT line.* FROM commerce_order_lines line JOIN commerce_orders parent ON parent.id=line.order_id WHERE ${tail}`');
    expect(read('read-tail-r26-dedupe-sql.ts')).toContain('`SELECT to_jsonb(source)::text image FROM commerce_orders source WHERE ${tail}`');
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

  test('retirement:ts-statements pins the owner TypeScript SELECT KPI at 407 (before 413)', () => {
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
    expect((read('expense-posting-setup.ts').match(/SELECT /g) ?? []).length).toBe(1);
    expect((read('manufacturing-existing-posting-setup.ts').match(/SELECT /g) ?? []).length).toBe(2);
    expect((read('read-tail-r27-dedupe-sql.ts').match(/SELECT /g) ?? []).length).toBe(1);
    expect((read('settings-dedupe-sql.ts').match(/SELECT /g) ?? []).length).toBe(0);
  });
});
