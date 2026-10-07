import {z} from 'zod';
import type {WorkContext} from '@/server/modules/identity_access/public/authorization';
import {financeAiPrepareSchema} from '../domain/ai-contracts';
import {financeAiScope,type FinanceAiInput,type FinanceAiSnapshot,type FinanceAiSource} from '../domain/ai-source';
import type {FinanceError} from '../domain/invoice-line-revision';
type Row=Record<string,string|number|null>;
export type FinanceAiClosePeriod={id:string;name:string;start_date:string;end_date:string;state:string;blockers:unknown;draft_journals:unknown;draft_invoices:unknown;unmatched_bank:unknown;pending_tax:unknown;reconciliations:unknown;vat_reconciliation:unknown};
export interface FinanceAiSourcePorts{error:FinanceError;permission(actor:WorkContext,permission:'finance.read'):WorkContext;businessDate(timezone:string):string;hash(value:string):Promise<string>;revision(companyId:string):Promise<number>;workbench(actor:WorkContext,day:string):Promise<{attention:{id:string;label:string;count:number;amounts?:unknown[];href:string}[]}>;invoice(actor:WorkContext,id:string):Promise<{invoice:Row}>;invoiceChart(actor:WorkContext,accountId:string|number|null):Promise<Row|null>;invoiceAccounts(actor:WorkContext,chartId:string|number|null,currency:string|number|null):Promise<Row[]>;bankMovement(actor:WorkContext,id:string):Promise<Row|null>;compatiblePayments(actor:WorkContext,movement:Row):Promise<Row[]>;closeWorkspace(actor:WorkContext):Promise<FinanceAiClosePeriod[]>;periodVersion(actor:WorkContext,id:string):Promise<Row|null>;vatWorkspace(actor:WorkContext,input:{chartId?:string;filingMonth:string}):Promise<{chartId:string;filingMonth:string;profiles:Row[];reconciliation:unknown;entries:Row[]}>;reportChart(actor:WorkContext,id:string):Promise<Row|null>;statements(actor:WorkContext,periodId:string|undefined,chartId:string,dates:{startDate?:string;endDate?:string}):Promise<{dates:{startDate?:string;endDate?:string};balanceSheet:unknown;profitAndLoss:unknown;trialBalance:unknown}>;}
export function createFinanceAiSource(ports:FinanceAiSourcePorts){
function pick(row: Row, keys: string[]) { return Object.fromEntries(keys.map(key => [key, row[key] ?? null])); }
async function financeAiSnapshot(actor: WorkContext, raw: FinanceAiInput, timezone: string): Promise<FinanceAiSnapshot> {
  actor = financeAiScope(actor,ports.permission,ports.error);
  const input = financeAiPrepareSchema.parse(raw), sources: FinanceAiSource[] = [];
  const companyRevision = await ports.revision(actor.companyId);
  let version = companyRevision + 1, href = "finance", accounts: Row[] = [], bankPaymentIds: string[] = [];
  if (input.kind === "today_brief") {
    const day = z.string().date().parse(input.sourceId);
    if (day !== ports.businessDate(timezone)) throw ports.error(409, "AI_STALE", "Prepare the brief for the current Company business date.");
    const work = await ports.workbench(actor,day);
    sources.push({ id: `company-day:${day}`, label: `Company status as of ${day}`, data: { businessDate: day, timezone, attention: work.attention.map(item => ({ id: item.id, label: item.label, count: item.count, amounts: item.amounts ?? [], href: item.href })) } });
  } else if (["invoice_review", "collection_draft", "payment_preparation"].includes(input.kind)) {
    const { invoice } = await ports.invoice(actor,input.sourceId);
    version = Number(invoice.version);
    if (input.kind === "collection_draft" && (invoice.kind !== "receivable" || !["posted", "partially_paid"].includes(String(invoice.status)) || Number(invoice.outstanding_minor) <= 0)) throw ports.error(409, "AI_STAGE_CHANGED", "Collection drafts require an open posted customer invoice.");
    if (input.kind === "payment_preparation" && (invoice.kind !== "payable" || !["posted", "partially_paid"].includes(String(invoice.status)) || Number(invoice.outstanding_minor) <= 0)) throw ports.error(409, "AI_STAGE_CHANGED", "Payment preparation requires an open posted supplier invoice.");
    const chart = await ports.invoiceChart(actor,invoice.debit_account_id);
    if (!chart) throw ports.error(409, "AI_STAGE_CHANGED", "The originating chart is unavailable.");
    if (input.kind === "invoice_review") accounts = await ports.invoiceAccounts(actor,chart.id,invoice.currency);
    sources.push({ id: `invoice:${invoice.id}`, label: String(invoice.invoice_number), data: { ...pick(invoice, ["id", "version", "kind", "invoice_number", "party_id", "party_name", "tax_code", "invoice_date", "due_date", "currency", "subtotal_minor", "tax_minor", "total_minor", "outstanding_minor", "status", "source_type", "source_id", "debit_account_id", "credit_account_id", "tax_account_id"]), chart, availableAccounts: accounts, note: "Amounts are deterministic source facts. Attachment contents have not been read. VAT entitlement requires independent review." } });
    href = `finance/invoices/${encodeURIComponent(input.sourceId)}?kind=${invoice.kind}`;
  } else if (input.kind === "bank_match") {
    const transaction = await ports.bankMovement(actor,input.sourceId);
    if (!transaction) throw ports.error(404, "BANK_TRANSACTION_NOT_FOUND", "Bank transaction not found in this Company.");
    if (transaction.match_status === "matched") throw ports.error(409, "AI_STAGE_CHANGED", "This movement is already reconciled.");
    version = Number(transaction.version);
    const candidates = await ports.compatiblePayments(actor,transaction);
    bankPaymentIds = candidates.map(row => String(row.id));
    sources.push({ id: `bank:${input.sourceId}`, label: String(transaction.external_id), data: { ...pick(transaction, ["id", "version", "external_id", "description", "transaction_date", "currency", "amount_minor", "bank_account", "statement_number", "match_status"]), compatiblePayments: candidates, compatibilityRule: "Exact 1:1 posted payment with the same bank account, currency and signed amount. Human reconciliation and evidence remain required." } });
    href = `finance/bank?match=${encodeURIComponent(input.sourceId)}`;
  } else if (input.kind === "close_review" || input.kind === "tax_review") {
    const period = (await ports.closeWorkspace(actor)).find(row => row.id === input.sourceId);
    if (!period) throw ports.error(404, "PERIOD_NOT_FOUND", "Accounting period not found in this Company.");
    const row = await ports.periodVersion(actor,input.sourceId);
    version = Number(row?.close_version ?? 0) + 1;
    const periodData = { id: period.id, startDate: period.start_date, endDate: period.end_date, state: period.state, blockers: period.blockers, draftJournals: period.draft_journals, draftInvoices: period.draft_invoices, unmatchedBank: period.unmatched_bank, pendingTax: period.pending_tax, reconciliations: period.reconciliations, vatReconciliation: period.vat_reconciliation };
    sources.push({ id: `period:${period.id}`, label: period.name, data: periodData });
    if (input.kind === "tax_review") {
      const tax = await ports.vatWorkspace(actor, {chartId:input.chartId,filingMonth:input.filingMonth??period.end_date.slice(0,7)});
      if (!tax.profiles.some(profile => profile.statutory_chart_id === tax.chartId)) throw ports.error(409, "STATUTORY_PROFILE_REQUIRED", "Tax suggestions require an approved statutory accounting profile.");
      sources.push({ id: `vat:${period.id}`, label: `Reviewed statutory VAT sources · ${tax.filingMonth}`, data: { chartId: tax.chartId, filingMonth: tax.filingMonth, reconciliation: tax.reconciliation, entries: tax.entries.map(entry => pick(entry, ["id", "version", "document_reference", "document_date", "accounting_date", "tax_point_date", "claim_month", "treatment", "status", "taxable_minor", "tax_minor", "deductible_minor"])), note: "Do not infer NPAEs eligibility, VAT entitlement or filing readiness. This is an evidence review, not tax advice or a filing." } });
      href = `finance/tax?chart=${encodeURIComponent(tax.chartId)}&filingMonth=${tax.filingMonth}`;
    } else href = `finance/close?periodId=${encodeURIComponent(input.sourceId)}`;
  } else {
    const chart = await ports.reportChart(actor,input.sourceId);
    if (!chart) throw ports.error(404, "CHART_NOT_FOUND", "Chart not found in this Company.");
    const report = await ports.statements(actor,input.periodId,input.sourceId,{startDate:input.startDate,endDate:input.endDate});
    const {startDate,endDate}=report.dates;
    sources.push({ id: `report:${input.sourceId}`, label: `${chart.name} · ${startDate??"Beginning"} to ${endDate??"All posted dates"}`, data: { chart, periodId:input.periodId??null, startDate:startDate??null, endDate:endDate??null, balanceSheet: report.balanceSheet, profitAndLoss: report.profitAndLoss, trialBalance: report.trialBalance, note: "These are current category summaries, not a certified complete statutory statement set. Keep amounts in the chart functional currency and cite the supplied totals." } });
    href = `finance/reports?${new URLSearchParams({ chart: input.sourceId, ...(input.periodId?{periodId:input.periodId}:{}),...(startDate?{startDate}:{}),...(endDate?{endDate}:{}) })}`;
  }
  const hash = await ports.hash(JSON.stringify({ input: { ...input, retry: false }, companyId: actor.companyId, version, companyRevision, sources }));
  return { version, companyRevision, hash, sources, href, accounts, bankPaymentIds, input: { ...input, retry: false } };
}

return {financeAiSnapshot};}
