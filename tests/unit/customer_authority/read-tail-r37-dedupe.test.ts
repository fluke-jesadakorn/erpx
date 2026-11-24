import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { allColumnsProjection } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r37-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Exact pre-dedupe full-row reads copied verbatim from the r36 baseline (BEFORE=41).
const PROJECTION_BINDINGS = 'SELECT * FROM customer_access_bindings WHERE email=?';
const PROJECTION_SUPPORT = (table: string) => `SELECT * FROM customer_portal_support_${table}`;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r37 full-row projection family', () => {
  test('renders byte-identically to both pre-dedupe full-row reads', () => {
    expect(allColumnsProjection('customer_access_bindings', ' WHERE email=?')).toBe(PROJECTION_BINDINGS);
    for (const table of ['responses', 'acknowledgements', 'closures']) {
      expect(allColumnsProjection(`customer_portal_support_${table}`)).toBe(PROJECTION_SUPPORT(table));
    }
    for (const sql of [PROJECTION_BINDINGS, ...['responses', 'acknowledgements', 'closures'].map(PROJECTION_SUPPORT)]) readOnly(sql);
  });

  test('one shared scaffold, two pinned callers and intact locks/kinds/eligibility', () => {
    const shared = src('customer-authority-read-tail-r37-sources.ts');
    expect(shared.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(shared).toContain('export const allColumnsProjection');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const unit = src('portal-projection-unit.ts');
    expect(unit).toContain("allColumnsProjection('customer_access_bindings',' WHERE email=?')");
    expect(unit).not.toContain(PROJECTION_BINDINGS);

    const support = src('portal-support-sources.ts');
    expect(support).toContain('const supportSource');
    expect(support).toContain('allColumnsProjection(`customer_portal_support_${table}`)');
    expect(support).not.toContain(PROJECTION_SUPPORT('responses'));
    // Lock statements stay byte-identical.
    expect(support).toContain('LOCK TABLE customer_portal_support_responses,customer_portal_support_acknowledgements,customer_portal_support_closures IN SHARE ROW EXCLUSIVE MODE');
    expect(support).toContain('export function portalSupportSourceReadLocks');

    const access = src('portal-access.ts');
    expect(access).toContain('FOR UPDATE');
    expect(access).not.toContain('allColumnsProjection');
    const eligibility = src('assignment-eligibility-source.ts');
    expect(eligibility).toContain('export const');
    expect(eligibility).not.toContain('allColumnsProjection');
    const locks = src('portal-source-locks.ts');
    expect(locks).not.toContain('allColumnsProjection');
    const kinds = src('portal-purchase-binding-source.ts');
    expect(kinds).toContain('o.kind IN (${kinds})');
    expect(kinds).not.toContain('allColumnsProjection');
  });

  test('r21-r36 builders and source locks stay untouched', () => {
    const counts: Array<[string, number, string]> = [
      ['customer-authority-read-sql.ts', 2, 'export const jsonbAggFact'],
      ['package-obligation-sql.ts', 3, 'export const latestSupportVersion'],
      ['customer-authority-read-tail-r23-sources.ts', 1, 'export const jsonbImageProjection'],
      ['customer-authority-read-tail-r24-sources.ts', 1, 'export const bindingFactProjection'],
      ['customer-authority-read-tail-r25-sources.ts', 1, 'export const ownerColumnProjection'],
      ['customer-authority-read-tail-r26-sources.ts', 1, 'export const accessBindingOrderProjection'],
      ['customer-authority-read-tail-r27-sources.ts', 1, 'export const accessEventEvidenceProbe'],
      ['customer-authority-read-tail-r28-sources.ts', 1, 'export const releaseGateRead'],
      ['customer-authority-read-tail-r29-sources.ts', 1, 'export const reviewEligibilityRows'],
      ['customer-authority-read-tail-r30-sources.ts', 1, 'export const existenceProbe'],
      ['customer-authority-read-tail-r31-sources.ts', 1, 'export const supportConclusionIdentity'],
      ['customer-authority-read-tail-r32-sources.ts', 1, 'export const factExistenceWitness'],
      ['customer-authority-read-tail-r33-sources.ts', 1, 'export const companyScopeSelect'],
      ['customer-authority-read-tail-r34-sources.ts', 1, 'export const bindingCommerceOrderSource'],
      ['customer-authority-read-tail-r35-sources.ts', 1, 'export const scalarProjection'],
      ['customer-authority-read-tail-r36-sources.ts', 1, 'export const pointRead'],
      ['portal-binding-source.ts', 1, 'export const portalBindingSource'],
      ['portal-purchase-binding-source.ts', 1, 'export const portalPurchaseBindingSource'],
      // supportSource's SELECT head now lives in the r37 builder; only the
      // installed-regclass probe remains as a literal SELECT in this file.
      ['portal-support-sources.ts', 1, 'const supportSource'],
    ];
    for (const [name, count, symbol] of counts) {
      const helper = src(name);
      expect(helper.match(/\bSELECT\b/g) ?? [], name).toHaveLength(count);
      expect(helper, name).toContain(symbol);
      if (name === 'portal-support-sources.ts') expect(helper, name).toContain('allColumnsProjection');
      else expect(helper, name).not.toContain('allColumnsProjection');
    }
  });

  test('retirement:ts-statements is strictly below the recorded BEFORE=41', () => {
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
    expect(total).toBeLessThan(41);
    expect(total).toBeLessThanOrEqual(40);
  });
});
