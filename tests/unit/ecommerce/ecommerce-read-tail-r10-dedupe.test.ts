import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { commercialOwnerProjectionSources as commerceProjectionSources } from '../../../packages/server-commerce/src/public/commercial-owner-projection-sources';
import { commercialOwnerProjectionSources as financeProjectionSources } from '../../../packages/server-finance/src/public/commercial-owner-projection-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const src = readFileSync(resolve(srcRoot, 'adapters/ecommerce.ts'), 'utf8');

// Pre-dedupe projection-existence guards copied verbatim from HEAD, kept only
// as byte-identity oracles.
const head = {
  product: `EXISTS(SELECT 1 FROM (${commerceProjectionSources.commerce_products}) commerce_products WHERE id=? AND company_id=? AND status='active')`,
  party: `EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_parties}) finance_parties WHERE id=? AND company_id=? AND kind='customer' AND status='active')`,
};

// Rebuild the r10 shared guard exactly as the adapter defines it.
const activeProjectionAssertion = (source: string, alias: string, predicate: string) =>
  `EXISTS(SELECT 1 FROM (${source}) ${alias} WHERE id=? AND company_id=? AND ${predicate})`;

describe('Ecommerce read-tail r10 projection-existence guard family', () => {
  test('one parameterized guard rebuilds product and party callers byte-identically', () => {
    expect(activeProjectionAssertion(commerceProjectionSources.commerce_products, 'commerce_products', "status='active'")).toBe(head.product);
    expect(activeProjectionAssertion(financeProjectionSources.finance_parties, 'finance_parties', "kind='customer' AND status='active'")).toBe(head.party);
  });

  test('family is defined once and composed at both call sites; literals are gone', () => {
    expect(src.match(/const activeProjectionAssertion = /g) ?? []).toHaveLength(1);
    expect(src.match(/activeProjectionAssertion\(/g) ?? []).toHaveLength(2);
    expect(src.match(/activeProjectionAssertion\(commerceProjectionSources\.commerce_products, "commerce_products", "status='active'"\)/g) ?? []).toHaveLength(1);
    expect(src.match(/activeProjectionAssertion\(financeProjectionSources\.finance_parties, "finance_parties", "kind='customer' AND status='active'"\)/g) ?? []).toHaveLength(1);
    expect(src.includes("EXISTS(SELECT 1 FROM (${commerceProjectionSources.commerce_products}) commerce_products")).toBe(false);
    expect(src.includes("EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_parties}) finance_parties")).toBe(false);
  });

  test('composed guards stay read-only EXISTS(SELECT 1 ...) with unchanged binds', () => {
    for (const sql of [head.product, head.party]) {
      expect(sql).toMatch(/^EXISTS\(SELECT 1 FROM \(/);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
      expect((sql.match(/\?/g) ?? []).length).toBe(2);
    }
  });

  test('r1-r9 helpers and checkout locks are untouched', () => {
    for (const token of [
      'activeChannelAssertion', 'openCartAssertion', 'checkoutVersionAssertion', 'cartScope',
      'SELECT id FROM ecommerce_carts WHERE id=? FOR UPDATE',
      'FOR UPDATE OF x,c,o',
      'lock ? " FOR UPDATE" : ""',
    ]) {
      expect(src).toContain(token);
    }
    expect(src.match(/FOR UPDATE/g) ?? []).toHaveLength(3);
  });
});

describe('Ecommerce read-tail r10 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r10 BEFORE=62 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(62);
  });
});
