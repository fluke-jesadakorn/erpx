import "server-only";
import type {SqlDatabase,SqlStatement} from "@erpx/platform-runtime-server/public/runtime/types";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import {attachmentOwnerVersionGuardSql} from "./attachment-owner-version-guard";
import {requireAttachmentDescriptor} from './native-lane-36/client';
import type {TargetRow} from "@erpx/files-server/public/contracts/attachments-operational";
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
    attachmentOwnerVersionGuardSql(table),
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
export function OperationalAttachmentMaintenanceFacts(db:SqlDatabase){return {
readModuleScoped:(facts:{targetType:string;targetId:string;companyId:string;}):Promise<TargetRow | null>=>{
  if (facts.targetType !== 'maintenance_work') return Promise.resolve(null);
  return requireAttachmentDescriptor(facts.targetType,facts.targetId,facts.companyId) as Promise<TargetRow | null>;
},versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>remainingOperationalAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>remainingOperationalAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
