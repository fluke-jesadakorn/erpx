import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { jsonbAggFact, bindingsExists } from '../../../modules/customer-authority/server/src/adapters/customer-authority-read-sql';

const srcRoot = resolve(__dirname, '../../../modules/customer-authority/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Pre-dedupe fragments copied verbatim from HEAD, used only as byte-identity oracles.
const headAgg = {
  r: '(SELECT jsonb_agg(to_jsonb(r) ORDER BY r.id)',
  a: '(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id)',
  c: '(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id)',
  b: '(SELECT jsonb_agg(to_jsonb(b) ORDER BY b.id)',
  o: '(SELECT jsonb_agg(to_jsonb(o) ORDER BY o.id)',
  f: '(SELECT jsonb_agg(to_jsonb(f) ORDER BY f.id)',
} as const;
const headExists = {
  o: 'EXISTS(SELECT 1 FROM bindings b WHERE b.order_id=o.id AND b.company_id=o.company_id)',
  f: 'EXISTS(SELECT 1 FROM bindings b WHERE b.order_id=f.order_id AND b.company_id=o.company_id)',
  a: 'EXISTS(SELECT 1 FROM bindings b WHERE b.id=a.binding_id)',
} as const;
const predicates = {
  o: 'b.order_id=o.id AND b.company_id=o.company_id',
  f: 'b.order_id=f.order_id AND b.company_id=o.company_id',
  a: 'b.id=a.binding_id',
} as const;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

describe('Customer Authority read-tail r21 byte-identical families', () => {
  test('jsonbAggFact reproduces every pre-dedupe projection literal', () => {
    for (const [alias, head] of Object.entries(headAgg)) {
      expect(jsonbAggFact(alias), alias).toBe(head);
      readOnly(jsonbAggFact(alias));
    }
  });

  test('bindingsExists reproduces every pre-dedupe existence probe', () => {
    for (const key of Object.keys(headExists) as (keyof typeof headExists)[]) {
      expect(bindingsExists(predicates[key]), key).toBe(headExists[key]);
      readOnly(bindingsExists(predicates[key]));
    }
  });

  test('the shared module owns the only literal copies and is read-only', () => {
    const family = src('customer-authority-read-sql.ts');
    expect(family.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    const caller = src('portal-projection-unit.ts');
    expect(caller).not.toContain('(SELECT jsonb_agg(to_jsonb(');
    expect(caller).not.toContain('EXISTS(SELECT 1 FROM bindings b WHERE');
    expect(caller.match(/jsonbAggFact\(/g) ?? []).toHaveLength(7);
    expect(caller.match(/bindingsExists\(/g) ?? []).toHaveLength(3);
  });

  test('caller suffixes keep the composed projection byte-identical', () => {
    const caller = src('portal-projection-unit.ts');
    // Source holds the interpolation verbatim; builder output + suffix together
    // reproduce the exact pre-dedupe projection text from HEAD.
    expect(caller).toContain("${jsonbAggFact('b')} FROM bindings b)");
    expect(caller).toContain("${jsonbAggFact('o')} FROM (${commerce.commerce_orders}) o WHERE ${bindingsExists('b.order_id=o.id AND b.company_id=o.company_id')})");
    expect(caller).toContain("${jsonbAggFact('f')} FROM (${commerce.commerce_fulfillments}) f");
    expect(caller).toContain("${jsonbAggFact('a')} FROM customer_portal_actions a WHERE ${bindingsExists('b.id=a.binding_id')}),${support})::text core");
    for (const key of Object.keys(headExists) as (keyof typeof headExists)[]) readOnly(headExists[key]);
  });
});

describe('Customer Authority read-tail r21 retirement check', () => {
  test('retirement:ts-statements stays strictly below the BEFORE=78 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(78);
  });
});
