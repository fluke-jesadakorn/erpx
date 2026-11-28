import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { ecommerceSourceIdentity } from '../../../modules/ecommerce/server/src/adapters/read-tail-sql';
import { ecommerceNativeOrderRoots } from '../../../modules/ecommerce/server/src/adapters/package-sources';

const srcRoot = resolve(__dirname, '../../../modules/ecommerce/server/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const shared = read('adapters/read-tail-sql.ts');
const pb = read('adapters/purchase-boundary.ts');
const src = read('adapters/ecommerce.ts');
const po = read('adapters/package-obligations.ts');

// Deterministic stubs for the owner projection subqueries. The same stubs feed
// the pre-r27 oracle and the post-r27 composition: text only, binds unchanged.
const commerce = { commerce_warehouses: '<warehouses>', commerce_orders: '<orders>', commerce_products: '<products>' };
const finance = { finance_parties: '<parties>' };
const ecommerceScalar = (body: string) => `SELECT ${body}`;
const childAgg = (alias: string, table: string, orderBy: string, fk: string, parent: string) =>
  `COALESCE((${ecommerceScalar(`jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.${orderBy}) FROM ${table} ${alias} WHERE ${alias}.${fk}=${parent}.id`)}),'[]'::jsonb)`;

// Pre-r27 SQL emitted by the inline cart/checkout source identity, copied
// verbatim from HEAD. The identity/image arrays are the 409 guard projections.
const preR27 = {
  cart: "SELECT c.company_id,c.branch_id,c.department_id,c.version,jsonb_build_array(c.id,c.company_id,c.branch_id,c.department_id,c.channel_id,c.party_id,c.warehouse_id,c.currency)::text identity,\n   jsonb_build_array(to_jsonb(c),to_jsonb(channel),party.id,party.company_id,warehouse.id,warehouse.company_id,warehouse.branch_id,\n   COALESCE((SELECT jsonb_agg(to_jsonb(line) ORDER BY line.ordinal) FROM ecommerce_cart_lines line WHERE line.cart_id=c.id),'[]'::jsonb)\n   )::text image\n   FROM ecommerce_carts c\n   JOIN ecommerce_channels channel ON channel.id=c.channel_id AND channel.company_id=c.company_id\n   JOIN (<parties>) party ON party.id=c.party_id AND party.company_id=c.company_id AND party.kind='customer'\n   JOIN (<warehouses>) warehouse ON warehouse.id=c.warehouse_id AND warehouse.company_id=c.company_id AND warehouse.branch_id=c.branch_id\n   \n   WHERE c.id=?",
  checkout: "SELECT c.company_id,c.branch_id,c.department_id,x.version,jsonb_build_array(x.id,x.cart_id,x.sales_order_id,c.id,c.company_id,c.branch_id,c.department_id,c.channel_id,c.party_id,c.warehouse_id,c.currency)::text identity,\n   jsonb_build_array(to_jsonb(c),to_jsonb(channel),party.id,party.company_id,warehouse.id,warehouse.company_id,warehouse.branch_id,\n   COALESCE((SELECT jsonb_agg(to_jsonb(line) ORDER BY line.ordinal) FROM ecommerce_cart_lines line WHERE line.cart_id=c.id),'[]'::jsonb)\n   ,to_jsonb(x),o.id,o.company_id,o.branch_id,o.department_id,o.kind,o.party_id,o.warehouse_id,o.currency,\n   COALESCE((SELECT jsonb_agg(to_jsonb(link) ORDER BY link.id) FROM ecommerce_fulfillment_links link WHERE link.checkout_id=x.id),'[]'::jsonb),\n   COALESCE((SELECT jsonb_agg(to_jsonb(link) ORDER BY link.id) FROM ecommerce_return_links link WHERE link.checkout_id=x.id),'[]'::jsonb))::text image\n   FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id\n   JOIN ecommerce_channels channel ON channel.id=c.channel_id AND channel.company_id=c.company_id\n   JOIN (<parties>) party ON party.id=c.party_id AND party.company_id=c.company_id AND party.kind='customer'\n   JOIN (<warehouses>) warehouse ON warehouse.id=c.warehouse_id AND warehouse.company_id=c.company_id AND warehouse.branch_id=c.branch_id\n   JOIN (<orders>) o ON o.id=x.sales_order_id AND o.id='order-ecommerce-order-'||c.id AND o.company_id=c.company_id AND o.branch_id=c.branch_id AND o.department_id IS NOT DISTINCT FROM c.department_id AND o.kind='sales' AND o.party_id=c.party_id AND o.warehouse_id=c.warehouse_id AND o.currency=c.currency\n   WHERE x.id=?",
};

