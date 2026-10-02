import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import { ApiError } from "./http";
import { requirePermission } from "./iam-permissions";
import { sha256 } from "./crypto";
import { getInvoice, statements } from "./finance";
import { closeWorkspace, financeWorkbench } from "./finance-workspace";
import { vatWorkspace } from "./finance-tax-register";
import { financeAiDate, financeAiPrepareSchema, financeAiResultSchema, type FinanceAiResult } from "@/lib/finance-ai";

export type FinanceAiInput = z.infer<typeof financeAiPrepareSchema>;
export type FinanceAiSource = { id: string; label: string; data: Record<string, unknown> };
export type FinanceAiSnapshot = { version: number; companyRevision: number; hash: string; sources: FinanceAiSource[]; href: string; accounts: Record<string, unknown>[]; bankPaymentIds: string[]; input: FinanceAiInput };
type Row = Record<string, string | number | null>;

export function requireFinanceAiScope(actor: WorkContext) {
  actor = requirePermission(actor, "finance.read");
  if (actor.beta || actor.branchId || actor.departmentId || actor.assignmentScope !== "holding") throw new ApiError(403, "FINANCE_AI_SCOPE_REQUIRED", "Finance AI requires an authorized Company assignment. Continue in the owning workflow at branch or department scope.");
  return actor;
}

// Owner commands write their source changes and authority-stamped receipt together.
// This also invalidates aggregate explanations after an upstream PMS/Commerce change.
export const financeRevisionSql = `(SELECT count(*) FROM command_receipts WHERE authority_json::jsonb->>'companyId'=? AND (operation LIKE 'finance.%' OR operation LIKE 'commerce.%' OR operation LIKE 'expense.%' OR operation LIKE 'pms.%' OR operation LIKE 'people.payroll.%') AND operation NOT LIKE 'finance.ai.%' AND operation NOT LIKE 'expense.ai.%')`;
export async function financeAiRevision(db: SqlDatabase, companyId: string) { return Number(await db.prepare(`SELECT ${financeRevisionSql} AS revision`).bind(companyId).first("revision") ?? 0); }

