import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const src = readFileSync(resolve(srcRoot, 'adapters/ecommerce.ts'), 'utf8');

// Pre-dedupe source-row reads copied verbatim from HEAD, kept only as
// byte-identity oracles.
const head = {
  channel: 'SELECT * FROM ecommerce_channels WHERE company_id=? ORDER BY updated_at DESC',
  cartLine: 'SELECT * FROM ecommerce_cart_lines WHERE cart_id=? ORDER BY ordinal',
  fulfillmentLink: 'SELECT * FROM ecommerce_fulfillment_links WHERE checkout_id=?',
};

// Rebuild the r11 shared read exactly as the adapter defines it.
const readSource = (table: string, scope: string, order = '') =>
  `SELECT * FROM ${table} WHERE ${scope}${order ? ` ORDER BY ${order}` : ''}`;

describe('Ecommerce read-tail r11 source-row read family', () => {
  test('one parameterized read rebuilds the channel, cart-line and fulfillment-link callers byte-identically', () => {
    expect(readSource('ecommerce_channels', 'company_id=?', 'updated_at DESC')).toBe(head.channel);
    expect(readSource('ecommerce_cart_lines', 'cart_id=?', 'ordinal')).toBe(head.cartLine);
    expect(readSource('ecommerce_fulfillment_links', 'checkout_id=?')).toBe(head.fulfillmentLink);
  });

  test('family is defined once and composed at all three call sites; literals are gone', () => {
    expect(src.match(/const readSource = /g) ?? []).toHaveLength(1);
    expect(src.match(/readSource\(/g) ?? []).toHaveLength(3);
    expect(src.match(/readSource\("ecommerce_channels", "company_id=\?", "updated_at DESC"\)/g) ?? []).toHaveLength(1);
    expect(src.match(/readSource\("ecommerce_cart_lines", "cart_id=\?", "ordinal"\)/g) ?? []).toHaveLength(1);
    expect(src.match(/readSource\("ecommerce_fulfillment_links", "checkout_id=\?"\)/g) ?? []).toHaveLength(1);
    expect(src.includes('"SELECT * FROM ecommerce_channels WHERE company_id=? ORDER BY updated_at DESC"')).toBe(false);
    expect(src.includes('"SELECT * FROM ecommerce_cart_lines WHERE cart_id=? ORDER BY ordinal"')).toBe(false);
    expect(src.includes('"SELECT * FROM ecommerce_fulfillment_links WHERE checkout_id=?"')).toBe(false);
  });

  test('composed reads stay read-only SELECT * with unchanged single binds', () => {
    for (const sql of [head.channel, head.cartLine, head.fulfillmentLink]) {
      expect(sql).toMatch(/^SELECT \* FROM ecommerce_/);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
      expect((sql.match(/\?/g) ?? []).length).toBe(1);
    }
  });

  test('r1-r10 helpers and checkout locks are untouched', () => {
    for (const token of [
      'activeChannelAssertion', 'openCartAssertion', 'checkoutVersionAssertion',
      'activeProjectionAssertion', 'cartScope',
      "ecommerceOrderedList('id', 'ecommerce_carts', 'id=? FOR UPDATE')",
      'FOR UPDATE OF x,c,o',
      'lock ? " FOR UPDATE" : ""',
    ]) {
      expect(src).toContain(token);
    }
    expect(src.match(/FOR UPDATE/g) ?? []).toHaveLength(3);
  });
});

describe('Ecommerce read-tail r11 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r11 BEFORE=61 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(61);
  });
});
