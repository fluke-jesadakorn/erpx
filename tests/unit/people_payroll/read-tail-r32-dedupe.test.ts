import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { ppRd32TwoColProjection } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r32-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol, 'g')) ?? []).length;

describe('People Payroll read-tail r32 two-column projection family dedupe', () => {
  test('canonical builder owns the exact read-only two-predicate projection', () => {
    const normal = ppRd32TwoColProjection('pay_group_id,period_end', 'hr_payroll_runs', 'id', 'company_id');
    expect(normal).toBe('SELECT pay_group_id,period_end FROM hr_payroll_runs WHERE id=? AND company_id=?');
    readOnly(normal);
    const reversed = ppRd32TwoColProjection('status,workflow_stage', 'finance_expenses', 'company_id', 'id');
    expect(reversed).toBe('SELECT status,workflow_stage FROM finance_expenses WHERE company_id=? AND id=?');
    readOnly(reversed);
  });

  test('each of the 11 callers keeps no local copy and uses the shared builder', () => {
    const callers: Array<[string, string, string]> = [
      ['people-evaluation.ts', 'SELECT id,observed_on FROM hr_evaluation_evidence WHERE assessment_id=? AND company_id=?', "ppRd32TwoColProjection('id,observed_on', 'hr_evaluation_evidence', 'assessment_id', 'company_id')"],
      ['people-evaluation.ts', 'SELECT pay_group_id,status FROM hr_engagements WHERE id=? AND company_id=?', "ppRd32TwoColProjection('pay_group_id,status', 'hr_engagements', 'id', 'company_id')"],
      ['workflow-approvals-facts.ts', 'SELECT created_by,prepared_by FROM finance_expenses WHERE id=? AND company_id=?', "ppRd32TwoColProjection('created_by,prepared_by', 'finance_expenses', 'id', 'company_id')"],
      ['workflow-approvals-facts.ts', 'SELECT claimant_user_id,prepared_by FROM finance_expenses WHERE id=? AND company_id=?', "ppRd32TwoColProjection('claimant_user_id,prepared_by', 'finance_expenses', 'id', 'company_id')"],
      ['payroll-purchases.ts', 'SELECT source_type,source_id FROM hr_payment_batches WHERE id=? AND company_id=?', "ppRd32TwoColProjection('source_type,source_id', 'hr_payment_batches', 'id', 'company_id')"],
      ['payroll-payslips.ts', 'SELECT export_html,export_sha256 FROM hr_issued_payslips WHERE id=? AND company_id=?', "ppRd32TwoColProjection('export_html,export_sha256', 'hr_issued_payslips', 'id', 'company_id')"],
      ['calendar-commands-facts.ts', 'SELECT assignee_id,responsibility FROM people_preparation_tasks WHERE id=? AND company_id=?', "ppRd32TwoColProjection('assignee_id,responsibility', 'people_preparation_tasks', 'id', 'company_id')"],
      ['people-payroll.ts', 'SELECT pay_group_id,period_end FROM hr_payroll_runs WHERE id=? AND company_id=?', "ppRd32TwoColProjection('pay_group_id,period_end', 'hr_payroll_runs', 'id', 'company_id')"],
      ['expense-purchases.ts', 'SELECT status,workflow_stage FROM finance_expenses WHERE company_id=? AND id=?', "ppRd32TwoColProjection('status,workflow_stage', 'finance_expenses', 'company_id', 'id')"],
      ['people-lifecycle.ts', 'SELECT version,definition FROM people_access_packages WHERE company_id=? AND id=?', "ppRd32TwoColProjection('version,definition', 'people_access_packages', 'company_id', 'id')"],
      ['people-lifecycle.ts', 'SELECT package_id,revision FROM people_position_packages WHERE company_id=? AND position_id=?', "ppRd32TwoColProjection('package_id,revision', 'people_position_packages', 'company_id', 'position_id')"],
    ];
    for (const [name, literal, call] of callers) {
      expect(src(name), name).not.toContain(`"${literal}"`);
      expect(src(name), name).not.toContain(`'${literal}'`);
      expect(src(name), name).toContain(call);
    }
    expect(uses('people-evaluation.ts', 'ppRd32TwoColProjection\\(')).toBe(2);
    expect(uses('workflow-approvals-facts.ts', 'ppRd32TwoColProjection\\(')).toBe(2);
    expect(uses('payroll-purchases.ts', 'ppRd32TwoColProjection\\(')).toBe(2);
    expect(uses('payroll-payslips.ts', 'ppRd32TwoColProjection\\(')).toBe(1);
    expect(uses('calendar-commands-facts.ts', 'ppRd32TwoColProjection\\(')).toBe(1);
    expect(uses('people-payroll.ts', 'ppRd32TwoColProjection\\(')).toBe(10);
    expect(uses('expense-purchases.ts', 'ppRd32TwoColProjection\\(')).toBe(1);
    expect(uses('people-lifecycle.ts', 'ppRd32TwoColProjection\\(')).toBe(2);
  });

  test('the shared module declares the read fragment exactly once', () => {
    const s = src('pp-read-tail-r32-sources.ts');
    expect((s.match(/export const /g) ?? []).length).toBe(1);
    expect(s.includes('SELECT ${cols} FROM ${table} WHERE ${c1}=? AND ${c2}=?')).toBe(true);
  });

  test('lock-bearing and cross-owner reads in touched files remain untouched', () => {
    expect(src('payroll-purchases.ts')).toContain('"SELECT source_type,source_id FROM hr_payment_batches WHERE id=? AND company_id=? FOR SHARE"');
    expect(src('people-lifecycle.ts')).toContain('"SELECT * FROM people_access_packages WHERE company_id=$1 AND id=$2 FOR UPDATE"');
    expect(src('people-evaluation.ts')).toContain('"SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(src('people-evaluation.ts')).toContain('"SELECT id FROM hr_engagements WHERE id=? FOR UPDATE"');
  });

  test('worktree SELECT count is strictly lower than the recorded 1409 r31 baseline', () => {
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
    expect(total).toBeLessThan(1409);
    expect(total).toBe(1265);
  });
});
