import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import {salesGoodsPostingFulfillmentSources} from '@erpx/server-commerce/public/sales-goods-posting-setup';
import {approvedSalesPostingDefaults,approvedSalesPostingDefaultMatches,approvedSalesPostingEffectiveService} from '@erpx/server-finance/adapters/approved-sales-posting-defaults';
import {commercePostingFinanceReferences,validateCommercePostingFinanceReferences} from '@erpx/server-finance/adapters/commerce-posting-setup';
import type {SalesGoodsPostingSetupInput} from '@erpx/server-commerce/public/sales-goods-posting-setup';

export async function requireSalesGoodsPostingFinanceAuthority(database:SqlDatabase,actor:WorkContext){
 requirePermission(actor,'finance.read');requirePermission(actor,'finance.write');
 if(actor.scope.kind!=='holding'||actor.branchId||actor.departmentId)throw new ApiError(403,'COMPANY_SCOPE_REQUIRED','Use a current Company Finance assignment for Sales goods posting setup.');
 const current=await requireQueuedCommandAuthority(database,actor,'finance.write');requirePermission(current,'finance.read');
 await assertProductionModuleEnabled(database,actor.companyId,'sales');await assertProductionModuleEnabled(database,actor.companyId,'finance');return current;
}
/** Financial source images stay inside the owner. No chart/account permission
 * or package is inferred from a caller's source label. */
