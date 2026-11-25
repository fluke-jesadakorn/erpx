import { describe, expect, test, vi } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
vi.mock('server-only', () => ({}));
import { benefitExistsHead, benefitOwnerExistsGuard } from '../../../packages/server-benefits/src/adapters/benefits-read-sql';
import { benefitPackageSettlementSources } from '../../../packages/server-benefits/src/adapters/package-obligations';

const srcRoot = resolve(__dirname, '../../../packages/server-benefits/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');

// Byte oracles captured at HEAD before the r28 fold. The byte-identical
// head `EXISTS(SELECT 1 FROM ${table} ${tail})` was duplicated across two
// owner-local renderers: the owner existence guard (`WHERE id=? AND ...`)
// and the settlement terminal predicate (`event JOIN command_receipts ...`).
// Both now route through the owner-local benefitExistsHead renderer. Rendered
// SQL, bind order, column references and source locks must be unchanged.
const preDedupe = {
  versionGuard: (table: string) => `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?)`,
  payrollGuard: `EXISTS(SELECT 1 FROM benefit_payroll_inputs WHERE id=? AND engagement_id=? AND status='released' AND employee_deduction_minor=? AND employer_cost_minor=?)`,
  terminal: (table: string, id: string, kind: string) => `EXISTS(SELECT 1 FROM ${table} event JOIN command_receipts command ON command.record_id=${id}.id
 AND command.operation='benefits.${kind}.'||CASE WHEN event.action IN ('create','elect') THEN 'create' ELSE event.action END
 AND command.actor_id=event.actor_id AND command.authority_json::jsonb->>'companyId'=${id}.company_id
 AND command.result_json::jsonb->>'id'=${id}.id AND command.result_json::jsonb->>'status'=event.status
 AND command.result_json::jsonb->>'version'=event.version::text
 WHERE event.${kind === 'window' ? 'window' : kind}_id=${id}.id AND event.status=${id}.status AND event.version<=${id}.version)`,
} as const;

const kinds = [
  ['plans', 'benefit_plan_events', 'p', 'plan', 'benefit_plans', '97b5421d5d4c4a09eb502787a4e6b58aab8aaeaea30afa3502ec761b22db3bc8'],
  ['windows', 'benefit_window_events', 'w', 'window', 'benefit_enrollment_windows', '0bbbcbf19683b66a5aa9bab9a151c9fe33d56bf3cb38de9dc2b267f1f347e6bf'],
  ['dependents', 'benefit_dependent_events', 'd', 'dependent', 'benefit_dependents', '815e70fc0c27e3542558d6ddd65624b386129b53a2b8d418458fb13f027cb760'],
  ['elections', 'benefit_election_events', 'e', 'election', 'benefit_elections', 'a2554a230dbe096c5fc6027190dc5ec31ac6cd8e668ba9f3bec59b898b96628a'],
] as const;

const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

const kpi = () => {
  let total = 0;
  const walk = (dir: string) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) walk(path);
      else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
    }
  };
  walk(srcRoot);
  return total;
};

