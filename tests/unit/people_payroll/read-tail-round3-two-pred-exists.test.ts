import { describe, expect, test } from 'vitest';
import { readdirSync, readFileSync } from 'node:fs';
import { ppRdRound3ExistsByIdTwoPred } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-round3-sources';
import { PP_RD22_ENGAGEMENT_VERSION_GUARD } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r22-sources';
import { PP_RD23_EVAL_ASSESSMENT_VERSION_GUARD } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r23-sources';

const src = (name: string) => readFileSync(
  new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8',
);

describe('People Payroll round-3 by-id/two-predicate exists family', () => {
  test('one canonical builder renders both former constants byte-identically', () => {
    expect(ppRdRound3ExistsByIdTwoPred('hr_engagements', 'company_id', 'version')).toBe(
      'EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=?)',
    );
    expect(ppRdRound3ExistsByIdTwoPred('hr_evaluation_assessments', 'version', 'status')).toBe(
      'EXISTS(SELECT 1 FROM hr_evaluation_assessments WHERE id=? AND version=? AND status=?)',
    );
    expect(PP_RD22_ENGAGEMENT_VERSION_GUARD).toBe('EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND company_id=? AND version=?)');
    expect(PP_RD23_EVAL_ASSESSMENT_VERSION_GUARD).toBe('EXISTS(SELECT 1 FROM hr_evaluation_assessments WHERE id=? AND version=? AND status=?)');
    for (const sql of [PP_RD22_ENGAGEMENT_VERSION_GUARD, PP_RD23_EVAL_ASSESSMENT_VERSION_GUARD]) {
      expect((sql.match(/\?/g) ?? []).length).toBe(3);
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('both constants delegate and consumer imports stay unchanged', () => {
    expect(src('pp-read-tail-r22-sources.ts')).toContain("from './pp-read-tail-round3-sources'");
    expect(src('pp-read-tail-r22-sources.ts')).toContain("ppRdRound3ExistsByIdTwoPred('hr_engagements', 'company_id', 'version')");
    expect(src('pp-read-tail-r23-sources.ts')).toContain("from './pp-read-tail-round3-sources'");
    expect(src('pp-read-tail-r23-sources.ts')).toContain("ppRdRound3ExistsByIdTwoPred('hr_evaluation_assessments', 'version', 'status')");
    expect(src('people-evaluation.ts')).toContain('PP_RD22_ENGAGEMENT_VERSION_GUARD');
    expect(src('people-evaluation.ts')).toContain('PP_RD23_EVAL_ASSESSMENT_VERSION_GUARD');
    const canonical = src('pp-read-tail-round3-sources.ts');
    expect((canonical.match(/export const /g) ?? []).length).toBe(1);
    expect((canonical.match(/SELECT /g) ?? []).length).toBe(1);
    expect(canonical).not.toContain('@erpx/server-');
  });

  test('the fold lowers the owner SELECT count below 1145', () => {
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
    expect(total).toBeLessThan(1145);
    expect(total).toBeLessThanOrEqual(1144);
  });
});
