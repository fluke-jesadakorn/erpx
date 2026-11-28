import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { ppRdRound2ByIdCompanyConstant } from '../../../modules/people-payroll/server/src/adapters/pp-read-tail-round2-sources';

const src = (name: string) => readFileSync(
  new URL(`../../../modules/people-payroll/server/src/adapters/${name}`, import.meta.url), 'utf8',
);
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('People Payroll read-tail round-2 by-id/by-company/constant family', () => {
  test('one parameterized builder renders both originals byte-identically', () => {
    const leave = ppRdRound2ByIdCompanyConstant('id', 'leave_requests', 'state', 'pending_approval');
    const expense = ppRdRound2ByIdCompanyConstant('1', 'finance_expenses', 'workflow_version', '1', false);
    expect(leave).toBe("SELECT id FROM leave_requests WHERE id=? AND company_id=? AND state='pending_approval'");
    expect(expense).toBe('SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND workflow_version=1');
    expect((leave.match(/\?/g) ?? []).length).toBe(2);
    expect((expense.match(/\?/g) ?? []).length).toBe(2);
    readOnly(leave);
    readOnly(expense);
  });

  test('callers delegate and keep no inline copy', () => {
    expect(src('leave-read.ts')).toContain("ppRdRound2ByIdCompanyConstant('id', 'leave_requests', 'state', 'pending_approval')");
    expect(src('leave-read.ts')).not.toContain("SELECT id FROM leave_requests WHERE id=? AND company_id=? AND state='pending_approval'");
    expect(src('expense-read.ts')).toContain("ppRdRound2ByIdCompanyConstant('1', 'finance_expenses', 'workflow_version', '1', false)");
    expect(src('expense-read.ts')).not.toContain('SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND workflow_version=1');
  });

  test('the canonical module owns exactly one SELECT and no lock/native/cross-owner text', () => {
    const helper = src('pp-read-tail-round2-sources.ts');
    expect((helper.match(/export const /g) ?? []).length).toBe(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
    expect(helper).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativePpStatement/i);
    expect(helper).not.toContain('@erpx/server-');
  });

  test('the fold lowers the owner SELECT count below 1150', () => {
    const root = new URL('../../../modules/people-payroll/server/src/', import.meta.url);
    const files: URL[] = [];
    const walk = (dir: URL) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = new URL(entry.name + (entry.isDirectory() ? '/' : ''), dir);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) files.push(path);
      }
    };
    walk(root);
    const total = files.reduce((n, path) => n + (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length, 0);
    expect(total).toBeLessThan(1150);
    expect(total).toBeLessThanOrEqual(1149);
  });
});
