import "server-only";
import type {SqlDatabase,SqlStatement} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
import type {Row} from "../../files/public/contracts/attachments-finance";
const versionDescriptors:Record<string,{table:string;column?:string;extra?:string}>={"pos_sale": {"table": "pos_sales"}, "pos_refund": {"table": "pos_refunds"}, "pos_shift": {"table": "pos_shifts"}};
const allowedTargets=new Set(["pos_sale", "pos_refund", "pos_shift"]);
function financeOperationsAttachmentTargetVersionGuard(
  db: SqlDatabase,
  commandId: string,
  targetType: string,
  targetId: string,
  companyId: string,
  version: number,
  index: number,
): SqlStatement | null {
if(!allowedTargets.has(targetType))return null;
  const item = versionDescriptors[targetType];
  if (!item) return null;
  const column = item.column ?? "version";
  return assertion(db, commandId, `attachment-owner-${index}`, `EXISTS(SELECT 1 FROM ${item.table} WHERE id=? AND company_id=? AND ${column}=?${item.extra ? ` AND ${item.extra}` : ""})`, [targetId, companyId, version]);
}
function financeOperationsAttachmentRecordVersionStep(
  db: SqlDatabase,
  targetType: string,
  targetId: string,
  expectedVersion: number,
  now: number,
): SqlStatement | null {
if(!allowedTargets.has(targetType))return null;
  const item = versionDescriptors[targetType];
  if (!item) return null;
  const column = item.column ?? "version";
  return db.prepare(`UPDATE ${item.table} SET ${column}=${column}+1,updated_at=? WHERE id=? AND ${column}=?${item.extra ? ` AND ${item.extra}` : ""}`)
    .bind(now, targetId, expectedVersion);
}
export function FinanceAttachmentPosFacts(db:SqlDatabase){return {
readOwner2:(facts:{companyId:string;targetId:string;targetType:string;purposeKey:string;})=>{const table=({"pos_sale": "pos_sales", "pos_refund": "pos_refunds", "pos_shift": "pos_shifts"} as Record<string,string>)[facts.targetType];return db.prepare(`SELECT company_id,branch_id,NULL::text department_id,version FROM ${table} WHERE id=? AND company_id=?`).bind(facts.targetId,facts.companyId).first<Row>();},
readOwner3:(facts:{companyId:string;targetId:string;targetType:string;purposeKey:string;})=>{const table=({"pos_sale": "pos_sales", "pos_refund": "pos_refunds", "pos_shift": "pos_shifts"} as Record<string,string>)[facts.targetType];return db.prepare(`SELECT company_id,branch_id,NULL::text department_id,version FROM ${table} WHERE id=? AND company_id=?`).bind(facts.targetId,facts.companyId).first<Row>();},versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>financeOperationsAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>financeOperationsAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
