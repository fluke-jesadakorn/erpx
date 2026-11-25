import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { ownerColumnProjection } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r25-sources';
import { bindingCommerceOrderSource, unionFactRow } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r34-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Exact r25/r34 projection/source heads are byte-identical parameterizations of
// `SELECT <projection> FROM <source>`; the r38 family owns one canonical head.
const BINDINGS_COLS = `"id","company_id","order_id","customer_id","email","expires_at","status","reason","created_by","approved_by","reviewed_at","version","created_at","updated_at"`;
const IDENTITIES_COLS = `"binding_id","user_id","email","bound_at"`;
const HEADS: Array<[string, string]> = [
  [BINDINGS_COLS, 'customer_access_bindings'],
  [IDENTITIES_COLS, 'customer_access_identities'],
  [`'binding' kind,binding.id,to_jsonb(binding) fact,PENDING pending`, 'bindings binding LEFT JOIN commerce_orders original ON original.id=binding.order_id'],
  [`'closure',closure.id,to_jsonb(closure),PENDING`, 'customer_portal_support_closures closure CROSS JOIN scope LEFT JOIN customer_access_bindings binding ON binding.id=closure.binding_id'],
];

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r38 canonical projection/source head family', () => {
  test('r25 and r34 render byte-identically through one canonical head', () => {
    for (const [projection, source] of HEADS) {
      const expected = `SELECT ${projection} FROM ${source}`;
      expect(ownerColumnProjection(projection, source)).toBe(expected);
      expect(unionFactRow(projection, source)).toBe(expected);
      expect(unionFactRow(projection, source)).toBe(ownerColumnProjection(projection, source));
      readOnly(expected);
    }
  });

  test('one canonical head, r34 delegates, and r21-r37 builders stay intact', () => {
    const r25 = src('customer-authority-read-tail-r25-sources.ts');
    expect(r25.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(r25).toContain('export const ownerColumnProjection');
    expect(r25).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const r34 = src('customer-authority-read-tail-r34-sources.ts');
    expect(r34.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(r34).toContain('export const bindingCommerceOrderSource');
    expect(r34).toContain('export const unionFactRow');
    expect(r34).toContain('ownerColumnProjection(projection, from)');
    expect(r34).not.toContain('`SELECT ${projection} FROM ${from}`');
    expect(r34).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    // The r34 binding/order core keeps its own distinct head and is untouched.
    expect(bindingCommerceOrderSource('c', 'from', '')).toContain('FROM from JOIN (');
    expect(r34).toContain('o.id=b.order_id AND o.company_id=b.company_id');

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
      ['customer-authority-read-tail-r34-sources.ts', 0, 'export const bindingCommerceOrderSource'],
      ['customer-authority-read-tail-r35-sources.ts', 0, 'export const scalarProjection'],
      ['customer-authority-read-tail-r36-sources.ts', 0, 'export const pointRead'],
      ['customer-authority-read-tail-r37-sources.ts', 0, 'export const allColumnsProjection'],
    ];
    for (const [name, count, symbol] of counts) {
      const helper = src(name);
      expect(helper.match(/\bSELECT\b/g) ?? [], name).toHaveLength(count);
      expect(helper, name).toContain(symbol);
    }
  });

  test('locks, source locks, native kinds and eligibility helpers are untouched', () => {
    const access = src('portal-access.ts');
    expect(access).toContain('FOR UPDATE');
    expect(access).not.toContain('ownerColumnProjection');
    expect(access).not.toContain('unionFactRow');

    const locks = src('portal-source-locks.ts');
    expect(locks).toContain('LOCK TABLE');
    expect(locks).not.toContain('ownerColumnProjection');
    expect(locks).not.toContain('unionFactRow');

    const kinds = src('portal-purchase-binding-source.ts');
    expect(kinds).toContain('o.kind IN (${kinds})');
    expect(kinds).not.toContain('ownerColumnProjection');
    expect(kinds).not.toContain('unionFactRow');

    const eligibility = src('assignment-eligibility-source.ts');
    expect(eligibility).toContain('export const assignmentSource');
    expect(eligibility).not.toContain('ownerColumnProjection');
    expect(eligibility).not.toContain('unionFactRow');

    const lastAudit = src('last-audit-owner-projection-sources.ts');
    expect(lastAudit.match(/ownerColumnProjection\(/g) ?? []).toHaveLength(2);
    const facts = src('package-obligations.ts');
    expect(facts.match(/unionFactRow\(/g) ?? []).toHaveLength(4);
    expect(facts).toContain("database.prepare('LOCK TABLE command_receipts,audit_events,outbox_events IN SHARE MODE').run()");
  });

  test('retirement:ts-statements is strictly below the recorded BEFORE=40', () => {
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
    expect(total).toBeLessThan(40);
    expect(total).toBe(20);
  });
});
