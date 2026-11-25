import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { ppRd38SinglePredTail } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r38-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(`${symbol}\\(`, 'g')) ?? []).length;

describe('People Payroll read-tail r38 single-predicate-tail dedupe', () => {
  const f: Array<[string, string[], string, string]> = [
    ['people-evaluation.ts', ['id,kind,version,payload_json,updated_at', 'hr_evaluation_definitions', 'company_id', ' ORDER BY kind,id'], 'SELECT id,kind,version,payload_json,updated_at FROM hr_evaluation_definitions WHERE company_id=? ORDER BY kind,id', 'ppRd38SinglePredTail("id,kind,version,payload_json,updated_at", "hr_evaluation_definitions", "company_id", " ORDER BY kind,id")'],
    ['leave-self-service.ts', ['id,name,version,effective_from,effective_to', 'leave_policy_versions', 'company_id', ' ORDER BY name,version DESC'], 'SELECT id,name,version,effective_from,effective_to FROM leave_policy_versions WHERE company_id=? ORDER BY name,version DESC', "ppRd38SinglePredTail('id,name,version,effective_from,effective_to', 'leave_policy_versions', 'company_id', ' ORDER BY name,version DESC')"],
    ['flow-settings-facts.ts', ["name title,version,created_by actor,created_at at,'Leave policy' kind", 'leave_policy_versions', 'company_id', ' ORDER BY created_at DESC'], "SELECT name title,version,created_by actor,created_at at,'Leave policy' kind FROM leave_policy_versions WHERE company_id=? ORDER BY created_at DESC", 'ppRd38SinglePredTail("name title,version,created_by actor,created_at at,\'Leave policy\' kind", "leave_policy_versions", "company_id", " ORDER BY created_at DESC")'],
    ['people-payroll.ts', ['*', 'hr_compensation_revisions', 'engagement_id', ' ORDER BY effective_from DESC,created_at DESC'], 'SELECT * FROM hr_compensation_revisions WHERE engagement_id=? ORDER BY effective_from DESC,created_at DESC', "ppRd38SinglePredTail('*', 'hr_compensation_revisions', 'engagement_id', ' ORDER BY effective_from DESC,created_at DESC')"],
    ['people-lifecycle.ts', ['*', 'people_access_packages', 'company_id', " ORDER BY definition->>'name',id"], "SELECT * FROM people_access_packages WHERE company_id=? ORDER BY definition->>'name',id", "ppRd38SinglePredTail('*', 'people_access_packages', 'company_id', \" ORDER BY definition->>'name',id\")"],
    ['people-lifecycle.ts', ['*', 'people_preparation_tasks', 'company_id', ' ORDER BY updated_at DESC,id'], 'SELECT * FROM people_preparation_tasks WHERE company_id=? ORDER BY updated_at DESC,id', "ppRd38SinglePredTail('*', 'people_preparation_tasks', 'company_id', ' ORDER BY updated_at DESC,id')"],
    ['payroll-adjustments.ts', ['revision,taxable_arrears_minor,non_taxable_arrears_minor,time_entries_json,reason,created_at', 'hr_payroll_adjustment_revisions', 'run_id', ' ORDER BY revision'], 'SELECT revision,taxable_arrears_minor,non_taxable_arrears_minor,time_entries_json,reason,created_at FROM hr_payroll_adjustment_revisions WHERE run_id=? ORDER BY revision', "ppRd38SinglePredTail('revision,taxable_arrears_minor,non_taxable_arrears_minor,time_entries_json,reason,created_at', 'hr_payroll_adjustment_revisions', 'run_id', ' ORDER BY revision')"],
    ['organization-draft-facts.ts', ['engagement_id,id,policy_version_id,effective_from,effective_to,version', 'leave_policy_assignments', 'company_id', ' ORDER BY engagement_id,id'], 'SELECT engagement_id,id,policy_version_id,effective_from,effective_to,version FROM leave_policy_assignments WHERE company_id=? ORDER BY engagement_id,id', "ppRd38SinglePredTail('engagement_id,id,policy_version_id,effective_from,effective_to,version', 'leave_policy_assignments', 'company_id', ' ORDER BY engagement_id,id')"],
  ];

  test('the shared builder owns one byte-identical read-only literal per call', () => {
    for (const [, args, expected] of f) {
      expect(ppRd38SinglePredTail(...(args as [string, string, string, string]))).toBe(expected);
      readOnly(expected);
    }
    expect((src('pp-read-tail-r38-sources.ts').match(/export const /g) ?? []).length).toBe(1);
  });

  test('each caller keeps no local copy and uses the shared builder', () => {
    const expectedCounts: Record<string, number> = {
      'people-evaluation.ts': 1, 'leave-self-service.ts': 1, 'flow-settings-facts.ts': 1,
      'people-payroll.ts': 1, 'people-lifecycle.ts': 2, 'payroll-adjustments.ts': 1,
      'organization-draft-facts.ts': 1,
    };
    for (const [name, , , call] of f) expect(src(name), name).toContain(call);
    for (const [name, expectedCount] of Object.entries(expectedCounts)) {
      expect(uses(name, 'ppRd38SinglePredTail'), name).toBe(expectedCount);
    }
    for (const [name, , expected] of f) expect(src(name), name).not.toContain(expected);
  });

  test('lock-bearing reads, native kinds and r1-r37 families stay untouched', () => {
    expect(src('pp-read-tail-r37-sources.ts')).toContain('export const ppRd37StatusGuard5');
    expect(src('pp-read-tail-r36-sources.ts')).toContain('export const ppRd36TwoPredTail');
    expect(src('people-evaluation.ts')).toContain('SELECT id FROM hr_engagements WHERE id=? FOR UPDATE');
    expect(src('people-evaluation.ts')).toContain('SELECT id FROM hr_evaluation_definitions WHERE id=? FOR SHARE');
    expect(src('payroll-recoveries.ts')).toContain('pg_advisory_xact_lock');
    expect(src('payroll-adjustments.ts')).toContain('"SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE"');
    expect(src('people-lifecycle.ts')).toContain('nativePpStatement');
  });

  test('worktree SELECT count is strictly lower than the recorded 1351 r37 baseline', () => {
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
    expect(total).toBeLessThan(1351);
    expect(total).toBe(1175);
  });
});
