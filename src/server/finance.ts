import { readListPage } from "./list-page";
import { prepareFlowCommand } from "./flow-routing";
import { invoiceHistoryCte } from "./finance-history";
import { calculateVatCloseEvidence,pendingVatReviewSql,vatCloseGuards } from "./finance-tax-register";
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
import { prepareAttachmentBinding } from "./attachments/service";
import { companyRates, snapshotMoney, snapshotMoneyCarry, snapshotMoneyReversal } from "./company-money";
import { decimalUnits, resolveRate } from "@/lib/company-money";
import { requireFinanceMoneyScale } from "./finance-money-policy";
import { invoiceLineSchema } from "@/lib/finance-invoice-lines";
import { currentInvoiceLines, nativeInvoicePosting, prepareInvoiceLineRevision } from "./finance-invoice-lines";
import { readyPaymentWithholding, prepareWithholdingConsumption, prepareWithholdingReversal } from "./finance-withholding";

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

function fxMicros(currency:string,baseCurrency:string,value?:string){
  const rate=currency===baseCurrency?1_000_000:value?decimalToMinor(value,6):0;
  if(rate<1||rate>1_000_000_000_000)throw new ApiError(422,"FX_RATE_REQUIRED",`Provide a positive ${baseCurrency} per ${currency} exchange rate.`);
  return rate;
}

export async function confirmedFunctionalFx(write:SqlDatabase,actor:WorkContext,currency:string,baseCurrency:string,date:string){
  if(currency===baseCurrency)return {micros:1_000_000,sourceRateId:null as string|null,targetRateId:null as string|null,usedLatestFallback:false};
  const rates=await companyRates(write,actor.companyId),source=resolveRate(rates,currency,date),target=resolveRate(rates,baseCurrency,date);
  if(!source||!target)throw new ApiError(422,"MANUAL_RATE_REQUIRED",`Confirm company manual USD rates for ${currency} and ${baseCurrency} before posting.`);
  const sourceQuote=decimalUnits(source.rate.currencyPerUsd,18),targetQuote=decimalUnits(target.rate.currencyPerUsd,18);
  const micros=(targetQuote*BigInt(1_000_000)+sourceQuote/BigInt(2))/sourceQuote;
  if(micros<BigInt(1)||micros>BigInt(1_000_000_000_000))throw new ApiError(422,"FX_RATE_PRECISION","The functional-currency rate is outside this ledger's supported six-decimal precision.");
  return {micros:Number(micros),sourceRateId:source.rate.id==='USD'?null:source.rate.id,targetRateId:target.rate.id==='USD'?null:target.rate.id,usedLatestFallback:source.usedLatestFallback||target.usedLatestFallback};
}

async function confirmedFunctionalFxMicros(write:SqlDatabase,actor:WorkContext,currency:string,baseCurrency:string,date:string){
  return (await confirmedFunctionalFx(write,actor,currency,baseCurrency,date)).micros;
}

function convertAtFx(amount:number,rate:number){
  const product=BigInt(amount)*BigInt(rate),negative=product<BigInt(0),absolute=negative?-product:product,converted=(absolute+BigInt(500_000))/BigInt(1_000_000),signed=negative?-converted:converted;
  if(signed>BigInt(Number.MAX_SAFE_INTEGER)||signed<BigInt(Number.MIN_SAFE_INTEGER))throw new ApiError(422,"MONEY_OVERFLOW","The converted amount exceeds supported precision.");
  return Number(signed);
}

function proportionalBase(baseOutstanding:number,transactionOutstanding:number,allocated:number){
  if(allocated===transactionOutstanding)return baseOutstanding;
  return Number((BigInt(baseOutstanding)*BigInt(allocated)+BigInt(Math.floor(transactionOutstanding/2)))/BigInt(transactionOutstanding));
}

async function legacyMoneySnapshot(write:SqlDatabase,actor:WorkContext,input:{sourceType:string;sourceId:string;amountKind:string;transactionDate:string;currency:string;amountUnits:number}):Promise<SqlStatement>{
  // This adapter preserves the original two-decimal ledger meaning. New postings
  // still require a dated company quote and the shared immutable money evidence.
  await requireFinanceMoneyScale(write,actor.companyId,input.currency);
  return (await snapshotMoney(write,actor,{...input,amountUnits:String(input.amountUnits),currencyScale:2})).statement;
}

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
  lines: z.array(lineSchema.pick({ accountId: true, description: true, debit: true, credit: true }).extend({
    subledgerKind: z.enum(["none", "receivable", "payable"]).default("none"),
  })).min(2).max(500),
  subledgerItems: z.array(z.object({
    kind: z.enum(["receivable", "payable"]),
    partyId: z.string().min(1).max(128),
    reference: z.string().trim().min(1).max(120),
    dueDate: z.string().date(),
    controlAccountId: z.string().min(1).max(128),
    offsetAccountId: z.string().min(1).max(128),
    amount: moneySchema,
  })).max(2000).default([]),
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
  controlKind: z.enum(["none", "receivable", "payable", "bank"]).default("none"),
});

export const changeAccountStatusSchema = z.object({
  status: z.enum(["active", "inactive"]),
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
});

export const createPeriodSchema = z.object({
  name: z.string().trim().min(1).max(100),
  startDate: z.string().date(),
  endDate: z.string().date(),
});

export const closeProofSchema = z.object({
  proof: z.string().trim().min(1).max(2000),
  expectedVersion: z.number().int().nonnegative().optional(),
  attachmentSetIds: z.array(z.string().min(1)).max(20).default([]),
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
  pendingStatements.push(write.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,parent_id,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
    .bind(accountId, input.chartId, actor.companyId, input.code, input.name, input.category, input.parentId ?? null, input.currency ?? chart.base_currency, input.isControl||input.controlKind!=="none" ? 1 : 0, input.controlKind, now, now));
  const result = { id: accountId, ...input, companyId: actor.companyId, currency: input.currency ?? chart.base_currency, version: 1 };
  return saveCommand(write, actor, id, "finance.account.create", input, result, pendingStatements);
}

export async function changeAccountStatus(actor: WorkContext, chartId: string, accountId: string, input: z.infer<typeof changeAccountStatusSchema>, id: string) {
  actor = requirePermission(actor, "finance.write");
  const { write } = await resources(actor), operation = `finance.account.${input.status}`;
  const previous = await priorCommand(write, actor, id, operation, { chartId, accountId, ...input });
  if (previous) return previous;
  const account = await write.prepare("SELECT id,status,version,is_control FROM finance_accounts WHERE id=? AND chart_id=? AND company_id=?").bind(accountId, chartId, actor.companyId).first<{ id: string; status: string; version: number; is_control: number }>();
  if (!account) throw new ApiError(404, "ACCOUNT_NOT_FOUND", "The account is not available in this chart.");
  if (Number(account.version) !== input.expectedVersion) throw new ApiError(409, "VERSION_CONFLICT", "The account changed. Refresh before changing its status.");
  if (account.status === input.status) throw new ApiError(409, "ACCOUNT_STATUS_UNCHANGED", `The account is already ${input.status}.`);
  if (input.status === "inactive") {
    const draft = await write.prepare("SELECT 1 FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE l.account_id=? AND j.company_id=? AND j.status='draft' LIMIT 1").bind(accountId, actor.companyId).first();
    if (draft) throw new ApiError(409, "ACCOUNT_HAS_DRAFT_JOURNAL", "Post or correct draft journals before making this account inactive.");
    const outstanding = await write.prepare("SELECT 1 FROM finance_invoices WHERE company_id=? AND outstanding_minor>0 AND status IN ('draft','posted','partially_paid') AND (debit_account_id=? OR credit_account_id=?) LIMIT 1").bind(actor.companyId, accountId, accountId).first();
    if (outstanding) throw new ApiError(409, "ACCOUNT_HAS_OPEN_INVOICES", "Resolve open invoices before making this account inactive.");
  }
  const now = Date.now(), result = { id: accountId, chartId, status: input.status, version: input.expectedVersion + 1 };
  return saveCommand(write, actor, id, operation, { chartId, accountId, ...input }, result, [
    assertion(write, id, "account-version", "EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND chart_id=? AND company_id=? AND status=? AND version=?)", [accountId, chartId, actor.companyId, account.status, input.expectedVersion]),
    write.prepare("UPDATE finance_accounts SET status=?,version=version+1,updated_at=? WHERE id=? AND chart_id=? AND company_id=? AND version=?").bind(input.status, now, accountId, chartId, actor.companyId, input.expectedVersion),
  ]);
}

export async function importAccounts(actor: WorkContext, chartId: string, rows: Array<{ code: string; name: string; category: z.infer<typeof createAccountSchema>["category"]; currency?: string; controlKind?: "none"|"receivable"|"payable"|"bank" }>, id: string) {
  actor = requirePermission(actor,"finance.write");

  const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const input = { chartId, rows }; const previous = await priorCommand(write, actor, id, "finance.account.import", input); if (previous) return previous;
  const chart = await write.prepare("SELECT base_currency FROM finance_charts WHERE id=? AND company_id=?").bind(chartId, actor.companyId).first<{ base_currency: string }>(); if (!chart) throw new ApiError(404, "CHART_NOT_FOUND", "Chart not found.");
  if(!rows.length||rows.length>1000)throw new ApiError(422,"IMPORT_SIZE","Import between 1 and 1,000 accounts.");
  if(new Set(rows.map(row=>row.code.trim())).size!==rows.length)throw new ApiError(422,"DUPLICATE_ACCOUNT_CODE","Each account code must appear once.");
  const now=Date.now();
  for(const [index,row] of rows.entries()){
    const existing=await write.prepare("SELECT * FROM finance_accounts WHERE chart_id=? AND company_id=? AND code=?").bind(chartId,actor.companyId,row.code).first<FinanceRow>();
    const currency=(row.currency??String(existing?.currency??chart.base_currency)).toUpperCase(),controlKind=row.controlKind??String(existing?.control_kind??"none");
    const structuralChange=existing&&(existing.name!==row.name||existing.category!==row.category||existing.currency!==currency||existing.control_kind!==controlKind);
    const unused="NOT EXISTS(SELECT 1 FROM finance_journal_lines WHERE account_id=?) AND NOT EXISTS(SELECT 1 FROM finance_invoices WHERE debit_account_id=? OR credit_account_id=? OR tax_account_id=?) AND NOT EXISTS(SELECT 1 FROM finance_invoice_line_versions WHERE account_id=? OR tax_account_id=?)";
    if(structuralChange){
      if(!await write.prepare(`SELECT (${unused}) unused`).bind(existing.id,existing.id,existing.id,existing.id,existing.id,existing.id).first("unused"))throw new ApiError(409,"ACCOUNT_HISTORY_IMMUTABLE",`Account ${row.code} is already referenced. Create a new account to change its accounting meaning.`);
      pendingStatements.push(assertion(write,id,`account-history-${index}`,unused,[existing.id,existing.id,existing.id,existing.id,existing.id,existing.id]));
    }
    if(existing)pendingStatements.push(assertion(write,id,`account-version-${index}`,"EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND version=?)",[existing.id,actor.companyId,existing.version]));
    else pendingStatements.push(assertion(write,id,`new-account-${index}`,"NOT EXISTS(SELECT 1 FROM finance_accounts WHERE chart_id=? AND code=?)",[chartId,row.code]));
    pendingStatements.push(write.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(chart_id,code) DO UPDATE SET name=excluded.name,category=excluded.category,currency=excluded.currency,is_control=excluded.is_control,control_kind=excluded.control_kind,version=finance_accounts.version+1,updated_at=excluded.updated_at")
      .bind(`acct-${crypto.randomUUID()}`,chartId,actor.companyId,row.code,row.name,row.category,currency,row.controlKind===undefined?Number(existing?.is_control??(controlKind!=="none"?1:0)):controlKind!=="none"?1:0,controlKind,now,now));
  }
  return saveCommand(write,actor,id,"finance.account.import",input,{imported:rows.length,chartId},pendingStatements);
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
  await requireFinanceMoneyScale(write,actor.companyId,input.currency);
  await requireFinanceMoneyScale(write,actor.companyId,chart.base_currency);
  const preparedFxEvidence=input.fxRate?null:await confirmedFunctionalFx(write,actor,input.currency,chart.base_currency,input.entryDate);
  const fx=input.fxRate?fxMicros(input.currency,chart.base_currency,input.fxRate):preparedFxEvidence!.micros;
  const convert=(amount:number)=>convertAtFx(amount,fx);
  const values=input.lines.map(l=>{const debit=decimalToMinor(l.debit),credit=decimalToMinor(l.credit);if(debit<0||credit<0||(debit>0)===(credit>0))throw new ApiError(422,"INVALID_JOURNAL_LINE","Each line requires one positive debit or credit.");return {...l,debit,credit,baseDebit:convert(debit),baseCredit:convert(credit)};});
  const total=(field:"debit"|"credit"|"baseDebit"|"baseCredit")=>values.reduce((sum,l)=>sum+BigInt(l[field]),BigInt(0));
  if(total("debit")!==total("credit"))throw new ApiError(422,"UNBALANCED_JOURNAL","Debits and credits must balance in transaction currency.");
  if(total("debit")>BigInt(Number.MAX_SAFE_INTEGER))throw new ApiError(422,"MONEY_OVERFLOW","The journal total exceeds supported precision.");
  const baseTarget=convert(Number(total("debit"))),lastDebit=[...values].reverse().find(line=>line.debit>0),lastCredit=[...values].reverse().find(line=>line.credit>0);
  if(!lastDebit||!lastCredit)throw new ApiError(422,"UNBALANCED_JOURNAL","A journal requires both debit and credit lines.");
  lastDebit.baseDebit+=baseTarget-Number(total("baseDebit"));lastCredit.baseCredit+=baseTarget-Number(total("baseCredit"));
  if(lastDebit.baseDebit<0||lastCredit.baseCredit<0||total("baseDebit")!==total("baseCredit"))throw new ApiError(422,"UNBALANCED_JOURNAL","Functional-currency rounding could not be balanced.");
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
  // Owner adapters consume these exact retained amounts; public journal results
  // and command receipts keep their existing shape.
  return {steps,preparedLines:values,preparedFxEvidence,result:{id:journalId,number,status,version:1,debit:minorToDecimal(Number(total("debit"))),credit:minorToDecimal(Number(total("credit"))),fxRateMicros:fx,baseCurrency:chart.base_currency}};
}
export async function createJournal(actor:WorkContext,input:z.infer<typeof createJournalSchema>,id:string) {
  actor = requirePermission(actor,"finance.write");
  if(input.sourceType==="fiscal_close")throw new ApiError(422,"FISCAL_CLOSING_OWNER_REQUIRED","Prepare and independently review a fiscal closing through the Finance year-closing workflow.");

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
  const lineAccountIds=new Set(input.lines.map(line=>line.accountId));
  if(lineAccountIds.size!==input.lines.length)throw new ApiError(422,"DUPLICATE_OPENING_ACCOUNT","Each account may appear once in an opening balance import.");
  const accountIds=[...new Set([...lineAccountIds,...input.subledgerItems.map(item=>item.offsetAccountId)])];
  if(input.subledgerItems.some(item=>!lineAccountIds.has(item.controlAccountId)))throw new ApiError(422,"OPENING_CONTROL_LINE_REQUIRED","Every subledger item must identify its matching opening-balance control account.");
  const accounts=await write.prepare(`SELECT id,code,name,status,is_control,control_kind,updated_at FROM finance_accounts WHERE company_id=? AND chart_id=? AND id IN (${accountIds.map(()=>"?").join(",")})`).bind(actor.companyId,input.chartId,...accountIds).all<FinanceRow>();
  if(accounts.results.length!==accountIds.length||accounts.results.some(account=>account.status!=="active"))throw new ApiError(422,"ACCOUNT_INVALID","Every opening balance account must be active and belong to this chart.");
  let debit=BigInt(0),credit=BigInt(0);
  const accountById=new Map(accounts.results.map(account=>[String(account.id),account]));
  const lines=input.lines.map(line=>{
    const debitMinor=decimalToMinor(line.debit),creditMinor=decimalToMinor(line.credit);
    if(debitMinor<0||creditMinor<0||(debitMinor>0)===(creditMinor>0))throw new ApiError(422,"INVALID_OPENING_LINE","Each opening line requires one positive debit or credit.");
    debit+=BigInt(debitMinor);credit+=BigInt(creditMinor);
    const account=accountById.get(line.accountId)!;
    if(["receivable","payable"].includes(String(account.control_kind))&&line.subledgerKind!==account.control_kind)throw new ApiError(422,"OPENING_CONTROL_SCHEDULE_REQUIRED",`${account.code} ${account.name} requires a ${account.control_kind} schedule.`);
    if(line.subledgerKind!=="none"){
      if(Number(account.is_control)!==1)throw new ApiError(422,"OPENING_CONTROL_ACCOUNT_REQUIRED","Receivable and payable opening lines must use accounts marked as control accounts.");
      if(line.subledgerKind==="receivable"&&debitMinor<=0)throw new ApiError(422,"OPENING_RECEIVABLE_DIRECTION","Opening receivables must be a debit balance.");
      if(line.subledgerKind==="payable"&&creditMinor<=0)throw new ApiError(422,"OPENING_PAYABLE_DIRECTION","Opening payables must be a credit balance.");
    }
    return {...line,debit:minorToDecimal(debitMinor),credit:minorToDecimal(creditMinor),accountCode:String(account.code),accountName:String(account.name)};
  });
  if(debit!==credit)throw new ApiError(422,"UNBALANCED_OPENING_BALANCE","Opening debits and credits must balance.");
  if(debit>BigInt(Number.MAX_SAFE_INTEGER))throw new ApiError(422,"MONEY_OVERFLOW","The opening balance total exceeds supported precision.");
  const references=new Set<string>();
  const partyIds=[...new Set(input.subledgerItems.map(item=>item.partyId))];
  const parties=partyIds.length?await write.prepare(`SELECT id,kind,currency,status,version,updated_at FROM finance_parties WHERE company_id=? AND id IN (${partyIds.map(()=>"?").join(",")})`).bind(actor.companyId,...partyIds).all<FinanceRow>():{results:[] as FinanceRow[]};
  const partyById=new Map(parties.results.map(party=>[String(party.id),party]));
  const scheduleByAccount=new Map<string,bigint>();
  const subledgerItems=input.subledgerItems.map(item=>{
    const amountMinor=decimalToMinor(item.amount);
    if(amountMinor<=0)throw new ApiError(422,"INVALID_OPENING_SUBLEDGER_AMOUNT","Each opening subledger amount must be greater than zero.");
    const party=partyById.get(item.partyId);
    if(!party||party.status!=="active"||party.kind!==(item.kind==="receivable"?"customer":"supplier"))throw new ApiError(422,"OPENING_PARTY_INVALID",`Opening ${item.kind} ${item.reference} requires an active ${item.kind==="receivable"?"customer":"supplier"}.`);
    if(party.currency!==input.currency)throw new ApiError(422,"OPENING_PARTY_CURRENCY_MISMATCH",`Opening ${item.reference} must use a party with currency ${input.currency}.`);
    const controlLine=input.lines.find(line=>line.accountId===item.controlAccountId);
    if(!controlLine||controlLine.subledgerKind!==item.kind)throw new ApiError(422,"OPENING_CONTROL_KIND_MISMATCH",`Opening ${item.reference} does not match the selected control-account type.`);
    const offset=accountById.get(item.offsetAccountId);
    if(!offset||offset.status!=="active"||item.offsetAccountId===item.controlAccountId)throw new ApiError(422,"OPENING_OFFSET_ACCOUNT_INVALID",`Opening ${item.reference} requires a different active offset account in this chart.`);
    const referenceKey=`${item.kind}:${item.reference.toLocaleLowerCase("en-US")}`;
    if(references.has(referenceKey))throw new ApiError(422,"DUPLICATE_OPENING_REFERENCE","Opening subledger references must be unique for each kind.");
    references.add(referenceKey);
    scheduleByAccount.set(item.controlAccountId,(scheduleByAccount.get(item.controlAccountId)??BigInt(0))+BigInt(amountMinor));
    return {...item,amount:minorToDecimal(amountMinor),partyKind:String(party.kind)};
  });
  for(const line of lines.filter(line=>line.subledgerKind!=="none")){
    const expected=BigInt(decimalToMinor(line.subledgerKind==="receivable"?line.debit:line.credit));
    const scheduled=scheduleByAccount.get(line.accountId)??BigInt(0);
    if(expected!==scheduled)throw new ApiError(422,"OPENING_SUBLEDGER_MISMATCH",`${line.accountCode} ${line.accountName} is ${minorToDecimal(Number(expected))}, but its ${line.subledgerKind} schedule totals ${minorToDecimal(Number(scheduled))}.`);
  }
  if(references.size){
    const existing=await write.prepare(`SELECT kind,invoice_number FROM finance_invoices WHERE company_id=? AND (${[...references].map(()=>"(kind=? AND lower(invoice_number)=?)").join(" OR ")}) LIMIT 1`).bind(actor.companyId,...[...references].flatMap(value=>{const split=value.indexOf(":");return [value.slice(0,split),value.slice(split+1)];})).first<FinanceRow>();
    if(existing)throw new ApiError(409,"OPENING_REFERENCE_EXISTS",`Invoice ${existing.invoice_number} already exists for ${existing.kind}.`);
  }
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
      .map(account=>({id:account.id,status:account.status,isControl:account.is_control,controlKind:account.control_kind,updatedAt:account.updated_at}))
      .sort((a,b)=>String(a.id).localeCompare(String(b.id))),
    parties:parties.results.map(party=>({id:party.id,kind:party.kind,currency:party.currency,status:party.status,version:party.version,updatedAt:party.updated_at})).sort((a,b)=>String(a.id).localeCompare(String(b.id))),
  });
  return {previewToken,chartId:input.chartId,periodId:input.periodId,entryDate:input.entryDate,currency:input.currency,debit:minorToDecimal(Number(debit)),credit:minorToDecimal(Number(credit)),lines,subledgerItems,subledgerCount:subledgerItems.length};
}

