import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { ppRd30Exists4Guard } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r30-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(symbol, 'g')) ?? []).length;

describe('People Payroll read-tail r30 parameterized family dedupe', () => {
  test('canonical builder owns the exact read-only EXISTS guard', () => {
    const guard = ppRd30Exists4Guard('hr_evaluation_definitions', 'id', 'company_id', 'kind', 'version');
    expect(guard).toBe('EXISTS(SELECT 1 FROM hr_evaluation_definitions WHERE id=? AND company_id=? AND kind=? AND version=?)');
    readOnly(guard);
  });

  test('each caller keeps no local copy and uses the shared builder', () => {
    const callers: Array<[string, string, string]> = [
      ['people-evaluation.ts', "EXISTS(SELECT 1 FROM hr_evaluation_definitions WHERE id=? AND company_id=? AND kind=? AND version=?)", 'hr_evaluation_definitions'],
      ['payroll-statutory.ts', "EXISTS(SELECT 1 FROM hr_thai_payroll_profiles WHERE id=? AND company_id=? AND status=? AND version=?)", 'hr_thai_payroll_profiles'],
      ['people-payroll.ts', "EXISTS(SELECT 1 FROM hr_country_packs WHERE id=? AND status=? AND record_version=? AND source_digest=?)", 'hr_country_packs'],
      ['people-payroll.ts', "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND pay_group_id=? AND currency=?)", 'hr_engagements'],
      ['people-lifecycle.ts', "EXISTS(SELECT 1 FROM people_position_packages WHERE company_id=? AND position_id=? AND package_id=? AND revision=?)", 'people_position_packages'],
    ];
    for (const [name, literal, table] of callers) {
      expect(src(name), name).not.toContain(literal);
    }
    expect(uses('people-evaluation.ts', 'ppRd30Exists4Guard\\(')).toBe(1);
    expect(uses('payroll-statutory.ts', 'ppRd30Exists4Guard\\(')).toBe(1);
    expect(uses('people-payroll.ts', 'ppRd30Exists4Guard\\(')).toBe(2);
    expect(uses('people-lifecycle.ts', 'ppRd30Exists4Guard\\(')).toBe(1);
    expect(src('people-evaluation.ts')).toContain("ppRd30Exists4Guard('hr_evaluation_definitions'");
    expect(src('payroll-statutory.ts')).toContain("ppRd30Exists4Guard('hr_thai_payroll_profiles'");
    expect(src('people-payroll.ts')).toContain("ppRd30Exists4Guard('hr_country_packs'");
    expect(src('people-payroll.ts')).toContain("ppRd30Exists4Guard('hr_engagements'");
    expect(src('people-lifecycle.ts')).toContain("ppRd30Exists4Guard('people_position_packages'");
  });

  test('the shared module declares the read fragment exactly once', () => {
    const s = src('pp-read-tail-r30-sources.ts');
    expect((s.match(/export const /g) ?? []).length).toBe(1);
    expect(s.includes('EXISTS(SELECT 1 FROM ${table} WHERE ${c1}=? AND ${c2}=? AND ${c3}=? AND ${c4}=?)')).toBe(true);
  });

  test('lock-bearing reads in touched owners remain untouched', () => {
    expect(src('people-evaluation.ts')).toContain('SELECT id FROM hr_engagements WHERE id=? FOR UPDATE');
    expect(src('people-evaluation.ts')).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
    expect(src('payroll-statutory.ts')).toContain('SELECT pg_advisory_xact_lock(hashtextextended(?,0))');
  });

  test('worktree SELECT count is strictly lower than the recorded 1416 r29 baseline', () => {
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
    expect(total).toBeLessThan(1416);
    expect(total).toBe(1195);
  });
});
