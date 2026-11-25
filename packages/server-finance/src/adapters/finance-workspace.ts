import { financeReferenceListSql } from './finance-read-tail-r42-sql';
import { UNMATCHED_BANK_TXN_COUNT_SQL } from './finance-read-tail-r44-sql';
import { companyCountSql, correlatedCountSql } from './finance-read-tail-r55-sql';
import "server-only";
import { requireFinancePurchasedRead } from "@erpx/server-finance/adapters/purchase-boundary";

import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError } from "@erpx/server-runtime/public/http";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import {calculateCloseReconciliations} from '@erpx/server-finance/public/finance';
import { intercompanyWorkspace } from "@erpx/server-finance/public/finance-intercompany";
import { consolidationRunWorkspace } from "@erpx/server-finance/public/finance-consolidation";
import { pendingVatReviewSql } from "@erpx/server-finance/public/finance-tax-register";
import {financeExpansionClosePending} from "@erpx/server-finance/public/native-expansion-close";
import { readListPage } from "@erpx/server-work_projection/public/list-page";
import {financeSetupAuthority} from "@erpx/server-finance/public/finance-starter";

import {createFinanceWorkspace} from '@erpx/server-finance/application/workspace';
import type {PaymentRecord,CountRow,ClosePeriodSource,CloseReadiness,PaymentObligationFilters} from '@erpx/server-finance/domain/workspace';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {financeWorkspacePeoplePayments,financeWorkspacePendingPeopleSql,financeWorkspaceReadyExpenseAiSql} from '@erpx/server-people_payroll/public/finance-workspace-sources';
import {headers as nextHeaders} from 'next/headers';
import {nativePeriodCloseWorkspace} from '@erpx/feature-finance/public/server';
import {financeWorkspaceReturnJoinSql} from '@erpx/server-commerce/public/finance-obligation-sources';
import {nativeSetupCounts, type SetupCounts} from './native-lane-3/setup-client';
export type {FinanceAttention,PaymentObligation,PaymentObligationFilters,ClosePeriodRow} from '@erpx/server-finance/domain/workspace';
const readinessUnavailable=(status=503,code='NATIVE_FINANCE_PERIOD_CLOSE_UNAVAILABLE'):never=>{throw new ApiError(status,code,'The native Finance period-close readiness is unavailable.');};
/** Exact native readiness projection. The native owner computes the count; a
 * missing, malformed or denied reply closes the read without a TS SQL fallback. */
export async function periodCloseReadiness(reply:Response|undefined):Promise<CloseReadiness>{
  if(!reply||reply.status!==200||!reply.body){void reply?.body?.cancel().catch(()=>undefined);return readinessUnavailable(reply?.status && reply.status>=400&&reply.status<600?reply.status:503);}
  let value:unknown;
  try{value=await reply.json();}catch{return readinessUnavailable(502,'NATIVE_FINANCE_PERIOD_CLOSE_INVALID');}
  const count=(value as {data?:{readiness?:{openManufacturing?:unknown}}})?.data?.readiness?.openManufacturing;
  if(typeof count!=='number'||!Number.isSafeInteger(count)||count<0)return readinessUnavailable(502,'NATIVE_FINANCE_PERIOD_CLOSE_INVALID');
  return {openManufacturing:count};
}
/** Company Finance period-close readiness read through the owner's native
 * workspace transport. The current browser headers carry the session context. */
