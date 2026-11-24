import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { existenceProbe } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r30-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Exact pre-dedupe call-site text copied verbatim from HEAD (r29 baseline, BEFORE=56).
// Only closed FROM/WHERE text is parameterized; emitted SQL is pinned byte-for-byte.
const BINDING_FROM = 'customer_access_bindings binding JOIN customer_access_identities identity ON identity.binding_id=binding.id';
const BINDING_PRED = "binding.id=? AND binding.company_id=? AND binding.status='approved' AND binding.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND identity.user_id=? AND identity.email=? AND binding.email=?";
const VERSION_FROM = 'customer_access_bindings';
const VERSION_PRED = "id=? AND company_id=? AND version=? AND status='approved' AND expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint";
const WORKFORCE_FROM = '(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope';
const WORKFORCE_PRED = 'b.company_id=? AND b.branch_id=?';
const IDENTITY_PRED = 'binding_id=? AND user_id=? AND email=?';

const BINDING_ORIGINAL = `SELECT 1 FROM ${BINDING_FROM} WHERE ${BINDING_PRED}`;
const VERSION_ORIGINAL = `EXISTS(SELECT 1 FROM ${VERSION_FROM} WHERE ${VERSION_PRED})`;
const WORKFORCE_ORIGINAL = `SELECT 1 FROM${WORKFORCE_FROM} WHERE ${WORKFORCE_PRED}`;
const IDENTITY_ORIGINAL = `SELECT 1 FROM customer_access_identities WHERE ${IDENTITY_PRED}`;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT 1');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r30 owner existence-probe family', () => {
  test('renders byte-identically to every pre-dedupe call site', () => {
    expect(existenceProbe(`FROM ${BINDING_FROM}`, BINDING_PRED)).toBe(BINDING_ORIGINAL);
    expect(`EXISTS(${existenceProbe(`FROM ${VERSION_FROM}`, VERSION_PRED)})`).toBe(VERSION_ORIGINAL);
    expect(existenceProbe(`FROM${WORKFORCE_FROM}`, WORKFORCE_PRED)).toBe(WORKFORCE_ORIGINAL);
    expect(existenceProbe('FROM customer_access_identities', IDENTITY_PRED)).toBe(IDENTITY_ORIGINAL);
    for (const sql of [BINDING_ORIGINAL, VERSION_ORIGINAL, WORKFORCE_ORIGINAL, IDENTITY_ORIGINAL]) readOnly(sql);
  });

  test('one shared literal, no inline copies and pinned caller arguments', () => {
    const shared = src('customer-authority-read-tail-r30-sources.ts');
    expect(shared.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(shared).toContain('export const existenceProbe');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const withdraw = src('attachment-withdraw-proof.ts');
    // The link-scope SELECT is a separate untouched read, not part of this family.
    expect(withdraw.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(withdraw).toContain('existenceProbe(\n  \'FROM customer_access_bindings binding');
    expect(withdraw).not.toContain(BINDING_ORIGINAL);
    expect(withdraw).toContain('bindingIdentityCurrentSql');

    const facts = src('attachments-portal-facts.ts');
    expect(facts.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(facts).toContain("EXISTS(${existenceProbe('FROM customer_access_bindings'");
    expect(facts).not.toContain(VERSION_ORIGINAL);

    const read = src('attachment-read.ts');
    expect(read).toContain("existenceProbe('FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope',current.where)");
    expect(read).not.toContain(WORKFORCE_ORIGINAL);

    const access = src('portal-access.ts');
    // The remaining SELECT is the FOR UPDATE binding lock, which is untouched.
    expect(access.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(access).toContain("existenceProbe('FROM customer_access_identities','binding_id=? AND user_id=? AND email=?')");
    expect(access).not.toContain(IDENTITY_ORIGINAL);
  });

  test('r21-r29 builders, source locks, native kinds and manager helpers stay untouched', () => {
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
    ];
    for (const [name, count, symbol] of counts) {
      const helper = src(name);
      expect(helper.match(/\bSELECT\b/g) ?? [], name).toHaveLength(count);
      expect(helper, name).toContain(symbol);
      expect(helper, name).not.toContain('existenceProbe');
    }
    for (const name of ['manager-eligibility.ts', 'portal-source-locks.ts', 'portal_commerce_locks.rs', 'native-owner-read.ts']) {
      expect(src(name), name).not.toContain('existenceProbe');
    }
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=56', () => {
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
    expect(total).toBeLessThan(56);
    expect(total).toBeLessThanOrEqual(52);
  });
});
