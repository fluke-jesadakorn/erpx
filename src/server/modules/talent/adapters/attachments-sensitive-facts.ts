import "server-only";
import type {SqlDatabase,SqlStatement} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
const sensitiveTables:Record<string,string>={talent_candidate:"talent_candidates",talent_offer:"talent_offers"};
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
export function SensitiveAttachmentTalentFacts(db:SqlDatabase){return {
readTalentApplications1:(facts:{userId:string;userId2:string;userId3:string;targetId:string;companyId:string;})=>db.prepare(`SELECT c.company_id,c.version,
        min(r.branch_id) FILTER (WHERE a.owner_id=?) owner_branch_id,
        min(r.department_id) FILTER (WHERE a.owner_id=?) owner_department_id,
        CASE WHEN EXISTS(SELECT 1 FROM talent_applications owned WHERE owned.candidate_id=c.id AND owned.company_id=c.company_id AND owned.owner_id=?) THEN 1 ELSE 0 END is_owner
      FROM talent_candidates c
      LEFT JOIN talent_applications a ON a.candidate_id=c.id AND a.company_id=c.company_id
      LEFT JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id
      WHERE c.id=? AND c.company_id=?
      GROUP BY c.id,c.company_id,c.version`).bind(facts.userId,facts.userId2,facts.userId3,facts.targetId,facts.companyId).first<{
      company_id: string;
      version: number;
      owner_branch_id: string | null;
      owner_department_id: string | null;
      is_owner: number;
    }>(),
readTalentOffers1:(facts:{targetId:string;companyId:string;})=>db.prepare(`SELECT o.company_id,o.version,r.branch_id,r.department_id,a.owner_id
      FROM talent_offers o
      JOIN talent_applications a ON a.id=o.application_id AND a.company_id=o.company_id
      JOIN talent_requisitions r ON r.id=a.requisition_id AND r.company_id=a.company_id
      WHERE o.id=? AND o.company_id=?`).bind(facts.targetId,facts.companyId).first<{
      company_id: string;
      version: number;
      branch_id: string | null;
      department_id: string | null;
      owner_id: string;
    }>(),versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>sensitiveAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>sensitiveAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
