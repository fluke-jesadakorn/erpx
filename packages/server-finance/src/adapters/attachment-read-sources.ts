import {serviceInitialPhysicalCountFinanceImages,posInitialPhysicalCountFinanceImages,manufacturingInitialPhysicalCountFinanceImages} from '@erpx/server-inventory/public/finance-record-sources';
import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {posFinanceRecordSourceImages,posFinanceRecordSourceReadLocks} from '@erpx/server-pos/public/finance-record-sources';
import {pmsFinanceRecordSourceImages,pmsFinanceRecordSourceReadLocks} from '@erpx/server-pms/public/finance-record-sources';
import {payrollFinanceRecordSourceImages,payrollFinanceRecordSourceReadLocks} from '@erpx/server-people_payroll/public/finance-record-sources';
import {financeSourceDocumentReadLeases} from '@erpx/server-finance/adapters/document-purchases';
/** Closed actual Finance File owners. Tax identity uses its separately
 * registered original approval/payment/certificate reader. */
export const financeAttachmentReadTables={
 finance_billing_correction:'finance_billing_corrections',finance_business_bill:'finance_business_billing_runs',finance_plan_edition:'finance_plan_editions',finance_book_rule:'finance_book_rules',finance_book_adjustment:'finance_book_adjustments',finance_bank_match_group:'finance_bank_match_groups',finance_collection_case:'finance_collection_cases',finance_business_contract:'finance_business_contracts',finance_recognition_run:'finance_recognition_runs',finance_instrument:'finance_instrument_editions',finance_instrument_run:'finance_instrument_runs',finance_asset_policy:'finance_asset_policy_editions',finance_asset_policy_run:'finance_asset_policy_runs',finance_group_report:'finance_group_report_editions',finance_ownership_interest:'finance_ownership_interests',finance_acquisition:'finance_acquisitions',finance_intercompany_pair:'finance_intercompany_document_pairs',finance_journal:'finance_journals',finance_bank_statement:'finance_bank_statements',finance_bank_transaction:'finance_bank_transactions',finance_period:'finance_periods',finance_accrual_schedule:'finance_accrual_schedules',finance_asset:'finance_assets',finance_lease:'finance_leases',finance_loan:'finance_loans',finance_revenue_contract:'finance_revenue_contracts',finance_payment:'finance_payments',finance_refund_obligation:'finance_refund_obligations',finance_supplier_batch:'finance_supplier_batches',
} as const;
export type FinanceAttachmentReadTarget=keyof typeof financeAttachmentReadTables;
export function isFinanceAttachmentReadTarget(target:string):target is FinanceAttachmentReadTarget{return Object.hasOwn(financeAttachmentReadTables,target);}
const relatedTables=['finance_invoices','finance_journal_lines','finance_book_postings','finance_allocations','finance_payment_reversals','finance_supplier_instructions','finance_bank_match_allocations','finance_close_checks','finance_close_reconciliations','finance_accrual_schedule_runs','finance_asset_depreciations','finance_asset_disposals','finance_revenue_obligations','finance_book_account_mappings','finance_group_entities','finance_intercompany_pair_claims','finance_refund_instructions','finance_collection_deliveries'];
export function financeAttachmentReadSourceLocks(database:SqlDatabase,companyId:string){return [
 ...financeSourceDocumentReadLeases(database,companyId),posFinanceRecordSourceReadLocks(database),pmsFinanceRecordSourceReadLocks(database),payrollFinanceRecordSourceReadLocks(database),
 database.prepare(`LOCK TABLE ${[...new Set([...Object.values(financeAttachmentReadTables),...relatedTables])].join(',')} IN SHARE ROW EXCLUSIVE MODE`),
];}

/** Closed Manufacturing journal readers only. A pool marker selects this lease
 * path, never authority. Repeat raw marker selection after all leases and at
 * final read; a changed selection fails instead of upgrading/falling back. */