function pick(row: Row, keys: string[]) { return Object.fromEntries(keys.map(key => [key, row[key] ?? null])); }
export async function financeAiSnapshot(db: SqlDatabase, actor: WorkContext, raw: FinanceAiInput, timezone: string): Promise<FinanceAiSnapshot> {
  actor = requireFinanceAiScope(actor);
  const input = financeAiPrepareSchema.parse(raw), sources: FinanceAiSource[] = [];
  const companyRevision = await financeAiRevision(db, actor.companyId);
  let version = companyRevision + 1, href = "finance", accounts: Row[] = [], bankPaymentIds: string[] = [];
  if (input.kind === "today_brief") {
    const day = z.string().date().parse(input.sourceId);
    if (day !== financeAiDate(timezone)) throw new ApiError(409, "AI_STALE", "Prepare the brief for the current Company business date.");
    const work = await financeWorkbench(actor, day);
    sources.push({ id: `company-day:${day}`, label: `Company status as of ${day}`, data: { businessDate: day, timezone, attention: work.attention.map(item => ({ id: item.id, label: item.label, count: item.count, amounts: item.amounts ?? [], href: item.href })) } });
  } else if (["invoice_review", "collection_draft", "payment_preparation"].includes(input.kind)) {
    const { invoice } = await getInvoice(actor, input.sourceId);
    version = Number(invoice.version);
    if (input.kind === "collection_draft" && (invoice.kind !== "receivable" || !["posted", "partially_paid"].includes(String(invoice.status)) || Number(invoice.outstanding_minor) <= 0)) throw new ApiError(409, "AI_STAGE_CHANGED", "Collection drafts require an open posted customer invoice.");
    if (input.kind === "payment_preparation" && (invoice.kind !== "payable" || !["posted", "partially_paid"].includes(String(invoice.status)) || Number(invoice.outstanding_minor) <= 0)) throw new ApiError(409, "AI_STAGE_CHANGED", "Payment preparation requires an open posted supplier invoice.");
    const chart = await db.prepare("SELECT c.id,c.name,c.base_currency FROM finance_charts c JOIN finance_accounts a ON a.chart_id=c.id AND a.company_id=c.company_id WHERE a.id=? AND c.company_id=?").bind(invoice.debit_account_id, actor.companyId).first<Row>();
    if (!chart) throw new ApiError(409, "AI_STAGE_CHANGED", "The originating chart is unavailable.");
    if (input.kind === "invoice_review") accounts = (await db.prepare("SELECT id,code,name,category,currency,control_kind FROM finance_accounts WHERE company_id=? AND chart_id=? AND currency=? AND status='active' ORDER BY code LIMIT 200").bind(actor.companyId, chart.id, invoice.currency).all<Row>()).results;
    sources.push({ id: `invoice:${invoice.id}`, label: String(invoice.invoice_number), data: { ...pick(invoice, ["id", "version", "kind", "invoice_number", "party_id", "party_name", "tax_code", "invoice_date", "due_date", "currency", "subtotal_minor", "tax_minor", "total_minor", "outstanding_minor", "status", "source_type", "source_id", "debit_account_id", "credit_account_id", "tax_account_id"]), chart, availableAccounts: accounts, note: "Amounts are deterministic source facts. Attachment contents have not been read. VAT entitlement requires independent review." } });
    href = `finance/invoices/${encodeURIComponent(input.sourceId)}?kind=${invoice.kind}`;
  } else if (input.kind === "bank_match") {
    const transaction = await db.prepare("SELECT t.*,s.bank_account,s.statement_number FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id AND s.company_id=t.company_id WHERE t.id=? AND t.company_id=?").bind(input.sourceId, actor.companyId).first<Row>();
    if (!transaction) throw new ApiError(404, "BANK_TRANSACTION_NOT_FOUND", "Bank transaction not found in this Company.");
    if (transaction.match_status === "matched") throw new ApiError(409, "AI_STAGE_CHANGED", "This movement is already reconciled.");
    version = Number(transaction.version);
    const candidates = (await db.prepare(`SELECT p.id,p.version,p.payment_number,p.payment_date,p.currency,p.amount_minor,p.kind,p.bank_account FROM finance_payments p JOIN finance_journals j ON j.id=p.journal_id AND j.company_id=p.company_id WHERE p.company_id=? AND p.currency=? AND p.bank_account=? AND p.status IN ('released','settled') AND j.status='posted' AND CASE WHEN p.kind='receipt' THEN p.amount_minor ELSE -p.amount_minor END=? AND NOT EXISTS(SELECT 1 FROM finance_bank_transactions t WHERE t.company_id=p.company_id AND t.matched_type='payment' AND t.matched_id=p.id AND t.match_status='matched') ORDER BY p.payment_date,p.id LIMIT 100`).bind(actor.companyId, transaction.currency, transaction.bank_account, transaction.amount_minor).all<Row>()).results;
    bankPaymentIds = candidates.map(row => String(row.id));
    sources.push({ id: `bank:${input.sourceId}`, label: String(transaction.external_id), data: { ...pick(transaction, ["id", "version", "external_id", "description", "transaction_date", "currency", "amount_minor", "bank_account", "statement_number", "match_status"]), compatiblePayments: candidates, compatibilityRule: "Exact 1:1 posted payment with the same bank account, currency and signed amount. Human reconciliation and evidence remain required." } });
    href = `finance/bank?match=${encodeURIComponent(input.sourceId)}`;
  } else if (input.kind === "close_review" || input.kind === "tax_review") {
    const period = (await closeWorkspace(actor)).find(row => row.id === input.sourceId);
    if (!period) throw new ApiError(404, "PERIOD_NOT_FOUND", "Accounting period not found in this Company.");
    const row = await db.prepare("SELECT close_version FROM finance_periods WHERE company_id=? AND id=?").bind(actor.companyId, input.sourceId).first<Row>();
    version = Number(row?.close_version ?? 0) + 1;
    const periodData = { id: period.id, startDate: period.start_date, endDate: period.end_date, state: period.state, blockers: period.blockers, draftJournals: period.draft_journals, draftInvoices: period.draft_invoices, unmatchedBank: period.unmatched_bank, pendingTax: period.pending_tax, reconciliations: period.reconciliations, vatReconciliation: period.vat_reconciliation };
    sources.push({ id: `period:${period.id}`, label: period.name, data: periodData });
    if (input.kind === "tax_review") {
      const tax = await vatWorkspace(actor, { chartId:input.chartId,filingMonth:input.filingMonth??period.end_date.slice(0, 7) });
      if (!tax.profiles.some(profile => profile.statutory_chart_id === tax.chartId)) throw new ApiError(409, "STATUTORY_PROFILE_REQUIRED", "Tax suggestions require an approved statutory accounting profile.");
      sources.push({ id: `vat:${period.id}`, label: `Reviewed statutory VAT sources · ${tax.filingMonth}`, data: { chartId: tax.chartId, filingMonth: tax.filingMonth, reconciliation: tax.reconciliation, entries: tax.entries.map(entry => pick(entry, ["id", "version", "document_reference", "document_date", "accounting_date", "tax_point_date", "claim_month", "treatment", "status", "taxable_minor", "tax_minor", "deductible_minor"])), note: "Do not infer NPAEs eligibility, VAT entitlement or filing readiness. This is an evidence review, not tax advice or a filing." } });
      href = `finance/tax?chart=${encodeURIComponent(tax.chartId)}&filingMonth=${tax.filingMonth}`;
    } else href = `finance/close?periodId=${encodeURIComponent(input.sourceId)}`;
  } else {
    const chart = await db.prepare("SELECT id,name,base_currency,accounting_basis FROM finance_charts WHERE id=? AND company_id=?").bind(input.sourceId, actor.companyId).first<Row>();
    if (!chart) throw new ApiError(404, "CHART_NOT_FOUND", "Chart not found in this Company.");
    const report = await statements(actor, input.periodId, input.sourceId, { startDate:input.startDate,endDate:input.endDate });
    const {startDate,endDate}=report.dates;
    sources.push({ id: `report:${input.sourceId}`, label: `${chart.name} · ${startDate??"Beginning"} to ${endDate??"All posted dates"}`, data: { chart, periodId:input.periodId??null, startDate:startDate??null, endDate:endDate??null, balanceSheet: report.balanceSheet, profitAndLoss: report.profitAndLoss, trialBalance: report.trialBalance, note: "These are current category summaries, not a certified complete statutory statement set. Keep amounts in the chart functional currency and cite the supplied totals." } });
    href = `finance/reports?${new URLSearchParams({ chart: input.sourceId, ...(input.periodId?{periodId:input.periodId}:{}),...(startDate?{startDate}:{}),...(endDate?{endDate}:{}) })}`;
  }
  const hash = await sha256(JSON.stringify({ input: { ...input, retry: false }, companyId: actor.companyId, version, companyRevision, sources }));
  return { version, companyRevision, hash, sources, href, accounts, bankPaymentIds, input: { ...input, retry: false } };
}

