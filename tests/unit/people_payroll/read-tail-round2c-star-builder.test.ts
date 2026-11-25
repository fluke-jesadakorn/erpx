import { describe, expect, test } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { ppRdRound2cStarByOnePred } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-round2c-sources';
import { ppRd57CompRevisionByEngagementSql } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r57-sources';
import { ppRd58RosterShiftsByWeekSql } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r58-sources';
import { ppRd59ThaiProfilesByCompanySql } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r59-sources';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('People Payroll round-2c cross-builder row-list family', () => {
  test('one canonical builder renders all three r57/r58/r59 heads byte-identically', () => {
    expect(ppRdRound2cStarByOnePred('hr_compensation_revisions', 'engagement_id', '')).toBe(
      'SELECT * FROM hr_compensation_revisions WHERE engagement_id=?',
    );
    expect(ppRdRound2cStarByOnePred('people_roster_shifts', 'week_id', '')).toBe(
      'SELECT * FROM people_roster_shifts WHERE week_id=?',
    );
    expect(ppRdRound2cStarByOnePred('hr_thai_payroll_profiles', 'company_id', '')).toBe(
      'SELECT * FROM hr_thai_payroll_profiles WHERE company_id=?',
    );
    expect(ppRd57CompRevisionByEngagementSql(" AND status='approved'")).toBe(
      "SELECT * FROM hr_compensation_revisions WHERE engagement_id=? AND status='approved'",
    );
    expect(ppRd58RosterShiftsByWeekSql(" AND state='active'")).toBe(
      "SELECT * FROM people_roster_shifts WHERE week_id=? AND state='active'",
    );
    expect(ppRd59ThaiProfilesByCompanySql(' ORDER BY created_at DESC')).toBe(
      'SELECT * FROM hr_thai_payroll_profiles WHERE company_id=? ORDER BY created_at DESC',
    );
  });

  test('the three former owner-local heads are gone and delegate to the canonical module', () => {
    const local = ['pp-read-tail-r57-sources.ts', 'pp-read-tail-r58-sources.ts', 'pp-read-tail-r59-sources.ts'];
    for (const name of local) {
      const helper = src(name);
      expect(helper).toContain("from './pp-read-tail-round2c-sources'");
      expect(helper).toContain('ppRdRound2cStarByOnePred(');
      expect(helper).not.toMatch(/FROM hr_compensation_revisions WHERE engagement_id=\?\$\{tail\}|FROM people_roster_shifts WHERE week_id=\?\$\{tail\}|FROM hr_thai_payroll_profiles WHERE company_id=\?\$\{tail\}/);
    }
    expect((src('pp-read-tail-r57-sources.ts').match(/SELECT /g) ?? []).length).toBe(1);
    expect((src('pp-read-tail-r58-sources.ts').match(/SELECT /g) ?? []).length).toBe(0);
    expect((src('pp-read-tail-r59-sources.ts').match(/SELECT /g) ?? []).length).toBe(0);
    const canonical = src('pp-read-tail-round2c-sources.ts');
    expect((canonical.match(/export const /g) ?? []).length).toBe(1);
    expect((canonical.match(/SELECT /g) ?? []).length).toBe(1);
    expect(canonical).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|pg_advisory|nativePpStatement/i);
    expect(canonical).not.toContain('@erpx/server-');
  });

  test('the fold lowers the owner SELECT count below 1148', () => {
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
    expect(total).toBeLessThan(1148);
    expect(total).toBeLessThanOrEqual(1146);
  });
});
