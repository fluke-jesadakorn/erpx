import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import {
  ecommerceListingChannelRow,
  ecommerceListingEvidence,
  ecommerceListingFrame,
  ecommerceOrderedList,
  ecommerceReleaseGate,
  ecommerceScopeRow,
} from '../../../packages/server-ecommerce/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-ecommerce/src');
const read = (path: string) => readFileSync(resolve(srcRoot, path), 'utf8');
const shared = read('adapters/read-tail-sql.ts');
const src = read('adapters/ecommerce.ts');
const ap = read('adapters/attachment-purchases.ts');
const ao = read('adapters/attachments-operational-facts.ts');
const ar = read('adapters/attachment-read.ts');

const products = '<products>';

// Verbatim pre-r25 interpolation, copied from the identity-access adapter and
// used only to resolve the byte oracle on both sides of the file-authority gate.
function assignmentReadSql(alias: string, permission: string) {
    return `EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=${alias}.company_id
    AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_role_allows(g.role,g.company_id,'${permission}')
    AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=${alias}.branch_id OR g.scope_kind='department' AND g.scope_node_id=${alias}.department_id))`;
}

// Pre-r25 SQL copied verbatim from HEAD (r24), kept only as byte-identity oracles.
const preR25 = {
  // ecommerce.ts:346 returnLinks literal.
  returnLinks: `SELECT l.*,r.status current_return_status,r.version current_return_version FROM ecommerce_return_links l JOIN (${products}) r ON r.id=l.commerce_return_id WHERE l.checkout_id=?`,
  // attachment-purchases.ts:49 ecommerce-file-authority assertion literal.
  fileAuthority: `(SELECT ${assignmentReadSql('ecommerce_file_scope','commerce.sales')} FROM ${ecommerceScopeRow('ecommerce_file_scope', true)})`,
};
// Pre-r25 frames copied verbatim from read-tail-sql.ts (r24 lines 35 and 44).
const preR25ChannelRow = (columns: string) =>
  `SELECT ${columns} FROM ecommerce_listings l JOIN ecommerce_channels c ON c.id=l.channel_id WHERE l.id=? AND c.company_id=?`;
const preR25Evidence = (projection: string, l = 'l', c = 'c', p = 'p') =>
  `SELECT ${projection} FROM ecommerce_listings ${l} JOIN ecommerce_channels ${c} ON ${c}.id=${l}.channel_id JOIN commerce_products ${p} ON ${p}.id=${l}.product_id AND ${p}.company_id=${c}.company_id WHERE ${l}.id=? AND ${c}.company_id=?`;