/** Reject invented evidence and unsupported financial mutations before publishing. */
export function validateFinanceAiResult(raw: unknown, snapshot: FinanceAiSnapshot): FinanceAiResult {
  const result = financeAiResultSchema.parse(raw);
  for (const citation of result.citations) {
    const source = snapshot.sources.find(row => row.id === citation.sourceId);
    if (!source || !Object.hasOwn(source.data, citation.field)) throw new ApiError(422, "AI_OUTPUT_INVALID", "AI referenced evidence outside the supplied source facts.");
  }
  if (result.bankPaymentId && (snapshot.input.kind !== "bank_match" || !snapshot.bankPaymentIds.includes(result.bankPaymentId))) throw new ApiError(422, "AI_OUTPUT_INVALID", "AI selected an incompatible bank payment.");
  if (result.accounts) {
    const ids = [result.accounts.debitAccountId, result.accounts.creditAccountId, ...(result.accounts.taxAccountId ? [result.accounts.taxAccountId] : [])];
    if (Number(snapshot.sources[0]?.data.tax_minor ?? 0) > 0 && !result.accounts.taxAccountId) throw new ApiError(422, "AI_OUTPUT_INVALID", "A taxed invoice requires a distinct tax posting account.");
    if (snapshot.input.kind !== "invoice_review" || new Set(ids).size !== ids.length || ids.some(id => !snapshot.accounts.some(account => account.id === id))) throw new ApiError(422, "AI_OUTPUT_INVALID", "AI suggested unavailable or conflicting posting accounts.");
  }
  return result;
}
