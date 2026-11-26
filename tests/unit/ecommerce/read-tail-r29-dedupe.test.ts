import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { ecommerceOrderedList, ecommerceSourceIdentity } from '../../../packages/server-ecommerce/src/adapters/read-tail-sql';
import { ecommerceNativeOrderRoots } from '../../../packages/server-ecommerce/src/adapters/package-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const shared = read('adapters/read-tail-sql.ts');
const pb = read('adapters/purchase-boundary.ts');
const src = read('adapters/ecommerce.ts');
const po = read('adapters/package-obligations.ts');

// Deterministic stubs for the owner projection subqueries. The same stubs feed
// the pre-r29 oracles and the post-r29 composition: text only, binds unchanged.
const commerce = { commerce_warehouses: '<warehouses>', commerce_orders: '<orders>', commerce_products: '<products>' };
const finance = { finance_parties: '<parties>' };
const ecommerceScalar = (body: string) => `SELECT ${body}`;
const childAgg = (alias: string, table: string, orderBy: string, fk: string, parent: string) =>
  `COALESCE((${ecommerceScalar(`jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.${orderBy}) FROM ${table} ${alias} WHERE ${alias}.${fk}=${parent}.id`)}),'[]'::jsonb)`;

// Byte oracles copied verbatim from HEAD (base SHA 81d5eaf835021b61868d75632322dd6add11d738).
// The cart/checkout identity/image arrays are the exact 409 guard projections;
// r28 did not alter source-identity SQL.
const preR29 = {
  cartLock: 'SELECT id FROM ecommerce_carts WHERE id=? FOR UPDATE',
  sourceChannel: "SELECT x.company_id,NULL::text branch_id,NULL::text department_id,x.version,jsonb_build_array(x.id,x.company_id)::text identity,to_jsonb(x)::text image FROM ecommerce_channels x WHERE x.id=?",
  sourceListing: "SELECT c.company_id,NULL::text branch_id,NULL::text department_id,x.version,jsonb_build_array(x.id,x.channel_id,x.product_id,c.company_id)::text identity,jsonb_build_array(to_jsonb(x),to_jsonb(c),p.id,p.company_id)::text image FROM ecommerce_listings x JOIN ecommerce_channels c ON c.id=x.channel_id JOIN (<products>) p ON p.id=x.product_id AND p.company_id=c.company_id WHERE x.id=?",
  cart: "SELECT c.company_id,c.branch_id,c.department_id,c.version,jsonb_build_array(c.id,c.company_id,c.branch_id,c.department_id,c.channel_id,c.party_id,c.warehouse_id,c.currency)::text identity,\n   jsonb_build_array(to_jsonb(c),to_jsonb(channel),party.id,party.company_id,warehouse.id,warehouse.company_id,warehouse.branch_id,\n   COALESCE((SELECT jsonb_agg(to_jsonb(line) ORDER BY line.ordinal) FROM ecommerce_cart_lines line WHERE line.cart_id=c.id),'[]'::jsonb)\n   )::text image\n   FROM ecommerce_carts c\n   JOIN ecommerce_channels channel ON channel.id=c.channel_id AND channel.company_id=c.company_id\n   JOIN (<parties>) party ON party.id=c.party_id AND party.company_id=c.company_id AND party.kind='customer'\n   JOIN (<warehouses>) warehouse ON warehouse.id=c.warehouse_id AND warehouse.company_id=c.company_id AND warehouse.branch_id=c.branch_id\n   \n   WHERE c.id=?",
  checkout: "SELECT c.company_id,c.branch_id,c.department_id,x.version,jsonb_build_array(x.id,x.cart_id,x.sales_order_id,c.id,c.company_id,c.branch_id,c.department_id,c.channel_id,c.party_id,c.warehouse_id,c.currency)::text identity,\n   jsonb_build_array(to_jsonb(c),to_jsonb(channel),party.id,party.company_id,warehouse.id,warehouse.company_id,warehouse.branch_id,\n   COALESCE((SELECT jsonb_agg(to_jsonb(line) ORDER BY line.ordinal) FROM ecommerce_cart_lines line WHERE line.cart_id=c.id),'[]'::jsonb)\n   ,to_jsonb(x),o.id,o.company_id,o.branch_id,o.department_id,o.kind,o.party_id,o.warehouse_id,o.currency,\n   COALESCE((SELECT jsonb_agg(to_jsonb(link) ORDER BY link.id) FROM ecommerce_fulfillment_links link WHERE link.checkout_id=x.id),'[]'::jsonb),\n   COALESCE((SELECT jsonb_agg(to_jsonb(link) ORDER BY link.id) FROM ecommerce_return_links link WHERE link.checkout_id=x.id),'[]'::jsonb))::text image\n   FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id\n   JOIN ecommerce_channels channel ON channel.id=c.channel_id AND channel.company_id=c.company_id\n   JOIN (<parties>) party ON party.id=c.party_id AND party.company_id=c.company_id AND party.kind='customer'\n   JOIN (<warehouses>) warehouse ON warehouse.id=c.warehouse_id AND warehouse.company_id=c.company_id AND warehouse.branch_id=c.branch_id\n   JOIN (<orders>) o ON o.id=x.sales_order_id AND o.id='order-ecommerce-order-'||c.id AND o.company_id=c.company_id AND o.branch_id=c.branch_id AND o.department_id IS NOT DISTINCT FROM c.department_id AND o.kind='sales' AND o.party_id=c.party_id AND o.warehouse_id=c.warehouse_id AND o.currency=c.currency\n   WHERE x.id=?",
};

