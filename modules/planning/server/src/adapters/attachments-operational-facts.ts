import "server-only";
import type {SqlDatabase,SqlStatement} from "@erpx/platform-runtime-server/public/runtime/types";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import type {TargetRow} from "@erpx/files-server/public/contracts/attachments-operational";
import {planningAttachmentTargetRow,planningAttachmentTargetVersionExists} from "./planning-attachment-target-sql";
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
    planningAttachmentTargetVersionExists(table),
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
export function OperationalAttachmentPlanningFacts(db:SqlDatabase){return {
readModuleScoped:(facts:{targetType:string;targetId:string;companyId:string;})=>{const moduleTarget=({"planning_scenario": {"table": "planning_scenarios", "branch": "branch_id", "department": null}} as Record<string,{table:string;branch:string;department:string|null}>)[facts.targetType];return db.prepare(planningAttachmentTargetRow(`${moduleTarget.branch} branch_id,${moduleTarget.department ?? "NULL::text"} department_id,version`,moduleTarget.table)).bind(facts.targetId,facts.companyId).first<TargetRow>();},versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>remainingOperationalAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>remainingOperationalAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
