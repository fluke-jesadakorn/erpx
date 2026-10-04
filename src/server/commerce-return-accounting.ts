import 'server-only';
import type { WorkContext } from './authorization';
import type { SqlDatabase, SqlStatement } from './runtime/types';
import { assertion } from './commands';
import { prepareJournal } from './finance';
import { minorToDecimal } from '@/lib/finance-money';
import { ApiError } from './http';

type Posting={account:string;debit:number;credit:number};
function proportional(total:number,numerator:number,denominator:number){
 if(denominator<=0||numerator<0||numerator>denominator)throw new ApiError(409,'RETURN_CARRYING_RECONCILIATION_REQUIRED','Reconcile original return carrying evidence before posting.');
 return Number((BigInt(total)*BigInt(numerator)+BigInt(Math.floor(denominator/2)))/BigInt(denominator));
}

/** The return consumes the original posted control value, including its final rounding cent. */
export async function prepareReturnCredit(write:SqlDatabase,a:WorkContext,input:{key:string;invoiceId:string;returnId:string;periodId:string;entryDate:string;postings:Posting[]}){
 const original=await write.prepare(`SELECT i.id,i.company_id,i.kind,i.currency,i.total_minor,i.fx_rate_micros,i.debit_account_id,i.credit_account_id,
  j.id journal_id,j.chart_id,j.status,c.base_currency,
  (SELECT SUM(CASE WHEN i.kind='receivable' THEN l.base_debit_minor-l.base_credit_minor ELSE l.base_credit_minor-l.base_debit_minor END)
   FROM finance_journal_lines l WHERE l.journal_id=j.id AND l.account_id=CASE WHEN i.kind='receivable' THEN i.debit_account_id ELSE i.credit_account_id END) control_base
  FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id AND j.company_id=i.company_id JOIN finance_charts c ON c.id=j.chart_id
  WHERE i.id=? AND i.company_id=?`).bind(input.invoiceId,a.companyId).first<{id:string;company_id:string;kind:string;currency:string;total_minor:number;fx_rate_micros:number;debit_account_id:string;credit_account_id:string;journal_id:string;chart_id:string;status:string;base_currency:string;control_base:number}>();
 if(!original||original.status!=='posted'||original.debit_account_id===original.credit_account_id||Number(original.control_base)<=0)throw new ApiError(409,'RETURN_ORIGINAL_CONTROL_REQUIRED','The original posted invoice control value must be reconciled before crediting.');
 const incoming=original.kind==='receivable',control=incoming?original.debit_account_id:original.credit_account_id,
   total=input.postings.find(p=>p.account===control)?.[incoming?'credit':'debit'];
 if(total===undefined||total<=0)throw new ApiError(422,'RETURN_CONTROL_MAPPING_REQUIRED','The credit note must reverse the original invoice control account.');
 const prior=await write.prepare(`SELECT COALESCE(SUM(r.total_minor),0) transaction_total,
  COALESCE(SUM((SELECT SUM(CASE WHEN ? THEN l.base_credit_minor-l.base_debit_minor ELSE l.base_debit_minor-l.base_credit_minor END)
   FROM finance_journal_lines l WHERE l.journal_id=r.credit_journal_id AND l.account_id=?)),0) base_total
  FROM commerce_returns r WHERE r.invoice_id=? AND r.id<>? AND r.status IN ('approved','posted')`).bind(incoming,control,original.id,input.returnId).first<{transaction_total:number;base_total:number}>();
 const previous=Number(prior?.transaction_total??0),previousBase=Number(prior?.base_total??0),baseCredit=proportional(Number(original.control_base),previous+total,Number(original.total_minor))-previousBase;
 if(baseCredit<0)throw new ApiError(409,'RETURN_CARRYING_RECONCILIATION_REQUIRED','Earlier credits do not reconcile to the original invoice value.');
 const journal=await prepareJournal(write,a,{chartId:original.chart_id,periodId:input.periodId,entryDate:input.entryDate,currency:original.currency,
  fxRate:minorToDecimal(Number(original.fx_rate_micros),6),memo:`Original carrying-value return ${input.returnId}`,sourceType:incoming?'credit_note':'debit_note',sourceId:input.returnId,isOpening:false,
  lines:input.postings.filter(p=>p.debit||p.credit).map(p=>({accountId:p.account,debit:minorToDecimal(p.debit),credit:minorToDecimal(p.credit)}))},`${input.key}:credit`,'draft');
 // Allocate each retained original account, including VAT and reviewed price
 // variance. A full return must reverse every original account's final cent.
 // Adjust a partial account's cent only while that source still has capacity;
 // never move a fully consumed tax amount onto another account.
 const origin=(await write.prepare(`SELECT account_id,SUM(debit_minor-credit_minor) transaction_amount,
   SUM(base_debit_minor-base_credit_minor) base_amount FROM finance_journal_lines WHERE journal_id=? GROUP BY account_id`)
   .bind(original.journal_id).all<{account_id:string;transaction_amount:number;base_amount:number}>()).results;
 const earlier=(await write.prepare(`SELECT l.account_id,SUM(l.credit_minor-l.debit_minor) transaction_amount,
   SUM(l.base_credit_minor-l.base_debit_minor) base_amount FROM commerce_returns r JOIN finance_journal_lines l ON l.journal_id=r.credit_journal_id
   WHERE r.invoice_id=? AND r.id<>? AND r.status IN ('approved','posted') GROUP BY l.account_id`)
   .bind(original.id,input.returnId).all<{account_id:string;transaction_amount:number;base_amount:number}>()).results;
 const allocated=journal.preparedLines.map(line=>{
   const source=origin.find(row=>row.account_id===line.accountId),used=earlier.find(row=>row.account_id===line.accountId),
    transaction=line.credit-line.debit,sourceAmount=Number(source?.transaction_amount??0),
    previousAmount=Number(used?.transaction_amount??0),previousCarrying=Number(used?.base_amount??0),
    cumulative=previousAmount+transaction,baseSource=Number(source?.base_amount??0);
   if(!source||!sourceAmount||Math.sign(transaction)!==Math.sign(sourceAmount)||Math.sign(cumulative)!==Math.sign(sourceAmount)||Math.abs(cumulative)>Math.abs(sourceAmount))
    throw new ApiError(409,'RETURN_ACCOUNT_CARRYING_REQUIRED','The credit must consume the original posted account and document amount.');
   const carrying=Math.sign(baseSource)*(proportional(Math.abs(baseSource),Math.abs(cumulative),Math.abs(sourceAmount)))-previousCarrying;
   if(carrying&&Math.sign(carrying)!==Math.sign(sourceAmount))throw new ApiError(409,'RETURN_CARRYING_RECONCILIATION_REQUIRED','Earlier account credits do not reconcile to the original posted source.');
   return {...line,baseDebit:line.debit?Math.abs(carrying):0,baseCredit:line.credit?Math.abs(carrying):0,
    fullyConsumed:Math.abs(cumulative)===Math.abs(sourceAmount),remainingBase:Math.abs(baseSource)-Math.abs(previousCarrying)-Math.abs(carrying)};
 });
 const controlPosting=allocated.find(line=>line.accountId===control);
 if(!controlPosting||(incoming?controlPosting.baseCredit:controlPosting.baseDebit)!==baseCredit)
  throw new ApiError(409,'RETURN_CARRYING_RECONCILIATION_REQUIRED','Original control and retained account credits must agree.');
 const difference=allocated.reduce((sum,line)=>sum+line.baseDebit-line.baseCredit,0);
 if(difference){
  const candidates=allocated.filter(line=>line.accountId!==control&&!line.fullyConsumed)
   .sort((left,right)=>right.remainingBase-left.remainingBase||right.baseDebit+right.baseCredit-left.baseDebit-left.baseCredit);
  const counter=candidates.find(line=>{const delta=line.credit?difference:-difference,amount=line.baseDebit+line.baseCredit+delta;return amount>=0&&delta<=line.remainingBase;});
  if(!counter)throw new ApiError(409,'RETURN_CARRYING_RECONCILIATION_REQUIRED','Partial account rounding cannot consume more than the original source carrying.');
  if(counter.credit)counter.baseCredit+=difference;else counter.baseDebit-=difference;
 }
 const basePostings=allocated.map((line,index)=>({...journal.preparedLines[index],baseDebit:line.baseDebit,baseCredit:line.baseCredit}));
 const steps:SqlStatement[]=[
  write.prepare('SELECT id FROM finance_invoices WHERE id=? FOR UPDATE').bind(original.id),
  assertion(write,input.key,'original-credit-capacity',`COALESCE((SELECT SUM(r.total_minor) FROM commerce_returns r WHERE r.invoice_id=? AND r.id<>? AND r.status IN ('approved','posted')),0)=?`,[original.id,input.returnId,previous]),
  ...journal.steps,
  ...basePostings.map((line,ordinal)=>write.prepare('UPDATE finance_journal_lines SET base_debit_minor=?,base_credit_minor=? WHERE journal_id=? AND ordinal=? AND account_id=?')
   .bind(line.baseDebit,line.baseCredit,journal.result.id,ordinal,line.accountId)),
  assertion(write,input.key,'credit-carrying-balance',"(SELECT SUM(base_debit_minor)=SUM(base_credit_minor) FROM finance_journal_lines WHERE journal_id=?)",[journal.result.id]),
 ];
 return {steps,journalId:journal.result.id,baseCreditMinor:baseCredit,controlAccountId:control,baseCurrency:original.base_currency,basePostings};
}

/** Separate applied receivable/payable carrying from the refund that stays on the control account. */
export function returnCompletionValues(input:{totalMinor:number;baseCreditMinor:number;outstandingMinor:number;baseOutstandingMinor:number}){
 const credited=Math.min(input.totalMinor,input.outstandingMinor),refund=input.totalMinor-credited,
  originalApplied=credited===input.totalMinor?input.baseCreditMinor:proportional(input.baseCreditMinor,credited,input.totalMinor),
  currentApplied=credited?proportional(input.baseOutstandingMinor,credited,input.outstandingMinor):0;
 return {creditedMinor:credited,refundMinor:refund,baseCreditedMinor:currentApplied,baseRefundMinor:input.baseCreditMinor-originalApplied,realizedFxMinor:currentApplied-originalApplied};
}
