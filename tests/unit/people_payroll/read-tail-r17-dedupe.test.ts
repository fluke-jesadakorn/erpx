import {describe,expect,test} from 'vitest';
import {readFileSync,existsSync} from 'node:fs';

const dir = new URL('../../../modules/people-payroll/server/src/adapters/', import.meta.url);
const src = (name: string) => readFileSync(new URL(name, dir), 'utf8');

const EXPECTED_CTE = `WITH accounts AS (
    SELECT * FROM jsonb_to_recordset(?::jsonb) AS a(id text,name text,email text,"personId" text)
  ), people AS (
    SELECT p.id,p.id AS "personId",COALESCE(p.user_id,(SELECT a.id FROM accounts a WHERE a."personId"=p.id ORDER BY a.id LIMIT 1)) AS "userId",p.legal_name AS name,
      COALESCE(p.email,'') AS email,COALESCE(e.job_title,'') AS "jobTitle",
      COALESCE(e.status,p.status) AS status,p.status AS profile_status,
      e.id AS "engagementId",e.position_id AS "positionId",e.manager_person_id AS "managerPersonId",e.level_id,e.department_id,
      COALESCE((SELECT jsonb_agg(a) FROM accounts a WHERE a."personId"=p.id OR a.id=p.user_id),'[]'::jsonb) AS accounts,
      retirement.definition->>'lastWorkingDate' AS "retirementDate",
      retirement.effective_at<=? AS "retired"
    FROM hr_people p
    LEFT JOIN LATERAL (SELECT * FROM hr_engagements x WHERE x.person_id=p.id AND x.company_id=p.company_id AND x.worker_type='employee'
      ORDER BY CASE WHEN x.status IN ('active','onboarding','suspended') THEN 0 ELSE 1 END,x.start_date DESC,x.id LIMIT 1) e ON true
    LEFT JOIN LATERAL (SELECT definition,effective_at FROM people_lifecycle_cases c
      WHERE c.company_id=p.company_id AND c.person_id=p.id AND c.engagement_id=e.id
        AND ?=1 AND c.definition->>'departureReason'='retirement' AND c.status IN ('scheduled','completed')
      ORDER BY c.updated_at DESC,c.id LIMIT 1) retirement ON true
    WHERE p.company_id=? AND p.merged_into_id IS NULL AND e.worker_type='employee'
      AND (?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?)
  ), directory AS (
    SELECT * FROM people
    UNION ALL SELECT 'account-'||a.id,NULL,a.id,a.name,a.email,''::text,'Profile not linked','active',
      NULL,NULL,NULL,NULL,NULL,jsonb_build_array(a),NULL,NULL
      FROM accounts a WHERE false AND a."personId" IS NULL AND NOT EXISTS(SELECT 1 FROM people p WHERE p."userId"=a.id)
  ) SELECT *,name AS page_key FROM directory`;

describe('People Payroll read-tail r17 byte-identical dedupe', () => {
  const file = src('people-directory-page.ts');

  test('the 1988-char directory CTE is declared exactly once, byte-identical', () => {
    const m = file.match(/const directoryCte = `([\s\S]*?)`;/);
    expect(m).not.toBeNull();
    expect(m![1]).toBe(EXPECTED_CTE);
    expect(file.split(EXPECTED_CTE)).toHaveLength(2);
  });

  test('both page and selected ports reference the shared const with no inline copy', () => {
    expect(file.match(/const sql = directoryCte;/g)).toHaveLength(2);
    expect(file).not.toContain('const sql = `WITH accounts AS (');
  });

  test('the CTE is read-only and exported keys are unchanged', () => {
    expect(EXPECTED_CTE).toHaveLength(1988);
    expect((EXPECTED_CTE.match(/SELECT /g) ?? []).length).toBe(10);
    expect(EXPECTED_CTE).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    expect(file).toContain('export async function peopleDirectoryPage(');
    expect(file).toContain('export type {DirectoryQuery}');
  });

  test('gpo-sources orphan adapter, barrel and package exports are removed', () => {
    expect(existsSync(new URL('gpo-sources.ts', dir))).toBe(false);
    expect(existsSync(new URL('../public/gpo-sources.ts', dir))).toBe(false);
    const pkg = readFileSync(new URL('../../../modules/people-payroll/server/package.json', import.meta.url), 'utf8');
    expect(pkg).not.toContain('"./adapters/gpo-sources"');
    expect(pkg).not.toContain('"./public/gpo-sources"');
  });
});
