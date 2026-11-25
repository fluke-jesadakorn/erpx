import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { commerceOrdersReadSql } from '../../../packages/server-commerce/src/adapters/read-tail-r21-dedupe-sql';

const root = resolve(import.meta.dirname ?? process.cwd(), '../../..');
const adapters = resolve(root, 'packages/server-commerce/src/adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const norm = (src: string) => src.replace(/\s+/g, ' ');
const q = (s: string) => (s.includes("'") ? `"${s.replaceAll('"', '\\"')}"` : `'${s}'`);
const call = (cs: Case) => `commerceOrdersReadSql(${q(cs.columns)}, ${q(cs.tail)})`;

type Case = {
  file: string;
  columns: string;
  tail: string;
  expected: string;
  bind: string;
  label: string;
};

const scope = 'company_id,branch_id,department_id,version';
const c = (file: string, tail: string, label: string): Case => ({
  file, columns: scope, tail,
  expected: `SELECT ${scope} FROM commerce_orders WHERE ${tail}`,
  bind: '.bind(facts.targetId,facts.companyId)', label,
});
const a = (file: string, tail: string, label: string): Case => ({
  file, columns: '*', tail,
  expected: `SELECT * FROM commerce_orders WHERE ${tail}`,
  bind: '', label,
});

const AF = 'attachments-operational-facts.ts';
const AP = 'attachments-policy-facts.ts';
const RP = 'record-purchases.ts';
const SP = 'sales-package-sources.ts';
const PP = 'supply-package-sources.ts';

const CASES: Case[] = [
  c(AF, "id=? AND company_id=? AND kind IN ('quote','sales')", 'operational readCommerceOrders2'),
  c(AP, "id=? AND company_id=? AND kind='sales'", 'policy readSalesDelivery'),
  c(AP, "id=? AND company_id=? AND kind='purchase'", 'policy readPurchaseOrder'),
  a(RP, 'company_id=? ORDER BY updated_at DESC', 'purchases reference rows'),
  a(SP, "kind IN ('quote','sales')", 'sales settlement orders'),
  a(PP, "kind='purchase'", 'supply settlement orders'),
];

// Reconstructed independently of the helper body: the closed byte oracle.
const oracle = (cs: Case) => `SELECT ${cs.columns} FROM commerce_orders WHERE ${cs.tail}`;

describe('Commerce read-tail r21 order-head renderer', () => {
  test('all 6 sites render byte-identically to the retired literals', () => {
    expect(CASES).toHaveLength(6);
    for (const cs of CASES) {
      expect(commerceOrdersReadSql(cs.columns, cs.tail)).toBe(cs.expected);
      expect(commerceOrdersReadSql(cs.columns, cs.tail)).toBe(oracle(cs));
      expect((cs.expected.match(/SELECT /g) ?? []).length).toBe(1);
      expect(cs.expected).toMatch(/^SELECT (?:company_id,branch_id,department_id,version|\*) FROM commerce_orders WHERE /);
      expect(cs.expected).not.toMatch(/FOR UPDATE|FOR SHARE|LOCK /);
    }
    expect(commerceOrdersReadSql('*', "kind='purchase'")).not.toBe(commerceOrdersReadSql(scope, "kind='purchase'"));
  });

  test('callers delegate with unchanged columns, tails and binds', () => {
    for (const cs of CASES) {
      const src = read(cs.file);
      expect(norm(src), cs.label).toContain(norm(call(cs)));
      expect(src, cs.label).not.toContain(`'${cs.expected}'`);
      expect(src, cs.label).not.toContain(`"${cs.expected}"`);
      expect(norm(src), cs.label).toContain(norm("import { commerceOrdersReadSql } from"));
      if (cs.bind) expect(norm(src), cs.label).toContain(norm(cs.bind));
    }
    // The distinct source witnesses keep their own binds; no bind was hoisted.
    expect(norm(read(RP))).toContain(norm('.bind(actor.companyId)'));
    expect(read(SP)).toContain('orders:commerceOrdersReadSql');
    expect(read(PP)).toContain('orders:commerceOrdersReadSql');
  });

  test('delegation counts match the 1 + 2 + 1 + 1 + 1 sites', () => {
    for (const file of [AF, RP, SP, PP]) expect(read(file).split('commerceOrdersReadSql(').length - 1).toBe(1);
    expect(read(AP).split('commerceOrdersReadSql(').length - 1).toBe(2);
  });
});

describe('Commerce read-tail r21 protected families and retirement', () => {
  test('aliased, state-bearing and warehouse/lock statements stay inline', () => {
    // Near-miss heads are deliberately not folded.
    expect(read(AF)).toContain('SELECT company_id,branch_id,department_id,version,state FROM commerce_orders');
    expect(read(AP)).toContain('SELECT o.company_id,o.branch_id,o.department_id,o.version FROM commerce_orders o');
    expect(read(AP)).toContain('FROM ${descriptor.table} WHERE id=? AND company_id=?${extra}');
    expect(read(SP)).toContain('COMMERCE_WAREHOUSE_IDENTITY_SQL');
    expect(read(PP)).toContain('COMMERCE_WAREHOUSE_IDENTITY_SQL');
    const plans = read('native_commerce_read_sql.rs');
    expect(plans).toContain('assert_eq!(STATEMENTS.len(), 20);');
  });

  test('pins the owner TypeScript SELECT KPI at 407 (before 413)', () => {
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
    expect(total).toBe(407);
  });
});
