import { describe, expect, test } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';

const adapters = new URL('../../../modules/people-payroll/server/src/adapters/', import.meta.url);
const src = (name: string) => readFileSync(new URL(name, adapters), 'utf8');

/** Write-stage adapters for the employees/leave/expenses migration slice. */
const writerAdapters = [
  'leave.ts',
  'leave-self-service.ts',
  'expenses.ts',
  'expense-payments.ts',
  'expense-workflow.ts',
  'expense-payment-cancellation.ts',
  'expense-drafts.ts',
  'final-pay-cancellation.ts',
];

const dml = /(?:INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+[a-z_][a-z0-9_]*\s+SET)/i;

describe('People Payroll employees/leave/expense writers stay native-owned', () => {
  test('no TypeScript DML literal remains in the slice adapters', () => {
    for (const name of writerAdapters) {
      expect(src(name), name).not.toMatch(dml);
    }
  });

  test('write stages bind owner statement ids instead of SQL', () => {
    for (const name of ['leave.ts', 'leave-self-service.ts', 'expenses.ts', 'expense-payments.ts', 'expense-workflow.ts']) {
      expect(src(name).match(/nativePpStatement\(/g)?.length ?? 0, name).toBeGreaterThan(0);
    }
  });

  test('the owner has zero TypeScript DML literals across every .ts source', () => {
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
    for (const file of files) expect(readFileSync(file, 'utf8').match(dml) ?? [], file.pathname).toEqual([]);
  });

  test('lock-bearing reads in the slice remain at the caller', () => {
    expect(src('leave.ts')).toContain('SELECT id FROM leave_requests WHERE id=? AND company_id=? FOR UPDATE');
    expect(src('payroll-readiness.ts')).toContain('FOR UPDATE OF r SKIP LOCKED');
  });
});
