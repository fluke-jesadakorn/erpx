import {benefitOwnerExistsGuard} from "@erpx/server-benefits/adapters/benefits-read-sql";
import {nativeBenefitReadFirst,RD_ATTACHMENT_BENEFIT_DEPENDENTS,RD_ATTACHMENT_BENEFIT_ELECTIONS} from "@erpx/server-benefits/adapters/native-benefit-read";
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

type SensitiveFact={company_id:string;version:number;status:string;branch_id:string|null;department_id:string|null;is_owner:number};

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
/** The dependent/election point reads are owned by the native Benefits read
 * plan registry; no TypeScript SQL remains for the pair. */
export function SensitiveAttachmentBenefitsFacts(db:SqlDatabase){return {
readBenefitDependents1:(facts:{userId:string;targetId:string;companyId:string;})=>nativeBenefitReadFirst<SensitiveFact>(RD_ATTACHMENT_BENEFIT_DEPENDENTS,[facts.userId,facts.targetId,facts.companyId],db),
readBenefitElections1:(facts:{userId:string;targetId:string;companyId:string;})=>nativeBenefitReadFirst<SensitiveFact>(RD_ATTACHMENT_BENEFIT_ELECTIONS,[facts.userId,facts.targetId,facts.companyId],db),versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>sensitiveAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>sensitiveAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
