import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { ppRd34ActivePayGroupProjection } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r34-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol, 'g')) ?? []).length;

describe('People Payroll read-tail r34 active pay group projection family dedupe', () => {
  const callers: Array<[string, string, string]> = [
    ['payroll-statutory.ts', 'country_code,currency,frequency', 'ppRd34ActivePayGroupProjection(\'country_code,currency,frequency\')'],
    ['people-payroll.ts', 'id,currency', 'ppRd34ActivePayGroupProjection(\'id,currency\')'],
    ['payroll-readiness.ts', 'overtime_multiplier_bps,proration_method', 'ppRd34ActivePayGroupProjection(\'overtime_multiplier_bps,proration_method\')'],
    ['people-handoffs.ts', 'id,currency,frequency', 'ppRd34ActivePayGroupProjection(\'id,currency,frequency\')'],
  ];

  test('the shared builder owns one byte-identical read-only projection literal', () => {
    for (const [, cols] of callers) {
      const literal = ppRd34ActivePayGroupProjection(cols);
      expect(literal).toBe(`SELECT ${cols} FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active'`);
      readOnly(literal);
    }
    const s = src('pp-read-tail-r34-sources.ts');
    expect((s.match(/export const /g) ?? []).length).toBe(1);
    expect(s.includes("SELECT ${cols} FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active'")).toBe(true);
  });

  test('each of the 4 callers keeps no local copy and uses the shared builder', () => {
    for (const [name, cols, call] of callers) {
      const literal = `SELECT ${cols} FROM hr_pay_groups WHERE id=? AND company_id=? AND status='active'`;
      expect(src(name), name).not.toContain(`"${literal}"`);
      expect(src(name), name).not.toContain(`'${literal}'`);
      expect(src(name), name).toContain(call);
      expect(uses(name, 'ppRd34ActivePayGroupProjection\\(')).toBe(1);
    }
  });

  test('lock-bearing reads, the r23 guards and the payroll-readiness EXISTS guard stay untouched', () => {
    expect(src('pp-read-tail-r23-sources.ts')).toContain("export const PP_RD23_HR_PAY_GROUPS_ACTIVE_CURRENCY_GUARD = \"EXISTS(SELECT 1 FROM hr_pay_groups WHERE id=? AND company_id=? AND currency=? AND status='active')\";");
    expect(src('payroll-readiness.ts')).toContain('ppRd37StatusGuard5("hr_pay_groups", "company_id", "active", "overtime_multiplier_bps", "proration_method")');
    expect(src('pp-read-tail-r37-sources.ts')).toContain("EXISTS(SELECT 1 FROM ${table} WHERE id=? AND ${c2}=? AND status='${status}' AND ${c4}=? AND ${c5}=?)");
    expect(src('people-handoffs.ts')).toContain('PP_RD23_HR_PAY_GROUPS_ACTIVE_CURRENCY_GUARD');
    expect(src('people-payroll.ts')).toContain('PP_RD23_HR_PAY_GROUPS_ACTIVE_CURRENCY_GUARD');
    expect(src('payroll-statutory.ts')).toContain('"SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(src('people-handoffs.ts')).toContain('"SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
  });

  test('worktree SELECT count is strictly lower than the recorded 1393 r33 baseline', () => {
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
    expect(total).toBeLessThan(1393);
    expect(total).toBe(1228);
  });
});
