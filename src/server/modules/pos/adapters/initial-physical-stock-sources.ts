import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
/** Original native POS settings, register and command evidence only. Receipt
 * result equality is joined with its actual operation/actor audit; a receipt
 * result alone does not identify an accounting operation. */
export const posInitialPhysicalStockAccountingRows=`SELECT settings.company_id,register.warehouse_id,register.branch_id,settings.currency,settings.accounts_json,
 bootstrap.id bootstrap_receipt_id,bootstrap.request_hash bootstrap_request_hash,bootstrap_audit.id bootstrap_audit_id,
 approval.id approval_receipt_id,approval.request_hash approval_request_hash,approval_audit.id approval_audit_id,
 encode(sha256(convert_to(jsonb_build_object('currency',settings.currency,'accounts',settings.accounts_json::jsonb,'preparedBy',settings.accounting_prepared_by,'approvedBy',settings.accounting_approved_by,'register',to_jsonb(register))::text,'UTF8')),'hex') accounting_image_hash,
 jsonb_build_object('settings',to_jsonb(settings),'register',to_jsonb(register),'bootstrap',to_jsonb(bootstrap),'bootstrapAudit',to_jsonb(bootstrap_audit),'approval',to_jsonb(approval),'approvalAudit',to_jsonb(approval_audit))::text accounting_image
 FROM pos_settings settings JOIN pos_registers register ON register.company_id=settings.company_id AND register.active
 JOIN pos_audit bootstrap_audit ON bootstrap_audit.company_id=settings.company_id AND bootstrap_audit.record_id=settings.company_id AND bootstrap_audit.operation='settings.bootstrap' AND bootstrap_audit.branch_id IS NULL AND bootstrap_audit.actor_id=settings.accounting_prepared_by
 JOIN pos_receipts bootstrap ON bootstrap.company_id=settings.company_id AND bootstrap.result_json::jsonb=bootstrap_audit.details_json::jsonb->'result' AND bootstrap.result_json::jsonb->>'id'=settings.company_id AND bootstrap.result_json::jsonb->>'registerId'=register.id AND bootstrap.result_json::jsonb->>'warehouseId'=register.warehouse_id
 JOIN pos_audit approval_audit ON approval_audit.company_id=settings.company_id AND approval_audit.record_id=settings.company_id AND approval_audit.operation='accounting.approve' AND approval_audit.branch_id IS NULL AND approval_audit.actor_id=settings.accounting_approved_by AND approval_audit.actor_id<>bootstrap_audit.actor_id
 JOIN pos_receipts approval ON approval.company_id=settings.company_id AND approval.result_json::jsonb=approval_audit.details_json::jsonb->'result' AND approval.result_json::jsonb->>'id'=settings.company_id AND approval.result_json::jsonb->>'state'='approved'
 WHERE settings.accounting_prepared_by IS NOT NULL AND settings.accounting_approved_by IS NOT NULL`;
export function posInitialPhysicalStockAccountingLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE pos_settings,pos_registers,pos_receipts,pos_audit IN SHARE MODE')];}
