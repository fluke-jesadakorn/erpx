import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  ppRd44AuthorityProbe,
  ppRd44NamedSourceProjection,
  ppRd44PurchaseClockProbe,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r44-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;

const render = {
  ppRd44NamedSourceProjection: (a: string[]) => ppRd44NamedSourceProjection(a[0], a[1], a[2]),
  ppRd44AuthorityProbe: (a: string[]) => ppRd44AuthorityProbe(a[0], a[1]),
  ppRd44PurchaseClockProbe: (a: string[]) => ppRd44PurchaseClockProbe(a[0], a[1], a[2]),
};
type Sym = keyof typeof render;
type Row = { file: string; sym: Sym; args: string[]; literal: string; call: string };

// Each literal is copied verbatim from the pre-r44 inline call site (the § token
// marks a fixed owner fragment that is passed as a builder argument). Bind arrays stay unchanged.
const oracles: Row[] = [
  {
    file: 'collaboration-notification-snapshot-sources.ts',
    sym: 'ppRd44NamedSourceProjection',
    args: ['"id", "shift_id", "person_id", "engagement_id", "state", "version"', 'people_roster_assignments', 'a'],
    literal: 'FROM (SELECT "id", "shift_id", "person_id", "engagement_id", "state", "version" FROM people_roster_assignments) a',
    call: `ppRd44NamedSourceProjection('"id", "shift_id", "person_id", "engagement_id", "state", "version"', 'people_roster_assignments', 'a')`,
  },
  {
    file: 'identity-snapshot-sources.ts',
    sym: 'ppRd44NamedSourceProjection',
    args: ['"company_id", "person_id", "case_id", "effective_at", "cancelled_at", "restored_at"', 'people_access_cutoffs', 'x'],
    literal: 'FROM (SELECT "company_id", "person_id", "case_id", "effective_at", "cancelled_at", "restored_at" FROM people_access_cutoffs) x',
    call: `ppRd44NamedSourceProjection('"company_id", "person_id", "case_id", "effective_at", "cancelled_at", "restored_at"', 'people_access_cutoffs', 'x')`,
  },
  {
    file: 'finance-owned-snapshot-sources.ts',
    sym: 'ppRd44NamedSourceProjection',
    args: ['"id","run_id","journal_id","created_at","account_mapping_id","account_mapping_version"', 'hr_payroll_postings', 'p'],
    literal: 'FROM (SELECT "id","run_id","journal_id","created_at","account_mapping_id","account_mapping_version" FROM hr_payroll_postings) p',
    call: `ppRd44NamedSourceProjection('"id","run_id","journal_id","created_at","account_mapping_id","account_mapping_version"', 'hr_payroll_postings', 'p')`,
  },
  {
    file: 'workflow-final-owner-projections.ts',
    sym: 'ppRd44NamedSourceProjection',
    args: ['"id","user_id","company_id","legal_name","preferred_name","email","phone","country_code","status","version","created_by","created_at","updated_at","merged_into_id","work_email","work_phone"', 'hr_people', 'hr_people'],
    literal: 'FROM (SELECT "id","user_id","company_id","legal_name","preferred_name","email","phone","country_code","status","version","created_by","created_at","updated_at","merged_into_id","work_email","work_phone" FROM hr_people) hr_people',
    call: `ppRd44NamedSourceProjection('"id","user_id","company_id","legal_name","preferred_name","email","phone","country_code","status","version","created_by","created_at","updated_at","merged_into_id","work_email","work_phone"', 'hr_people', 'hr_people')`,
  },
  {
    file: 'expense-ai.ts',
    sym: 'ppRd44AuthorityProbe',
    args: ['\u00a7Q', 'allowed'],
    literal: 'SELECT \u00a7Q allowed',
    call: `ppRd44AuthorityProbe(authority.query, 'allowed')`,
  },
  {
    file: 'expense-record-purchases.ts',
    sym: 'ppRd44AuthorityProbe',
    args: ['\u00a7Q', 'unchanged'],
    literal: 'SELECT \u00a7Q unchanged',
    call: `ppRd44AuthorityProbe(routingAuthority.query, 'unchanged')`,
  },
  {
    file: 'expenses.ts',
    sym: 'ppRd44AuthorityProbe',
    args: ['\u00a7Q', 'ok'],
    literal: 'SELECT \u00a7Q ok',
    call: `ppRd44AuthorityProbe(ppRd29FinanceExpenseTreasurySql(expensePaymentTreasurySql('e')), 'ok')`,
  },
  {
    file: 'payroll-readiness.ts',
    sym: 'ppRd44PurchaseClockProbe',
    args: ['\u00a7P', '\u00a7C', 'payroll_purchase_clock'],
    literal: '(SELECT \u00a7P FROM (SELECT \u00a7C now_ms) payroll_purchase_clock)',
    call: `ppRd44PurchaseClockProbe(purchasedCapabilityPredicate(company,"people.payroll",purpose,"payroll_purchase_clock.now_ms"), packageClockSql, 'payroll_purchase_clock')`,
  },
  {
    file: 'workforce-purchases.ts',
    sym: 'ppRd44PurchaseClockProbe',
    args: ['\u00a7P', '\u00a7C', 'workforce_history_clock'],
    literal: '(SELECT \u00a7P FROM (SELECT \u00a7C now_ms) workforce_history_clock)',
    call: `ppRd44PurchaseClockProbe(purchasedCapabilityPredicate(companySql,"people.workforce","history","workforce_history_clock.now_ms"), packageClockSql, 'workforce_history_clock')`,
  },
];

