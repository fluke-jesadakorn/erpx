import { describe, expect, test } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { ppRdRound5ExistsByIdAndStatus } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-round5-sources';
import { PP_RD19_ACTIVE_ENGAGEMENT_EXISTS_SQL } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r19-sources';
import { PP_RD23_POSTED_PAYROLL_RUN_GUARD } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r23-sources';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('People Payroll round-5 by-id/status guard family', () => {
  test('one canonical builder renders both former constants byte-identically', () => {
    expect(ppRdRound5ExistsByIdAndStatus('hr_engagements', 'version', 'active')).toBe(
      "EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=? AND status='active')",
    );
    expect(ppRdRound5ExistsByIdAndStatus('hr_payroll_runs', 'company_id', 'posted')).toBe(
      "EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='posted')",
    );
    expect(PP_RD19_ACTIVE_ENGAGEMENT_EXISTS_SQL).toBe("EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=? AND status='active')");
    expect(PP_RD23_POSTED_PAYROLL_RUN_GUARD).toBe("EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='posted')");
    for (const sql of [PP_RD19_ACTIVE_ENGAGEMENT_EXISTS_SQL, PP_RD23_POSTED_PAYROLL_RUN_GUARD]) {
      expect((sql.match(/\?/g) ?? []).length).toBe(2);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('both constants delegate and consumer imports stay unchanged', () => {
    expect(src('pp-read-tail-r19-sources.ts')).toContain("from './pp-read-tail-round5-sources'");
    expect(src('pp-read-tail-r19-sources.ts')).toContain("ppRdRound5ExistsByIdAndStatus('hr_engagements', 'version', 'active')");
    expect(src('pp-read-tail-r23-sources.ts')).toContain("from './pp-read-tail-round5-sources'");
    expect(src('pp-read-tail-r23-sources.ts')).toContain("ppRdRound5ExistsByIdAndStatus('hr_payroll_runs', 'company_id', 'posted')");
    for (const name of ['people-evaluation.ts', 'people-payroll.ts', 'expense-routing.ts']) {
      expect(src(name)).toContain('PP_RD19_ACTIVE_ENGAGEMENT_EXISTS_SQL');
    }
    expect(src('people-payroll.ts')).toContain('PP_RD23_POSTED_PAYROLL_RUN_GUARD');
    const canonical = src('pp-read-tail-round5-sources.ts');
    expect((canonical.match(/export const /g) ?? []).length).toBe(1);
    expect((canonical.match(/SELECT /g) ?? []).length).toBe(1);
    expect(canonical).not.toContain('@erpx/server-');
  });

  test('the fold lowers the owner SELECT count below 1142', () => {
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
    expect(total).toBeLessThan(1142);
    expect(total).toBeLessThanOrEqual(1141);
  });
});