export async function nativePeriodCloseReadiness(actor:WorkContext,periodId:string):Promise<CloseReadiness>{
  if(!periodId||periodId.includes('/')||periodId.includes('\\'))return readinessUnavailable(422,'NATIVE_FINANCE_PERIOD_CLOSE_INVALID');
  const reply=await nativePeriodCloseWorkspace(new Request('http://native-period-close.invalid',{headers:new Headers(await nextHeaders() as unknown as Headers)}),actor as Parameters<typeof nativePeriodCloseWorkspace>[1],actor.holdingSlug,periodId);
  return periodCloseReadiness(reply);
}
function nativeFinanceWorkspace(provided?:SqlDatabase){let write:SqlDatabase;
  const setup = new Map<string, Promise<SetupCounts>>();
  const setupFacts = (actor: WorkContext) => {
    const key = `${actor.holdingId}\u0000${actor.companyId}`;
    let pending = setup.get(key);
    if (!pending) { pending = nativeSetupCounts(actor); setup.set(key, pending); }
    return pending;
  };
  return createFinanceWorkspace({error:(...args)=>new ApiError(...args),permission:requirePermission,hasPermission,ready:async actor=>{write=provided??(await holdingResources(actor.holdingId)).write;await requireFinancePurchasedRead(write,actor)},today:()=>new Date().toISOString().slice(0,10),businessDate:()=>new Intl.DateTimeFormat('en-CA',{timeZone:'Asia/Bangkok',year:'numeric',month:'2-digit',day:'2-digit'}).format(new Date()),reconciliation:(actor,period)=>calculateCloseReconciliations(write,actor,period),expansionPending:(actor,start,end)=>financeExpansionClosePending(write,actor.companyId,start,end),closeReadiness:(actor,periodId)=>nativePeriodCloseReadiness(actor,periodId),consolidation:consolidationRunWorkspace,intercompany:intercompanyWorkspace,setupAuthority:actor=>financeSetupAuthority(write,actor.companyId),paymentRecords:async(actor,query)=>{  const clauses = ["p.company_id=?"], args: unknown[] = [actor.companyId];
  const search = query.search;
  if (search) { clauses.push("(p.payment_number ILIKE ? OR COALESCE(f.name,'') ILIKE ?)"); args.push(`%${search}%`,`%${search}%`); }
  if (query.status) { clauses.push("p.status=?"); args.push(query.status); }
  return readListPage<PaymentRecord>(write, {
    sql: `SELECT p.id,p.payment_number,p.kind,p.payment_date,p.currency,p.amount_minor,p.outstanding_minor,p.status,f.name AS party_name,
      CAST(p.payment_date AS text)||':'||p.payment_number AS page_key FROM finance_payments p LEFT JOIN finance_parties f ON f.id=p.party_id
      WHERE ${clauses.join(" AND ")}`,
    args, scope: [actor.holdingId,actor.companyId,"finance-payment-records",search,query.status], cursor: query.cursor, limit:query.limit, descending: true,
  });
},peoplePayments:actor=>financeWorkspacePeoplePayments(write,actor.companyId),workbenchCounters:async(actor,today)=>write
    .prepare(`SELECT
      (${companyCountSql('finance_invoices', " AND status='draft'")}) draft_invoices,
      (${companyCountSql('finance_invoices', " AND kind='receivable' AND status IN ('posted','partially_paid') AND outstanding_minor>0 AND due_date<?")}) overdue_receivables,
      (SELECT COALESCE(sum(outstanding_minor),0) FROM finance_invoices WHERE company_id=? AND kind='receivable' AND status IN ('posted','partially_paid') AND outstanding_minor>0 AND due_date<?) overdue_receivable_minor,
      (${companyCountSql('finance_invoices', " AND kind='payable' AND status IN ('posted','partially_paid') AND outstanding_minor>0 AND due_date<=?")}) due_payables,
      (SELECT COALESCE(sum(outstanding_minor),0) FROM finance_invoices WHERE company_id=? AND kind='payable' AND status IN ('posted','partially_paid') AND outstanding_minor>0 AND due_date<=?) due_payable_minor,
      (${companyCountSql('finance_journals', " AND status IN ('draft','in_review')")}) draft_journals,
      ((${companyCountSql('finance_payments', " AND status NOT IN ('settled','voided') AND outstanding_minor>0")})
        + (SELECT count(*) FROM finance_supplier_batches b WHERE b.company_id=? AND b.status IN ('draft','approved','exported') AND (b.status IN ('draft','approved') OR EXISTS (SELECT 1 FROM finance_supplier_instructions i WHERE i.batch_id=b.id AND i.status IN ('exported','unknown','failed'))))
        + (${companyCountSql('finance_refund_obligations', " AND status IN ('pending','approved') AND outstanding_minor>0")})
        + ${financeWorkspacePendingPeopleSql}) pending_payments,
      (${UNMATCHED_BANK_TXN_COUNT_SQL}) unmatched_bank,
      ${financeWorkspaceReadyExpenseAiSql} ready_ai_proposals,
      (${companyCountSql('finance_periods', " AND state='closing'")}) closing_periods,
      (${companyCountSql('finance_periods', " AND state='open' AND end_date<?")}) expired_open_periods`)
    .bind(
      actor.companyId,
      actor.companyId,
      today,
      actor.companyId,
      today,
      actor.companyId,
      today,
      actor.companyId,
      today,
      actor.companyId,
      actor.companyId,
      actor.companyId,
      actor.companyId,
      actor.companyId,
      actor.companyId,
      actor.companyId,
      actor.companyId,
      actor.companyId,
      today,
    )
    .first<CountRow>(),
firstCurrency:async(actor)=>{const currency=(await setupFacts(actor)).firstCurrency;return currency===null?null:{base_currency:currency};},
dueAmounts:async(actor,today)=>(await write.prepare(`SELECT kind,currency,SUM(outstanding_minor) amount_minor FROM finance_invoices
    WHERE company_id=? AND status IN ('posted','partially_paid') AND outstanding_minor>0
      AND ((kind='receivable' AND due_date<?) OR (kind='payable' AND due_date<=?))
    GROUP BY kind,currency ORDER BY kind,currency`).bind(actor.companyId,today,today).all<{kind:string;currency:string;amount_minor:number}>()).results,
genericPayments:async(actor)=>(await write.prepare(`SELECT p.id,p.payment_number source_id,p.kind,p.payment_date due_date,p.currency,p.outstanding_minor amount_minor,p.status
      FROM finance_payments p WHERE p.company_id=? AND p.status NOT IN ('settled','voided') AND p.outstanding_minor>0 ORDER BY p.payment_date,p.id`).bind(actor.companyId).all<Record<string,string|number|null>>()).results,
supplierBatches:async(actor)=>(await write.prepare(`SELECT b.id,b.planned_date due_date,b.currency,b.status,
      COALESCE(SUM(CASE WHEN i.status IN ('reserved','exported','unknown','failed') THEN i.amount_minor ELSE 0 END),0) amount_minor
      FROM finance_supplier_batches b LEFT JOIN finance_supplier_instructions i ON i.batch_id=b.id
      WHERE b.company_id=? AND b.status IN ('draft','approved','exported')
        AND (b.status IN ('draft','approved') OR EXISTS (SELECT 1 FROM finance_supplier_instructions pending WHERE pending.batch_id=b.id AND pending.status IN ('exported','unknown','failed')))
      GROUP BY b.id ORDER BY b.planned_date,b.id`).bind(actor.companyId).all<Record<string,string|number|null>>()).results,
refunds:async(actor)=>(await write.prepare(`SELECT o.id,o.return_id,o.direction,COALESCE(r.return_date,c.entry_date) due_date,o.currency,o.status,o.outstanding_minor amount_minor
      FROM finance_refund_obligations o ${financeWorkspaceReturnJoinSql} LEFT JOIN finance_billing_corrections c ON c.id=o.billing_correction_id AND c.company_id=o.company_id
      WHERE o.company_id=? AND o.status IN ('pending','approved') AND o.outstanding_minor>0 ORDER BY COALESCE(r.return_date,c.entry_date),o.id`).bind(actor.companyId).all<Record<string,string|number|null>>()).results,
periods:async(actor)=>(await write
    .prepare(`SELECT p.*,
      (${correlatedCountSql('j', 'finance_journals', " AND j.period_id=p.id AND j.status IN ('draft','in_review')")}) draft_journals,
      (${correlatedCountSql('b', 'finance_bank_transactions', " AND b.transaction_date BETWEEN p.start_date AND p.end_date AND b.match_status<>'matched'")}) unmatched_bank,
      (${correlatedCountSql('i', 'finance_invoices', " AND i.status IN ('draft','in_review') AND i.invoice_date BETWEEN p.start_date AND p.end_date")}) draft_invoices,
      (${correlatedCountSql('r', 'finance_calculation_runs', " AND r.period_id=p.id AND r.status IN ('prepared','reviewed','stale')")}) pending_calculations
      FROM finance_periods p WHERE p.company_id=? ORDER BY p.start_date DESC`)
    .bind(actor.companyId)
    .all<ClosePeriodSource>()).results,
pendingTax:async(actor,endDate)=>write.prepare(pendingVatReviewSql).bind(endDate,actor.companyId).first("value"),
charts:async(actor)=>(await write.prepare(financeReferenceListSql('finance_charts','company_id=?','created_at')).bind(actor.companyId).all<Record<string, string | number | null>>()).results,
accountCount:async(actor)=>({count:(await setupFacts(actor)).accountCount}),
periodCount:async(actor)=>({count:(await setupFacts(actor)).periodCount}),
taxCount:async(actor)=>({count:(await setupFacts(actor)).taxCount}),
bankCount:async(actor)=>({count:(await setupFacts(actor)).bankCount}),
parties:async(actor)=>(await write.prepare("SELECT id,kind,code,name,currency,status,version,updated_at FROM finance_parties WHERE company_id=? AND kind IN ('customer','supplier') AND status='active' ORDER BY kind,name").bind(actor.companyId).all<Record<string, string | number | null>>()).results,
profileCount:async(actor,businessDate)=>write.prepare("SELECT count(*) count FROM finance_accounting_profiles p JOIN finance_charts c ON c.id=p.statutory_chart_id AND c.company_id=p.company_id AND c.status='active' WHERE p.company_id=? AND p.status='approved' AND p.effective_date<=?").bind(actor.companyId,businessDate).first<{count:number}>(),
controlCount:async(actor)=>write.prepare("SELECT count(*) count FROM (SELECT a.chart_id FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id AND c.status='active' WHERE a.company_id=? AND a.status='active' AND NOT EXISTS(SELECT 1 FROM finance_book_rules r WHERE r.secondary_chart_id=a.chart_id AND r.status='approved') GROUP BY a.chart_id HAVING bool_or(a.control_kind='bank' AND a.category='asset') AND bool_or(a.control_kind='receivable' AND a.category='asset') AND bool_or(a.control_kind='payable' AND a.category='liability')) configured").bind(actor.companyId).first<{count:number}>()});}
export const paymentRecordsWorkspace=(actor:WorkContext,query:{q?:string;paymentStatus?:string;paymentCursor?:string},limit=30,provided?:SqlDatabase)=>nativeFinanceWorkspace(provided).paymentRecordsWorkspace(actor,query,limit);
export const financeWorkbench=(actor:WorkContext,asOf?:string,provided?:SqlDatabase)=>nativeFinanceWorkspace(provided).financeWorkbench(actor,asOf);
export const paymentObligationsWorkspace=(actor:WorkContext,filters:PaymentObligationFilters={},provided?:SqlDatabase)=>nativeFinanceWorkspace(provided).paymentObligationsWorkspace(actor,filters);
export const closeWorkspace=(actor:WorkContext,provided?:SqlDatabase)=>nativeFinanceWorkspace(provided).closeWorkspace(actor);
export const groupFinanceWorkspace=async(actor:WorkContext)=>{const write=(await holdingResources(actor.holdingId)).write;await requireFinancePurchasedRead(write,actor);return nativeFinanceWorkspace().groupFinanceWorkspace(actor);};
export const financeSetupWorkspace=(actor:WorkContext,provided?:SqlDatabase)=>nativeFinanceWorkspace(provided).financeSetupWorkspace(actor);