export function manufacturingInitialStockJournalReadSourceLocks(database:SqlDatabase,companyId:string){return [
 database.prepare('LOCK TABLE company_package_grants IN SHARE ROW EXCLUSIVE MODE'),
 database.prepare('LOCK TABLE command_receipts,inventory_counts,inventory_count_lines,inventory_count_lot_lines,inventory_count_decisions,inventory_adjustment_moves IN SHARE ROW EXCLUSIVE MODE'),
 ...financeAttachmentReadSourceLocks(database,companyId),
];}

/** POS Count journals use the same receipt-before-Count writer-safe order. */
export function serviceInitialStockJournalReadSourceLocks(database:SqlDatabase,companyId:string){return [database.prepare('LOCK TABLE company_package_grants IN SHARE ROW EXCLUSIVE MODE'),database.prepare('LOCK TABLE command_receipts,inventory_counts,inventory_count_lines,inventory_count_lot_lines,inventory_count_decisions,inventory_adjustment_moves IN SHARE ROW EXCLUSIVE MODE'),...financeAttachmentReadSourceLocks(database,companyId)];}
export function posInitialStockJournalReadSourceLocks(database:SqlDatabase,companyId:string){return [database.prepare('LOCK TABLE company_package_grants IN SHARE ROW EXCLUSIVE MODE'),database.prepare('LOCK TABLE command_receipts,inventory_counts,inventory_count_lines,inventory_count_lot_lines,inventory_count_decisions,inventory_adjustment_moves IN SHARE ROW EXCLUSIVE MODE'),...financeAttachmentReadSourceLocks(database,companyId)];}

