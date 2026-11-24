import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import {
  ecommerceOrderedList,
  ecommerceScalar,
  ecommerceScopeRow,
} from '../../../packages/server-ecommerce/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const shared = read('adapters/read-tail-sql.ts');
const src = read('adapters/ecommerce.ts');
const po = read('adapters/package-obligations.ts');
const pb = read('adapters/purchase-boundary.ts');
const emp = read('adapters/executive-metric-projections.ts');
const ap = read('adapters/attachment-purchases.ts');

const orders = '<orders>';
const products = '<products>';
const cartScope = 'c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?::text IS NULL OR c.department_id=?)';

// Verbatim pre-r26 interpolations, used only to resolve the byte oracle on
// both sides of each frame gate. Text only: binds and locks stay unchanged.
function assignmentReadSql(alias: string, permission: string) {
  return `EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=${alias}.company_id
    AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_role_allows(g.role,g.company_id,'${permission}')
    AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=${alias}.branch_id OR g.scope_kind='department' AND g.scope_node_id=${alias}.department_id))`;
}

// Pre-r26 SQL copied verbatim from HEAD, kept only as byte-identity oracles.
const preR26 = {
  checkoutDetail: `SELECT x.*,c.channel_id,c.party_id,c.warehouse_id,c.branch_id,c.department_id,o.number,o.state order_state FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id JOIN (${orders}) o ON o.id=x.sales_order_id WHERE x.id=? AND ${cartScope}`,
  checkoutLock: `SELECT x.*,c.branch_id,c.department_id,o.version current_order_version,o.state order_state FROM ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id JOIN (${orders}) o ON o.id=x.sales_order_id WHERE x.id=? AND ${cartScope} FOR UPDATE OF x,c,o`,
  cartOpen: `SELECT c.* FROM ecommerce_carts c WHERE c.id=? AND ${cartScope}`,
  cartLocked: `SELECT c.* FROM ecommerce_carts c WHERE c.id=? AND ${cartScope} FOR UPDATE`,
  reviewChannel: 'SELECT x.* FROM ecommerce_channels x  WHERE x.id=? AND x.company_id=?',
  reviewListing: 'SELECT x.*,c.company_id FROM ecommerce_listings x JOIN ecommerce_channels c ON c.id=x.channel_id WHERE x.id=? AND c.company_id=?',
  cartsCte: 'SELECT cart.* FROM ecommerce_carts cart,scope WHERE cart.company_id=scope.company_id',
  checkoutsCte: 'SELECT checkout.*,cart.company_id,cart.branch_id,cart.department_id,cart.party_id,cart.warehouse_id,cart.currency,cart.version current_cart_version FROM ecommerce_checkouts checkout JOIN carts cart ON cart.id=checkout.cart_id',
  nodeBranch: `COALESCE((SELECT n.display_name FROM (${'<nodes>'}) n WHERE n.id=c.branch_id AND n.company_id=c.company_id),'Branch')`,
  childLines: `COALESCE((SELECT jsonb_agg(to_jsonb(line) ORDER BY line.ordinal) FROM ecommerce_cart_lines line WHERE line.cart_id=c.id),'[]'::jsonb)`,
  scopeCompany: '(SELECT ?::text company_id)',
  scopeBranch: '(SELECT ?::text company_id,?::text branch_id,?::text department_id) ecommerce_file_review',
  returnLinks: `SELECT l.*,r.status current_return_status,r.version current_return_version FROM ecommerce_return_links l JOIN (${products}) r ON r.id=l.commerce_return_id WHERE l.checkout_id=?`,
};

