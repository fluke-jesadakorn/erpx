import {benefitDependentParticipants,benefitElectionParticipants} from "../../people_payroll/public/benefits-owner-facts";
import "server-only";
import type {SqlDatabase,SqlStatement} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
const sensitiveTables:Record<string,string>={benefit_plan:"benefit_plans",benefit_dependent:"benefit_dependents",benefit_election:"benefit_elections"};
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
export function SensitiveAttachmentBenefitsFacts(db:SqlDatabase){return {
readBenefitDependents1:(facts:{userId:string;targetId:string;companyId:string;})=>db.prepare(`SELECT d.company_id,d.version,d.status,e.branch_id,e.department_id,
        CASE WHEN iam_same_person(p.user_id,?,d.company_id) THEN 1 ELSE 0 END is_owner
      FROM benefit_dependents d
      ${benefitDependentParticipants()}
      WHERE d.id=? AND d.company_id=?`).bind(facts.userId,facts.targetId,facts.companyId).first<{
      company_id: string;
      version: number;
      status: string;
      branch_id: string | null;
      department_id: string | null;
      is_owner: number;
    }>(),
readBenefitElections1:(facts:{userId:string;targetId:string;companyId:string;})=>db.prepare(`SELECT x.company_id,x.version,x.status,e.branch_id,e.department_id,
        CASE WHEN iam_same_person(p.user_id,?,x.company_id) THEN 1 ELSE 0 END is_owner
      FROM benefit_elections x
      ${benefitElectionParticipants()}
      WHERE x.id=? AND x.company_id=?`).bind(facts.userId,facts.targetId,facts.companyId).first<{
      company_id: string;
      version: number;
      status: string;
      branch_id: string | null;
      department_id: string | null;
      is_owner: number;
    }>(),versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>sensitiveAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>sensitiveAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
