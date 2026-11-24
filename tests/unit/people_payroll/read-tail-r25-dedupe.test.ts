import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { ppRd25ByIdCompany } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r25-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const quoted = (literal: string) => JSON.stringify(literal);

const table = 'hr_payroll_runs';
const literal = ppRd25ByIdCompany(table);

describe('People Payroll read-tail r25 parameterized family dedupe', () => {
  test('canonical builder owns the exact read-only fragment', () => {
    expect(literal).toBe('SELECT * FROM hr_payroll_runs WHERE id=? AND company_id=?');
    expect(literal).toContain('SELECT ');
    expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('each caller keeps no local copy and uses one shared symbol', () => {
    const callers: [string, number, string][] = [
      ['expense-payment-read.ts', 1, 'expense_payment_drafts'],
      ['leave.ts', 1, 'leave_requests'],
      ['operations-staff-settings-facts.ts', 1, 'hr_engagements'],
      ['overtime.ts', 1, 'overtime_requests'],
      ['payroll-statutory.ts', 1, 'hr_thai_payroll_profiles'],
      ['people-evaluation-read.ts', 1, 'hr_evaluation_assessments'],
      ['people-evaluation.ts', 2, 'hr_evaluation_evidence'],
      ['people-lifecycle.ts', 1, 'people_lifecycle_cases'],
      ['people-roster.ts', 2, 'people_roster_weeks'],
      ['expenses.ts', 1, 'finance_expenses'],
    ];
    for (const [name, count, calledTable] of callers) {
      const s = src(name);
      expect(s, name).not.toContain(quoted(ppRd25ByIdCompany(calledTable)));
      expect(s.match(/ppRd25ByIdCompany/g) ?? [], name).toHaveLength(count + 1);
    }
    expect(src('people-evaluation.ts')).toContain("ppRd25ByIdCompany('hr_evaluation_compensation_drafts')");
    expect(src('people-roster.ts')).toContain("ppRd25ByIdCompany('people_roster_requests')");
  });

  test('the shared module declares the fragment exactly once', () => {
    const s = src('pp-read-tail-r25-sources.ts');
    expect(s.split('`SELECT * FROM ${table} WHERE id=? AND company_id=?`')).toHaveLength(2);
  });

  test('worktree SELECT count is strictly lower than the recorded 1459 r24 baseline', () => {
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
    expect(total).toBeLessThan(1459);
    expect(total).toBe(1351);
  });
});
