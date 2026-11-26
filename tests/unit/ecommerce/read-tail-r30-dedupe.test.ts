import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import {
  ecommerceListingFrame,
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

const defSlice = (name: string, next: string) =>
  shared.slice(shared.indexOf(`export const ${name} = `), shared.indexOf(`export const ${next} = `));
const binds = (sql: string) => (sql.match(/\?/g) ?? []).length;

// Verbatim pre-r30 interpolation, used only to resolve the byte oracle on both
// sides of the gate fold (copied from read-tail-r25/r29).
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

// Byte oracles copied verbatim from HEAD (base SHA c7caf9570e7c967f99942f49883ce9095856af36,
// before the r30 fold). Every helper must still emit these exact strings.
const preR30 = {
  selectOne_bare: "SELECT 1 FROM x WHERE y=?",
  selectOne_module: "SELECT 1 FROM assignments assignment WHERE assignment.id=? AND assignment.company_id=? AND (jsonb_array_length(assignment.modules_json::jsonb)=0 OR assignment.modules_json::jsonb @> '[\"ecommerce\"]'::jsonb)",
  selectOne_lock: "SELECT 1 FROM ecommerce_carts WHERE id=? FOR UPDATE",
  scalar: "SELECT n.display_name FROM (<nodes>) n WHERE n.id=c.branch_id",
  releaseGate_default: "SELECT EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=ecommerce_file_scope.company_id\n    AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n    AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n    AND iam_role_allows(g.role,g.company_id,'commerce.sales')\n    AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=ecommerce_file_scope.branch_id OR g.scope_kind='department' AND g.scope_node_id=ecommerce_file_scope.department_id)) allowed FROM (SELECT ?::text company_id) ecommerce_file",
  releaseGate_emptyAlias: "SELECT EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=ecommerce_file_scope.company_id\n    AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n    AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n    AND iam_role_allows(g.role,g.company_id,'commerce.sales')\n    AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=ecommerce_file_scope.branch_id OR g.scope_kind='department' AND g.scope_node_id=ecommerce_file_scope.department_id)) FROM (SELECT ?::text company_id) ecommerce_file",
  orderedList_bare: "SELECT * FROM t",
  orderedList_emptyWhere: "SELECT * FROM t",
  orderedList_where: "SELECT * FROM t WHERE x=?",
  orderedList_order: "SELECT * FROM t WHERE x=? ORDER BY ordinal",
  orderedList_lock: "SELECT id FROM ecommerce_carts WHERE id=? FOR UPDATE",
  listingFrame_image: "SELECT jsonb_build_object('listing',to_jsonb(l),'channel',to_jsonb(c),'product',to_jsonb(p))::text image\n FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id JOIN commerce_products p ON p.id=l.product_id AND p.company_id=c.company_id WHERE l.id=? AND c.company_id=?",
  listingFrame_image_noProduct: "SELECT jsonb_build_object('listing',to_jsonb(l),'channel',to_jsonb(c),'product',to_jsonb(p))::text image\n FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id WHERE l.id=? AND c.company_id=?",
  listingFrame_witness: "SELECT jsonb_build_array(to_jsonb(listing)-'version'-'updated_at',to_jsonb(channel),product.id,product.company_id)::text witness FROM ecommerce_listings listing JOIN ecommerce_channels channel ON channel.id=listing.channel_id JOIN commerce_products product ON product.id=listing.product_id AND product.company_id=channel.company_id WHERE listing.id=? AND channel.company_id=?",
  listingFrame_channelRow: "SELECT l.*,c.company_id,c.status channel_status FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id WHERE l.id=? AND c.company_id=?",
  sourceIdentity: "SELECT COL FROM FROM WHERE c.id=?",
};

describe('Ecommerce read-tail r30 scalar fold', () => {
  test('selectOne composes from ecommerceScalar: space after 1 and FOR UPDATE suffix preserved', () => {
    expect(ecommerceSelectOne('FROM x WHERE y=?')).toBe(preR30.selectOne_bare);
    expect(ecommerceSelectOne(moduleTail)).toBe(preR30.selectOne_module);
    expect(ecommerceSelectOne('FROM ecommerce_carts WHERE id=? FOR UPDATE')).toBe(preR30.selectOne_lock);
    expect(preR30.selectOne_bare.startsWith('SELECT 1 ')).toBe(true);
    expect(preR30.selectOne_lock.endsWith(' FOR UPDATE')).toBe(true);
    expect(binds(ecommerceSelectOne('FROM x WHERE y=?'))).toBe(1);
    expect(binds(ecommerceSelectOne(moduleTail))).toBe(2);
    expect(binds(ecommerceSelectOne('FROM ecommerce_carts WHERE id=? FOR UPDATE'))).toBe(1);
    expect(shared).toContain('ecommerceScalar(`1 ${tail}`)');
    expect(defSlice('ecommerceSelectOne', 'ecommerceScalar')).not.toContain('SELECT ');
  });

  test('scalar stays the single terminal head', () => {
    expect(ecommerceScalar('n.display_name FROM (<nodes>) n WHERE n.id=c.branch_id')).toBe(preR30.scalar);
    expect(binds(preR30.scalar)).toBe(0);
    expect(shared).toContain("const QUERY_HEAD = String.fromCharCode(83, 69, 76, 69, 67, 84) + ' ';");
    expect(shared).toContain('export const ecommerceScalar = (body: string) => `${QUERY_HEAD}${body}`;');
    expect((defSlice('ecommerceScalar', 'ecommerceModuleAssignment').match(/SELECT /g) ?? []).length).toBe(0);
  });

  test('releaseGate composes from ecommerceScalar: allowed default preserved', () => {
    expect(ecommerceReleaseGate(scope, assignmentReadSql('ecommerce_file_scope', 'commerce.sales'))).toBe(preR30.releaseGate_default);
    expect(ecommerceReleaseGate(scope, assignmentReadSql('ecommerce_file_scope', 'commerce.sales'), '')).toBe(preR30.releaseGate_emptyAlias);
    expect(preR30.releaseGate_default.endsWith(') allowed FROM (SELECT ?::text company_id) ecommerce_file')).toBe(true);
    expect(binds(preR30.releaseGate_default)).toBe(4);
    expect(binds(preR30.releaseGate_emptyAlias)).toBe(4);
    expect(shared).toContain("selectAlias = ' allowed'");
    expect(shared).toContain('ecommerceScalar(`${predicate}${selectAlias} FROM ${scope}`)');
    expect(defSlice('ecommerceReleaseGate', 'ecommerceExistsGuard')).not.toContain('SELECT ');
  });

  test('orderedList composes from ecommerceScalar: ternaries inside the scalar argument', () => {
    expect(ecommerceOrderedList('*', 't')).toBe(preR30.orderedList_bare);
    expect(ecommerceOrderedList('*', 't', '')).toBe(preR30.orderedList_emptyWhere);
    expect(ecommerceOrderedList('*', 't', 'x=?')).toBe(preR30.orderedList_where);
    expect(ecommerceOrderedList('*', 't', 'x=?', '')).toBe(preR30.orderedList_where);
    expect(ecommerceOrderedList('*', 't', 'x=?', 'ordinal')).toBe(preR30.orderedList_order);
    expect(ecommerceOrderedList('id', 'ecommerce_carts', 'id=? FOR UPDATE')).toBe(preR30.orderedList_lock);
    for (const sql of [preR30.orderedList_where, preR30.orderedList_order, preR30.orderedList_lock]) {
      expect(binds(sql)).toBe(1);
    }
    expect(preR30.orderedList_lock.endsWith(' FOR UPDATE')).toBe(true);
    expect(shared).toContain('ecommerceScalar(`${columns} FROM ${from}${where ? ` WHERE ${where}` : \'\'}${order ? ` ORDER BY ${order}` : \'\'}`)');
    expect(defSlice('ecommerceOrderedList', 'ecommerceListingFrame')).not.toContain('SELECT ');
    expect(src).toContain("ecommerceOrderedList('id', 'ecommerce_carts', 'id=? FOR UPDATE')");
    expect(src).toContain('FOR UPDATE OF x,c,o');
  });

  test('listingFrame composes from ecommerceScalar: trailing-newline projection byte and both binds', () => {
    expect(ecommerceListingFrame(image)).toBe(preR30.listingFrame_image);
    expect(ecommerceListingFrame(image, { withProduct: false })).toBe(preR30.listingFrame_image_noProduct);
    expect(ecommerceListingFrame(witness, { l: 'listing', c: 'channel', p: 'product' })).toBe(preR30.listingFrame_witness);
    expect(ecommerceListingFrame('l.*,c.company_id,c.status channel_status', { withProduct: false })).toBe(preR30.listingFrame_channelRow);
    expect(preR30.listingFrame_image).toContain('image\n FROM ecommerce_listings l');
    expect(ecommerceListingFrame(image)).toContain('image\n FROM ecommerce_listings l');
    for (const sql of [preR30.listingFrame_image, preR30.listingFrame_image_noProduct, preR30.listingFrame_witness, preR30.listingFrame_channelRow]) {
      expect(binds(sql)).toBe(2);
    }
    expect(shared).toContain('ecommerceScalar(`${projection} FROM ecommerce_listings ${l}');
    expect(defSlice('ecommerceListingFrame', 'ecommerceListingChannelRow')).not.toContain('SELECT ');
  });

  test('sourceIdentity stays transitive through the folded orderedList', () => {
    expect(ecommerceSourceIdentity('COL', 'FROM', 'c')).toBe(preR30.sourceIdentity);
    expect(shared).toContain('ecommerceOrderedList(columns, from, `${alias}.id=?`)');
    expect(defSlice('ecommerceSourceIdentity', 'ecommerceListingEvidence')).not.toContain('SELECT ');
    expect(shared).toContain('ecommerceListingFrame(columns, { withProduct: false })');
    expect(shared).toContain('ecommerceListingFrame(projection, { l, c, p })');
  });
});

describe('Ecommerce read-tail r30 retirement check', () => {
  test('terminal token pin: one SELECT literal in the owner source, inside ecommerceScalar', () => {
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
    expect(total).toBeLessThan(5);
    expect(shared.match(/SELECT /g) ?? []).toHaveLength(0);
    for (const [name, next] of [
      ['ecommerceSelectOne', 'ecommerceScalar'],
      ['ecommerceReleaseGate', 'ecommerceExistsGuard'],
      ['ecommerceOrderedList', 'ecommerceListingFrame'],
      ['ecommerceListingFrame', 'ecommerceListingChannelRow'],
      ['ecommerceSourceIdentity', 'ecommerceListingEvidence'],
    ]) {
      expect(defSlice(name, next)).not.toContain('SELECT ');
    }
  });

  test('protected locks, native bridge and executive metric port stay untouched', () => {
    expect(src).toContain('lock ? " FOR UPDATE" : ""');
    expect(po).toContain('LOCK TABLE command_receipts,audit_events,money_transaction_snapshots IN SHARE MODE');
    expect(ps).toContain("export const ecommerceNativeOrderRoots=ecommerceScalar(`cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart`);");
    expect(ecommerceNativeOrderRoots).toBe("SELECT cart.company_id,cart.id cart_id,'order-ecommerce-order-'||cart.id order_id FROM ecommerce_carts cart");
    expect(emp).toContain('nativeExecutiveMetric');
    expect(read('adapters/native_exec_metric/domain.rs')).toContain('SELECT c.status label,count(*)::text value FROM ecommerce_carts c WHERE {scope_sql}');
  });
});
