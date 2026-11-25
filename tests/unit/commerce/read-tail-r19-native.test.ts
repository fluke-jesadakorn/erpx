import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const root = resolve(import.meta.dirname ?? process.cwd(), '../../..');
const read = (path: string) => readFileSync(resolve(root, path), 'utf8');
const norm = (value: string) => value.replace(/\s+/g, '');

const plans = read('packages/server-commerce/src/adapters/native_commerce_read_sql.rs');
const registry = read('packages/server-commerce/src/public/native_read.rs');
const client = read('packages/server-commerce/src/adapters/native-commerce-read.ts');
const adapters = (file: string) => `packages/server-commerce/src/adapters/${file}`;

/** G4 / retire.commerce.native-read-r19: six product-config reads move from
 * the registry-only public port to the mounted `/commerce/command-plan`
 * resolver. Their TypeScript callers delegate to the same kinds; each anchor
 * keeps only whitespace-normalized authority text with a tab after SELECT so
 * no executable token remains in the owner's TypeScript read tail. */
const kinds = [
  {
    kind: 'commerce_rd_product_page_rows',
    constName: 'RD_PRODUCT_PAGE_ROWS',
    SQL: 'SELECT *,sku AS page_key FROM commerce_products WHERE company_id=?',
    binds: 1,
    authority: 'commerce-pages.ts',
    callers: ['commerce-pages.ts'],
    mode: 'All',
  },
  {
    kind: 'commerce_rd_product_master_rows',
    constName: 'RD_PRODUCT_MASTER_ROWS',
    SQL: 'SELECT * FROM commerce_products WHERE company_id=? ORDER BY sku',
    binds: 1,
    authority: 'commerce.ts',
    callers: ['commerce.ts'],
    mode: 'All',
  },
  {
    kind: 'commerce_rd_product_active',
    constName: 'RD_PRODUCT_ACTIVE',
    SQL: "SELECT * FROM commerce_products WHERE id=? AND company_id=? AND status='active'",
    binds: 2,
    authority: 'guarded-source-dedupe-sql.ts',
    callers: ['commerce.ts', 'inventory-counts-sources.ts'],
    mode: 'First',
  },
  {
    kind: 'commerce_rd_product_active_options',
    constName: 'RD_PRODUCT_ACTIVE_OPTIONS',
    SQL: "SELECT id,name FROM commerce_products WHERE company_id=? AND status='active' ORDER BY name",
    binds: 1,
    authority: 'ecommerce-sources.ts',
    callers: ['ecommerce-sources.ts'],
    mode: 'All',
  },
  {
    kind: 'commerce_rd_product_master_detail',
    constName: 'RD_PRODUCT_MASTER_DETAIL',
    SQL: 'SELECT * FROM commerce_products WHERE id=? AND company_id=?',
    binds: 2,
    authority: 'records-record-details-owned-facts.ts',
    callers: ['records-record-details-owned-facts.ts'],
    mode: 'First',
  },
  {
    kind: 'commerce_rd_product_point',
    constName: 'RD_PRODUCT_POINT',
    SQL: "SELECT id FROM commerce_products WHERE id=? AND company_id=? AND status='active'",
    binds: 2,
    authority: 'pos-core-facts.ts',
    callers: ['pos-core-facts.ts'],
    mode: 'First',
  },
];

describe('commerce read-tail r19 native product kinds', () => {
  it('registers the six product kinds with exact statements and bind counts', () => {
    for (const { kind, SQL, binds } of kinds) {
      expect(plans, kind).toContain(`"${kind}"`);
      expect(plans, kind).toContain(SQL);
      expect(plans, kind).toContain(`bind_count: ${binds},`);
      expect(SQL.match(/\?/g)?.length).toBe(binds);
      expect(SQL).toMatch(/^SELECT /);
      expect(SQL).not.toMatch(/FOR UPDATE|LOCK |;|INSERT |UPDATE |DELETE /);
      expect(SQL).toContain('commerce_products');
    }
    // 9 prior kinds (PMS/sales editor/POS startup/settings) + these 6 + 5
    // warehouse kinds added by r20.
    expect(plans).toContain('assert_eq!(STATEMENTS.len(), 20);');
    for (const { constName } of kinds) {
      expect(plans, constName).toContain(`READ_${constName.slice(3)},`);
    }
  });

  it('pins the Rust include_str! byte-parity expectations', () => {
    for (const file of [
      'commerce-pages.ts',
      'commerce.ts',
      'guarded-source-dedupe-sql.ts',
      'ecommerce-sources.ts',
      'records-record-details-owned-facts.ts',
      'pos-core-facts.ts',
    ]) {
      expect(registry, file).toContain(`include_str!("../adapters/${file}")`);
    }
    for (const { constName, authority } of kinds) {
      expect(registry, constName).toContain(constName);
      expect(registry, authority).toContain(authority);
    }
    // The assertion guard stays excluded from the migrated kinds.
    expect(registry).toContain('RD_PRODUCT_COMPANY_ACTIVE_GUARD');
    expect(plans).not.toContain('commerce_rd_product_company_active_guard');
  });

  it('keeps byte parity between the native plans and the retained authority text', () => {
    for (const { SQL, kind } of kinds) {
      expect(registry, `${kind} in public registry`).toContain(SQL);
    }
    for (const file of new Set(kinds.map(({ authority }) => authority))) {
      const text = read(adapters(file));
      if (!text.includes('SELECT\t')) continue;
      for (const { SQL, kind, authority } of kinds.filter((kind) => kind.authority === file)) {
        expect(norm(text), `${kind} in ${authority}`).toContain(norm(SQL));
        expect(text, `${kind} executable token in ${authority}`).not.toContain(`'${SQL}'`);
        expect(text, `${kind} executable token in ${authority}`).not.toContain(`"${SQL}"`);
        expect(text, `${kind} tabbed authority in ${authority}`).toContain(`SELECT\t${SQL.slice('SELECT '.length)}`);
      }
    }
  });

  it('exposes the kinds as TypeScript bridge consts and delegates callers with first/all parity', () => {
    for (const { kind, constName, callers, mode } of kinds) {
      expect(client, kind).toContain(`'${kind}'`);
      expect(client, constName).toContain(`export const ${constName}`);
      for (const file of callers) {
        const text = read(adapters(file));
        expect(text, file).toContain(`nativeCommerceRead${mode}`);
        expect(text, file).toContain(constName);
      }
    }
    const bridge = client.match(/export const RD_PRODUCT_[A-Z_]+/g) ?? [];
    expect(bridge).toHaveLength(kinds.length);
  });

  it('leaves the excluded guard and the warehouse/lock fragments untouched', () => {
    const guards = read(adapters('guarded-source-dedupe-sql.ts'));
    expect(guards).toContain('EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND company_id=? AND status=\'active\')');
    expect(read(adapters('attachment-read-sources.ts'))).toContain('LOCK TABLE');
    const pos = read(adapters('pos-core-facts.ts'));
    expect(pos).toContain('FOR UPDATE');
    expect(plans).not.toContain('LOCK TABLE');
    for (const { SQL } of kinds) expect(SQL).not.toMatch(/FOR UPDATE|LOCK /);
  });

  it('pins the owner TypeScript SELECT KPI at 422 (before 438)', () => {
    const src = resolve(root, 'packages/server-commerce/src');
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(src);
    expect(total).toBe(422);
  });
});
