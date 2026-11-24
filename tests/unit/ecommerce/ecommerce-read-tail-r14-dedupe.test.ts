import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import {
  ecommerceExistsGuard,
  ecommerceOrderedList,
  ecommerceListingChannelRow,
  ecommerceSourceIdentity,
} from '../../../packages/server-ecommerce/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const src = read('adapters/ecommerce.ts');
const pb = read('adapters/purchase-boundary.ts');
const ao = read('adapters/attachments-operational-facts.ts');
const shared = read('adapters/read-tail-sql.ts');

// Deterministic stubs: interpolations are identical in the pre-dedupe source
// and the shared helper, so byte-identity of the frame is provable.
const commerceProjectionSources = { commerce_products: '<products>', commerce_orders: '<orders>' };
const cartScope = 'c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?)';

// Pre-dedupe SQL copied verbatim from HEAD, kept only as byte-identity oracles.
const head = {
  channelGuard: "EXISTS(SELECT 1 FROM ecommerce_channels WHERE id=? AND company_id=? AND status='active')",
  cartGuard: "EXISTS(SELECT 1 FROM ecommerce_carts WHERE id=? AND version=? AND status='open')",
  checkoutGuard: "EXISTS(SELECT 1 FROM ecommerce_checkouts WHERE id=? AND version=?)",
  listingList: `SELECT l.*,p.name product_name,c.name channel_name FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=l.product_id WHERE c.company_id=? ORDER BY l.updated_at DESC`,
  cartList: `SELECT c.* FROM ecommerce_carts c WHERE ${cartScope} ORDER BY c.updated_at DESC`,
  checkoutList: `SELECT x.*,o.number,o.state order_state FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=x.sales_order_id WHERE ${cartScope} ORDER BY x.updated_at DESC`,
  listingRow: "SELECT l.*,c.company_id,c.status channel_status FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id WHERE l.id=? AND c.company_id=?",
  attachmentRow: "SELECT c.company_id,NULL::text branch_id,NULL::text department_id,l.version FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id WHERE l.id=? AND c.company_id=?",
  sourceChannel: "SELECT x.company_id,NULL::text branch_id,NULL::text department_id,x.version,jsonb_build_array(x.id,x.company_id)::text identity,to_jsonb(x)::text image FROM ecommerce_channels x WHERE x.id=?",
  sourceListing: `SELECT c.company_id,NULL::text branch_id,NULL::text department_id,x.version,jsonb_build_array(x.id,x.channel_id,x.product_id,c.company_id)::text identity,jsonb_build_array(to_jsonb(x),to_jsonb(c),p.id,p.company_id)::text image FROM ecommerce_listings x JOIN ecommerce_channels c ON c.id=x.channel_id JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=x.product_id AND p.company_id=c.company_id WHERE x.id=?`,
};

