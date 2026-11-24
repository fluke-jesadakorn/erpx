import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { companyScopeSelect } from '../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r33-sources';

const srcRoot = resolve(__dirname, '../../../packages/server-customer_authority/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Exact pre-dedupe company-scope CTE text copied verbatim from the r32 baseline
// (BEFORE=49). Two call sites are byte-identical; the third is the same probe
// with a caller-owned trailing column list. Only that list is parameterized.
const SCOPE = 'SELECT ?::text company_id';
const WORKFORCE = 'SELECT ?::text company_id,?::text branch_id,?::text department_id';
const WORKFORCE_COLUMNS = ',?::text branch_id,?::text department_id';

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('Customer Authority read-tail r33 company-scope probe family', () => {
  test('renders byte-identically to all three pre-dedupe scope probes', () => {
    expect(companyScopeSelect()).toBe(SCOPE);
    expect(companyScopeSelect(WORKFORCE_COLUMNS)).toBe(WORKFORCE);
    expect(SCOPE).toBe(WORKFORCE.slice(0, SCOPE.length));
    for (const sql of [SCOPE, WORKFORCE]) readOnly(sql);
  });

  test('one shared literal, three pinned callers and intact control plane', () => {
    const shared = src('customer-authority-read-tail-r33-sources.ts');
    expect(shared.match(/\bSELECT\b/g) ?? []).toHaveLength(1);
    expect(shared).toContain('export const companyScopeSelect');
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);

    const obligations = src('package-obligations.ts');
    expect(obligations.match(/companyScopeSelect\(/g) ?? []).toHaveLength(1);
    expect(obligations).toContain('WITH scope AS(${companyScopeSelect()})');
    expect(obligations).not.toContain('WITH scope AS(SELECT ?::text company_id)');
    expect(obligations).toContain("database.prepare('LOCK TABLE command_receipts,audit_events,outbox_events IN SHARE MODE').run()");

    const gate = src('portal-release-gate.ts');
    expect(gate.match(/companyScopeSelect\(/g) ?? []).toHaveLength(1);
    expect(gate).toContain('const gateSource=`(${companyScopeSelect()}) ${gateAlias}`;');
    expect(gate).not.toContain('(SELECT ?::text company_id)');
    expect(gate.match(/releaseGateRead\(/g) ?? []).toHaveLength(2);

    const read = src('attachment-read.ts');
    expect(read.match(/companyScopeSelect\(/g) ?? []).toHaveLength(1);
    expect(read).toContain("`FROM(${companyScopeSelect(',?::text branch_id,?::text department_id')})workforce_actor_scope`");
    expect(read).not.toContain('FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope');
    expect(read).toContain('existenceProbe(');
  });

  test('r21-r32 builders, source locks, native kinds and eligibility helpers stay untouched', () => {
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
    ];
    for (const [name, count, symbol] of counts) {
      const helper = src(name);
      expect(helper.match(/\bSELECT\b/g) ?? [], name).toHaveLength(count);
      expect(helper, name).toContain(symbol);
      expect(helper, name).not.toContain('companyScopeSelect');
    }
    for (const name of ['manager-eligibility.ts', 'assignment-eligibility-source.ts', 'portal-source-locks.ts', 'portal_commerce_locks.rs', 'native-owner-read.ts', 'portal-access.ts', 'portal-support-sources.ts']) {
      expect(src(name), name).not.toContain('companyScopeSelect');
    }
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=49', () => {
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
    expect(total).toBeLessThan(49);
    // r38 further reduced the tail (40 -> 39); keep the current exact pin.
    expect(total).toBe(39);
  });
});
