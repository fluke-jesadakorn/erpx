import {childBillingRunExistsSql} from './finance-read-tail-r43-sql';
import "server-only";
import type {SqlDatabase,SqlStatement} from "@erpx/platform-runtime-server/public/runtime/types";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import type {Row} from "@erpx/files-server/public/contracts/attachments-finance";
import {nativeFinanceAttachmentVersionGuard,nativeFinanceAttachmentVersionStep} from "./native-finance-owner-write";
const allowedTargets=new Set(["finance_billing_correction", "finance_business_bill", "finance_plan_edition", "finance_book_rule", "finance_book_adjustment", "finance_bank_match_group", "finance_collection_case", "finance_business_contract", "finance_recognition_run", "finance_instrument", "finance_instrument_run", "finance_asset_policy", "finance_asset_policy_run", "finance_group_report", "finance_ownership_interest", "finance_acquisition", "finance_intercompany_pair", "finance_journal", "finance_bank_statement", "finance_bank_transaction", "finance_period", "finance_accrual_schedule", "finance_asset", "finance_tax_identity", "finance_lease", "finance_loan", "finance_revenue_contract", "finance_payment", "finance_refund_obligation", "finance_supplier_batch"]);
const financeRecords:Record<string,{table:string;version:string;write:string}>={"finance_billing_correction": {"table": "finance_billing_corrections", "version": "version", "write": "finance.write"}, "finance_business_bill": {"table": "finance_business_billing_runs", "version": "version", "write": "finance.write"}, "finance_plan_edition": {"table": "finance_plan_editions", "version": "version", "write": "finance.write"}, "finance_book_rule": {"table": "finance_book_rules", "version": "version", "write": "finance.write"}, "finance_book_adjustment": {"table": "finance_book_adjustments", "version": "version", "write": "finance.write"}, "finance_bank_match_group": {"table": "finance_bank_match_groups", "version": "version", "write": "finance.write"}, "finance_collection_case": {"table": "finance_collection_cases", "version": "version", "write": "finance.write"}, "finance_business_contract": {"table": "finance_business_contracts", "version": "version", "write": "finance.write"}, "finance_recognition_run": {"table": "finance_recognition_runs", "version": "version", "write": "finance.write"}, "finance_instrument": {"table": "finance_instrument_editions", "version": "version", "write": "finance.write"}, "finance_instrument_run": {"table": "finance_instrument_runs", "version": "version", "write": "finance.write"}, "finance_asset_policy": {"table": "finance_asset_policy_editions", "version": "version", "write": "finance.write"}, "finance_asset_policy_run": {"table": "finance_asset_policy_runs", "version": "version", "write": "finance.write"}, "finance_group_report": {"table": "finance_group_report_editions", "version": "version", "write": "finance.write"}, "finance_ownership_interest": {"table": "finance_ownership_interests", "version": "version", "write": "finance.write"}, "finance_acquisition": {"table": "finance_acquisitions", "version": "version", "write": "finance.write"}, "finance_intercompany_pair": {"table": "finance_intercompany_document_pairs", "version": "version", "write": "finance.write"}, "finance_journal": {"table": "finance_journals", "version": "version", "write": "finance.write"}, "finance_bank_statement": {"table": "finance_bank_statements", "version": "version", "write": "finance.write"}, "finance_bank_transaction": {"table": "finance_bank_transactions", "version": "version", "write": "finance.write"}, "finance_period": {"table": "finance_periods", "version": "close_version", "write": "finance.write"}, "finance_accrual_schedule": {"table": "finance_accrual_schedules", "version": "version", "write": "finance.write"}, "finance_asset": {"table": "finance_assets", "version": "version", "write": "finance.write"}, "finance_tax_identity": {"table": "finance_tax_identities", "version": "version", "write": "finance.write"}, "finance_lease": {"table": "finance_leases", "version": "version", "write": "finance.write"}, "finance_loan": {"table": "finance_loans", "version": "version", "write": "finance.write"}, "finance_revenue_contract": {"table": "finance_revenue_contracts", "version": "version", "write": "finance.write"}, "finance_payment": {"table": "finance_payments", "version": "version", "write": "finance.pay"}, "finance_refund_obligation": {"table": "finance_refund_obligations", "version": "version", "write": "finance.pay"}, "finance_supplier_batch": {"table": "finance_supplier_batches", "version": "version", "write": "finance.pay"}};
type StepContext = { actor: WorkContext; commandId: string };
function financeOperationsAttachmentTargetVersionGuard(
  commandId: string,
  targetType: string,
  targetId: string,
  companyId: string,
  version: number,
  actor?: WorkContext,
): SqlStatement | null {
  // Routing discriminator only. The native owner owns the target allow-list,
  // the version check and the Company scope; no assertion SQL is authored here.
  if (!allowedTargets.has(targetType)) return null;
  if (!commandId || !actor) throw new Error("The native Finance attachment guard requires an authorized actor.");
  return nativeFinanceAttachmentVersionGuard({
    companyId,
    targetType,
    targetId,
    expectedVersion: version,
    userId: actor.userId,
    assignmentId: actor.assignmentId,
    role: actor.role,
    scopeKind: actor.assignmentScope,
  });
}
function financeOperationsAttachmentRecordVersionStep(
  targetType: string,
  targetId: string,
  expectedVersion: number,
  now: number,
  context?: StepContext,
): SqlStatement | null {
  // The single version step executes on the mounted native Finance port with
  // receipt, audit and outbox in the caller unit. The reply is an inert
  // sequencing descriptor; no UPDATE is authored in TypeScript.
  if (!allowedTargets.has(targetType)) return null;
  if (!context) throw new Error("The native Finance attachment version step requires an authorized actor.");
  return nativeFinanceAttachmentVersionStep({
    companyId: context.actor.companyId,
    targetType,
    targetId,
    expectedVersion,
    now,
    userId: context.actor.userId,
    assignmentId: context.actor.assignmentId,
    role: context.actor.role,
    scopeKind: context.actor.assignmentScope,
    key: context.commandId,
  });
}
export function FinanceAttachmentFinanceFacts(db:SqlDatabase){return {
readOwner1:(facts:{companyId:string;targetId:string;targetType:string;purposeKey:string;})=>{const finance=financeRecords[facts.targetType],policy={targetType:facts.targetType,purposeKey:facts.purposeKey};return db.prepare(`SELECT company_id,NULL::text branch_id,NULL::text department_id,${finance.version} AS version${policy.targetType==="finance_tax_identity"?",party_id,status":policy.purposeKey==="finance_support"?(policy.targetType==="finance_business_bill"?",CASE WHEN EXISTS(${childBillingRunExistsSql('finance_business_billing_runs.id')}) THEN 'superseded' ELSE no_charge_status END AS status":",status"):""} FROM ${finance.table} WHERE id=? AND company_id=?`).bind(facts.targetId,facts.companyId).first<Row>();},versionGuard:(commandId:string,targetType:string,targetId:string,companyId:string,version:number,_index:number,actor?:WorkContext)=>financeOperationsAttachmentTargetVersionGuard(commandId,targetType,targetId,companyId,version,actor),versionStep:(targetType:string,targetId:string,expectedVersion:number,now:number,context?:StepContext)=>financeOperationsAttachmentRecordVersionStep(targetType,targetId,expectedVersion,now,context)
};}
