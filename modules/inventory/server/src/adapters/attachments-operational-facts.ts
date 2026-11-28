import "server-only";
import type {SqlDatabase,SqlStatement} from "@erpx/platform-runtime-server/public/runtime/types";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import type {TargetRow} from "@erpx/files-server/public/contracts/attachments-operational";
import {nativeInventoryReadFirst,INVENTORY_RD_ATTACHMENT_OPERATIONAL_TRANSFER} from './native-inventory-read';
import {attachmentOwnerVersionGuardSql} from './read-tail-sql';
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
export function OperationalAttachmentInventoryFacts(db:SqlDatabase){return {
/** Owner-native read: planned by `RD_ATTACHMENT_OPERATIONAL_TRANSFER` and
 * executed on the caller's unit; this module carries no SQL for it. */
readInventoryTransfers1:(facts:{targetId:string;companyId:string;})=>nativeInventoryReadFirst<TargetRow & {
        source_branch_id: string;
        target_branch_id: string;
        status: string;
        created_by: string;
        prepared_by: string;
        delivery_mode: string;
      }>(INVENTORY_RD_ATTACHMENT_OPERATIONAL_TRANSFER,[facts.targetId,facts.companyId],db),versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>remainingOperationalAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>remainingOperationalAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
