import { describe, expect, test } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { ppRdRound2bNotExistsByOnePred } from '../../../modules/people-payroll/server/src/adapters/pp-read-tail-round2b-sources';
import { ppRd62PeopleEmailGuard } from '../../../modules/people-payroll/server/src/adapters/pp-read-tail-r62-sources';
import { ppRd63CompRevisionEngagementGuard } from '../../../modules/people-payroll/server/src/adapters/pp-read-tail-r63-sources';

const src = (name: string) => readFileSync(
  new URL(`../../../modules/people-payroll/server/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('People Payroll round-2b cross-builder not-exists family', () => {
  test('one canonical builder renders both r62/r63 heads byte-identically', () => {
    const people = ppRdRound2bNotExistsByOnePred('hr_people', 'company_id', ' AND lower(email)=? AND id!=?');
    const revisions = ppRdRound2bNotExistsByOnePred('hr_compensation_revisions', 'engagement_id', " AND status='approved'");
    expect(people).toBe('NOT EXISTS(SELECT 1 FROM hr_people WHERE company_id=? AND lower(email)=? AND id!=?)');
    expect(revisions).toBe("NOT EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE engagement_id=? AND status='approved')");
    expect(ppRd62PeopleEmailGuard('')).toBe('NOT EXISTS(SELECT 1 FROM hr_people WHERE company_id=?)');
    expect(ppRd63CompRevisionEngagementGuard('')).toBe('NOT EXISTS(SELECT 1 FROM hr_compensation_revisions WHERE engagement_id=?)');
    for (const sql of [people, revisions]) {
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory|nativePpStatement/i);
    }
  });

  test('the two former owner-local heads are gone and delegate to the canonical module', () => {
    for (const name of ['pp-read-tail-r62-sources.ts', 'pp-read-tail-r63-sources.ts']) {
      const helper = src(name);
      expect((helper.match(/SELECT /g) ?? []).length).toBe(0);
      expect(helper).toContain("from './pp-read-tail-round2b-sources'");
      expect(helper).toContain('ppRdRound2bNotExistsByOnePred(');
    }
    const canonical = src('pp-read-tail-round2b-sources.ts');
    expect((canonical.match(/export const /g) ?? []).length).toBe(1);
    expect((canonical.match(/SELECT /g) ?? []).length).toBe(1);
    expect(canonical).not.toContain('@erpx/server-');
  });

  test('the fold lowers the owner SELECT count below 1149', () => {
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
    expect(total).toBeLessThan(1149);
    expect(total).toBeLessThanOrEqual(1148);
  });
});
