import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import {
  PP_RD19_HR_ENGAGEMENT_VERSION_EXISTS_SQL,
  PP_RD19_HR_PEOPLE_P_FROM_SQL,
  PP_RD19_EVAL_DEF_OPEN_EXISTS_SQL,
  PP_RD19_ACTIVE_ENGAGEMENT_EXISTS_SQL,
  ppRd19InList,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r19-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const quoted = (literal: string) => JSON.stringify(literal);
const literals = [
  PP_RD19_HR_ENGAGEMENT_VERSION_EXISTS_SQL,
  PP_RD19_HR_PEOPLE_P_FROM_SQL,
  PP_RD19_EVAL_DEF_OPEN_EXISTS_SQL,
  PP_RD19_ACTIVE_ENGAGEMENT_EXISTS_SQL,
];

describe('People Payroll read-tail r19 byte-identical dedupe', () => {
  test('canonical constants own the exact read-only guard/projection literals', () => {
    expect(PP_RD19_HR_ENGAGEMENT_VERSION_EXISTS_SQL).toBe('EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=?)');
    expect(PP_RD19_HR_PEOPLE_P_FROM_SQL).toBe('FROM (SELECT "id", "user_id", "company_id", "legal_name", "preferred_name", "email", "phone", "country_code", "status", "version", "created_by", "created_at", "updated_at", "merged_into_id", "work_email", "work_phone" FROM hr_people) p');
    expect(PP_RD19_EVAL_DEF_OPEN_EXISTS_SQL).toBe("EXISTS(SELECT 1 FROM hr_evaluation_definitions WHERE id=? AND version=? AND payload_json::jsonb->>'status'='open')");
    expect(PP_RD19_ACTIVE_ENGAGEMENT_EXISTS_SQL).toBe("EXISTS(SELECT 1 FROM hr_engagements WHERE id=? AND version=? AND status='active')");
    for (const sql of literals) {
      expect(sql).toContain('SELECT ');
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('each duplicate family has one shared literal and composed callers with wrappers/binds kept', () => {
    const families = [
      { constant: 'PP_RD19_HR_ENGAGEMENT_VERSION_EXISTS_SQL', literal: PP_RD19_HR_ENGAGEMENT_VERSION_EXISTS_SQL, callers: [['leave.ts', 1], ['people-directory.ts', 1], ['people-evaluation.ts', 5]] as [string, number][] },
      { constant: 'PP_RD19_HR_PEOPLE_P_FROM_SQL', literal: PP_RD19_HR_PEOPLE_P_FROM_SQL, callers: [['identity-snapshot-sources.ts', 3], ['learning-talent-snapshot-sources.ts', 1]] as [string, number][] },
      { constant: 'PP_RD19_EVAL_DEF_OPEN_EXISTS_SQL', literal: PP_RD19_EVAL_DEF_OPEN_EXISTS_SQL, callers: [['people-evaluation.ts', 4]] as [string, number][] },
      { constant: 'PP_RD19_ACTIVE_ENGAGEMENT_EXISTS_SQL', literal: PP_RD19_ACTIVE_ENGAGEMENT_EXISTS_SQL, callers: [['people-evaluation.ts', 2], ['people-payroll.ts', 1], ['expense-routing.ts', 1]] as [string, number][] },
    ];
    for (const f of families) {
      for (const [name, count] of f.callers) {
        const s = src(name);
        expect(s, name).not.toContain(quoted(f.literal));
        expect(s.match(new RegExp(f.constant, 'g')) ?? [], name).toHaveLength(count + 1);
      }
    }
    expect(src('leave.ts')).toContain('PP_RD19_HR_ENGAGEMENT_VERSION_EXISTS_SQL, [engagement_id, engagement_version]');
    expect(src('people-directory.ts')).toContain('PP_RD19_HR_ENGAGEMENT_VERSION_EXISTS_SQL, [engagement, value2]');
    expect(src('expense-routing.ts')).toContain('query: PP_RD19_ACTIVE_ENGAGEMENT_EXISTS_SQL, args: [staff.engagement_id, staff.engagement_version]');
    expect(src('identity-snapshot-sources.ts')).toContain('"account_access_hr_people_p_from": PP_RD19_HR_PEOPLE_P_FROM_SQL');
  });

  test('the six calendar inList helpers import one shared builder and keep no local copy', () => {
    const builders = [
      'calendar-leave-facts.ts', 'calendar-appointments-facts.ts', 'calendar-index-facts.ts',
      'calendar-roster-facts.ts', 'calendar-lifecycle-facts.ts', 'calendar-access-facts.ts',
    ];
    for (const name of builders) {
      const s = src(name);
      expect(s, name).toContain("import { ppRd19InList as inList } from './pp-read-tail-r19-sources';");
      expect(s, name).not.toContain('const inList = (expression: string) =>');
      expect(s, name).toContain('inList(');
    }
    expect(ppRd19InList('e.company_id')).toBe('e.company_id IN (SELECT jsonb_array_elements_text(?::jsonb))');
    expect(src('calendar-access-facts.ts')).toContain('const idList = (ids: string[]) => JSON.stringify(ids);');
  });

  test('the shared module declares each literal exactly once', () => {
    const s = src('pp-read-tail-r19-sources.ts');
    for (const literal of literals) expect(s.split(quoted(literal))).toHaveLength(2);
    expect(s.split('IN (SELECT jsonb_array_elements_text(?::jsonb))')).toHaveLength(2);
  });

  test('worktree read-only SELECT count is strictly lower than the recorded 1572 before', () => {
    const root = new URL('../../../packages/server-people_payroll/src/', import.meta.url);
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
    expect(total).toBeLessThan(1572);
    expect(total).toBe(1552);
  });
});
