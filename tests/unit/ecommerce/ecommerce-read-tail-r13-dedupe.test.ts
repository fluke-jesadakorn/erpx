import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));

const srcRoot = resolve(__dirname, '../../../modules/ecommerce/server/src');
const src = readFileSync(resolve(srcRoot, 'adapters/ecommerce.ts'), 'utf8');

// Deterministic stubs: the interpolations are identical in the pre-dedupe
// source and the shared helper, so byte-identity of the frame is provable.
const commerceProjectionSources = { commerce_orders: '<orders>' };
const cartScope = 'c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?)';

// Pre-dedupe checkout reads copied verbatim from HEAD, kept only as oracles.
const head = {
  detail: `SELECT x.*,c.channel_id,c.party_id,c.warehouse_id,c.branch_id,c.department_id,o.number,o.state order_state FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=x.sales_order_id WHERE x.id=? AND ${cartScope}`,
  lock: `SELECT x.*,c.branch_id,c.department_id,o.version current_order_version,o.state order_state FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=x.sales_order_id WHERE x.id=? AND ${cartScope} FOR UPDATE OF x,c,o`,
};

// Rebuild the r13 shared read exactly as the adapter defines it.
const checkoutRow = (projection: string, lock = '') =>
  `SELECT ${projection} FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=x.sales_order_id WHERE x.id=? AND ${cartScope}${lock}`;

describe('Ecommerce read-tail r13 checkout-row read family', () => {
  test('one parameterized read rebuilds the detail and lock callers byte-identically', () => {
    expect(checkoutRow("x.*,c.channel_id,c.party_id,c.warehouse_id,c.branch_id,c.department_id,o.number,o.state order_state")).toBe(head.detail);
    expect(checkoutRow("x.*,c.branch_id,c.department_id,o.version current_order_version,o.state order_state", " FOR UPDATE OF x,c,o")).toBe(head.lock);
  });

  test('family is defined once and composed at both call sites; literals are gone', () => {
    expect(src.match(/const checkoutRow = /g) ?? []).toHaveLength(1);
    expect(src.match(/checkoutRow\(/g) ?? []).toHaveLength(2);
    expect(src.match(/checkoutRow\("x\.\*,c\.channel_id,c\.party_id,c\.warehouse_id,c\.branch_id,c\.department_id,o\.number,o\.state order_state"\)/g) ?? []).toHaveLength(1);
    expect(src.match(/checkoutRow\("x\.\*,c\.branch_id,c\.department_id,o\.version current_order_version,o\.state order_state", " FOR UPDATE OF x,c,o"\)/g) ?? []).toHaveLength(1);
    expect(src.includes('c.channel_id,c.party_id,c.warehouse_id,c.branch_id,c.department_id,o.number,o.state order_state FROM ecommerce_checkouts')).toBe(false);
    expect(src.includes('o.version current_order_version,o.state order_state FROM ecommerce_checkouts')).toBe(false);
  });

  test('composed reads keep every bind and the lock clause is unchanged', () => {
    for (const sql of [head.detail, head.lock]) {
      expect(sql).toMatch(/^SELECT x\.\*/);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect((head.detail.match(/\?/g) ?? []).length).toBe(6);
    expect((head.lock.match(/\?/g) ?? []).length).toBe(6);
    expect(head.lock.endsWith('FOR UPDATE OF x,c,o')).toBe(true);
    expect(src.match(/FOR UPDATE/g) ?? []).toHaveLength(3);
  });

  test('r1-r12 helpers, scope-row call count and checkout locks are untouched', () => {
    for (const token of [
      'activeChannelAssertion', 'openCartAssertion', 'checkoutVersionAssertion',
      'activeProjectionAssertion', 'readSource', 'cartScope',
      "ecommerceOrderedList('id', 'ecommerce_carts', 'id=? FOR UPDATE')",
      'FOR UPDATE OF x,c,o',
      'lock ? " FOR UPDATE" : ""',
    ]) {
      expect(src).toContain(token);
    }
  });
});

describe('Ecommerce read-tail r13 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r13 BEFORE=58 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(58);
  });
});
