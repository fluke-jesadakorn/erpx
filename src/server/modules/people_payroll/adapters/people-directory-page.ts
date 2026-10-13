import 'server-only';import {internalAccessPeople} from '../public/iam-subject-facts';import type {WorkContext} from '../../identity_access/public/authorization';import type {SqlDatabase} from '../../runtime/public/runtime/types';import {holdingResources} from '../../runtime/public/runtime';import {companyMembers} from '../../organization/public/member-directory';import {readListPage} from '../../work_projection/public/list-page';import type {PeopleDirectoryRow,DirectoryQuery} from '../domain/people-directory-page';import {createPeopleDirectoryPageApplication,type DirectoryPagePorts} from '../application/people-directory-page';export type {DirectoryQuery} from '../domain/people-directory-page';
export async function peopleDirectoryPage(actor:WorkContext,query:DirectoryQuery){let write:SqlDatabase;const ports:DirectoryPagePorts={clock:{now:Date.now},prepare:async a=>{write=(await holdingResources(a.holdingId)).write;},members:async actor=>{const employees=await internalAccessPeople(write,actor.companyId);const ids=new Set(employees.map(p=>p.personId));return (await companyMembers(actor)).filter(m=>m.personId&&ids.has(m.personId));},async page(request){const {actor,members,query,now,canRetirement}=request;  const sql = `WITH accounts AS (
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
const args:unknown[]=[JSON.stringify(members),now,canRetirement?1:0,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId];  const conditions = [`profile_status ${query.status === 'archived' ? '=' : '<>'} 'archived'`];
  const filteredArgs: unknown[] = [];
  if (query.q) { conditions.push(`strpos(lower(name||' '||email||' '||"jobTitle"),lower(?))>0`); filteredArgs.push(query.q.slice(0,200)); }
  if (query.level) { conditions.push(query.level === 'unset' ? 'level_id IS NULL' : 'level_id=?'); if(query.level !== 'unset') filteredArgs.push(query.level); }
  if (query.department) {conditions.push('department_id=?');filteredArgs.push(query.department);}
  if (query.retirement === 'retired' || query.retirement === 'upcoming') conditions.push(`"retirementDate" IS NOT NULL AND retired=${query.retirement === 'retired' ? 'true' : 'false'}`);
return readListPage<PeopleDirectoryRow&{page_key:string}>(write,{    sql:`${sql} WHERE ${conditions.join(' AND ')}`,args:[...args,...filteredArgs],cursor:query.cursor,
    scope:[actor.companyId,actor.userId,actor.branchId,actor.departmentId,query.q??'',query.status??'',query.level??'',query.department??'',query.retirement??''],
});},async selected(request){const {actor,members,query,now,canRetirement}=request;  const sql = `WITH accounts AS (
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
const args:unknown[]=[JSON.stringify(members),now,canRetirement?1:0,actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId];return await write.prepare(`${sql} WHERE
    (?::text IS NOT NULL AND id=?) OR (?::text IS NOT NULL AND ("userId"=? OR accounts @> jsonb_build_array(jsonb_build_object('id',?::text))))
    OR (?::text IS NOT NULL AND "personId"=(SELECT person_id FROM people_lifecycle_cases WHERE id=? AND company_id=?)) LIMIT 1`)
    .bind(...args,query.person??null,query.person??null,query.member??null,query.member??null,query.member??null,query.case??null,query.case??null,actor.companyId).first<PeopleDirectoryRow>();}};return createPeopleDirectoryPageApplication(ports).peopleDirectoryPage(actor,query);}
