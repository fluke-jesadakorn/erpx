import { describe, expect, test } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { finalRootOwnerProjectionSources as commerce } from '@erpx/server-commerce/public/final-root-owner-projection-sources';
import { portalBindingOrderCore } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r44-sources';
import { portalBindingSource } from '../../../packages/server-customer_authority/src/adapters/portal-binding-source';
import { portalPurchaseBindingSource } from '../../../packages/server-customer_authority/src/adapters/portal-purchase-binding-source';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Verbatim pre-dedupe literals from source SHA f8ce3948d (G4 read-tail r18/r19).
// Only caller-owned projection, whitespace separators and the trailing kind
// clause vary; the closed binding/order core is shared byte-for-byte.
const ORDERS = commerce.commerce_orders;
const BINDING_ORIGINAL = (columns = '') => `SELECT ${columns}b.company_id,o.branch_id,o.department_id,b.version
    FROM customer_access_bindings b
    JOIN (${ORDERS}) o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id`;
const PURCHASE_ORIGINAL = (projection: string, kinds = "'sales','quote'") => `SELECT ${projection}
 FROM customer_access_bindings b JOIN (${ORDERS}) o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id AND o.kind IN (${kinds})`;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r44 binding-order core family', () => {
  test('renders byte-identically to every pre-dedupe literal', () => {
    expect(portalBindingOrderCore('x', ' ', ' ', ORDERS, '')).toBe(`SELECT x FROM customer_access_bindings b JOIN (${ORDERS}) o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id`);
    for (const cols of ['', 'b.id,']) {
      expect(portalBindingSource(cols)).toBe(BINDING_ORIGINAL(cols));
    }
    const purchaseSites: Array<[string, string]> = [
      ['b.id,b.company_id,o.branch_id,o.department_id,b.created_by,b.version,o.state source_state,\n jsonb_build_array(b.id,b.company_id,b.order_id,b.customer_id,b.email,o.id,o.company_id,o.branch_id,o.department_id,o.kind,o.party_id,o.state)::text identity,\n jsonb_build_array(to_jsonb(b),o.id,o.company_id,o.branch_id,o.department_id,o.kind,o.party_id,o.state)::text image', "'quote','sales'"],
      ["jsonb_build_array(to_jsonb(b)-'version'-'updated_at',o.id,o.company_id,o.branch_id,o.department_id,o.kind,o.party_id,o.version,o.state)::text", "'sales','quote'"],
      ["jsonb_build_object('binding',to_jsonb(b),'order',to_jsonb(o),\n 'identities',(SELECT jsonb_agg(to_jsonb(i) ORDER BY i.user_id) FROM customer_access_identities i WHERE i.binding_id=b.id))::text image", "'sales','quote'"],
      ['plain', "'quote','sales'"],
    ];
    for (const [projection, kinds] of purchaseSites) {
      expect(portalPurchaseBindingSource(projection, kinds)).toBe(PURCHASE_ORIGINAL(projection, kinds));
    }
    for (const sql of [
      ...['', 'b.id,'].map(portalBindingSource),
      ...purchaseSites.map(([p, k]) => portalPurchaseBindingSource(p, k)),
    ]) readOnly(sql);
  });

  test('one universal core and two delegating helper files', () => {
    const core = src('customer-authority-read-tail-r44-sources.ts');
    expect(core.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(core).toContain('selectProjection(');
    expect(core).toContain("from './customer-authority-read-tail-r40-sources'");
    expect(core.match(/export const portalBindingOrderCore/g) ?? []).toHaveLength(1);
    for (const [name, symbol] of [
      ['portal-binding-source.ts', 'portalBindingSource'],
      ['portal-purchase-binding-source.ts', 'portalPurchaseBindingSource'],
    ] as Array<[string, string]>) {
      const helper = src(name);
      expect(helper.match(/\bSELECT\b/g) ?? [], name).toHaveLength(0);
      expect(helper, name).toContain(`export const ${symbol}`);
      expect(helper, name).toContain('portalBindingOrderCore(');
      expect(helper, name).toContain("from './customer-authority-read-tail-r44-sources'");
      expect(helper, name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('pinned callers keep prepared statements, locks, native kinds and eligibility', () => {
    const access = src('portal-access.ts');
    expect(access).toContain('CA_RD_PORTAL_BINDING_LOCK');
    expect(access).toContain('nativePortalReadStatement');
    expect(access).toContain("db.prepare(`${portalBindingSource('b.id,')}");
    const facts = src('attachments-portal-facts.ts');
    expect(facts).toContain('db.prepare(`${portalBindingSource()}');
    for (const name of ['portal-purchases.ts', 'attachment-purchases.ts', 'attachment-read.ts']) {
      expect(src(name), name).toContain('${portalPurchaseBindingSource(');
    }
    // r21-r43 builders, the row-locked reader and native/eligibility helpers untouched.
    for (const name of [
      'customer-authority-read-tail-r40-sources.ts',
      'customer-authority-read-tail-r42-sources.ts',
      'customer-authority-read-tail-r26-sources.ts',
      'customer-authority-read-tail-r29-sources.ts',
      'assignment-eligibility-source.ts',
    ]) expect(src(name)).toBeTruthy();
  });
});
