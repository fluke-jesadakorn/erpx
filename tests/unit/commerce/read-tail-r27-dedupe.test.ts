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

/** r27 family: the settings-image witness read shared by the Expense and
 * existing Manufacturing posting-setup state snapshots. Both now delegate to
 * mounted native `commerce_rd_*_settings_image` command-plan kinds; the TS
 * renderer that prepared the shared head is deleted by this cutover. */
const HEAD = 'SELECT to_jsonb(s)::text image FROM commerce_settings s WHERE ';
const EXPENSE_TAIL = 's.company_id=?';
const MFG_TAIL = 'company_id=?';

const FAMILY: Site[] = [
  {
    id: 'expense-settings-image',
    file: 'expense-posting-setup.ts',
    sql: `${HEAD}${EXPENSE_TAIL}`,
    kind: 'commerce_rd_expense_settings_image',
    call: 'nativeCommerceReadFirst<string>(RD_EXPENSE_SETTINGS_IMAGE, [companyId], database)',
    digest: '69b010d0774d6c20ab35d2b3b4c99355cda2b6c86fbb4782c276f69741e8d1c6',
  },
  {
    id: 'mfg-existing-settings-image',
    file: 'manufacturing-existing-posting-setup.ts',
    sql: `${HEAD}${MFG_TAIL}`,
    kind: 'commerce_rd_mfg_existing_settings_image',
    call: 'nativeCommerceReadFirst<string>(RD_MFG_EXISTING_SETTINGS_IMAGE, [actor.companyId], database)',
    digest: '23756a35df50c65ba2ff2e5fbc0015335037358942968c7c0985994d926c09aa',
  },
];

describe('Commerce r27 settings-image family native cutover', () => {
  test('both posting-setup snapshots delegate to mounted native owner reads', () => {
    expect(existsSync(resolve(adapters, 'read-tail-r27-dedupe-sql.ts'))).toBe(false);
    expect(count(src, /SELECT to_jsonb\(s\)::text image FROM commerce_settings s WHERE /g)).toBe(0);
    for (const site of FAMILY) {
      const text = read(site.file);
      expect(norm(text), `${site.id} native call`).toContain(norm(site.call));
      expect(text, `${site.id} renderer import`).not.toContain('read-tail-r27-dedupe-sql');
      expect(text, `${site.id} renderer call`).not.toContain('commerceSettingsImageSql');
      expect(text, `${site.id} retired literal`).not.toContain(site.sql);
    }
    const bridge = read('native-commerce-read.ts');
    expect(bridge).toContain(
      "export const RD_EXPENSE_SETTINGS_IMAGE = 'commerce_rd_expense_settings_image';",
    );
    expect(bridge).toContain(
      "export const RD_MFG_EXISTING_SETTINGS_IMAGE = 'commerce_rd_mfg_existing_settings_image';",
    );
    expect(bridge).not.toContain('SELECT ');
    // Sibling SHARE ROW EXCLUSIVE locks and neighbouring source reads stay live.
    expect(read('expense-posting-setup.ts')).toContain('LOCK TABLE command_receipts,commerce_settings IN SHARE ROW EXCLUSIVE MODE');
    expect(read('manufacturing-existing-posting-setup.ts')).toContain('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE');
    expect(read('expense-posting-setup.ts')).toContain('SELECT to_jsonb(r)::text image FROM command_receipts r WHERE r.command_id=?');
    expect(read('manufacturing-existing-posting-setup.ts')).toContain("database.prepare('SELECT to_jsonb(s)::text image FROM commerce_mfg_existing_posting_confirmations s WHERE company_id=?')");
  });

  test('exact retired literals are owned by the native read registry', () => {
    const plans = read('native_commerce_read_sql.rs');
    for (const site of FAMILY) {
      expect(sha(site.sql), site.id).toBe(site.digest);
      expect(site.sql, site.id).toContain(HEAD);
      expect((site.sql.match(/SELECT /g) ?? []).length, site.id).toBe(1);
      expect(site.sql, site.id).not.toMatch(/FOR UPDATE|FOR SHARE|LOCK TABLE|;/);
      const constant = site.kind.replace('commerce_rd_', 'RD_').toUpperCase();
      expect(plans).toContain(`pub const ${constant}: &str = "${site.kind}";`);
      expect(plans).toContain(`"${site.sql}"`);
      expect(plans).toContain(`kind: ${constant},`);
    }
    expect(plans).toContain('assert_eq!(STATEMENTS.len(), 29);');
    expect(plans).toContain('fn posting_setup_settings_image_reads_keep_their_exact_literals()');
    expect(sha(FAMILY.map((site) => site.sql).join('\n'))).toBe('b41fa29dda25eb56b97ddf900caf59512e1d12211e5bca16954a5f844d2706a8');
  });

  test('protected renderers, row locks, native registries and accounting locks stay untouched', () => {
    expect(read('read-tail-r21-dedupe-sql.ts')).toContain('`SELECT ${columns} FROM commerce_orders WHERE ${tail}`');
    expect(read('read-tail-r25-dedupe-sql.ts')).toContain('`SELECT line.* FROM commerce_order_lines line JOIN commerce_orders parent ON parent.id=line.order_id WHERE ${tail}`');
    expect(existsSync(resolve(adapters, 'read-tail-r26-dedupe-sql.ts'))).toBe(false);
    expect(read('settings-dedupe-sql.ts')).toContain('SELECT\t* FROM commerce_settings WHERE company_id=?');
    expect(read('guarded-source-dedupe-sql.ts')).toContain("NOT EXISTS(SELECT 1 FROM commerce_settings WHERE company_id=?)");
    expect(read('guarded-source-dedupe-sql.ts')).toContain('`SELECT 1 FROM command_receipts WHERE ${tail}`');
    expect(read('attachment-parent-guard-sql.ts')).toContain('`EXISTS(SELECT 1 FROM commerce_fulfillments f WHERE f.${parent}=child.${child} AND f.order_id=o.id)`');
    expect(read('commerce.ts')).toContain("'SELECT id FROM commerce_returns WHERE id=? FOR UPDATE'");
    expect(read('commerce-goods-accounting.ts')).toContain('"SELECT id FROM commerce_fulfillments WHERE id=? FOR UPDATE"');
    expect(read('commerce-goods-accounting.ts')).toContain('SELECT * FROM commerce_invoice_goods_carrying WHERE invoice_id=? AND company_id=?');
    expect(read('sales-package-sources.ts')).toContain('LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,commerce_fulfillment_evidence,commerce_returns,commerce_return_lines,commerce_return_lot_allocations,commerce_return_stock_moves,commerce_goods_valuations,commerce_invoice_goods_carrying,commerce_stock,commerce_stock_moves,commerce_warehouses,commerce_products,commerce_settings IN SHARE MODE');
    expect(read('supply-package-sources.ts')).toContain('LOCK TABLE commerce_orders,commerce_order_lines,commerce_fulfillments,commerce_fulfillment_lines,commerce_returns,commerce_return_lines,commerce_warehouses,commerce_products,commerce_goods_valuations,commerce_invoice_goods_carrying IN SHARE MODE');
  });

  test('retirement:ts-statements pins the owner TypeScript SELECT KPI at 387 (before 388)', () => {
    expect(count(src, /SELECT /g)).toBe(387);
    expect((read('expense-posting-setup.ts').match(/SELECT /g) ?? []).length).toBe(1);
    expect((read('manufacturing-existing-posting-setup.ts').match(/SELECT /g) ?? []).length).toBe(2);
  });
});
