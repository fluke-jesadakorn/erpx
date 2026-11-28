import { describe, expect, test } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { ppRdRound4ByProjectionIdCompany } from '../../../modules/people-payroll/server/src/adapters/pp-read-tail-round4-sources';
import { ppRd33FourColByIdCompany, ppRd33StarByTwoPredicates } from '../../../modules/people-payroll/server/src/adapters/pp-read-tail-r33-sources';

const src = (name: string) => readFileSync(
  new URL(`../../../modules/people-payroll/server/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('People Payroll round-7 r33 id/company alias fold', () => {
  test('the r33 alias renders byte-identically from the round-4 builder', () => {
    for (const [cols, table] of [
      ['status,version,definition,review_snapshot', 'people_lifecycle_cases'],
      ['source_type,source_id,status,version', 'hr_payment_batches'],
      ['id,status,version,input_digest', 'hr_payroll_runs'],
      ['id,version,status,merged_into_id', 'hr_people'],
    ]) {
      const expected = `SELECT ${cols} FROM ${table} WHERE id=? AND company_id=?`;
      expect(ppRd33FourColByIdCompany(cols, table)).toBe(expected);
      expect(ppRdRound4ByProjectionIdCompany(cols, table)).toBe(expected);
      expect((expected.match(/\?/g) ?? []).length).toBe(2);
    }
    expect(ppRd33StarByTwoPredicates('t', 'c1', 'c2')).toBe('SELECT * FROM t WHERE c1=? AND c2=?');
  });

  test('the r33 module delegates its id/company head and keeps its star head', () => {
    const s = src('pp-read-tail-r33-sources.ts');
    expect((s.match(/export const /g) ?? []).length).toBe(2);
    expect((s.match(/SELECT /g) ?? []).length).toBe(1);
    expect(s).toContain("from './pp-read-tail-round4-sources'");
    expect(s).toContain('ppRdRound4ByProjectionIdCompany(cols, table)');
    expect(s.includes('SELECT * FROM ${table} WHERE ${c1}=? AND ${c2}=?')).toBe(true);
    for (const name of ['project-worker-startup-jobs.ts', 'people-payroll.ts', 'payroll-readiness.ts', 'people-handoffs.ts']) {
      expect(src(name)).toContain('ppRd33FourColByIdCompany');
    }
  });

  test('the fold lowers the owner SELECT count below 1140', () => {
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
    expect(total).toBeLessThan(1140);
    expect(total).toBeLessThanOrEqual(1139);
  });
});