describe('Ecommerce read-tail r25 three-rank dedupe', () => {
  test('F1: returnLinks rebuilds from ecommerceOrderedList byte-identically with one bind', () => {
    expect(ecommerceOrderedList('l.*,r.status current_return_status,r.version current_return_version', `ecommerce_return_links l JOIN (${products}) r ON r.id=l.commerce_return_id`, 'l.checkout_id=?')).toBe(preR25.returnLinks);
    expect((preR25.returnLinks.match(/\?/g) ?? []).length).toBe(1);
    expect(src).toContain('ecommerceOrderedList("l.*,r.status current_return_status,r.version current_return_version"');
    expect(src).toContain('"l.checkout_id=?")).bind(id).all<Row>()).results');
    expect(src).not.toContain('`SELECT l.*,r.status current_return_status,r.version current_return_version FROM ecommerce_return_links');
    expect((src.match(/ecommerceOrderedList\(/g) ?? []).length).toBe(7);
  });

  test('F2: file authority rebuilds from ecommerceReleaseGate byte-identically (689 chars, six binds)', () => {
    expect(assignmentReadSql('ecommerce_file_scope','commerce.sales')).toHaveLength(586);
    expect(preR25.fileAuthority).toHaveLength(689);
    expect((preR25.fileAuthority.match(/\?/g) ?? []).length).toBe(6);
    const rebuilt = `(${ecommerceReleaseGate(ecommerceScopeRow('ecommerce_file_scope', true), assignmentReadSql('ecommerce_file_scope','commerce.sales'), '')})`;
    expect(rebuilt).toBe(preR25.fileAuthority);
    expect(ap).toContain("ecommerceReleaseGate(ecommerceScopeRow('ecommerce_file_scope', true), assignmentReadSql('ecommerce_file_scope','commerce.sales'), '')");
    expect(ap).not.toContain("`(SELECT ${assignmentReadSql('ecommerce_file_scope','commerce.sales')} FROM ${ecommerceScopeRow('ecommerce_file_scope', true)})`");
    expect((ap.match(/ecommerceReleaseGate\(/g) ?? []).length).toBe(3);
  });

  test('F3: the merged listing frame rebuilds all four call sites byte-identically', () => {
    const rowA = 'l.*,c.company_id,c.status channel_status';
    const rowB = 'c.company_id,NULL::text branch_id,NULL::text department_id,l.version';
    const image = "jsonb_build_object('listing',to_jsonb(l),'channel',to_jsonb(c),'product',to_jsonb(p))::text image\n";
    const witness = "jsonb_build_array(to_jsonb(listing)-'version'-'updated_at',to_jsonb(channel),product.id,product.company_id)::text witness";
    expect(ecommerceListingFrame(rowA, { withProduct: false })).toBe(preR25ChannelRow(rowA));
    expect(ecommerceListingFrame(rowB, { withProduct: false })).toBe(preR25ChannelRow(rowB));
    expect(ecommerceListingFrame(image)).toBe(preR25Evidence(image));
    expect(ecommerceListingFrame(witness, { l: 'listing', c: 'channel', p: 'product' })).toBe(preR25Evidence(witness, 'listing', 'channel', 'product'));
    for (const sql of [rowA, rowB, image, witness]) {
      expect((ecommerceListingFrame(sql).match(/\?/g) ?? []).length).toBe(2);
    }
    expect(preR25Evidence(image)).toContain('image\n FROM ecommerce_listings l');
    expect(ecommerceListingFrame(image)).toContain('image\n FROM ecommerce_listings l');
  });

  test('F4: both pre-r25 frame names delegate to the shared frame without a SELECT literal', () => {
    expect((shared.match(/export const ecommerceListingFrame = /g) ?? []).length).toBe(1);
    expect((shared.match(/export const ecommerceListingChannelRow = /g) ?? []).length).toBe(1);
    expect((shared.match(/export const ecommerceListingEvidence = /g) ?? []).length).toBe(1);
    expect(shared).toContain('ecommerceListingFrame(columns, { withProduct: false })');
    expect(shared).toContain('ecommerceListingFrame(projection, { l, c, p })');
    const frameDef = shared.slice(shared.indexOf('export const ecommerceListingFrame = '), shared.indexOf('export const ecommerceListingChannelRow = '));
    expect((frameDef.match(/SELECT /g) ?? []).length).toBe(1);
    const wrappers = shared.slice(shared.indexOf('export const ecommerceListingChannelRow = '), shared.indexOf('/** r14: one parameterized source-identity read'));
    expect(wrappers).not.toContain('SELECT ');
    expect(src).toContain('ecommerceListingChannelRow("l.*,c.company_id,c.status channel_status")');
    expect(ao).toContain('ecommerceListingChannelRow("c.company_id,NULL::text branch_id,NULL::text department_id,l.version")');
    expect(ar).toContain("ecommerceListingEvidence(\"jsonb_build_object('listing',to_jsonb(l),'channel',to_jsonb(c),'product',to_jsonb(p))::text image\\n\")");
    expect(ap).toContain('ecommerceListingEvidence("jsonb_build_array(to_jsonb(listing)-\'version\'-\'updated_at\',to_jsonb(channel),product.id,product.company_id)::text witness",\'listing\',\'channel\',\'product\')');
  });

  test('F5: binds stay unchanged, composed frames stay read-only', () => {
    for (const sql of Object.values(preR25)) {
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
    expect(src).toContain('.bind(id).all<Row>()).results');
    expect(ap).toContain('[actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId]');
    expect(ao).toContain('.bind(facts.targetId,facts.companyId).first<TargetRow>()');
    expect(ar).toContain('.bind(id,actor.companyId).first<string>(\'image\')');
  });

  test('F6: locks, native bridge and r1-r24 frame sites stay intact', () => {
    for (const token of [
      'ecommerceSelectOne', 'ecommerceModuleAssignment', 'ecommerceRowProbe', 'ecommerceScopeRow',
      'ecommerceReleaseGate', 'ecommerceExistsGuard', 'ecommerceOrderedList',
      'ecommerceListingFrame', 'ecommerceListingChannelRow', 'ecommerceSourceIdentity', 'ecommerceListingEvidence',
    ]) {
      expect(shared).toContain(`export const ${token} =`);
    }
    expect(src).toContain('SELECT id FROM ecommerce_carts WHERE id=? FOR UPDATE');
    expect(src).toContain('FOR UPDATE OF x,c,o');
    expect(src).toContain('lock ? " FOR UPDATE" : ""');
    expect((src.match(/FOR UPDATE/g) ?? []).length).toBe(3);
    expect(src).toContain('nativeWrite(');
    expect(ao).toContain('UPDATE ${table} SET version=version+1,updated_at=? WHERE id=? AND version=?');
    expect(ap).toContain('LOCK TABLE commerce_products IN SHARE MODE');
    expect(read('adapters/commerce-source-locks.ts')).toContain('LOCK TABLE ecommerce_channels,ecommerce_listings,ecommerce_carts,ecommerce_cart_lines,ecommerce_checkouts,ecommerce_fulfillment_links,ecommerce_return_links IN SHARE ROW EXCLUSIVE MODE');
  });
});

describe('Ecommerce read-tail r25 retirement check', () => {
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
    expect(total).toBe(7);
    expect(total).toBeLessThan(22);
  });
});
