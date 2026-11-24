import {benefitDependentParticipants,benefitElectionParticipants} from "@erpx/server-people_payroll/public/benefits-owner-facts";
import {benefitOwnerExistsGuard} from "@erpx/server-benefits/adapters/benefits-read-sql";
import "server-only";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
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
    benefitOwnerExistsGuard(table, `company_id=? AND version=?`),
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
/** Byte-identical owner-local sensitive-fact read projection shared by the
 * dependent/election pair; generated SQL is unchanged. */
export const sensitiveFactSelect=(alias:string,table:string,participants:()=>string)=>`SELECT ${alias}.company_id,${alias}.version,${alias}.status,e.branch_id,e.department_id,
        CASE WHEN iam_same_person(p.user_id,?,${alias}.company_id) THEN 1 ELSE 0 END is_owner
      FROM ${table} ${alias}
      ${participants()}
      WHERE ${alias}.id=? AND ${alias}.company_id=?`;
export function SensitiveAttachmentBenefitsFacts(db:SqlDatabase){return {
readBenefitDependents1:(facts:{userId:string;targetId:string;companyId:string;})=>db.prepare(sensitiveFactSelect('d','benefit_dependents',benefitDependentParticipants)).bind(facts.userId,facts.targetId,facts.companyId).first<{
      company_id: string;
      version: number;
      status: string;
      branch_id: string | null;
      department_id: string | null;
      is_owner: number;
    }>(),
readBenefitElections1:(facts:{userId:string;targetId:string;companyId:string;})=>db.prepare(sensitiveFactSelect('x','benefit_elections',benefitElectionParticipants)).bind(facts.userId,facts.targetId,facts.companyId).first<{
      company_id: string;
      version: number;
      status: string;
      branch_id: string | null;
      department_id: string | null;
      is_owner: number;
    }>(),versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>sensitiveAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>sensitiveAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
