import "server-only";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import type {TargetRow} from "@erpx/server-files/public/contracts/attachments-operational";
const versionOwners:Record<string,string>={};
const allowedTargets=new Set<string>([]);
function remainingOperationalAttachmentTargetVersionGuard(
  db: SqlDatabase,
  commandId: string,
  targetType: string,
  targetId: string,
  companyId: string,
  version: number,
  index: number,
): SqlStatement | null {
if(!allowedTargets.has(targetType))return null;
  const table = versionOwners[targetType];
  if (!table) return null;
  return assertion(
    db,
    commandId,
    `attachment-owner-${index}`,
    `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?)`,
    [targetId, companyId, version],
  );
}
function remainingOperationalAttachmentRecordVersionStep(
  db: SqlDatabase,
  targetType: string,
  targetId: string,
  expectedVersion: number,
  now: number,
): SqlStatement | null {
if(!allowedTargets.has(targetType))return null;
  const table = versionOwners[targetType];
  if (!table) return null;
  return db
    .prepare(
      `UPDATE ${table} SET version=version+1,updated_at=? WHERE id=? AND version=?`,
    )
    .bind(now, targetId, expectedVersion);
}
export function OperationalAttachmentCommerceFacts(db:SqlDatabase){return {
readCommerceOrders1:(facts:{targetId:string;companyId:string;})=>db.prepare("SELECT company_id,branch_id,department_id,version,state FROM commerce_orders WHERE id=? AND company_id=? AND kind='purchase'").bind(facts.targetId,facts.companyId).first<TargetRow & { state: string }>(),
readCommerceOrders2:(facts:{targetId:string;companyId:string;})=>db.prepare("SELECT company_id,branch_id,department_id,version FROM commerce_orders WHERE id=? AND company_id=? AND kind IN ('quote','sales')").bind(facts.targetId,facts.companyId).first<TargetRow>(),versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>remainingOperationalAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>remainingOperationalAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