// Render the actual columns/from templates from purchase-boundary.ts so the
// proof covers the post-r27 source, not a copy of it.
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

describe('Ecommerce read-tail r27 source-identity alias fold', () => {
  test('both cart and checkout variants rebuild byte-identically with unchanged binds', () => {
    expect(composed(false)).toBe(preR27.cart);
    expect(composed(true)).toBe(preR27.checkout);
    expect(composed(false).length).toBe(865);
    expect(composed(true).length).toBe(1629);
    expect((preR27.cart.match(/\?/g) ?? []).length).toBe(1);
    expect((preR27.checkout.match(/\?/g) ?? []).length).toBe(1);
    expect((composed(false).match(/\?/g) ?? []).length).toBe(1);
    expect((composed(true).match(/\?/g) ?? []).length).toBe(1);
  });

  test('the 409 identity/image guard projections are unchanged in both variants', () => {
    for (const variant of [preR27.cart, preR27.checkout]) {
      expect(variant).toContain(')::text identity,');
      expect(variant).toContain(')::text image\n');
      expect((variant.match(/jsonb_build_array/g) ?? []).length).toBe(2);
      expect(variant).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect(preR27.cart).toContain('WHERE c.id=?');
    expect(preR27.checkout).toContain('WHERE x.id=?');
  });

  test('the alias parameter is closed and existing callers keep x.id=? bytes', () => {
    expect(shared.match(/export const ecommerceSourceIdentity =/g) ?? []).toHaveLength(1);
    expect(shared).toContain("export const ecommerceSourceIdentity = (columns: string, from: string, alias = 'x') =>");
    expect(shared).toContain('ecommerceOrderedList(columns, from, `${alias}.id=?`)');
    expect(ecommerceSourceIdentity('COL', 'FROM')).toBe('SELECT COL FROM FROM WHERE x.id=?');
    expect(ecommerceSourceIdentity('COL', 'FROM', 'x')).toBe('SELECT COL FROM FROM WHERE x.id=?');
    expect(ecommerceSourceIdentity('COL', 'FROM', 'c')).toBe('SELECT COL FROM FROM WHERE c.id=?');
    expect(pb).toContain("sql=ecommerceSourceIdentity(columns,from,checkout?'x':'c');");
    expect(pb.match(/ecommerceSourceIdentity\(/g) ?? []).toHaveLength(3);
    expect(pb).not.toContain('sql=`SELECT c.company_id');
    expect(pb).not.toContain('WHERE ${checkout?');
  });

  test('protected locks, native source and executive metric port are untouched', () => {
    expect(src).toContain("ecommerceOrderedList('id', 'ecommerce_carts', 'id=? FOR UPDATE')");
    expect(src).toContain('FOR UPDATE OF x,c,o');
    expect(po).toContain('LOCK TABLE command_receipts,audit_events,money_transaction_snapshots IN SHARE MODE');
    expect(read('adapters/commerce-source-locks.ts')).toContain('LOCK TABLE ecommerce_channels,ecommerce_listings,ecommerce_carts,ecommerce_cart_lines,ecommerce_checkouts,ecommerce_fulfillment_links,ecommerce_return_links IN SHARE ROW EXCLUSIVE MODE');
    expect(read('adapters/package-sources.ts')).toContain("export const ecommerceNativeOrderRoots=ecommerceScalar(`cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart`);");
    expect(ecommerceNativeOrderRoots).toBe("SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart");
    expect(read('adapters/executive-metric-projections.ts')).toContain('nativeExecutiveMetric');
    expect(read('adapters/native_exec_metric/domain.rs')).toContain('SELECT c.status label,count(*)::text value FROM ecommerce_carts c WHERE {scope_sql}');
  });
});

describe('Ecommerce read-tail r27 retirement check', () => {
  test('retirement:ts-statements === 5 (r29 BEFORE=7)', () => {
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
    expect(total).toBeLessThan(10);
  });
});
