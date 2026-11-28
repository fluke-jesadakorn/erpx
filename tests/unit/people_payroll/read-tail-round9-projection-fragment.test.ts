import { describe, expect, test } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { ppRdRound9ProjectionFragment } from '../../../modules/people-payroll/server/src/adapters/pp-read-tail-round9-sources';
import {
  PP_RD18_HR_ENGAGEMENTS_E_JOIN_SQL,
  PP_RD18_HR_ENGAGEMENTS_E_FROM_SQL,
} from '../../../modules/people-payroll/server/src/adapters/pp-read-tail-r18-sources';

const src = (name: string) => readFileSync(
  new URL(`../../../modules/people-payroll/server/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('People Payroll round-9 quoted projection fragment family', () => {
  test('one renderer produces byte-identical JOIN and FROM forms', () => {
    const columns = '"id", "person_id", "status"';
    expect(ppRdRound9ProjectionFragment('JOIN', columns, 'hr_engagements', 'e')).toBe(
      'JOIN (SELECT "id", "person_id", "status" FROM hr_engagements) e',
    );
    expect(ppRdRound9ProjectionFragment('FROM', columns, 'hr_engagements', 'e')).toBe(
      'FROM (SELECT "id", "person_id", "status" FROM hr_engagements) e',
    );
    expect(PP_RD18_HR_ENGAGEMENTS_E_JOIN_SQL.startsWith('JOIN (SELECT ')).toBe(true);
    expect(PP_RD18_HR_ENGAGEMENTS_E_FROM_SQL.startsWith('FROM (SELECT ')).toBe(true);
    expect(PP_RD18_HR_ENGAGEMENTS_E_JOIN_SQL.slice(5)).toBe(PP_RD18_HR_ENGAGEMENTS_E_FROM_SQL.slice(5));
    expect(PP_RD18_HR_ENGAGEMENTS_E_JOIN_SQL.endsWith(' FROM hr_engagements) e')).toBe(true);
  });

  test('the r18 engagement pair delegates and keeps no local SQL token', () => {
    const s = src('pp-read-tail-r18-sources.ts');
    expect(s).toContain("from './pp-read-tail-round9-sources'");
    expect(s).toContain("ppRdRound9ProjectionFragment('JOIN', HR_ENGAGEMENT_COLUMNS, 'hr_engagements', 'e')");
    expect(s).toContain("ppRdRound9ProjectionFragment('FROM', HR_ENGAGEMENT_COLUMNS, 'hr_engagements', 'e')");
    expect(s.split('JOIN (SELECT \\"id\\", \\"person_id\\"')).toHaveLength(1);
    const canonical = src('pp-read-tail-round9-sources.ts');
    expect((canonical.match(/export const /g) ?? []).length).toBe(1);
    expect((canonical.match(/SELECT /g) ?? []).length).toBe(1);
    expect(canonical).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|pg_advisory|INSERT INTO|DELETE FROM/i);
    expect(canonical).not.toContain('@erpx/server-');
  });

  test('the fold lowers the owner SELECT count below 1138', () => {
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
    expect(total).toBeLessThan(1138);
    expect(total).toBeLessThanOrEqual(1137);
  });
});
