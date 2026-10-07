import "server-only";
import type {SqlDatabase,SqlStatement} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
import type {TargetRow} from "../../files/public/contracts/attachments-operational";
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
export function OperationalAttachmentAssetsFacts(db:SqlDatabase){return {
readModuleScoped:(facts:{targetType:string;targetId:string;companyId:string;})=>{const moduleTarget=({"asset": {"table": "operational_assets", "branch": "branch_id", "department": "department_id"}} as Record<string,{table:string;branch:string;department:string|null}>)[facts.targetType];return db.prepare(`SELECT company_id,${moduleTarget.branch} branch_id,${moduleTarget.department ?? "NULL::text"} department_id,version FROM ${moduleTarget.table} WHERE id=? AND company_id=?`).bind(facts.targetId,facts.companyId).first<TargetRow>();},versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>remainingOperationalAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>remainingOperationalAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
