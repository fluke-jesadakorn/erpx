import "server-only";

import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "./authorization";
import { ApiError } from "./http";
import { requirePermission } from "./iam-permissions";
import { holdingResources } from "./runtime";
import { calculateCloseReconciliations, type CloseReconciliation } from "./finance";
import { intercompanyWorkspace } from "./finance-intercompany";
import { consolidationRunWorkspace } from "./finance-consolidation";
import { pendingVatReviewSql,type VatCloseEvidence } from "./finance-tax-register";
import {financeExpansionClosePending} from "./finance-expansion-close";
import { readListPage } from "./list-page";
import {financeSetupAuthority} from "./finance-starter";

type PaymentRecord = { id: string; page_key: string; payment_number: string; kind: string; payment_date: string; currency: string; amount_minor: number; outstanding_minor: number; status: string; party_name: string | null };
export async function paymentRecordsWorkspace(actor: WorkContext, query: { q?: string; paymentStatus?: string; paymentCursor?: string }, limit=30) {
  actor = requirePermission(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  const clauses = ["p.company_id=?"], args: unknown[] = [actor.companyId];
  const search = query.q?.trim().slice(0,180);
  if (search) { clauses.push("(p.payment_number ILIKE ? OR COALESCE(f.name,'') ILIKE ?)"); args.push(`%${search}%`,`%${search}%`); }
  if (query.paymentStatus) { clauses.push("p.status=?"); args.push(query.paymentStatus); }
  return readListPage<PaymentRecord>(write, {
    sql: `SELECT p.id,p.payment_number,p.kind,p.payment_date,p.currency,p.amount_minor,p.outstanding_minor,p.status,f.name AS party_name,
      CAST(p.payment_date AS text)||':'||p.payment_number AS page_key FROM finance_payments p LEFT JOIN finance_parties f ON f.id=p.party_id
      WHERE ${clauses.join(" AND ")}`,
    args, scope: [actor.holdingId,actor.companyId,"finance-payment-records",search,query.paymentStatus], cursor: query.paymentCursor, limit, descending: true,
  });
}

export type FinanceAttention = {
  id: string;
  label: string;
  description: string;
  count: number;
  amountMinor: number | null;
  currency: string | null;
  amounts?: Array<{currency:string;amountMinor:number}>;
  href: string;
  tone: "urgent" | "attention" | "normal";
};

export type PaymentObligation = {
  id: string;
  sourceType: string;
  sourceLabel: string;
  sourceId: string;
  dueDate: string | null;
  currency: string;
  amountMinor: number;
  status: string;
  nextAction: string;
  href: string;
};

export type PaymentObligationFilters = {
  sourceType?: string;
  status?: string;
};

type CountRow = Record<string, number | string | null>;
export type ClosePeriodRow = {
  [key: string]: string | number | null | CloseReconciliation[] | VatCloseEvidence;
  id: string;
  name: string;
  start_date: string;
  end_date: string;
  state: string;
  draft_journals: number;
  unmatched_bank: number;
  draft_invoices: number;
  open_manufacturing: number;
  pending_calculations: number;
  pending_tax: number;
  pending_source_tasks: number;
  blockers: number;
  reconciliation_ready: number;
  reconciliations: CloseReconciliation[];
  vat_reconciliation: VatCloseEvidence|null;
};
type ClosePeriodSource = {
  [key: string]: string | number | null;
  id: string;
  name: string;
  start_date: string;
  end_date: string;
  state: string;
  draft_journals: number;
  unmatched_bank: number;
  draft_invoices: number;
  open_manufacturing: number;
  pending_calculations: number;
};

export async function financeWorkbench(actor: WorkContext, asOf?: string) {
  actor = requirePermission(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  const today = asOf ?? new Date().toISOString().slice(0, 10);
  const row = await write
    .prepare(`SELECT
      (SELECT count(*) FROM finance_invoices WHERE company_id=? AND status='draft') draft_invoices,
      (SELECT count(*) FROM finance_invoices WHERE company_id=? AND kind='receivable' AND status IN ('posted','partially_paid') AND outstanding_minor>0 AND due_date<?) overdue_receivables,
      (SELECT COALESCE(sum(outstanding_minor),0) FROM finance_invoices WHERE company_id=? AND kind='receivable' AND status IN ('posted','partially_paid') AND outstanding_minor>0 AND due_date<?) overdue_receivable_minor,
      (SELECT count(*) FROM finance_invoices WHERE company_id=? AND kind='payable' AND status IN ('posted','partially_paid') AND outstanding_minor>0 AND due_date<=?) due_payables,
      (SELECT COALESCE(sum(outstanding_minor),0) FROM finance_invoices WHERE company_id=? AND kind='payable' AND status IN ('posted','partially_paid') AND outstanding_minor>0 AND due_date<=?) due_payable_minor,
      (SELECT count(*) FROM finance_journals WHERE company_id=? AND status IN ('draft','in_review')) draft_journals,
      ((SELECT count(*) FROM finance_payments WHERE company_id=? AND status NOT IN ('settled','voided') AND outstanding_minor>0)
        + (SELECT count(*) FROM finance_supplier_batches b WHERE b.company_id=? AND b.status IN ('draft','approved','exported') AND (b.status IN ('draft','approved') OR EXISTS (SELECT 1 FROM finance_supplier_instructions i WHERE i.batch_id=b.id AND i.status IN ('exported','unknown','failed'))))
        + (SELECT count(*) FROM finance_refund_obligations WHERE company_id=? AND status IN ('pending','approved') AND outstanding_minor>0)
        + (SELECT count(*) FROM hr_payment_batches b WHERE b.company_id=? AND b.status IN ('draft','approved','exported','partially_settled'))) pending_payments,
      (SELECT count(*) FROM finance_bank_transactions WHERE company_id=? AND match_status<>'matched') unmatched_bank,
      (SELECT count(*) FROM expense_ai_proposals WHERE company_id=? AND state='ready') ready_ai_proposals,
      (SELECT count(*) FROM finance_periods WHERE company_id=? AND state='closing') closing_periods,
      (SELECT count(*) FROM finance_periods WHERE company_id=? AND state='open' AND end_date<?) expired_open_periods`)
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
    .first<CountRow>();
  const chart = await write
    .prepare("SELECT base_currency FROM finance_charts WHERE company_id=? ORDER BY created_at LIMIT 1")
    .bind(actor.companyId)
    .first<{ base_currency: string }>();
  const currency = chart?.base_currency ?? null;
  const dueAmounts=(await write.prepare(`SELECT kind,currency,SUM(outstanding_minor) amount_minor FROM finance_invoices
    WHERE company_id=? AND status IN ('posted','partially_paid') AND outstanding_minor>0
      AND ((kind='receivable' AND due_date<?) OR (kind='payable' AND due_date<=?))
    GROUP BY kind,currency ORDER BY kind,currency`).bind(actor.companyId,today,today).all<{kind:string;currency:string;amount_minor:number}>()).results;
  const attention: FinanceAttention[] = [
    { id: "invoice-review", label: "Documents to review", description: "Review the party, dates, tax, and accounts before approval", count: Number(row?.draft_invoices ?? 0), amountMinor: null, currency, href: "finance/invoices?status=draft", tone: "attention" },
    { id: "journal-review", label: "Journals to post", description: "Journals prepared for independent approval", count: Number(row?.draft_journals ?? 0), amountMinor: null, currency, href: "finance/accounting?status=draft", tone: "attention" },
    { id: "receivable-overdue", label: "Overdue receivables", description: "Follow up collection and record the contact evidence", count: Number(row?.overdue_receivables ?? 0), amountMinor: Number(row?.overdue_receivable_minor ?? 0), currency, href: "finance/invoices?kind=receivable&queue=open", tone: "urgent" },
    { id: "payable-due", label: "Due payables", description: "Schedule payment for approved obligations", count: Number(row?.due_payables ?? 0), amountMinor: Number(row?.due_payable_minor ?? 0), currency, href: "finance/invoices?kind=payable&queue=open", tone: "urgent" },
    { id: "payment-review", label: "Payment obligations", description: "Review approval, export, and bank confirmation across every source", count: Number(row?.pending_payments ?? 0), amountMinor: null, currency, href: "finance/payments", tone: "attention" },
    { id: "bank-unmatched", label: "Unreconciled bank items", description: "Match bank evidence to receipts and payments", count: Number(row?.unmatched_bank ?? 0), amountMinor: null, currency, href: "finance/bank", tone: "attention" },
    { id: "ai-review", label: "AI proposals to review", description: "Open evidence, compare the proposal, and accept or correct it", count: Number(row?.ready_ai_proposals ?? 0), amountMinor: null, currency, href: "expenses", tone: "normal" },
    { id: "period-close", label: "Period-close work", description: "Resolve blockers and collect close evidence", count: Number(row?.closing_periods ?? 0) + Number(row?.expired_open_periods ?? 0), amountMinor: null, currency, href: "finance/close", tone: "urgent" },
  ];
  const visible = attention.filter((item) => {
    if (item.id === "payment-review") return hasPermission(actor, "finance.pay");
    if (["journal-review", "period-close"].includes(item.id)) return hasPermission(actor, "finance.write");
    return true;
  });
  for(const item of visible){
    const kind=item.id==="receivable-overdue"?"receivable":item.id==="payable-due"?"payable":null;
    if(!kind)continue;
    item.amounts=dueAmounts.filter(row=>row.kind===kind).map(row=>({currency:row.currency,amountMinor:Number(row.amount_minor)}));
    // A cross-currency total has no valid currency label. Keep the compatible
    // scalar only when all obligations use the same transaction currency.
    item.currency=item.amounts.length===1?item.amounts[0].currency:null;
    item.amountMinor=item.amounts.length===1?item.amounts[0].amountMinor:null;
  }
  return {
    role: actor.role,
    currency,
    attention: visible,
    totalAttention: visible.reduce((sum, item) => sum + item.count, 0),
    canPrepare: hasPermission(actor, "finance.write"),
    canApprove: hasPermission(actor, "finance.approve"),
    canPay: hasPermission(actor, "finance.pay"),
  };
}

export async function paymentObligationsWorkspace(actor: WorkContext, filters: PaymentObligationFilters = {}): Promise<PaymentObligation[]> {
  actor = requirePermission(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  const canReadPeoplePayments = hasPermission(actor, "payroll.payment.read") || hasPermission(actor, "finance.pay");
  const [generic, supplier, refunds, people] = await Promise.all([
    write.prepare(`SELECT p.id,p.payment_number source_id,p.kind,p.payment_date due_date,p.currency,p.outstanding_minor amount_minor,p.status
      FROM finance_payments p WHERE p.company_id=? AND p.status NOT IN ('settled','voided') AND p.outstanding_minor>0 ORDER BY p.payment_date,p.id`).bind(actor.companyId).all<Record<string,string|number|null>>(),
    write.prepare(`SELECT b.id,b.planned_date due_date,b.currency,b.status,
      COALESCE(SUM(CASE WHEN i.status IN ('reserved','exported','unknown','failed') THEN i.amount_minor ELSE 0 END),0) amount_minor
      FROM finance_supplier_batches b LEFT JOIN finance_supplier_instructions i ON i.batch_id=b.id
      WHERE b.company_id=? AND b.status IN ('draft','approved','exported')
        AND (b.status IN ('draft','approved') OR EXISTS (SELECT 1 FROM finance_supplier_instructions pending WHERE pending.batch_id=b.id AND pending.status IN ('exported','unknown','failed')))
      GROUP BY b.id ORDER BY b.planned_date,b.id`).bind(actor.companyId).all<Record<string,string|number|null>>(),
    write.prepare(`SELECT o.id,o.return_id,o.direction,COALESCE(r.return_date,c.entry_date) due_date,o.currency,o.status,o.outstanding_minor amount_minor
      FROM finance_refund_obligations o LEFT JOIN commerce_returns r ON r.id=o.return_id LEFT JOIN finance_billing_corrections c ON c.id=o.billing_correction_id AND c.company_id=o.company_id
      WHERE o.company_id=? AND o.status IN ('pending','approved') AND o.outstanding_minor>0 ORDER BY COALESCE(r.return_date,c.entry_date),o.id`).bind(actor.companyId).all<Record<string,string|number|null>>(),
    canReadPeoplePayments
      ? write.prepare(`SELECT b.id,b.source_type,b.source_id,b.currency,b.status,r.pay_date due_date,
          COALESCE(SUM(CASE WHEN i.status IN ('outstanding','failed') THEN i.amount_minor ELSE 0 END),0) amount_minor
          FROM hr_payment_batches b LEFT JOIN hr_payment_batch_items i ON i.batch_id=b.id
          LEFT JOIN hr_payroll_runs r ON r.id=b.source_id AND b.source_type='payroll' AND r.company_id=b.company_id
          WHERE b.company_id=? AND b.status IN ('draft','approved','exported','partially_settled')
          GROUP BY b.id,r.pay_date ORDER BY r.pay_date NULLS LAST,b.id`).bind(actor.companyId).all<Record<string,string|number|null>>()
      : Promise.resolve({ results: [] as Record<string,string|number|null>[] }),
  ]);
  const nextAction = (status: string, sourceType: string) => {
    if (sourceType === "generic_payment") return status === "draft" || status === "in_review" ? "Submit for independent approval" : status === "approved" ? "Release through Treasury" : "Match bank evidence";
    if (sourceType === "supplier_batch") return status === "draft" ? "Review beneficiaries and approve" : status === "approved" ? "Export through Treasury" : "Confirm each bank result";
    if (sourceType === "refund") return status === "pending" ? "Review and approve refund" : "Prepare or confirm refund instruction";
    return status === "draft" ? "Approve payroll or expense payment" : status === "approved" ? "Export payment instruction" : "Confirm each bank result";
  };
  const rows: PaymentObligation[] = [
    ...generic.results.map(row => ({ id: String(row.id), sourceType: "generic_payment", sourceLabel: String(row.kind) === "receipt" ? "Customer receipt payment" : "Supplier payment", sourceId: String(row.source_id), dueDate: row.due_date ? String(row.due_date) : null, currency: String(row.currency), amountMinor: Number(row.amount_minor), status: String(row.status), nextAction: nextAction(String(row.status), "generic_payment"), href: `finance/payments/${row.id}` })),
    ...supplier.results.map(row => ({ id: String(row.id), sourceType: "supplier_batch", sourceLabel: "Supplier payment batch", sourceId: String(row.id), dueDate: row.due_date ? String(row.due_date) : null, currency: String(row.currency), amountMinor: Number(row.amount_minor), status: String(row.status), nextAction: nextAction(String(row.status), "supplier_batch"), href: `finance/supplier-batches/${row.id}` })),
    ...refunds.results.map(row => ({ id: String(row.id), sourceType: "refund", sourceLabel: String(row.direction) === "outgoing" ? "Customer refund" : "Supplier refund", sourceId: String(row.return_id), dueDate: row.due_date ? String(row.due_date) : null, currency: String(row.currency), amountMinor: Number(row.amount_minor), status: String(row.status), nextAction: nextAction(String(row.status), "refund"), href: `finance/refunds/${row.id}` })),
    ...people.results.map(row => ({ id: String(row.id), sourceType: "people_payment", sourceLabel: String(row.source_type) === "payroll" ? "Payroll payment batch" : String(row.source_type) === "expense" ? "Expense payment batch" : "Contractor payment batch", sourceId: String(row.source_id), dueDate: row.due_date ? String(row.due_date) : null, currency: String(row.currency), amountMinor: Number(row.amount_minor), status: String(row.status), nextAction: nextAction(String(row.status), "people_payment"), href: String(row.source_type) === "payroll" ? `people/payroll/runs/${encodeURIComponent(String(row.source_id))}` : String(row.source_type) === "expense" ? `expenses/${row.source_id}` : `finance/invoices/${row.source_id}` })),
  ];
  return rows
    .filter(row => !filters.sourceType || row.sourceType === filters.sourceType)
    .filter(row => !filters.status || row.status === filters.status)
    .sort((left, right) => `${left.dueDate ?? "9999-99-99"}:${left.id}`.localeCompare(`${right.dueDate ?? "9999-99-99"}:${right.id}`));
}

export async function closeWorkspace(actor: WorkContext) {
  actor = requirePermission(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  const periods = await write
    .prepare(`SELECT p.*,
      (SELECT count(*) FROM finance_journals j WHERE j.company_id=p.company_id AND j.period_id=p.id AND j.status IN ('draft','in_review')) draft_journals,
      (SELECT count(*) FROM finance_bank_transactions b WHERE b.company_id=p.company_id AND b.transaction_date BETWEEN p.start_date AND p.end_date AND b.match_status<>'matched') unmatched_bank,
      (SELECT count(*) FROM finance_invoices i WHERE i.company_id=p.company_id AND i.status IN ('draft','in_review') AND i.invoice_date BETWEEN p.start_date AND p.end_date) draft_invoices,
      (SELECT count(*) FROM manufacturing_orders o WHERE o.company_id=p.company_id AND o.status IN ('reserved','in_progress','awaiting_qc')) open_manufacturing,
      (SELECT count(*) FROM finance_calculation_runs r WHERE r.company_id=p.company_id AND r.period_id=p.id AND r.status IN ('prepared','reviewed','stale')) pending_calculations
      FROM finance_periods p WHERE p.company_id=? ORDER BY p.start_date DESC`)
    .bind(actor.companyId)
    .all<ClosePeriodSource>();
  return Promise.all(periods.results.map(async(period): Promise<ClosePeriodRow> => {
    const reconciliation=await calculateCloseReconciliations(write,actor,period);
    const pendingTax=Number(await write.prepare(pendingVatReviewSql).bind(period.end_date,actor.companyId).first("value")??0),expansionPending=await financeExpansionClosePending(write,actor.companyId,String(period.start_date),String(period.end_date));
    return {
      ...period,
      pending_tax:pendingTax,
      pending_source_tasks:expansionPending.length,
      blockers: Number(period.draft_journals) + Number(period.unmatched_bank) + Number(period.draft_invoices) + Number(period.open_manufacturing) + Number(period.pending_calculations) + pendingTax + expansionPending.length + (reconciliation.ready?0:1),
      reconciliation_ready: reconciliation.ready?1:0,
      reconciliations: reconciliation.rows,
      vat_reconciliation: reconciliation.vat,
    };
  }));
}

export async function groupFinanceWorkspace(actor: WorkContext) {
  actor = requirePermission(actor, "finance.read");
  if (actor.scope.kind !== "holding") throw new ApiError(403, "HOLDING_FINANCE_REQUIRED", "Group Finance is available only in a Holding workspace.");
  const [consolidation, intercompany] = await Promise.all([consolidationRunWorkspace(actor), intercompanyWorkspace(actor)]);
  return { ...consolidation, ...intercompany, intercompanyStatus: intercompany.statusCounts };
}

export async function financeSetupWorkspace(actor: WorkContext) {
  actor = requirePermission(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  const [charts, accounts, periods, taxes, bankStatements, parties,authority,profiles,controls] = await Promise.all([
    write.prepare("SELECT * FROM finance_charts WHERE company_id=? ORDER BY created_at").bind(actor.companyId).all<Record<string, string | number | null>>(),
    write.prepare("SELECT count(*) count FROM finance_accounts WHERE company_id=? AND status='active'").bind(actor.companyId).first<{ count: number }>(),
    write.prepare("SELECT count(*) count FROM finance_periods WHERE company_id=?").bind(actor.companyId).first<{ count: number }>(),
    write.prepare("SELECT count(*) count FROM finance_tax_codes WHERE company_id=? AND status='active'").bind(actor.companyId).first<{ count: number }>(),
    write.prepare("SELECT count(DISTINCT bank_account) count FROM finance_bank_statements WHERE company_id=?").bind(actor.companyId).first<{ count: number }>(),
    write.prepare("SELECT id,kind,code,name,currency,status,version,updated_at FROM finance_parties WHERE company_id=? AND kind IN ('customer','supplier') AND status='active' ORDER BY kind,name").bind(actor.companyId).all<Record<string, string | number | null>>(),
    financeSetupAuthority(write,actor.companyId),
    write.prepare("SELECT count(*) count FROM finance_accounting_profiles p JOIN finance_charts c ON c.id=p.statutory_chart_id AND c.company_id=p.company_id AND c.status='active' WHERE p.company_id=? AND p.status='approved' AND p.effective_date<=?").bind(actor.companyId,new Intl.DateTimeFormat('en-CA',{timeZone:'Asia/Bangkok',year:'numeric',month:'2-digit',day:'2-digit'}).format(new Date())).first<{count:number}>(),
    write.prepare("SELECT count(*) count FROM (SELECT a.chart_id FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id AND c.status='active' WHERE a.company_id=? AND a.status='active' AND NOT EXISTS(SELECT 1 FROM finance_book_rules r WHERE r.secondary_chart_id=a.chart_id AND r.status='approved') GROUP BY a.chart_id HAVING bool_or(a.control_kind='bank' AND a.category='asset') AND bool_or(a.control_kind='receivable' AND a.category='asset') AND bool_or(a.control_kind='payable' AND a.category='liability')) configured").bind(actor.companyId).first<{count:number}>(),
  ]);
  return {
    charts: charts.results,
    parties: parties.results,
    checks: [
      { id: "chart", label: "Chart of accounts and base currency", complete: charts.results.length > 0, detail: `${charts.results.length} chart(s)` },
      { id: "accounts", label: "Posting accounts", complete: Number(accounts?.count ?? 0) > 0, detail: `${Number(accounts?.count ?? 0)} active account(s)` },
      {id:'controls',label:'Bank, receivable and payable controls',complete:Number(controls?.count??0)>0,detail:`${Number(controls?.count??0)} primary book(s) with all three control mappings`},
      {id:'profile',label:'Reviewed Company accounting policy',complete:Number(profiles?.count??0)>0,detail:`${Number(profiles?.count??0)} effective independently approved profile(s)`},
      {id:'approvers',label:'Independent Finance preparer and approver',complete:authority.independent,detail:`${authority.preparerCount} preparer(s), ${authority.approverCount} approver(s) with current Company authority`},
      { id: "periods", label: "Accounting periods", complete: Number(periods?.count ?? 0) > 0, detail: `${Number(periods?.count ?? 0)} period(s)` },
      { id: "tax", label: "Tax codes", complete: Number(taxes?.count ?? 0) > 0, detail: `${Number(taxes?.count ?? 0)} active code(s)` },
      { id: "bank", label: "Bank accounts", complete: Number(bankStatements?.count ?? 0) > 0, detail: `${Number(bankStatements?.count ?? 0)} account(s) from statements` },
    ],
  };
}
