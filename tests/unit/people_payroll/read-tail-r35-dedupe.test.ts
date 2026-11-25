import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { ppRd35PayrollItemsByRun } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r35-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol, 'g')) ?? []).length;

describe('People Payroll read-tail r35 payroll-items-by-run family dedupe', () => {
  const callers: Array<[string, string, string, string]> = [
    ['payroll-deductions.ts', 'id,engagement_id,input_snapshot_json,result_json', ' ORDER BY id', "ppRd35PayrollItemsByRun('id,engagement_id,input_snapshot_json,result_json', ' ORDER BY id')"],
    ['people-payroll.ts', 'id,worker_type,gross_minor,deductions_minor,employer_cost_minor,net_minor,input_snapshot_json,result_json', ' ORDER BY id', "ppRd35PayrollItemsByRun('id,worker_type,gross_minor,deductions_minor,employer_cost_minor,net_minor,input_snapshot_json,result_json', ' ORDER BY id')"],
    ['payroll-adjustments.ts', 'input_snapshot_json', '', "ppRd35PayrollItemsByRun('input_snapshot_json')"],
    ['payroll-statutory.ts', 'gross_minor,deductions_minor,net_minor,result_json', '', "ppRd35PayrollItemsByRun('gross_minor,deductions_minor,net_minor,result_json')"],
  ];

  test('the shared builder owns one byte-identical read-only projection literal', () => {
    for (const [, cols, tail] of callers) {
      const literal = ppRd35PayrollItemsByRun(cols, tail);
      expect(literal).toBe(`SELECT ${cols} FROM hr_payroll_items WHERE run_id=?${tail}`);
      readOnly(literal);
    }
    expect(ppRd35PayrollItemsByRun('input_snapshot_json')).toBe('SELECT input_snapshot_json FROM hr_payroll_items WHERE run_id=?');
    expect(ppRd35PayrollItemsByRun('x', ' ORDER BY id')).toBe('SELECT x FROM hr_payroll_items WHERE run_id=? ORDER BY id');
    const s = src('pp-read-tail-r35-sources.ts');
    expect((s.match(/export const /g) ?? []).length).toBe(1);
    expect(s.includes('SELECT ${cols} FROM hr_payroll_items WHERE run_id=?${tail}')).toBe(true);
  });

  test('each of the 4 callers keeps no local copy and uses the shared builder', () => {
    for (const [name, cols, tail, call] of callers) {
      const literal = `SELECT ${cols} FROM hr_payroll_items WHERE run_id=?${tail}`;
      expect(src(name), name).not.toContain(`"${literal}"`);
      expect(src(name), name).not.toContain(`'${literal}'`);
      expect(src(name), name).toContain(call);
      expect(uses(name, 'ppRd35PayrollItemsByRun\\(')).toBe(1);
    }
  });

  test('lock-bearing reads and r1-r34 families stay untouched', () => {
    expect(src('pp-read-tail-r34-sources.ts')).toContain("export const ppRd34ActivePayGroupProjection = (cols: string) =>");
    expect(src('pp-read-tail-r33-sources.ts')).toContain('export const ppRd33FourColByIdCompany');
    expect(src('payroll-statutory.ts')).toContain('ppRd34ActivePayGroupProjection(');
    expect(src('payroll-statutory.ts')).toContain('"SELECT pg_advisory_xact_lock(hashtextextended(?,0))"');
    expect(src('payroll-adjustments.ts')).toContain('ppRd25ByIdCompany(');
    expect(src('payroll-adjustments.ts')).toContain('"SELECT run_id FROM hr_payroll_adjustments WHERE run_id=? FOR UPDATE"');
    expect(src('payroll-adjustments.ts')).toContain('"SELECT id FROM hr_time_entries WHERE id=? FOR SHARE"');
    expect(src('people-payroll.ts')).toContain('ppRd34ActivePayGroupProjection(');
    expect(src('people-payroll.ts')).toContain("NOT EXISTS(SELECT 1 FROM hr_payroll_items WHERE run_id=? AND status NOT IN ('calculated','approved','posted'))");
  });

  test('worktree SELECT count is strictly lower than the recorded 1390 r34 baseline', () => {
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
    expect(total).toBeLessThan(1390);
    expect(total).toBe(1197);
  });
});
