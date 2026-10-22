import "server-only";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import type {Row} from "@erpx/server-files/public/contracts/attachments-finance";
const versionDescriptors:Record<string,{table:string;column?:string;extra?:string}>={"iam_review": {"table": "iam_reviews"}, "iam_migration": {"table": "iam_migrations"}};
const allowedTargets=new Set(["iam_review", "iam_migration"]);
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
  const hasUpdatedAt = !["finance_bank_statements", "finance_periods", "finance_assets", "finance_plan_editions", "finance_book_rules", "finance_business_billing_runs", "iam_reviews", "iam_migrations", "company_provisioning"].includes(item.table);
  return db.prepare(`UPDATE ${item.table} SET ${column}=${column}+1${hasUpdatedAt ? ",updated_at=?" : ""} WHERE id=? AND ${column}=?${item.extra ? ` AND ${item.extra}` : ""}`)
    .bind(...(hasUpdatedAt ? [now, targetId, expectedVersion] : [targetId, expectedVersion]));
}
export function FinanceAttachmentIdentityAccessFacts(db:SqlDatabase){return {
readOwner9:(facts:{companyId:string;targetId:string;targetType:string;purposeKey:string;})=>{const table=({"iam_review": "iam_reviews", "iam_migration": "iam_migrations"} as Record<string,string>)[facts.targetType];return db.prepare(`SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM ${table} WHERE id=? AND company_id=?`).bind(facts.targetId,facts.companyId).first<Row>();},versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>financeOperationsAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>financeOperationsAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
