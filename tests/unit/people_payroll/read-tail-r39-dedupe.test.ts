import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { ppRd39NumberedThreePred } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r39-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const readOnly = (literal: string) => {
  expect(literal).toContain('SELECT ');
  expect(literal).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
};
const uses = (name: string, symbol: string) => (src(name).match(new RegExp(`${symbol}\\(`, 'g')) ?? []).length;

describe('People Payroll read-tail r39 numbered-three-predicate dedupe', () => {
  const f: Array<[string, string[], string, string]> = [
    ['people-iam-purchases.ts', ['1', 'iam_receipts', 'id', 'company_id', 'actor_id'],
      'SELECT 1 FROM iam_receipts WHERE id=$1 AND company_id=$2 AND actor_id=$3',
      'ppRd39NumberedThreePred("1","iam_receipts","id","company_id","actor_id")'],
    ['people-iam-purchases.ts', ['*', 'people_package_revisions', 'company_id', 'package_id', 'revision'],
      'SELECT * FROM people_package_revisions WHERE company_id=$1 AND package_id=$2 AND revision=$3',
      'ppRd39NumberedThreePred("*","people_package_revisions","company_id","package_id","revision")'],
    ['people-lifecycle.ts', ['definition', 'people_package_revisions', 'company_id', 'package_id', 'revision'],
      'SELECT definition FROM people_package_revisions WHERE company_id=$1 AND package_id=$2 AND revision=$3',
      'ppRd39NumberedThreePred("definition","people_package_revisions","company_id","package_id","revision")'],
  ];

  test('the shared builder owns one byte-identical read-only literal per call', () => {
    for (const [, args, expected] of f) {
      expect(ppRd39NumberedThreePred(...(args as [string, string, string, string, string]))).toBe(expected);
      readOnly(expected);
    }
    expect((src('pp-read-tail-r39-sources.ts').match(/export const /g) ?? []).length).toBe(1);
  });

  test('each caller keeps no local copy and uses the shared builder', () => {
    const expectedCounts: Record<string, number> = {
      'people-iam-purchases.ts': 2,
      'people-lifecycle.ts': 1,
    };
    for (const [name, , , call] of f) expect(src(name), name).toContain(call);
    for (const [name, expectedCount] of Object.entries(expectedCounts)) {
      expect(uses(name, 'ppRd39NumberedThreePred'), name).toBe(expectedCount);
    }
    for (const [name, , expected] of f) expect(src(name), name).not.toContain(expected);
  });

  test('lock-bearing reads, native kinds and r1-r38 families stay untouched', () => {
    expect(src('pp-read-tail-r38-sources.ts')).toContain('export const ppRd38SinglePredTail');
    expect(src('pp-read-tail-r37-sources.ts')).toContain('export const ppRd37StatusGuard5');
    expect(src('pp-read-tail-r36-sources.ts')).toContain('export const ppRd36NumberedTwoPred');
    expect(src('people-lifecycle.ts')).toContain('SELECT * FROM people_access_packages WHERE company_id=$1 AND id=$2 FOR UPDATE');
    expect(src('people-lifecycle.ts')).toContain('nativePpQuery');
    expect(src('people-iam-purchases.ts')).toContain('ppRd36NumberedTwoPred');
  });

  test('worktree SELECT count is strictly lower than the recorded 1344 r38 baseline', () => {
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
    expect(total).toBeLessThan(1344);
    expect(total).toBe(1164);
  });
});
