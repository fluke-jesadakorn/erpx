import "server-only";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {nativeTalentReadFirst} from "./native-talent-read";import {existsVersionRow} from "./talent-read-tail-r27-sources";
const sensitiveTables:Record<string,string>={talent_candidate:"talent_candidates",talent_offer:"talent_offers"};
// A document bind advances the Offer version. Keep the immutable accepted
// source exact until its original HR delivery has actually been consumed.
const offerFilesMutable=(alias:string)=>`(${alias}.status<>'accepted' OR EXISTS(SELECT 1 FROM talent_hire_handoffs handoff JOIN talent_hire_consumption_receipts receipt ON receipt.handoff_id=handoff.id AND receipt.company_id=handoff.company_id AND receipt.source_offer_version=handoff.source_offer_version WHERE handoff.offer_id=${alias}.id AND handoff.company_id=${alias}.company_id))`;
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
    existsVersionRow(table,targetType==="talent_offer"?` AND ${offerFilesMutable(table)}`:""),
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
readTalentApplications1:(facts:{userId:string;userId2:string;userId3:string;targetId:string;companyId:string;})=>nativeTalentReadFirst<{
      company_id: string;
      version: number;
      owner_branch_id: string | null;
      owner_department_id: string | null;
      is_owner: number;
    }>('talent_rd_sensitive_applications',[facts.userId,facts.userId2,facts.userId3,facts.targetId,facts.companyId],db),
readTalentOffers1:(facts:{targetId:string;companyId:string;})=>nativeTalentReadFirst<{
      company_id: string;
      version: number;
      branch_id: string | null;
      department_id: string | null;
      owner_id: string;
      pending_hire_delivery: number;
    }>('talent_rd_sensitive_offer',[facts.targetId,facts.companyId],db),versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>sensitiveAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>sensitiveAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
