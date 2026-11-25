import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { scalarProjection } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r35-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Exact pre-dedupe SQL copied verbatim from the r34 baseline (BEFORE=43).
const WITNESS = 'SELECT (sourceQueryValue) witness';
const ALLOWED = 'SELECT authorityValue allowed';

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r35 scalar projection family', () => {
  test('renders byte-identically to both pre-dedupe scalar witness reads', () => {
    expect(scalarProjection('(sourceQueryValue)', 'witness')).toBe(WITNESS);
    expect(scalarProjection('authorityValue', 'allowed')).toBe(ALLOWED);
    for (const sql of [WITNESS, ALLOWED]) readOnly(sql);
  });

  test('one shared scaffold, two pinned callers and intact locks/kinds/eligibility', () => {
    const shared = src('customer-authority-read-tail-r35-sources.ts');
    expect(shared.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(shared).toContain('export const scalarProjection');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const attach = src('attachment-purchases.ts');
    expect(attach.match(/scalarProjection\(/g) ?? []).toHaveLength(2);
    expect(attach).not.toContain('SELECT (${sourceQuery(external)}) witness');
    expect(attach).not.toContain('SELECT ${authority.where} allowed');
    // Locks, native kinds and eligibility helpers stay byte-identical.
    expect(attach).toContain('companyPurchaseReadLocks(database,company)');
    expect(attach).toContain('commerceCompleteOwnerSourceReadLocks(database)');
    expect(attach).toContain('customerPortalSourceReadLocks(database)');
    expect(attach).toContain("assignmentReadSql('portal_file_staff','commerce.read')");

    const access = src('portal-access.ts');
    expect(access).toContain('CA_RD_PORTAL_BINDING_LOCK');
    expect(access).toContain('nativePortalReadStatement');
    expect(access).not.toContain('scalarProjection');
    const eligibility = src('assignment-eligibility-source.ts');
    expect(eligibility).toContain('export const');
    expect(eligibility).not.toContain('scalarProjection');
    const kinds = src('portal-purchase-binding-source.ts');
    expect(kinds).toContain('o.kind IN (${kinds})');
    expect(kinds).not.toContain('scalarProjection');
  });

  test('r21-r34 builders and source locks stay untouched', () => {
    const counts: Array<[string, number, string]> = [
      ['customer-authority-read-sql.ts', 1, 'export const jsonbAggFact'],
      ['package-obligation-sql.ts', 1, 'export const latestSupportVersion'],
      ['customer-authority-read-tail-r23-sources.ts', 0, 'export const jsonbImageProjection'],
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
      ['customer-authority-read-tail-r34-sources.ts', 0, 'export const bindingCommerceOrderSource'],
      ['portal-binding-source.ts', 0, 'export const portalBindingSource'],
      ['portal-purchase-binding-source.ts', 0, 'export const portalPurchaseBindingSource'],
    ];
    for (const [name, count, symbol] of counts) {
      const helper = src(name);
      expect(helper.match(/\bSELECT\b/g) ?? [], name).toHaveLength(count);
      expect(helper, name).toContain(symbol);
      expect(helper, name).not.toContain('scalarProjection');
    }
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=43', () => {
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
    expect(total).toBeLessThan(43);
    // r43 further reduced the tail (26 -> 22); keep the current exact pin.
    expect(total).toBe(15);
  });
});