describe('Ecommerce read-tail r14 four-family dedupe', () => {
  test('exists row guard rebuilds the channel, cart and checkout callers byte-identically', () => {
    expect(ecommerceExistsGuard('ecommerce_channels', "id=? AND company_id=? AND status='active'")).toBe(head.channelGuard);
    expect(ecommerceExistsGuard('ecommerce_carts', "id=? AND version=? AND status='open'")).toBe(head.cartGuard);
    expect(ecommerceExistsGuard('ecommerce_checkouts', 'id=? AND version=?')).toBe(head.checkoutGuard);
  });

  test('ordered list read rebuilds the listing, cart and checkout callers byte-identically', () => {
    expect(ecommerceOrderedList('l.*,p.name product_name,c.name channel_name', `ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=l.product_id`, 'c.company_id=?', 'l.updated_at DESC')).toBe(head.listingList);
    expect(ecommerceOrderedList('c.*', 'ecommerce_carts c', cartScope, 'c.updated_at DESC')).toBe(head.cartList);
    expect(ecommerceOrderedList('x.*,o.number,o.state order_state', `ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id JOIN (${commerceProjectionSources.commerce_orders}) o ON o.id=x.sales_order_id`, cartScope, 'x.updated_at DESC')).toBe(head.checkoutList);
  });

  test('listing-channel row read rebuilds the detail and attachment callers byte-identically', () => {
    expect(ecommerceListingChannelRow('l.*,c.company_id,c.status channel_status')).toBe(head.listingRow);
    expect(ecommerceListingChannelRow('c.company_id,NULL::text branch_id,NULL::text department_id,l.version')).toBe(head.attachmentRow);
  });

  test('source-identity read rebuilds the channel and listing callers byte-identically', () => {
    expect(ecommerceSourceIdentity('x.company_id,NULL::text branch_id,NULL::text department_id,x.version,jsonb_build_array(x.id,x.company_id)::text identity,to_jsonb(x)::text image', 'ecommerce_channels x')).toBe(head.sourceChannel);
    expect(ecommerceSourceIdentity('c.company_id,NULL::text branch_id,NULL::text department_id,x.version,jsonb_build_array(x.id,x.channel_id,x.product_id,c.company_id)::text identity,jsonb_build_array(to_jsonb(x),to_jsonb(c),p.id,p.company_id)::text image', `ecommerce_listings x JOIN ecommerce_channels c ON c.id=x.channel_id JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=x.product_id AND p.company_id=c.company_id`)).toBe(head.sourceListing);
  });

  test('each family is defined once in the shared helper and composed at every call site; literals are gone', () => {
    for (const [name, count] of [['ecommerceExistsGuard', 1], ['ecommerceOrderedList', 1], ['ecommerceListingChannelRow', 1], ['ecommerceSourceIdentity', 1]] as const) {
      expect(shared.match(new RegExp(`export const ${name} = `, 'g')) ?? []).toHaveLength(count);
    }
    expect((src.match(/ecommerceExistsGuard\(/g) ?? []).length).toBe(3);
    expect((src.match(/ecommerceOrderedList\(/g) ?? []).length).toBe(3);
    expect((src.match(/ecommerceListingChannelRow\(/g) ?? []).length).toBe(1);
    expect((ao.match(/ecommerceListingChannelRow\(/g) ?? []).length).toBe(1);
    expect((pb.match(/ecommerceSourceIdentity\(/g) ?? []).length).toBe(2);
    for (const oracle of Object.values(head)) {
      expect(src).not.toContain(oracle);
      expect(ao).not.toContain(oracle);
      expect(pb).not.toContain(oracle);
    }
  });

  test('composed reads stay read-only with unchanged binds and no added locks', () => {
    for (const sql of Object.values(head)) {
      expect(sql).toMatch(/^EXISTS\(SELECT 1 FROM |^SELECT /);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    for (const sql of [head.channelGuard, head.cartGuard, head.checkoutGuard]) expect((sql.match(/\?/g) ?? []).length).toBe(2);
    expect((head.listingList.match(/\?/g) ?? []).length).toBe(1);
    expect((head.cartList.match(/\?/g) ?? []).length).toBe(5);
    expect((head.checkoutList.match(/\?/g) ?? []).length).toBe(5);
    for (const sql of [head.listingRow, head.attachmentRow]) expect((sql.match(/\?/g) ?? []).length).toBe(2);
    for (const sql of [head.sourceChannel, head.sourceListing]) expect((sql.match(/\?/g) ?? []).length).toBe(1);
    expect(src.match(/FOR UPDATE/g) ?? []).toHaveLength(3);
    expect(pb).not.toMatch(/FOR UPDATE|LOCK TABLE|FOR SHARE/i);
    expect(ao).not.toMatch(/FOR UPDATE|LOCK TABLE|FOR SHARE/i);
    expect(shared).not.toMatch(/FOR UPDATE|LOCK TABLE|FOR SHARE/i);
  });

  test('r1-r13 helpers and conditional FOR UPDATE reads are untouched', () => {
    for (const token of [
      'activeChannelAssertion', 'openCartAssertion', 'checkoutVersionAssertion',
      'activeProjectionAssertion', 'readSource', 'checkoutRow', 'cartScope',
      'SELECT id FROM ecommerce_carts WHERE id=? FOR UPDATE',
      'FOR UPDATE OF x,c,o',
      'lock ? " FOR UPDATE" : ""',
    ]) {
      expect(src).toContain(token);
    }
    expect(shared.match(/export const ecommerceModuleAssignment =/g) ?? []).toHaveLength(1);
    expect(shared.match(/export const ecommerceScopeRow =/g) ?? []).toHaveLength(1);
    expect(ao.match(/attachmentVersionGuardSql\(/g) ?? []).toHaveLength(2);
    expect(pb.match(/const childAgg=/g) ?? []).toHaveLength(1);
  });
});

describe('Ecommerce read-tail r14 retirement check', () => {
  test('retirement:ts-statements stays strictly below the r14 BEFORE=57 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(57);
    expect(total).toBe(51);
  });
});
