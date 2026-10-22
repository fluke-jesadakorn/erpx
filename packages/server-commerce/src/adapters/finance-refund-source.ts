import { commercialOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/commercial-owner-projection-sources";
import {financeSalesReceiptOriginalSources} from '@erpx/server-finance/public/sales-receipt-sources';
import 'server-only';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import {assertion} from "@erpx/server-workflow/public/command-assertions";
type Row = Record<string, string | number | null>;
export async function refundCashAccounts(db:SqlDatabase,actor:WorkContext,owner:Row){
 if(await serviceRefundCashOwnerShape(db,actor,String(owner.id)))return (await db.prepare(`SELECT account_id id,code,name FROM (${serviceCashSources}) source WHERE source.company_id=? AND source.obligation_id=? ORDER BY code`).bind(actor.companyId,owner.id).all<Row>()).results;
 return (await db.prepare(`SELECT id,code,name FROM (${financeProjectionSources.finance_accounts}) finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND category='asset' AND id<>? AND (id=(SELECT cash_account_id FROM commerce_settings WHERE company_id=finance_accounts.company_id) OR (?::text IS NOT NULL AND control_kind='bank')) ORDER BY code`).bind(actor.companyId,owner.chart_id,owner.balance_account_id,owner.billing_correction_id??null).all<Row>()).results;
}
/** Shape only: this chooses an owned constructor, never an account or authority. */
export async function serviceRefundCashOwnerShape(db:SqlDatabase,actor:WorkContext,id:string){
 return !!await db.prepare(`SELECT 1 FROM (${financeProjectionSources.finance_refund_obligations}) refund
 JOIN commerce_returns returned ON returned.id=refund.return_id AND returned.company_id=refund.company_id
 JOIN commerce_orders sale ON sale.id=returned.order_id AND sale.company_id=refund.company_id AND sale.kind='sales'
 JOIN commerce_fulfillments fulfilled ON fulfilled.id=returned.fulfillment_id AND fulfilled.order_id=sale.id
 WHERE refund.id=? AND refund.company_id=? AND refund.billing_correction_id IS NULL AND refund.direction='outgoing' AND refund.branch_id IS NULL AND refund.department_id IS NULL
 AND returned.kind='customer_return' AND returned.branch_id IS NULL AND sale.branch_id IS NULL AND sale.department_id IS NULL AND sale.warehouse_id IS NULL AND fulfilled.completion_kind='service' AND fulfilled.journal_id IS NULL
 AND NOT EXISTS(SELECT 1 FROM commerce_settings configured WHERE configured.company_id=refund.company_id)`).bind(id,actor.companyId).first();
}
export function refundCashAccountGuard(db: SqlDatabase, actor: WorkContext, key: string, facts: {
    cashAccountId: string;
    chartId: Row[string];
    balanceAccountId: Row[string];
    billingCorrectionId: Row[string];
    serviceCash?:ServiceRefundCashSource;
}) {
 if(facts.serviceCash){
  if(facts.serviceCash.cashAccountId!==facts.cashAccountId)throw new Error('SERVICE_REFUND_CASH_SELECTION_MISMATCH');
  return serviceRefundCashSourceGuard(db,actor,key,facts.serviceCash);
 }
 return assertion(db, key, 'cash-account', `EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_accounts}) finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND category='asset' AND status='active' AND id<>? AND (id=(SELECT cash_account_id FROM commerce_settings WHERE company_id=finance_accounts.company_id) OR (?::text IS NOT NULL AND control_kind='bank')))`, [facts.cashAccountId, actor.companyId, facts.chartId, facts.balanceAccountId, facts.billingCorrectionId]); }
export function refundForeignCreditGuard(db: SqlDatabase, key: string, id: Row[string], sourceBase: number) { return assertion(db, key, 'source-credit', `EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_refund_obligations}) o JOIN commerce_returns r ON r.id=o.return_id JOIN (${financeProjectionSources.finance_journals}) j ON j.id=o.source_credit_journal_id AND j.id=r.credit_journal_id AND j.company_id=o.company_id AND j.chart_id=o.chart_id WHERE o.id=? AND o.base_outstanding_minor=? AND j.status IN ('posted','reversed'))`, [id, sourceBase]); }
export async function billingCorrectionExternalCredit(db: SqlDatabase, invoiceId: string) { return db.prepare("SELECT 1 FROM commerce_returns WHERE invoice_id=? AND status IN ('requested','approved','posted')").bind(invoiceId).first(); }

/** Closed original Service receipt provenance. No account/source SQL is accepted. */
export type ServiceRefundCashSource={obligationId:string;cashAccountId:string;image:string};
const serviceCashSources=`SELECT refund.id obligation_id,refund.company_id,account.id account_id,account.code,account.name,
 jsonb_agg(jsonb_build_array(receipt.image,to_jsonb(account),to_jsonb(chart),to_jsonb(returned),to_jsonb(sale),to_jsonb(fulfilled),to_jsonb(primary_completion),to_jsonb(invoiced),
 jsonb_build_array(refund.id,refund.company_id,refund.return_id,refund.invoice_id,refund.party_id,refund.direction,refund.currency,refund.chart_id,refund.balance_account_id,refund.amount_minor,refund.base_amount_minor,refund.source_credit_journal_id),
 (SELECT jsonb_agg(to_jsonb(line) ORDER BY line.ordinal) FROM (${financeProjectionSources.finance_journal_lines}) line WHERE line.journal_id=receipt.journal_id)) ORDER BY receipt.id)::text image
 FROM (${financeProjectionSources.finance_refund_obligations}) refund
 JOIN commerce_returns returned ON returned.id=refund.return_id AND returned.company_id=refund.company_id AND returned.invoice_id=refund.invoice_id AND returned.credit_journal_id=refund.source_credit_journal_id
 JOIN commerce_orders sale ON sale.id=returned.order_id AND sale.company_id=refund.company_id AND sale.kind='sales' AND sale.party_id=refund.party_id AND sale.currency=refund.currency
 JOIN commerce_fulfillments fulfilled ON fulfilled.id=returned.fulfillment_id AND fulfilled.order_id=sale.id AND fulfilled.invoice_id=refund.invoice_id
 JOIN (${financeProjectionSources.finance_invoices}) invoice ON invoice.id=refund.invoice_id AND invoice.company_id=refund.company_id AND invoice.party_id=refund.party_id AND invoice.kind='receivable' AND invoice.currency=refund.currency AND invoice.debit_account_id=refund.balance_account_id AND invoice.source_type='commerce_fulfillment'
 JOIN commerce_fulfillments primary_completion ON primary_completion.id=invoice.source_id AND primary_completion.order_id=sale.id AND primary_completion.invoice_id=invoice.id AND primary_completion.completion_kind='service' AND primary_completion.journal_id IS NULL
 JOIN (${financeProjectionSources.finance_journals}) invoiced ON invoiced.id=invoice.journal_id AND invoiced.company_id=refund.company_id AND invoiced.chart_id=refund.chart_id AND invoiced.currency=refund.currency AND invoiced.status='posted' AND invoiced.source_type='invoice' AND invoiced.source_id=invoice.id
 JOIN (${financeProjectionSources.finance_parties}) customer ON customer.id=refund.party_id AND customer.company_id=refund.company_id AND customer.kind='customer' AND customer.person_id IS NULL
 JOIN (${financeProjectionSources.finance_journals}) credit ON credit.id=refund.source_credit_journal_id AND credit.company_id=refund.company_id AND credit.chart_id=refund.chart_id AND credit.currency=refund.currency AND credit.status='posted' AND credit.source_type='credit_note' AND credit.source_id=returned.id
 JOIN (${financeSalesReceiptOriginalSources}) receipt ON receipt.company_id=refund.company_id AND receipt.invoice_id=invoice.id AND receipt.party_id=refund.party_id AND receipt.currency=refund.currency AND receipt.status='settled'
 JOIN (${financeProjectionSources.finance_journals}) received ON received.id=receipt.journal_id AND received.company_id=refund.company_id AND received.chart_id=refund.chart_id AND received.currency=refund.currency AND received.status='posted'
 JOIN (${financeProjectionSources.finance_accounts}) account ON account.id=receipt.bank_account AND account.company_id=refund.company_id AND account.chart_id=refund.chart_id AND account.currency=refund.currency AND account.category='asset' AND account.control_kind='bank' AND account.status='active' AND account.id<>refund.balance_account_id
 JOIN (${financeProjectionSources.finance_charts}) chart ON chart.id=refund.chart_id AND chart.company_id=refund.company_id AND chart.status='active'
 WHERE refund.billing_correction_id IS NULL AND refund.direction='outgoing' AND refund.branch_id IS NULL AND refund.department_id IS NULL
 AND returned.kind='customer_return' AND returned.status='posted' AND returned.branch_id IS NULL AND returned.stock_journal_id IS NULL AND returned.goods_fx_journal_id IS NULL AND returned.refund_minor=refund.amount_minor AND returned.base_refund_minor=refund.base_amount_minor
 AND sale.branch_id IS NULL AND sale.department_id IS NULL AND sale.warehouse_id IS NULL AND fulfilled.completion_kind='service' AND fulfilled.journal_id IS NULL
 AND NOT EXISTS(SELECT 1 FROM commerce_settings configured WHERE configured.company_id=refund.company_id)
 AND EXISTS(SELECT 1 FROM commerce_fulfillment_lines actual JOIN commerce_order_lines line ON line.id=actual.order_line_id AND line.order_id=sale.id WHERE actual.fulfillment_id=fulfilled.id AND line.item_type='service')
 AND NOT EXISTS(SELECT 1 FROM commerce_fulfillment_lines actual LEFT JOIN commerce_order_lines line ON line.id=actual.order_line_id AND line.order_id=sale.id WHERE actual.fulfillment_id=fulfilled.id AND line.item_type IS DISTINCT FROM 'service')
 AND (SELECT count(*) FROM (${financeProjectionSources.finance_journal_lines}) line WHERE line.journal_id=received.id)=2
 AND EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_journal_lines}) line WHERE line.journal_id=received.id AND line.account_id=account.id AND line.debit_minor=receipt.amount_minor AND line.credit_minor=0)
 AND EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_journal_lines}) line WHERE line.journal_id=received.id AND line.account_id=refund.balance_account_id AND line.credit_minor=receipt.amount_minor AND line.debit_minor=0)
 GROUP BY refund.id,refund.company_id,account.id,account.code,account.name`;
export async function readServiceRefundCashSource(db:SqlDatabase,actor:WorkContext,obligationId:string,cashAccountId:string):Promise<ServiceRefundCashSource|null>{
 const image=await db.prepare(`SELECT image FROM (${serviceCashSources}) source WHERE source.company_id=? AND source.obligation_id=? AND source.account_id=?`).bind(actor.companyId,obligationId,cashAccountId).first<string>('image');
 return image?{obligationId,cashAccountId,image}:null;
}
export function serviceRefundCashSourceGuard(db:SqlDatabase,actor:WorkContext,key:string,source:ServiceRefundCashSource){
 return assertion(db,key,'service-refund-cash-source',`(SELECT image FROM (${serviceCashSources}) original WHERE original.company_id=? AND original.obligation_id=? AND original.account_id=?)=?`,[actor.companyId,source.obligationId,source.cashAccountId,source.image]);
}

/** Query-free Commerce-only additions; Finance declares its own references. */
export function serviceRefundCashSourceReadLocks(db:SqlDatabase){return [db.prepare('LOCK TABLE commerce_settings,commerce_order_lines,commerce_fulfillment_lines IN SHARE MODE')];}