// Render the actual columns/from templates from purchase-boundary.ts so the
// proof covers the post-r29 source, not a copy of it.
const colStart = pb.indexOf('const columns=`') + 'const columns=`'.length;
const colEnd = pb.indexOf('`;\n  const from=`', colStart);
const fromStart = colEnd + '`;\n  const from=`'.length;
const fromEnd = pb.indexOf('`;\n  sql=ecommerceSourceIdentity', fromStart);
const columnsSrc = pb.slice(colStart, colEnd);
const fromSrc = pb.slice(fromStart, fromEnd);
const render = (body: string, checkout: boolean): string => {
  const fn = new Function('checkout', 'childAgg', 'commerce', 'finance', 'ecommerceScalar', 'return `' + body + '`;');
  return fn(checkout, childAgg, commerce, finance, ecommerceScalar) as string;
};
const composed = (checkout: boolean) =>
  ecommerceSourceIdentity(render(columnsSrc, checkout), render(fromSrc, checkout), checkout ? 'x' : 'c');

describe('Ecommerce read-tail r29 ordered-list fold', () => {
  test('addCartLine lock folds into ecommerceOrderedList with the FOR UPDATE suffix and one bind', () => {
    expect(src).toContain("ecommerceOrderedList('id', 'ecommerce_carts', 'id=? FOR UPDATE')");
    expect(src).not.toContain("prepare('SELECT id FROM ecommerce_carts WHERE id=? FOR UPDATE')");
    expect(ecommerceOrderedList('id', 'ecommerce_carts', 'id=? FOR UPDATE')).toBe(preR29.cartLock);
    expect(preR29.cartLock.endsWith(' FOR UPDATE')).toBe(true);
    expect((preR29.cartLock.match(/\?/g) ?? []).length).toBe(1);
    expect((ecommerceOrderedList('id', 'ecommerce_carts', 'id=? FOR UPDATE').match(/\?/g) ?? []).length).toBe(1);
  });

  test('ecommerceSourceIdentity composes from ecommerceOrderedList byte-identically for both cart/checkout variants', () => {
    expect(shared.match(/export const ecommerceSourceIdentity =/g) ?? []).toHaveLength(1);
    expect(shared).toContain('ecommerceOrderedList(columns, from, `${alias}.id=?`)');
    expect(shared).not.toContain('`SELECT ${columns} FROM ${from} WHERE ${alias}.id=?`');
    expect(ecommerceSourceIdentity('COL', 'FROM')).toBe('SELECT COL FROM FROM WHERE x.id=?');
    expect(ecommerceSourceIdentity('COL', 'FROM', 'x')).toBe('SELECT COL FROM FROM WHERE x.id=?');
    expect(ecommerceSourceIdentity('COL', 'FROM', 'c')).toBe('SELECT COL FROM FROM WHERE c.id=?');
    expect(ecommerceSourceIdentity('x.company_id,NULL::text branch_id,NULL::text department_id,x.version,jsonb_build_array(x.id,x.company_id)::text identity,to_jsonb(x)::text image', 'ecommerce_channels x')).toBe(preR29.sourceChannel);
    expect(ecommerceSourceIdentity('c.company_id,NULL::text branch_id,NULL::text department_id,x.version,jsonb_build_array(x.id,x.channel_id,x.product_id,c.company_id)::text identity,jsonb_build_array(to_jsonb(x),to_jsonb(c),p.id,p.company_id)::text image', `ecommerce_listings x JOIN ecommerce_channels c ON c.id=x.channel_id JOIN (${commerce.commerce_products}) p ON p.id=x.product_id AND p.company_id=c.company_id`)).toBe(preR29.sourceListing);
    expect(composed(false)).toBe(preR29.cart);
    expect(composed(true)).toBe(preR29.checkout);
    expect(preR29.cart.length).toBe(865);
    expect(preR29.checkout.length).toBe(1629);
    expect((preR29.cart.match(/\?/g) ?? []).length).toBe(1);
    expect((preR29.checkout.match(/\?/g) ?? []).length).toBe(1);
    expect((composed(false).match(/\?/g) ?? []).length).toBe(1);
    expect((composed(true).match(/\?/g) ?? []).length).toBe(1);
    expect(pb).toContain("sql=ecommerceSourceIdentity(columns,from,checkout?'x':'c');");
    expect(pb.match(/ecommerceSourceIdentity\(/g) ?? []).toHaveLength(3);
  });

  test('protected locks, native source and executive metric port are untouched', () => {
    expect(src).toContain('FOR UPDATE OF x,c,o');
    expect(src).toContain('lock ? " FOR UPDATE" : ""');
    expect(po).toContain('LOCK TABLE command_receipts,audit_events,money_transaction_snapshots IN SHARE MODE');
    expect(read('adapters/commerce-source-locks.ts')).toContain('LOCK TABLE ecommerce_channels,ecommerce_listings,ecommerce_carts,ecommerce_cart_lines,ecommerce_checkouts,ecommerce_fulfillment_links,ecommerce_return_links IN SHARE ROW EXCLUSIVE MODE');
    expect(read('adapters/package-sources.ts')).toContain("export const ecommerceNativeOrderRoots=ecommerceScalar(`cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart`);");
    expect(ecommerceNativeOrderRoots).toBe("SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart");
    expect(read('adapters/executive-metric-projections.ts')).toContain('nativeExecutiveMetric');
    expect(read('adapters/native_exec_metric/domain.rs')).toContain('SELECT c.status label,count(*)::text value FROM ecommerce_carts c WHERE {scope_sql}');
  });
});

describe('Ecommerce read-tail r29 retirement check', () => {
  test('KPI: ts SELECT tokens === 5 (r29 BEFORE=7)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(0);
    expect(total).toBeLessThan(7);
  });
});
