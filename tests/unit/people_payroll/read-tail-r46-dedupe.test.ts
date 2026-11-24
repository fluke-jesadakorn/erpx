import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  ppRd46ByIdCompanyStatusIn,
  ppRd46NamedJoinSource,
  ppRd46OwnerFileProbe,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r46-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol + '\\(', 'g')) ?? []).length;

const render = {
  ppRd46OwnerFileProbe: (a: string[]) => ppRd46OwnerFileProbe(a[0], a[1]),
  ppRd46ByIdCompanyStatusIn: (a: string[]) => ppRd46ByIdCompanyStatusIn(a[0], a[1], a[2]),
  ppRd46NamedJoinSource: (a: string[]) => ppRd46NamedJoinSource(a[0], a[1], a[2]),
};
type Sym = keyof typeof render;
type Row = { file: string; sym: Sym; args: string[]; literal: string; call: string };

const engagementCols = 'id,version,person_id,worker_type,employment_type,start_date,end_date,job_title,branch_id,department_id';
const runCols = '"id","company_id","pay_group_id","country_pack_id","period_start","period_end","pay_date","currency","mode","status","revision","totals_json","input_digest","error_message","version","created_by","approved_by","posted_by","created_at","updated_at","scope_engagement_id","submitted_by","submitted_digest","submitted_at","adjustment_of_run_id"';
const revisionCols = '"company_id", "package_id", "revision", "definition", "created_by", "created_at"';

// Each literal is copied verbatim from the pre-r46 inline head. Byte-identity
// oracles only; they are never live query sources. Bind arrays stay unchanged.
const oracles: Row[] = [
  // Family 1 — owner file probe (3 -> 1, -2; one bind at each call site unchanged)
  {
    file: 'beta-management-sources.ts',
    sym: 'ppRd46OwnerFileProbe',
    args: ['finance_expenses', 'e'],
    literal: 'EXISTS(SELECT 1 FROM finance_expenses e WHERE e.id=a.owner_id AND e.company_id=?)',
    call: `ppRd46OwnerFileProbe('finance_expenses','e')`,
  },
  {
    file: 'beta-management-sources.ts',
    sym: 'ppRd46OwnerFileProbe',
    args: ['expense_payment_drafts', 'e'],
    literal: 'EXISTS(SELECT 1 FROM expense_payment_drafts e WHERE e.id=a.owner_id AND e.company_id=?)',
    call: `ppRd46OwnerFileProbe('expense_payment_drafts','e')`,
  },
  {
    file: 'beta-management-sources.ts',
    sym: 'ppRd46OwnerFileProbe',
    args: ['leave_requests', 'l'],
    literal: 'EXISTS(SELECT 1 FROM leave_requests l WHERE l.id=a.owner_id AND l.company_id=?)',
    call: `ppRd46OwnerFileProbe('leave_requests','l')`,
  },
  // Family 2 — by-id/by-company status IN point read (2 -> 1, -1)
  {
    file: 'payroll-adjustments.ts',
    sym: 'ppRd46ByIdCompanyStatusIn',
    args: [engagementCols, 'hr_engagements', "'active','ended'"],
    literal: `SELECT ${engagementCols} FROM hr_engagements WHERE id=? AND company_id=? AND status IN ('active','ended')`,
    call: `ppRd46ByIdCompanyStatusIn('${engagementCols}','hr_engagements',"'active','ended'")`,
  },
  {
    file: 'payroll-payslips.ts',
    sym: 'ppRd46ByIdCompanyStatusIn',
    args: ['*', 'hr_payroll_runs', "'approved','posted'"],
    literal: `SELECT * FROM hr_payroll_runs WHERE id=? AND company_id=? AND status IN ('approved','posted')`,
    call: `ppRd46ByIdCompanyStatusIn('*','hr_payroll_runs',"'approved','posted'")`,
  },
  // Family 3 — named JOIN source fragment (2 -> 1, -1)
  {
    file: 'finance-owned-snapshot-sources.ts',
    sym: 'ppRd46NamedJoinSource',
    args: [runCols, 'hr_payroll_runs', 'r'],
    literal: `JOIN (SELECT ${runCols} FROM hr_payroll_runs) r`,
    call: `ppRd46NamedJoinSource('${runCols}','hr_payroll_runs','r')`,
  },
  {
    file: 'identity-snapshot-sources.ts',
    sym: 'ppRd46NamedJoinSource',
    args: [revisionCols, 'people_package_revisions', 'r'],
    literal: `JOIN (SELECT ${revisionCols} FROM people_package_revisions) r`,
    call: `ppRd46NamedJoinSource('${revisionCols}','people_package_revisions','r')`,
  },
];

