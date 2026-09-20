import { readListPage } from "./list-page";
import { requirePermission } from "@/server/iam-permissions";
import { samePerson } from "@/lib/iam";
import { hasPermission } from "@/lib/permissions";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { z } from "zod";
import { assertion,executeCommand,replayCommand } from "./commands";
import type { WorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { ApiError } from "@/server/http";
import { decimalToMinor as parseMinor, minorToDecimal } from "@/lib/finance-money";

/** Money crossing an API boundary is a decimal string. Storage uses integer minor units. */
export const moneySchema = z.string().regex(/^-?\d+(\.\d{1,6})?$/, "Use a decimal money amount.");

export function decimalToMinor(value: string, scale = 2): number {
  try { return parseMinor(value, scale); }
  catch (error) {
    const message = error instanceof Error ? error.message : "Invalid money.";
    throw new ApiError(422, message.includes("decimal") ? "INVALID_MONEY" : message.includes("places") ? "MONEY_SCALE" : "MONEY_OVERFLOW", message);
  }
}
export { minorToDecimal } from "@/lib/finance-money";

export type JournalLineInput = {
  accountId: string;
  description?: string;
  debit: string;
  credit: string;
  dimensions?: Record<string, string>;
};

const lineSchema = z.object({
  accountId: z.string().min(1).max(128),
  description: z.string().max(500).optional(),
  debit: moneySchema,
  credit: moneySchema,
  dimensions: z.record(z.string().max(80), z.string().max(160)).optional(),
});

export const createJournalSchema = z.object({
  chartId: z.string().min(1).max(128),
  periodId: z.string().min(1).max(128),
  entryDate: z.string().date(),
  currency: z.string().length(3).transform((value) => value.toUpperCase()),
  fxRate: z.string().optional(),
  memo: z.string().max(2000).default(""),
  sourceType: z.string().max(64).default("manual"),
  sourceId: z.string().max(128).optional(),
  isOpening: z.boolean().default(false),
  lines: z.array(lineSchema).min(2).max(500),
});

export const openingTrialBalanceSchema = z.object({
  chartId: z.string().min(1).max(128),
  periodId: z.string().min(1).max(128),
  entryDate: z.string().date(),
  currency: z.string().length(3).transform((value) => value.toUpperCase()),
  lines: z.array(lineSchema.pick({ accountId: true, description: true, debit: true, credit: true })).min(2).max(500),
});

export const createChartSchema = z.object({
  name: z.string().trim().min(1).max(160),
  baseCurrency: z.string().length(3).transform((value) => value.toUpperCase()),
  accountingBasis: z.string().trim().max(40).default("IFRS"),
});

export const createAccountSchema = z.object({
  chartId: z.string().min(1),
  code: z.string().trim().min(1).max(40),
  name: z.string().trim().min(1).max(160),
  category: z.enum(["asset", "liability", "equity", "revenue", "expense", "other_income", "other_expense"]),
  parentId: z.string().max(128).nullable().optional(),
  currency: z.string().length(3).transform((value) => value.toUpperCase()).optional(),
  isControl: z.boolean().default(false),
});

export const createPeriodSchema = z.object({
  name: z.string().trim().min(1).max(100),
  startDate: z.string().date(),
  endDate: z.string().date(),
});

export const closeProofSchema = z.object({
  proof: z.string().trim().min(1).max(2000),
  expectedVersion: z.number().int().nonnegative().optional(),
});

type FinanceRow = Record<string, string | number | null>;

function requireFinanceRole(actor: WorkContext, approver = false) {
  if (!hasPermission(actor,"finance.read")) throw new ApiError(403, "FINANCE_FORBIDDEN", "A finance assignment is required.");
  if (approver && !hasPermission(actor,"finance.approve"))
    throw new ApiError(403, "FINANCE_APPROVAL_REQUIRED", "An independent finance approver is required.");
}

async function hash(value:unknown) {
 const bytes=await crypto.subtle.digest("SHA-256",new TextEncoder().encode(JSON.stringify(value)));
 return Array.from(new Uint8Array(bytes),b=>b.toString(16).padStart(2,"0")).join("");
}
async function priorCommand(write:SqlDatabase,actor:WorkContext,id:string,operation:string,input:unknown) {
  const receipt=await write.prepare("SELECT record_id FROM command_receipts WHERE command_id=?").bind(id).first<{record_id:string}>();
  if(receipt)return replayCommand(write,actor,id,operation,receipt.record_id,input);
  if(await write.prepare("SELECT 1 FROM finance_commands WHERE command_id=?").bind(id).first())throw new ApiError(409,"IDEMPOTENCY_CONFLICT","This legacy command has no recorded work context. Review its result before starting a new command.");
  return null;
}
async function saveCommand(write:SqlDatabase,actor:WorkContext,id:string,operation:string,input:unknown,result:Record<string,unknown>,statements:SqlStatement[]) {
 return executeCommand(write,actor,id,operation,input,String(result.id??result.chartId??id),Number(result.version??1),result,statements);
}

async function resources(actor: WorkContext,readOnly=false) {
  requireFinanceRole(actor);
  if(!hasPermission(actor,"finance.write")&&!readOnly)throw new ApiError(403,"READ_ONLY_ROLE","Auditors have read-only access to finance.");
  return holdingResources(actor.holdingId);
}

async function ensurePeriod(write: SqlDatabase, actor: WorkContext, periodId: string, entryDate: string, allowClosing = false) {
  const period = await write.prepare("SELECT * FROM finance_periods WHERE id=? AND company_id=?").bind(periodId, actor.companyId).first<FinanceRow>();
  if (!period) throw new ApiError(404, "PERIOD_NOT_FOUND", "Accounting period not found.");
  if (entryDate < String(period.start_date) || entryDate > String(period.end_date)) throw new ApiError(422, "DATE_OUTSIDE_PERIOD", "Entry date is outside the accounting period.");
  if (period.state === "closed" || (!allowClosing && period.state === "closing")) throw new ApiError(409, "PERIOD_CLOSED", "The accounting period does not accept postings.");
  return period;
}

export async function listCharts(actor: WorkContext) {
  actor = requirePermission(actor,"finance.read");

  const { write } = await resources(actor,true);
  return (await write.prepare("SELECT * FROM finance_charts WHERE company_id=? ORDER BY name").bind(actor.companyId).all()).results;
}

export async function createChart(actor: WorkContext, input: z.infer<typeof createChartSchema>, id: string) {
  actor = requirePermission(actor,"finance.write");

  const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.chart.create", input); if (previous) return previous;
  const chartId = `chart-${crypto.randomUUID()}`; const now = Date.now();
  pendingStatements.push(
    write.prepare("INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)").bind(chartId, actor.companyId, input.name, input.baseCurrency, input.accountingBasis, actor.userId, now, now),
  );
  const result = { id: chartId, ...input, companyId: actor.companyId, version: 1 };
  return saveCommand(write, actor, id, "finance.chart.create", input, result, pendingStatements);
}

export async function listAccounts(actor: WorkContext, chartId: string) {
  actor = requirePermission(actor,"finance.read");

  const { write } = await resources(actor,true);
  return (await write.prepare("SELECT * FROM finance_accounts WHERE company_id=? AND chart_id=? ORDER BY code").bind(actor.companyId, chartId).all()).results;
}

export async function createAccount(actor: WorkContext, input: z.infer<typeof createAccountSchema>, id: string) {
  actor = requirePermission(actor,"finance.write");

  const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.account.create", input); if (previous) return previous;
  const chart = await write.prepare("SELECT id,base_currency FROM finance_charts WHERE id=? AND company_id=?").bind(input.chartId, actor.companyId).first<{ id: string; base_currency: string }>();
  if (!chart) throw new ApiError(404, "CHART_NOT_FOUND", "Chart not found.");
  const accountId = `acct-${crypto.randomUUID()}`; const now = Date.now();
  pendingStatements.push(write.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,parent_id,currency,is_control,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
    .bind(accountId, input.chartId, actor.companyId, input.code, input.name, input.category, input.parentId ?? null, input.currency ?? chart.base_currency, input.isControl ? 1 : 0, now, now));
  const result = { id: accountId, ...input, companyId: actor.companyId, currency: input.currency ?? chart.base_currency, version: 1 };
  return saveCommand(write, actor, id, "finance.account.create", input, result, pendingStatements);
}

export async function importAccounts(actor: WorkContext, chartId: string, rows: Array<{ code: string; name: string; category: z.infer<typeof createAccountSchema>["category"]; currency?: string }>, id: string) {
  actor = requirePermission(actor,"finance.write");

  const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const input = { chartId, rows }; const previous = await priorCommand(write, actor, id, "finance.account.import", input); if (previous) return previous;
  const chart = await write.prepare("SELECT base_currency FROM finance_charts WHERE id=? AND company_id=?").bind(chartId, actor.companyId).first<{ base_currency: string }>(); if (!chart) throw new ApiError(404, "CHART_NOT_FOUND", "Chart not found.");
  const now = Date.now(); const statements = rows.map((row) => write.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(chart_id,code) DO UPDATE SET name=excluded.name,category=excluded.category,currency=excluded.currency,updated_at=excluded.updated_at").bind(`acct-${crypto.randomUUID()}`, chartId, actor.companyId, row.code, row.name, row.category, row.currency ?? chart.base_currency, now, now));
  if (!rows.length) throw new ApiError(422, "EMPTY_IMPORT", "Provide at least one account.");
  pendingStatements.push(...statements); const result = { imported: rows.length, chartId }; return saveCommand(write, actor, id, "finance.account.import", input, result, pendingStatements);
}

export async function createPeriod(actor: WorkContext, input: z.infer<typeof createPeriodSchema>, id: string) {
  actor = requirePermission(actor,"finance.write");

  const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.period.create", input); if (previous) return previous;
  if (input.endDate < input.startDate) throw new ApiError(422, "INVALID_PERIOD", "Period end must be on or after its start.");
  const periodId = `period-${crypto.randomUUID()}`; const now = Date.now();
  pendingStatements.push(write.prepare("INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES(?,?,?,?,?,?)").bind(periodId, actor.companyId, input.name, input.startDate, input.endDate, now));
  const result = { id: periodId, ...input, state: "open", companyId: actor.companyId, version: 0 }; return saveCommand(write, actor, id, "finance.period.create", input, result, pendingStatements);
}

export async function prepareJournal(write:SqlDatabase,actor:WorkContext,input:z.infer<typeof createJournalSchema>,key:string,status:"draft"|"posted"="draft") {
  const chart=await write.prepare("SELECT id,base_currency FROM finance_charts WHERE id=? AND company_id=? AND status='active'").bind(input.chartId,actor.companyId).first<{id:string;base_currency:string}>();if(!chart)throw new ApiError(404,"CHART_NOT_FOUND","Active chart not found.");
  await ensurePeriod(write,actor,input.periodId,input.entryDate);
  const fx=input.currency===chart.base_currency?1000000:input.fxRate?decimalToMinor(input.fxRate,6):0;
  if(fx<1||fx>1000000000000)throw new ApiError(422,"FX_RATE_REQUIRED","Provide a positive exchange rate for this foreign-currency journal.");
  const convert=(amount:number)=>{const n=(BigInt(amount)*BigInt(fx)+BigInt(500000))/BigInt(1000000);if(n>BigInt(Number.MAX_SAFE_INTEGER))throw new ApiError(422,"MONEY_OVERFLOW","The converted amount exceeds supported precision.");return Number(n);};
  const values=input.lines.map(l=>{const debit=decimalToMinor(l.debit),credit=decimalToMinor(l.credit);if(debit<0||credit<0||(debit>0)===(credit>0))throw new ApiError(422,"INVALID_JOURNAL_LINE","Each line requires one positive debit or credit.");return {...l,debit,credit,baseDebit:convert(debit),baseCredit:convert(credit)};});
  const total=(field:"debit"|"credit"|"baseDebit"|"baseCredit")=>values.reduce((sum,l)=>sum+BigInt(l[field]),BigInt(0));
  if(total("debit")!==total("credit")||total("baseDebit")!==total("baseCredit"))throw new ApiError(422,"UNBALANCED_JOURNAL","Debits and credits must balance in both transaction and functional currency. Review exchange rounding.");
  if(total("debit")>BigInt(Number.MAX_SAFE_INTEGER))throw new ApiError(422,"MONEY_OVERFLOW","The journal total exceeds supported precision.");
  const accountIds=[...new Set(values.map(l=>l.accountId))];
  const valid=await write.prepare(`SELECT id FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND id IN (${accountIds.map(()=>"?").join(",")})`).bind(actor.companyId,input.chartId,...accountIds).all();if(valid.results.length!==accountIds.length)throw new ApiError(422,"ACCOUNT_INVALID","Every posting account must be active and belong to this chart.");
  const journalId=`journal-${key}`,number=`J-${key}`,now=Date.now();
  if (input.isOpening) {
    const existing = await write.prepare("SELECT id FROM finance_journals WHERE company_id=? AND chart_id=? AND is_opening=1 AND status IN ('draft','in_review','posted') LIMIT 1").bind(actor.companyId,input.chartId).first();
    if (existing) throw new ApiError(409,"OPENING_BALANCE_EXISTS","Reverse or finish the existing opening balance before importing another one.");
  }
  const steps:SqlStatement[]=[
    assertion(write,key,"posting-period","EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)",[input.periodId,actor.companyId,input.entryDate,input.entryDate]),
    assertion(write,key,"posting-chart","EXISTS(SELECT 1 FROM finance_charts WHERE id=? AND company_id=? AND status='active')",[input.chartId,actor.companyId]),
    ...(input.isOpening ? [assertion(write,key,"opening-balance","NOT EXISTS(SELECT 1 FROM finance_journals WHERE company_id=? AND chart_id=? AND is_opening=1 AND status IN ('draft','in_review','posted'))",[actor.companyId,input.chartId])] : []),
    ...accountIds.map((accountId,index)=>assertion(write,key,`posting-account-${index}`,"EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND status='active')",[accountId,actor.companyId,input.chartId])),
    write.prepare("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,is_opening,status,created_by,posted_by,posted_at,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(journalId,actor.companyId,input.chartId,input.periodId,number,input.entryDate,input.currency,fx,input.sourceType,input.sourceId??null,input.memo,input.isOpening?1:0,status,actor.userId,status==="posted"?actor.userId:null,status==="posted"?now:null,now,now),
    ...values.map((l,index)=>write.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(`${journalId}-${index}`,journalId,l.accountId,l.description??"",l.debit,l.credit,l.baseDebit,l.baseCredit,JSON.stringify(l.dimensions??{}),index)),
  ];
  return {steps,result:{id:journalId,number,status,version:1,debit:minorToDecimal(Number(total("debit"))),credit:minorToDecimal(Number(total("credit")))}};
}
export async function createJournal(actor:WorkContext,input:z.infer<typeof createJournalSchema>,id:string) {
  actor = requirePermission(actor,"finance.write");

  const {write}=await resources(actor),journalId=`journal-${id}`;
  const previous=await replayCommand<{id:string;number:string;status:string;version:number;debit:string;credit:string}>(write,actor,id,"finance.journal.create",journalId,input);if(previous)return previous;
  const {steps,result}=await prepareJournal(write,actor,input,id);
  return executeCommand(write,actor,id,"finance.journal.create",input,journalId,1,result,steps);
}

export async function previewOpeningTrialBalance(actor: WorkContext, input: z.infer<typeof openingTrialBalanceSchema>) {
  actor = requirePermission(actor,"finance.write");
  const {write}=await resources(actor,true);
  const chart=await write.prepare("SELECT id,base_currency,updated_at FROM finance_charts WHERE id=? AND company_id=? AND status='active'").bind(input.chartId,actor.companyId).first<FinanceRow>();
  if(!chart)throw new ApiError(404,"CHART_NOT_FOUND","Active chart not found.");
  if(input.currency!==chart.base_currency)throw new ApiError(422,"OPENING_CURRENCY_MISMATCH","Opening balances must use the chart's functional currency.");
  const period=await ensurePeriod(write,actor,input.periodId,input.entryDate);
  const accountIds=[...new Set(input.lines.map(line=>line.accountId))];
  if(accountIds.length!==input.lines.length)throw new ApiError(422,"DUPLICATE_OPENING_ACCOUNT","Each account may appear once in an opening balance import.");
  const accounts=await write.prepare(`SELECT id,code,name,status,updated_at FROM finance_accounts WHERE company_id=? AND chart_id=? AND id IN (${accountIds.map(()=>"?").join(",")})`).bind(actor.companyId,input.chartId,...accountIds).all<FinanceRow>();
  if(accounts.results.length!==accountIds.length||accounts.results.some(account=>account.status!=="active"))throw new ApiError(422,"ACCOUNT_INVALID","Every opening balance account must be active and belong to this chart.");
  let debit=BigInt(0),credit=BigInt(0);
  const accountById=new Map(accounts.results.map(account=>[String(account.id),account]));
  const lines=input.lines.map(line=>{
    const debitMinor=decimalToMinor(line.debit),creditMinor=decimalToMinor(line.credit);
    if(debitMinor<0||creditMinor<0||(debitMinor>0)===(creditMinor>0))throw new ApiError(422,"INVALID_OPENING_LINE","Each opening line requires one positive debit or credit.");
    debit+=BigInt(debitMinor);credit+=BigInt(creditMinor);
    const account=accountById.get(line.accountId)!;
    return {...line,debit:minorToDecimal(debitMinor),credit:minorToDecimal(creditMinor),accountCode:String(account.code),accountName:String(account.name)};
  });
  if(debit!==credit)throw new ApiError(422,"UNBALANCED_OPENING_BALANCE","Opening debits and credits must balance.");
  if(debit>BigInt(Number.MAX_SAFE_INTEGER))throw new ApiError(422,"MONEY_OVERFLOW","The opening balance total exceeds supported precision.");
  const existing=await write.prepare("SELECT id FROM finance_journals WHERE company_id=? AND chart_id=? AND is_opening=1 AND status IN ('draft','in_review','posted') LIMIT 1").bind(actor.companyId,input.chartId).first();
  if(existing)throw new ApiError(409,"OPENING_BALANCE_EXISTS","Reverse or finish the existing opening balance before importing another one.");
  const previewToken=await hash({
    companyId:actor.companyId,
    input:{
      ...input,
      lines:input.lines.map(line=>({
        ...line,
        debit:minorToDecimal(decimalToMinor(line.debit)),
        credit:minorToDecimal(decimalToMinor(line.credit)),
      })),
    },
    chart:{id:chart.id,currency:chart.base_currency,updatedAt:chart.updated_at},
    period:{id:period.id,state:period.state,version:period.close_version},
    accounts:accounts.results
      .map(account=>({id:account.id,status:account.status,updatedAt:account.updated_at}))
      .sort((a,b)=>String(a.id).localeCompare(String(b.id))),
  });
  return {previewToken,chartId:input.chartId,periodId:input.periodId,entryDate:input.entryDate,currency:input.currency,debit:minorToDecimal(Number(debit)),credit:minorToDecimal(Number(credit)),lines};
}

export async function importOpeningTrialBalance(actor: WorkContext, input: z.infer<typeof openingTrialBalanceSchema> & {previewToken:string}, id: string) {
  const {previewToken,...definition}=input;
  const journalInput:z.infer<typeof createJournalSchema>={ ...definition, memo: "Verified opening trial balance", sourceType: "opening_import", sourceId:definition.chartId, isOpening: true, lines: definition.lines.map((line) => ({ accountId: line.accountId, debit: line.debit, credit: line.credit, description: line.description })) };
  const {write}=await resources(actor,true);
  if(await write.prepare("SELECT 1 FROM command_receipts WHERE command_id=?").bind(`${id}:journal`).first())return createJournal(actor,journalInput,`${id}:journal`);
  const preview=await previewOpeningTrialBalance(actor,definition);
  if(preview.previewToken!==previewToken)throw new ApiError(409,"OPENING_PREVIEW_STALE","The chart, period, accounts or opening balances changed. Preview the import again.");
  const journal = await createJournal(actor,journalInput,`${id}:journal`);
  return journal;
}

export async function listJournals(actor: WorkContext, query: { periodId?: string; status?: string }) {
  actor = requirePermission(actor,"finance.read");

  const { write } = await resources(actor,true); const clauses = ["company_id=?"]; const args: (string | number)[] = [actor.companyId]; if (query.periodId) { clauses.push("period_id=?"); args.push(query.periodId); } if (query.status) { clauses.push("status=?"); args.push(query.status); }
  return (await write.prepare(`SELECT * FROM finance_journals WHERE ${clauses.join(" AND ")} ORDER BY entry_date DESC,journal_number DESC LIMIT 500`).bind(...args).all()).results;
}

export async function listInvoices(actor: WorkContext, limit=100) {
  actor = requirePermission(actor,"finance.read");

  const { write } = await resources(actor,true);
  return (await write.prepare(`SELECT i.id,i.invoice_number,i.kind,i.invoice_date,i.due_date,i.currency,i.total_minor,i.outstanding_minor,i.status,p.name AS party_name
    FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id
    WHERE i.company_id=? ORDER BY i.invoice_date DESC,i.invoice_number DESC LIMIT ?`).bind(actor.companyId,Math.max(1,Math.min(limit,100))).all()).results;
}

export async function getInvoice(actor: WorkContext, invoiceId: string) {
  actor = requirePermission(actor,"finance.read");

  const { write } = await resources(actor,true);
  const invoice = await write.prepare(`SELECT i.*,p.code AS party_code,p.name AS party_name
    FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id
    WHERE i.id=? AND i.company_id=?`).bind(invoiceId, actor.companyId).first<FinanceRow>();
  if (!invoice) throw new ApiError(404, "INVOICE_NOT_FOUND", "Invoice not found in this company.");
  const allocations = await write.prepare(`SELECT a.id,a.amount_minor,a.created_at,p.payment_number,p.status,p.payment_slip_id
    FROM finance_allocations a JOIN finance_payments p ON p.id=a.payment_id
    WHERE a.invoice_id=? AND a.company_id=? ORDER BY a.created_at`).bind(invoiceId, actor.companyId).all();
  return { invoice, allocations: allocations.results };
}

export async function listPayments(actor: WorkContext, limit=100) {
  actor = requirePermission(actor,"finance.read");

  const { write } = await resources(actor,true);
  return (await write.prepare(`SELECT p.id,p.payment_number,p.kind,p.payment_date,p.currency,p.amount_minor,p.outstanding_minor,p.status,f.name AS party_name
    FROM finance_payments p LEFT JOIN finance_parties f ON f.id=p.party_id
    WHERE p.company_id=? ORDER BY p.payment_date DESC,p.payment_number DESC LIMIT ?`).bind(actor.companyId,Math.max(1,Math.min(limit,100))).all()).results;
}

export async function getPayment(actor: WorkContext, paymentId: string) {
  actor = requirePermission(actor,"finance.read");

  const { write } = await resources(actor,true);
  const payment = await write.prepare(`SELECT p.*,f.code AS party_code,f.name AS party_name
    FROM finance_payments p LEFT JOIN finance_parties f ON f.id=p.party_id
    WHERE p.id=? AND p.company_id=?`).bind(paymentId, actor.companyId).first<FinanceRow>();
  if (!payment) throw new ApiError(404, "PAYMENT_NOT_FOUND", "Payment not found in this company.");
  const allocations = await write.prepare(`SELECT a.id,a.amount_minor,a.created_at,i.invoice_number,i.status
    FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id
    WHERE a.payment_id=? AND a.company_id=? ORDER BY a.created_at`).bind(paymentId, actor.companyId).all();
  const [reversal,periods] = await Promise.all([
    write.prepare("SELECT * FROM finance_payment_reversals WHERE payment_id=?").bind(paymentId).first<FinanceRow>(),
    write.prepare("SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date DESC").bind(actor.companyId).all<FinanceRow>(),
  ]);
  return { payment, allocations: allocations.results, reversal, periods:periods.results };
}

export const reversePaymentSchema=z.object({
  expectedVersion:z.number().int().positive(),
  periodId:z.string().min(1),
  entryDate:z.string().date(),
  reason:z.string().trim().min(3).max(500),
});

export async function reversePayment(actor:WorkContext,paymentId:string,input:z.infer<typeof reversePaymentSchema>,id:string){
  actor=requirePermission(actor,"finance.write");requireFinanceRole(actor,true);
  const {write}=await resources(actor),previous=await replayCommand(write,actor,id,"finance.payment.reverse",paymentId,input);if(previous)return previous;
  const payment=await write.prepare("SELECT p.*,j.chart_id,j.status journal_status,j.journal_number FROM finance_payments p JOIN finance_journals j ON j.id=p.journal_id WHERE p.id=? AND p.company_id=?").bind(paymentId,actor.companyId).first<FinanceRow>();
  if(!payment)throw new ApiError(404,"POSTED_PAYMENT_NOT_FOUND","A posted payment with accounting evidence was not found.");
  if(!hasPermission(actor,"finance.approve")||samePerson(actor,payment.created_by))throw new ApiError(403,"INDEPENDENT_CORRECTION_REQUIRED","A different finance approver must reverse the payment.");
  if(payment.status!=="settled"||payment.journal_status!=="posted")throw new ApiError(409,"PAYMENT_NOT_REVERSIBLE","Only a settled payment with a posted journal can be reversed.");
  const allocations=await write.prepare("SELECT a.*,i.total_minor,i.outstanding_minor,i.source_type FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id WHERE a.payment_id=? AND a.company_id=? ORDER BY a.id").bind(paymentId,actor.companyId).all<FinanceRow>();
  if(!allocations.results.length)throw new ApiError(409,"PAYMENT_ALLOCATION_REQUIRED","Only an allocated payment can be corrected here.");
  if(allocations.results.some(row=>["expense_claim","pms"].includes(String(row.source_type))))throw new ApiError(409,"REVERSE_AT_SOURCE","Reverse this payment from its originating module.");
  const now=Date.now(),reversal=`payment-reversal-${id}`;
  const steps:SqlStatement[]=[
    assertion(write,id,"payment","EXISTS(SELECT 1 FROM finance_payments p JOIN finance_journals j ON j.id=p.journal_id WHERE p.id=? AND p.company_id=? AND p.version=? AND p.status='settled' AND j.status='posted')",[paymentId,actor.companyId,input.expectedVersion]),
    assertion(write,id,"not-reconciled","NOT EXISTS(SELECT 1 FROM finance_bank_transactions WHERE company_id=? AND matched_type='payment' AND matched_id=? AND match_status='matched')",[actor.companyId,paymentId]),
    assertion(write,id,"period","EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)",[input.periodId,actor.companyId,input.entryDate,input.entryDate]),
    assertion(write,id,"not-reversed","NOT EXISTS(SELECT 1 FROM finance_payment_reversals WHERE payment_id=?)",[paymentId]),
    write.prepare("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,chart_id,?,?,?,currency,fx_rate_micros,'payment_reversal',?,?,'posted',?,?,?,?,? FROM finance_journals WHERE id=?").bind(reversal,input.periodId,`PAY-REV-${id}`,input.entryDate,paymentId,input.reason,actor.userId,actor.userId,now,now,now,payment.journal_id),
    write.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?||'-'||ordinal,?,account_id,description,credit_minor,debit_minor,base_credit_minor,base_debit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=?").bind(reversal,reversal,payment.journal_id),
    write.prepare("UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=?,reversed_at=?,updated_at=? WHERE id=?").bind(actor.userId,now,now,payment.journal_id),
  ];
  for(const allocation of allocations.results){
    const restored=Number(allocation.outstanding_minor)+Number(allocation.amount_minor);
    if(restored>Number(allocation.total_minor))throw new ApiError(409,"INVOICE_BALANCE_CONFLICT","The allocated invoice balance no longer permits this reversal.");
    steps.push(
      assertion(write,id,`return-${allocation.id}`,"NOT EXISTS(SELECT 1 FROM commerce_returns WHERE invoice_id=? AND status!='rejected')",[allocation.invoice_id]),
      assertion(write,id,`invoice-${allocation.id}`,"EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND outstanding_minor=? AND total_minor>=outstanding_minor+?)",[allocation.invoice_id,allocation.outstanding_minor,allocation.amount_minor]),
      write.prepare("UPDATE finance_invoices SET outstanding_minor=outstanding_minor+?,status=CASE WHEN outstanding_minor+?=total_minor THEN 'posted' ELSE 'partially_paid' END,version=version+1,updated_at=? WHERE id=?").bind(allocation.amount_minor,allocation.amount_minor,now,allocation.invoice_id),
    );
  }
  steps.push(
    write.prepare("UPDATE finance_payments SET status='voided',version=version+1,updated_at=? WHERE id=?").bind(now,paymentId),
    write.prepare("INSERT INTO finance_payment_reversals(id,payment_id,reversal_journal_id,reason,reversed_by,reversed_at) VALUES(?,?,?,?,?,?)").bind(`payment-correction-${id}`,paymentId,reversal,input.reason,actor.userId,now),
  );
  return executeCommand(write,actor,id,"finance.payment.reverse",input,paymentId,input.expectedVersion+1,{id:paymentId,status:"voided",version:input.expectedVersion+1,reversalJournalId:reversal},steps);
}

export async function postJournal(actor: WorkContext, journalId: string, id: string, input: { expectedVersion: number }) {
  actor = requirePermission(actor,"finance.write");

  const {write}=await resources(actor);const previous=await replayCommand(write,actor,id,"finance.journal.post",journalId,input);if(previous)return previous;
  const journal=await write.prepare("SELECT * FROM finance_journals WHERE id=? AND company_id=?").bind(journalId,actor.companyId).first<FinanceRow>();if(!journal)throw new ApiError(404,"JOURNAL_NOT_FOUND","Journal not found.");
  if(samePerson(actor,journal.created_by)||!hasPermission(actor,"finance.approve"))throw new ApiError(403,"INDEPENDENT_APPROVER_REQUIRED","A different finance approver must post a manual journal.");
  const now=Date.now();return executeCommand(write,actor,id,"finance.journal.post",input,journalId,input.expectedVersion+1,{id:journalId,status:"posted",version:input.expectedVersion+1},[
    assertion(write,id,"journal","EXISTS(SELECT 1 FROM finance_journals WHERE id=? AND company_id=? AND version=? AND status IN ('draft','in_review'))",[journalId,actor.companyId,input.expectedVersion]),
    assertion(write,id,"period","EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)",[journal.period_id,actor.companyId,journal.entry_date,journal.entry_date]),
    assertion(write,id,"active-accounts","NOT EXISTS(SELECT 1 FROM finance_journal_lines l LEFT JOIN finance_accounts a ON a.id=l.account_id AND a.company_id=? AND a.chart_id=? WHERE l.journal_id=? AND (a.id IS NULL OR a.status!='active' OR a.currency!=?))",[actor.companyId,journal.chart_id,journalId,journal.currency]),
    assertion(write,id,"balanced","(SELECT COUNT(*)>=2 AND SUM(base_debit_minor)=SUM(base_credit_minor) AND SUM(debit_minor)=SUM(credit_minor) FROM finance_journal_lines WHERE journal_id=?)",[journalId]),
    write.prepare("UPDATE finance_journals SET status='posted',version=version+1,posted_by=?,posted_at=?,updated_at=? WHERE id=?").bind(actor.userId,now,now,journalId),
  ]);
}

export async function reverseJournal(actor: WorkContext, journalId: string, id: string, input: { periodId: string; entryDate: string; memo?: string }) {
  actor = requirePermission(actor,"finance.write");

  requireFinanceRole(actor,true);const {write}=await resources(actor);const previous=await replayCommand(write,actor,id,"finance.journal.reverse",journalId,input);if(previous)return previous;
  const journal=await write.prepare("SELECT * FROM finance_journals WHERE id=? AND company_id=? AND status='posted'").bind(journalId,actor.companyId).first<FinanceRow>();if(!journal)throw new ApiError(404,"POSTED_JOURNAL_NOT_FOUND","Posted journal not found.");
  if(!["manual","opening_import","reversal"].includes(String(journal.source_type)))throw new ApiError(409,"REVERSE_AT_SOURCE","Reverse the originating business document so its stock and settlement records remain consistent.");
  const reversal=`reversal-${id}`,now=Date.now();return executeCommand(write,actor,id,"finance.journal.reverse",input,journalId,Number(journal.version)+1,{reversalJournalId:reversal,reversedJournalId:journalId},[
    assertion(write,id,"source","EXISTS(SELECT 1 FROM finance_journals WHERE id=? AND version=? AND status='posted')",[journalId,journal.version]),
    assertion(write,id,"period","EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)",[input.periodId,actor.companyId,input.entryDate,input.entryDate]),
    write.prepare("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,chart_id,?,?,?,currency,fx_rate_micros,'reversal',id,?,'posted',?,?,?,?,? FROM finance_journals WHERE id=?").bind(reversal,input.periodId,`REV-${id}`,input.entryDate,input.memo??`Reversal of ${journal.journal_number}`,actor.userId,actor.userId,now,now,now,journalId),
    write.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?||'-'||ordinal,?,account_id,description,credit_minor,debit_minor,base_credit_minor,base_debit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=?").bind(reversal,reversal,journalId),
    write.prepare("UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=?,reversed_at=?,updated_at=? WHERE id=?").bind(actor.userId,now,now,journalId),
  ]);
}

export async function trialBalance(actor: WorkContext, periodId?: string, chartId?: string) {
  actor = requirePermission(actor,"finance.read");

  const { write } = await resources(actor,true); const clauses = ["j.company_id=?", "j.status IN ('posted','reversed')"]; const args: (string | number)[] = [actor.companyId]; if (periodId) { clauses.push("j.period_id=?"); args.push(periodId); }
  if(chartId){clauses.push("j.chart_id=?");args.push(chartId);} else {const currencies=await write.prepare("SELECT DISTINCT base_currency FROM finance_charts WHERE company_id=?").bind(actor.companyId).all();if(currencies.results.length>1)throw new ApiError(422,"CHART_REQUIRED","Select a chart to keep reporting currencies separate.");}
  const rows = await write.prepare(`SELECT a.id,a.code,a.name,a.category,COALESCE(SUM(l.base_debit_minor),0) AS debit,COALESCE(SUM(l.base_credit_minor),0) AS credit FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id JOIN finance_accounts a ON a.id=l.account_id WHERE ${clauses.join(" AND ")} GROUP BY a.id,a.code,a.name,a.category ORDER BY a.code`).bind(...args).all<FinanceRow>();
  return rows.results.map((row): FinanceRow & { debit: string; credit: string; balance: string } => ({ ...row, debit: minorToDecimal(Number(row.debit)), credit: minorToDecimal(Number(row.credit)), balance: minorToDecimal(Number(row.debit) - Number(row.credit)) }));
}

export async function statements(actor: WorkContext, periodId?: string, chartId?: string) {
  const tb = await trialBalance(actor, periodId, chartId); const byCategory = new Map<string, number>(); for (const row of tb) byCategory.set(row.category as string, (byCategory.get(row.category as string) ?? 0) + decimalToMinor(String(row.balance)));
  const value = (category: string) => minorToDecimal(byCategory.get(category) ?? 0);
  return { balanceSheet: { assets: value("asset"), liabilities: minorToDecimal(-decimalToMinor(value("liability"))), equity: minorToDecimal(-decimalToMinor(value("equity"))) }, profitAndLoss: { revenue: minorToDecimal(-decimalToMinor(value("revenue"))), expenses: value("expense"), otherIncome: minorToDecimal(-decimalToMinor(value("other_income"))), otherExpenses: value("other_expense") }, trialBalance: tb };
}

function refundCloseGuards(write:SqlDatabase,actor:WorkContext,period:FinanceRow,key:string) {
 return [
  assertion(write,key,"unresolved-supplier-bills","NOT EXISTS(SELECT 1 FROM procurement_bill_matches WHERE company_id=? AND status IN ('review','approved','invoiced') AND invoice_date<=?)",[actor.companyId,period.end_date]),
  assertion(write,key,"unresolved-supplier-bank","NOT EXISTS(SELECT 1 FROM finance_supplier_instructions i JOIN finance_supplier_batches b ON b.id=i.batch_id WHERE b.company_id=? AND i.status IN ('exported','unknown') AND b.planned_date<=?)",[actor.companyId,period.end_date]),
  assertion(write,key,"unfinished-returns","NOT EXISTS(SELECT 1 FROM commerce_returns WHERE company_id=? AND status='approved' AND return_date<=?)",[actor.companyId,period.end_date]),
  assertion(write,key,"unresolved-refund-bank","NOT EXISTS(SELECT 1 FROM finance_refund_instructions WHERE company_id=? AND status IN ('exported','unknown') AND planned_date<=?)",[actor.companyId,period.end_date]),
 ];
}
export async function beginClose(actor: WorkContext, periodId: string, id: string) {
  actor = requirePermission(actor,"finance.write");

  const { write } = await resources(actor); const previous = await replayCommand(write,actor,id,"finance.period.begin-close",periodId,{periodId}); if (previous) return previous;
  const period = await write.prepare("SELECT * FROM finance_periods WHERE id=? AND company_id=?").bind(periodId, actor.companyId).first<FinanceRow>(); if (!period) throw new ApiError(404, "PERIOD_NOT_FOUND", "Accounting period not found."); if (period.state !== "open") throw new ApiError(409, "INVALID_PERIOD_TRANSITION", "Only an open period can begin close.");
  const draftJournals = await write.prepare("SELECT COUNT(*) AS count FROM finance_journals WHERE company_id=? AND period_id=? AND status IN ('draft','in_review')").bind(actor.companyId, periodId).first<{ count: number }>();
  const draftInvoices = await write.prepare("SELECT COUNT(*) AS count FROM finance_invoices WHERE company_id=? AND status IN ('draft','in_review') AND invoice_date BETWEEN ? AND ?").bind(actor.companyId, period.start_date, period.end_date).first<{ count: number }>();
  const unmatchedBank = await write.prepare("SELECT COUNT(*) AS count FROM finance_bank_transactions WHERE company_id=? AND match_status IN ('unmatched','proposed') AND transaction_date BETWEEN ? AND ?").bind(actor.companyId,period.start_date,period.end_date).first<{ count: number }>();
  const billsPending=await write.prepare("SELECT COUNT(*) n FROM procurement_bill_matches WHERE company_id=? AND status IN ('review','approved','invoiced') AND invoice_date<=?").bind(actor.companyId,period.end_date).first('n');
  if(Number(billsPending))throw new ApiError(409,'CLOSE_NOT_READY','Resolve supplier invoice matching and AP posting before beginning close.');
  const supplierPending=await write.prepare("SELECT COUNT(*) AS count FROM finance_supplier_instructions i JOIN finance_supplier_batches b ON b.id=i.batch_id WHERE b.company_id=? AND i.status IN ('exported','unknown') AND b.planned_date<=?").bind(actor.companyId,period.end_date).first<{count:number}>();
  if(Number(supplierPending?.count))throw new ApiError(409,"CLOSE_NOT_READY","Resolve exported supplier bank instructions before beginning close.");
  const refunds=await write.prepare("SELECT (SELECT COUNT(*) FROM commerce_returns WHERE company_id=? AND status='approved' AND return_date<=?) returns,(SELECT COUNT(*) FROM finance_refund_instructions WHERE company_id=? AND status IN ('exported','unknown') AND planned_date<=?) instructions").bind(actor.companyId,period.end_date,actor.companyId,period.end_date).first<{returns:number;instructions:number}>();
  if(Number(refunds?.returns)||Number(refunds?.instructions))throw new ApiError(409,"CLOSE_NOT_READY",`Complete ${refunds?.returns??0} return adjustments and resolve ${refunds?.instructions??0} exported refund instructions before beginning close.`);
  const readiness = { draftJournals: draftJournals?.count ?? 0, draftInvoices: draftInvoices?.count ?? 0, unmatchedBank: unmatchedBank?.count ?? 0 };
  if (Object.values(readiness).some((value) => value > 0)) throw new ApiError(409, "CLOSE_NOT_READY", `Resolve ${readiness.draftJournals} journals, ${readiness.draftInvoices} invoices, and ${readiness.unmatchedBank} bank rows before beginning close.`);
  return executeCommand(write,actor,id,"finance.period.begin-close",{periodId},periodId,Number(period.close_version)+1,{id:periodId,state:"closing",version:Number(period.close_version)+1},[
    assertion(write,id,"period","EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND close_version=?)",[periodId,actor.companyId,period.close_version]),
    assertion(write,id,"journals","NOT EXISTS(SELECT 1 FROM finance_journals WHERE company_id=? AND period_id=? AND status IN ('draft','in_review'))",[actor.companyId,periodId]),
    assertion(write,id,"invoices","NOT EXISTS(SELECT 1 FROM finance_invoices WHERE company_id=? AND status IN ('draft','in_review') AND invoice_date BETWEEN ? AND ?)",[actor.companyId,period.start_date,period.end_date]),
    assertion(write,id,"bank","NOT EXISTS(SELECT 1 FROM finance_bank_transactions WHERE company_id=? AND match_status IN ('unmatched','proposed') AND transaction_date BETWEEN ? AND ?)",[actor.companyId,period.start_date,period.end_date]),
    ...refundCloseGuards(write,actor,period,id),
    write.prepare("UPDATE finance_periods SET state='closing',close_started_by=?,close_version=close_version+1 WHERE id=?").bind(actor.userId,periodId),
  ]);
}

export async function closePeriod(actor: WorkContext, periodId: string, input: z.infer<typeof closeProofSchema>, id: string) {
  actor = requirePermission(actor,"finance.write");

  requireFinanceRole(actor, true); const { write } = await resources(actor); const previous = await replayCommand(write,actor,id,"finance.period.close",periodId,input); if (previous) return previous;
  const period = await write.prepare("SELECT * FROM finance_periods WHERE id=? AND company_id=?").bind(periodId, actor.companyId).first<FinanceRow>(); if (!period) throw new ApiError(404, "PERIOD_NOT_FOUND", "Accounting period not found."); if (period.state !== "closing") throw new ApiError(409, "INVALID_PERIOD_TRANSITION", "Begin the close before approving it."); if (String(period.close_started_by) === actor.userId) throw new ApiError(409, "CLOSE_APPROVAL_SEPARATION", "A different person must approve the period close."); if (input.expectedVersion !== undefined && input.expectedVersion !== Number(period.close_version)) throw new ApiError(409, "VERSION_CONFLICT", "The close changed. Refresh and review again.");
  const now=Date.now();const version=Number(period.close_version);return executeCommand(write,actor,id,"finance.period.close",input,periodId,version+1,{id:periodId,state:"closed",version:version+1},[
    assertion(write,id,"period","EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='closing' AND close_version=? AND close_started_by!=?)",[periodId,actor.companyId,version,actor.userId]),
    ...refundCloseGuards(write,actor,period,id),
    assertion(write,id,"close-journals","NOT EXISTS(SELECT 1 FROM finance_journals WHERE company_id=? AND period_id=? AND status IN ('draft','in_review'))",[actor.companyId,periodId]),
    assertion(write,id,"close-invoices","NOT EXISTS(SELECT 1 FROM finance_invoices WHERE company_id=? AND status IN ('draft','in_review') AND invoice_date BETWEEN ? AND ?)",[actor.companyId,period.start_date,period.end_date]),
    assertion(write,id,"close-bank","NOT EXISTS(SELECT 1 FROM finance_bank_transactions WHERE company_id=? AND match_status IN ('unmatched','proposed') AND transaction_date BETWEEN ? AND ?)",[actor.companyId,period.start_date,period.end_date]),
    write.prepare("UPDATE finance_periods SET state='closed',closed_by=?,closed_at=?,close_version=close_version+1 WHERE id=?").bind(actor.userId,now,periodId),
    write.prepare("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),id,periodId,"finance.period.close",actor.userId,actor.assignmentId,version,await hash(input),now),
  ]);
}

export async function reopenPeriod(actor: WorkContext, periodId: string, input: z.infer<typeof closeProofSchema>, id: string) {
  actor = requirePermission(actor,"finance.write");

  requireFinanceRole(actor,true);const {write}=await resources(actor);const previous=await replayCommand(write,actor,id,"finance.period.reopen",periodId,input);if(previous)return previous;
  const period=await write.prepare("SELECT * FROM finance_periods WHERE id=? AND company_id=? AND state='closed'").bind(periodId,actor.companyId).first<FinanceRow>();if(!period)throw new ApiError(404,"PERIOD_NOT_CLOSED","Closed period not found.");
  if(period.closed_by===actor.userId)throw new ApiError(409,"CLOSE_APPROVAL_SEPARATION","A different approver must reopen this period.");
  const version=input.expectedVersion??Number(period.close_version);return executeCommand(write,actor,id,"finance.period.reopen",input,periodId,version+1,{id:periodId,state:"open",version:version+1},[
    assertion(write,id,"period","EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND state='closed' AND close_version=?)",[periodId,version]),
    write.prepare("UPDATE finance_periods SET state='open',closed_by=NULL,closed_at=NULL,close_version=close_version+1 WHERE id=?").bind(periodId),
    write.prepare("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),id,periodId,"finance.period.reopen",actor.userId,actor.assignmentId,version,await hash(input),Date.now()),
  ]);
}

export const partySchema = z.object({ kind: z.enum(["customer", "supplier", "employee"]), code: z.string().min(1).max(64), name: z.string().min(1).max(200), currency: z.string().length(3).transform((v) => v.toUpperCase()), taxRegistration: z.string().max(100).optional(), paymentTermsDays: z.number().int().min(0).max(365).default(30), creditLimit: moneySchema.default("0") });
export async function createParty(actor: WorkContext, input: z.infer<typeof partySchema>, id: string) {
  actor = requirePermission(actor,"finance.write");
 const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.party.create", input); if (previous) return previous; const partyId = `party-${crypto.randomUUID()}`; const now = Date.now(); pendingStatements.push(write.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,tax_registration,payment_terms_days,credit_limit_minor,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(partyId, actor.companyId, input.kind, input.code, input.name, input.currency, input.taxRegistration ?? null, input.paymentTermsDays, decimalToMinor(input.creditLimit), now, now)); const output = { id: partyId, ...input, creditLimitMinor: decimalToMinor(input.creditLimit) }; return saveCommand(write, actor, id, "finance.party.create", input, output, pendingStatements); }

export const invoiceSchema = z.object({ expectedVersion:z.number().int().positive().optional(), partyId: z.string().min(1), kind: z.enum(["payable", "receivable"]), invoiceNumber: z.string().min(1).max(100), invoiceDate: z.string().date(), dueDate: z.string().date(), currency: z.string().length(3).transform((v) => v.toUpperCase()), subtotal: moneySchema, tax: moneySchema.default("0"), taxCode: z.string().max(64).optional(), taxAccountId:z.string().min(1).optional(), sourceType: z.string().max(64).optional(), sourceId: z.string().max(128).optional(), debitAccountId: z.string().min(1), creditAccountId: z.string().min(1) });
export async function createInvoice(actor:WorkContext,input:z.infer<typeof invoiceSchema>,id:string,existingId?:string) {
  actor = requirePermission(actor,"finance.write");

  const {write}=await resources(actor),invoiceId=existingId??`invoice-${id}`;const operation=existingId?"finance.invoice.update":"finance.invoice.create";const previous=await replayCommand(write,actor,id,operation,invoiceId,input);if(previous)return previous;
  const subtotal=decimalToMinor(input.subtotal),tax=decimalToMinor(input.tax),total=subtotal+tax;
  if(subtotal<=0||tax<0||!Number.isSafeInteger(total))throw new ApiError(422,"INVALID_INVOICE_AMOUNT","Provide a positive subtotal and nonnegative tax within supported precision.");
  if(input.dueDate<input.invoiceDate)throw new ApiError(422,"INVALID_DUE_DATE","Due date cannot precede the invoice date.");
  if(tax&&!input.taxAccountId)throw new ApiError(422,"TAX_ACCOUNT_REQUIRED","Map a separate input or output tax account before recording tax.");
  const accountIds=[input.debitAccountId,input.creditAccountId,...(tax?[input.taxAccountId!]:[])];if(new Set(accountIds).size!==accountIds.length)throw new ApiError(422,"ACCOUNT_INVALID","Use distinct principal and tax posting accounts.");
  const accounts=await write.prepare(`SELECT id,chart_id FROM finance_accounts WHERE company_id=? AND status='active' AND currency=? AND id IN (${accountIds.map(()=>"?").join(",")})`).bind(actor.companyId,input.currency,...accountIds).all<{id:string;chart_id:string}>();
  if(accounts.results.length!==accountIds.length||new Set(accounts.results.map(a=>a.chart_id)).size!==1)throw new ApiError(422,"ACCOUNT_INVALID","Invoice accounts must use one active chart and currency.");
  const now=Date.now();return executeCommand(write,actor,id,operation,input,invoiceId,existingId?(input.expectedVersion??0)+1:1,{id:invoiceId,total:minorToDecimal(total),outstanding:minorToDecimal(total),status:"draft",version:existingId?(input.expectedVersion??0)+1:1},[
    ...(existingId?[assertion(write,id,"invoice-version","EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND status='draft' AND version=? AND source_type='standalone')",[existingId,actor.companyId,input.expectedVersion??0])]:[]),
    assertion(write,id,"party","EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind=? AND currency=? AND status='active' AND person_id IS NULL)",[input.partyId,actor.companyId,input.kind==="payable"?"supplier":"customer",input.currency]),
    existingId?write.prepare("UPDATE finance_invoices SET party_id=?,kind=?,invoice_number=?,invoice_date=?,due_date=?,currency=?,subtotal_minor=?,tax_minor=?,total_minor=?,outstanding_minor=?,tax_code=?,debit_account_id=?,credit_account_id=?,tax_account_id=?,created_by=?,version=version+1,updated_at=? WHERE id=?").bind(input.partyId,input.kind,input.invoiceNumber,input.invoiceDate,input.dueDate,input.currency,subtotal,tax,total,total,input.taxCode??null,input.debitAccountId,input.creditAccountId,input.taxAccountId??null,actor.userId,now,existingId):write.prepare("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,tax_code,debit_account_id,credit_account_id,tax_account_id,source_type,source_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(invoiceId,actor.companyId,input.partyId,input.kind,input.invoiceNumber,input.invoiceDate,input.dueDate,input.currency,subtotal,tax,total,total,input.taxCode??null,input.debitAccountId,input.creditAccountId,input.taxAccountId??null,"standalone",null,actor.userId,now,now),
  ]);
}
export async function voidDraftInvoice(actor:WorkContext,invoiceId:string,expectedVersion:number,key:string){
  actor = requirePermission(actor,"finance.write");
const {write}=await resources(actor);return executeCommand(write,actor,key,"finance.invoice.void",{expectedVersion},invoiceId,expectedVersion+1,{id:invoiceId,status:"voided",version:expectedVersion+1},[
  assertion(write,key,"draft","EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND status='draft' AND source_type='standalone' AND version=?)",[invoiceId,actor.companyId,expectedVersion]),
  write.prepare("UPDATE finance_invoices SET status='voided',outstanding_minor=0,version=version+1,updated_at=? WHERE id=?").bind(Date.now(),invoiceId),
]);}
export async function invoiceWorkspace(actor:WorkContext){
  actor = requirePermission(actor,"finance.read");
const {write}=await resources(actor,true);const [charts,accounts,parties,invoices,periods]=await Promise.all([
  listCharts(actor),write.prepare("SELECT * FROM finance_accounts WHERE company_id=? AND status='active' ORDER BY code").bind(actor.companyId).all<FinanceRow>(),
  write.prepare("SELECT * FROM finance_parties WHERE company_id=? AND status='active' AND kind IN ('customer','supplier') AND person_id IS NULL ORDER BY name").bind(actor.companyId).all<FinanceRow>(),listInvoices(actor),
  write.prepare("SELECT * FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date DESC").bind(actor.companyId).all<FinanceRow>(),
]);return {charts:charts as FinanceRow[],accounts:accounts.results,parties:parties.results,invoices,periods:periods.results};}

export async function postInvoice(actor:WorkContext,invoiceId:string,input:{periodId:string;entryDate:string;expectedVersion?:number},id:string) {
  actor = requirePermission(actor,"finance.write");

  requireFinanceRole(actor,true);
  const {write}=await resources(actor);const previous=await replayCommand(write,actor,id,"finance.invoice.post",invoiceId,input);if(previous)return previous;
  const invoice=await write.prepare("SELECT * FROM finance_invoices WHERE id=? AND company_id=? AND status='draft'").bind(invoiceId,actor.companyId).first<FinanceRow>();if(!invoice)throw new ApiError(404,"INVOICE_NOT_FOUND","Draft invoice not found.");
  if(samePerson(actor,invoice.created_by)||!hasPermission(actor,"finance.approve"))throw new ApiError(403,"INDEPENDENT_APPROVER_REQUIRED","A different finance approver must post a standalone invoice.");
  if(input.expectedVersion!==undefined && input.expectedVersion!==Number(invoice.version))throw new ApiError(409,"VERSION_CONFLICT","The invoice changed. Refresh its Debit / Credit before posting.");
  const tax=Number(invoice.tax_minor),subtotal=Number(invoice.subtotal_minor),total=Number(invoice.total_minor);if(tax&&!invoice.tax_account_id)throw new ApiError(422,"TAX_ACCOUNT_REQUIRED","This invoice needs a separate tax posting account.");
  const chart=await write.prepare("SELECT chart_id FROM finance_accounts WHERE id=? AND company_id=?").bind(invoice.debit_account_id,actor.companyId).first<{chart_id:string}>();if(!chart)throw new ApiError(422,"ACCOUNT_INVALID","Invoice account is unavailable.");
  const payable=invoice.kind==="payable";
  const lines:JournalLineInput[]=[{accountId:String(invoice.debit_account_id),debit:minorToDecimal(payable?subtotal:total),credit:"0"},{accountId:String(invoice.credit_account_id),debit:"0",credit:minorToDecimal(payable?total:subtotal)}];
  if(tax)lines.push({accountId:String(invoice.tax_account_id),debit:payable?minorToDecimal(tax):"0",credit:payable?"0":minorToDecimal(tax)});
  const matched=await (await import("./supplier-bill-matching")).matchedInvoicePosting(write,actor,invoice,id);
  const journal=await prepareJournal(write,actor,{chartId:chart.chart_id,periodId:input.periodId,entryDate:input.entryDate,currency:String(invoice.currency),memo:`Invoice ${invoice.invoice_number}`,sourceType:"invoice",sourceId:invoiceId,isOpening:false,lines:matched?.lines??lines},id,"posted");
  return executeCommand(write,actor,id,"finance.invoice.post",input,invoiceId,Number(invoice.version)+1,{id:invoiceId,status:"posted",journalId:journal.result.id},[
    assertion(write,id,"invoice","EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND status='draft' AND version=?)",[invoiceId,actor.companyId,invoice.version]),...journal.steps,
    write.prepare("UPDATE finance_invoices SET status='posted',journal_id=?,approved_by=?,posted_at=?,version=version+1,updated_at=? WHERE id=?").bind(journal.result.id,actor.userId,Date.now(),Date.now(),invoiceId),
    ...(matched?.steps??[]),
  ]);
}

export async function aging(actor: WorkContext, kind: "payable" | "receivable", asOf: string) {
  actor = requirePermission(actor,"finance.read");
 const { write } = await resources(actor,true); const rows = await write.prepare("SELECT i.*,p.code AS party_code,p.name AS party_name FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id WHERE i.company_id=? AND i.kind=? AND i.outstanding_minor>0 AND i.status IN ('posted','partially_paid') ORDER BY i.due_date").bind(actor.companyId, kind).all<FinanceRow>(); return rows.results.map((row) => { const days = Math.max(0, Math.floor((Date.parse(asOf) - Date.parse(String(row.due_date))) / 86400000)); const bucket = days === 0 ? "current" : days <= 30 ? "1-30" : days <= 60 ? "31-60" : days <= 90 ? "61-90" : "90+"; return { ...row, outstanding: minorToDecimal(Number(row.outstanding_minor)), bucket }; }); }

export async function createPayment(actor: WorkContext, input: { partyId?: string; kind: "payment" | "receipt" | "refund"; paymentNumber: string; paymentDate: string; currency: string; amount: string; bankAccount?: string }, id: string) {
  actor = requirePermission(actor,"finance.write");
 const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.payment.create", input); if (previous) return previous; const paymentId = `pay-${crypto.randomUUID()}`; const amount = decimalToMinor(input.amount); if (amount <= 0) throw new ApiError(422, "INVALID_PAYMENT_AMOUNT", "Payment amount must be greater than zero."); const now = Date.now(); pendingStatements.push(write.prepare("INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(paymentId, actor.companyId, input.partyId ?? null, input.kind, input.paymentNumber, input.paymentDate, input.currency.toUpperCase(), amount, amount, input.bankAccount ?? null, actor.userId, now, now)); const output = { id: paymentId, amount: minorToDecimal(amount), outstanding: minorToDecimal(amount), status: "draft" }; return saveCommand(write, actor, id, "finance.payment.create", input, output, pendingStatements); }

export async function approvePayment(actor: WorkContext, paymentId: string, input: { expectedVersion: number; proof: string }, id: string) {
  actor = requirePermission(actor,"finance.write");
 requireFinanceRole(actor, true); const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.payment.approve", input); if (previous) return previous; if (!input.proof.trim()) throw new ApiError(422, "HUMAN_DECISION_REQUIRED", "Payment approval evidence is required."); const payment = await write.prepare("SELECT * FROM finance_payments WHERE id=? AND company_id=?").bind(paymentId, actor.companyId).first<FinanceRow>(); if (!payment) throw new ApiError(404, "PAYMENT_NOT_FOUND", "Payment not found."); if (Number(payment.version) !== input.expectedVersion) throw new ApiError(409, "VERSION_CONFLICT", "Payment changed. Refresh before approving."); if (samePerson(actor,payment.created_by)) throw new ApiError(409, "SELF_REVIEW_FORBIDDEN", "A different person must approve the payment."); if (payment.status !== "draft" && payment.status !== "in_review") throw new ApiError(409, "PAYMENT_NOT_APPROVABLE", "Payment is not awaiting approval."); pendingStatements.push(assertion(write,id,"payment-version","EXISTS(SELECT 1 FROM finance_payments WHERE id=? AND company_id=? AND version=?)",[paymentId,actor.companyId,input.expectedVersion])); pendingStatements.push(write.prepare("UPDATE finance_payments SET status='approved',approved_by=?,version=version+1,updated_at=? WHERE id=? AND version=? AND company_id=?").bind(actor.userId, Date.now(), paymentId, input.expectedVersion, actor.companyId)); const output = { id: paymentId, status: "approved", version: input.expectedVersion + 1 }; return saveCommand(write, actor, id, "finance.payment.approve", input, output, pendingStatements); }

export async function releasePayment(actor: WorkContext, paymentId: string, input: { expectedVersion: number; proof: string }, id: string) {
  actor = requirePermission(actor,"finance.write");
 requireFinanceRole(actor, true); const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.payment.release", input); if (previous) return previous; if (!input.proof.trim()) throw new ApiError(422, "HUMAN_DECISION_REQUIRED", "Payment release evidence is required."); const payment = await write.prepare("SELECT * FROM finance_payments WHERE id=? AND company_id=? AND status='approved'").bind(paymentId, actor.companyId).first<FinanceRow>(); if (!payment) throw new ApiError(404, "PAYMENT_NOT_APPROVED", "Only an approved payment can be released."); if (Number(payment.version) !== input.expectedVersion) throw new ApiError(409, "VERSION_CONFLICT", "Payment changed. Refresh before releasing."); pendingStatements.push(assertion(write,id,"payment-version","EXISTS(SELECT 1 FROM finance_payments WHERE id=? AND company_id=? AND version=?)",[paymentId,actor.companyId,input.expectedVersion])); pendingStatements.push(write.prepare("UPDATE finance_payments SET status='released',released_by=?,version=version+1,updated_at=? WHERE id=? AND version=? AND company_id=?").bind(actor.userId, Date.now(), paymentId, input.expectedVersion, actor.companyId)); const output = { id: paymentId, status: "released", version: input.expectedVersion + 1, bankFileRequired: true }; return saveCommand(write, actor, id, "finance.payment.release", input, output, pendingStatements); }

export async function allocatePayment(actor:WorkContext,paymentId:string,input:{invoiceId:string;amount:string;expectedVersion:number},key:string){
  actor = requirePermission(actor,"finance.write");

  const {write}=await resources(actor);const previous=await replayCommand(write,actor,key,"finance.payment.allocate",paymentId,input);if(previous)return previous;const amount=decimalToMinor(input.amount);if(amount<=0)throw new ApiError(422,"INVALID_ALLOCATION","Allocation must be positive.");
  return executeCommand(write,actor,key,"finance.payment.allocate",input,paymentId,input.expectedVersion+1,{paymentId,invoiceId:input.invoiceId,allocated:minorToDecimal(amount)},[
    assertion(write,key,"supplier-reservation","NOT EXISTS(SELECT 1 FROM finance_supplier_instructions WHERE invoice_id=? AND status IN ('reserved','exported','unknown'))",[input.invoiceId]),
    assertion(write,key,"targets","EXISTS(SELECT 1 FROM finance_payments p JOIN finance_invoices i ON i.id=? JOIN finance_journals j ON j.id=p.journal_id WHERE p.id=? AND p.company_id=? AND i.company_id=p.company_id AND i.source_type!='expense_claim' AND p.party_id=i.party_id AND p.currency=i.currency AND p.version=? AND p.status IN ('released','settled') AND j.status='posted' AND i.status IN ('posted','partially_paid') AND ((p.kind='receipt' AND i.kind='receivable') OR (p.kind='payment' AND i.kind='payable')) AND p.outstanding_minor>=? AND i.outstanding_minor>=?)",[input.invoiceId,paymentId,actor.companyId,input.expectedVersion,amount,amount]),
    write.prepare("INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?)").bind(`allocation-${key}`,actor.companyId,paymentId,input.invoiceId,amount,actor.userId,Date.now()),
    write.prepare("UPDATE finance_payments SET outstanding_minor=outstanding_minor-?,status=CASE WHEN outstanding_minor=? THEN 'settled' ELSE status END,version=version+1,updated_at=? WHERE id=?").bind(amount,amount,Date.now(),paymentId),
    write.prepare("UPDATE finance_invoices SET outstanding_minor=outstanding_minor-?,status=CASE WHEN outstanding_minor=? THEN 'paid' ELSE 'partially_paid' END,version=version+1,updated_at=? WHERE id=?").bind(amount,amount,Date.now(),input.invoiceId),
  ]);
}

export async function exportPaymentFile(actor: WorkContext, paymentIds: string[], id: string) {
  actor = requirePermission(actor,"finance.write");
 const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const input = { paymentIds }; const previous = await priorCommand(write, actor, id, "finance.payment.export", input); if (previous) return previous; if (!paymentIds.length) throw new ApiError(422, "EMPTY_PAYMENT_BATCH", "Select at least one released payment."); const rows = await write.prepare(`SELECT id,payment_number,payment_date,currency,outstanding_minor,bank_account FROM finance_payments WHERE company_id=? AND status='released' AND id IN (${paymentIds.map(() => "?").join(",")}) ORDER BY payment_number`).bind(actor.companyId, ...paymentIds).all<FinanceRow>(); if (rows.results.length !== paymentIds.length) throw new ApiError(409, "PAYMENT_BATCH_INVALID", "Every payment must be released and belong to this company."); const csv = ["payment_id,payment_number,payment_date,currency,amount,bank_account", ...rows.results.map((row) => [row.id, row.payment_number, row.payment_date, row.currency, minorToDecimal(Number(row.outstanding_minor)), row.bank_account ?? ""].map((value) => `"${String(value).replaceAll('"', '""')}"`).join(","))].join("\n"); const output = { format: "csv", rows: rows.results.length, sha256: await hash(csv), content: csv, settlementRequired: true }; return saveCommand(write, actor, id, "finance.payment.export", input, output, pendingStatements); }

export async function bankWorkspace(actor:WorkContext,query?:{print?:string;transactionCursor?:string;statementCursor?:string;match?:string;new?:string}) {
 actor=requirePermission(actor,'finance.read');const {write}=await resources(actor,true);
 const statementSql='SELECT *,statement_date AS page_key FROM finance_bank_statements WHERE company_id=?';
 const transactionSql='SELECT t.*,s.bank_account,s.statement_number,t.transaction_date AS page_key FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.company_id=?';
 const paged=!!query&&query.print!=='1';
 const [accounts,statements,transactions,payments]=await Promise.all([
  !query||query.new==='1'?write.prepare("SELECT id,code,name,currency FROM finance_accounts WHERE company_id=? AND category='asset' AND status='active' ORDER BY code").bind(actor.companyId).all<FinanceRow>():{results:[] as FinanceRow[]},
  paged?readListPage<FinanceRow & {id:string;page_key:string}>(write,{sql:statementSql,args:[actor.companyId],cursor:query.statementCursor,descending:true,scope:[actor.companyId,'bank-statements']}).then(page=>({results:page.items,page})):
    write.prepare(`${statementSql} ORDER BY statement_date DESC,id`).bind(actor.companyId).all<FinanceRow>().then(result=>({...result,page:null})),
  paged?readListPage<FinanceRow & {id:string;page_key:string}>(write,{sql:transactionSql,args:[actor.companyId],cursor:query.transactionCursor,descending:true,scope:[actor.companyId,'bank-transactions']}).then(page=>({results:page.items,page})):
    write.prepare(`${transactionSql} ORDER BY t.transaction_date DESC,t.id`).bind(actor.companyId).all<FinanceRow>().then(result=>({...result,page:null})),
  !query||query.match?write.prepare("SELECT p.* FROM finance_payments p JOIN finance_journals j ON j.id=p.journal_id WHERE p.company_id=? AND p.status IN ('released','settled') AND j.status='posted' AND NOT EXISTS(SELECT 1 FROM finance_bank_transactions t WHERE t.company_id=p.company_id AND t.matched_type='payment' AND t.matched_id=p.id AND t.match_status='matched') ORDER BY p.payment_date DESC").bind(actor.companyId).all<FinanceRow>():{results:[] as FinanceRow[]},
 ]);
 return {accounts:accounts.results,statements:statements.results,statementPage:statements.page,transactions:transactions.results,transactionPage:transactions.page,payments:payments.results};
}
export async function importBankStatement(actor:WorkContext,input:{bankAccount:string;statementNumber:string;statementDate:string;opening:string;closing:string;sourceHash:string;rows:Array<{externalId:string;transactionDate:string;description?:string;amount:string;currency:string}>},key:string){
  actor = requirePermission(actor,"finance.write");

  const {write}=await resources(actor),id=`statement-${key}`;const previous=await replayCommand(write,actor,key,"finance.bank.import",id,input);if(previous)return previous;
  if(!input.rows.length||input.rows.length>500)throw new ApiError(422,"STATEMENT_SIZE","Import between 1 and 500 statement rows.");
  const account=await write.prepare("SELECT currency FROM finance_accounts WHERE id=? AND company_id=? AND category='asset' AND status='active'").bind(input.bankAccount,actor.companyId).first<{currency:string}>();if(!account)throw new ApiError(422,"BANK_ACCOUNT_INVALID","Select an active bank ledger account.");
  const opening=decimalToMinor(input.opening),closing=decimalToMinor(input.closing),amounts=input.rows.map(r=>decimalToMinor(r.amount));
  if(BigInt(opening)+amounts.reduce((n,a)=>n+BigInt(a),BigInt(0))!==BigInt(closing))throw new ApiError(422,"STATEMENT_UNBALANCED","Opening balance plus movements must equal the closing balance.");
  if(new Set(input.rows.map(r=>r.externalId)).size!==input.rows.length)throw new ApiError(422,"DUPLICATE_TRANSACTION","Each bank transaction needs a unique external reference.");
  if(input.rows.some(r=>r.currency.toUpperCase()!==account.currency||r.transactionDate>input.statementDate))throw new ApiError(422,"STATEMENT_ROW_INVALID","Rows must use the bank account currency and cannot follow the statement date.");
  const digest=await hash({bankAccount:input.bankAccount,statementNumber:input.statementNumber,opening,closing,rows:input.rows});
  return executeCommand(write,actor,key,"finance.bank.import",input,id,1,{id,imported:input.rows.length,status:"imported"},[
    assertion(write,key,"duplicate-statement","NOT EXISTS(SELECT 1 FROM finance_bank_statements WHERE company_id=? AND bank_account=? AND (statement_number=? OR source_hash=?))",[actor.companyId,input.bankAccount,input.statementNumber,digest]),
    ...input.rows.map((r,i)=>assertion(write,key,`external-${i}`,"NOT EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.company_id=? AND s.bank_account=? AND t.external_id=?)",[actor.companyId,input.bankAccount,r.externalId])),
    write.prepare("INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,source_hash,imported_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(id,actor.companyId,input.bankAccount,input.statementNumber,input.statementDate,opening,closing,digest,actor.userId,Date.now()),
    ...input.rows.map((r,i)=>write.prepare("INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,description,amount_minor,currency) VALUES(?,?,?,?,?,?,?,?)").bind(`bankrow-${key}-${i}`,id,actor.companyId,r.transactionDate,r.externalId,r.description??"",amounts[i],r.currency.toUpperCase())),
  ]);
}
export async function reconcileBankTransaction(actor:WorkContext,transactionId:string,input:{expectedVersion:number;matchedType:string;matchedId:string;proof:string},key:string){
  actor = requirePermission(actor,"finance.write");

  requireFinanceRole(actor,true);const {write}=await resources(actor);if(input.matchedType!=="payment"||!input.proof.trim())throw new ApiError(422,"MATCH_INPUT_INVALID","Select a posted payment and provide review evidence.");
  return executeCommand(write,actor,key,"finance.bank.reconcile",input,transactionId,input.expectedVersion+1,{id:transactionId,status:"matched",version:input.expectedVersion+1},[
    assertion(write,key,"payment-match","EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id JOIN finance_payments p ON p.id=? JOIN finance_journals j ON j.id=p.journal_id WHERE t.id=? AND t.company_id=? AND p.company_id=t.company_id AND t.version=? AND t.match_status IN ('unmatched','proposed') AND p.currency=t.currency AND p.bank_account=s.bank_account AND p.status IN ('released','settled') AND j.status='posted' AND t.amount_minor=CASE WHEN p.kind='receipt' THEN p.amount_minor ELSE -p.amount_minor END)",[input.matchedId,transactionId,actor.companyId,input.expectedVersion]),
    assertion(write,key,"already-matched","NOT EXISTS(SELECT 1 FROM finance_bank_transactions WHERE company_id=? AND matched_type='payment' AND matched_id=? AND match_status='matched')",[actor.companyId,input.matchedId]),
    write.prepare("UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=?,version=version+1 WHERE id=?").bind(input.matchedId,transactionId),
    write.prepare("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),key,transactionId,"finance.bank.reconcile",actor.userId,actor.assignmentId,input.expectedVersion,await hash(input),Date.now()),
  ]);
}

export const taxCodeSchema = z.object({ code: z.string().min(1).max(40), name: z.string().min(1).max(160), rate: z.string(), recoverable: z.string().default("1"), effectiveFrom: z.string().date(), effectiveTo: z.string().date().optional() });
export async function createTaxCode(actor: WorkContext, input: z.infer<typeof taxCodeSchema>, id: string) {
  actor = requirePermission(actor,"finance.write");
 const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.tax-code.create", input); if (previous) return previous; const taxId = `tax-${crypto.randomUUID()}`; const rate = decimalToMinor(input.rate, 6); const recoverable = decimalToMinor(input.recoverable, 6); if (rate < 0 || recoverable < 0 || recoverable > 1_000_000) throw new ApiError(422, "INVALID_TAX_RATE", "Tax rates are non-negative and recoverability is between 0 and 1."); pendingStatements.push(write.prepare("INSERT INTO finance_tax_codes(id,company_id,code,name,rate_micros,recoverable_micros,effective_from,effective_to,status) VALUES(?,?,?,?,?,?,?,?,?)").bind(taxId, actor.companyId, input.code, input.name, rate, recoverable, input.effectiveFrom, input.effectiveTo ?? null, "active")); const output = { id: taxId, ...input, rateMicros: rate, recoverableMicros: recoverable }; return saveCommand(write, actor, id, "finance.tax-code.create", input, output, pendingStatements); }

export const assetSchema = z.object({ assetNumber: z.string().min(1).max(64), name: z.string().min(1).max(200), accountId: z.string(), depreciationAccountId: z.string().optional(), currency: z.string().length(3).transform((v) => v.toUpperCase()).default("USD"), cost: moneySchema, residual: moneySchema.default("0"), usefulLifeMonths: z.number().int().positive().max(1200), inServiceDate: z.string().date() });
export async function createAsset(actor: WorkContext, input: z.infer<typeof assetSchema>, id: string) {
  actor = requirePermission(actor,"finance.write");
 const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.asset.create", input); if (previous) return previous; const account = await write.prepare("SELECT id FROM finance_accounts WHERE id=? AND company_id=?").bind(input.accountId, actor.companyId).first(); if (!account) throw new ApiError(422, "ACCOUNT_INVALID", "Asset account is not available."); const assetId = `asset-${crypto.randomUUID()}`; const now = Date.now(); pendingStatements.push(write.prepare("INSERT INTO finance_assets(id,company_id,asset_number,name,account_id,depreciation_account_id,currency,cost_minor,residual_minor,useful_life_months,in_service_date,status,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?, 'draft',?,?,?)").bind(assetId, actor.companyId, input.assetNumber, input.name, input.accountId, input.depreciationAccountId ?? null, input.currency, decimalToMinor(input.cost), decimalToMinor(input.residual), input.usefulLifeMonths, input.inServiceDate, actor.userId, now, now)); const output = { id: assetId, status: "draft", cost: input.cost }; return saveCommand(write, actor, id, "finance.asset.create", input, output, pendingStatements); }

export async function depreciateAsset(actor: WorkContext, assetId: string, input: { periodId: string; entryDate: string }, id: string) {
  actor = requirePermission(actor,"finance.write");
 const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.asset.depreciate", input); if (previous) return previous; const asset = await write.prepare("SELECT * FROM finance_assets WHERE id=? AND company_id=?").bind(assetId, actor.companyId).first<FinanceRow>(); if (!asset) throw new ApiError(404, "ASSET_NOT_FOUND", "Asset not found."); if (!asset.depreciation_account_id) throw new ApiError(422, "DEPRECIATION_ACCOUNT_REQUIRED", "Configure the accumulated depreciation account first."); if (asset.status === "retired") throw new ApiError(409, "ASSET_RETIRED", "A retired asset cannot be depreciated."); const remaining = Number(asset.cost_minor) - Number(asset.residual_minor) - Number(asset.accumulated_depreciation_minor); if (remaining <= 0) return { id: assetId, amount: "0.00", status: "complete" }; const monthly = Math.min(Math.ceil((Number(asset.cost_minor) - Number(asset.residual_minor)) / Number(asset.useful_life_months)), remaining); const account = await write.prepare("SELECT chart_id FROM finance_accounts WHERE id=? AND company_id=?").bind(asset.account_id, actor.companyId).first<{ chart_id: string }>(); if (!account) throw new ApiError(422, "ACCOUNT_INVALID", "Asset account is unavailable."); const journal = await createJournal(actor, { chartId: account.chart_id, periodId: input.periodId, entryDate: input.entryDate, currency: String(asset.currency), memo: `Depreciation ${asset.asset_number}`, sourceType: "asset_depreciation", sourceId: assetId, isOpening: false, lines: [{ accountId: String(asset.depreciation_account_id), debit: minorToDecimal(monthly), credit: "0" }, { accountId: String(asset.account_id), debit: "0", credit: minorToDecimal(monthly) }] }, `${id}:journal`); await postJournal(actor, journal.id, `${id}:post`, { expectedVersion: 1 }); pendingStatements.push(write.prepare("UPDATE finance_assets SET accumulated_depreciation_minor=accumulated_depreciation_minor+?,status='in_service',version=version+1 WHERE id=? AND company_id=?").bind(monthly, assetId, actor.companyId)); const output = { id: assetId, journalId: journal.id, amount: minorToDecimal(monthly), status: "posted" }; return saveCommand(write, actor, id, "finance.asset.depreciate", input, output, pendingStatements); }

export const budgetSchema = z.object({ name: z.string().min(1).max(160), fiscalYear: z.number().int().min(2000).max(2200), currency: z.string().length(3).transform((v) => v.toUpperCase()) });
export async function createBudget(actor: WorkContext, input: z.infer<typeof budgetSchema>, id: string) {
  actor = requirePermission(actor,"finance.write");
 const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.budget.create", input); if (previous) return previous; const budgetId = `budget-${crypto.randomUUID()}`; const now = Date.now(); pendingStatements.push(write.prepare("INSERT INTO finance_budgets(id,company_id,name,fiscal_year,currency,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)").bind(budgetId, actor.companyId, input.name, input.fiscalYear, input.currency, actor.userId, now, now)); const output = { id: budgetId, ...input, status: "draft" }; return saveCommand(write, actor, id, "finance.budget.create", input, output, pendingStatements); }


export const leaseSchema = z.object({ leaseNumber: z.string().min(1).max(64), counterparty: z.string().min(1).max(200), currency: z.string().length(3).transform((v) => v.toUpperCase()), commencementDate: z.string().date(), endDate: z.string().date(), payment: moneySchema, discountRate: z.string().default("0"), paymentFrequency: z.string().default("monthly") });
export async function createLease(actor: WorkContext, input: z.infer<typeof leaseSchema>, id: string) {
  actor = requirePermission(actor,"finance.write");
 const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.lease.create", input); if (previous) return previous; const leaseId = `lease-${crypto.randomUUID()}`; const now = Date.now(); const payment = decimalToMinor(input.payment); const rate = decimalToMinor(input.discountRate, 6); pendingStatements.push(write.prepare("INSERT INTO finance_leases(id,company_id,lease_number,counterparty,currency,commencement_date,end_date,payment_frequency,payment_minor,discount_rate_micros,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(leaseId, actor.companyId, input.leaseNumber, input.counterparty, input.currency, input.commencementDate, input.endDate, input.paymentFrequency, payment, rate, actor.userId, now, now)); const output = { id: leaseId, status: "draft", payment: input.payment }; return saveCommand(write, actor, id, "finance.lease.create", input, output, pendingStatements); }

export const loanSchema = z.object({ loanNumber: z.string().min(1).max(64), lender: z.string().min(1).max(200), currency: z.string().length(3).transform((v) => v.toUpperCase()), principal: moneySchema, annualRate: z.string().default("0"), commencementDate: z.string().date(), maturityDate: z.string().date() });
export async function createLoan(actor: WorkContext, input: z.infer<typeof loanSchema>, id: string) {
  actor = requirePermission(actor,"finance.write");
 const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.loan.create", input); if (previous) return previous; const loanId = `loan-${crypto.randomUUID()}`; const now = Date.now(); const principal = decimalToMinor(input.principal); pendingStatements.push(write.prepare("INSERT INTO finance_loans(id,company_id,loan_number,lender,currency,principal_minor,annual_rate_micros,commencement_date,maturity_date,outstanding_minor,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(loanId, actor.companyId, input.loanNumber, input.lender, input.currency, principal, decimalToMinor(input.annualRate, 6), input.commencementDate, input.maturityDate, principal, actor.userId, now, now)); const output = { id: loanId, status: "draft", principal: input.principal }; return saveCommand(write, actor, id, "finance.loan.create", input, output, pendingStatements); }

export const revenueContractSchema = z.object({ customerId: z.string(), contractNumber: z.string().min(1).max(64), currency: z.string().length(3).transform((v) => v.toUpperCase()), transactionPrice: moneySchema });
export async function createRevenueContract(actor: WorkContext, input: z.infer<typeof revenueContractSchema>, id: string) {
  actor = requirePermission(actor,"finance.write");
 const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.revenue-contract.create", input); if (previous) return previous; const customer = await write.prepare("SELECT id FROM finance_parties WHERE id=? AND company_id=? AND kind='customer'").bind(input.customerId, actor.companyId).first(); if (!customer) throw new ApiError(404, "CUSTOMER_NOT_FOUND", "Customer not found."); const contractId = `contract-${crypto.randomUUID()}`; const now = Date.now(); pendingStatements.push(write.prepare("INSERT INTO finance_revenue_contracts(id,company_id,customer_id,contract_number,currency,transaction_price_minor,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(contractId, actor.companyId, input.customerId, input.contractNumber, input.currency, decimalToMinor(input.transactionPrice), actor.userId, now, now)); const output = { id: contractId, status: "draft", transactionPrice: input.transactionPrice }; return saveCommand(write, actor, id, "finance.revenue-contract.create", input, output, pendingStatements); }

export const revenueObligationSchema = z.object({ contractId: z.string(), description: z.string().min(1).max(200), allocated: moneySchema, recognitionMethod: z.enum(["point_in_time", "over_time", "milestone"]), startDate: z.string().date(), endDate: z.string().date().optional(), revenueAccountId: z.string(), deferredAccountId: z.string() });
export async function createRevenueObligation(actor: WorkContext, input: z.infer<typeof revenueObligationSchema>, id: string) {
  actor = requirePermission(actor,"finance.write");
 const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.revenue-obligation.create", input); if (previous) return previous; const contract = await write.prepare("SELECT id FROM finance_revenue_contracts WHERE id=? AND company_id=?").bind(input.contractId, actor.companyId).first(); if (!contract) throw new ApiError(404, "CONTRACT_NOT_FOUND", "Revenue contract not found."); const obligationId = `obligation-${crypto.randomUUID()}`; pendingStatements.push(write.prepare("INSERT INTO finance_revenue_obligations(id,contract_id,description,allocated_minor,recognition_method,start_date,end_date,revenue_account_id,deferred_account_id) VALUES(?,?,?,?,?,?,?,?,?)").bind(obligationId, input.contractId, input.description, decimalToMinor(input.allocated), input.recognitionMethod, input.startDate, input.endDate ?? null, input.revenueAccountId, input.deferredAccountId)); const output = { id: obligationId, status: "pending", allocated: input.allocated }; return saveCommand(write, actor, id, "finance.revenue-obligation.create", input, output, pendingStatements); }

export async function recognizeRevenue(actor: WorkContext, obligationId: string, input: { amount: string; periodId: string; entryDate: string }, id: string) {
  actor = requirePermission(actor,"finance.write");
 const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.revenue.recognize", input); if (previous) return previous; const obligation = await write.prepare("SELECT o.*,c.company_id,c.currency FROM finance_revenue_obligations o JOIN finance_revenue_contracts c ON c.id=o.contract_id WHERE o.id=? AND c.company_id=?").bind(obligationId, actor.companyId).first<FinanceRow>(); if (!obligation) throw new ApiError(404, "OBLIGATION_NOT_FOUND", "Revenue obligation not found."); const amount = decimalToMinor(input.amount); const remaining = Number(obligation.allocated_minor) - Number(obligation.recognized_minor); if (amount <= 0 || amount > remaining) throw new ApiError(422, "REVENUE_EXCEEDS_ALLOCATION", "Recognition exceeds the remaining obligation."); const account = await write.prepare("SELECT chart_id FROM finance_accounts WHERE id=? AND company_id=?").bind(obligation.revenue_account_id, actor.companyId).first<{ chart_id: string }>(); if (!account) throw new ApiError(422, "ACCOUNT_INVALID", "Revenue account is unavailable."); const journal = await createJournal(actor, { chartId: account.chart_id, periodId: input.periodId, entryDate: input.entryDate, currency: String(obligation.currency), memo: `Revenue recognition ${obligation.description}`, sourceType: "revenue_recognition", sourceId: obligationId, isOpening: false, lines: [{ accountId: String(obligation.deferred_account_id), debit: minorToDecimal(amount), credit: "0" }, { accountId: String(obligation.revenue_account_id), debit: "0", credit: minorToDecimal(amount) }] }, `${id}:journal`); await postJournal(actor, journal.id, `${id}:post`, { expectedVersion: 1 }); pendingStatements.push(write.prepare("UPDATE finance_revenue_obligations SET recognized_minor=recognized_minor+?,status=CASE WHEN recognized_minor+?=allocated_minor THEN 'complete' ELSE 'active' END WHERE id=?").bind(amount, amount, obligationId)); const output = { id: obligationId, journalId: journal.id, recognized: minorToDecimal(amount) }; return saveCommand(write, actor, id, "finance.revenue.recognize", input, output, pendingStatements); }

export async function financeOverviewCounts(actor:WorkContext) {
  actor=requirePermission(actor,"finance.read");const {write}=await resources(actor,true);
  return (await write.prepare(`SELECT
    (SELECT count(*) FROM finance_journals WHERE company_id=? ) AS journals,
    (SELECT count(*) FROM finance_journals WHERE company_id=? AND status='draft') AS posting,
    (SELECT count(*) FROM finance_invoices WHERE company_id=?) AS invoices,
    (SELECT count(*) FROM finance_payments WHERE company_id=?) AS payments,
    (SELECT count(*) FROM finance_invoices WHERE company_id=? AND kind='receivable' AND status IN ('posted','partially_paid') AND outstanding_minor>0) AS receivable,
    (SELECT count(*) FROM finance_invoices WHERE company_id=? AND kind='payable' AND status IN ('posted','partially_paid') AND outstanding_minor>0) AS payable,
    (SELECT count(*) FROM finance_bank_transactions WHERE company_id=? AND match_status<>'matched') AS reconcile`)
    .bind(...Array(7).fill(actor.companyId)).first<{journals:number;posting:number;invoices:number;payments:number;receivable:number;payable:number;reconcile:number}>())!;
}
export async function invoicesPage(actor:WorkContext,query:{q?:string;kind?:string;status?:string;queue?:string;cursor?:string}) {
  actor=requirePermission(actor,"finance.read");const {write}=await resources(actor,true);
  return readListPage<FinanceRow & {id:string;page_key:string}>(write,{
    sql:`SELECT i.id,i.invoice_number,i.kind,i.invoice_date,i.due_date,i.currency,i.total_minor,i.outstanding_minor,i.status,p.name AS party_name,
      i.invoice_date||':'||i.invoice_number AS page_key FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id WHERE i.company_id=?
      AND (?='' OR i.kind=?) AND (?='' OR i.status=?) AND (?<>'open' OR (i.status IN ('posted','partially_paid') AND i.outstanding_minor>0))
      AND (?='' OR strpos(lower(i.invoice_number||' '||p.name),lower(?))>0)`,
    args:[actor.companyId,query.kind??'',query.kind??'',query.status??'',query.status??'',query.queue??'',query.q??'',query.q??''],
    descending:true,cursor:query.cursor,scope:[actor.companyId,'invoices',query.q??'',query.kind??'',query.status??'',query.queue??''],
  });
}
