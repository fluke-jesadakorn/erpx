import "server-only";
import type {SqlDatabase,SqlStatement} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
const sensitiveTables:Record<string,string>={learning_course:"learning_courses",learning_assignment:"learning_assignments"};
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
export function SensitiveAttachmentLearningFacts(db:SqlDatabase){return {
readLearningAssignments1:(facts:{userId:string;targetId:string;companyId:string;})=>db.prepare(`SELECT c.company_id,c.version,NULL::text branch_id,NULL::text department_id,c.status,
        CASE WHEN EXISTS(SELECT 1 FROM learning_assignments a WHERE a.course_id=c.id AND a.company_id=c.company_id AND a.learner_id=? AND a.status!='cancelled') THEN 1 ELSE 0 END is_learner
      FROM learning_courses c WHERE c.id=? AND c.company_id=?`).bind(facts.userId,facts.targetId,facts.companyId).first<{
      company_id: string;
      version: number;
      branch_id: null;
      department_id: null;
      status: string;
      is_learner: number;
    }>(),
readLearningAssignments2:(facts:{userId:string;targetId:string;companyId:string;})=>db.prepare(`SELECT a.company_id,a.version,a.branch_id,a.department_id,a.status,a.learner_id,
        CASE WHEN iam_same_person(a.learner_id,?,a.company_id) THEN 1 ELSE 0 END is_learner
      FROM learning_assignments a WHERE a.id=? AND a.company_id=?`).bind(facts.userId,facts.targetId,facts.companyId).first<{
      company_id: string;
      version: number;
      branch_id: string | null;
      department_id: string | null;
      status: string;
      learner_id: string;
      is_learner: number;
    }>(),versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>sensitiveAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>sensitiveAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
