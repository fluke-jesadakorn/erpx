import {z} from 'zod';import type {WorkContext} from '@erpx/server-identity_access/public/authorization';import type {createJournalSchema} from '@erpx/server-finance/public/finance';import {minorToDecimal} from "@erpx/feature-finance/lib/finance-money";import {BusinessRuleError,type Clock} from '@erpx/server-shared/index';import type {PayrollStatutoryCommands} from '@erpx/server-people_payroll/application/payroll-statutory';import {payrollRemittanceSchema,type RemittanceKind,type Obligation,type PostedRun,descriptions} from '@erpx/server-people_payroll/domain/payroll-remittance';
export interface PayrollRemittanceRepository<Effect>{postedRun(runId:string,companyId:string):Promise<PostedRun|null>;
credits(journal_id:string,value:string):Promise<{results:{account_id:string;amount_minor:number}[]}>;
remitted(id:string,kind:"withholding_tax" | "social_security"):Promise<{amount_minor:number}|null>;
bankTransaction(bankTransactionId:string,companyId:string):Promise<{
    id:string;version:number;transaction_date:string;external_id:string;amount_minor:number;currency:string;match_status:string;bank_account:string;
  }|null>;
bankAccount(bankAccountId:string,companyId:string,chart_id:string,currency:string):Promise<Record<string,unknown>|null>;
periods(companyId:string,transaction_date:string,transaction_date2:string):Promise<{results:{id:string}[]}>;
lockRun(runId:string,companyId:string):Effect;
guard8(key:string,value:"payroll-posted",runId:string,companyId:string,expectedRunVersion:number):Effect;
guard9(key:string,value:"liability-balance",journal_id:string,value2:string,amountMinor:number,runId:string,kind:"withholding_tax" | "social_security"):Effect;
guard10(key:string,value:"bank-match",bankTransactionId:string,companyId:string,bankVersion:number,value2:number,currency:string,bankReference:string,period_end:string,bankAccountId:string):Effect;
insertRemittance(remittanceId:string,companyId:string,runId:string,kind:"withholding_tax" | "social_security",amountMinor:number,accountId:string,bankAccountId:string,bankTransactionId:string,id:string,bankReference:string,userId:string,now:number):Effect;
matchBank(remittanceId:string,bankTransactionId:string,bankVersion:number):Effect;}export interface PayrollRemittancePorts<Effect>{repository:PayrollRemittanceRepository<Effect>;clock:Clock;production():Promise<boolean>;authorize(actor:WorkContext,permission:string):WorkContext;prepare(holdingId:string):Promise<void>;commands:PayrollStatutoryCommands<Effect>;statutoryReady(runId:string):Promise<void>;journal(actor:WorkContext,input:z.infer<typeof createJournalSchema>,key:string,state:'posted') :Promise<{result:{id:string};steps:Effect[]}>;}export function createPayrollRemittanceApplication<Effect>(ports:PayrollRemittancePorts<Effect>){const repository=ports.repository;async function postedRun(companyId: string, runId: string): Promise<PostedRun> {
  const run = await repository.postedRun(runId,companyId);
  if (!run) throw new BusinessRuleError(409,"PAYROLL_POSTING_MISSING","A posted payroll accrual is required before remitting statutory liabilities.");
  return run;
}
async function obligation(run: PostedRun, kind: RemittanceKind): Promise<Obligation> {
  const credits=(await repository.credits(run.journal_id,descriptions[kind])).results;
  if (credits.length > 1 || credits.some(row=>Number(row.amount_minor)<0))
    throw new BusinessRuleError(409,"PAYROLL_LIABILITY_AMBIGUOUS","The statutory liability must have one account and a nonnegative posted balance.");
  const accruedMinor=Number(credits[0]?.amount_minor??0);
  const remitted=await repository.remitted(run.id,kind);
  const remittedMinor=Number(remitted?.amount_minor??0),outstandingMinor=accruedMinor-remittedMinor;
  if (![accruedMinor,remittedMinor,outstandingMinor].every(Number.isSafeInteger) || outstandingMinor<0)
    throw new BusinessRuleError(409,"PAYROLL_LIABILITY_INVALID","Statutory liability totals need Finance review.");
  return {kind,accountId:credits[0]?.account_id??null,accruedMinor,remittedMinor,outstandingMinor};
}
async function getPayrollLiabilities(actor: WorkContext, runId: string) {
  actor=ports.authorize(actor,"finance.pay");
  if(actor.branchId||actor.departmentId)throw new BusinessRuleError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Use a company Treasury assignment to view payroll liabilities.");
  await ports.prepare(actor.holdingId);const run=await postedRun(actor.companyId,runId);
  return {runId,runVersion:run.version,currency:run.currency,obligations:await Promise.all((Object.keys(descriptions) as RemittanceKind[]).map(kind=>obligation(run,kind)))};
}
async function remitPayrollLiability(actor: WorkContext, runId: string, input: z.infer<typeof payrollRemittanceSchema>, key: string) {
  actor=ports.authorize(actor,"finance.pay");
  if(actor.branchId||actor.departmentId)throw new BusinessRuleError(403,"COMPANY_PAYROLL_SCOPE_REQUIRED","Use a company Treasury assignment to remit payroll liabilities.");
  await ports.prepare(actor.holdingId);
  const previous=await ports.commands.replay(actor,key,"payroll.liability.remit",runId,input);
  if(previous)return previous;
  const run=await postedRun(actor.companyId,runId);
  if(await ports.production())await ports.statutoryReady(runId);
  if(run.version!==input.expectedRunVersion)throw new BusinessRuleError(409,"PAYROLL_RUN_CHANGED","Refresh the payroll run before remitting its liability.");
  const due=await obligation(run,input.kind);
  if(!due.accountId||input.amountMinor>due.outstandingMinor)throw new BusinessRuleError(422,"PAYROLL_LIABILITY_EXCEEDED","Remittance must not exceed the outstanding statutory liability.");
  const transaction=await repository.bankTransaction(input.bankTransactionId,actor.companyId);
  if(!transaction||transaction.version!==input.bankVersion||!["unmatched","proposed"].includes(transaction.match_status)||
    transaction.bank_account!==input.bankAccountId||transaction.currency!==run.currency||
    Number(transaction.amount_minor)!==-input.amountMinor||transaction.external_id!==input.bankReference||
    transaction.transaction_date<run.period_end)
    throw new BusinessRuleError(422,"BANK_EVIDENCE_MISMATCH","Match the exact imported bank debit, account, currency, date, reference and version.");
  const bank=await repository.bankAccount(input.bankAccountId,actor.companyId,run.chart_id,run.currency);
  if(!bank)throw new BusinessRuleError(422,"BANK_ACCOUNT_INVALID","Select an active bank asset account in this payroll chart and currency.");
  const periods=(await repository.periods(actor.companyId,transaction.transaction_date,transaction.transaction_date)).results;
  if(periods.length!==1)throw new BusinessRuleError(409,"PAYROLL_SETTLEMENT_PERIOD_REQUIRED","One open accounting period must contain the bank transaction date.");
  const remittanceId=`payroll-remit-${key}`,now=ports.clock.now();
  const journal=await ports.journal(actor,{
    chartId:run.chart_id,periodId:periods[0].id,entryDate:transaction.transaction_date,currency:run.currency,
    memo:`Payroll ${input.kind.replaceAll("_"," ")} remittance · ${input.bankReference}`,
    sourceType:"payroll_remittance",sourceId:remittanceId,isOpening:false,
    lines:[
      {accountId:due.accountId,debit:minorToDecimal(input.amountMinor),credit:"0",description:"Statutory payroll payable remitted"},
      {accountId:input.bankAccountId,debit:"0",credit:minorToDecimal(input.amountMinor),description:"Confirmed statutory bank debit"},
    ],
  },`${key}-journal`,"posted");
  const result={id:remittanceId,runId,kind:input.kind,amountMinor:input.amountMinor,journalId:journal.result.id,remainingMinor:due.outstandingMinor-input.amountMinor};
  return ports.commands.commit(actor,key,"payroll.liability.remit",input,runId,run.version,result,[
    repository.lockRun(runId,actor.companyId),
    repository.guard8(key,"payroll-posted",runId,actor.companyId,input.expectedRunVersion),
    repository.guard9(key,"liability-balance",run.journal_id,descriptions[input.kind],input.amountMinor,runId,input.kind),
    repository.guard10(key,"bank-match",input.bankTransactionId,actor.companyId,input.bankVersion,-input.amountMinor,run.currency,input.bankReference,run.period_end,input.bankAccountId),
    ...journal.steps,
    repository.insertRemittance(remittanceId,actor.companyId,runId,input.kind,input.amountMinor,due.accountId,input.bankAccountId,input.bankTransactionId,journal.result.id,input.bankReference,actor.userId,now),
    repository.matchBank(remittanceId,input.bankTransactionId,input.bankVersion),
  ]);
}return {getPayrollLiabilities,remitPayrollLiability};}
