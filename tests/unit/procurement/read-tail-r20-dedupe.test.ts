import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  receiptCount,
  expenseAccountBy,
  hrPersonIn,
} from '../../../packages/server-procurement/src/adapters/read-tail-r20';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-procurement/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('Procurement read-tail r20 byte-identical / parameterized dedupe', () => {
  test('receipt count projection renders the exact original literal', () => {
    const literal = '(SELECT count(*) FROM procurement_bill_match_receipts br WHERE br.match_id=m.id) receipt_count';
    expect(receiptCount('m.id')).toBe(literal);
    const s = src('supplier-bills.ts');
    expect(s).not.toContain(literal);
    expect(s.match(/receiptCount\("m\.id"\)/g)?.length).toBe(2);
  });

  test('expense account by id renders the exact original literals with binds unchanged', () => {
    const accounts = 'ACCOUNTS', charts = 'CHARTS';
    const base = `FROM (${accounts}) a JOIN (${charts}) c ON c.id=a.chart_id AND c.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND a.chart_id=? AND a.currency=c.base_currency AND a.category='expense' AND a.status='active'`;
    expect(expenseAccountBy(accounts, charts, 'a.id')).toBe(`SELECT a.id ${base}`);
    expect(expenseAccountBy(accounts, charts, '1')).toBe(`SELECT 1 ${base}`);
    const s = src('supplier-bills.ts');
    expect(s.match(/expenseAccountBy\(/g)?.length).toBe(2);
    expect(s.match(/a\.currency=c\.base_currency AND a\.category='expense' AND a\.status='active'/g)?.length).toBe(1);
  });

  test('HR person-in-company scope renders the exact original literals', () => {
    const people = 'HR';
    expect(hrPersonIn(people, 'r.created_by', 'r')).toBe(
      `r.created_by IN(SELECT user_id FROM (${people}) hr_people WHERE id=? AND company_id=r.company_id)`,
    );
    expect(hrPersonIn(people, 'h.recorded_by', 'o')).toBe(
      `h.recorded_by IN(SELECT user_id FROM (${people}) hr_people WHERE id=? AND company_id=o.company_id)`,
    );
    for (const name of ['calendar-procurement-rfqs-facts.ts', 'calendar-supplier-promises-facts.ts']) {
      const s = src(name);
      expect(s).toContain('hrPersonIn(');
      expect(s).not.toContain(' IN(SELECT user_id FROM (${people_payrollPlatformSources.hr_people}) hr_people WHERE id=? AND company_id=');
    }
  });

  test('no write, lock or r17/r18/r19 helper was touched', () => {
    const tail = src('read-tail-sql.ts');
    expect(tail).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(tail).toContain('export const pointRead =');
    expect(tail).toContain('export const orderedRead =');
    const r19 = src('read-tail-r19.ts');
    expect(r19).toContain('export const tableRead =');
    expect(r19).toContain('export const receiptImage =');
    expect(src('purchases.ts')).toContain('FOR UPDATE');
    expect(src('bill-purchases.ts')).toContain('FOR SHARE OF f,o');
  });

  test('worktree SELECT count is strictly lower than the recorded BEFORE=166', () => {
    const root = new URL('../../../packages/server-procurement/src/', import.meta.url);
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
    expect(total).toBeLessThan(166);
  });
});
