import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { finalRootOwnerProjectionSources as financeProjectionSources } from '@erpx/server-finance/public/final-root-owner-projection-sources';
import { evidenceSource } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r41-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Exact pre-dedupe source literals copied verbatim from HEAD (BEFORE=28).
// Only the projection columns and the closed FROM/JOIN tail are parameterized.
const IDENTITY_ORIGINAL = `SELECT e.id,e.change_id,e.action,e.customer_version,e.created_at,e.version,c.status change_status,c.version change_version,p.name customer_name,p.code customer_code
  FROM customer_identity_change_events e
  JOIN customer_identity_changes c ON c.id=e.change_id
  JOIN (${financeProjectionSources.finance_parties}) p ON p.id=c.customer_id AND p.company_id=c.company_id AND p.kind='customer'
  WHERE c.company_id=? AND e.created_at>=? AND e.created_at<?`;
const DUPLICATE_ORIGINAL = `SELECT r.id,l.name left_name,l.code left_code,q.name right_name,q.code right_code,r.status,r.created_at,r.updated_at,r.version
    FROM customer_duplicate_reviews r
    JOIN (${financeProjectionSources.finance_parties}) l ON l.id=r.left_customer_id AND l.company_id=r.company_id AND l.kind='customer'
    JOIN (${financeProjectionSources.finance_parties}) q ON q.id=r.right_customer_id AND q.company_id=r.company_id AND q.kind='customer'`;
const IDENTITY_COLS = 'e.id,e.change_id,e.action,e.customer_version,e.created_at,e.version,c.status change_status,c.version change_version,p.name customer_name,p.code customer_code';
const IDENTITY_TAIL = `
  FROM customer_identity_change_events e
  JOIN customer_identity_changes c ON c.id=e.change_id
  JOIN (${financeProjectionSources.finance_parties}) p ON p.id=c.customer_id AND p.company_id=c.company_id AND p.kind='customer'
  WHERE c.company_id=? AND e.created_at>=? AND e.created_at<?`;
const DUPLICATE_COLS = 'r.id,l.name left_name,l.code left_code,q.name right_name,q.code right_code,r.status,r.created_at,r.updated_at,r.version';
const DUPLICATE_TAIL = `
    FROM customer_duplicate_reviews r
    JOIN (${financeProjectionSources.finance_parties}) l ON l.id=r.left_customer_id AND l.company_id=r.company_id AND l.kind='customer'
    JOIN (${financeProjectionSources.finance_parties}) q ON q.id=r.right_customer_id AND q.company_id=r.company_id AND q.kind='customer'`;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r41 executive-evidence family', () => {
  test('renders byte-identically to both pre-dedupe source literals', () => {
    const identity = evidenceSource(IDENTITY_COLS, IDENTITY_TAIL);
    const duplicate = evidenceSource(DUPLICATE_COLS, DUPLICATE_TAIL);
    expect(identity).toBe(IDENTITY_ORIGINAL);
    expect(duplicate).toBe(DUPLICATE_ORIGINAL);
    readOnly(identity);
    readOnly(duplicate);
  });

  test('one canonical head, two delegating callers, prepared statements and binds intact', () => {
    const head = src('customer-authority-read-tail-r41-sources.ts');
    expect(head.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(head.match(/export const evidenceSource/g) ?? []).toHaveLength(1);
    expect(head).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const exec = src('executive-main-evidence-sources.ts');
    expect(exec.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(exec.match(/evidenceSource\(/g) ?? []).toHaveLength(2);
    expect(exec).toContain('customer-authority-read-tail-r41-sources');
    expect(exec).toContain('factsSearchDuplicateReviewEvidence');
    expect(exec).toContain('factsReadDuplicateReviewEvidence');
    expect(exec).toContain('factsSearchIdentityChangeEvidence');
    expect(exec).toContain('factsReadIdentityChangeEvidence');
    expect(exec.match(/\.bind\(/g) ?? []).toHaveLength(4);
    expect(exec).toContain('.bind(...values, input.limit + 1)');
    expect(exec).toContain('.bind(actor.companyId, Date.parse(filters.from), Date.parse(filters.to) + 86400000, id)');
  });

  test('r21-r40 helpers, locks, native kinds and eligibility stay untouched', () => {
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
      ['customer-authority-read-tail-r35-sources.ts', 0, 'export const scalarProjection'],
      ['customer-authority-read-tail-r36-sources.ts', 0, 'export const pointRead'],
      ['customer-authority-read-tail-r37-sources.ts', 0, 'export const allColumnsProjection'],
      ['customer-authority-read-tail-r40-sources.ts', 1, 'export const selectProjection'],
    ];
    for (const [name, count, symbol] of counts) {
      const helper = src(name);
      expect(helper.match(/\bSELECT\b/g) ?? [], name).toHaveLength(count);
      expect(helper, name).toContain(symbol);
      expect(helper, name).not.toContain('evidenceSource');
    }

    expect(src('portal-access.ts')).toContain('FOR UPDATE');
    expect(src('portal-source-locks.ts')).toContain('LOCK TABLE');
    expect(src('portal-purchase-binding-source.ts')).toContain('o.kind IN (${kinds})');
    expect(src('package-obligations.ts')).toContain("database.prepare('LOCK TABLE command_receipts,audit_events,outbox_events IN SHARE MODE').run()");
    expect(src('assignment-eligibility-source.ts')).toContain('export const assignmentSource');
    expect(src('manager-eligibility.ts')).toContain('export const');
    for (const name of [
      'portal-access.ts', 'portal-source-locks.ts', 'portal-purchase-binding-source.ts',
      'assignment-eligibility-source.ts', 'manager-eligibility.ts', 'package-obligations.ts',
    ]) {
      expect(src(name), name).not.toContain('evidenceSource');
    }
  });

  test('retirement:ts-statements is strictly below the recorded BEFORE=28', () => {
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
    expect(total).toBeLessThan(28);
    expect(total).toBe(17);
  });
});