const rows=(table:string,predicate:string)=>`(SELECT jsonb_agg(to_jsonb(child) ORDER BY to_jsonb(child)::text) FROM ${table} child WHERE ${predicate})`;
const children:Partial<Record<FinanceAttachmentReadTarget,string>>={
 finance_refund_obligation:`jsonb_build_object('instructions',${rows('finance_refund_instructions','child.obligation_id=d.id AND child.company_id=d.company_id')})`,
 finance_collection_case:`jsonb_build_object('deliveries',${rows('finance_collection_deliveries','child.case_id=d.id AND child.company_id=d.company_id')})`,
 finance_acquisition:`jsonb_build_object('ownership',${rows('finance_ownership_interests','child.id=d.ownership_id AND child.company_id=d.company_id')})`,
 finance_journal:`jsonb_build_object('lines',${rows('finance_journal_lines','child.journal_id=d.id')},'books',${rows('finance_book_postings','child.primary_journal_id=d.id OR child.secondary_journal_id=d.id')})`,
 finance_payment:`jsonb_build_object('allocations',${rows('finance_allocations','child.payment_id=d.id')},'reversals',${rows('finance_payment_reversals','child.payment_id=d.id')})`,
 finance_bank_statement:`jsonb_build_object('transactions',${rows('finance_bank_transactions','child.statement_id=d.id')})`,
 finance_bank_transaction:`jsonb_build_object('matches',${rows('finance_bank_match_allocations','child.bank_transaction_id=d.id')})`,
 finance_bank_match_group:`jsonb_build_object('allocations',${rows('finance_bank_match_allocations','child.group_id=d.id')})`,
 finance_period:`jsonb_build_object('journals',${rows('finance_journals','child.period_id=d.id AND child.company_id=d.company_id')},'checks',${rows('finance_close_checks','child.period_id=d.id AND child.company_id=d.company_id')},'reconciliations',${rows('finance_close_reconciliations','child.period_id=d.id AND child.company_id=d.company_id')})`,
 finance_supplier_batch:`jsonb_build_object('instructions',${rows('finance_supplier_instructions','child.batch_id=d.id')})`,
 finance_asset:`jsonb_build_object('depreciation',${rows('finance_asset_depreciations','child.asset_id=d.id AND child.company_id=d.company_id')},'disposals',${rows('finance_asset_disposals','child.asset_id=d.id AND child.company_id=d.company_id')})`,
 finance_accrual_schedule:`jsonb_build_object('runs',${rows('finance_accrual_schedule_runs','child.schedule_id=d.id AND child.company_id=d.company_id')})`,
 finance_revenue_contract:`jsonb_build_object('obligations',${rows('finance_revenue_obligations','child.contract_id=d.id')})`,
 finance_business_contract:`jsonb_build_object('billing',${rows('finance_business_billing_runs','child.contract_id=d.id AND child.company_id=d.company_id')})`,
 finance_business_bill:`jsonb_build_object('recognitions',${rows('finance_recognition_runs','child.billing_run_id=d.id AND child.company_id=d.company_id')})`,
 finance_book_rule:`jsonb_build_object('accounts',${rows('finance_book_account_mappings','child.rule_id=d.id')})`,
 finance_instrument:`jsonb_build_object('runs',${rows('finance_instrument_runs','child.instrument_id=d.id AND child.company_id=d.company_id')})`,
 finance_asset_policy:`jsonb_build_object('runs',${rows('finance_asset_policy_runs','child.policy_id=d.id AND child.company_id=d.company_id')})`,
 finance_intercompany_pair:`jsonb_build_object('claims',${rows('finance_intercompany_pair_claims','child.pair_id=d.id')})`,
};
function foundationImage(target:FinanceAttachmentReadTarget){
 const kind=target==='finance_journal'?'journal':target==='finance_payment'?'payment':null;if(!kind)return "'{}'::jsonb";
 const roots=kind==='journal'?`SELECT d.id,d.company_id UNION SELECT primary_journal_id,d.company_id FROM finance_book_postings WHERE secondary_journal_id=d.id AND company_id=d.company_id`:'SELECT d.id,d.company_id';
 const owner=(sql:string)=>`(SELECT jsonb_agg(to_jsonb(native_source) ORDER BY to_jsonb(native_source)::text) FROM (${sql}) native_source JOIN (${roots}) root ON root.id=native_source.id AND root.company_id=native_source.company_id)`;
 const assets=kind==='journal'?`, 'assets',(SELECT jsonb_agg(to_jsonb(asset) ORDER BY asset.id) FROM finance_assets asset JOIN (${roots}) root ON root.id=asset.activation_journal_id AND root.company_id=asset.company_id),'assetDisposals',(SELECT jsonb_agg(to_jsonb(disposal) ORDER BY disposal.id) FROM finance_asset_disposals disposal JOIN (${roots}) root ON root.id=disposal.journal_id AND root.company_id=disposal.company_id)`:'';
 return `jsonb_build_object('pos',${owner(posFinanceRecordSourceImages[kind])},'hotel',${owner(pmsFinanceRecordSourceImages[kind])}${kind==='journal'?`,'payroll',${owner(payrollFinanceRecordSourceImages.journal)},'manufacturingInitialStock',${owner(manufacturingInitialPhysicalCountFinanceImages)},'posInitialStock',${owner(posInitialPhysicalCountFinanceImages)},'serviceInitialStock',${owner(serviceInitialPhysicalCountFinanceImages)}`:`,'posPosting',(SELECT jsonb_agg(to_jsonb(native_source) ORDER BY to_jsonb(native_source)::text) FROM (${posFinanceRecordSourceImages.journal}) native_source WHERE native_source.id=d.journal_id AND native_source.company_id=d.company_id),'hotelPosting',(SELECT jsonb_agg(to_jsonb(native_source) ORDER BY to_jsonb(native_source)::text) FROM (${pmsFinanceRecordSourceImages.journal}) native_source WHERE native_source.id=d.journal_id AND native_source.company_id=d.company_id)`}${assets})`;
}
/** Full native headers and child evidence stay server-only. Included source
 * membership is closed owner SQL; the original File purchase reader decides
 * which actual owner capability is valid again after whole deferred effects. */
export function financeAttachmentReadSourceImage(database:SqlDatabase,companyId:string,target:FinanceAttachmentReadTarget,id:string){
 const group=['finance_group_report','finance_ownership_interest','finance_acquisition','finance_intercompany_pair'].includes(target);
 return database.prepare(`SELECT jsonb_build_object('owner',to_jsonb(d),'children',${children[target]??"'{}'::jsonb"},'foundations',${foundationImage(target)}${group?`,'entities',${rows('finance_group_entities','child.holding_id=d.holding_id')}`:''})::text image FROM ${financeAttachmentReadTables[target]} d WHERE d.id=? AND d.company_id=?`).bind(id,companyId).first<string>('image');
}