describe('benefits read-tail r28 exists-head dedupe', () => {
  test.each([
    ['plan version guard', benefitOwnerExistsGuard('benefit_plans', 'company_id=? AND version=?'), preDedupe.versionGuard('benefit_plans'), '44d2905bd272562514d4016ee8ec9d9d4cf1880236f81eb0cdb6eb0ce399d792', 3],
    ['dependent version guard', benefitOwnerExistsGuard('benefit_dependents', 'company_id=? AND version=?'), preDedupe.versionGuard('benefit_dependents'), '5c03ed75728d8d36ac88e279da49e4bc2d9a595f3408d7dffd95b8aba6957148', 3],
    ['election version guard', benefitOwnerExistsGuard('benefit_elections', 'company_id=? AND version=?'), preDedupe.versionGuard('benefit_elections'), '6aae10c24b924da3b656713c9280acdd086931d294d320d0374121517fe5f4a3', 3],
    ['payroll guard', benefitOwnerExistsGuard('benefit_payroll_inputs', "engagement_id=? AND status='released' AND employee_deduction_minor=? AND employer_cost_minor=?"), preDedupe.payrollGuard, '768a133a81160706d8fd3158a1b183100f962ead0578ac930f9ae5aa86ce5ff6', 4],
  ] as const)('%s rendering is byte-identical to the pre-dedupe literal', (_name, rendered, before, digest, binds) => {
    expect(rendered).toBe(before);
    expect(sha(rendered)).toBe(digest);
    expect(Buffer.from(rendered)).toEqual(Buffer.from(before));
    expect(rendered.match(/\?/g)).toHaveLength(binds);
    readOnly(rendered);
  });

  test.each(kinds)('%s settlement source keeps its exact pre-dedupe bytes', (key, events, alias, kind, table, digest) => {
    const before = `SELECT ${alias}.*,${preDedupe.terminal(events, alias, kind)} terminal_proven FROM ${table} ${alias}`;
    const after = benefitPackageSettlementSources[key];
    expect(after).toBe(before);
    expect(sha(after)).toBe(digest);
    expect(Buffer.from(after)).toEqual(Buffer.from(before));
    expect(after.match(/\?/g) ?? []).toHaveLength(0);
    expect(after.match(/SELECT/g)).toHaveLength(2);
    readOnly(after);
  });

  test('the shared renderer reproduces both pre-dedupe head families byte-for-byte', () => {
    expect(benefitExistsHead('benefit_plans', 'WHERE id=? AND company_id=? AND version=?')).toBe(preDedupe.versionGuard('benefit_plans'));
    expect(benefitExistsHead('benefit_plan_events', preDedupe.terminal('benefit_plan_events', 'p', 'plan').slice(
      `EXISTS(SELECT 1 FROM benefit_plan_events `.length, -1))).toBe(preDedupe.terminal('benefit_plan_events', 'p', 'plan'));
    expect(sha(benefitExistsHead('benefit_plans', 'WHERE id=? AND company_id=? AND version=?'))).toBe('44d2905bd272562514d4016ee8ec9d9d4cf1880236f81eb0cdb6eb0ce399d792');
  });

  test('the renderer owns the single EXISTS head and both shape callers delegate to it', () => {
    const head = src('benefits-read-sql.ts');
    expect(head).toContain('export const benefitExistsHead = (table: string, tail: string) =>');
    expect(head).toContain('`EXISTS(SELECT 1 FROM ${table} ${tail})`');
    expect(head.match(/EXISTS\(SELECT 1 FROM \$\{table\} \$\{tail\}\)/g) ?? []).toHaveLength(1);
    expect(head).toContain('benefitOwnerExistsGuard = (table: string, conditions: string) =>');
    expect(head).toContain('benefitExistsHead(table, `WHERE id=? AND ${conditions}`)');
    expect(head.match(/export const benefitExistsHead = \(/g) ?? []).toHaveLength(1);
    expect(head.match(/benefitExistsHead\(/g) ?? []).toHaveLength(1); // delegation only
    const obligations = src('package-obligations.ts');
    expect(obligations).toContain('benefitExistsHead');
    expect(obligations).toContain('return benefitExistsHead(table, `event JOIN command_receipts command ON');
    expect(obligations).not.toContain('`EXISTS(SELECT 1 FROM ${table} event JOIN');
    expect(obligations.match(/benefitExistsHead\(/g) ?? []).toHaveLength(1);
  });

  test('the fold adds no write, row-lock, FOR UPDATE, FOR SHARE, native registry or cross-owner consumer', () => {
    for (const name of ['benefits-read-sql.ts', 'package-obligations.ts']) {
      expect(src(name), name).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE/i);
      expect(src(name), name).not.toContain('native_');
      expect(src(name), name).not.toContain('@erpx/server-benefits/adapters/benefits-native-read');
    }
    for (const [name, count, mode] of [
      ['purchases.ts', 1, 'IN SHARE ROW EXCLUSIVE MODE'],
      ['package-obligations.ts', 1, 'IN SHARE MODE'],
      ['people-package-obligations.ts', 1, 'IN SHARE MODE'],
    ] as const) {
      expect(src(name).match(/LOCK\s+TABLE/g) ?? [], name).toHaveLength(count);
      expect(src(name), name).toContain(mode);
    }
  });

  test('retirement:ts-statements is 30, strictly below the r28 BEFORE=32 baseline', () => {
    expect(kpi()).toBe(30);
    expect(kpi()).toBeLessThan(32);
  });
});
