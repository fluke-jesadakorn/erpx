import {describe,expect,test} from 'vitest';
import {readFileSync} from 'node:fs';
import {
  PP_RD18_HR_PEOPLE_FROM_SQL,
  PP_RD18_HR_ENGAGEMENTS_E_JOIN_SQL,
  PP_RD18_HR_ENGAGEMENTS_E_FROM_SQL,
  PP_RD18_HR_PEOPLE_PROJ_SQL,
} from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r18-sources';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/adapters/${name}`, import.meta.url), 'utf8');
const quoted = (literal: string) => '"' + literal.replace(/"/g, '\\"') + '"';
const ALL = [PP_RD18_HR_PEOPLE_FROM_SQL, PP_RD18_HR_ENGAGEMENTS_E_JOIN_SQL, PP_RD18_HR_ENGAGEMENTS_E_FROM_SQL, PP_RD18_HR_PEOPLE_PROJ_SQL];

describe('People Payroll read-tail r18 byte-identical dedupe', () => {
  test('canonical constants own the exact read-only projection literals', () => {
    expect(PP_RD18_HR_PEOPLE_FROM_SQL).toBe('FROM (SELECT "id", "user_id", "company_id", "legal_name", "preferred_name", "email", "phone", "country_code", "status", "version", "created_by", "created_at", "updated_at", "merged_into_id", "work_email", "work_phone" FROM hr_people) hr_people');
    expect(PP_RD18_HR_ENGAGEMENTS_E_JOIN_SQL).toBe('JOIN (SELECT "id", "person_id", "company_id", "branch_id", "department_id", "worker_type", "employment_type", "status", "start_date", "end_date", "manager_person_id", "job_title", "currency", "version", "created_by", "created_at", "updated_at", "pay_group_id", "level_id", "position_id", "self_service_top_level" FROM hr_engagements) e');
    expect(PP_RD18_HR_ENGAGEMENTS_E_FROM_SQL).toBe('FROM (SELECT "id", "person_id", "company_id", "branch_id", "department_id", "worker_type", "employment_type", "status", "start_date", "end_date", "manager_person_id", "job_title", "currency", "version", "created_by", "created_at", "updated_at", "pay_group_id", "level_id", "position_id", "self_service_top_level" FROM hr_engagements) e');
    expect(PP_RD18_HR_PEOPLE_PROJ_SQL).toBe('SELECT "id","user_id","company_id","legal_name","preferred_name","email","phone","country_code","status","version","created_by","created_at","updated_at","merged_into_id","work_email","work_phone" FROM hr_people');
    for (const sql of ALL) {
      expect(sql).toContain('SELECT ');
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
  });

  test('each duplicate family has one owner literal and composed callers', () => {
    const families = [
      {constant:'PP_RD18_HR_PEOPLE_FROM_SQL', literal:PP_RD18_HR_PEOPLE_FROM_SQL, copies:8, callers:[['identity-snapshot-sources.ts',7],['learning-talent-snapshot-sources.ts',1]] as [string, number][]},
      {constant:'PP_RD18_HR_ENGAGEMENTS_E_JOIN_SQL', literal:PP_RD18_HR_ENGAGEMENTS_E_JOIN_SQL, copies:6, callers:[['identity-snapshot-sources.ts',5],['learning-talent-snapshot-sources.ts',1]] as [string, number][]},
      {constant:'PP_RD18_HR_ENGAGEMENTS_E_FROM_SQL', literal:PP_RD18_HR_ENGAGEMENTS_E_FROM_SQL, copies:4, callers:[['identity-snapshot-sources.ts',4]] as [string, number][]},
      {constant:'PP_RD18_HR_PEOPLE_PROJ_SQL', literal:PP_RD18_HR_PEOPLE_PROJ_SQL, copies:4, callers:[['commercial-owner-projection-sources.ts',1],['final-root-owner-projection-sources.ts',1],['inventory-executive-owner-projections.ts',1],['platform-owner-projection-sources.ts',1]] as [string, number][]},
    ];
    for (const f of families) {
      for (const [name, count] of f.callers) {
        const s = src(name);
        expect(s, name).not.toContain(quoted(f.literal));
        expect(s.match(new RegExp(f.constant, 'g')) ?? [], name).toHaveLength(count + 1);
      }
    }
  });

  test('the shared module declares its people literals once and delegates the engagement pair', () => {
    const s = src('pp-read-tail-r18-sources.ts');
    for (const literal of ALL) {
      if (literal === PP_RD18_HR_ENGAGEMENTS_E_JOIN_SQL || literal === PP_RD18_HR_ENGAGEMENTS_E_FROM_SQL) continue;
      expect(s.split(quoted(literal))).toHaveLength(2);
    }
    expect(s).toContain("from './pp-read-tail-round9-sources'");
    expect(s).toContain("ppRdRound9ProjectionFragment('JOIN', HR_ENGAGEMENT_COLUMNS, 'hr_engagements', 'e')");
    expect(s).toContain("ppRdRound9ProjectionFragment('FROM', HR_ENGAGEMENT_COLUMNS, 'hr_engagements', 'e')");
  });

  test('exported snapshot-source keys and aliases are unchanged', () => {
    const identity = src('identity-snapshot-sources.ts');
    expect(identity).toContain('"account_access_hr_people_from": PP_RD18_HR_PEOPLE_FROM_SQL');
    expect(identity).toContain('"iam_store_hr_engagements_e_join": PP_RD18_HR_ENGAGEMENTS_E_JOIN_SQL');
    expect(identity).toContain('"payroll_owner_facts_hr_engagements_from": PP_RD20_HR_ENGAGEMENTS_FROM_SQL');
    expect(src('commercial-owner-projection-sources.ts')).toContain('hr_people: PP_RD18_HR_PEOPLE_PROJ_SQL');
    expect(src('platform-owner-projection-sources.ts')).toContain('hr_people: PP_RD18_HR_PEOPLE_PROJ_SQL');
  });
});
