import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { pointRead } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r36-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Exact pre-dedupe SQL copied verbatim from the r35 baseline (BEFORE=42).
const RESPONSE = 'SELECT id,response_version,message,created_at,actor_id FROM customer_portal_support_responses WHERE request_id=? ORDER BY response_version DESC LIMIT 1';
const REQUEST = "SELECT id FROM customer_portal_actions WHERE binding_id=? AND user_id=? AND action='request_support'";

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r36 point-read family', () => {
  test('renders byte-identically to both pre-dedupe single-row reads', () => {
    expect(pointRead('id,response_version,message,created_at,actor_id', 'customer_portal_support_responses', 'request_id=?', ' ORDER BY response_version DESC LIMIT 1')).toBe(RESPONSE);
    expect(pointRead('id', 'customer_portal_actions', "binding_id=? AND user_id=? AND action='request_support'")).toBe(REQUEST);
    for (const sql of [RESPONSE, REQUEST]) readOnly(sql);
  });

  test('one shared scaffold, two pinned callers and intact locks/kinds/eligibility', () => {
    const shared = src('customer-authority-read-tail-r36-sources.ts');
    expect(shared.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(shared).toContain('export const pointRead');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const portal = src('portal-support.ts');
    expect(portal.match(/pointRead\(/g) ?? []).toHaveLength(2);
    expect(portal).not.toContain(RESPONSE);
    expect(portal).not.toContain(REQUEST);
    // Locks, native kinds and eligibility helpers stay byte-identical.
    expect(portal).toContain('LOCK TABLE company_package_grants IN SHARE ROW EXCLUSIVE MODE');
    expect(portal).toContain('customerPortalSourceReadLocks(database)');
    expect(portal).toContain('portalSupportSourceReadLocks(database)');
    expect(portal).toContain("'quote','sales'");

    const access = src('portal-access.ts');
    expect(access).toContain('FOR UPDATE');
    expect(access).not.toContain('pointRead');
    const eligibility = src('assignment-eligibility-source.ts');
    expect(eligibility).toContain('export const');
    expect(eligibility).not.toContain('pointRead');
    const locks = src('portal-source-locks.ts');
    expect(locks).not.toContain('pointRead');
    const kinds = src('portal-purchase-binding-source.ts');
    expect(kinds).toContain('o.kind IN (${kinds})');
    expect(kinds).not.toContain('pointRead');
  });

  test('r21-r35 builders and source locks stay untouched', () => {
    const counts: Array<[string, number, string]> = [
      ['customer-authority-read-sql.ts', 2, 'export const jsonbAggFact'],
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
      ['customer-authority-read-tail-r33-sources.ts', 1, 'export const companyScopeSelect'],
      ['customer-authority-read-tail-r34-sources.ts', 0, 'export const bindingCommerceOrderSource'],
      ['customer-authority-read-tail-r35-sources.ts', 0, 'export const scalarProjection'],
      ['portal-binding-source.ts', 1, 'export const portalBindingSource'],
      ['portal-purchase-binding-source.ts', 1, 'export const portalPurchaseBindingSource'],
      // r37 moves supportSource's full-row SELECT head into allColumnsProjection;
      // the installed-regclass probe is the only literal SELECT left in this file.
      ['portal-support-sources.ts', 1, 'const supportSource'],
    ];
    for (const [name, count, symbol] of counts) {
      const helper = src(name);
      expect(helper.match(/\bSELECT\b/g) ?? [], name).toHaveLength(count);
      expect(helper, name).toContain(symbol);
      expect(helper, name).not.toContain('pointRead');
    }
  });

  test('retirement:ts-statements is strictly below the recorded BEFORE=42', () => {
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
    expect(total).toBeLessThan(42);
    expect(total).toBeLessThanOrEqual(41);
  });
});