describe('People Payroll read-tail r46 three-family dedupe', () => {
  test('the three shared builders own byte-identical, read-only literals', () => {
    expect(ppRd46OwnerFileProbe('finance_expenses', 'e')).toBe('EXISTS(SELECT 1 FROM finance_expenses e WHERE e.id=a.owner_id AND e.company_id=?)');
    expect((ppRd46OwnerFileProbe('finance_expenses', 'e').match(/\?/g) ?? []).length).toBe(1);
    expect(ppRd46ByIdCompanyStatusIn('a', 'hr_engagements', "'active','ended'")).toBe(
      `SELECT a FROM hr_engagements WHERE id=? AND company_id=? AND status IN ('active','ended')`,
    );
    expect(ppRd46NamedJoinSource('"a"', 'hr_payroll_runs', 'r')).toBe('JOIN (SELECT "a" FROM hr_payroll_runs) r');
    for (const o of oracles) {
      expect(render[o.sym](o.args), o.file + ' ' + o.sym).toBe(o.literal);
      readOnly(o.literal);
    }
    expect(oracles.length).toBe(7);
  });

  test('each caller uses its builder, keeps no inline head and keeps its binds', () => {
    expect((src('pp-read-tail-r46-sources.ts').match(/export const ppRd46/g) ?? []).length).toBe(3);
    const expectedCounts: Record<string, [Sym, number][]> = {
      'beta-management-sources.ts': [['ppRd46OwnerFileProbe', 3]],
      'payroll-adjustments.ts': [['ppRd46ByIdCompanyStatusIn', 1]],
      'payroll-payslips.ts': [['ppRd46ByIdCompanyStatusIn', 1]],
      'finance-owned-snapshot-sources.ts': [['ppRd46NamedJoinSource', 1]],
      'identity-snapshot-sources.ts': [['ppRd46NamedJoinSource', 1]],
    };
    for (const o of oracles) expect(src(o.file), o.file + ' builder call').toContain(o.call);
    for (const name of Object.keys(expectedCounts)) {
      expect(src(name), name + ' imports r46 builders').toContain("from './pp-read-tail-r46-sources'");
    }
    expect(src('beta-management-sources.ts')).not.toContain('EXISTS(SELECT 1 FROM finance_expenses');
    expect(src('beta-management-sources.ts')).not.toContain('EXISTS(SELECT 1 FROM expense_payment_drafts');
    expect(src('beta-management-sources.ts')).not.toContain('EXISTS(SELECT 1 FROM leave_requests');
    expect(src('payroll-adjustments.ts')).not.toContain(`SELECT ${engagementCols} FROM hr_engagements`);
    expect(src('payroll-payslips.ts')).not.toContain(`SELECT * FROM hr_payroll_runs WHERE id=? AND company_id=? AND status IN ('approved','posted')`);
    expect(src('finance-owned-snapshot-sources.ts')).not.toContain('JOIN (SELECT \\"id\\",\\"company_id\\",\\"pay_group_id\\"');
    expect(src('identity-snapshot-sources.ts')).not.toMatch(/"people_lifecycle_worker_effects_people_package_revisions_r_join": "JOIN \(SELECT/);
    expect(src('payroll-adjustments.ts')).toContain('.bind(engagement_id, company_id).first<Row>()');
    expect(src('payroll-payslips.ts')).toContain('.bind(runId, companyId).first<Row>()');
    for (const [name, pairs] of Object.entries(expectedCounts)) {
      for (const [sym, count] of pairs) expect(uses(name, sym), name + ' ' + sym).toBe(count);
    }
  });

  test('locks, advisory, native kinds, cross-owner projections and r1-r45 builders stay untouched', () => {
    expect(src('payroll-adjustments.ts')).toContain('SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE');
    expect(src('payroll-adjustments.ts')).toContain('SELECT id FROM hr_time_entries WHERE id=? FOR SHARE');
    expect(src('payroll-adjustments.ts')).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(src('payroll-adjustments.ts')).toContain("nativePpStatement('pp_payroll_adjustments_stmt_7')");
    expect(src('payroll-statutory.ts')).toContain('pg_advisory_xact_lock');
    expect(src('pp-read-tail-r51-sources.ts')).toContain('financeProjectionSources.finance_accounts');
    expect(src('beta-management-sources.ts')).toContain('ppRd42FourColOwnerProjection');
    expect(src('finance-owned-snapshot-sources.ts')).toContain('ppRd44NamedSourceProjection');
    expect(src('identity-snapshot-sources.ts')).toContain('ppRd44NamedSourceProjection');
    expect(src('pp-read-tail-r45-sources.ts')).toContain('export const ppRd45RunByIdProjection');
    expect(src('pp-read-tail-r44-sources.ts')).toContain('export const ppRd44NamedSourceProjection');
    expect(src('pp-read-tail-r37-sources.ts')).toContain('export const ppRd37IdVersionInGuard');
    expect(src('pp-read-tail-r27-sources.ts')).toContain('export const ppRd27IdVersionStatusInGuard');
    for (const name of ['beta-management-sources.ts', 'payroll-adjustments.ts', 'payroll-payslips.ts', 'finance-owned-snapshot-sources.ts', 'identity-snapshot-sources.ts']) {
      expect(src(name), name + ' has no foreign r46 symbol').not.toMatch(/ppRd46(?!OwnerFileProbe|ByIdCompanyStatusIn|NamedJoinSource)/);
    }
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=1265', () => {
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
    expect(total).toBeLessThan(1265);
    expect(total).toBe(1221);
  });
});
