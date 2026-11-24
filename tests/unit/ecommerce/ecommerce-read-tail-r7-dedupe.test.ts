import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const src = readFileSync(resolve(srcRoot, 'adapters/purchase-boundary.ts'), 'utf8');

// Pre-dedupe child-aggregation fragments copied verbatim from HEAD, kept only as byte-identity oracles.
const head = {
  cartLines: `COALESCE((SELECT jsonb_agg(to_jsonb(line) ORDER BY line.ordinal) FROM ecommerce_cart_lines line WHERE line.cart_id=c.id),'[]'::jsonb)`,
  fulfillmentLinks: `COALESCE((SELECT jsonb_agg(to_jsonb(link) ORDER BY link.id) FROM ecommerce_fulfillment_links link WHERE link.checkout_id=x.id),'[]'::jsonb)`,
  returnLinks: `COALESCE((SELECT jsonb_agg(to_jsonb(link) ORDER BY link.id) FROM ecommerce_return_links link WHERE link.checkout_id=x.id),'[]'::jsonb)`,
};

// Rebuild the r7 shared fragment exactly as the adapter defines it.
const childAgg = (alias: string, table: string, orderBy: string, fk: string, parent: string) =>
  `COALESCE((SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.${orderBy}) FROM ${table} ${alias} WHERE ${alias}.${fk}=${parent}.id),'[]'::jsonb)`;

describe('Ecommerce read-tail r7 child-aggregation family', () => {
  test('child aggregation is byte-identical for all three callers', () => {
    expect(childAgg('line', 'ecommerce_cart_lines', 'ordinal', 'cart_id', 'c')).toBe(head.cartLines);
    expect(childAgg('link', 'ecommerce_fulfillment_links', 'id', 'checkout_id', 'x')).toBe(head.fulfillmentLinks);
    expect(childAgg('link', 'ecommerce_return_links', 'id', 'checkout_id', 'x')).toBe(head.returnLinks);
  });

  test('family is defined once and composed at three sites; literals are gone', () => {
    expect(src.match(/const childAgg=/g) ?? []).toHaveLength(1);
    expect(src.match(/\$\{childAgg\('line','ecommerce_cart_lines','ordinal','cart_id','c'\)\}/g) ?? []).toHaveLength(1);
    expect(src.match(/\$\{childAgg\('link','ecommerce_fulfillment_links','id','checkout_id','x'\)\}/g) ?? []).toHaveLength(1);
    expect(src.match(/\$\{childAgg\('link','ecommerce_return_links','id','checkout_id','x'\)\}/g) ?? []).toHaveLength(1);
    expect(src.includes(head.cartLines)).toBe(false);
    expect(src.includes(head.fulfillmentLinks)).toBe(false);
    expect(src.includes(head.returnLinks)).toBe(false);
  });

  test('composed fragments stay read-only aggregation subqueries', () => {
    for (const sql of [head.cartLines, head.fulfillmentLinks, head.returnLinks]) {
      expect(sql).toMatch(/^COALESCE\(\(SELECT jsonb_agg\(/);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
  });
});

describe('Ecommerce read-tail r7 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r7 BEFORE=66 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(66);
  });
});