describe('People Payroll read-tail r44 three-family dedupe', () => {
  test('the three shared builders own byte-identical, read-only literals', () => {
    expect(ppRd44NamedSourceProjection('"a"', 't', 'x')).toBe('FROM (SELECT "a" FROM t) x');
    expect(ppRd44AuthorityProbe('\u00a7Q', 'allowed')).toBe('SELECT \u00a7Q allowed');
    expect(ppRd44PurchaseClockProbe('\u00a7P', '\u00a7C', 'clk')).toBe('(SELECT \u00a7P FROM (SELECT \u00a7C now_ms) clk)');
    for (const o of oracles) {
      expect(render[o.sym](o.args), o.file + ' ' + o.sym).toBe(o.literal);
      readOnly(o.literal);
    }
    expect(oracles.length).toBe(9);
  });

  test('each caller uses its builder, keeps no inline copy and keeps its binds', () => {
    const shared = src('pp-read-tail-r44-sources.ts');
    expect((shared.match(/export const ppRd44/g) ?? []).length).toBe(3);
    const expectedCounts: Record<string, [Sym, number]> = {
      'collaboration-notification-snapshot-sources.ts': ['ppRd44NamedSourceProjection', 3],
      'identity-snapshot-sources.ts': ['ppRd44NamedSourceProjection', 11],
      'finance-owned-snapshot-sources.ts': ['ppRd44NamedSourceProjection', 3],
      'workflow-final-owner-projections.ts': ['ppRd44NamedSourceProjection', 3],
      'expense-ai.ts': ['ppRd44AuthorityProbe', 1],
      'expense-record-purchases.ts': ['ppRd44AuthorityProbe', 1],
      'expenses.ts': ['ppRd44AuthorityProbe', 1],
      'payroll-readiness.ts': ['ppRd44PurchaseClockProbe', 1],
      'workforce-purchases.ts': ['ppRd44PurchaseClockProbe', 1],
    };
    for (const o of oracles) {
      expect(src(o.file), o.file + ' builder call').toContain(o.call);
    }
    for (const name of ['collaboration-notification-snapshot-sources.ts', 'identity-snapshot-sources.ts', 'finance-owned-snapshot-sources.ts', 'workflow-final-owner-projections.ts']) {
      expect(src(name), name + ' inline named projection removed').not.toMatch(/FROM \(SELECT "/);
    }
    expect(src('expense-ai.ts')).not.toContain('SELECT ${authority.query} allowed');
    expect(src('expense-record-purchases.ts')).not.toContain('SELECT ${routingAuthority.query} unchanged');
    expect(src('expenses.ts')).not.toContain("SELECT ${ppRd29FinanceExpenseTreasurySql(expensePaymentTreasurySql('e'))} ok");
    expect(src('payroll-readiness.ts')).not.toContain('now_ms) payroll_purchase_clock)`');
    expect(src('workforce-purchases.ts')).not.toContain('now_ms) workforce_history_clock)`');
    expect(src('expense-ai.ts')).toContain('.bind(...authority.args)');
    expect(src('expense-record-purchases.ts')).toContain('.bind(...routingAuthority.args)');
    expect(src('expenses.ts')).toContain('.bind(expense.id,actor.companyId,actor.assignmentId,actor.userId)');
    expect(src('payroll-readiness.ts')).toContain('purchasedCapabilityPredicate(company,"people.payroll",purpose,"payroll_purchase_clock.now_ms")');
    expect(src('workforce-purchases.ts')).toContain('purchasedCapabilityPredicate(companySql,"people.workforce","history","workforce_history_clock.now_ms")');
    for (const [name, [sym, count]] of Object.entries(expectedCounts)) {
      expect(uses(name, sym), name).toBe(count);
    }
  });

  test('locks, native kinds, cross-owner projections and r1-r43 builders stay untouched', () => {
    expect(src('people-evaluation.ts')).toContain('SELECT id FROM hr_engagements WHERE id=? FOR UPDATE');
    expect(src('payroll-purchases.ts')).toContain('FOR SHARE');
    expect(src('payroll-statutory.ts')).toContain('pg_advisory_xact_lock');
    expect(src('people-payroll.ts')).toContain('nativePpStatement');
    expect(src('pp-read-tail-r49-sources.ts')).toContain('filesProjectionSources.attachments');
    expect(src('pp-read-tail-r51-sources.ts')).toContain('financeProjectionSources.finance_accounts');
    expect(src('pp-read-tail-r51-sources.ts')).toContain('filesProjectionSources.attachments');
    expect(src('pp-read-tail-r43-sources.ts')).toContain('export const ppRd43ActiveLeavePolicyProjection');
    expect(src('pp-read-tail-r42-sources.ts')).toContain('export const ppRd42FourColOwnerProjection');
    expect(src('pp-read-tail-r40-sources.ts')).toContain('export const ppRd40UnlinkedUserByCompany');
    expect(src('pp-read-tail-r38-sources.ts')).toContain('export const ppRd38SinglePredTail');
    expect(src('pp-read-tail-r32-sources.ts')).toContain('export const ppRd32TwoColProjection');
    expect(src('pp-read-tail-r20-sources.ts')).toContain('PP_RD20_HR_ENGAGEMENTS_FROM_SQL');
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=1295', () => {
    const root = new URL('../../../packages/server-people_payroll/src/', import.meta.url);
    const files: URL[] = [];
    const walk = (dir: URL) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = new URL(entry.name + (entry.isDirectory() ? '/' : ''), dir);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) files.push(p);
      }
    };
    walk(root);
    const total = files.reduce((n, p) => n + (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length, 0);
    expect(total).toBeLessThan(1295);
    expect(total).toBe(1164);
  });
});
