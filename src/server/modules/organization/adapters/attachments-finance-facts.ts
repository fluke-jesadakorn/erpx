import "server-only";
import type {SqlDatabase,SqlStatement} from "../../runtime/public/runtime/types";
import {assertion} from "../../workflow/public/commands";
import type {Row} from "../../files/public/contracts/attachments-finance";
const versionDescriptors:Record<string,{table:string;column?:string;extra?:string}>={"organization_change": {"table": "organization_node_changes"}};
const allowedTargets=new Set(["organization_change", "organization_node"]);
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
  if (targetType === "organization_node")
    return assertion(db, commandId, `attachment-owner-${index}`, "EXISTS(SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND hierarchy_revision=?)", [targetId, companyId, version]);
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
  if (targetType === "organization_node")
    return db.prepare("UPDATE organization_nodes SET hierarchy_revision=hierarchy_revision+1 WHERE id=? AND hierarchy_revision=?").bind(targetId, expectedVersion);
  const item = versionDescriptors[targetType];
  if (!item) return null;
  const column = item.column ?? "version";
  const hasUpdatedAt = !["finance_bank_statements", "finance_periods", "finance_assets", "finance_plan_editions", "finance_book_rules", "finance_business_billing_runs", "iam_reviews", "iam_migrations", "company_provisioning"].includes(item.table);
  return db.prepare(`UPDATE ${item.table} SET ${column}=${column}+1${hasUpdatedAt ? ",updated_at=?" : ""} WHERE id=? AND ${column}=?${item.extra ? ` AND ${item.extra}` : ""}`)
    .bind(...(hasUpdatedAt ? [now, targetId, expectedVersion] : [targetId, expectedVersion]));
}
export function FinanceAttachmentOrganizationFacts(db:SqlDatabase){return {
readOwner7:(facts:{companyId:string;targetId:string;targetType:string;purposeKey:string;})=>{return db.prepare(`SELECT company_id,CASE WHEN kind='branch' THEN id ELSE branch_id END branch_id,NULL::text department_id,hierarchy_revision AS version,kind
      FROM organization_nodes WHERE id=? AND company_id=? AND kind IN ('company','headquarters','branch','department')`).bind(facts.targetId,facts.companyId).first<Row>();},
readOwner8:(facts:{companyId:string;targetId:string;targetType:string;purposeKey:string;})=>{return db.prepare("SELECT company_id,NULL::text branch_id,NULL::text department_id,version,requested_by,state FROM organization_node_changes WHERE id=? AND company_id=?").bind(facts.targetId,facts.companyId).first<Row>();},versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>financeOperationsAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>financeOperationsAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
