import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  ppRd36SinglePred,
  ppRd36NumberedTwoPred,
  ppRd36ThreePred,
  ppRd36TwoPredTail,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r36-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol, 'g')) ?? []).length;

describe('People Payroll read-tail r36 four-family dedupe', () => {
  const f1: Array<[string, string[], string, string]> = [
    ['hire-handoff-pay-group.ts', ['record_id', 'command_receipts', 'command_id'], 'SELECT record_id FROM command_receipts WHERE command_id=?', 'ppRd36SinglePred("record_id", "command_receipts", "command_id")'],
    ['payroll-statutory.ts', ['mode,country_pack_id,currency', 'hr_payroll_runs', 'id'], 'SELECT mode,country_pack_id,currency FROM hr_payroll_runs WHERE id=?', 'ppRd36SinglePred("mode,country_pack_id,currency", "hr_payroll_runs", "id")'],
    ['people-roster.ts', ['1', 'pg_timezone_names', 'name'], 'SELECT 1 FROM pg_timezone_names WHERE name=?', 'ppRd36SinglePred("1", "pg_timezone_names", "name")'],
    ['people-lifecycle.ts', ['definition', 'people_lifecycle_settings', 'company_id'], 'SELECT definition FROM people_lifecycle_settings WHERE company_id=?', 'ppRd36SinglePred("definition", "people_lifecycle_settings", "company_id")'],
  ];
  const f2: Array<[string, string[], string, string]> = [
    ['technology-provisioning-projection.ts', ['user_id', 'hr_people', 'company_id', 'id'], 'SELECT user_id FROM hr_people WHERE company_id=$1 AND id=$2', 'ppRd36NumberedTwoPred("user_id", "hr_people", "company_id", "id")'],
    ['people-templates.ts', ['*', 'people_holding_templates', 'id', 'holding_id'], 'SELECT * FROM people_holding_templates WHERE id=$1 AND holding_id=$2', 'ppRd36NumberedTwoPred("*", "people_holding_templates", "id", "holding_id")'],
    ['technology-provisioning-facts.ts', ['package_id,revision', 'people_position_packages', 'company_id', 'position_id'], 'SELECT package_id,revision FROM people_position_packages WHERE company_id=$1 AND position_id=$2', 'ppRd36NumberedTwoPred("package_id,revision", "people_position_packages", "company_id", "position_id")'],
  ];
  const f3: Array<[string, string[], string, string]> = [
    ['leave-self-service.ts', ['leave_type_id,end_date,start_segment,end_segment', 'leave_requests', 'id', 'company_id', 'created_by'], 'SELECT leave_type_id,end_date,start_segment,end_segment FROM leave_requests WHERE id=? AND company_id=? AND created_by=?', 'ppRd36ThreePred("leave_type_id,end_date,start_segment,end_segment", "leave_requests", "id", "company_id", "created_by")'],
    ['people-evaluation-ai.ts', ['*', 'hr_evaluation_ai_jobs', 'id', 'company_id', 'created_by'], 'SELECT * FROM hr_evaluation_ai_jobs WHERE id=? AND company_id=? AND created_by=?', 'ppRd36ThreePred("*", "hr_evaluation_ai_jobs", "id", "company_id", "created_by")'],
    ['people-lifecycle.ts', ['definition', 'people_package_revisions', 'company_id', 'package_id', 'revision'], 'SELECT definition FROM people_package_revisions WHERE company_id=? AND package_id=? AND revision=?', 'ppRd36ThreePred("definition", "people_package_revisions", "company_id", "package_id", "revision")'],
  ];
  const f4: Array<[string, string[], string, string]> = [
    ['calendar-commands-facts.ts', ['id', 'hr_people', 'company_id', 'user_id', ' AND merged_into_id IS NULL'], 'SELECT id FROM hr_people WHERE company_id=? AND user_id=? AND merged_into_id IS NULL', 'ppRd36TwoPredTail("id", "hr_people", "company_id", "user_id", " AND merged_into_id IS NULL")'],
    ['retirement-clearance.ts', ['*', 'people_preparation_tasks', 'case_id', 'company_id', ' ORDER BY id'], 'SELECT * FROM people_preparation_tasks WHERE case_id=? AND company_id=? ORDER BY id', 'ppRd36TwoPredTail("*", "people_preparation_tasks", "case_id", "company_id", " ORDER BY id")'],
    ['payroll-adjustments.ts', ['id,source_digest,export_sha256', 'hr_issued_payslips', 'run_id', 'item_id', ' ORDER BY id'], 'SELECT id,source_digest,export_sha256 FROM hr_issued_payslips WHERE run_id=? AND item_id=? ORDER BY id', 'ppRd36TwoPredTail("id,source_digest,export_sha256", "hr_issued_payslips", "run_id", "item_id", " ORDER BY id")'],
    ['people-lifecycle.ts', ['definition', 'people_package_revisions', 'company_id', 'package_id', ' ORDER BY revision'], 'SELECT definition FROM people_package_revisions WHERE company_id=? AND package_id=? ORDER BY revision', 'ppRd36TwoPredTail("definition", "people_package_revisions", "company_id", "package_id", " ORDER BY revision")'],
  ];

  test('the four shared builders own one byte-identical read-only literal each', () => {
    const groups: Array<[string, Array<[string, string[], string, string]>, (...a: string[]) => string]> = [
      ['f1', f1, ppRd36SinglePred], ['f2', f2, ppRd36NumberedTwoPred],
      ['f3', f3, ppRd36ThreePred], ['f4', f4, ppRd36TwoPredTail],
    ];
    for (const [, rows, build] of groups) {
      for (const [, args, expected] of rows) {
        expect(build(...args)).toBe(expected);
        readOnly(build(...args));
      }
    }
    expect(ppRd36TwoPredTail('id', 't', 'a', 'b')).toBe('SELECT id FROM t WHERE a=? AND b=?');
    const helper = src('pp-read-tail-r36-sources.ts');
    expect((helper.match(/export const /g) ?? []).length).toBe(4);
  });

  test('each caller keeps no local copy and uses the shared builder', () => {
    const all: Array<[string, string, string]> = [];
    for (const [name, , , call] of f1) all.push([name, 'ppRd36SinglePred', call]);
    for (const [name, , , call] of f2) all.push([name, 'ppRd36NumberedTwoPred', call]);
    for (const [name, , , call] of f3) all.push([name, 'ppRd36ThreePred', call]);
    for (const [name, , , call] of f4) all.push([name, 'ppRd36TwoPredTail', call]);
    for (const [name, , call] of all) expect(src(name), name).toContain(call);
    for (const [name, symbol] of all) expect(uses(name, `${symbol}\\(`)).toBeGreaterThan(0);
    const expectedCounts: Record<string, number> = {
      'hire-handoff-pay-group.ts': 1, 'payroll-statutory.ts': 2, 'people-roster.ts': 4,
      'people-payroll.ts': 6, 'people-lifecycle.ts': 4, 'workflow-projection-source.ts': 1,
      'payroll-readiness.ts': 2, 'expense-ai-purchases.ts': 1,
      'work-report-candidates.ts': 1, 'technology-provisioning-projection.ts': 1,
      'people-templates.ts': 1, 'technology-provisioning-facts.ts': 1,
      'people-iam-purchases.ts': 5, 'leave-self-service.ts': 2,
      'people-evaluation-ai.ts': 1, 'people-evaluation-read.ts': 1,
      'project-worker-startup-state.ts': 1, 'expense-drafts.ts': 2,
      'calendar-commands-facts.ts': 1, 'retirement-clearance.ts': 1,
      'payroll-adjustments.ts': 1,
    };
    for (const [name, expectedCount] of Object.entries(expectedCounts)) {
      const total = ['ppRd36SinglePred', 'ppRd36NumberedTwoPred', 'ppRd36ThreePred', 'ppRd36TwoPredTail']
        .reduce((n, s) => n + uses(name, `${s}\\(`), 0);
      expect(total, name).toBe(expectedCount);
    }
  });

  test('lock-bearing reads and r1-r35 families stay untouched', () => {
    expect(src('pp-read-tail-r35-sources.ts')).toContain('export const ppRd35PayrollItemsByRun');
    expect(src('pp-read-tail-r34-sources.ts')).toContain('export const ppRd34ActivePayGroupProjection');
    expect(src('pp-read-tail-r25-sources.ts')).toContain('export const ppRd25ByIdCompany');
    expect(src('payroll-adjustments.ts')).toContain('ppRd35PayrollItemsByRun(');
    expect(src('payroll-adjustments.ts')).toContain('"SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE"');
    expect(src('payroll-adjustments.ts')).toContain('"SELECT id FROM hr_time_entries WHERE id=? FOR SHARE"');
    expect(src('payroll-statutory.ts')).toContain('"SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(src('people-roster.ts')).toContain('"SELECT id FROM hr_people WHERE id=? AND company_id=? FOR UPDATE"');
  });

  test('worktree SELECT count is strictly lower than the recorded 1387 r35 baseline', () => {
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
    expect(total).toBeLessThan(1387);
    expect(total).toBe(1202);
  });
});
