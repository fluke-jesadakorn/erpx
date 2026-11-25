import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { bindingCommerceOrderSource, unionFactRow } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r34-sources';
import { finalRootOwnerProjectionSources as commerce } from '../../../packages/server-commerce/src/public/final-root-owner-projection-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Exact pre-dedupe SQL copied verbatim from the r33 baseline (BEFORE=47).
const STAFF = `SELECT o.company_id,o.branch_id,o.department_id FROM customer_access_bindings b JOIN (${commerce.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id WHERE b.id=? AND b.company_id=?`;
const READ_AUDIT = `SELECT count(*)::int n FROM customer_access_reads r JOIN customer_access_bindings b ON b.id=r.binding_id AND b.version=r.binding_version JOIN customer_access_identities i ON i.binding_id=b.id AND i.user_id=r.user_id AND i.email=b.email JOIN (${commerce.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id WHERE r.id IN (?) AND r.user_id=? AND b.email=? AND b.status='approved' AND b.expires_at>? AND o.kind IN ('quote','sales') AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled') AND CURRENT_AUTHORITY`;
const UNIONS = [
  `SELECT 'binding' kind,binding.id,to_jsonb(binding) fact,BINDING_PENDING pending FROM bindings binding LEFT JOIN commerce_orders original ON original.id=binding.order_id`,
  `SELECT 'portal-invitation',invitation.id,jsonb_build_object('id',invitation.id),INVITATION_PENDING FROM customer_portal_invitations invitation JOIN bindings binding ON binding.id=invitation.binding_id`,
  `SELECT 'response',response.id,to_jsonb(response),RESPONSE_PENDING FROM customer_portal_support_responses response LEFT JOIN customer_access_bindings binding ON binding.id=response.binding_id LEFT JOIN customer_portal_actions request ON request.id=response.request_id CROSS JOIN scope WHERE response.company_id=scope.company_id OR binding.company_id=scope.company_id`,
  `SELECT 'closure',closure.id,to_jsonb(closure),CLOSURE_PENDING FROM customer_portal_support_closures closure CROSS JOIN scope LEFT JOIN customer_access_bindings binding ON binding.id=closure.binding_id WHERE closure.company_id=scope.company_id OR binding.company_id=scope.company_id`,
];

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r34 binding source core family', () => {
  test('renders byte-identically to both pre-dedupe binding/order cores', () => {
    expect(bindingCommerceOrderSource('o.company_id,o.branch_id,o.department_id', 'customer_access_bindings b', ' WHERE b.id=? AND b.company_id=?')).toBe(STAFF);
    expect(bindingCommerceOrderSource('count(*)::int n', 'customer_access_reads r JOIN customer_access_bindings b ON b.id=r.binding_id AND b.version=r.binding_version JOIN customer_access_identities i ON i.binding_id=b.id AND i.user_id=r.user_id AND i.email=b.email', ' AND o.party_id=b.customer_id WHERE r.id IN (?) AND r.user_id=? AND b.email=? AND b.status=\'approved\' AND b.expires_at>? AND o.kind IN (\'quote\',\'sales\') AND o.state IN (\'confirmed\',\'converted\',\'partially_fulfilled\',\'fulfilled\') AND CURRENT_AUTHORITY')).toBe(READ_AUDIT);
    for (const sql of [STAFF, READ_AUDIT]) readOnly(sql);
  });

  test('one shared core, two pinned callers and intact portal shells/control plane', () => {
    const shared = src('customer-authority-read-tail-r34-sources.ts');
    // r38: the union-fact SELECT head now delegates to the byte-identical r25
    // ownerColumnProjection, so this builder owns only the binding/order core.
    expect(shared.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(shared).toContain('ownerColumnProjection(projection, from)');
    expect(shared).toContain('export const bindingCommerceOrderSource');
    expect(shared).toContain('o.id=b.order_id AND o.company_id=b.company_id');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const attach = src('attachment-purchases.ts');
    expect(attach.match(/bindingCommerceOrderSource\(/g) ?? []).toHaveLength(1);
    expect(attach).not.toContain('SELECT o.company_id,o.branch_id,o.department_id FROM customer_access_bindings b JOIN');
    expect(attach).toContain('${portalPurchaseBindingSource(');
    expect(attach).toContain("checkPurchasedCapability(database,await commercePackageEnforcementMode(),scope)");

    const unit = src('portal-projection-unit.ts');
    expect(unit.match(/bindingCommerceOrderSource\(/g) ?? []).toHaveLength(1);
    expect(unit).not.toContain('SELECT count(*)::int n FROM customer_access_reads r JOIN customer_access_bindings b ON');
    expect(unit).toContain("allColumnsProjection('customer_access_bindings',' WHERE email=?')");
    expect(unit).not.toContain('WITH bindings AS(SELECT * FROM customer_access_bindings WHERE email=?)');

    const access = src('portal-access.ts');
    expect(access).toContain('FOR UPDATE');
    expect(access).not.toContain('bindingCommerceOrderSource');
    for (const name of ['portal-binding-source.ts', 'portal-purchase-binding-source.ts']) {
      expect(src(name), name).not.toContain('bindingCommerceOrderSource');
    }
  });
});

describe('Customer Authority read-tail r34 union fact row scaffold', () => {
  test('renders byte-identically to all four pre-dedupe UNION ALL rows', () => {
    expect(unionFactRow(`'binding' kind,binding.id,to_jsonb(binding) fact,BINDING_PENDING pending`, 'bindings binding LEFT JOIN commerce_orders original ON original.id=binding.order_id')).toBe(UNIONS[0]);
    expect(unionFactRow(`'portal-invitation',invitation.id,jsonb_build_object('id',invitation.id),INVITATION_PENDING`, 'customer_portal_invitations invitation JOIN bindings binding ON binding.id=invitation.binding_id')).toBe(UNIONS[1]);
    expect(unionFactRow(`'response',response.id,to_jsonb(response),RESPONSE_PENDING`, 'customer_portal_support_responses response LEFT JOIN customer_access_bindings binding ON binding.id=response.binding_id LEFT JOIN customer_portal_actions request ON request.id=response.request_id CROSS JOIN scope WHERE response.company_id=scope.company_id OR binding.company_id=scope.company_id')).toBe(UNIONS[2]);
    expect(unionFactRow(`'closure',closure.id,to_jsonb(closure),CLOSURE_PENDING`, 'customer_portal_support_closures closure CROSS JOIN scope LEFT JOIN customer_access_bindings binding ON binding.id=closure.binding_id WHERE closure.company_id=scope.company_id OR binding.company_id=scope.company_id')).toBe(UNIONS[3]);
    for (const sql of UNIONS) readOnly(sql);
  });

  test('four pinned callers and intact obligation control plane', () => {
    const shared = src('customer-authority-read-tail-r34-sources.ts');
    expect(shared.match(/export const unionFactRow/g) ?? []).toHaveLength(1);

    const facts = src('package-obligations.ts');
    expect(facts.match(/unionFactRow\(/g) ?? []).toHaveLength(4);
    for (const row of ["SELECT 'binding' kind,binding.id", "UNION ALL SELECT 'portal-invitation'", "UNION ALL SELECT 'response'", "UNION ALL SELECT 'closure'"]) {
      expect(facts).not.toContain(row);
    }
    expect(facts).toContain("database.prepare('LOCK TABLE command_receipts,audit_events,outbox_events IN SHARE MODE').run()");
    expect(facts).toContain('WITH scope AS(${companyScopeSelect()})');
    expect(facts).toContain('ORDER BY kind,id');
    for (const helper of ['bindingFactProjection(', 'accessEventEvidenceProbe(', 'factExistenceWitness(', 'companyScopeSelect(']) {
      expect(facts, helper).toContain(helper);
    }
  });

  test('r21-r33 builders, source locks and native kinds stay untouched', () => {
    const counts: Array<[string, number, string]> = [
      ['customer-authority-read-sql.ts', 1, 'export const jsonbAggFact'],
      ['package-obligation-sql.ts', 1, 'export const latestSupportVersion'],
      ['customer-authority-read-tail-r23-sources.ts', 1, 'export const jsonbImageProjection'],
      ['customer-authority-read-tail-r24-sources.ts', 1, 'export const bindingFactProjection'],
      ['customer-authority-read-tail-r25-sources.ts', 0, 'export const ownerColumnProjection'],
      ['customer-authority-read-tail-r26-sources.ts', 1, 'export const accessBindingOrderProjection'],
      ['customer-authority-read-tail-r27-sources.ts', 0, 'export const accessEventEvidenceProbe'],
      ['customer-authority-read-tail-r28-sources.ts', 0, 'export const releaseGateRead'],
      ['customer-authority-read-tail-r29-sources.ts', 1, 'export const reviewEligibilityRows'],
      ['customer-authority-read-tail-r30-sources.ts', 0, 'export const existenceProbe'],
      ['customer-authority-read-tail-r31-sources.ts', 0, 'export const supportConclusionIdentity'],
      ['customer-authority-read-tail-r32-sources.ts', 0, 'export const factExistenceWitness'],
      ['customer-authority-read-tail-r33-sources.ts', 0, 'export const companyScopeSelect'],
      ['portal-binding-source.ts', 0, 'export const portalBindingSource'],
      ['portal-purchase-binding-source.ts', 0, 'export const portalPurchaseBindingSource'],
    ];
    for (const [name, count, symbol] of counts) {
      const helper = src(name);
      expect(helper.match(/\bSELECT\b/g) ?? [], name).toHaveLength(count);
      expect(helper, name).toContain(symbol);
      expect(helper, name).not.toContain('bindingCommerceOrderSource');
      expect(helper, name).not.toContain('unionFactRow');
    }
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=47', () => {
    const files: string[] = [];
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) files.push(p);
      }
    };
    walk(srcRoot);
    const total = files.reduce((n, p) => n + (readFileSync(p, 'utf8').match(/SELECT/g) ?? []).length, 0);
    expect(total).toBeLessThan(47);
    // r43 further reduced the tail (26 -> 22); keep the current exact pin.
    expect(total).toBe(20);
  });
});