describe('Ecommerce read-tail r26 three-rank dedupe', () => {
  test('F1: three keyed single-row reads rebuild from ecommerceOrderedList byte-identically', () => {
    expect(ecommerceOrderedList('x.*,c.channel_id,c.party_id,c.warehouse_id,c.branch_id,c.department_id,o.number,o.state order_state', `ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id JOIN (${orders}) o ON o.id=x.sales_order_id`, `x.id=? AND ${cartScope}`)).toBe(preR26.checkoutDetail);
    expect(ecommerceOrderedList('x.*,c.branch_id,c.department_id,o.version current_order_version,o.state order_state', `ecommerce_checkouts x JOIN ecommerce_carts c ON c.id=x.cart_id JOIN (${orders}) o ON o.id=x.sales_order_id`, `x.id=? AND ${cartScope} FOR UPDATE OF x,c,o`)).toBe(preR26.checkoutLock);
    expect(ecommerceOrderedList('c.*', 'ecommerce_carts c', `c.id=? AND ${cartScope}`)).toBe(preR26.cartOpen);
    expect(ecommerceOrderedList('c.*', 'ecommerce_carts c', `c.id=? AND ${cartScope} FOR UPDATE`)).toBe(preR26.cartLocked);
    expect(ecommerceOrderedList('x.*', 'ecommerce_channels x ', 'x.id=? AND x.company_id=?')).toBe(preR26.reviewChannel);
    expect(ecommerceOrderedList('x.*,c.company_id', 'ecommerce_listings x JOIN ecommerce_channels c ON c.id=x.channel_id', 'x.id=? AND c.company_id=?')).toBe(preR26.reviewListing);
    for (const sql of [preR26.checkoutDetail, preR26.checkoutLock, preR26.cartOpen, preR26.cartLocked, preR26.reviewChannel, preR26.reviewListing]) {
      expect((sql.match(/\?/g) ?? []).length).toBeGreaterThan(0);
    }
    expect((preR26.checkoutDetail.match(/\?/g) ?? []).length).toBe(6);
    expect((preR26.checkoutLock.match(/\?/g) ?? []).length).toBe(6);
    expect((preR26.cartOpen.match(/\?/g) ?? []).length).toBe(6);
    expect((preR26.cartLocked.match(/\?/g) ?? []).length).toBe(6);
    expect((preR26.reviewChannel.match(/\?/g) ?? []).length).toBe(2);
    expect((preR26.reviewListing.match(/\?/g) ?? []).length).toBe(2);
    expect((src.match(/ecommerceOrderedList\(/g) ?? []).length).toBe(7);
  });

  test('F2: optional WHERE rebuilds both CTE frames byte-identically with zero binds', () => {
    expect(ecommerceOrderedList('cart.*', 'ecommerce_carts cart,scope', 'cart.company_id=scope.company_id')).toBe(preR26.cartsCte);
    expect(ecommerceOrderedList('checkout.*,cart.company_id,cart.branch_id,cart.department_id,cart.party_id,cart.warehouse_id,cart.currency,cart.version current_cart_version', 'ecommerce_checkouts checkout JOIN carts cart ON cart.id=checkout.cart_id')).toBe(preR26.checkoutsCte);
    expect((preR26.cartsCte.match(/\?/g) ?? []).length).toBe(0);
    expect((preR26.checkoutsCte.match(/\?/g) ?? []).length).toBe(0);
    expect(ecommerceOrderedList('*', 't')).toBe('SELECT * FROM t');
    expect(ecommerceOrderedList('*', 't', '')).toBe('SELECT * FROM t');
    expect(ecommerceOrderedList('*', 't', 'x=?')).toBe('SELECT * FROM t WHERE x=?');
    expect(ecommerceOrderedList('*', 't', 'x=?', '')).toBe('SELECT * FROM t WHERE x=?');
    expect(ecommerceOrderedList('*', 't', 'x=?', 'ordinal')).toBe('SELECT * FROM t WHERE x=? ORDER BY ordinal');
    expect(po).toContain('carts AS MATERIALIZED(${ecommerceOrderedList(\'cart.*\',\'ecommerce_carts cart,scope\',\'cart.company_id=scope.company_id\')})');
    expect(po).toContain('checkouts AS MATERIALIZED(${ecommerceOrderedList(\'checkout.*,cart.company_id');
    expect(po).not.toContain('carts AS MATERIALIZED(SELECT cart.* FROM ecommerce_carts cart,scope');
    expect(po).not.toContain('checkouts AS MATERIALIZED(SELECT checkout.*,cart.company_id');
    expect(po).toContain("import {ecommerceScopeRow,ecommerceOrderedList,ecommerceSelectOne} from './read-tail-sql';");
  });

  test('F3: the scalar head rebuilds label, child-aggregate, scope and reviewer frames byte-identically', () => {
    expect(ecommerceScalar('n.display_name FROM (<nodes>) n WHERE n.id=c.branch_id AND n.company_id=c.company_id')).toBe('SELECT n.display_name FROM (<nodes>) n WHERE n.id=c.branch_id AND n.company_id=c.company_id');
    expect(`COALESCE((${ecommerceScalar(`n.display_name FROM (${'<nodes>'}) n WHERE n.id=c.branch_id AND n.company_id=c.company_id`)}),'Branch')`).toBe(preR26.nodeBranch);
    expect(`COALESCE((${ecommerceScalar("jsonb_agg(to_jsonb(line) ORDER BY line.ordinal) FROM ecommerce_cart_lines line WHERE line.cart_id=c.id")}),'[]'::jsonb)`).toBe(preR26.childLines);
    expect(ecommerceScopeRow('', false, '')).toBe(preR26.scopeCompany);
    expect(ecommerceScopeRow('ecommerce_file_review', true, ' ')).toBe(preR26.scopeBranch);
    const reviewSql = (projection: string, scope: string) =>
      ecommerceScalar(`(${assignmentReadSql('ecommerce_file_review','records.review')} OR ${assignmentReadSql('ecommerce_file_review','commerce.approve')}) ${projection}FROM ${scope}`);
    expect(reviewSql('allowed ', ecommerceScopeRow('ecommerce_file_review', true))).toBe(`SELECT (${assignmentReadSql('ecommerce_file_review','records.review')} OR ${assignmentReadSql('ecommerce_file_review','commerce.approve')}) allowed FROM ${ecommerceScopeRow('ecommerce_file_review', true)}`);
    expect(reviewSql('allowed ', ecommerceScopeRow('ecommerce_file_review', true))).toHaveLength(1296);
    expect((reviewSql('allowed ', ecommerceScopeRow('ecommerce_file_review', true)).match(/\?/g) ?? []).length).toBe(9);
    expect((shared.match(/export const ecommerceScalar = /g) ?? []).length).toBe(1);
    expect(shared).toContain('`(${ecommerceScalar(`?::text company_id${branch ? \',?::text branch_id,?::text department_id\' : \'\'}`)})${separator}${alias}`');
    expect(emp).toContain('const nodeNameLabel = ');
    expect(emp).toContain('COALESCE((${ecommerceScalar(');
    expect(emp).not.toContain('COALESCE((SELECT n.display_name');
    expect(pb).toContain('const childAgg=');
    expect(pb).toContain('COALESCE((${ecommerceScalar(');
    expect(pb).not.toContain('COALESCE((SELECT jsonb_agg');
    expect(ap).toContain('const reviewScopeSql = ');
    expect(ap).toContain('ecommerceScalar(`(${assignmentReadSql(');
    expect(ap).not.toContain('`SELECT (${assignmentReadSql(');
  });

  test('F4: locks, native bridge and r1-r25 frame sites stay intact', () => {
    for (const token of [
      'ecommerceSelectOne', 'ecommerceScalar', 'ecommerceModuleAssignment', 'ecommerceRowProbe',
      'ecommerceScopeRow', 'ecommerceReleaseGate', 'ecommerceExistsGuard', 'ecommerceOrderedList',
      'ecommerceListingFrame', 'ecommerceListingChannelRow', 'ecommerceSourceIdentity', 'ecommerceListingEvidence',
    ]) {
      expect(shared).toContain(`export const ${token} =`);
    }
    expect(src).toContain('SELECT id FROM ecommerce_carts WHERE id=? FOR UPDATE');
    expect(src).toContain('FOR UPDATE OF x,c,o');
    expect(src).toContain('lock ? " FOR UPDATE" : ""');
    expect((src.match(/FOR UPDATE/g) ?? []).length).toBe(3);
    expect((src.match(/nativeWrite\(/g) ?? []).length).toBeGreaterThan(0);
    expect(src).not.toContain('`SELECT ${projection} FROM ecommerce_checkouts');
    expect(src).not.toContain('`SELECT c.* FROM ecommerce_carts c WHERE c.id=?');
    expect(src).not.toContain('`SELECT x.*${kind === "listing"');
    expect(read('adapters/commerce-source-locks.ts')).toContain('LOCK TABLE ecommerce_channels,ecommerce_listings,ecommerce_carts,ecommerce_cart_lines,ecommerce_checkouts,ecommerce_fulfillment_links,ecommerce_return_links IN SHARE ROW EXCLUSIVE MODE');
    expect(ap).toContain('LOCK TABLE commerce_products IN SHARE MODE');
    expect(ecommerceOrderedList('l.*,r.status current_return_status,r.version current_return_version', `ecommerce_return_links l JOIN (${products}) r ON r.id=l.commerce_return_id`, 'l.checkout_id=?')).toBe(preR26.returnLinks);
  });

  test('F5: composed frames stay read-only', () => {
    for (const [name, sql] of Object.entries(preR26)) {
      if (name === 'checkoutLock' || name === 'cartLocked') {
        expect(sql).toMatch(/FOR UPDATE( OF x,c,o)?$/);
        continue;
      }
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect(src).toContain('.bind(id, ...scopeValues(a)).first<Row>()');
    expect(src).toContain('.bind(id, a.companyId).first<Row>()');
    expect(po).toContain('.bind(input.companyId).all<{kind:string;id:string;fact:unknown;pending:boolean|null}>()');
  });
});

describe('Ecommerce read-tail r26 retirement check', () => {
  test('retirement:ts-statements === 11 (r26 BEFORE=19; r25 BEFORE=22)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT/g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBe(11);
    expect(total).toBeLessThan(19);
  });
});
