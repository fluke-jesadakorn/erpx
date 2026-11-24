import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { ppRd40UnlinkedUserByCompany } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r40-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(`${symbol}\\(`, 'g')) ?? []).length;

describe('People Payroll read-tail r40 unlinked-user-by-company dedupe', () => {
  const f: Array<[string, string[], string, string]> = [
    ['calendar-commands-facts.ts', ['id'],
      'SELECT id FROM hr_people WHERE company_id=? AND user_id=? AND merged_into_id IS NULL LIMIT 1',
      "ppRd40UnlinkedUserByCompany('id')"],
    ['work-review-facts.ts', ['legal_name'],
      'SELECT legal_name FROM hr_people WHERE company_id=? AND user_id=? AND merged_into_id IS NULL LIMIT 1',
      "ppRd40UnlinkedUserByCompany('legal_name')"],
  ];

  test('the shared builder owns one byte-identical read-only literal per call', () => {
    for (const [, args, expected] of f) {
      expect(ppRd40UnlinkedUserByCompany(...(args as [string]))).toBe(expected);
      readOnly(expected);
    }
    expect((src('pp-read-tail-r40-sources.ts').match(/export const /g) ?? []).length).toBe(1);
  });

  test('each caller keeps no local copy and uses the shared builder', () => {
    const expectedCounts: Record<string, number> = {
      'calendar-commands-facts.ts': 1,
      'work-review-facts.ts': 1,
    };
    for (const [name, , , call] of f) expect(src(name), name).toContain(call);
    for (const [name, expectedCount] of Object.entries(expectedCounts)) {
      expect(uses(name, 'ppRd40UnlinkedUserByCompany'), name).toBe(expectedCount);
    }
    for (const [name, , expected] of f) expect(src(name), name).not.toContain(expected);
  });

  test('lock-bearing reads, native kinds, cross-owner projections and r1-r39 builders stay untouched', () => {
    expect(src('pp-read-tail-r39-sources.ts')).toContain('export const ppRd39NumberedThreePred');
    expect(src('pp-read-tail-r38-sources.ts')).toContain('export const ppRd38SinglePredTail');
    expect(src('pp-read-tail-r36-sources.ts')).toContain('export const ppRd36NumberedTwoPred');
    expect(src('pp-read-tail-r32-sources.ts')).toContain('export const ppRd32TwoColProjection');
    expect(src('people-lifecycle.ts')).toContain("SELECT * FROM people_access_packages WHERE company_id=$1 AND id=$2 FOR UPDATE");
    expect(src('people-lifecycle.ts')).toContain('nativePpQuery');
  });

  test('worktree SELECT count is strictly lower than the recorded 1342 r39 baseline', () => {
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
    expect(total).toBeLessThan(1342);
    expect(total).toBe(1233);
  });
});
