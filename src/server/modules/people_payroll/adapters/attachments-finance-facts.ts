import "server-only";
import type {SqlDatabase,SqlStatement} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
import type {Row} from "../../files/public/contracts/attachments-finance";
const versionDescriptors:Record<string,{table:string;column?:string;extra?:string}>={"overtime_request": {"table": "overtime_requests"}, "time_entry": {"table": "hr_time_entries"}, "people_provision_operation": {"table": "people_provision_operations"}};
const allowedTargets=new Set(["overtime_request", "time_entry", "people_provision_operation"]);
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
export function FinanceAttachmentPeoplePayrollFacts(db:SqlDatabase){return {
readOwner6:(facts:{companyId:string;targetId:string;targetType:string;purposeKey:string;})=>{return db.prepare(`SELECT t.company_id,e.branch_id,e.department_id,t.version,t.submitted_by,t.status
      FROM hr_time_entries t JOIN hr_engagements e ON e.id=t.engagement_id WHERE t.id=? AND t.company_id=?`).bind(facts.targetId,facts.companyId).first<Row>();},
readOwner11:(facts:{companyId:string;targetId:string;targetType:string;purposeKey:string;})=>{return db.prepare("SELECT company_id,NULL::text branch_id,NULL::text department_id,version FROM people_provision_operations WHERE id=? AND company_id=?").bind(facts.targetId,facts.companyId).first<Row>();},versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>financeOperationsAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>financeOperationsAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
