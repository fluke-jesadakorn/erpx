import { describe, expect, test } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { ppRdRound4ByProjectionIdCompany } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-round4-sources';
import { ppRd25ByIdCompany } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r25-sources';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('People Payroll round-8 r25 select-star alias fold', () => {
  test('the r25 alias renders byte-identically from the round-4 builder', () => {
    for (const table of ['hr_pay_groups', 'people_roster_requests', 'hr_evaluation_compensation_drafts', 'leave_types']) {
      const expected = `SELECT * FROM ${table} WHERE id=? AND company_id=?`;
      expect(ppRd25ByIdCompany(table)).toBe(expected);
      expect(ppRdRound4ByProjectionIdCompany('*', table)).toBe(expected);
      expect((expected.match(/\?/g) ?? []).length).toBe(2);
    }
  });

  test('the r25 module keeps the consumer-facing name but no local SELECT token', () => {
    const s = src('pp-read-tail-r25-sources.ts');
    expect(s).toContain('export const ppRd25ByIdCompany');
    expect((s.match(/SELECT /g) ?? []).length).toBe(0);
    expect(s).toContain("from './pp-read-tail-round4-sources'");
    expect(s).toContain("ppRdRound4ByProjectionIdCompany('*', table)");
    for (const name of ['people-evaluation.ts', 'people-roster.ts', 'payroll-statutory.ts']) {
      expect(src(name)).toContain('ppRd25ByIdCompany');
    }
  });

  test('the fold lowers the owner SELECT count below 1139', () => {
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
    expect(total).toBeLessThan(1139);
    expect(total).toBeLessThanOrEqual(1138);
  });
});
