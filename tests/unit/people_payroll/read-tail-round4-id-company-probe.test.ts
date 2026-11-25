import { describe, expect, test } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { ppRdRound4ByProjectionIdCompany } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-round4-sources';
import { PP_RD23_LEAVE_TYPES_ID_COMPANY_EXISTS, PP_RD23_HR_ENGAGEMENTS_ID_COMPANY_EXISTS } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r23-sources';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('People Payroll round-4 by-id/by-company probe family', () => {
  test('one canonical builder renders all three former literals byte-identically', () => {
    expect(ppRdRound4ByProjectionIdCompany('1', 'leave_types')).toBe('SELECT 1 FROM leave_types WHERE id=? AND company_id=?');
    expect(ppRdRound4ByProjectionIdCompany('1', 'hr_engagements')).toBe('SELECT 1 FROM hr_engagements WHERE id=? AND company_id=?');
    expect(ppRdRound4ByProjectionIdCompany('timezone', 'hr_pay_groups')).toBe('SELECT timezone FROM hr_pay_groups WHERE id=? AND company_id=?');
    expect(PP_RD23_LEAVE_TYPES_ID_COMPANY_EXISTS).toBe('SELECT 1 FROM leave_types WHERE id=? AND company_id=?');
    expect(PP_RD23_HR_ENGAGEMENTS_ID_COMPANY_EXISTS).toBe('SELECT 1 FROM hr_engagements WHERE id=? AND company_id=?');
    for (const sql of [
      PP_RD23_LEAVE_TYPES_ID_COMPANY_EXISTS,
      PP_RD23_HR_ENGAGEMENTS_ID_COMPANY_EXISTS,
      ppRdRound4ByProjectionIdCompany('timezone', 'hr_pay_groups'),
    ]) {
      expect((sql.match(/\?/g) ?? []).length).toBe(2);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('all three callers delegate and keep no inline copy', () => {
    expect(src('pp-read-tail-r23-sources.ts')).toContain("from './pp-read-tail-round4-sources'");
    expect(src('pp-read-tail-r23-sources.ts')).toContain("ppRdRound4ByProjectionIdCompany('1', 'leave_types')");
    expect(src('pp-read-tail-r23-sources.ts')).toContain("ppRdRound4ByProjectionIdCompany('1', 'hr_engagements')");
    expect(src('payroll-readiness.ts')).toContain("from './pp-read-tail-round4-sources'");
    expect(src('payroll-readiness.ts')).toContain("ppRdRound4ByProjectionIdCompany('timezone', 'hr_pay_groups')");
    expect(src('payroll-readiness.ts')).not.toContain('SELECT timezone FROM hr_pay_groups WHERE id=? AND company_id=?');
    expect(src('leave-self-service.ts')).toContain('PP_RD23_LEAVE_TYPES_ID_COMPANY_EXISTS');
    expect(src('leave-self-service.ts')).toContain('PP_RD23_HR_ENGAGEMENTS_ID_COMPANY_EXISTS');
    const canonical = src('pp-read-tail-round4-sources.ts');
    expect((canonical.match(/export const /g) ?? []).length).toBe(1);
    expect((canonical.match(/SELECT /g) ?? []).length).toBe(1);
    expect(canonical).not.toContain('@erpx/server-');
  });

  test('the fold lowers the owner SELECT count below 1144', () => {
    const root = new URL('../../../packages/server-people_payroll/src/', import.meta.url);
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
    expect(total).toBeLessThan(1144);
    expect(total).toBeLessThanOrEqual(1142);
  });
});
