import "server-only";

import { z } from "zod";
import type { WorkContext } from "./authorization";
import { requirePermission } from "./iam-permissions";
import { ApiError } from "./http";
import { holdingResources, runtimeEnv } from "./runtime";
import type { SqlDatabase } from "./runtime/types";
import { assertion, executeCommand, replayCommand } from "./commands";
import { prepareJournal } from "./finance";
import { minorToDecimal } from "@/lib/finance-money";
import { assertStatutoryRunReady } from "./payroll-statutory";

export const payrollRemittanceSchema = z.object({
  kind: z.enum(["withholding_tax", "social_security"]),
  amountMinor: z.number().int().positive().safe(),
  expectedRunVersion: z.number().int().positive(),
  bankAccountId: z.string().min(1),
  bankTransactionId: z.string().min(1),
  bankVersion: z.number().int().positive(),
  bankReference: z.string().min(1).max(160),
}).strict();

type RemittanceKind = z.infer<typeof payrollRemittanceSchema>["kind"];
type Obligation = { kind: RemittanceKind; accountId: string | null; accruedMinor: number; remittedMinor: number; outstandingMinor: number };
type PostedRun = { id: string; version: number; currency: string; period_end: string; chart_id: string; journal_id: string };

const descriptions: Record<RemittanceKind, string> = {
  withholding_tax: "Payroll tax",
  social_security: "Payroll social security",
};

async function postedRun(write: SqlDatabase, companyId: string, runId: string): Promise<PostedRun> {
  const run = await write.prepare(`SELECT r.id,r.version,r.currency,r.period_end,j.chart_id,p.journal_id
    FROM hr_payroll_runs r JOIN hr_payroll_postings p ON p.run_id=r.id
    JOIN finance_journals j ON j.id=p.journal_id AND j.status='posted'
    WHERE r.id=? AND r.company_id=? AND r.status='posted'`).bind(runId,companyId).first<PostedRun>();
  if (!run) throw new ApiError(409,"PAYROLL_POSTING_MISSING","A posted payroll accrual is required before remitting statutory liabilities.");
  return run;
}

async function obligation(write: SqlDatabase, run: PostedRun, kind: RemittanceKind): Promise<Obligation> {
  const credits=(await write.prepare(`SELECT account_id,SUM(credit_minor-debit_minor) AS amount_minor
    FROM finance_journal_lines WHERE journal_id=? AND description=?
    GROUP BY account_id`).bind(run.journal_id,descriptions[kind]).all<{account_id:string;amount_minor:number}>()).results;
  if (credits.length > 1 || credits.some(row=>Number(row.amount_minor)<0))
    throw new ApiError(409,"PAYROLL_LIABILITY_AMBIGUOUS","The statutory liability must have one account and a nonnegative posted balance.");
  const accruedMinor=Number(credits[0]?.amount_minor??0);
  const remitted=await write.prepare(`SELECT COALESCE(SUM(amount_minor),0) AS amount_minor FROM hr_payroll_statutory_remittances
    WHERE run_id=? AND kind=?`).bind(run.id,kind).first<{amount_minor:number}>();
  const remittedMinor=Number(remitted?.amount_minor??0),outstandingMinor=accruedMinor-remittedMinor;
  if (![accruedMinor,remittedMinor,outstandingMinor].every(Number.isSafeInteger) || outstandingMinor<0)
    throw new ApiError(409,"PAYROLL_LIABILITY_INVALID","Statutory liability totals need Finance review.");
  return {kind,accountId:credits[0]?.account_id??null,accruedMinor,remittedMinor,outstandingMinor};
}

export async function getPayrollLiabilities(actor: WorkContext, runId: string) {
  actor=requirePermission(actor,"finance.pay");
  if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Use a company Treasury assignment to view payroll liabilities.");
  const {write}=await holdingResources(actor.holdingId),run=await postedRun(write,actor.companyId,runId);
  return {runId,runVersion:run.version,currency:run.currency,obligations:await Promise.all((Object.keys(descriptions) as RemittanceKind[]).map(kind=>obligation(write,run,kind)))};
}

