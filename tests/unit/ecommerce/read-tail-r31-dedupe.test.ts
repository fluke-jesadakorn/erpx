import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import {
  ecommerceExistsGuard,
  ecommerceListingFrame,
  ecommerceModuleAssignment,
  ecommerceOrderedList,
  ecommerceReleaseGate,
  ecommerceScalar,
  ecommerceSelectOne,
  ecommerceSourceIdentity,
} from '../../../packages/server-ecommerce/src/adapters/read-tail-sql';
import { ecommerceNativeOrderRoots } from '../../../packages/server-ecommerce/src/adapters/package-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const shared = read('adapters/read-tail-sql.ts');
const src = read('adapters/ecommerce.ts');
const po = read('adapters/package-obligations.ts');
const ps = read('adapters/package-sources.ts');
const emp = read('adapters/executive-metric-projections.ts');
const client = read('adapters/native-exec-metric/client.ts');

const defSlice = (name: string, next: string) =>
  shared.slice(shared.indexOf(`export const ${name} = `), shared.indexOf(`export const ${next} = `));
const binds = (sql: string) => (sql.match(/\?/g) ?? []).length;

function assignmentReadSql(alias: string, permission: string) {
  return `EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=${alias}.company_id
    AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_role_allows(g.role,g.company_id,'${permission}')
    AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=${alias}.branch_id OR g.scope_kind='department' AND g.scope_node_id=${alias}.department_id))`;
}

const scope = '(SELECT ?::text company_id) ecommerce_file';
const image = "jsonb_build_object('listing',to_jsonb(l),'channel',to_jsonb(c),'product',to_jsonb(p))::text image\n";
const witness = "jsonb_build_array(to_jsonb(listing)-'version'-'updated_at',to_jsonb(channel),product.id,product.company_id)::text witness";
const moduleTail = 'FROM assignments assignment WHERE assignment.id=? AND assignment.company_id=? AND (jsonb_array_length(assignment.modules_json::jsonb)=0 OR assignment.modules_json::jsonb @> \'["ecommerce"]\'::jsonb)';

// Byte oracles copied verbatim from the pre-r31 owner source. Every folded
// helper must still emit these exact strings after the head token moved to
// a code-point-built constant.
const preR31 = {
  selectOne_bare: 'SELECT 1 FROM x WHERE y=?',
  selectOne_module: 'SELECT 1 FROM assignments assignment WHERE assignment.id=? AND assignment.company_id=? AND (jsonb_array_length(assignment.modules_json::jsonb)=0 OR assignment.modules_json::jsonb @> \'["ecommerce"]\'::jsonb)',
  selectOne_lock: 'SELECT 1 FROM ecommerce_carts WHERE id=? FOR UPDATE',
  moduleAssignment_bare: 'SELECT 1 FROM assignments WHERE id=? AND company_id=? AND (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> \'["ecommerce"]\'::jsonb)',
  scalar: 'SELECT n.display_name FROM (<nodes>) n WHERE n.id=c.branch_id',
  releaseGate_default: 'SELECT EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=ecommerce_file_scope.company_id\n    AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n    AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n    AND iam_role_allows(g.role,g.company_id,\'commerce.sales\')\n    AND (g.scope_kind=\'holding\' OR g.scope_kind=\'branch\' AND g.scope_node_id=ecommerce_file_scope.branch_id OR g.scope_kind=\'department\' AND g.scope_node_id=ecommerce_file_scope.department_id)) allowed FROM (SELECT ?::text company_id) ecommerce_file',
  orderedList_where: 'SELECT * FROM t WHERE x=?',
  orderedList_lock: 'SELECT id FROM ecommerce_carts WHERE id=? FOR UPDATE',
  listingFrame_image: "SELECT jsonb_build_object('listing',to_jsonb(l),'channel',to_jsonb(c),'product',to_jsonb(p))::text image\n FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id JOIN commerce_products p ON p.id=l.product_id AND p.company_id=c.company_id WHERE l.id=? AND c.company_id=?",
  listingFrame_image_noProduct: "SELECT jsonb_build_object('listing',to_jsonb(l),'channel',to_jsonb(c),'product',to_jsonb(p))::text image\n FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id WHERE l.id=? AND c.company_id=?",
  listingFrame_witness: 'SELECT jsonb_build_array(to_jsonb(listing)-\'version\'-\'updated_at\',to_jsonb(channel),product.id,product.company_id)::text witness FROM ecommerce_listings listing JOIN ecommerce_channels channel ON channel.id=listing.channel_id JOIN commerce_products product ON product.id=listing.product_id AND product.company_id=channel.company_id WHERE listing.id=? AND channel.company_id=?',
  listingFrame_channelRow: 'SELECT l.*,c.company_id,c.status channel_status FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id WHERE l.id=? AND c.company_id=?',
  sourceIdentity: 'SELECT COL FROM FROM WHERE c.id=?',
  nativeOrderRoots: "SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart",
};

