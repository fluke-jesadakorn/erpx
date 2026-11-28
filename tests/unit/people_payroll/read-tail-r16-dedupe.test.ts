import {describe,expect,test} from 'vitest';
import {readFileSync} from 'node:fs';
import {PP_RD16_HR_PEOPLE_P_JOIN_SQL} from '../../../modules/people-payroll/server/src/adapters/pp-read-tail-r16-sources';

const src = (name: string) => readFileSync(new URL(`../../../modules/people-payroll/server/src/adapters/${name}`, import.meta.url), 'utf8');
const LITERAL = 'JOIN (SELECT "id", "user_id", "company_id", "legal_name", "preferred_name", "email", "phone", "country_code", "status", "version", "created_by", "created_at", "updated_at", "merged_into_id", "work_email", "work_phone" FROM hr_people) p';
const QUOTED = '"' + LITERAL.replace(/"/g, '\\"') + '"';
const CALLERS = [
  'identity-snapshot-sources.ts',
  'learning-talent-snapshot-sources.ts',
  'collaboration-notification-snapshot-sources.ts',
];

describe('People Payroll read-tail r16 byte-identical dedupe', () => {
  test('canonical constant owns the exact 267-character hr_people JOIN projection', () => {
    expect(PP_RD16_HR_PEOPLE_P_JOIN_SQL).toBe(LITERAL);
    expect(PP_RD16_HR_PEOPLE_P_JOIN_SQL).toContain('SELECT ');
    expect(PP_RD16_HR_PEOPLE_P_JOIN_SQL).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  test('all 15 former inline copies are imported references with no local copy', () => {
    for (const name of CALLERS) {
      const s = src(name);
      expect(s, name).toContain("import {PP_RD16_HR_PEOPLE_P_JOIN_SQL} from './pp-read-tail-r16-sources'");
      expect(s, name).not.toContain(QUOTED);
    }
    const refs = CALLERS.reduce((n, name) => n + (src(name).match(/PP_RD16_HR_PEOPLE_P_JOIN_SQL/g) ?? []).length, 0);
    // 15 object-key values + 3 import specifiers
    expect(refs).toBe(18);
  });

  test('the shared module declares the literal exactly once', () => {
    const s = src('pp-read-tail-r16-sources.ts');
    expect(s.split(QUOTED)).toHaveLength(2);
  });

  test('exported snapshot-source keys are unchanged', () => {
    expect(src('identity-snapshot-sources.ts')).toContain('"position_gpo_hr_people_person_join": "JOIN (SELECT \\"id\\", \\"user_id\\", \\"company_id\\", \\"legal_name\\", \\"preferred_name\\", \\"email\\", \\"phone\\", \\"country_code\\", \\"status\\", \\"version\\", \\"created_by\\", \\"created_at\\", \\"updated_at\\", \\"merged_into_id\\", \\"work_email\\", \\"work_phone\\" FROM hr_people) person"');
  });
});
