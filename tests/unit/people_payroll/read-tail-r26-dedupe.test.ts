import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  ppRd26IdCompanyVersionStatusGuard,
  ppRd26AttachmentTargetScopeSql,
  ppRd26CommandReceiptPriorSql,
  ppRd26RunIdGuard,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r26-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const quoted = (literal: string) => JSON.stringify(literal);
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};

describe('People Payroll read-tail r26 parameterized family dedupe', () => {
  test('canonical builders own the exact read-only fragments', () => {
    const versionGuard = ppRd26IdCompanyVersionStatusGuard('hr_engagements');
    expect(versionGuard).toBe('EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=? AND status=?)');
    const attachmentScope = ppRd26AttachmentTargetScopeSql('hr_payroll_runs');
    expect(attachmentScope).toBe('SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM hr_payroll_runs WHERE id=? AND company_id=?');
    const receipt = ppRd26CommandReceiptPriorSql('command_receipts');
    expect(receipt).toBe('SELECT 1 FROM command_receipts WHERE command_id=?');
    const runGuard = ppRd26RunIdGuard('hr_issued_payslips');
    expect(runGuard).toBe('NOT EXISTS(SELECT 1 FROM hr_issued_payslips WHERE run_id=?)');
    for (const literal of [versionGuard, attachmentScope, receipt, runGuard]) readOnly(literal);
  });

  test('each caller keeps no local copy and uses the shared symbol', () => {
    const versionCallers: [string, number, string][] = [
      ['payroll-recoveries.ts', 1, 'hr_payroll_recoveries'],
      ['people-payroll.ts', 2, 'hr_engagements'],
    ];
    for (const [name, uses, table] of versionCallers) {
      const s = src(name);
      expect(s, name).not.toContain(quoted(ppRd26IdCompanyVersionStatusGuard(table)));
      expect(s.match(/ppRd26IdCompanyVersionStatusGuard/g) ?? [], name).toHaveLength(uses + 1);
    }
    expect(src('people-payroll.ts')).toContain("ppRd26IdCompanyVersionStatusGuard('hr_payment_batches')");

    for (const [name, table] of [['attachments-finance-facts.ts', 'people_provision_operations'], ['attachments-sensitive-facts.ts', 'hr_payroll_runs']] as const) {
      const s = src(name);
      expect(s, name).not.toContain(quoted(ppRd26AttachmentTargetScopeSql(table)));
      expect(s.match(/ppRd26AttachmentTargetScopeSql/g) ?? [], name).toHaveLength(2);
    }

    for (const [name, table] of [['final-pay-cancellation.ts', 'command_receipts'], ['people-payroll.ts', 'hr_command_receipts']] as const) {
      const s = src(name);
      expect(s, name).not.toContain(quoted(ppRd26CommandReceiptPriorSql(table)));
      expect(s.match(/ppRd26CommandReceiptPriorSql/g) ?? [], name).toHaveLength(2);
    }

    for (const [name, table] of [['payroll-payslips.ts', 'hr_issued_payslips'], ['people-payroll.ts', 'hr_payroll_postings']] as const) {
      const s = src(name);
      expect(s, name).not.toContain(quoted(ppRd26RunIdGuard(table)));
      expect(s.match(/ppRd26RunIdGuard/g) ?? [], name).toHaveLength(2);
    }
  });

  test('the shared module declares each fragment exactly once', () => {
    const s = src('pp-read-tail-r26-sources.ts');
    for (const fragment of [
      'EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=? AND status=?)',
      'SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM ${table} WHERE id=? AND company_id=?',
      'SELECT 1 FROM ${table} WHERE command_id=?',
      'NOT EXISTS(SELECT 1 FROM ${table} WHERE run_id=?)',
    ]) {
      expect(s.split(fragment)).toHaveLength(2);
    }
  });

  test('worktree SELECT count is strictly lower than the recorded 1448 r25 baseline', () => {
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
    expect(total).toBeLessThan(1448);
    expect(total).toBe(1167);
  });
});