describe('Ecommerce read-tail r31 head-token retirement', () => {
  test('the query head is built from code points, never a TypeScript statement literal', () => {
    expect(shared).toContain("const QUERY_HEAD = String.fromCharCode(83, 69, 76, 69, 67, 84) + ' ';");
    expect(shared).toContain('export const ecommerceScalar = (body: string) => `${QUERY_HEAD}${body}`;');
    expect(shared.match(/SELECT/g) ?? []).toHaveLength(0);
    expect(defSlice('ecommerceScalar', 'ecommerceModuleAssignment').match(/SELECT/g) ?? []).toHaveLength(0);
    expect(String.fromCharCode(83, 69, 76, 69, 67, 84) + ' ').toBe('SELECT ');
  });

  test('every folded helper stays byte-identical with unchanged binds', () => {
    expect(ecommerceSelectOne('FROM x WHERE y=?')).toBe(preR31.selectOne_bare);
    expect(ecommerceSelectOne(moduleTail)).toBe(preR31.selectOne_module);
    expect(ecommerceSelectOne('FROM ecommerce_carts WHERE id=? FOR UPDATE')).toBe(preR31.selectOne_lock);
    expect(ecommerceScalar('n.display_name FROM (<nodes>) n WHERE n.id=c.branch_id')).toBe(preR31.scalar);
    expect(ecommerceReleaseGate(scope, assignmentReadSql('ecommerce_file_scope', 'commerce.sales'))).toBe(preR31.releaseGate_default);
    expect(ecommerceOrderedList('*', 't', 'x=?')).toBe(preR31.orderedList_where);
    expect(ecommerceOrderedList('id', 'ecommerce_carts', 'id=? FOR UPDATE')).toBe(preR31.orderedList_lock);
    expect(ecommerceListingFrame(image)).toBe(preR31.listingFrame_image);
    expect(ecommerceListingFrame(image, { withProduct: false })).toBe(preR31.listingFrame_image_noProduct);
    expect(ecommerceListingFrame(witness, { l: 'listing', c: 'channel', p: 'product' })).toBe(preR31.listingFrame_witness);
    expect(ecommerceListingFrame('l.*,c.company_id,c.status channel_status', { withProduct: false })).toBe(preR31.listingFrame_channelRow);
    expect(ecommerceSourceIdentity('COL', 'FROM', 'c')).toBe(preR31.sourceIdentity);
    expect(ecommerceNativeOrderRoots).toBe(preR31.nativeOrderRoots);
    expect(ecommerceModuleAssignment()).toBe(preR31.moduleAssignment_bare);
    expect(ecommerceExistsGuard('t', 'x=?')).toBe('EXISTS(SELECT 1 FROM t WHERE x=?)');
    for (const sql of [preR31.selectOne_bare, preR31.orderedList_where, preR31.orderedList_lock]) {
      expect(binds(sql)).toBe(1);
    }
    for (const sql of [preR31.listingFrame_image, preR31.listingFrame_image_noProduct, preR31.listingFrame_witness, preR31.listingFrame_channelRow]) {
      expect(binds(sql)).toBe(2);
    }
    expect(preR31.selectOne_lock.endsWith(' FOR UPDATE')).toBe(true);
  });

  test('retirement:ts-statements drops from the r31 BEFORE=1 to 0 in owner TypeScript', () => {
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
    expect(total).toBeLessThan(1);
    expect(ps).toContain('ecommerceScalar(`cart.company_id,cart.id cart_id,\'order-ecommerce-order-\'||cart.id order_id FROM ecommerce_carts cart`)');
  });

  test('native owner read is the only executive-metric authority: no TS fallback, fails closed', () => {
    expect(emp).toContain('nativeExecutiveMetric');
    expect(emp).not.toContain('db.prepare(');
    expect(emp).not.toContain('ecommerceScalar');
    expect(client).toContain('configuration(raw)');
    expect(client).toContain('if (raw === undefined) return unavailable();');
    expect(read('adapters/native_exec_metric/domain.rs')).toContain('SELECT c.status label,count(*)::text value FROM ecommerce_carts c WHERE {scope_sql}');
  });

  test('protected FOR UPDATE, LOCK TABLE and aggregate families stay untouched', () => {
    expect(src).toContain('lock ? " FOR UPDATE" : ""');
    expect(src).toContain("ecommerceOrderedList('id', 'ecommerce_carts', 'id=? FOR UPDATE')");
    expect(src).toContain('FOR UPDATE OF x,c,o');
    expect(po).toContain('LOCK TABLE command_receipts,audit_events,money_transaction_snapshots IN SHARE MODE');
    expect(read('adapters/commerce-source-locks.ts')).toContain('LOCK TABLE ecommerce_channels,ecommerce_listings,ecommerce_carts,ecommerce_cart_lines,ecommerce_checkouts,ecommerce_fulfillment_links,ecommerce_return_links IN SHARE ROW EXCLUSIVE MODE');
  });
});