export async function remitPayrollLiability(actor: WorkContext, runId: string, input: z.infer<typeof payrollRemittanceSchema>, key: string) {
  actor=requirePermission(actor,"finance.pay");
  if(actor.branchId||actor.departmentId)throw new ApiError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Use a company Treasury assignment to remit payroll liabilities.");
  const {write}=await holdingResources(actor.holdingId);
  const previous=await replayCommand(write,actor,key,"payroll.liability.remit",runId,input);
  if(previous)return previous;
  const run=await postedRun(write,actor.companyId,runId);
  if((await runtimeEnv()).ERPX_ENVIRONMENT==="production")await assertStatutoryRunReady(write,runId);
  if(run.version!==input.expectedRunVersion)throw new ApiError(409,"PAYROLL_RUN_CHANGED","Refresh the payroll run before remitting its liability.");
  const due=await obligation(write,run,input.kind);
  if(!due.accountId||input.amountMinor>due.outstandingMinor)throw new ApiError(422,"PAYROLL_LIABILITY_EXCEEDED","Remittance must not exceed the outstanding statutory liability.");
  const transaction=await write.prepare(`SELECT t.id,t.version,t.transaction_date,t.external_id,t.amount_minor,t.currency,t.match_status,
      s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id
    WHERE t.id=? AND t.company_id=?`).bind(input.bankTransactionId,actor.companyId).first<{
    id:string;version:number;transaction_date:string;external_id:string;amount_minor:number;currency:string;match_status:string;bank_account:string;
  }>();
  if(!transaction||transaction.version!==input.bankVersion||!["unmatched","proposed"].includes(transaction.match_status)||
    transaction.bank_account!==input.bankAccountId||transaction.currency!==run.currency||
    Number(transaction.amount_minor)!==-input.amountMinor||transaction.external_id!==input.bankReference||
    transaction.transaction_date<run.period_end)
    throw new ApiError(422,"BANK_EVIDENCE_MISMATCH","Match the exact imported bank debit, account, currency, date, reference and version.");
  const bank=await write.prepare(`SELECT id FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=?
    AND category='asset' AND currency=? AND status='active'`).bind(input.bankAccountId,actor.companyId,run.chart_id,run.currency).first();
  if(!bank)throw new ApiError(422,"BANK_ACCOUNT_INVALID","Select an active bank asset account in this payroll chart and currency.");
  const periods=(await write.prepare(`SELECT id FROM finance_periods WHERE company_id=? AND state='open'
    AND start_date<=? AND end_date>=?`).bind(actor.companyId,transaction.transaction_date,transaction.transaction_date).all<{id:string}>()).results;
  if(periods.length!==1)throw new ApiError(409,"PAYROLL_SETTLEMENT_PERIOD_REQUIRED","One open accounting period must contain the bank transaction date.");
  const remittanceId=`payroll-remit-${key}`,now=Date.now();
  const journal=await prepareJournal(write,actor,{
    chartId:run.chart_id,periodId:periods[0].id,entryDate:transaction.transaction_date,currency:run.currency,
    memo:`Payroll ${input.kind.replaceAll("_"," ")} remittance · ${input.bankReference}`,
    sourceType:"payroll_remittance",sourceId:remittanceId,isOpening:false,
    lines:[
      {accountId:due.accountId,debit:minorToDecimal(input.amountMinor),credit:"0",description:"Statutory payroll payable remitted"},
      {accountId:input.bankAccountId,debit:"0",credit:minorToDecimal(input.amountMinor),description:"Confirmed statutory bank debit"},
    ],
  },`${key}-journal`,"posted");
  const result={id:remittanceId,runId,kind:input.kind,amountMinor:input.amountMinor,journalId:journal.result.id,remainingMinor:due.outstandingMinor-input.amountMinor};
  return executeCommand(write,actor,key,"payroll.liability.remit",input,runId,run.version,result,[
    write.prepare("SELECT id FROM hr_payroll_runs WHERE id=? AND company_id=? FOR UPDATE").bind(runId,actor.companyId),
    assertion(write,key,"payroll-posted","EXISTS(SELECT 1 FROM hr_payroll_runs WHERE id=? AND company_id=? AND status='posted' AND version=?)",[runId,actor.companyId,input.expectedRunVersion]),
    assertion(write,key,"liability-balance",`(SELECT COALESCE(SUM(credit_minor-debit_minor),0) FROM finance_journal_lines WHERE journal_id=? AND description=?)
      >= ? + (SELECT COALESCE(SUM(amount_minor),0) FROM hr_payroll_statutory_remittances WHERE run_id=? AND kind=?)`,
      [run.journal_id,descriptions[input.kind],input.amountMinor,runId,input.kind]),
    assertion(write,key,"bank-match",`EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id
      WHERE t.id=? AND t.company_id=? AND t.version=? AND t.match_status IN ('unmatched','proposed') AND
        t.amount_minor=? AND t.currency=? AND t.external_id=? AND t.transaction_date>=? AND s.bank_account=?)`,
      [input.bankTransactionId,actor.companyId,input.bankVersion,-input.amountMinor,run.currency,input.bankReference,run.period_end,input.bankAccountId]),
    ...journal.steps,
    write.prepare(`INSERT INTO hr_payroll_statutory_remittances(id,company_id,run_id,kind,amount_minor,liability_account_id,
      bank_account_id,bank_transaction_id,journal_id,bank_reference,recorded_by,recorded_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)`)
      .bind(remittanceId,actor.companyId,runId,input.kind,input.amountMinor,due.accountId,input.bankAccountId,
        input.bankTransactionId,journal.result.id,input.bankReference,actor.userId,now),
    write.prepare(`UPDATE finance_bank_transactions SET match_status='matched',matched_type='payroll_remittance',matched_id=?,
      version=version+1 WHERE id=? AND version=?`).bind(remittanceId,input.bankTransactionId,input.bankVersion),
  ]);
}
