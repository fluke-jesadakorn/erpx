import {peoplePayrollFinalOwnerProjections as benefitsFinalOwnerProjections} from "@erpx/server-benefits/public/people-payroll-final-owner-projections";
const benefitsFinalSources=benefitsFinalOwnerProjections();
import "server-only";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
const sensitiveTables:Record<string,string>={hr_evaluation_assessment:"hr_evaluation_assessments",hr_person:"hr_people",people_lifecycle_case:"people_lifecycle_cases",people_clearance_task:"people_preparation_tasks",payroll_run:"hr_payroll_runs",payroll_advance:"hr_payroll_advances",payroll_final_pay:"hr_payroll_final_pay_preparations"};
const sensitiveRecordTables = new Set([
  "hr_evaluation_assessment",
  "hr_person",
  "people_lifecycle_case",
  "payroll_advance",
  "payroll_final_pay",
  "talent_candidate",
  "talent_offer",
  "learning_course",
  "learning_assignment",
  "benefit_plan",
  "benefit_dependent",
  "benefit_election",
]);

function sensitiveAttachmentTargetVersionGuard(
  db: SqlDatabase,
  commandId: string,
  targetType: string,
  targetId: string,
  companyId: string,
  version: number,
  index: number,
): SqlStatement | null {
  const table = sensitiveTables[targetType];
  if (!table) return null;
  return assertion(
    db,
    commandId,
    `sensitive-attachment-owner-${index}`,
    `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?)`,
    [targetId, companyId, version],
  );
}
function sensitiveAttachmentRecordVersionStep(
  db: SqlDatabase,
  targetType: string,
  targetId: string,
  expectedVersion: number,
  now = Date.now(),
): SqlStatement | null {
  if (!sensitiveRecordTables.has(targetType)) return null;
  const table = sensitiveTables[targetType];
  if(!table)return null;
  return db
    .prepare(`UPDATE ${table} SET version=version+1,updated_at=? WHERE id=? AND version=?`)
    .bind(now, targetId, expectedVersion);
}
export function SensitiveAttachmentPeoplePayrollFacts(db:SqlDatabase){return {
readHrPeople1:(facts:{userId:string;targetId:string;companyId:string;})=>db.prepare(`SELECT p.company_id,p.user_id,p.version,e.branch_id,e.department_id,
        CASE WHEN iam_same_person(p.user_id,?,p.company_id) THEN 1 ELSE 0 END is_self
      FROM hr_people p
      LEFT JOIN LATERAL (
        SELECT x.branch_id,x.department_id FROM hr_engagements x
        WHERE x.person_id=p.id AND x.company_id=p.company_id
        ORDER BY CASE WHEN x.status IN ('active','onboarding','suspended') THEN 0 ELSE 1 END,x.start_date DESC,x.id
        LIMIT 1
      ) e ON true
      WHERE p.id=? AND p.company_id=? AND p.merged_into_id IS NULL`).bind(facts.userId,facts.targetId,facts.companyId).first<{
      company_id: string;
      user_id: string | null;
      version: number;
      branch_id: string | null;
      department_id: string | null;
      is_self: number;
    }>(),
readPeopleLifecycleCases1:(facts:{targetId:string;companyId:string;})=>db.prepare(`SELECT c.company_id,c.version,e.branch_id,e.department_id
      FROM people_lifecycle_cases c
      JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=c.company_id
      WHERE c.id=? AND c.company_id=?`).bind(facts.targetId,facts.companyId).first<{
      company_id: string;
      version: number;
      branch_id: string | null;
      department_id: string | null;
    }>(),
readPeoplePreparationTasks1:(facts:{targetId:string;companyId:string;})=>db.prepare(`SELECT t.company_id,t.version,t.status,t.assignee_id,t.completed_by,t.clearance_type,
        e.branch_id,e.department_id
      FROM people_preparation_tasks t
      JOIN people_lifecycle_cases c ON c.id=t.case_id AND c.company_id=t.company_id
      JOIN hr_engagements e ON e.id=c.engagement_id AND e.company_id=c.company_id
      WHERE t.id=? AND t.company_id=? AND t.clearance_type IS NOT NULL`).bind(facts.targetId,facts.companyId).first<{
      company_id: string;
      version: number;
      status: string;
      assignee_id: string | null;
      completed_by: string | null;
      clearance_type: import("@erpx/feature-people_payroll/lib/retirement").ClearanceKind;
      branch_id: string | null;
      department_id: string | null;
    }>(),
readHrPayrollRuns1:(facts:{targetId:string;companyId:string;})=>db.prepare("SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM hr_payroll_runs WHERE id=? AND company_id=?").bind(facts.targetId,facts.companyId).first<{ company_id: string; branch_id: null; department_id: null; version: number }>(),
readHrPayrollAdvances1:(facts:{userId:string;targetId:string;companyId:string;})=>db.prepare(`SELECT a.company_id,a.version,a.status,a.requested_by,e.branch_id,e.department_id,
        CASE WHEN iam_same_person(p.user_id,?,a.company_id) THEN 1 ELSE 0 END is_self
      FROM hr_payroll_advances a
      JOIN hr_engagements e ON e.id=a.engagement_id AND e.company_id=a.company_id
      JOIN hr_people p ON p.id=a.person_id AND p.company_id=a.company_id
      WHERE a.id=? AND a.company_id=?`).bind(facts.userId,facts.targetId,facts.companyId).first<{
      company_id: string;
      version: number;
      status: string;
      requested_by: string;
      branch_id: string | null;
      department_id: string | null;
      is_self: number;
    }>(),
readHrPayrollFinalPayPreparations1:(facts:{targetId:string;companyId:string;})=>db.prepare(`SELECT fp.company_id,fp.version,fp.status,t.assignee_id,t.completed_by,t.clearance_type,
        e.branch_id,e.department_id
      FROM hr_payroll_final_pay_preparations fp
      JOIN people_preparation_tasks t ON t.id=fp.task_id AND t.company_id=fp.company_id
      JOIN hr_engagements e ON e.id=fp.engagement_id AND e.company_id=fp.company_id
      WHERE fp.id=? AND fp.company_id=?`).bind(facts.targetId,facts.companyId).first<{
      company_id: string;
      version: number;
      status: string;
      assignee_id: string | null;
      completed_by: string | null;
      clearance_type: import("@erpx/feature-people_payroll/lib/retirement").ClearanceKind;
      branch_id: string | null;
      department_id: string | null;
    }>(),
readHrPeople2:(facts:{userId:string;targetId:string;companyId:string;})=>db.prepare(`SELECT p.company_id,p.version,NULL::text branch_id,NULL::text department_id,p.status,
        CASE WHEN EXISTS(SELECT 1 FROM hr_people h JOIN hr_engagements e ON e.person_id=h.id AND e.company_id=h.company_id WHERE h.company_id=p.company_id AND h.user_id=? AND h.status='active' AND h.merged_into_id IS NULL AND e.status='active') THEN 1 ELSE 0 END is_employee
      ${benefitsFinalSources.attachments_sensitive_facts_benefit_plans_p_from} WHERE p.id=? AND p.company_id=?`).bind(facts.userId,facts.targetId,facts.companyId).first<{
      company_id: string;
      version: number;
      branch_id: null;
      department_id: null;
      status: string;
      is_employee: number;
    }>(),versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>sensitiveAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>sensitiveAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
