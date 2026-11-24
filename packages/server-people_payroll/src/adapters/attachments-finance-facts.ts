import "server-only";
import type {SqlDatabase,SqlStatement} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { nativePpStatement } from '@erpx/server-people_payroll/adapters/native-people-payroll-command';
import { ppRd26AttachmentTargetScopeSql } from './pp-read-tail-r26-sources';
const financeVersionKinds: Record<string, string> = {
  overtime_request: 'pp_attach_finance_overtime',
  time_entry: 'pp_attach_finance_time_entry',
  people_provision_operation: 'pp_attach_finance_provision',
};
import type {Row} from "@erpx/server-files/public/contracts/attachments-finance";
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
  const kind = financeVersionKinds[targetType];
  if (!kind) return null;
  // Table/column are fixed by this allow-list; the owner owns the SQL.
  return nativePpStatement(kind).bind(now, targetId, expectedVersion);
}
export function FinanceAttachmentPeoplePayrollFacts(db:SqlDatabase){return {
readOwner6:(facts:{companyId:string;targetId:string;targetType:string;purposeKey:string;})=>{return db.prepare(`SELECT t.company_id,e.branch_id,e.department_id,t.version,t.submitted_by,t.status
      FROM hr_time_entries t JOIN hr_engagements e ON e.id=t.engagement_id WHERE t.id=? AND t.company_id=?`).bind(facts.targetId,facts.companyId).first<Row>();},
readOwner11:(facts:{companyId:string;targetId:string;targetType:string;purposeKey:string;})=>{return db.prepare(ppRd26AttachmentTargetScopeSql('people_provision_operations')).bind(facts.targetId,facts.companyId).first<Row>();},versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,index:number)=>financeOperationsAttachmentTargetVersionGuard(db,commandId,targetType,targetId,companyId,version,index),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number)=>financeOperationsAttachmentRecordVersionStep(db,targetType,targetId,expectedVersion,now)
};}
