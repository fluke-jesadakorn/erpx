import "server-only";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import type {TargetRow} from "@erpx/server-files/public/contracts/attachments-operational";
import {nativeProcurementReadFirst,PROCUREMENT_RD_OPERATIONAL_ATTACHMENT_BILL_MATCH} from "./native-procurement-read";
/** Owner-native read: the statement is planned by the Procurement native read
 * port (`RD_OPERATIONAL_ATTACHMENT_BILL_MATCH`) and executed on the caller's
 * open unit. This module carries no SQL for this read. */
const versionOwners:Record<string,string>={"supplier_bill_match": "procurement_bill_matches"};
const allowedTargets=new Set<string>(["supplier_bill_match"]);
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
export function OperationalAttachmentProcurementFacts(db:SqlDatabase){return {
readProcurementBillMatches1:(facts:{targetId:string;companyId:string;})=>nativeProcurementReadFirst<TargetRow>(PROCUREMENT_RD_OPERATIONAL_ATTACHMENT_BILL_MATCH,[facts.targetId,facts.companyId],db),versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>remainingOperationalAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>remainingOperationalAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
