import { describe, expect, test } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { ppRdRound2dExistsById } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-round2d-sources';
import { ppRd57PeopleIdGuard } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r57-sources';
import { ppRd64TimeEntryIdGuard } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r64-sources';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('People Payroll round-2d cross-builder id-exists family', () => {
  test('one canonical builder renders both r57/r64 heads byte-identically', () => {
    expect(ppRdRound2dExistsById('hr_people', '')).toBe('EXISTS(SELECT 1 FROM hr_people WHERE id=?)');
    expect(ppRdRound2dExistsById('hr_time_entries', '')).toBe('EXISTS(SELECT 1 FROM hr_time_entries WHERE id=?)');
    expect(ppRd57PeopleIdGuard(' AND version=?')).toBe('EXISTS(SELECT 1 FROM hr_people WHERE id=? AND version=?)');
    expect(ppRd64TimeEntryIdGuard(" AND state='draft'")).toBe("EXISTS(SELECT 1 FROM hr_time_entries WHERE id=? AND state='draft')");
    for (const sql of [
      ppRdRound2dExistsById('hr_people', ' AND version=?'),
      ppRdRound2dExistsById('hr_time_entries', " AND state='draft'"),
    ]) {
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativePpStatement/i);
    }
  });

  test('both former owner-local heads are gone and delegate to the canonical module', () => {
    for (const name of ['pp-read-tail-r57-sources.ts', 'pp-read-tail-r64-sources.ts']) {
      const helper = src(name);
      expect(helper).toContain("from './pp-read-tail-round2d-sources'");
      expect(helper).toContain('ppRdRound2dExistsById(');
      expect((helper.match(/SELECT /g) ?? []).length).toBe(0);
    }
    expect(src('pp-read-tail-r57-sources.ts')).toContain("ppRdRound2dExistsById('hr_people', tail)");
    expect(src('pp-read-tail-r64-sources.ts')).toContain("ppRdRound2dExistsById('hr_time_entries', tail)");
    const canonical = src('pp-read-tail-round2d-sources.ts');
    expect((canonical.match(/export const /g) ?? []).length).toBe(1);
    expect((canonical.match(/SELECT /g) ?? []).length).toBe(1);
    expect(canonical).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|pg_advisory|nativePpStatement/i);
    expect(canonical).not.toContain('@erpx/server-');
  });

  test('the fold lowers the owner SELECT count below 1146', () => {
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
    expect(total).toBeLessThan(1146);
    expect(total).toBeLessThanOrEqual(1145);
  });
});
