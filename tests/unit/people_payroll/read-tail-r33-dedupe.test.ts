import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { ppRd33FourColByIdCompany, ppRd33StarByTwoPredicates } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r33-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol, 'g')) ?? []).length;

describe('People Payroll read-tail r33 four-column and select-star family dedupe', () => {
  test('canonical builders own the exact read-only fragments', () => {
    const fourCol = ppRd33FourColByIdCompany('status,version,definition,review_snapshot', 'people_lifecycle_cases');
    expect(fourCol).toBe('SELECT status,version,definition,review_snapshot FROM people_lifecycle_cases WHERE id=? AND company_id=?');
    readOnly(fourCol);
    const star = ppRd33StarByTwoPredicates('overtime_payroll_handoffs', 'request_id', 'company_id');
    expect(star).toBe('SELECT * FROM overtime_payroll_handoffs WHERE request_id=? AND company_id=?');
    readOnly(star);
    const reversed = ppRd33StarByTwoPredicates('people_lifecycle_cases', 'company_id', 'id');
    expect(reversed).toBe('SELECT * FROM people_lifecycle_cases WHERE company_id=? AND id=?');
    readOnly(reversed);
  });

  test('each of the 8 callers keeps no local copy and uses a shared builder', () => {
    const callers: Array<[string, string, string]> = [
      ['project-worker-startup-jobs.ts', 'SELECT status,version,definition,review_snapshot FROM people_lifecycle_cases WHERE id=? AND company_id=?', "ppRd33FourColByIdCompany('status,version,definition,review_snapshot', 'people_lifecycle_cases')"],
      ['people-payroll.ts', 'SELECT source_type,source_id,status,version FROM hr_payment_batches WHERE id=? AND company_id=?', "ppRd33FourColByIdCompany('source_type,source_id,status,version', 'hr_payment_batches')"],
      ['payroll-readiness.ts', 'SELECT id,status,version,input_digest FROM hr_payroll_runs WHERE id=? AND company_id=?', "ppRd33FourColByIdCompany('id,status,version,input_digest', 'hr_payroll_runs')"],
      ['people-handoffs.ts', 'SELECT id,version,status,merged_into_id FROM hr_people WHERE id=? AND company_id=?', "ppRd33FourColByIdCompany('id,version,status,merged_into_id', 'hr_people')"],
      ['overtime-read.ts', 'SELECT * FROM overtime_payroll_handoffs WHERE request_id=? AND company_id=?', "ppRd33StarByTwoPredicates('overtime_payroll_handoffs', 'request_id', 'company_id')"],
      ['payroll-adjustments.ts', 'SELECT * FROM hr_payroll_runs WHERE id=? AND company_id=?', "ppRd25ByIdCompany('hr_payroll_runs')"],
      ['payroll-recoveries.ts', 'SELECT * FROM hr_payroll_recoveries WHERE id=? AND company_id=?', "ppRd25ByIdCompany('hr_payroll_recoveries')"],
      ['people-lifecycle.ts', 'SELECT * FROM people_lifecycle_cases WHERE company_id=? AND id=?', "ppRd33StarByTwoPredicates('people_lifecycle_cases', 'company_id', 'id')"],
    ];
    for (const [name, literal, call] of callers) {
      expect(src(name), name).not.toContain(`"${literal}"`);
      expect(src(name), name).not.toContain(`'${literal}'`);
      expect(src(name), name).toContain(call);
    }
    expect(uses('project-worker-startup-jobs.ts', 'ppRd33FourColByIdCompany\\(')).toBe(1);
    expect(uses('people-payroll.ts', 'ppRd33FourColByIdCompany\\(')).toBe(1);
    expect(uses('payroll-readiness.ts', 'ppRd33FourColByIdCompany\\(')).toBe(1);
    expect(uses('people-handoffs.ts', 'ppRd33FourColByIdCompany\\(')).toBe(1);
    expect(uses('overtime-read.ts', 'ppRd33StarByTwoPredicates\\(')).toBe(1);
    expect(uses('payroll-adjustments.ts', 'ppRd25ByIdCompany\\(')).toBe(1);
    expect(uses('payroll-recoveries.ts', 'ppRd25ByIdCompany\\(')).toBe(1);
    expect(uses('people-lifecycle.ts', 'ppRd33StarByTwoPredicates\\(')).toBe(1);
  });

  test('the shared module declares each read fragment exactly once', () => {
    const s = src('pp-read-tail-r33-sources.ts');
    expect((s.match(/export const /g) ?? []).length).toBe(2);
    expect(s.includes('SELECT ${cols} FROM ${table} WHERE id=? AND company_id=?')).toBe(true);
    expect(s.includes('SELECT * FROM ${table} WHERE ${c1}=? AND ${c2}=?')).toBe(true);
  });

  test('lock-bearing and advisory reads in touched files remain untouched', () => {
    expect(src('payroll-readiness.ts')).toContain('"SELECT to_jsonb(g)::text image FROM hr_pay_groups g WHERE g.id=? AND g.company_id=? AND g.status=\'active\' AND g.schedule_enabled AND g.schedule_version=? FOR SHARE"');
    expect(src('people-handoffs.ts')).toContain('"SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(src('payroll-adjustments.ts')).toContain('"SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE"');
    expect(src('payroll-adjustments.ts')).toContain('"SELECT id FROM hr_time_entries WHERE id=? FOR SHARE"');
    expect(src('payroll-recoveries.ts')).toContain("'SELECT id FROM hr_payroll_recoveries WHERE id=? FOR UPDATE'");
    expect(src('people-lifecycle.ts')).toContain('"SELECT * FROM people_lifecycle_cases WHERE company_id=$1 AND id=$2 FOR UPDATE"');
  });

  test('worktree SELECT count is strictly lower than the recorded 1399 r32 baseline', () => {
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
    expect(total).toBeLessThan(1399);
    expect(total).toBe(1175);
  });
});