export function salesGoodsPostingFinanceSourceLocks(database:SqlDatabase){return [database.prepare('LOCK TABLE finance_customer_receipts,finance_invoice_line_versions,finance_financial_events,finance_payment_reversals IN SHARE MODE')];}
export async function salesGoodsPostingFinanceState(database:SqlDatabase,actor:WorkContext){
 const [references,approved,money,history]=await Promise.all([
  commercePostingFinanceReferences(database,actor),
  approvedSalesPostingDefaults(database,actor.companyId),
  database.prepare("SELECT 'settings' kind,to_jsonb(s)::text source FROM company_money_settings s WHERE s.company_id=? UNION ALL SELECT 'scale:'||s.currency,to_jsonb(s)::text FROM company_currency_scales s WHERE s.company_id=? UNION ALL SELECT 'rate:'||r.id,to_jsonb(r)::text FROM company_fx_rates r WHERE r.company_id=? ORDER BY kind").bind(actor.companyId,actor.companyId,actor.companyId).all<{kind:string;source:string}>(),
  database.prepare(`WITH scope AS(SELECT ?::text company_id),fulfillments AS MATERIALIZED(SELECT actual.* FROM (${salesGoodsPostingFulfillmentSources}) actual WHERE actual.company_id=(SELECT company_id FROM scope)),invoices AS MATERIALIZED(SELECT invoice.* FROM finance_invoices invoice WHERE invoice.company_id=(SELECT company_id FROM scope) AND (EXISTS(SELECT 1 FROM fulfillments f WHERE f.invoice_id=invoice.id) OR invoice.source_type='commerce_fulfillment' AND EXISTS(SELECT 1 FROM fulfillments f WHERE f.id=invoice.source_id))),payments AS MATERIALIZED(SELECT p.* FROM finance_payments p WHERE p.company_id=(SELECT company_id FROM scope) AND EXISTS(SELECT 1 FROM finance_allocations a JOIN invoices i ON i.id=a.invoice_id WHERE a.payment_id=p.id AND a.company_id=p.company_id)),reversals AS MATERIALIZED(SELECT r.* FROM finance_payment_reversals r JOIN payments p ON p.id=r.payment_id),primary_journals AS MATERIALIZED(SELECT journal_id id FROM invoices WHERE journal_id IS NOT NULL UNION SELECT journal_id FROM payments WHERE journal_id IS NOT NULL UNION SELECT fx_journal_id FROM payments WHERE fx_journal_id IS NOT NULL UNION SELECT reversal_journal_id FROM reversals UNION SELECT fx_reversal_journal_id FROM reversals WHERE fx_reversal_journal_id IS NOT NULL),journals AS MATERIALIZED(SELECT id FROM primary_journals UNION SELECT b.secondary_journal_id FROM finance_book_postings b JOIN primary_journals p ON p.id=b.primary_journal_id WHERE b.company_id=(SELECT company_id FROM scope))
 SELECT 'invoice:'||i.id kind,to_jsonb(i)::text source FROM invoices i
 UNION ALL SELECT 'invoice-line:'||l.id,to_jsonb(l)::text FROM finance_invoice_line_versions l JOIN invoices i ON i.id=l.invoice_id AND i.company_id=l.company_id
 UNION ALL SELECT 'allocation:'||a.id,to_jsonb(a)::text FROM finance_allocations a JOIN invoices i ON i.id=a.invoice_id AND i.company_id=a.company_id
 UNION ALL SELECT 'payment:'||p.id,to_jsonb(p)::text FROM payments p
 UNION ALL SELECT 'reversal:'||r.id,to_jsonb(r)::text FROM reversals r
 UNION ALL SELECT 'receipt:'||r.id,to_jsonb(r)::text FROM finance_customer_receipts r JOIN payments p ON p.id=r.payment_id AND p.company_id=r.company_id
 UNION ALL SELECT 'journal:'||j.id,to_jsonb(j)::text FROM finance_journals j JOIN journals actual ON actual.id=j.id WHERE j.company_id=(SELECT company_id FROM scope)
 UNION ALL SELECT 'capture:'||c.id,to_jsonb(c)::text FROM finance_financial_events c JOIN journals actual ON actual.id=c.journal_id WHERE c.company_id=(SELECT company_id FROM scope)
 UNION ALL SELECT 'book:'||b.secondary_journal_id,to_jsonb(b)::text FROM finance_book_postings b JOIN primary_journals p ON p.id=b.primary_journal_id WHERE b.company_id=(SELECT company_id FROM scope)
 UNION ALL SELECT 'line:'||l.id,to_jsonb(l)::text FROM finance_journal_lines l JOIN journals actual ON actual.id=l.journal_id
 UNION ALL SELECT 'money:'||m.source_type||':'||m.source_id||':'||m.amount_kind,to_jsonb(m)::text FROM money_transaction_snapshots m WHERE m.company_id=(SELECT company_id FROM scope) AND (m.source_id IN(SELECT id FROM invoices) OR m.source_id IN(SELECT id FROM payments) OR m.source_id IN(SELECT id FROM journals))
 UNION ALL SELECT 'integrity',jsonb_build_object(
 'fulfillmentInvoice',EXISTS(SELECT 1 FROM fulfillments f WHERE f.invoice_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM finance_invoices i WHERE i.id=f.invoice_id AND i.company_id=f.company_id AND i.kind='receivable')),
 'invoiceLink',EXISTS(SELECT 1 FROM invoices i JOIN fulfillments f ON i.source_type='commerce_fulfillment' AND i.source_id=f.id WHERE f.invoice_id IS DISTINCT FROM i.id),
 'invoiceJournal',EXISTS(SELECT 1 FROM invoices i WHERE i.journal_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM finance_journals j WHERE j.id=i.journal_id AND j.company_id=i.company_id)),
 'invoiceLine',EXISTS(SELECT 1 FROM finance_invoice_line_versions l JOIN invoices i ON i.id=l.invoice_id WHERE l.company_id<>i.company_id),
 'allocation',EXISTS(SELECT 1 FROM finance_allocations a JOIN invoices i ON i.id=a.invoice_id LEFT JOIN finance_payments p ON p.id=a.payment_id WHERE a.company_id<>i.company_id OR p.company_id IS DISTINCT FROM i.company_id),
 'paymentJournal',EXISTS(SELECT 1 FROM payments p WHERE p.journal_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM finance_journals j WHERE j.id=p.journal_id AND j.company_id=p.company_id) OR p.fx_journal_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM finance_journals j WHERE j.id=p.fx_journal_id AND j.company_id=p.company_id)),
 'reversalJournal',EXISTS(SELECT 1 FROM reversals r JOIN payments p ON p.id=r.payment_id WHERE NOT EXISTS(SELECT 1 FROM finance_journals j WHERE j.id=r.reversal_journal_id AND j.company_id=p.company_id) OR r.fx_reversal_journal_id IS NOT NULL AND NOT EXISTS(SELECT 1 FROM finance_journals j WHERE j.id=r.fx_reversal_journal_id AND j.company_id=p.company_id)),
 'book',EXISTS(SELECT 1 FROM finance_book_postings b JOIN primary_journals p ON p.id=b.primary_journal_id WHERE b.company_id<>(SELECT company_id FROM scope) OR NOT EXISTS(SELECT 1 FROM finance_journals j WHERE j.id=b.secondary_journal_id AND j.company_id=b.company_id)),
 'capture',EXISTS(SELECT 1 FROM finance_financial_events c JOIN journals actual ON actual.id=c.journal_id WHERE c.company_id<>(SELECT company_id FROM scope)))::text
 ORDER BY kind,source`).bind(actor.companyId).all<{kind:string;source:string}>(),
 ]);
 const {defaults,expectedSalesDefaults,service}=approved;
 return {references,defaults,expectedSalesDefaults,service,retainedFinancialImage:JSON.stringify([approved.editions.map(row=>row.source),history.results]),historyInvalid:history.results.some(row=>row.kind==='integrity'&&Object.values(JSON.parse(row.source) as Record<string,boolean>).some(Boolean)),defaultsBlocked:approved.defaultsBlocked,image:JSON.stringify([references.image,approved.editions.map(row=>row.source),money.results,history.results])};
}
export async function validateSalesGoodsPostingFinanceState(database:SqlDatabase,actor:WorkContext,input:SalesGoodsPostingSetupInput,requiresApprovedDefaults:boolean){
 const state=await salesGoodsPostingFinanceState(database,actor);
 if(state.historyInvalid)throw new ApiError(409,'SALES_GOODS_POSTING_HISTORY_REQUIRES_REVIEW','The existing Sales financial source has a missing or foreign owner relationship. Reconcile the original source before initial goods setup.');
 await validateCommercePostingFinanceReferences(database,actor,input);
 if(state.defaultsBlocked||requiresApprovedDefaults&&!state.defaults)throw new ApiError(409,'SALES_DEFAULTS_REQUIRED','Restore or independently approve current Sales defaults before adding goods posting to existing documents.');
 if(JSON.stringify(input.expectedSalesDefaults)!==JSON.stringify(state.expectedSalesDefaults))throw new ApiError(409,'SALES_GOODS_POSTING_DEFAULTS_CHANGED','The reviewed Sales defaults edition changed. Keep the mapping draft and review the current edition.');
 if(!approvedSalesPostingDefaultMatches(state.defaults,input))throw new ApiError(409,'SALES_GOODS_POSTING_DEFAULTS_CHANGED','Retain the current independently approved Sales ledger, currency, receivable, revenue and VAT settings.');
}
/** Inserting a genuine goods mapping must leave the effective reviewed Sales
 * settings intact, including the original optional no-VAT default behavior. */
export function salesGoodsPostingEffectiveService(database:SqlDatabase,actor:WorkContext){return approvedSalesPostingEffectiveService(database,actor.companyId);}