export async function importOpeningTrialBalance(actor: WorkContext, input: z.infer<typeof openingTrialBalanceSchema> & {previewToken:string}, id: string) {
  const {previewToken,...definition}=input;
  actor=requirePermission(actor,"finance.write");
  const journalInput:z.infer<typeof createJournalSchema>={ chartId:definition.chartId,periodId:definition.periodId,entryDate:definition.entryDate,currency:definition.currency, memo: "Verified opening trial balance and subledger schedules", sourceType: "opening_import", sourceId:definition.chartId, isOpening: true, lines: definition.lines.map((line) => ({ accountId: line.accountId, debit: line.debit, credit: line.credit, description: line.description })) };
  // Keep the record identifier path-safe and consistent with other journal
  // commands. The idempotency key already scopes this operation, so adding a
  // second suffix here only creates an unnecessary colon in the URL segment.
  const {write}=await resources(actor),commandId=id,journalId=`journal-${commandId}`;
  const commandInput={journal:journalInput,subledgerItems:definition.subledgerItems};
  const replay=await replayCommand<{id:string;number:string;status:string;version:number;debit:string;credit:string;subledgerCount:number}>(write,actor,commandId,"finance.journal.create",journalId,commandInput);if(replay)return replay;
  const preview=await previewOpeningTrialBalance(actor,definition);
  if(preview.previewToken!==previewToken)throw new ApiError(409,"OPENING_PREVIEW_STALE","The chart, period, accounts or opening balances changed. Preview the import again.");
  const journal=await prepareJournal(write,actor,journalInput,commandId),now=Date.now();
  const invoiceSteps=definition.subledgerItems.flatMap((item,index)=>{
    const amount=decimalToMinor(item.amount),invoiceId=`${journalId}-subledger-${index}`,receivable=item.kind==="receivable";
    return [
      assertion(write,commandId,`opening-party-${index}`,"EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind=? AND currency=? AND status='active')",[item.partyId,actor.companyId,receivable?"customer":"supplier",definition.currency]),
      assertion(write,commandId,`opening-control-${index}`,"EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND status='active' AND is_control=1)",[item.controlAccountId,actor.companyId,definition.chartId]),
      assertion(write,commandId,`opening-reference-${index}`,"NOT EXISTS(SELECT 1 FROM finance_invoices WHERE company_id=? AND kind=? AND lower(invoice_number)=lower(?))",[actor.companyId,item.kind,item.reference]),
      write.prepare("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,base_total_minor,base_outstanding_minor,debit_account_id,credit_account_id,source_type,source_id,status,journal_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,0,?,?,?,?,?,?,?,?,'draft',?,?,?,?)").bind(invoiceId,actor.companyId,item.partyId,item.kind,item.reference,definition.entryDate,item.dueDate,definition.currency,amount,amount,amount,amount,amount,receivable?item.controlAccountId:item.offsetAccountId,receivable?item.offsetAccountId:item.controlAccountId,"opening_import",journalId,journalId,actor.userId,now,now),
    ];
  });
  const result={...journal.result,subledgerCount:definition.subledgerItems.length};
  return executeCommand(write,actor,commandId,"finance.journal.create",commandInput,journalId,1,result,[...journal.steps,...invoiceSteps]);
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

export async function workInvoiceRows(actor: WorkContext) {
  actor = requirePermission(actor,"finance.read");
  const { write } = await resources(actor,true);
  return (await write.prepare("SELECT i.* FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id WHERE i.company_id=? ORDER BY i.updated_at DESC").bind(actor.companyId).all<FinanceRow>()).results;
}
export async function getInvoice(actor: WorkContext, invoiceId: string) {
  actor = requirePermission(actor,"finance.read");

  const { write } = await resources(actor,true);
  const invoice = await write.prepare(`SELECT i.*,p.code AS party_code,p.name AS party_name,
    s.usd_units::text AS snapshot_usd_units,s.amount_units::text AS snapshot_source_units,s.fx_rate_id AS snapshot_rate_id
    FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id
    LEFT JOIN money_transaction_snapshots s ON s.company_id=i.company_id AND s.source_type='finance_invoice' AND s.source_id=i.id AND s.amount_kind='total'
    WHERE i.id=? AND i.company_id=?`).bind(invoiceId, actor.companyId).first<FinanceRow>();
  if (!invoice) throw new ApiError(404, "INVOICE_NOT_FOUND", "Invoice not found in this company.");
  const allocations = await write.prepare(`SELECT a.id,a.payment_id,a.amount_minor,a.created_at,p.payment_number,p.status,p.payment_slip_id
    FROM finance_allocations a JOIN finance_payments p ON p.id=a.payment_id
    WHERE a.invoice_id=? AND a.company_id=? ORDER BY a.created_at`).bind(invoiceId, actor.companyId).all();
  const [lines,lineHistory,thaiDocuments] = await Promise.all([
    currentInvoiceLines(write,actor.companyId,invoiceId,Number(invoice.line_revision)),
    write.prepare("SELECT source_version,origin_kind,COUNT(*) line_count,SUM(subtotal_minor) subtotal_minor,SUM(tax_minor) tax_minor,MIN(created_at) created_at FROM finance_invoice_line_versions WHERE invoice_id=? AND company_id=? GROUP BY source_version,origin_kind ORDER BY source_version").bind(invoiceId,actor.companyId).all(),
    write.prepare("SELECT id,document_number,document_kind,edition,issue_date FROM finance_issued_thai_documents WHERE invoice_id=? AND company_id=? ORDER BY edition").bind(invoiceId,actor.companyId).all(),
  ]);
  return { invoice, allocations: allocations.results, lines, lineHistory:lineHistory.results,thaiDocuments:thaiDocuments.results };
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
  const allocations = await write.prepare(`SELECT a.id,a.invoice_id,a.amount_minor,a.created_at,i.invoice_number,i.status
    FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id
    WHERE a.payment_id=? AND a.company_id=? ORDER BY a.created_at`).bind(paymentId, actor.companyId).all();
  const canConfirm=hasPermission(actor,"finance.pay")&&!actor.branchId&&!actor.departmentId;
  const [reversal,periods,invoices,bankTransactions,fxAccounts] = await Promise.all([
    write.prepare("SELECT * FROM finance_payment_reversals WHERE payment_id=?").bind(paymentId).first<FinanceRow>(),
    write.prepare("SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date DESC").bind(actor.companyId).all<FinanceRow>(),
    canConfirm&&payment.party_id&&["payment","receipt"].includes(String(payment.kind))?write.prepare(`SELECT id,invoice_number,kind,outstanding_minor,status,version
      FROM finance_invoices WHERE company_id=? AND party_id=? AND currency=? AND kind=? AND status IN ('posted','partially_paid') AND outstanding_minor>0 AND source_type!='expense_claim'
      ORDER BY due_date,invoice_number`).bind(actor.companyId,payment.party_id,payment.currency,payment.kind==="receipt"?"receivable":"payable").all<FinanceRow>():Promise.resolve({results:[] as FinanceRow[]}),
    canConfirm&&payment.bank_account?write.prepare(`SELECT t.id,t.transaction_date,t.external_id,t.description,t.amount_minor,t.currency,t.match_status,t.version,s.statement_number
      FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id
      WHERE t.company_id=? AND s.bank_account=? AND t.currency=? AND t.match_status IN ('unmatched','proposed') AND t.amount_minor=? AND t.transaction_date>=?
      ORDER BY t.transaction_date DESC,t.external_id`).bind(actor.companyId,payment.bank_account,payment.currency,payment.kind==="receipt"?payment.amount_minor:-(Number(payment.amount_minor)-Number(payment.withheld_minor??0)),payment.payment_date).all<FinanceRow>():Promise.resolve({results:[] as FinanceRow[]}),
    canConfirm&&payment.bank_account?write.prepare(`SELECT a.id,a.code,a.name,a.category,a.currency,c.base_currency
      FROM finance_accounts bank JOIN finance_charts c ON c.id=bank.chart_id JOIN finance_accounts a ON a.chart_id=c.id
      WHERE bank.id=? AND bank.company_id=? AND a.company_id=bank.company_id AND a.status='active' AND a.currency=c.base_currency AND a.category IN ('other_income','other_expense')
      ORDER BY a.code`).bind(payment.bank_account,actor.companyId).all<FinanceRow>():Promise.resolve({results:[] as FinanceRow[]}),
  ]);
  return { payment, allocations: allocations.results, reversal, periods:periods.results, invoices:invoices.results, bankTransactions:bankTransactions.results,fxAccounts:fxAccounts.results };
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
  const payment=await write.prepare("SELECT p.*,j.chart_id,j.status journal_status,j.journal_number,j.entry_date journal_entry_date FROM finance_payments p JOIN finance_journals j ON j.id=p.journal_id WHERE p.id=? AND p.company_id=?").bind(paymentId,actor.companyId).first<FinanceRow>();
  if(!payment)throw new ApiError(404,"POSTED_PAYMENT_NOT_FOUND","A posted payment with accounting evidence was not found.");
  if(!hasPermission(actor,"finance.approve")||samePerson(actor,payment.created_by))throw new ApiError(403,"INDEPENDENT_CORRECTION_REQUIRED","A different finance approver must reverse the payment.");
  if(payment.status!=="settled"||payment.journal_status!=="posted")throw new ApiError(409,"PAYMENT_NOT_REVERSIBLE","Only a settled payment with a posted journal can be reversed.");
  if(input.entryDate<String(payment.journal_entry_date))throw new ApiError(422,"REVERSAL_BEFORE_SOURCE","The reversal cannot precede the payment date.");
  const allocations=await write.prepare("SELECT a.*,i.total_minor,i.outstanding_minor,i.source_type FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id WHERE a.payment_id=? AND a.company_id=? ORDER BY a.id").bind(paymentId,actor.companyId).all<FinanceRow>();
  if(!allocations.results.length)throw new ApiError(409,"PAYMENT_ALLOCATION_REQUIRED","Only an allocated payment can be corrected here.");
  if(allocations.results.some(row=>["expense_claim","pms"].includes(String(row.source_type))))throw new ApiError(409,"REVERSE_AT_SOURCE","Reverse this payment from its originating module.");
  const now=Date.now(),reversal=`payment-reversal-${id}`,fxReversal=payment.fx_journal_id?`payment-fx-reversal-${id}`:null;
  const steps:SqlStatement[]=[
    assertion(write,id,"payment","EXISTS(SELECT 1 FROM finance_payments p JOIN finance_journals j ON j.id=p.journal_id WHERE p.id=? AND p.company_id=? AND p.version=? AND p.status='settled' AND j.status='posted')",[paymentId,actor.companyId,input.expectedVersion]),
    assertion(write,id,"bank-match","(SELECT COUNT(*) FROM finance_bank_transactions WHERE company_id=? AND matched_type='payment' AND matched_id=? AND match_status='matched')<=1",[actor.companyId,paymentId]),
    assertion(write,id,"period","EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)",[input.periodId,actor.companyId,input.entryDate,input.entryDate]),
    assertion(write,id,"not-reversed","NOT EXISTS(SELECT 1 FROM finance_payment_reversals WHERE payment_id=?)",[paymentId]),
    write.prepare("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,chart_id,?,?,?,currency,fx_rate_micros,'payment_reversal',?,?,'posted',?,?,?,?,? FROM finance_journals WHERE id=?").bind(reversal,input.periodId,`PAY-REV-${id}`,input.entryDate,paymentId,input.reason,actor.userId,actor.userId,now,now,now,payment.journal_id),
    write.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?||'-'||ordinal,?,account_id,description,credit_minor,debit_minor,base_credit_minor,base_debit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=?").bind(reversal,reversal,payment.journal_id),
    write.prepare("UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=?,reversed_at=?,updated_at=? WHERE id=?").bind(actor.userId,now,now,payment.journal_id),
    ...(fxReversal?[
      assertion(write,id,"fx-journal","EXISTS(SELECT 1 FROM finance_journals WHERE id=? AND company_id=? AND status='posted')",[payment.fx_journal_id,actor.companyId]),
      write.prepare("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,chart_id,?,?,?,currency,fx_rate_micros,'payment_fx_reversal',?,?,'posted',?,?,?,?,? FROM finance_journals WHERE id=?").bind(fxReversal,input.periodId,`PAY-FX-REV-${id}`,input.entryDate,paymentId,input.reason,actor.userId,actor.userId,now,now,now,payment.fx_journal_id),
      write.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?||'-'||ordinal,?,account_id,description,credit_minor,debit_minor,base_credit_minor,base_debit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=?").bind(fxReversal,fxReversal,payment.fx_journal_id),
      write.prepare("UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=?,reversed_at=?,updated_at=? WHERE id=?").bind(actor.userId,now,now,payment.fx_journal_id),
    ]:[]),
  ];
  for(const allocation of allocations.results){
    const restored=Number(allocation.outstanding_minor)+Number(allocation.amount_minor);
    if(restored>Number(allocation.total_minor))throw new ApiError(409,"INVOICE_BALANCE_CONFLICT","The allocated invoice balance no longer permits this reversal.");
    steps.push(
      assertion(write,id,`return-${allocation.id}`,"NOT EXISTS(SELECT 1 FROM commerce_returns WHERE invoice_id=? AND status!='rejected')",[allocation.invoice_id]),
      assertion(write,id,`invoice-${allocation.id}`,"EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND outstanding_minor=? AND total_minor>=outstanding_minor+?)",[allocation.invoice_id,allocation.outstanding_minor,allocation.amount_minor]),
      write.prepare("UPDATE finance_invoices SET outstanding_minor=outstanding_minor+?,base_outstanding_minor=base_outstanding_minor+?,status=CASE WHEN outstanding_minor+?=total_minor THEN 'posted' ELSE 'partially_paid' END,version=version+1,updated_at=? WHERE id=?").bind(allocation.amount_minor,allocation.base_invoice_minor,allocation.amount_minor,now,allocation.invoice_id),
    );
  }
  const taxReversal=await prepareWithholdingReversal(write,actor,payment,id,reversal,input.reason);
  const originalMoney=await write.prepare("SELECT 1 AS present FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_payment' AND source_id=? AND amount_kind='settled'").bind(actor.companyId,paymentId).first<{present:number}>();
  const moneyReversal=originalMoney?await snapshotMoneyReversal(write,actor,{sourceType:"finance_payment_reversal",sourceId:reversal,amountKind:"gross",transactionDate:input.entryDate,original:{sourceType:"finance_payment",sourceId:paymentId,amountKind:"settled"},amountUnits:String(payment.amount_minor)}):null;
  const taxMoneyReversals:SqlStatement[]=[];
  if(payment.withholding_assessment_id){
    for(const [amountKind,amount] of [["bank_settled",Number(payment.amount_minor)-Number(payment.withheld_minor)],["withheld",Number(payment.withheld_minor)]] as const){
      const source=await write.prepare("SELECT 1 FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_payment' AND source_id=? AND amount_kind=?").bind(actor.companyId,paymentId,amountKind).first();
      if(source&&amount>0)taxMoneyReversals.push((await snapshotMoneyReversal(write,actor,{sourceType:"finance_payment_reversal",sourceId:reversal,amountKind,transactionDate:input.entryDate,original:{sourceType:"finance_payment",sourceId:paymentId,amountKind},amountUnits:String(amount)})).statement);
    }
  }
  steps.push(...taxReversal,...(moneyReversal?[moneyReversal.statement]:[]),...taxMoneyReversals,
    write.prepare("UPDATE finance_bank_transactions SET match_status='unmatched',matched_type=NULL,matched_id=NULL,version=version+1 WHERE company_id=? AND matched_type='payment' AND matched_id=? AND match_status='matched'").bind(actor.companyId,paymentId),
    write.prepare("UPDATE finance_payments SET status='voided',version=version+1,updated_at=? WHERE id=?").bind(now,paymentId),
    write.prepare("INSERT INTO finance_payment_reversals(id,payment_id,reversal_journal_id,fx_reversal_journal_id,reason,reversed_by,reversed_at) VALUES(?,?,?,?,?,?,?)").bind(`payment-correction-${id}`,paymentId,reversal,fxReversal,input.reason,actor.userId,now),
  );
  return executeCommand(write,actor,id,"finance.payment.reverse",input,paymentId,input.expectedVersion+1,{id:paymentId,status:"voided",version:input.expectedVersion+1,reversalJournalId:reversal},steps);
}

async function journalPostSteps(write:SqlDatabase,actor:WorkContext,journal:FinanceRow,id:string,input:{expectedVersion:number}){
  const journalId=String(journal.id);
  if(samePerson(actor,journal.created_by)||!hasPermission(actor,"finance.approve"))throw new ApiError(403,"INDEPENDENT_APPROVER_REQUIRED","A different finance approver must post a manual journal.");
  const debit=await write.prepare("SELECT COALESCE(SUM(debit_minor),0) AS amount FROM finance_journal_lines WHERE journal_id=?").bind(journalId).first<{amount:number}>();
  const snapshot=await legacyMoneySnapshot(write,actor,{sourceType:"finance_journal",sourceId:journalId,amountKind:"debit_total",transactionDate:String(journal.entry_date),currency:String(journal.currency),amountUnits:Number(debit?.amount??0)});
  const now=Date.now(),opening=journal.source_type==="opening_import";return [
    assertion(write,id,"journal","EXISTS(SELECT 1 FROM finance_journals WHERE id=? AND company_id=? AND version=? AND status IN ('draft','in_review'))",[journalId,actor.companyId,input.expectedVersion]),
    assertion(write,id,"period","EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)",[journal.period_id,actor.companyId,journal.entry_date,journal.entry_date]),
    assertion(write,id,"active-accounts","NOT EXISTS(SELECT 1 FROM finance_journal_lines l LEFT JOIN finance_accounts a ON a.id=l.account_id AND a.company_id=? AND a.chart_id=? LEFT JOIN finance_charts c ON c.id=a.chart_id WHERE l.journal_id=? AND (a.id IS NULL OR a.status!='active' OR a.currency NOT IN (?,c.base_currency)))",[actor.companyId,journal.chart_id,journalId,journal.currency]),
    assertion(write,id,"balanced","(SELECT COUNT(*)>=2 AND SUM(base_debit_minor)=SUM(base_credit_minor) AND SUM(debit_minor)=SUM(credit_minor) FROM finance_journal_lines WHERE journal_id=?)",[journalId]),
    ...(opening?[assertion(write,id,"opening-subledgers","NOT EXISTS(SELECT 1 FROM finance_invoices WHERE company_id=? AND source_type='opening_import' AND source_id=? AND (status!='draft' OR journal_id!=?))",[actor.companyId,journalId,journalId])]:[]),
    write.prepare("UPDATE finance_journals SET status='posted',version=version+1,posted_by=?,posted_at=?,updated_at=? WHERE id=?").bind(actor.userId,now,now,journalId),
    ...(snapshot?[snapshot]:[]),
    ...(opening?[write.prepare("UPDATE finance_invoices SET status='posted',approved_by=?,posted_at=?,version=version+1,updated_at=? WHERE company_id=? AND source_type='opening_import' AND source_id=? AND status='draft'").bind(actor.userId,now,now,actor.companyId,journalId)]:[]),
  ];
}

export async function postJournal(actor:WorkContext,journalId:string,id:string,input:{expectedVersion:number}){
  actor=requirePermission(actor,"finance.write");
  const {write}=await resources(actor),previous=await replayCommand(write,actor,id,"finance.journal.post",journalId,input);if(previous)return previous;
  const journal=await write.prepare("SELECT * FROM finance_journals WHERE id=? AND company_id=?").bind(journalId,actor.companyId).first<FinanceRow>();
  if(!journal)throw new ApiError(404,"JOURNAL_NOT_FOUND","Journal not found.");
  return executeCommand(write,actor,id,"finance.journal.post",input,journalId,input.expectedVersion+1,{id:journalId,status:"posted",version:input.expectedVersion+1},await journalPostSteps(write,actor,journal,id,input));
}

export const journalBatchSchema=z.object({
  journals:z.array(z.object({id:z.string().min(1).max(128),expectedVersion:z.number().int().positive()})).min(1).max(100),
  reason:z.string().trim().min(10).max(2000),
});
export async function postJournalBatch(actor:WorkContext,input:z.infer<typeof journalBatchSchema>,id:string){
  actor=requirePermission(requirePermission(actor,"finance.write"),"finance.approve");input=journalBatchSchema.parse(input);
  const {write}=await resources(actor),batchId=`journal-batch-${id}`;
  const previous=await replayCommand(write,actor,id,"finance.batch.post",batchId,input);if(previous)return previous;
  if(new Set(input.journals.map(j=>j.id)).size!==input.journals.length)throw new ApiError(422,"DUPLICATE_JOURNAL","Select each draft once.");
  const journals=[] as FinanceRow[];
  for(const item of input.journals){
    const journal=await write.prepare("SELECT * FROM finance_journals WHERE id=? AND company_id=?").bind(item.id,actor.companyId).first<FinanceRow>();
    if(!journal)throw new ApiError(404,"JOURNAL_NOT_FOUND","A selected journal is not in this Company.");
    if(!["manual","opening_import"].includes(String(journal.source_type)))throw new ApiError(409,"BATCH_SOURCE_ADAPTER_REQUIRED","Review source-owned drafts through their module until its batch adapter validates source versions and approved rules.");
    if(journal.version!==item.expectedVersion||!["draft","in_review"].includes(String(journal.status)))throw new ApiError(409,"VERSION_CONFLICT","A selected draft changed. Review the entire batch again.");
    if(journals.length&&["chart_id","period_id","currency"].some(field=>journal[field]!==journals[0][field]))throw new ApiError(422,"BATCH_SCOPE_MISMATCH","A batch requires one Company, ledger, period and transaction currency.");
    journals.push(journal);
  }
  const steps:SqlStatement[]=[],posted=[] as Array<{id:string;version:number}>;let lineCount=0;
  for(const [index,journal] of journals.entries()){
    const item=input.journals[index],childKey=`${id}:${index}`;
    const lines=(await write.prepare("SELECT id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=? ORDER BY ordinal").bind(item.id).all<FinanceRow>()).results;
    lineCount+=lines.length;if(lineCount>1000)throw new ApiError(422,"BATCH_LINE_LIMIT","A batch may contain at most 1,000 lines.");
    const fingerprint=JSON.stringify(lines.map(l=>[l.id,l.account_id,l.debit_minor,l.credit_minor,l.base_debit_minor,l.base_credit_minor,l.dimension_json,l.ordinal]));
    const flow=await prepareFlowCommand(write,actor,childKey,"journal",item.id,"post",{...item,reason:input.reason},item.expectedVersion+1);
    steps.push(...flow.before,
      assertion(write,childKey,"batch-lines","(SELECT jsonb_agg(jsonb_build_array(id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) ORDER BY ordinal) FROM finance_journal_lines WHERE journal_id=?)=?::jsonb",[item.id,fingerprint]),
      ...await journalPostSteps(write,actor,journal,childKey,item),...flow.after,
      write.prepare("INSERT INTO finance_batch_review_items(command_id,journal_id,company_id,reviewed_version,reviewed_by,assignment_id,evidence_digest,reason,reviewed_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(id,item.id,actor.companyId,item.expectedVersion,actor.userId,actor.assignmentId,await hash({batchId,reason:input.reason,lines}),input.reason,Date.now()),
      write.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(childKey));
    posted.push({id:item.id,version:item.expectedVersion+1});
  }
  return executeCommand(write,actor,id,"finance.batch.post",input,batchId,1,{id:batchId,status:"posted",journals:posted,lineCount},steps);
}

export async function reverseJournal(actor: WorkContext, journalId: string, id: string, input: { periodId: string; entryDate: string; memo?: string }) {
  actor = requirePermission(actor,"finance.write");

  requireFinanceRole(actor,true);const {write}=await resources(actor);const previous=await replayCommand(write,actor,id,"finance.journal.reverse",journalId,input);if(previous)return previous;
  const journal=await write.prepare("SELECT * FROM finance_journals WHERE id=? AND company_id=? AND status='posted'").bind(journalId,actor.companyId).first<FinanceRow>();if(!journal)throw new ApiError(404,"POSTED_JOURNAL_NOT_FOUND","Posted journal not found.");
  if(!["manual","opening_import","reversal"].includes(String(journal.source_type)))throw new ApiError(409,"REVERSE_AT_SOURCE","Reverse the originating business document so its stock and settlement records remain consistent.");
  const opening=journal.source_type==="opening_import";
  if(opening&&await write.prepare("SELECT 1 FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id WHERE i.company_id=? AND i.source_type='opening_import' AND i.source_id=? LIMIT 1").bind(actor.companyId,journalId).first())throw new ApiError(409,"OPENING_SUBLEDGER_SETTLED","This opening journal has payment or receipt allocations. Reverse those settlements and correct the affected documents instead.");
  if(input.entryDate<String(journal.entry_date))throw new ApiError(422,"REVERSAL_BEFORE_SOURCE","The reversal cannot precede the original accounting date.");
  if(journal.source_type==="reversal"&&await write.prepare("SELECT 1 FROM finance_journals WHERE id=? AND source_type='opening_import'").bind(journal.source_id).first())throw new ApiError(409,"REIMPORT_OPENING_BALANCE","Prepare a corrected opening import instead of reversing an opening reversal.");
  const reversal=`reversal-${id}`,now=Date.now();return executeCommand(write,actor,id,"finance.journal.reverse",input,journalId,Number(journal.version)+1,{reversalJournalId:reversal,reversedJournalId:journalId},[
    assertion(write,id,"source","EXISTS(SELECT 1 FROM finance_journals WHERE id=? AND version=? AND status='posted')",[journalId,journal.version]),
    ...(opening?[assertion(write,id,"opening-unsettled","NOT EXISTS(SELECT 1 FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id WHERE i.company_id=? AND i.source_type='opening_import' AND i.source_id=?)",[actor.companyId,journalId])]:[]),
    assertion(write,id,"period","EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)",[input.periodId,actor.companyId,input.entryDate,input.entryDate]),
    write.prepare("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) SELECT ?,company_id,chart_id,?,?,?,currency,fx_rate_micros,'reversal',id,?,'posted',?,?,?,?,? FROM finance_journals WHERE id=?").bind(reversal,input.periodId,`REV-${id}`,input.entryDate,input.memo??`Reversal of ${journal.journal_number}`,actor.userId,actor.userId,now,now,now,journalId),
    write.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) SELECT ?||'-'||ordinal,?,account_id,description,credit_minor,debit_minor,base_credit_minor,base_debit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=?").bind(reversal,reversal,journalId),
    write.prepare("UPDATE finance_journals SET status='reversed',version=version+1,reversed_by=?,reversed_at=?,updated_at=? WHERE id=?").bind(actor.userId,now,now,journalId),
    ...(opening?[write.prepare("UPDATE finance_invoices SET status='voided',outstanding_minor=0,base_outstanding_minor=0,version=version+1,updated_at=? WHERE company_id=? AND source_type='opening_import' AND source_id=?").bind(now,actor.companyId,journalId)]:[]),
  ]);
}

export const reportDateSchema=z.object({startDate:z.string().date().optional(),endDate:z.string().date().optional()})
  .refine(value=>!value.startDate||!value.endDate||value.startDate<=value.endDate,{message:"Report start must not follow its end date."});
export type ReportDates=z.infer<typeof reportDateSchema>;
export async function trialBalance(actor: WorkContext, periodId?: string, chartId?: string, dates:ReportDates={},options:{excludeYearClose?:boolean}={}) {
  actor = requirePermission(actor,"finance.read");
  dates=reportDateSchema.parse(dates);

  const { write } = await resources(actor,true); const clauses = ["j.company_id=?", "j.status IN ('posted','reversed')"]; const args: (string | number)[] = [actor.companyId]; if (periodId) { clauses.push("j.period_id=?"); args.push(periodId); }
  if(options.excludeYearClose)clauses.push("j.source_type<>'fiscal_close'");
  if(!chartId){
    const charts=await write.prepare("SELECT id FROM finance_charts WHERE company_id=?").bind(actor.companyId).all<{id:string}>();
    if(charts.results.length>1)throw new ApiError(422,"CHART_REQUIRED","Select a ledger explicitly when the Company has multiple ledgers, including archived ledgers.");
    chartId=charts.results[0]?.id;
  }
  if(chartId){clauses.push("j.chart_id=?");args.push(chartId);}
  if(chartId&&!await write.prepare("SELECT 1 FROM finance_charts WHERE id=? AND company_id=?").bind(chartId,actor.companyId).first())throw new ApiError(404,"CHART_NOT_FOUND","Ledger not found in this Company.");
  if(periodId&&!await write.prepare("SELECT 1 FROM finance_periods WHERE id=? AND company_id=?").bind(periodId,actor.companyId).first())throw new ApiError(404,"PERIOD_NOT_FOUND","Report period not found in this Company.");
  if(dates.startDate){clauses.push("j.entry_date>=?");args.push(dates.startDate);}
  if(dates.endDate){clauses.push("j.entry_date<=?");args.push(dates.endDate);}
  const rows = await write.prepare(`SELECT a.id,a.code,a.name,a.category,COALESCE(SUM(l.base_debit_minor),0) AS debit,COALESCE(SUM(l.base_credit_minor),0) AS credit FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id JOIN finance_accounts a ON a.id=l.account_id WHERE ${clauses.join(" AND ")} GROUP BY a.id,a.code,a.name,a.category ORDER BY a.code`).bind(...args).all<FinanceRow>();
  return rows.results.map((row): FinanceRow & { debit: string; credit: string; balance: string } => ({ ...row, debit: minorToDecimal(Number(row.debit)), credit: minorToDecimal(Number(row.credit)), balance: minorToDecimal(Number(row.debit) - Number(row.credit)) }));
}

export async function statements(actor: WorkContext, periodId?: string, chartId?: string, dates:ReportDates={}) {
  actor=requirePermission(actor,"finance.read");dates=reportDateSchema.parse(dates);
  if(periodId){
    const {write}=await resources(actor,true),period=await write.prepare("SELECT start_date,end_date FROM finance_periods WHERE id=? AND company_id=?").bind(periodId,actor.companyId).first<{start_date:string;end_date:string}>();
    if(!period)throw new ApiError(404,"PERIOD_NOT_FOUND","Report period not found in this Company.");
    dates={startDate:period.start_date,endDate:period.end_date};
  }
  const [tb,balance,income]=await Promise.all([trialBalance(actor,undefined,chartId,dates),trialBalance(actor,undefined,chartId,{endDate:dates.endDate}),trialBalance(actor,undefined,chartId,dates,{excludeYearClose:true})]);
  const category=(rows:typeof tb,name:string)=>rows.filter(row=>row.category===name).reduce((total,row)=>total+decimalToMinor(String(row.balance)),0);
  const recordedEquity=-category(balance,"equity"),unclosedProfit=-category(balance,"revenue")-category(balance,"other_income")-category(balance,"expense")-category(balance,"other_expense");
  return {dates,balanceSheet:{assets:minorToDecimal(category(balance,"asset")),liabilities:minorToDecimal(-category(balance,"liability")),equity:minorToDecimal(recordedEquity+unclosedProfit),recordedEquity:minorToDecimal(recordedEquity),unclosedProfit:minorToDecimal(unclosedProfit)},
    profitAndLoss:{revenue:minorToDecimal(-category(income,"revenue")),expenses:minorToDecimal(category(income,"expense")),otherIncome:minorToDecimal(-category(income,"other_income")),otherExpenses:minorToDecimal(category(income,"other_expense")),netProfit:minorToDecimal(-category(income,"revenue")-category(income,"expense")-category(income,"other_income")-category(income,"other_expense"))},trialBalance:tb};
}

export type CloseReconciliation = {
  kind: "receivable" | "payable" | "bank";
  accountId: string;
  accountCode: string;
  accountName: string;
  chartName: string;
  currency: string;
  ledgerMinor: number;
  sourceMinor: number;
  differenceMinor: number;
  source: Record<string, unknown>;
};

export async function calculateCloseReconciliations(write: SqlDatabase, actor: WorkContext, period: FinanceRow) {
  const accounts = await write.prepare("SELECT a.id,a.code,a.name,a.currency,a.control_kind,c.name chart_name,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.status='active' AND c.status='active' AND a.control_kind IN ('receivable','payable','bank') ORDER BY c.name,a.code").bind(actor.companyId).all<FinanceRow>();
  const rows: CloseReconciliation[] = [];
  for (const account of accounts.results) {
    const kind = String(account.control_kind) as CloseReconciliation["kind"];
    const accountId = String(account.id);
    const ledgerSigned = Number(await write.prepare(`SELECT COALESCE(SUM(l.base_debit_minor-l.base_credit_minor),0) value FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND l.account_id=? AND j.status IN ('posted','reversed') AND j.entry_date<=?`).bind(actor.companyId,accountId,period.end_date).first("value") ?? 0);
    const ledgerMinor = ledgerSigned===0?0:kind === "payable" ? -ledgerSigned : ledgerSigned;
    if (kind === "bank") {
      const statement = await write.prepare("SELECT id,statement_number,statement_date,closing_minor,closing_fx_rate_micros,base_closing_minor,status FROM finance_bank_statements WHERE company_id=? AND bank_account=? AND statement_date<=? ORDER BY statement_date DESC,created_at DESC LIMIT 1").bind(actor.companyId,accountId,period.end_date).first<FinanceRow>();
      const sourceMinor = statement ? Number(Number(statement.base_closing_minor)===0&&Number(statement.closing_minor)!==0&&Number(statement.closing_fx_rate_micros??1_000_000)===1_000_000?statement.closing_minor:statement.base_closing_minor) : 0;
      rows.push({kind,accountId,accountCode:String(account.code),accountName:String(account.name),chartName:String(account.chart_name),currency:String(account.currency),ledgerMinor,sourceMinor,differenceMinor:ledgerMinor-sourceMinor,source:{statementId:statement?.id??null,statementNumber:statement?.statement_number??null,statementDate:statement?.statement_date??null,transactionClosingMinor:statement?.closing_minor??null,closingFxRateMicros:statement?.closing_fx_rate_micros??null,periodEnd:period.end_date,evidenceComplete:statement?.statement_date===period.end_date}});
      continue;
    }
    const source = await write.prepare(`${invoiceHistoryCte}
      SELECT COALESCE(SUM(i.historical_base_outstanding_minor),0) value,COUNT(*) invoice_count,
        COUNT(*) FILTER(WHERE i.currency<>? AND i.historical_outstanding_minor>0 AND NOT EXISTS(
          SELECT 1 FROM finance_fx_revaluation_lines l JOIN finance_fx_revaluation_runs r ON r.id=l.run_id
          WHERE l.invoice_id=i.id AND r.period_id=? AND r.status='posted')) foreign_open_count,
        COUNT(*) FILTER(WHERE i.legacy_date_evidence) legacy_date_count
      FROM historical_invoice_balances i WHERE i.kind=? AND
        CASE WHEN i.kind='receivable' THEN i.debit_account_id ELSE i.credit_account_id END=?`)
      .bind(actor.companyId,period.end_date,account.base_currency,period.id,kind,accountId).first<FinanceRow>();
    const sourceMinor=Number(source?.value??0);
    rows.push({kind,accountId,accountCode:String(account.code),accountName:String(account.name),chartName:String(account.chart_name),currency:String(account.currency),ledgerMinor,sourceMinor,differenceMinor:ledgerMinor-sourceMinor,source:{invoiceCount:Number(source?.invoice_count??0),foreignOpenCount:Number(source?.foreign_open_count??0),legacyDateCount:Number(source?.legacy_date_count??0),asOf:period.end_date}});
  }
  const vat=await calculateVatCloseEvidence(write,actor,String(period.end_date));
  const ready=(!vat||vat.ready)&&rows.every(row=>row.differenceMinor===0&&Number(row.source.foreignOpenCount??0)===0&&Number(row.source.legacyDateCount??0)===0&&(row.kind!=="bank"||row.source.evidenceComplete===true));
  const reconciliationHash=await hash({rows:rows.map(row=>({kind:row.kind,accountId:row.accountId,currency:row.currency,ledgerMinor:row.ledgerMinor,sourceMinor:row.sourceMinor,differenceMinor:row.differenceMinor,source:row.source})),vat});
  return {rows,vat,ready,reconciliationHash};
}

function reconciliationAssertions(write:SqlDatabase,actor:WorkContext,period:FinanceRow,key:string,rows:CloseReconciliation[]) {
  return rows.flatMap((row,index)=>{
    const signedLedger=row.kind==="payable"?-row.ledgerMinor:row.ledgerMinor;
    const checks=[assertion(write,key,`reconciliation-ledger-${index}`,"COALESCE((SELECT SUM(l.base_debit_minor-l.base_credit_minor) FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? AND l.account_id=? AND j.status IN ('posted','reversed') AND j.entry_date<=?),0)=?",[actor.companyId,row.accountId,period.end_date,signedLedger])];
    if(row.kind==="bank")checks.push(assertion(write,key,`reconciliation-bank-${index}`,"EXISTS(SELECT 1 FROM finance_bank_statements WHERE company_id=? AND bank_account=? AND statement_date=? AND CASE WHEN base_closing_minor=0 AND closing_minor!=0 AND closing_fx_rate_micros=1000000 THEN closing_minor ELSE base_closing_minor END=?)",[actor.companyId,row.accountId,period.end_date,row.sourceMinor]));
    else {
      checks.push(assertion(write,key,`reconciliation-subledger-${index}`,`${invoiceHistoryCte}
        SELECT COALESCE(SUM(historical_base_outstanding_minor),0)=? AND COUNT(*) FILTER(WHERE legacy_date_evidence)=0 FROM historical_invoice_balances
        WHERE kind=? AND CASE WHEN kind='receivable' THEN debit_account_id ELSE credit_account_id END=?`,
        [actor.companyId,period.end_date,row.sourceMinor,row.kind,row.accountId]));
      checks.push(assertion(write,key,`reconciliation-foreign-${index}`,`${invoiceHistoryCte}
        SELECT COUNT(*)=? FROM historical_invoice_balances i JOIN finance_charts c ON c.id=i.chart_id
        WHERE i.kind=? AND CASE WHEN i.kind='receivable' THEN i.debit_account_id ELSE i.credit_account_id END=?
          AND i.currency<>c.base_currency AND i.historical_outstanding_minor>0 AND NOT EXISTS(
            SELECT 1 FROM finance_fx_revaluation_lines l JOIN finance_fx_revaluation_runs r ON r.id=l.run_id
            WHERE l.invoice_id=i.id AND r.period_id=? AND r.status='posted')`,
        [actor.companyId,period.end_date,Number(row.source.foreignOpenCount??0),row.kind,row.accountId,period.id]));
    }
    return checks;
  });
}

export const prepareFxRevaluationSchema=z.object({
  chartId:z.string().min(1),periodId:z.string().min(1),entryDate:z.string().date(),
  rates:z.array(z.object({currency:z.string().length(3).transform(value=>value.toUpperCase()),rate:z.string()})).min(1).max(50),
  gainAccountId:z.string().min(1),lossAccountId:z.string().min(1),proof:z.string().trim().min(3).max(2000),
});
export const decideFxRevaluationSchema=z.object({expectedVersion:z.number().int().positive(),proof:z.string().trim().min(3).max(2000)});

type FxExposure={invoiceId:string;kind:"receivable"|"payable";currency:string;controlAccountId:string;transactionOutstanding:number;priorBase:number;revaluedBase:number;adjustment:number;rateMicros:number;version:number};

async function fxExposures(write:SqlDatabase,actor:WorkContext,chartId:string,period:FinanceRow,rates:Record<string,string>){
  const chart=await write.prepare("SELECT id,base_currency FROM finance_charts WHERE id=? AND company_id=? AND status='active'").bind(chartId,actor.companyId).first<FinanceRow>();
  if(!chart)throw new ApiError(404,"CHART_NOT_FOUND","Active ledger chart not found.");
  const rows=await write.prepare(`SELECT i.id,i.kind,i.currency,i.total_minor,i.base_total_minor,i.fx_rate_micros,i.version,
    CASE WHEN i.kind='receivable' THEN i.debit_account_id ELSE i.credit_account_id END control_account_id,
    i.total_minor-COALESCE((SELECT SUM(a.amount_minor) FROM finance_allocations a JOIN finance_payments p ON p.id=a.payment_id WHERE a.invoice_id=i.id AND p.status<>'voided' AND p.payment_date<=?),0) transaction_outstanding,
    (CASE WHEN i.base_total_minor=0 AND i.total_minor>0 AND i.fx_rate_micros=1000000 THEN i.total_minor ELSE i.base_total_minor END)
      -COALESCE((SELECT SUM(CASE WHEN a.base_invoice_minor=0 THEN a.amount_minor ELSE a.base_invoice_minor END) FROM finance_allocations a JOIN finance_payments p ON p.id=a.payment_id WHERE a.invoice_id=i.id AND p.status<>'voided' AND p.payment_date<=?),0)
      +COALESCE((SELECT SUM(l.adjustment_minor) FROM finance_fx_revaluation_lines l JOIN finance_fx_revaluation_runs r ON r.id=l.run_id JOIN finance_periods rp ON rp.id=r.period_id WHERE l.invoice_id=i.id AND r.status='posted' AND rp.end_date<?),0) prior_base
    FROM finance_invoices i JOIN finance_accounts a ON a.id=CASE WHEN i.kind='receivable' THEN i.debit_account_id ELSE i.credit_account_id END
    WHERE i.company_id=? AND a.chart_id=? AND i.currency<>? AND i.invoice_date<=? AND i.status IN ('posted','partially_paid','paid')
    ORDER BY i.currency,i.invoice_number,i.id`).bind(period.end_date,period.end_date,period.end_date,actor.companyId,chartId,chart.base_currency,period.end_date).all<FinanceRow>();
  const exposures:FxExposure[]=[];
  for(const row of rows.results){const outstanding=Number(row.transaction_outstanding);if(outstanding<=0)continue;const rate=fxMicros(String(row.currency),String(chart.base_currency),rates[String(row.currency)]),priorBase=Number(row.prior_base),revaluedBase=convertAtFx(outstanding,rate);exposures.push({invoiceId:String(row.id),kind:String(row.kind) as FxExposure["kind"],currency:String(row.currency),controlAccountId:String(row.control_account_id),transactionOutstanding:outstanding,priorBase,revaluedBase,adjustment:revaluedBase-priorBase,rateMicros:rate,version:Number(row.version)});}
  return {chart,exposures,sourceHash:await hash({chartId,periodId:period.id,periodEnd:period.end_date,exposures})};
}

export async function fxRevaluationWorkspace(actor:WorkContext,periodId:string){
  actor=requirePermission(actor,"finance.read");const {write}=await resources(actor,true),period=await write.prepare("SELECT * FROM finance_periods WHERE id=? AND company_id=?").bind(periodId,actor.companyId).first<FinanceRow>();
  if(!period)throw new ApiError(404,"PERIOD_NOT_FOUND","Accounting period not found.");
  const [currencies,accounts,runs]=await Promise.all([
    write.prepare(`SELECT DISTINCT a.chart_id,c.name chart_name,c.base_currency,i.currency FROM finance_invoices i JOIN finance_accounts a ON a.id=CASE WHEN i.kind='receivable' THEN i.debit_account_id ELSE i.credit_account_id END JOIN finance_charts c ON c.id=a.chart_id WHERE i.company_id=? AND i.currency<>c.base_currency AND i.invoice_date<=? AND i.status IN ('posted','partially_paid') AND i.outstanding_minor>0 ORDER BY c.name,i.currency`).bind(actor.companyId,period.end_date).all<FinanceRow>(),
    write.prepare("SELECT a.id,a.chart_id,a.code,a.name,a.category,a.currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.status='active' AND a.currency=c.base_currency AND a.category IN ('other_income','other_expense') ORDER BY a.code").bind(actor.companyId).all<FinanceRow>(),
    write.prepare("SELECT r.*,c.name chart_name FROM finance_fx_revaluation_runs r JOIN finance_charts c ON c.id=r.chart_id WHERE r.company_id=? AND r.period_id=? ORDER BY r.created_at DESC").bind(actor.companyId,periodId).all<FinanceRow>(),
  ]);
  return {period,currencies:currencies.results,accounts:accounts.results,runs:runs.results};
}

export async function prepareFxRevaluation(actor:WorkContext,input:z.infer<typeof prepareFxRevaluationSchema>,key:string){
  actor=requirePermission(actor,"finance.write");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_FINANCE_REQUIRED","Use a Company finance assignment to prepare revaluation.");input=prepareFxRevaluationSchema.parse(input);
  const {write}=await resources(actor),previous=await priorCommand(write,actor,key,"finance.fx-revaluation.prepare",input);if(previous)return previous;
  if(new Set(input.rates.map(row=>row.currency)).size!==input.rates.length)throw new ApiError(422,"DUPLICATE_FX_RATE","Provide one closing rate per currency.");
  const period=await write.prepare("SELECT * FROM finance_periods WHERE id=? AND company_id=? AND state='open'").bind(input.periodId,actor.companyId).first<FinanceRow>();if(!period)throw new ApiError(409,"PERIOD_NOT_OPEN","Revaluation requires an open period.");if(input.entryDate!==period.end_date)throw new ApiError(422,"REVALUATION_DATE_INVALID","Post revaluation on the period end date.");
  const rates=Object.fromEntries(input.rates.map(row=>[row.currency,row.rate])),calculated=await fxExposures(write,actor,input.chartId,period,rates);if(!calculated.exposures.length)throw new ApiError(422,"NO_FOREIGN_EXPOSURE","No open foreign monetary items require revaluation for this ledger and period.");
  const valid=await write.prepare("SELECT id,category FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND currency=? AND id IN (?,?)").bind(actor.companyId,input.chartId,calculated.chart.base_currency,input.gainAccountId,input.lossAccountId).all<FinanceRow>();
  if(valid.results.length!==2||valid.results.find(row=>row.id===input.gainAccountId)?.category!=="other_income"||valid.results.find(row=>row.id===input.lossAccountId)?.category!=="other_expense")throw new ApiError(422,"FX_ACCOUNT_INVALID","Select functional-currency other-income and other-expense accounts from this ledger.");
  const runId=`fx-revaluation-${crypto.randomUUID()}`,now=Date.now(),output={id:runId,status:"prepared",version:1,sourceHash:calculated.sourceHash,lines:calculated.exposures.length,netAdjustment:minorToDecimal(calculated.exposures.reduce((sum,row)=>sum+row.adjustment,0))};
  return saveCommand(write,actor,key,"finance.fx-revaluation.prepare",input,output,[
    assertion(write,key,"period","EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND end_date=?)",[input.periodId,actor.companyId,input.entryDate]),
    assertion(write,key,"posted-run","NOT EXISTS(SELECT 1 FROM finance_fx_revaluation_runs WHERE period_id=? AND chart_id=? AND status='posted')",[input.periodId,input.chartId]),
    write.prepare("INSERT INTO finance_fx_revaluation_runs(id,company_id,chart_id,period_id,entry_date,base_currency,rates_json,source_hash,gain_account_id,loss_account_id,preparation_proof,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(runId,actor.companyId,input.chartId,input.periodId,input.entryDate,calculated.chart.base_currency,JSON.stringify(rates),calculated.sourceHash,input.gainAccountId,input.lossAccountId,input.proof,actor.userId,now,now),
    ...calculated.exposures.map((row,index)=>write.prepare("INSERT INTO finance_fx_revaluation_lines(id,run_id,invoice_id,kind,currency,control_account_id,transaction_outstanding_minor,prior_base_minor,revalued_base_minor,adjustment_minor,rate_micros) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(`${runId}-${index}`,runId,row.invoiceId,row.kind,row.currency,row.controlAccountId,row.transactionOutstanding,row.priorBase,row.revaluedBase,row.adjustment,row.rateMicros)),
  ]);
}

export async function decideFxRevaluation(actor:WorkContext,runId:string,input:z.infer<typeof decideFxRevaluationSchema>,decision:"approve"|"reject",key:string){
  actor=requirePermission(actor,"finance.approve");requireFinanceRole(actor,true);if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_FINANCE_REQUIRED","Use a Company finance assignment to approve revaluation.");input=decideFxRevaluationSchema.parse(input);
  const {write}=await resources(actor),previous=await replayCommand(write,actor,key,`finance.fx-revaluation.${decision}`,runId,input);if(previous)return previous;
  const run=await write.prepare("SELECT r.*,p.state period_state,p.end_date FROM finance_fx_revaluation_runs r JOIN finance_periods p ON p.id=r.period_id WHERE r.id=? AND r.company_id=?").bind(runId,actor.companyId).first<FinanceRow>();if(!run)throw new ApiError(404,"FX_REVALUATION_NOT_FOUND","Revaluation run not found.");if(run.status!=="prepared"||Number(run.version)!==input.expectedVersion)throw new ApiError(409,"VERSION_CONFLICT","Revaluation changed. Refresh before deciding.");if(samePerson(actor,run.created_by))throw new ApiError(409,"SELF_REVIEW_FORBIDDEN","A different Finance approver must decide revaluation.");
  const now=Date.now();if(decision==="reject")return executeCommand(write,actor,key,"finance.fx-revaluation.reject",input,runId,2,{id:runId,status:"rejected",version:2},[assertion(write,key,"run","EXISTS(SELECT 1 FROM finance_fx_revaluation_runs WHERE id=? AND company_id=? AND status='prepared' AND version=1 AND created_by!=?)",[runId,actor.companyId,actor.userId]),write.prepare("UPDATE finance_fx_revaluation_runs SET status='rejected',decision_proof=?,approved_by=?,version=2,updated_at=? WHERE id=?").bind(input.proof,actor.userId,now,runId),write.prepare("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),key,runId,"finance.fx-revaluation.reject",actor.userId,actor.assignmentId,1,await hash(input),now)]);
  if(run.period_state!=="open")throw new ApiError(409,"PERIOD_NOT_OPEN","The revaluation period is no longer open.");const rates=JSON.parse(String(run.rates_json)) as Record<string,string>,period=await write.prepare("SELECT * FROM finance_periods WHERE id=?").bind(run.period_id).first<FinanceRow>(),calculated=await fxExposures(write,actor,String(run.chart_id),period!,rates);if(calculated.sourceHash!==run.source_hash)throw new ApiError(409,"FX_REVALUATION_STALE","Foreign balances changed after preparation. Reject this run and prepare it again.");
  const adjustmentLines:JournalLineInput[]=[],gain=calculated.exposures.filter(row=>row.kind==="receivable"?row.adjustment>0:row.adjustment<0).reduce((sum,row)=>sum+Math.abs(row.adjustment),0),loss=calculated.exposures.filter(row=>row.kind==="receivable"?row.adjustment<0:row.adjustment>0).reduce((sum,row)=>sum+Math.abs(row.adjustment),0);
  for(const row of calculated.exposures){if(!row.adjustment)continue;const debit=row.kind==="receivable"?row.adjustment>0:row.adjustment<0;adjustmentLines.push({accountId:row.controlAccountId,debit:debit?minorToDecimal(Math.abs(row.adjustment)):"0",credit:debit?"0":minorToDecimal(Math.abs(row.adjustment)),description:`Period-end FX revaluation ${row.invoiceId}`});}
  if(gain)adjustmentLines.push({accountId:String(run.gain_account_id),debit:"0",credit:minorToDecimal(gain),description:"Unrealized foreign exchange gain"});if(loss)adjustmentLines.push({accountId:String(run.loss_account_id),debit:minorToDecimal(loss),credit:"0",description:"Unrealized foreign exchange loss"});
  const journal=adjustmentLines.length?await prepareJournal(write,actor,{chartId:String(run.chart_id),periodId:String(run.period_id),entryDate:String(run.entry_date),currency:String(run.base_currency),memo:`Period-end FX revaluation ${run.entry_date}`,sourceType:"fx_revaluation",sourceId:runId,isOpening:false,lines:adjustmentLines},`${key}-journal`,"posted"):null;
  return executeCommand(write,actor,key,"finance.fx-revaluation.approve",input,runId,2,{id:runId,status:"posted",version:2,journalId:journal?.result.id??null,gain:minorToDecimal(gain),loss:minorToDecimal(loss)},[
    assertion(write,key,"run","EXISTS(SELECT 1 FROM finance_fx_revaluation_runs r JOIN finance_periods p ON p.id=r.period_id WHERE r.id=? AND r.company_id=? AND r.status='prepared' AND r.version=1 AND r.created_by!=? AND p.state='open')",[runId,actor.companyId,actor.userId]),
    assertion(write,key,"posted-run","NOT EXISTS(SELECT 1 FROM finance_fx_revaluation_runs WHERE period_id=? AND chart_id=? AND status='posted')",[run.period_id,run.chart_id]),
    ...calculated.exposures.map((row,index)=>assertion(write,key,`invoice-${index}`,"EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND version=? AND outstanding_minor=?)",[row.invoiceId,actor.companyId,row.version,row.transactionOutstanding])),
    ...(journal?.steps??[]),
    ...calculated.exposures.map(row=>write.prepare("UPDATE finance_invoices SET base_outstanding_minor=base_outstanding_minor+?,version=version+1,updated_at=? WHERE id=?").bind(row.adjustment,now,row.invoiceId)),
    write.prepare("UPDATE finance_fx_revaluation_runs SET status='posted',journal_id=?,decision_proof=?,approved_by=?,version=2,updated_at=? WHERE id=?").bind(journal?.result.id??null,input.proof,actor.userId,now,runId),
    write.prepare("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),key,runId,"finance.fx-revaluation.approve",actor.userId,actor.assignmentId,1,await hash(input),now),
  ]);
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
  const openManufacturing=await write.prepare("SELECT COUNT(*) count FROM manufacturing_orders WHERE company_id=? AND status IN ('reserved','in_progress','awaiting_qc')").bind(actor.companyId).first<{count:number}>();
  const calculationPending=await write.prepare("SELECT COUNT(*) count FROM finance_calculation_runs WHERE company_id=? AND period_id=? AND status IN ('prepared','reviewed','stale')").bind(actor.companyId,periodId).first<{count:number}>();
  const pendingTax=Number(await write.prepare(pendingVatReviewSql).bind(period.end_date,actor.companyId).first("value")??0);
  if(pendingTax)throw new ApiError(409,"CLOSE_NOT_READY",`Review ${pendingTax} source VAT documents in the statutory ledger before beginning close.`);
  const billsPending=await write.prepare("SELECT COUNT(*) n FROM procurement_bill_matches WHERE company_id=? AND status IN ('review','approved','invoiced') AND invoice_date<=?").bind(actor.companyId,period.end_date).first('n');
  if(Number(billsPending))throw new ApiError(409,'CLOSE_NOT_READY','Resolve supplier invoice matching and AP posting before beginning close.');
  const supplierPending=await write.prepare("SELECT COUNT(*) AS count FROM finance_supplier_instructions i JOIN finance_supplier_batches b ON b.id=i.batch_id WHERE b.company_id=? AND i.status IN ('exported','unknown') AND b.planned_date<=?").bind(actor.companyId,period.end_date).first<{count:number}>();
  if(Number(supplierPending?.count))throw new ApiError(409,"CLOSE_NOT_READY","Resolve exported supplier bank instructions before beginning close.");
  const refunds=await write.prepare("SELECT (SELECT COUNT(*) FROM commerce_returns WHERE company_id=? AND status='approved' AND return_date<=?) returns,(SELECT COUNT(*) FROM finance_refund_instructions WHERE company_id=? AND status IN ('exported','unknown') AND planned_date<=?) instructions").bind(actor.companyId,period.end_date,actor.companyId,period.end_date).first<{returns:number;instructions:number}>();
  if(Number(refunds?.returns)||Number(refunds?.instructions))throw new ApiError(409,"CLOSE_NOT_READY",`Complete ${refunds?.returns??0} return adjustments and resolve ${refunds?.instructions??0} exported refund instructions before beginning close.`);
  const readiness = { draftJournals: draftJournals?.count ?? 0, draftInvoices: draftInvoices?.count ?? 0, unmatchedBank: unmatchedBank?.count ?? 0, openManufacturing:openManufacturing?.count??0, pendingCalculations:calculationPending?.count??0 };
  if (Object.values(readiness).some((value) => value > 0)) throw new ApiError(409, "CLOSE_NOT_READY", `Resolve ${readiness.draftJournals} journals, ${readiness.draftInvoices} invoices, ${readiness.unmatchedBank} bank rows, ${readiness.openManufacturing} manufacturing orders, and ${readiness.pendingCalculations} calculation runs before beginning close.`);
  const reconciliation=await calculateCloseReconciliations(write,actor,period);
  if(reconciliation.vat&&!reconciliation.vat.ready)throw new ApiError(409,"CLOSE_VAT_RECONCILIATION_FAILED","Reconcile the reviewed statutory VAT register with every originating VAT account before beginning close.");
  if(!reconciliation.ready){const failed=reconciliation.rows.filter(row=>row.differenceMinor!==0||Number(row.source.foreignOpenCount??0)>0||Number(row.source.legacyDateCount??0)>0||(row.kind==="bank"&&row.source.evidenceComplete!==true));throw new ApiError(409,"CLOSE_RECONCILIATION_FAILED",`Reconcile, retain dated source journals or revalue ${failed.map(row=>`${row.accountCode} ${row.accountName}`).join(", ")} before beginning close.`);}
  const nextVersion=Number(period.close_version)+1,checkSource=await hash({periodId,closeVersion:period.close_version,readiness,reconciliation:reconciliation.reconciliationHash}),checkedAt=Date.now();
  return executeCommand(write,actor,id,"finance.period.begin-close",{periodId},periodId,nextVersion,{id:periodId,state:"closing",version:nextVersion,reconciliationHash:reconciliation.reconciliationHash,reconciliations:reconciliation.rows},[
    assertion(write,id,"period","EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND close_version=?)",[periodId,actor.companyId,period.close_version]),
    assertion(write,id,"journals","NOT EXISTS(SELECT 1 FROM finance_journals WHERE company_id=? AND period_id=? AND status IN ('draft','in_review'))",[actor.companyId,periodId]),
    assertion(write,id,"invoices","NOT EXISTS(SELECT 1 FROM finance_invoices WHERE company_id=? AND status IN ('draft','in_review') AND invoice_date BETWEEN ? AND ?)",[actor.companyId,period.start_date,period.end_date]),
    assertion(write,id,"bank","NOT EXISTS(SELECT 1 FROM finance_bank_transactions WHERE company_id=? AND match_status IN ('unmatched','proposed') AND transaction_date BETWEEN ? AND ?)",[actor.companyId,period.start_date,period.end_date]),
    assertion(write,id,"manufacturing","NOT EXISTS(SELECT 1 FROM manufacturing_orders WHERE company_id=? AND status IN ('reserved','in_progress','awaiting_qc'))",[actor.companyId]),
    assertion(write,id,"calculations","NOT EXISTS(SELECT 1 FROM finance_calculation_runs WHERE company_id=? AND period_id=? AND status IN ('prepared','reviewed','stale'))",[actor.companyId,periodId]),
    assertion(write,id,"vat-review",`(${pendingVatReviewSql})=0`,[period.end_date,actor.companyId]),
    ...refundCloseGuards(write,actor,period,id),
    ...reconciliationAssertions(write,actor,period,id,reconciliation.rows),
    ...vatCloseGuards(write,actor,id,String(period.end_date),reconciliation.vat),
    ...Object.entries(readiness).map(([check,count])=>write.prepare("INSERT INTO finance_close_checks(id,company_id,period_id,check_key,source_version,status,count_value,detail_json,checked_at,checked_by) VALUES(?,?,?,?,?,'pass',?,?,?,?) ON CONFLICT(period_id,check_key,source_version) DO NOTHING").bind(crypto.randomUUID(),actor.companyId,periodId,check,checkSource,count,JSON.stringify({count}),checkedAt,actor.userId)),
    ...(reconciliation.vat?[write.prepare("INSERT INTO finance_close_checks(id,company_id,period_id,check_key,source_version,status,count_value,detail_json,checked_at,checked_by) VALUES(?,?,?,?,?,'pass',0,?,?,?) ON CONFLICT(period_id,check_key,source_version) DO NOTHING").bind(crypto.randomUUID(),actor.companyId,periodId,"statutory-vat",checkSource,JSON.stringify(reconciliation.vat),checkedAt,actor.userId)]:[]),
    ...reconciliation.rows.map(row=>write.prepare("INSERT INTO finance_close_reconciliations(id,company_id,period_id,close_version,kind,account_id,currency,ledger_minor,source_minor,difference_minor,source_json,checked_at,checked_by) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),actor.companyId,periodId,nextVersion,row.kind,row.accountId,row.currency,row.ledgerMinor,row.sourceMinor,row.differenceMinor,JSON.stringify(row.source),checkedAt,actor.userId)),
    write.prepare("UPDATE finance_periods SET state='closing',close_started_by=?,close_version=close_version+1,reconciliation_hash=? WHERE id=?").bind(actor.userId,reconciliation.reconciliationHash,periodId),
  ]);
}

export const cancelCloseSchema=z.object({expectedVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(2000)});
export async function cancelClose(actor:WorkContext,periodId:string,input:z.infer<typeof cancelCloseSchema>,id:string){
  actor=requirePermission(actor,"finance.write");
  const {write}=await resources(actor),previous=await replayCommand(write,actor,id,"finance.period.cancel-close",periodId,input);if(previous)return previous;
  const period=await write.prepare("SELECT * FROM finance_periods WHERE id=? AND company_id=?").bind(periodId,actor.companyId).first<FinanceRow>();
  if(!period)throw new ApiError(404,"PERIOD_NOT_FOUND","Accounting period not found.");
  if(period.state!=="closing")throw new ApiError(409,"PERIOD_NOT_CLOSING","Only a prepared close can be cancelled.");
  if(period.close_started_by!==actor.userId&&!hasPermission(actor,"finance.approve"))throw new ApiError(403,"CLOSE_CANCELLATION_FORBIDDEN","The preparer or a Finance approver must cancel the preparation.");
  if(Number(period.close_version)!==input.expectedVersion)throw new ApiError(409,"VERSION_CONFLICT","The close changed. Refresh and review again.");
  return executeCommand(write,actor,id,"finance.period.cancel-close",input,periodId,input.expectedVersion+1,{id:periodId,state:"open",version:input.expectedVersion+1},[
    assertion(write,id,"period","EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='closing' AND close_version=?)",[periodId,actor.companyId,input.expectedVersion]),
    write.prepare("UPDATE finance_periods SET state='open',close_started_by=NULL,reconciliation_hash=NULL,close_version=close_version+1 WHERE id=? AND company_id=?").bind(periodId,actor.companyId),
    write.prepare("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),id,periodId,"finance.period.cancel-close",actor.userId,actor.assignmentId,input.expectedVersion,await hash(input),Date.now()),
  ]);
}

export async function closePeriod(actor: WorkContext, periodId: string, input: z.infer<typeof closeProofSchema>, id: string) {
  input = { ...input, attachmentSetIds: input.attachmentSetIds ?? [] };
  actor = requirePermission(actor,"finance.write");

  requireFinanceRole(actor, true); const { write } = await resources(actor); const previous = await replayCommand(write,actor,id,"finance.period.close",periodId,input); if (previous) return previous;
  const period = await write.prepare("SELECT * FROM finance_periods WHERE id=? AND company_id=?").bind(periodId, actor.companyId).first<FinanceRow>(); if (!period) throw new ApiError(404, "PERIOD_NOT_FOUND", "Accounting period not found."); if (period.state !== "closing") throw new ApiError(409, "INVALID_PERIOD_TRANSITION", "Begin the close before approving it."); if (String(period.close_started_by) === actor.userId) throw new ApiError(409, "CLOSE_APPROVAL_SEPARATION", "A different person must approve the period close."); if (input.expectedVersion !== undefined && input.expectedVersion !== Number(period.close_version)) throw new ApiError(409, "VERSION_CONFLICT", "The close changed. Refresh and review again.");
  const reconciliation=await calculateCloseReconciliations(write,actor,period);if(!reconciliation.ready||reconciliation.reconciliationHash!==period.reconciliation_hash)throw new ApiError(409,"CLOSE_RECONCILIATION_STALE","Ledger, subledger, or bank evidence changed after preparation. Reopen the period and prepare the close again.");
  const now=Date.now(),version=Number(period.close_version),attachmentBinding=input.attachmentSetIds.length?await prepareAttachmentBinding(write,actor,{attachmentSetIds:input.attachmentSetIds,bindingContract:{targetType:"finance_period",bindingType:"finance_period_close",contexts:[{purposeKey:"close_workpaper",componentKind:"action",componentRef:"close-approval"}]},targetId:periodId,expectedTargetVersion:version,commandId:id,bindingId:`${periodId}:${version+1}`}):null;return executeCommand(write,actor,id,"finance.period.close",input,periodId,version+1,{id:periodId,state:"closed",version:version+1},[
    ...(attachmentBinding?.guards??[]),
    assertion(write,id,"period","EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='closing' AND close_version=? AND close_started_by!=?)",[periodId,actor.companyId,version,actor.userId]),
    ...refundCloseGuards(write,actor,period,id),
    assertion(write,id,"close-journals","NOT EXISTS(SELECT 1 FROM finance_journals WHERE company_id=? AND period_id=? AND status IN ('draft','in_review'))",[actor.companyId,periodId]),
    assertion(write,id,"close-invoices","NOT EXISTS(SELECT 1 FROM finance_invoices WHERE company_id=? AND status IN ('draft','in_review') AND invoice_date BETWEEN ? AND ?)",[actor.companyId,period.start_date,period.end_date]),
    assertion(write,id,"close-bank","NOT EXISTS(SELECT 1 FROM finance_bank_transactions WHERE company_id=? AND match_status IN ('unmatched','proposed') AND transaction_date BETWEEN ? AND ?)",[actor.companyId,period.start_date,period.end_date]),
    assertion(write,id,"close-manufacturing","NOT EXISTS(SELECT 1 FROM manufacturing_orders WHERE company_id=? AND status IN ('reserved','in_progress','awaiting_qc'))",[actor.companyId]),
    assertion(write,id,"close-calculations","NOT EXISTS(SELECT 1 FROM finance_calculation_runs WHERE company_id=? AND period_id=? AND status IN ('prepared','reviewed','stale'))",[actor.companyId,periodId]),
    assertion(write,id,"close-vat-review",`(${pendingVatReviewSql})=0`,[period.end_date,actor.companyId]),
    ...reconciliationAssertions(write,actor,period,id,reconciliation.rows),
    ...vatCloseGuards(write,actor,id,String(period.end_date),reconciliation.vat),
    write.prepare("UPDATE finance_periods SET state='closed',closed_by=?,closed_at=?,close_version=close_version+1 WHERE id=?").bind(actor.userId,now,periodId),
    ...(attachmentBinding?.steps??[]),
    write.prepare("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),id,periodId,"finance.period.close",actor.userId,actor.assignmentId,version,await hash(input),now),
  ]);
}

export async function reopenPeriod(actor: WorkContext, periodId: string, input: z.infer<typeof closeProofSchema>, id: string) {
  input = { ...input, attachmentSetIds: input.attachmentSetIds ?? [] };
  actor = requirePermission(actor,"finance.write");

  requireFinanceRole(actor,true);const {write}=await resources(actor);const previous=await replayCommand(write,actor,id,"finance.period.reopen",periodId,input);if(previous)return previous;
  if(input.attachmentSetIds.length)throw new ApiError(422,"ATTACHMENT_ACTION_UNSUPPORTED","Close workpapers bind only to the close approval, not to reopening a period.");
  const period=await write.prepare("SELECT * FROM finance_periods WHERE id=? AND company_id=? AND state='closed'").bind(periodId,actor.companyId).first<FinanceRow>();if(!period)throw new ApiError(404,"PERIOD_NOT_CLOSED","Closed period not found.");
  if(period.closed_by===actor.userId)throw new ApiError(409,"CLOSE_APPROVAL_SEPARATION","A different approver must reopen this period.");
  const version=input.expectedVersion??Number(period.close_version);return executeCommand(write,actor,id,"finance.period.reopen",input,periodId,version+1,{id:periodId,state:"open",version:version+1},[
    assertion(write,id,"period","EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND state='closed' AND close_version=?)",[periodId,version]),
    write.prepare("UPDATE finance_periods SET state='open',closed_by=NULL,closed_at=NULL,reconciliation_hash=NULL,close_version=close_version+1 WHERE id=?").bind(periodId),
    write.prepare("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),id,periodId,"finance.period.reopen",actor.userId,actor.assignmentId,version,await hash(input),Date.now()),
  ]);
}

export const partySchema = z.object({ kind: z.enum(["customer", "supplier", "employee"]), code: z.string().min(1).max(64), name: z.string().min(1).max(200), currency: z.string().length(3).transform((v) => v.toUpperCase()), taxRegistration: z.string().max(100).optional(), paymentTermsDays: z.number().int().min(0).max(365).default(30), creditLimit: moneySchema.default("0") });
export async function createParty(actor: WorkContext, input: z.infer<typeof partySchema>, id: string) {
  actor = requirePermission(actor,"finance.write");
 const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.party.create", input); if (previous) return previous; await requireFinanceMoneyScale(write,actor.companyId,input.currency); const partyId = `party-${crypto.randomUUID()}`; const now = Date.now(); pendingStatements.push(write.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,tax_registration,payment_terms_days,credit_limit_minor,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(partyId, actor.companyId, input.kind, input.code, input.name, input.currency, input.taxRegistration ?? null, input.paymentTermsDays, decimalToMinor(input.creditLimit), now, now)); const output = { id: partyId, ...input, creditLimitMinor: decimalToMinor(input.creditLimit) }; return saveCommand(write, actor, id, "finance.party.create", input, output, pendingStatements); }

export const invoiceSchema = z.object({ expectedVersion:z.number().int().positive().optional(), aiProposalId:z.string().max(128).optional(), aiProposalVersion:z.number().int().positive().optional(), aiReason:z.string().trim().min(10).max(2000).optional(), partyId: z.string().min(1), kind: z.enum(["payable", "receivable"]), invoiceNumber: z.string().min(1).max(100), invoiceDate: z.string().date(), dueDate: z.string().date(), currency: z.string().length(3).transform((v) => v.toUpperCase()), subtotal: moneySchema, tax: moneySchema.default("0"), taxCode: z.string().max(64).optional(), taxAccountId:z.string().min(1).optional(), sourceType: z.string().max(64).optional(), sourceId: z.string().max(128).optional(), debitAccountId: z.string().min(1), creditAccountId: z.string().min(1), lines:z.array(invoiceLineSchema).min(1).max(100).optional() });
export async function createInvoice(actor:WorkContext,input:z.infer<typeof invoiceSchema>,id:string,existingId?:string) {
  actor = requirePermission(actor,"finance.write");
  input=invoiceSchema.parse(input);

  const {write}=await resources(actor),invoiceId=existingId??`invoice-${id}`;const operation=existingId?"finance.invoice.update":"finance.invoice.create";const previous=await replayCommand(write,actor,id,operation,invoiceId,input);if(previous)return previous;
  await requireFinanceMoneyScale(write,actor.companyId,input.currency);
  const subtotal=decimalToMinor(input.subtotal),tax=decimalToMinor(input.tax),total=subtotal+tax;
  if(subtotal<=0||tax<0||!Number.isSafeInteger(total))throw new ApiError(422,"INVALID_INVOICE_AMOUNT","Provide a positive subtotal and nonnegative tax within supported precision.");
  if(input.dueDate<input.invoiceDate)throw new ApiError(422,"INVALID_DUE_DATE","Due date cannot precede the invoice date.");
  if(tax&&!input.taxAccountId)throw new ApiError(422,"TAX_ACCOUNT_REQUIRED","Map a separate input or output tax account before recording tax.");
  const accountIds=[input.debitAccountId,input.creditAccountId,...(tax?[input.taxAccountId!]:[])];if(new Set(accountIds).size!==accountIds.length)throw new ApiError(422,"ACCOUNT_INVALID","Use distinct principal and tax posting accounts.");
  const accounts=await write.prepare(`SELECT id,chart_id FROM finance_accounts WHERE company_id=? AND status='active' AND currency=? AND id IN (${accountIds.map(()=>"?").join(",")})`).bind(actor.companyId,input.currency,...accountIds).all<{id:string;chart_id:string}>();
  if(accounts.results.length!==accountIds.length||new Set(accounts.results.map(a=>a.chart_id)).size!==1)throw new ApiError(422,"ACCOUNT_INVALID","Invoice accounts must use one active chart and currency.");
  if(input.aiProposalId&&!existingId)throw new ApiError(422,"AI_DRAFT_REQUIRED","AI coding applies only to an existing standalone draft.");
  const aiSteps=input.aiProposalId?await (await import("./finance-ai")).financeAiDraftApplication(write,actor,await (await import("./runtime")).runtimeEnv(),input.aiProposalId,invoiceId,input,id):[];
  if(existingId){
    const existing=await write.prepare("SELECT line_mode FROM finance_invoices WHERE id=? AND company_id=? AND status='draft' AND version=? AND source_type='standalone'").bind(existingId,actor.companyId,input.expectedVersion??0).first<{line_mode:string}>();
    if(!existing)throw new ApiError(409,"VERSION_CONFLICT","Refresh the standalone draft before editing it.");
    if(existing.line_mode==="native_lines"&&!input.lines)throw new ApiError(422,"INVOICE_LINES_REQUIRED","Send the complete line revision when editing a multi-line invoice.");
  }
  const revision=existingId?(input.expectedVersion??0)+1:1;
  const lineRevision=await prepareInvoiceLineRevision(write,actor,input,invoiceId,revision,{subtotal,tax},id);
  const now=Date.now();return executeCommand(write,actor,id,operation,input,invoiceId,existingId?(input.expectedVersion??0)+1:1,{id:invoiceId,total:minorToDecimal(total),outstanding:minorToDecimal(total),status:"draft",version:existingId?(input.expectedVersion??0)+1:1},[
    ...aiSteps,
    ...lineRevision.guards,
    ...(existingId?[assertion(write,id,"invoice-version","EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND status='draft' AND version=? AND source_type='standalone')",[existingId,actor.companyId,input.expectedVersion??0])]:[]),
    assertion(write,id,"party","EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind=? AND currency=? AND status='active' AND person_id IS NULL)",[input.partyId,actor.companyId,input.kind==="payable"?"supplier":"customer",input.currency]),
    existingId?write.prepare("UPDATE finance_invoices SET party_id=?,kind=?,invoice_number=?,invoice_date=?,due_date=?,currency=?,subtotal_minor=?,tax_minor=?,total_minor=?,outstanding_minor=?,tax_code=?,debit_account_id=?,credit_account_id=?,tax_account_id=?,created_by=?,version=version+1,updated_at=? WHERE id=?").bind(input.partyId,input.kind,input.invoiceNumber,input.invoiceDate,input.dueDate,input.currency,subtotal,tax,total,total,input.taxCode??null,input.debitAccountId,input.creditAccountId,input.taxAccountId??null,actor.userId,now,existingId):write.prepare("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,tax_code,debit_account_id,credit_account_id,tax_account_id,source_type,source_id,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(invoiceId,actor.companyId,input.partyId,input.kind,input.invoiceNumber,input.invoiceDate,input.dueDate,input.currency,subtotal,tax,total,total,input.taxCode??null,input.debitAccountId,input.creditAccountId,input.taxAccountId??null,"standalone",null,actor.userId,now,now),
    write.prepare("UPDATE finance_invoices SET line_mode=?,line_revision=? WHERE id=? AND company_id=?").bind(lineRevision.mode,revision,invoiceId,actor.companyId),
    ...lineRevision.steps,
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

export const invoicePostingSchema=z.object({
  periodId:z.string().min(1),entryDate:z.iso.date(),expectedVersion:z.number().int().positive().optional(),
  fxRate:moneySchema.optional(),attachmentSetIds:z.array(z.string().min(1)).max(20).optional(),
  fxGainAccountId:z.string().min(1).optional(),fxLossAccountId:z.string().min(1).optional(),
});
export async function postInvoice(actor:WorkContext,invoiceId:string,input:z.infer<typeof invoicePostingSchema>,id:string) {
  actor = requirePermission(actor,"finance.write");
  input=invoicePostingSchema.parse(input);

  requireFinanceRole(actor,true);
  const {write}=await resources(actor);const previous=await replayCommand(write,actor,id,"finance.invoice.post",invoiceId,input);if(previous)return previous;
  const invoice=await write.prepare("SELECT * FROM finance_invoices WHERE id=? AND company_id=? AND status='draft' AND source_type!='opening_import'").bind(invoiceId,actor.companyId).first<FinanceRow>();if(!invoice)throw new ApiError(404,"INVOICE_NOT_FOUND","A postable draft invoice was not found.");
  if(samePerson(actor,invoice.created_by)||!hasPermission(actor,"finance.approve"))throw new ApiError(403,"INDEPENDENT_APPROVER_REQUIRED","A different finance approver must post a standalone invoice.");
  if(input.expectedVersion!==undefined && input.expectedVersion!==Number(invoice.version))throw new ApiError(409,"VERSION_CONFLICT","The invoice changed. Refresh its Debit / Credit before posting.");
  const tax=Number(invoice.tax_minor),subtotal=Number(invoice.subtotal_minor),total=Number(invoice.total_minor);if(tax&&!invoice.tax_account_id)throw new ApiError(422,"TAX_ACCOUNT_REQUIRED","This invoice needs a separate tax posting account.");
  const chart=await write.prepare("SELECT a.chart_id,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.id=? AND a.company_id=?").bind(invoice.debit_account_id,actor.companyId).first<{chart_id:string;base_currency:string}>();if(!chart)throw new ApiError(422,"ACCOUNT_INVALID","Invoice account is unavailable.");
  const payable=invoice.kind==="payable";
  const attachmentSetIds=input.attachmentSetIds??[];
  if(attachmentSetIds.length&&!payable)throw new ApiError(422,"SUPPLIER_BILL_REQUIRED","Supplier bill review evidence belongs to a payable invoice.");
  const attachmentBinding=attachmentSetIds.length?await prepareAttachmentBinding(write,actor,{attachmentSetIds,bindingContract:{targetType:"supplier_bill",bindingType:"supplier_bill_posting",contexts:[{purposeKey:"bill_match_support",componentKind:"action",componentRef:"invoice-review"}]},targetId:invoiceId,expectedTargetVersion:Number(invoice.version),commandId:id,bindingId:`${invoiceId}:${Number(invoice.version)+1}`}):null;
  const lines:JournalLineInput[]=[{accountId:String(invoice.debit_account_id),debit:minorToDecimal(payable?subtotal:total),credit:"0"},{accountId:String(invoice.credit_account_id),debit:"0",credit:minorToDecimal(payable?total:subtotal)}];
  if(tax)lines.push({accountId:String(invoice.tax_account_id),debit:payable?minorToDecimal(tax):"0",credit:payable?"0":minorToDecimal(tax)});
  const native=await nativeInvoicePosting(write,actor,invoice,id);
  const matched=await (await import("./supplier-bill-matching")).matchedInvoicePosting(write,actor,invoice,id);
  const journal=await prepareJournal(write,actor,{chartId:chart.chart_id,periodId:input.periodId,entryDate:input.entryDate,currency:String(invoice.currency),fxRate:input.fxRate,memo:`Invoice ${invoice.invoice_number}`,sourceType:"invoice",sourceId:invoiceId,isOpening:false,lines:matched?.lines??native?.lines??lines},id,"posted");
  const goods=await (await import("./commerce-goods-accounting")).prepareCommerceInvoiceGoods(write,actor,{invoice,journal,key:id,periodId:input.periodId,entryDate:input.entryDate,gainAccountId:input.fxGainAccountId,lossAccountId:input.fxLossAccountId});
  const baseTotal=convertAtFx(total,journal.result.fxRateMicros);
  const draftSnapshot=await write.prepare("SELECT 1 FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_invoice' AND source_id=? AND amount_kind=?").bind(actor.companyId,invoiceId,`draft:v${invoice.line_revision}`).first();
  const snapshot=draftSnapshot?(await snapshotMoneyCarry(write,actor,{sourceType:"finance_invoice",sourceId:invoiceId,amountKind:"total",transactionDate:String(invoice.invoice_date),amountUnits:String(total),original:{sourceType:"finance_invoice",sourceId:invoiceId,amountKind:`draft:v${invoice.line_revision}`}})).statement:await legacyMoneySnapshot(write,actor,{sourceType:"finance_invoice",sourceId:invoiceId,amountKind:"total",transactionDate:String(invoice.invoice_date),currency:String(invoice.currency),amountUnits:total});
  return executeCommand(write,actor,id,"finance.invoice.post",input,invoiceId,Number(invoice.version)+1,{id:invoiceId,status:"posted",journalId:journal.result.id},[
    assertion(write,id,"invoice","EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND status='draft' AND version=?)",[invoiceId,actor.companyId,invoice.version]),...(attachmentBinding?.guards??[]),...(native?.guards??[]),...journal.steps,
    write.prepare("UPDATE finance_invoices SET status='posted',journal_id=?,fx_rate_micros=?,base_total_minor=?,base_outstanding_minor=?,approved_by=?,posted_at=?,version=version+1,updated_at=? WHERE id=?").bind(journal.result.id,journal.result.fxRateMicros,baseTotal,baseTotal,actor.userId,Date.now(),Date.now(),invoiceId),
    ...(snapshot?[snapshot]:[]),
    ...(goods?.steps??[]),...(matched?.steps??[]),
    ...(attachmentBinding?.steps??[]),
  ]);
}

export async function aging(actor: WorkContext, kind: "payable" | "receivable", asOf: string, chartId?: string) {
  actor = requirePermission(actor,"finance.read");
  if(!z.string().date().safeParse(asOf).success)throw new ApiError(422,"INVALID_AS_OF","Use a valid accounting cutoff date.");
  const {write}=await resources(actor,true);
  const rows=await write.prepare(`${invoiceHistoryCte}
    SELECT i.*,p.code party_code,p.name party_name FROM historical_invoice_balances i
    JOIN finance_parties p ON p.id=i.party_id AND p.company_id=i.company_id
    WHERE i.kind=? AND i.historical_outstanding_minor>0 AND (?::text IS NULL OR i.chart_id=?) ORDER BY i.due_date,i.id`)
    .bind(actor.companyId,asOf,kind,chartId??null,chartId??null).all<FinanceRow & {legacy_date_evidence:boolean}>();
  return rows.results.map(row=>{
    const days=Math.max(0,Math.floor((Date.parse(asOf)-Date.parse(String(row.due_date)))/86400000));
    const bucket=days===0?"current":days<=30?"1-30":days<=60?"31-60":days<=90?"61-90":"90+";
    return {...row,outstanding_minor:Number(row.historical_outstanding_minor),base_outstanding_minor:Number(row.historical_base_outstanding_minor),
      outstanding:minorToDecimal(Number(row.historical_outstanding_minor)),baseOutstanding:minorToDecimal(Number(row.historical_base_outstanding_minor)),bucket,asOf};
  });
}

export async function createPayment(actor: WorkContext, input: { partyId?: string; kind: "payment" | "receipt" | "refund"; paymentNumber: string; paymentDate: string; currency: string; amount: string; bankAccount?: string }, id: string) {
  actor = requirePermission(actor,"finance.write");
 if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_FINANCE_REQUIRED","Use a Company finance assignment to prepare a controlled payment.");
 if(input.kind==="refund")throw new ApiError(422,"REFUND_OWNER_WORKFLOW_REQUIRED","Prepare refunds from the approved customer return or refund obligation.");
 if(!input.partyId||!input.bankAccount)throw new ApiError(422,"PAYMENT_SETUP_REQUIRED","Select the customer or supplier and bank ledger account.");
 const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.payment.create", input); if (previous) return previous; const paymentId = `pay-${crypto.randomUUID()}`; const amount = decimalToMinor(input.amount); if (amount <= 0) throw new ApiError(422, "INVALID_PAYMENT_AMOUNT", "Payment amount must be greater than zero."); const currency=input.currency.toUpperCase(),now = Date.now();
 await requireFinanceMoneyScale(write,actor.companyId,currency);
 const requestedSnapshot=await legacyMoneySnapshot(write,actor,{sourceType:"finance_payment",sourceId:paymentId,amountKind:"requested",transactionDate:input.paymentDate,currency,amountUnits:amount});
 pendingStatements.push(
  requestedSnapshot,
  assertion(write,id,"party","EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND kind=? AND currency=? AND status='active' AND person_id IS NULL)",[input.partyId,actor.companyId,input.kind==="payment"?"supplier":"customer",currency]),
  assertion(write,id,"bank-account","EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND category='asset' AND control_kind='bank' AND currency=? AND status='active')",[input.bankAccount,actor.companyId,currency]),
  write.prepare("INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(paymentId, actor.companyId, input.partyId, input.kind, input.paymentNumber, input.paymentDate, currency, amount, amount, input.bankAccount, actor.userId, now, now),
 ); const output = { id: paymentId, amount: minorToDecimal(amount), outstanding: minorToDecimal(amount), status: "draft",version:1 }; return saveCommand(write, actor, id, "finance.payment.create", input, output, pendingStatements); }

export async function approvePayment(actor: WorkContext, paymentId: string, input: { expectedVersion: number; proof: string }, id: string) {
  actor = requirePermission(actor,"finance.write");
 requireFinanceRole(actor, true); if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_FINANCE_REQUIRED","Use a Company finance assignment to approve a controlled payment."); const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.payment.approve", input); if (previous) return previous; if (!input.proof.trim()) throw new ApiError(422, "HUMAN_DECISION_REQUIRED", "Payment approval evidence is required."); const payment = await write.prepare("SELECT * FROM finance_payments WHERE id=? AND company_id=?").bind(paymentId, actor.companyId).first<FinanceRow>(); if (!payment) throw new ApiError(404, "PAYMENT_NOT_FOUND", "Payment not found."); if (Number(payment.version) !== input.expectedVersion) throw new ApiError(409, "VERSION_CONFLICT", "Payment changed. Refresh before approving."); if (samePerson(actor,payment.created_by)) throw new ApiError(409, "SELF_REVIEW_FORBIDDEN", "A different person must approve the payment."); if (payment.status !== "draft" && payment.status !== "in_review") throw new ApiError(409, "PAYMENT_NOT_APPROVABLE", "Payment is not awaiting approval."); const withholding=await readyPaymentWithholding(write,actor,payment,id);pendingStatements.push(...(withholding?.guards??[])); const now=Date.now();pendingStatements.push(assertion(write,id,"payment-version","EXISTS(SELECT 1 FROM finance_payments WHERE id=? AND company_id=? AND version=? AND status IN ('draft','in_review') AND created_by!=?)",[paymentId,actor.companyId,input.expectedVersion,actor.userId])); pendingStatements.push(write.prepare("UPDATE finance_payments SET status='approved',approved_by=?,version=version+1,updated_at=? WHERE id=? AND version=? AND company_id=?").bind(actor.userId, now, paymentId, input.expectedVersion, actor.companyId),write.prepare("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),id,paymentId,"finance.payment.approve",actor.userId,actor.assignmentId,input.expectedVersion,await hash(input),now)); const output = { id: paymentId, status: "approved", version: input.expectedVersion + 1 }; return saveCommand(write, actor, id, "finance.payment.approve", input, output, pendingStatements); }

export async function releasePayment(actor: WorkContext, paymentId: string, input: { expectedVersion: number; proof: string }, id: string) {
  actor = requirePermission(actor,"finance.pay");
 if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_TREASURY_REQUIRED","Use a Company treasury assignment to release a controlled payment."); const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.payment.release", input); if (previous) return previous; if (!input.proof.trim()) throw new ApiError(422, "HUMAN_DECISION_REQUIRED", "Payment release evidence is required."); const payment = await write.prepare("SELECT * FROM finance_payments WHERE id=? AND company_id=? AND status='approved'").bind(paymentId, actor.companyId).first<FinanceRow>(); if (!payment) throw new ApiError(404, "PAYMENT_NOT_APPROVED", "Only an approved payment can be released."); if (Number(payment.version) !== input.expectedVersion) throw new ApiError(409, "VERSION_CONFLICT", "Payment changed. Refresh before releasing."); if(samePerson(actor,payment.created_by)||samePerson(actor,payment.approved_by))throw new ApiError(403,"INDEPENDENT_PAYER_REQUIRED","A third person must release and confirm this approved payment."); if(!payment.bank_account)throw new ApiError(422,"BANK_ACCOUNT_REQUIRED","Assign a bank ledger account before release."); const withholding=await readyPaymentWithholding(write,actor,payment,id);pendingStatements.push(...(withholding?.guards??[])); const now=Date.now();pendingStatements.push(assertion(write,id,"payment-version","EXISTS(SELECT 1 FROM finance_payments WHERE id=? AND company_id=? AND version=? AND status='approved' AND created_by!=? AND approved_by!=?)",[paymentId,actor.companyId,input.expectedVersion,actor.userId,actor.userId])); pendingStatements.push(write.prepare("UPDATE finance_payments SET status='released',released_by=?,version=version+1,updated_at=? WHERE id=? AND version=? AND company_id=?").bind(actor.userId, now, paymentId, input.expectedVersion, actor.companyId),write.prepare("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),id,paymentId,"finance.payment.release",actor.userId,actor.assignmentId,input.expectedVersion,await hash(input),now)); const output = { id: paymentId, status: "released", version: input.expectedVersion + 1, bankEvidenceRequired: true }; return saveCommand(write, actor, id, "finance.payment.release", input, output, pendingStatements); }

export const confirmPaymentSchema=z.object({
 expectedVersion:z.number().int().positive(),periodId:z.string().min(1),bankTransactionId:z.string().min(1),bankVersion:z.number().int().positive(),
 bankReference:z.string().trim().min(2).max(160),proof:z.string().trim().min(3).max(1000),
 fxRate:z.string().optional(),fxGainAccountId:z.string().min(1).optional(),fxLossAccountId:z.string().min(1).optional(),
 allocations:z.array(z.object({invoiceId:z.string().min(1),amount:moneySchema})).min(1).max(100),
 attachmentSetIds:z.array(z.string().min(1)).max(20).default([]),
});

export async function confirmPayment(actor:WorkContext,paymentId:string,input:z.infer<typeof confirmPaymentSchema>,key:string){
 actor=requirePermission(actor,"finance.pay");if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_TREASURY_REQUIRED","Use a Company treasury assignment to confirm bank evidence.");input=confirmPaymentSchema.parse(input);
 const {write}=await resources(actor),previous=await replayCommand(write,actor,key,"finance.payment.confirm",paymentId,input);if(previous)return previous;
 const payment=await write.prepare("SELECT * FROM finance_payments WHERE id=? AND company_id=?").bind(paymentId,actor.companyId).first<FinanceRow>();
 if(!payment)throw new ApiError(404,"PAYMENT_NOT_FOUND","Payment not found.");if(payment.status!=="released"||Number(payment.version)!==input.expectedVersion)throw new ApiError(409,"PAYMENT_NOT_CONFIRMABLE","Refresh the released payment before confirming it.");
 if(payment.kind==="refund")throw new ApiError(422,"REFUND_OWNER_WORKFLOW_REQUIRED","Confirm refunds from their approved refund obligation.");
 if(!payment.party_id||!payment.bank_account)throw new ApiError(422,"PAYMENT_SETUP_REQUIRED","The payment needs a party and bank ledger account.");
 if(samePerson(actor,payment.created_by)||samePerson(actor,payment.approved_by)||!samePerson(actor,payment.released_by))throw new ApiError(403,"INDEPENDENT_PAYER_REQUIRED","The independent payer who released this payment must confirm its bank result.");
 const attachmentBinding=input.attachmentSetIds.length?await prepareAttachmentBinding(write,actor,{attachmentSetIds:input.attachmentSetIds,bindingContract:{targetType:"finance_payment",bindingType:"finance_payment_confirmation",contexts:[{purposeKey:"payment_proof",componentKind:"action",componentRef:"bank-confirmation"}]},targetId:paymentId,expectedTargetVersion:input.expectedVersion,commandId:key,bindingId:`${paymentId}:${input.expectedVersion+1}`}):null;
 const ids=input.allocations.map(item=>item.invoiceId);if(new Set(ids).size!==ids.length)throw new ApiError(422,"DUPLICATE_ALLOCATION","Each invoice may appear once.");
 const amounts=input.allocations.map(item=>decimalToMinor(item.amount));if(amounts.some(amount=>amount<=0)||amounts.reduce((sum,amount)=>sum+amount,0)!==Number(payment.outstanding_minor))throw new ApiError(422,"ALLOCATION_TOTAL_MISMATCH","Allocation amounts must equal the payment amount.");
 const invoices=await write.prepare(`SELECT * FROM finance_invoices WHERE company_id=? AND id IN (${ids.map(()=>"?").join(",")})`).bind(actor.companyId,...ids).all<FinanceRow>();
 if(invoices.results.length!==ids.length)throw new ApiError(422,"INVOICE_INVALID","Every selected invoice must belong to this company.");const byId=new Map(invoices.results.map(row=>[String(row.id),row]));
 for(const [index,item] of input.allocations.entries()){const invoice=byId.get(item.invoiceId)!;if(invoice.party_id!==payment.party_id||invoice.currency!==payment.currency||invoice.kind!==(payment.kind==="receipt"?"receivable":"payable")||!["posted","partially_paid"].includes(String(invoice.status))||invoice.source_type==="expense_claim"||Number(invoice.outstanding_minor)<amounts[index])throw new ApiError(422,"INVOICE_ALLOCATION_MISMATCH","Invoices must be open, compatible with the payment, and have enough outstanding balance.");}
 const netBankMinor=Number(payment.amount_minor)-Number(payment.withheld_minor??0);
 const bank=await write.prepare("SELECT t.*,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=?").bind(input.bankTransactionId,actor.companyId).first<FinanceRow>(),signed=payment.kind==="receipt"?Number(payment.amount_minor):-netBankMinor;
 if(!bank||bank.bank_account!==payment.bank_account||bank.currency!==payment.currency||Number(bank.amount_minor)!==signed||String(bank.transaction_date)<String(payment.payment_date)||!["unmatched","proposed"].includes(String(bank.match_status))||Number(bank.version)!==input.bankVersion)throw new ApiError(422,"BANK_EVIDENCE_MISMATCH","Match the selected account, currency, date and exact bank amount.");
 const withholding=await readyPaymentWithholding(write,actor,payment,key,String(bank.transaction_date),input.allocations);
 const bankAccount=await write.prepare("SELECT a.id,a.chart_id,a.currency,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.id=? AND a.company_id=? AND a.category='asset' AND a.control_kind='bank' AND a.status='active'").bind(payment.bank_account,actor.companyId).first<FinanceRow>();if(!bankAccount)throw new ApiError(422,"BANK_ACCOUNT_INVALID","The selected ledger account is not configured as a bank control account.");
 const accountIds=[String(payment.bank_account),...invoices.results.map(invoice=>String(payment.kind==="receipt"?invoice.debit_account_id:invoice.credit_account_id))],accounts=await write.prepare(`SELECT id,chart_id,currency FROM finance_accounts WHERE company_id=? AND status='active' AND id IN (${[...new Set(accountIds)].map(()=>"?").join(",")})`).bind(actor.companyId,...new Set(accountIds)).all<FinanceRow>();
 if(accounts.results.length!==new Set(accountIds).size||new Set(accounts.results.map(account=>account.chart_id)).size!==1||accounts.results.some(account=>account.currency!==payment.currency))throw new ApiError(422,"PAYMENT_LEDGER_MISMATCH","Bank and control accounts must use one active chart and currency.");
 const settlementRate=input.fxRate?fxMicros(String(payment.currency),String(bankAccount.base_currency),input.fxRate):await confirmedFunctionalFxMicros(write,actor,String(payment.currency),String(bankAccount.base_currency),String(bank.transaction_date)),basePayment=convertAtFx(netBankMinor,settlementRate);
 const baseAllocations=input.allocations.map((item,index)=>{const invoice=byId.get(item.invoiceId)!,transactionOutstanding=Number(invoice.outstanding_minor),baseOutstanding=Number(invoice.base_outstanding_minor??convertAtFx(transactionOutstanding,Number(invoice.fx_rate_micros??1_000_000))),baseInvoice=proportionalBase(baseOutstanding,transactionOutstanding,amounts[index]),baseSettlement=convertAtFx(amounts[index],settlementRate);return {...item,amount:amounts[index],invoice,baseInvoice,baseSettlement,difference:baseSettlement-baseInvoice};});
 const lines:JournalLineInput[]=input.allocations.map((item,index)=>({accountId:String(payment.kind==="receipt"?byId.get(item.invoiceId)!.debit_account_id:byId.get(item.invoiceId)!.credit_account_id),debit:payment.kind==="payment"?minorToDecimal(amounts[index]):"0",credit:payment.kind==="receipt"?minorToDecimal(amounts[index]):"0",description:`Allocate ${byId.get(item.invoiceId)!.invoice_number}`}));
 lines.push({accountId:String(payment.bank_account),debit:payment.kind==="receipt"?minorToDecimal(Number(payment.amount_minor)):"0",credit:payment.kind==="payment"?minorToDecimal(netBankMinor):"0",description:input.bankReference});
 if(withholding&&Number(payment.withheld_minor)>0)lines.push({accountId:withholding.source.liabilityAccountId,debit:"0",credit:minorToDecimal(Number(payment.withheld_minor)),description:"Independently reviewed Thai withholding tax"});
 const journal=await prepareJournal(write,actor,{chartId:String(accounts.results[0].chart_id),periodId:input.periodId,entryDate:String(bank.transaction_date),currency:String(payment.currency),fxRate:input.fxRate,memo:`${payment.kind} ${payment.payment_number}: ${input.bankReference}`,sourceType:"payment",sourceId:paymentId,isOpening:false,lines},key,"posted");
 const adjustmentLines:JournalLineInput[]=[],gain=baseAllocations.filter(row=>payment.kind==="receipt"?row.difference>0:row.difference<0).reduce((sum,row)=>sum+Math.abs(row.difference),0),loss=baseAllocations.filter(row=>payment.kind==="receipt"?row.difference<0:row.difference>0).reduce((sum,row)=>sum+Math.abs(row.difference),0);
 for(const row of baseAllocations){if(!row.difference)continue;const debit=payment.kind==="receipt"?row.difference>0:row.difference<0;adjustmentLines.push({accountId:String(payment.kind==="receipt"?row.invoice.debit_account_id:row.invoice.credit_account_id),debit:debit?minorToDecimal(Math.abs(row.difference)):"0",credit:debit?"0":minorToDecimal(Math.abs(row.difference)),description:`Realized FX ${row.invoice.invoice_number}`});}
 if(gain){if(!input.fxGainAccountId)throw new ApiError(422,"FX_GAIN_ACCOUNT_REQUIRED","Select the functional-currency realized FX gain account.");adjustmentLines.push({accountId:input.fxGainAccountId,debit:"0",credit:minorToDecimal(gain),description:"Realized foreign exchange gain"});}
 if(loss){if(!input.fxLossAccountId)throw new ApiError(422,"FX_LOSS_ACCOUNT_REQUIRED","Select the functional-currency realized FX loss account.");adjustmentLines.push({accountId:input.fxLossAccountId,debit:minorToDecimal(loss),credit:"0",description:"Realized foreign exchange loss"});}
 if(gain||loss){const required=[...(gain?[input.fxGainAccountId!]:[]),...(loss?[input.fxLossAccountId!]:[])],validFx=await write.prepare(`SELECT id FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND currency=? AND ((id=? AND category='other_income') OR (id=? AND category='other_expense'))`).bind(actor.companyId,bankAccount.chart_id,bankAccount.base_currency,gain?input.fxGainAccountId!:"",loss?input.fxLossAccountId!:"").all();if(validFx.results.length!==new Set(required).size)throw new ApiError(422,"FX_ACCOUNT_INVALID","Realized gain and loss accounts must use the ledger functional currency and matching account category.");}
 const fxJournal=adjustmentLines.length?await prepareJournal(write,actor,{chartId:String(bankAccount.chart_id),periodId:input.periodId,entryDate:String(bank.transaction_date),currency:String(bankAccount.base_currency),memo:`Realized FX for ${payment.payment_number}`,sourceType:"payment_fx",sourceId:paymentId,isOpening:false,lines:adjustmentLines},`${key}-fx`,"posted"):null,now=Date.now();
 const snapshot=await legacyMoneySnapshot(write,actor,{sourceType:"finance_payment",sourceId:paymentId,amountKind:"settled",transactionDate:String(bank.transaction_date),currency:String(payment.currency),amountUnits:Number(payment.amount_minor)});
 const bankSnapshot=await legacyMoneySnapshot(write,actor,{sourceType:"finance_payment",sourceId:paymentId,amountKind:"bank_settled",transactionDate:String(bank.transaction_date),currency:String(payment.currency),amountUnits:netBankMinor});
 const withheldSnapshot=withholding&&Number(payment.withheld_minor)>0?await legacyMoneySnapshot(write,actor,{sourceType:"finance_payment",sourceId:paymentId,amountKind:"withheld",transactionDate:String(bank.transaction_date),currency:"THB",amountUnits:Number(payment.withheld_minor)}):null;
 const taxConsumption=withholding?await prepareWithholdingConsumption(write,actor,withholding,{commandId:key,paymentId,bankTransactionId:String(bank.id),bankDate:String(bank.transaction_date),journalId:journal.result.id}):null;
 return executeCommand(write,actor,key,"finance.payment.confirm",input,paymentId,input.expectedVersion+1,{id:paymentId,status:"settled",version:input.expectedVersion+1,journalId:journal.result.id,fxJournalId:fxJournal?.result.id??null,realizedFx:minorToDecimal(gain-loss),bankTransactionId:bank.id,withheldMinor:Number(payment.withheld_minor??0),netBankMinor,withholdingDocumentId:taxConsumption?.documentId??null},[
  ...(attachmentBinding?.guards??[]),...(withholding?.guards??[]),
  assertion(write,key,"payment","EXISTS(SELECT 1 FROM finance_payments WHERE id=? AND company_id=? AND version=? AND status='released' AND released_by=? AND created_by!=? AND approved_by!=?)",[paymentId,actor.companyId,input.expectedVersion,actor.userId,actor.userId,actor.userId]),
  assertion(write,key,"bank","EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=? AND t.version=? AND t.match_status IN ('unmatched','proposed') AND s.bank_account=? AND t.currency=? AND t.amount_minor=? AND t.transaction_date>=?)",[bank.id,actor.companyId,input.bankVersion,payment.bank_account,payment.currency,signed,payment.payment_date]),
  ...input.allocations.map((item,index)=>assertion(write,key,`invoice-${index}`,"EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND version=? AND status IN ('posted','partially_paid') AND outstanding_minor>=?)",[item.invoiceId,actor.companyId,byId.get(item.invoiceId)!.version,amounts[index]])),
  ...journal.steps,
  ...(fxJournal?.steps??[]),
  ...baseAllocations.map((item,index)=>write.prepare("INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,base_invoice_minor,base_payment_minor,realized_fx_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)").bind(`allocation-${key}-${index}`,actor.companyId,paymentId,item.invoiceId,item.amount,item.baseInvoice,item.baseSettlement,item.difference,actor.userId,now)),
  ...baseAllocations.map(item=>write.prepare("UPDATE finance_invoices SET outstanding_minor=outstanding_minor-?,base_outstanding_minor=base_outstanding_minor-?,status=CASE WHEN outstanding_minor=? THEN 'paid' ELSE 'partially_paid' END,version=version+1,updated_at=? WHERE id=?").bind(item.amount,item.baseInvoice,item.amount,now,item.invoiceId)),
  write.prepare("UPDATE finance_payments SET outstanding_minor=0,status='settled',journal_id=?,fx_rate_micros=?,base_amount_minor=?,fx_journal_id=?,version=version+1,updated_at=? WHERE id=?").bind(journal.result.id,settlementRate,basePayment,fxJournal?.result.id??null,now,paymentId),
  snapshot,bankSnapshot,...(withheldSnapshot?[withheldSnapshot]:[]),...(taxConsumption?.steps??[]),
  write.prepare("UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=?,version=version+1 WHERE id=?").bind(paymentId,bank.id),
  ...(attachmentBinding?.steps??[]),
  write.prepare("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),key,paymentId,"finance.payment.confirm",actor.userId,actor.assignmentId,input.expectedVersion,await hash(input),now),
 ]);
}

export async function allocatePayment(actor:WorkContext,paymentId:string,input:{invoiceId:string;amount:string;expectedVersion:number},key:string){
  actor = requirePermission(actor,"finance.write");

  const {write}=await resources(actor);const previous=await replayCommand(write,actor,key,"finance.payment.allocate",paymentId,input);if(previous)return previous;const amount=decimalToMinor(input.amount);if(amount<=0)throw new ApiError(422,"INVALID_ALLOCATION","Allocation must be positive.");
  return executeCommand(write,actor,key,"finance.payment.allocate",input,paymentId,input.expectedVersion+1,{paymentId,invoiceId:input.invoiceId,allocated:minorToDecimal(amount)},[
    assertion(write,key,"supplier-reservation","NOT EXISTS(SELECT 1 FROM finance_supplier_instructions WHERE invoice_id=? AND status IN ('reserved','exported','unknown'))",[input.invoiceId]),
    assertion(write,key,"targets","EXISTS(SELECT 1 FROM finance_payments p JOIN finance_invoices i ON i.id=? JOIN finance_journals j ON j.id=p.journal_id WHERE p.id=? AND p.company_id=? AND i.company_id=p.company_id AND i.source_type!='expense_claim' AND p.party_id=i.party_id AND p.currency=i.currency AND p.version=? AND p.status IN ('released','settled') AND j.status='posted' AND i.status IN ('posted','partially_paid') AND ((p.kind='receipt' AND i.kind='receivable') OR (p.kind='payment' AND i.kind='payable')) AND p.outstanding_minor>=? AND i.outstanding_minor>=?)",[input.invoiceId,paymentId,actor.companyId,input.expectedVersion,amount,amount]),
    assertion(write,key,"allocation-history","EXISTS(SELECT 1 FROM finance_payments p JOIN finance_journals pj ON pj.id=p.journal_id JOIN finance_periods period ON period.id=pj.period_id JOIN finance_charts c ON c.id=pj.chart_id JOIN finance_invoices i ON i.id=? JOIN finance_journals ij ON ij.id=i.journal_id WHERE p.id=? AND p.company_id=? AND period.company_id=p.company_id AND period.state='open' AND pj.entry_date>=ij.entry_date AND pj.chart_id=ij.chart_id AND p.currency=c.base_currency AND i.fx_rate_micros=1000000 AND COALESCE(p.fx_rate_micros,1000000)=1000000 AND EXISTS(SELECT 1 FROM finance_journal_lines l WHERE l.journal_id=pj.id AND l.account_id=CASE WHEN i.kind='receivable' THEN i.debit_account_id ELSE i.credit_account_id END))",[input.invoiceId,paymentId,actor.companyId]),
    write.prepare("INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,base_invoice_minor,base_payment_minor,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(`allocation-${key}`,actor.companyId,paymentId,input.invoiceId,amount,amount,amount,actor.userId,Date.now()),
    write.prepare("UPDATE finance_payments SET outstanding_minor=outstanding_minor-?,status=CASE WHEN outstanding_minor=? THEN 'settled' ELSE status END,version=version+1,updated_at=? WHERE id=?").bind(amount,amount,Date.now(),paymentId),
    write.prepare("UPDATE finance_invoices SET outstanding_minor=outstanding_minor-?,base_outstanding_minor=GREATEST(0,base_outstanding_minor-?),status=CASE WHEN outstanding_minor=? THEN 'paid' ELSE 'partially_paid' END,version=version+1,updated_at=? WHERE id=?").bind(amount,amount,amount,Date.now(),input.invoiceId),
  ]);
}

export async function exportPaymentFile(actor: WorkContext, paymentIds: string[], id: string) {
  actor = requirePermission(actor,"finance.write");
 const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const input = { paymentIds }; const previous = await priorCommand(write, actor, id, "finance.payment.export", input); if (previous) return previous; if (!paymentIds.length) throw new ApiError(422, "EMPTY_PAYMENT_BATCH", "Select at least one released payment."); const rows = await write.prepare(`SELECT id,payment_number,payment_date,currency,outstanding_minor,withheld_minor,bank_account FROM finance_payments WHERE company_id=? AND status='released' AND id IN (${paymentIds.map(() => "?").join(",")}) ORDER BY payment_number`).bind(actor.companyId, ...paymentIds).all<FinanceRow>(); if (rows.results.length !== paymentIds.length) throw new ApiError(409, "PAYMENT_BATCH_INVALID", "Every payment must be released and belong to this company."); const csv = ["payment_id,payment_number,payment_date,currency,amount,bank_account", ...rows.results.map((row) => [row.id, row.payment_number, row.payment_date, row.currency, minorToDecimal(Number(row.outstanding_minor)-Number(row.withheld_minor??0)), row.bank_account ?? ""].map((value) => `"${String(value).replaceAll('"', '""')}"`).join(","))].join("\n"); const output = { format: "csv", rows: rows.results.length, sha256: await hash(csv), content: csv, settlementRequired: true }; return saveCommand(write, actor, id, "finance.payment.export", input, output, pendingStatements); }

export async function bankWorkspace(actor:WorkContext,query?:{print?:string;transactionCursor?:string;statementCursor?:string;match?:string;new?:string;chart?:string;view?:string;q?:string}) {
 actor=requirePermission(actor,'finance.read');const {write}=await resources(actor,true);
 const statementSql=`SELECT s.*,a.currency,c.base_currency,s.statement_date AS page_key FROM finance_bank_statements s JOIN finance_accounts a ON a.id=s.bank_account AND a.company_id=s.company_id JOIN finance_charts c ON c.id=a.chart_id AND c.company_id=s.company_id WHERE s.company_id=? AND (?='' OR a.chart_id=?)`;
 const transactionSql=`SELECT t.*,s.bank_account,s.statement_number,t.transaction_date AS page_key FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id JOIN finance_accounts a ON a.id=s.bank_account AND a.company_id=t.company_id WHERE t.company_id=? AND (?='' OR a.chart_id=?) AND (?<>'unmatched' OR t.match_status<>'matched') AND (?='' OR strpos(lower(t.external_id||' '||COALESCE(t.description,'')),lower(?))>0)`;
 const statementArgs=[actor.companyId,query?.chart??'',query?.chart??''];
 const transactionArgs=[...statementArgs,query?.view??'',query?.q??'',query?.q??''];
 const paged=!!query&&query.print!=='1';
 const [accounts,statements,transactions,payments,selectedTransaction]=await Promise.all([
  !query||query.new==='1'?write.prepare("SELECT a.id,a.chart_id,a.code,a.name,a.currency,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.category='asset' AND a.control_kind IN ('none','bank') AND a.status='active' ORDER BY a.code").bind(actor.companyId).all<FinanceRow>():{results:[] as FinanceRow[]},
  paged?readListPage<FinanceRow & {id:string;page_key:string}>(write,{sql:statementSql,args:statementArgs,cursor:query.statementCursor,descending:true,scope:[actor.companyId,'bank-statements',query?.chart??'']}).then(page=>({results:page.items,page})):
    write.prepare(`${statementSql} ORDER BY s.statement_date DESC,s.id`).bind(...statementArgs).all<FinanceRow>().then(result=>({...result,page:null})),
  paged?readListPage<FinanceRow & {id:string;page_key:string}>(write,{sql:transactionSql,args:transactionArgs,cursor:query.transactionCursor,descending:true,scope:[actor.companyId,'bank-transactions',query?.chart??'',query?.view??'',query?.q??'']}).then(page=>({results:page.items,page})):
    write.prepare(`${transactionSql} ORDER BY t.transaction_date DESC,t.id`).bind(...transactionArgs).all<FinanceRow>().then(result=>({...result,page:null})),
  !query||query.match?write.prepare("SELECT p.* FROM finance_payments p JOIN finance_journals j ON j.id=p.journal_id WHERE p.company_id=? AND p.status IN ('released','settled') AND j.status='posted' AND NOT EXISTS(SELECT 1 FROM finance_bank_transactions t WHERE t.company_id=p.company_id AND t.matched_type='payment' AND t.matched_id=p.id AND t.match_status='matched') ORDER BY p.payment_date DESC").bind(actor.companyId).all<FinanceRow>():{results:[] as FinanceRow[]},
  query?.match?write.prepare("SELECT t.*,s.bank_account,s.statement_number FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id AND s.company_id=t.company_id WHERE t.id=? AND t.company_id=?").bind(query.match,actor.companyId).first<FinanceRow>():null,
 ]);
 if(query?.match&&!selectedTransaction)throw new ApiError(404,"BANK_TRANSACTION_NOT_FOUND","Bank movement not found in this Company.");
 return {accounts:accounts.results,statements:statements.results,statementPage:statements.page,transactions:transactions.results,transactionPage:transactions.page,payments:payments.results,selectedTransaction};
}
export async function importBankStatement(actor:WorkContext,input:{bankAccount:string;statementNumber:string;statementDate:string;opening:string;closing:string;closingFxRate?:string;sourceHash:string;rows:Array<{externalId:string;transactionDate:string;description?:string;amount:string;currency:string}>},key:string){
  actor = requirePermission(actor,"finance.write");

  const {write}=await resources(actor),id=`statement-${key}`;const previous=await replayCommand(write,actor,key,"finance.bank.import",id,input);if(previous)return previous;
  if(input.rows.length>500)throw new ApiError(422,"STATEMENT_SIZE","Import at most 500 statement rows.");
  const account=await write.prepare("SELECT a.currency,a.control_kind,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.id=? AND a.company_id=? AND a.category='asset' AND a.control_kind IN ('none','bank') AND a.status='active'").bind(input.bankAccount,actor.companyId).first<{currency:string;control_kind:string;base_currency:string}>();if(!account)throw new ApiError(422,"BANK_ACCOUNT_INVALID","Select an active cash or bank ledger account.");
  await requireFinanceMoneyScale(write,actor.companyId,account.currency);
  await requireFinanceMoneyScale(write,actor.companyId,account.base_currency);
  const opening=decimalToMinor(input.opening),closing=decimalToMinor(input.closing),amounts=input.rows.map(r=>decimalToMinor(r.amount));
  if(BigInt(opening)+amounts.reduce((n,a)=>n+BigInt(a),BigInt(0))!==BigInt(closing))throw new ApiError(422,"STATEMENT_UNBALANCED","Opening balance plus movements must equal the closing balance.");
  if(new Set(input.rows.map(r=>r.externalId)).size!==input.rows.length)throw new ApiError(422,"DUPLICATE_TRANSACTION","Each bank transaction needs a unique external reference.");
  if(input.rows.some(r=>r.currency.toUpperCase()!==account.currency||r.transactionDate>input.statementDate))throw new ApiError(422,"STATEMENT_ROW_INVALID","Rows must use the bank account currency and cannot follow the statement date.");
  const closingFxRate=input.closingFxRate?fxMicros(account.currency,account.base_currency,input.closingFxRate):await confirmedFunctionalFxMicros(write,actor,account.currency,account.base_currency,input.statementDate),baseClosing=convertAtFx(closing,closingFxRate),digest=await hash({bankAccount:input.bankAccount,statementNumber:input.statementNumber,opening,closing,closingFxRate,rows:input.rows});
  return executeCommand(write,actor,key,"finance.bank.import",input,id,1,{id,imported:input.rows.length,status:"imported"},[
    assertion(write,key,"duplicate-statement","NOT EXISTS(SELECT 1 FROM finance_bank_statements WHERE company_id=? AND bank_account=? AND (statement_number=? OR source_hash=?))",[actor.companyId,input.bankAccount,input.statementNumber,digest]),
    ...input.rows.map((r,i)=>assertion(write,key,`external-${i}`,"NOT EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.company_id=? AND s.bank_account=? AND t.external_id=?)",[actor.companyId,input.bankAccount,r.externalId])),
    assertion(write,key,"bank-account","EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND category='asset' AND status='active' AND control_kind IN ('none','bank'))",[input.bankAccount,actor.companyId]),
    write.prepare("UPDATE finance_accounts SET is_control=1,control_kind='bank',version=version+CASE WHEN control_kind='none' THEN 1 ELSE 0 END,updated_at=? WHERE id=? AND company_id=? AND control_kind='none'").bind(Date.now(),input.bankAccount,actor.companyId),
    write.prepare("INSERT INTO finance_bank_statements(id,company_id,bank_account,statement_number,statement_date,opening_minor,closing_minor,closing_fx_rate_micros,base_closing_minor,source_hash,imported_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(id,actor.companyId,input.bankAccount,input.statementNumber,input.statementDate,opening,closing,closingFxRate,baseClosing,digest,actor.userId,Date.now()),
    ...input.rows.map((r,i)=>write.prepare("INSERT INTO finance_bank_transactions(id,statement_id,company_id,transaction_date,external_id,description,amount_minor,currency) VALUES(?,?,?,?,?,?,?,?)").bind(`bankrow-${key}-${i}`,id,actor.companyId,r.transactionDate,r.externalId,r.description??"",amounts[i],r.currency.toUpperCase())),
  ]);
}
export async function reconcileBankTransaction(actor:WorkContext,transactionId:string,input:{expectedVersion:number;matchedType:string;matchedId:string;proof:string;attachmentSetIds:string[]},key:string){
  actor = requirePermission(actor,"finance.write");

  requireFinanceRole(actor,true);const {write}=await resources(actor);if(input.matchedType!=="payment"||!input.proof.trim())throw new ApiError(422,"MATCH_INPUT_INVALID","Select a posted payment and provide review evidence.");
  const previous=await replayCommand(write,actor,key,"finance.bank.reconcile",transactionId,input);if(previous)return previous;
  const attachmentBinding=input.attachmentSetIds.length?await prepareAttachmentBinding(write,actor,{attachmentSetIds:input.attachmentSetIds,bindingContract:{targetType:"finance_bank_transaction",bindingType:"finance_bank_reconciliation",contexts:[{purposeKey:"reconciliation_workpaper",componentKind:"action",componentRef:"reconcile"}]},targetId:transactionId,expectedTargetVersion:input.expectedVersion,commandId:key,bindingId:`${transactionId}:${input.expectedVersion+1}`}):null;
  return executeCommand(write,actor,key,"finance.bank.reconcile",input,transactionId,input.expectedVersion+1,{id:transactionId,status:"matched",version:input.expectedVersion+1},[
    ...(attachmentBinding?.guards??[]),
    assertion(write,key,"payment-match","EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id JOIN finance_payments p ON p.id=? JOIN finance_journals j ON j.id=p.journal_id WHERE t.id=? AND t.company_id=? AND p.company_id=t.company_id AND t.version=? AND t.match_status IN ('unmatched','proposed') AND p.currency=t.currency AND p.bank_account=s.bank_account AND p.status IN ('released','settled') AND j.status='posted' AND t.amount_minor=CASE WHEN p.kind='receipt' THEN p.amount_minor ELSE -(p.amount_minor-p.withheld_minor) END)",[input.matchedId,transactionId,actor.companyId,input.expectedVersion]),
    assertion(write,key,"already-matched","NOT EXISTS(SELECT 1 FROM finance_bank_transactions WHERE company_id=? AND matched_type='payment' AND matched_id=? AND match_status='matched')",[actor.companyId,input.matchedId]),
    write.prepare("UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=?,version=version+1 WHERE id=?").bind(input.matchedId,transactionId),
    ...(attachmentBinding?.steps??[]),
    write.prepare("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),key,transactionId,"finance.bank.reconcile",actor.userId,actor.assignmentId,input.expectedVersion,await hash(input),Date.now()),
  ]);
}

export const taxCodeSchema = z.object({ code: z.string().min(1).max(40), name: z.string().min(1).max(160), rate: z.string(), recoverable: z.string().default("1"), effectiveFrom: z.string().date(), effectiveTo: z.string().date().optional() });
export async function createTaxCode(actor: WorkContext, input: z.infer<typeof taxCodeSchema>, id: string) {
  actor = requirePermission(actor,"finance.write");
 const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.tax-code.create", input); if (previous) return previous; const taxId = `tax-${crypto.randomUUID()}`; const rate = decimalToMinor(input.rate, 6); const recoverable = decimalToMinor(input.recoverable, 6); if (rate < 0 || recoverable < 0 || recoverable > 1_000_000) throw new ApiError(422, "INVALID_TAX_RATE", "Tax rates are non-negative and recoverability is between 0 and 1."); pendingStatements.push(write.prepare("INSERT INTO finance_tax_codes(id,company_id,code,name,rate_micros,recoverable_micros,effective_from,effective_to,status) VALUES(?,?,?,?,?,?,?,?,?)").bind(taxId, actor.companyId, input.code, input.name, rate, recoverable, input.effectiveFrom, input.effectiveTo ?? null, "active")); const output = { id: taxId, ...input, rateMicros: rate, recoverableMicros: recoverable }; return saveCommand(write, actor, id, "finance.tax-code.create", input, output, pendingStatements); }

export { assetSchema, createAsset, depreciateAsset } from "./finance-assets";

export const budgetSchema = z.object({ name: z.string().min(1).max(160), fiscalYear: z.number().int().min(2000).max(2200), currency: z.string().length(3).transform((v) => v.toUpperCase()) });
export async function createBudget(actor: WorkContext, input: z.infer<typeof budgetSchema>, id: string) {
  actor = requirePermission(actor,"finance.write");
 const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.budget.create", input); if (previous) return previous; const budgetId = `budget-${crypto.randomUUID()}`; const now = Date.now(); pendingStatements.push(write.prepare("INSERT INTO finance_budgets(id,company_id,name,fiscal_year,currency,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)").bind(budgetId, actor.companyId, input.name, input.fiscalYear, input.currency, actor.userId, now, now)); const output = { id: budgetId, ...input, status: "draft" }; return saveCommand(write, actor, id, "finance.budget.create", input, output, pendingStatements); }


export const leaseSchema = z.object({ leaseNumber: z.string().min(1).max(64), counterparty: z.string().min(1).max(200), currency: z.string().length(3).transform((v) => v.toUpperCase()), commencementDate: z.string().date(), endDate: z.string().date(), payment: moneySchema, discountRate: z.string().default("0"), paymentFrequency: z.string().default("monthly") });
export async function createLease(actor: WorkContext, input: z.infer<typeof leaseSchema>, id: string) {
  actor = requirePermission(actor,"finance.write");
 const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.lease.create", input); if (previous) return previous; await requireFinanceMoneyScale(write,actor.companyId,input.currency); const leaseId = `lease-${crypto.randomUUID()}`; const now = Date.now(); const payment = decimalToMinor(input.payment); const rate = decimalToMinor(input.discountRate, 6); pendingStatements.push(write.prepare("INSERT INTO finance_leases(id,company_id,lease_number,counterparty,currency,commencement_date,end_date,payment_frequency,payment_minor,discount_rate_micros,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(leaseId, actor.companyId, input.leaseNumber, input.counterparty, input.currency, input.commencementDate, input.endDate, input.paymentFrequency, payment, rate, actor.userId, now, now)); const output = { id: leaseId, status: "draft", payment: input.payment }; return saveCommand(write, actor, id, "finance.lease.create", input, output, pendingStatements); }

export const loanSchema = z.object({ loanNumber: z.string().min(1).max(64), lender: z.string().min(1).max(200), currency: z.string().length(3).transform((v) => v.toUpperCase()), principal: moneySchema, annualRate: z.string().default("0"), commencementDate: z.string().date(), maturityDate: z.string().date() });
export async function createLoan(actor: WorkContext, input: z.infer<typeof loanSchema>, id: string) {
  actor = requirePermission(actor,"finance.write");
 const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.loan.create", input); if (previous) return previous; await requireFinanceMoneyScale(write,actor.companyId,input.currency); const loanId = `loan-${crypto.randomUUID()}`; const now = Date.now(); const principal = decimalToMinor(input.principal); pendingStatements.push(write.prepare("INSERT INTO finance_loans(id,company_id,loan_number,lender,currency,principal_minor,annual_rate_micros,commencement_date,maturity_date,outstanding_minor,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(loanId, actor.companyId, input.loanNumber, input.lender, input.currency, principal, decimalToMinor(input.annualRate, 6), input.commencementDate, input.maturityDate, principal, actor.userId, now, now)); const output = { id: loanId, status: "draft", principal: input.principal }; return saveCommand(write, actor, id, "finance.loan.create", input, output, pendingStatements); }

export const revenueContractSchema = z.object({ customerId: z.string(), contractNumber: z.string().min(1).max(64), currency: z.string().length(3).transform((v) => v.toUpperCase()), transactionPrice: moneySchema });
export async function createRevenueContract(actor: WorkContext, input: z.infer<typeof revenueContractSchema>, id: string) {
  actor = requirePermission(actor,"finance.write");
 const { write } = await resources(actor); const pendingStatements:SqlStatement[]=[]; const previous = await priorCommand(write, actor, id, "finance.revenue-contract.create", input); if (previous) return previous; await requireFinanceMoneyScale(write,actor.companyId,input.currency); const customer = await write.prepare("SELECT id FROM finance_parties WHERE id=? AND company_id=? AND kind='customer'").bind(input.customerId, actor.companyId).first(); if (!customer) throw new ApiError(404, "CUSTOMER_NOT_FOUND", "Customer not found."); const contractId = `contract-${crypto.randomUUID()}`; const now = Date.now(); pendingStatements.push(write.prepare("INSERT INTO finance_revenue_contracts(id,company_id,customer_id,contract_number,currency,transaction_price_minor,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(contractId, actor.companyId, input.customerId, input.contractNumber, input.currency, decimalToMinor(input.transactionPrice), actor.userId, now, now)); const output = { id: contractId, status: "draft", transactionPrice: input.transactionPrice }; return saveCommand(write, actor, id, "finance.revenue-contract.create", input, output, pendingStatements); }

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
export async function invoicesPage(actor:WorkContext,query:{q?:string;kind?:string;status?:string;queue?:string;cursor?:string;chartId?:string}) {
  actor=requirePermission(actor,"finance.read");const {write}=await resources(actor,true);
  return readListPage<FinanceRow & {id:string;page_key:string}>(write,{
    sql:`SELECT i.id,i.invoice_number,i.kind,i.invoice_date,i.due_date,i.currency,i.total_minor,i.outstanding_minor,i.status,p.name AS party_name,
      s.usd_units::text AS snapshot_usd_units,s.amount_units::text AS snapshot_source_units,s.fx_rate_id AS snapshot_rate_id,
      i.invoice_date||':'||i.invoice_number AS page_key FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id
      LEFT JOIN money_transaction_snapshots s ON s.company_id=i.company_id AND s.source_type='finance_invoice' AND s.source_id=i.id AND s.amount_kind='total'
      WHERE i.company_id=?
      AND (?='' OR EXISTS (SELECT 1 FROM finance_accounts a WHERE a.id=i.debit_account_id AND a.company_id=i.company_id AND a.chart_id=?))
      AND (?='' OR i.kind=?) AND (?='' OR i.status=?) AND (?<>'open' OR (i.status IN ('posted','partially_paid') AND i.outstanding_minor>0))
      AND (?='' OR strpos(lower(i.invoice_number||' '||p.name),lower(?))>0)`,
    args:[actor.companyId,query.chartId??'',query.chartId??'',query.kind??'',query.kind??'',query.status??'',query.status??'',query.queue??'',query.q??'',query.q??''],
    descending:true,cursor:query.cursor,scope:[actor.companyId,'invoices',query.chartId??'',query.q??'',query.kind??'',query.status??'',query.queue??''],
  });
}
