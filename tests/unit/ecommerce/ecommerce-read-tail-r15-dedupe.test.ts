import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { ecommerceListingEvidence } from '../../../packages/server-ecommerce/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const ar = read('adapters/attachment-read.ts');
const ap = read('adapters/attachment-purchases.ts');
const shared = read('adapters/read-tail-sql.ts');
const src = read('adapters/ecommerce.ts');
const ao = read('adapters/attachments-operational-facts.ts');

// Pre-dedupe SQL copied verbatim from HEAD (before r15), kept as byte-identity oracles.
const head = {
  image: "SELECT jsonb_build_object('listing',to_jsonb(l),'channel',to_jsonb(c),'product',to_jsonb(p))::text image\n FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id JOIN commerce_products p ON p.id=l.product_id AND p.company_id=c.company_id WHERE l.id=? AND c.company_id=?",
  witness: "SELECT jsonb_build_array(to_jsonb(listing)-'version'-'updated_at',to_jsonb(channel),product.id,product.company_id)::text witness FROM ecommerce_listings listing JOIN ecommerce_channels channel ON channel.id=listing.channel_id JOIN commerce_products product ON product.id=listing.product_id AND product.company_id=channel.company_id WHERE listing.id=? AND channel.company_id=?",
};

describe('Ecommerce read-tail r15 listing-evidence dedupe', () => {
  test('parameterized frame rebuilds the attachment image read byte-identically', () => {
    expect(ecommerceListingEvidence("jsonb_build_object('listing',to_jsonb(l),'channel',to_jsonb(c),'product',to_jsonb(p))::text image\n")).toBe(head.image);
  });

  test('parameterized frame rebuilds the purchase witness read byte-identically', () => {
    expect(ecommerceListingEvidence("jsonb_build_array(to_jsonb(listing)-'version'-'updated_at',to_jsonb(channel),product.id,product.company_id)::text witness", 'listing', 'channel', 'product')).toBe(head.witness);
  });

  test('the family is defined once in the shared helper and composed at both callers; literals are gone', () => {
    expect((shared.match(/export const ecommerceListingEvidence = /g) ?? []).length).toBe(1);
    expect((ar.match(/ecommerceListingEvidence\(/g) ?? []).length).toBe(1);
    expect((ap.match(/ecommerceListingEvidence\(/g) ?? []).length).toBe(1);
    for (const oracle of Object.values(head)) {
      expect(ar).not.toContain(oracle);
      expect(ap).not.toContain(oracle);
      expect(shared).not.toContain(oracle);
    }
  });

  test('composed reads stay read-only with unchanged binds and no added locks', () => {
    for (const sql of Object.values(head)) {
      expect(sql).toMatch(/^SELECT /);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    for (const sql of Object.values(head)) expect((sql.match(/\?/g) ?? []).length).toBe(2);
    expect(ar).toMatch(/ecommerceCommerceSourceReadLocks/);
    expect(ap).toMatch(/LOCK TABLE commerce_products IN SHARE MODE/);
  });

  test('r1-r14 helpers, conditional FOR UPDATE and LOCK TABLE reads are untouched', () => {
    for (const token of [
      'ecommerceModuleAssignment', 'ecommerceScopeRow', 'ecommerceExistsGuard',
      'ecommerceOrderedList', 'ecommerceListingChannelRow', 'ecommerceSourceIdentity',
    ]) {
      expect(shared).toContain(token);
    }
    expect(shared.match(/export const ecommerceModuleAssignment =/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const ecommerceSourceIdentity =/g) ?? []).toHaveLength(1);
    expect(ap).toContain('sourceQuery');
    expect(ap).toContain('ecommerceCommerceSourceReadLocks');
    expect(src).toContain('SELECT id FROM ecommerce_carts WHERE id=? FOR UPDATE');
    expect(src).toContain('FOR UPDATE OF x,c,o');
    expect(src).toContain('lock ? " FOR UPDATE" : ""');
    expect(ap).toContain('LOCK TABLE commerce_products IN SHARE MODE');
    expect(ao).toContain('attachmentVersionGuardSql(');
  });
});

describe('Ecommerce read-tail r15 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r15 BEFORE=51 baseline (r22 pin 26)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(51);
    expect(total).toBe(26);
  });
});
