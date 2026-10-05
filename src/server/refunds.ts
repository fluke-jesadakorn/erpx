import 'server-only';
import {refundNotifications} from './refund-notifications';
import { z } from 'zod';
import { createHash } from 'node:crypto';
import type { WorkContext } from './authorization';
import type { SqlDatabase } from './runtime/types';
import { holdingResources } from './runtime';
import { requirePermission } from './iam-permissions';
import { hasPermission } from '@/lib/permissions';
import { samePerson } from '@/lib/iam';
import { ApiError } from './http';
import { assertion, executeCommand, replayCommand } from './commands';
import { prepareJournal } from './finance';
import { decimalToMinor, minorToDecimal } from '@/lib/finance-money';
import type { WorkItem } from '@/lib/work-item';
import { prepareAttachmentBinding } from './attachments/service';
import { snapshotMoney } from './company-money';

type Row = Record<string, string | number | null>;
const version = z.number().int().positive();
const reason = z.string().trim().min(3).max(1000);
export const refundApprovalSchema = z.object({expectedVersion:version,reason,beneficiaryName:z.string().trim().min(2).max(160),beneficiaryAccount:z.string().trim().min(3).max(160)});
export const refundInstructionSchema = z.object({expectedVersion:version,amount:z.string().regex(/^\d+(\.\d{1,2})?$/),plannedDate:z.iso.date(),bankAccount:z.string().trim().min(1).max(160),cashAccountId:z.string().min(1),fxGainAccountId:z.string().min(1).optional(),fxLossAccountId:z.string().min(1).optional()});
export const refundExportSchema = z.object({instructions:z.array(z.object({id:z.string().min(1),expectedVersion:version})).min(1).max(100)});
export const refundConfirmationSchema = z.object({expectedVersion:version,bankTransactionId:z.string().min(1),bankVersion:version,periodId:z.string().min(1),bankReference:z.string().trim().min(2).max(160),evidence:reason,attachmentSetIds:z.array(z.string().min(1)).max(20).default([])});
export const refundExceptionSchema = z.object({expectedVersion:version,outcome:z.enum(['unknown','failed','cancelled']),evidence:reason});
function scope(actor:WorkContext,alias='o') {
 return {sql:`${alias}.company_id=? AND (?::text IS NULL OR ${alias}.branch_id=?) AND (?::text IS NULL OR ${alias}.department_id=?)`,args:[actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId]};
}
function treasury(actor:WorkContext) {
 requirePermission(actor,'finance.pay');
 if(actor.branchId||actor.departmentId)throw new ApiError(403,'COMPANY_TREASURY_REQUIRED','Use a Company treasury assignment to access bank evidence.');
}
async function obligation(db:SqlDatabase,actor:WorkContext,id:string) {
 requirePermission(actor,'finance.read');const s=scope(actor);
 const row=await db.prepare(`SELECT o.*,COALESCE(r.requested_by,c.prepared_by) return_requested_by,COALESCE(r.return_date,c.entry_date) return_date,r.order_id,COALESCE(r.number,c.number) return_number,p.name party_name FROM finance_refund_obligations o LEFT JOIN commerce_returns r ON r.id=o.return_id LEFT JOIN finance_billing_corrections c ON c.id=o.billing_correction_id AND c.company_id=o.company_id JOIN finance_parties p ON p.id=o.party_id WHERE o.id=? AND ${s.sql}`).bind(id,...s.args).first<Row>();
 if(!row)throw new ApiError(404,'REFUND_NOT_FOUND','Refund obligation not found.');return row;
}
async function instruction(db:SqlDatabase,actor:WorkContext,id:string) {
 const row=await db.prepare('SELECT * FROM finance_refund_instructions WHERE id=? AND company_id=?').bind(id,actor.companyId).first<Row>();
 if(!row)throw new ApiError(404,'REFUND_INSTRUCTION_NOT_FOUND','Refund instruction not found.');
 return {row,owner:await obligation(db,actor,String(row.obligation_id))};
}
export async function getRefund(actor:WorkContext,id:string) {const {write}=await holdingResources(actor.holdingId);return obligation(write,actor,id);}
function checkVersion(row:Row,expected:number) {
 if(Number(row.version)!==expected)throw new ApiError(409,'VERSION_CONFLICT','The record changed. Refresh before continuing.');
}
function payer(actor:WorkContext,row:Row) {
 treasury(actor);
 if(samePerson(actor,row.approved_by)||samePerson(actor,row.created_by))throw new ApiError(403,'INDEPENDENT_PAYER_REQUIRED','A different treasury officer must execute the approved obligation.');
}
function proportionalCarrying(base:number,outstanding:number,amount:number) {
 if(!Number.isSafeInteger(base)||base<0||!Number.isSafeInteger(outstanding)||outstanding<=0||amount>outstanding)throw new ApiError(409,'REFUND_CARRYING_EVIDENCE_REQUIRED','Reconcile the refund source carrying value before confirming cash.');
 if(amount===outstanding)return base;
 return Number((BigInt(base)*BigInt(amount)+BigInt(Math.floor(outstanding/2)))/BigInt(outstanding));
}
async function fxAccounts(db:SqlDatabase,actor:WorkContext,chartId:string,gainId:string|undefined,lossId:string|undefined) {
 const chart=await db.prepare("SELECT base_currency FROM finance_charts WHERE id=? AND company_id=? AND status='active'").bind(chartId,actor.companyId).first<{base_currency:string}>();
 if(!chart)throw new ApiError(404,'CHART_NOT_FOUND','Active refund chart not found.');
 const expected=[...(gainId?[gainId]:[]),...(lossId?[lossId]:[])];
 if(expected.length){const rows=await db.prepare("SELECT id FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND currency=? AND ((id=? AND category='other_income') OR (id=? AND category='other_expense'))").bind(actor.companyId,chartId,chart.base_currency,gainId??'',lossId??'').all();
  if(rows.results.length!==new Set(expected).size)throw new ApiError(422,'FX_ACCOUNT_INVALID','Refund FX gain and loss accounts must use the functional currency and matching category.');
 }
 return chart;
}
export async function listRefunds(actor:WorkContext,forWork=false) {
 requirePermission(actor,'finance.read');const {write}=await holdingResources(actor.holdingId),s=scope(actor);
 return (await write.prepare(`SELECT o.*,COALESCE(r.number,c.number) return_number,p.name party_name,r.order_id,COALESCE(r.requested_by,c.prepared_by) return_requested_by,(SELECT COUNT(*) FROM finance_refund_instructions i WHERE i.obligation_id=o.id AND i.status='unknown') uncertain_count,(SELECT COUNT(*) FROM finance_refund_instructions i WHERE i.obligation_id=o.id AND i.status IN ('exported','unknown') AND i.prepared_by<>?) reviewable_exceptions FROM finance_refund_obligations o LEFT JOIN commerce_returns r ON r.id=o.return_id LEFT JOIN finance_billing_corrections c ON c.id=o.billing_correction_id AND c.company_id=o.company_id JOIN finance_parties p ON p.id=o.party_id WHERE ${s.sql} ORDER BY o.updated_at DESC${forWork?'':' LIMIT 200'}`).bind(actor.userId,...s.args).all<Row>()).results;
}
export async function refundWorkspace(actor:WorkContext,id:string) {
 const {write}=await holdingResources(actor.holdingId),owner=await obligation(write,actor,id),bankAccess=hasPermission(actor,'finance.pay')&&!actor.branchId&&!actor.departmentId;
 const [instructions,accounts,periods,bank,fx]=await Promise.all([
  write.prepare('SELECT * FROM finance_refund_instructions WHERE obligation_id=? ORDER BY created_at,id').bind(id).all<Row>(),
  write.prepare("SELECT id,code,name FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND category='asset' AND id<>? AND (id=(SELECT cash_account_id FROM commerce_settings WHERE company_id=finance_accounts.company_id) OR (?::text IS NOT NULL AND control_kind='bank')) ORDER BY code").bind(actor.companyId,owner.chart_id,owner.balance_account_id,owner.billing_correction_id??null).all<Row>(),
  write.prepare("SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date").bind(actor.companyId).all<Row>(),
  bankAccess?write.prepare("SELECT t.*,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.company_id=? AND t.currency=? AND t.match_status IN ('unmatched','proposed') ORDER BY t.transaction_date DESC LIMIT 200").bind(actor.companyId,owner.currency).all<Row>():Promise.resolve({results:[] as Row[]}),
  write.prepare("SELECT a.id,a.code,a.name,a.category FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.chart_id=? AND a.status='active' AND a.currency=c.base_currency AND a.category IN ('other_income','other_expense') ORDER BY a.code").bind(actor.companyId,owner.chart_id).all<Row>(),
 ]);
 return {owner,instructions:instructions.results,accounts:accounts.results,periods:periods.results,bank:bank.results,fxAccounts:fx.results,
  canApprove:hasPermission(actor,'finance.approve')&&!samePerson(actor,owner.created_by)&&!samePerson(actor,owner.return_requested_by),
  canPay:bankAccess&&!samePerson(actor,owner.created_by)&&!samePerson(actor,owner.approved_by),canReviewException:hasPermission(actor,'finance.approve'),userId:actor.userId};
}
export async function approveRefund(actor:WorkContext,id:string,input:z.infer<typeof refundApprovalSchema>,key:string) {
 input=refundApprovalSchema.parse(input);requirePermission(actor,'finance.approve');const {write}=await holdingResources(actor.holdingId);
 const replay=await replayCommand(write,actor,key,'finance.refund.approve',id,input);if(replay)return replay;
 const row=await obligation(write,actor,id);checkVersion(row,input.expectedVersion);
 if(samePerson(actor,row.created_by)||samePerson(actor,row.return_requested_by))throw new ApiError(403,'INDEPENDENT_APPROVER_REQUIRED','The refund requires independent review of its source, beneficiary and amount.');
 const now=Date.now();return executeCommand(write,actor,key,'finance.refund.approve',input,id,input.expectedVersion+1,{id,status:'approved',version:input.expectedVersion+1},[
  assertion(write,key,'obligation',"EXISTS(SELECT 1 FROM finance_refund_obligations WHERE id=? AND company_id=? AND status='pending' AND version=?)",[id,actor.companyId,input.expectedVersion]),
  write.prepare("UPDATE finance_refund_obligations SET status='approved',beneficiary_name=?,beneficiary_account=?,approval_reason=?,approved_by=?,approved_at=?,version=version+1,updated_at=? WHERE id=?").bind(input.beneficiaryName,input.beneficiaryAccount,input.reason,actor.userId,now,now,id),
  refundNotifications(write,key,id,'treasury','Refund approved: prepare and verify bank instructions',actor.userId),
 ]);
}
export async function prepareRefundInstruction(actor:WorkContext,id:string,input:z.infer<typeof refundInstructionSchema>,key:string) {
 input=refundInstructionSchema.parse(input);treasury(actor);const {write}=await holdingResources(actor.holdingId),instructionId=`refund-instruction-${key}`,commandInput={obligationId:id,...input};
 const replay=await replayCommand(write,actor,key,'finance.refund.prepare',instructionId,commandInput);if(replay)return replay;
 const row=await obligation(write,actor,id);payer(actor,row);checkVersion(row,input.expectedVersion);
 if(input.bankAccount!==input.cashAccountId)throw new ApiError(422,'BANK_LEDGER_MISMATCH','Select the same bank ledger account used by the imported statement.');
 const amount=decimalToMinor(input.amount);if(amount<=0||input.plannedDate<String(row.return_date))throw new ApiError(422,'INVALID_REFUND_AMOUNT_DATE','Use a positive amount and a date on or after the return.');
 await fxAccounts(write,actor,String(row.chart_id),input.fxGainAccountId,input.fxLossAccountId);
 const snapshot=await snapshotMoney(write,actor,{sourceType:'finance_refund_instruction',sourceId:instructionId,amountKind:'prepared',transactionDate:input.plannedDate,currency:String(row.currency),currencyScale:2,amountUnits:String(amount)});
 const now=Date.now();return executeCommand(write,actor,key,'finance.refund.prepare',commandInput,instructionId,1,{id:instructionId,obligationId:id,status:'prepared',version:1},[
  assertion(write,key,'available',"EXISTS(SELECT 1 FROM finance_refund_obligations o WHERE o.id=? AND o.company_id=? AND o.status='approved' AND o.version=? AND o.outstanding_minor-COALESCE((SELECT SUM(i.amount_minor) FROM finance_refund_instructions i WHERE i.obligation_id=o.id AND i.status IN ('prepared','exported','unknown')),0)>=?)",[id,actor.companyId,input.expectedVersion,amount]),
  assertion(write,key,'cash-account',"EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND category='asset' AND status='active' AND id<>? AND (id=(SELECT cash_account_id FROM commerce_settings WHERE company_id=finance_accounts.company_id) OR (?::text IS NOT NULL AND control_kind='bank')))",[input.cashAccountId,actor.companyId,row.chart_id,row.balance_account_id,row.billing_correction_id??null]),
  write.prepare('INSERT INTO finance_refund_instructions(id,obligation_id,company_id,amount_minor,planned_date,bank_account,cash_account_id,fx_gain_account_id,fx_loss_account_id,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)').bind(instructionId,id,actor.companyId,amount,input.plannedDate,input.bankAccount,input.cashAccountId,input.fxGainAccountId??null,input.fxLossAccountId??null,actor.userId,now,now),
  snapshot.statement,
  write.prepare('UPDATE finance_refund_obligations SET version=version+1,updated_at=? WHERE id=?').bind(now,id),
 ]);
}
const csvCell=(value:unknown)=>`"${String(value??'').replace(/^[=+@\-\t\r]/,"'$&").replaceAll('"','""')}"`;
/** Immutable export receipt; exporting never moves cash or marks an obligation paid. */
export async function exportRefundInstructions(actor:WorkContext,input:z.infer<typeof refundExportSchema>,key:string) {
 input=refundExportSchema.parse(input);treasury(actor);const {write}=await holdingResources(actor.holdingId);
 const replay=await replayCommand(write,actor,key,'finance.refund.export',key,input);if(replay)return replay;
 if(new Set(input.instructions.map(i=>i.id)).size!==input.instructions.length)throw new ApiError(422,'DUPLICATE_INSTRUCTION','Select each instruction once.');
 const rows=[];
 for(const item of input.instructions){const {row,owner}=await instruction(write,actor,item.id);payer(actor,owner);checkVersion(row,item.expectedVersion);if(row.status!=='prepared'||owner.status!=='approved')throw new ApiError(409,'REFUND_NOT_PREPARED','Only prepared instructions for approved obligations can be exported.');rows.push({row,owner});}
 const content=[['instruction_id','source_return','direction','date','currency','amount','company_bank_account','counterparty','counterparty_account'].map(csvCell).join(','),...rows.map(({row,owner})=>[row.id,owner.return_number,owner.direction,row.planned_date,owner.currency,minorToDecimal(Number(row.amount_minor)),row.bank_account,owner.beneficiary_name,owner.beneficiary_account].map(csvCell).join(','))].join('\r\n');
 const sha256=createHash('sha256').update(content,'utf8').digest('hex'),now=Date.now();
 return executeCommand(write,actor,key,'finance.refund.export',input,key,1,{id:key,format:'csv',content,sha256,rows:rows.length,confirmationRequired:true},rows.flatMap(({row})=>[
  assertion(write,key,`instruction-${row.id}`,"EXISTS(SELECT 1 FROM finance_refund_instructions WHERE id=? AND company_id=? AND status='prepared' AND version=?)",[row.id,actor.companyId,row.version]),
  write.prepare("UPDATE finance_refund_instructions SET status='exported',export_hash=?,export_command_id=?,exported_at=?,version=version+1,updated_at=? WHERE id=?").bind(sha256,key,now,now,row.id),
 ]));
}
export async function recordRefundException(actor:WorkContext,id:string,input:z.infer<typeof refundExceptionSchema>,key:string) {
 input=refundExceptionSchema.parse(input);requirePermission(actor,'finance.read');const {write}=await holdingResources(actor.holdingId);
 const replay=await replayCommand(write,actor,key,'finance.refund.exception',id,input);if(replay)return replay;
 const {row,owner}=await instruction(write,actor,id);checkVersion(row,input.expectedVersion);
 if(input.outcome==='cancelled'){
  payer(actor,owner);if(row.status!=='prepared')throw new ApiError(409,'EXPORTED_REFUND_REQUIRES_REVIEW','An exported instruction cannot be cancelled without reviewing the bank result.');
 }else if(input.outcome==='unknown'){
  payer(actor,owner);if(row.status!=='exported')throw new ApiError(409,'REFUND_NOT_EXPORTED','Only exported instructions can have an uncertain bank result.');
 }else{
  requirePermission(actor,'finance.approve');
  if(samePerson(actor,row.prepared_by))throw new ApiError(403,'INDEPENDENT_FAILURE_REVIEW_REQUIRED','A different finance approver must verify definitive bank failure.');
  if(!['exported','unknown'].includes(String(row.status)))throw new ApiError(409,'REFUND_NOT_EXPORTED','Review an exported or uncertain instruction.');
 }
 const now=Date.now();return executeCommand(write,actor,key,'finance.refund.exception',input,id,input.expectedVersion+1,{id,status:input.outcome,version:input.expectedVersion+1},[
  assertion(write,key,'instruction','EXISTS(SELECT 1 FROM finance_refund_instructions WHERE id=? AND company_id=? AND version=? AND status=?)',[id,actor.companyId,input.expectedVersion,row.status]),
  write.prepare('UPDATE finance_refund_instructions SET status=?,evidence=?,resolved_by=?,version=version+1,updated_at=? WHERE id=?').bind(input.outcome,input.evidence,actor.userId,now,id),
  write.prepare('UPDATE finance_refund_obligations SET version=version+1,updated_at=? WHERE id=?').bind(now,owner.id),
  refundNotifications(write,key,String(owner.id),input.outcome==='unknown'?'review':'treasury','Refund bank exception updated: review source evidence before retrying',actor.userId),
 ]);
}
export async function confirmRefundInstruction(actor:WorkContext,id:string,input:z.infer<typeof refundConfirmationSchema>,key:string) {
 input=refundConfirmationSchema.parse(input);treasury(actor);const {write}=await holdingResources(actor.holdingId);
 const replay=await replayCommand(write,actor,key,'finance.refund.confirm',id,input);if(replay)return replay;
 const {row,owner}=await instruction(write,actor,id);payer(actor,owner);checkVersion(row,input.expectedVersion);
 if(!['exported','unknown'].includes(String(row.status))||owner.status!=='approved')throw new ApiError(409,'REFUND_NOT_EXPORTED','Confirm an approved exported instruction using bank evidence.');
 const bank=await write.prepare('SELECT t.*,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=?').bind(input.bankTransactionId,actor.companyId).first<Row>();
 const outgoing=owner.direction==='outgoing',amount=Number(row.amount_minor);
 if(!bank||bank.currency!==owner.currency||bank.bank_account!==row.bank_account||Number(bank.amount_minor)!==(outgoing?-amount:amount)||String(bank.transaction_date)<String(row.planned_date))throw new ApiError(422,'BANK_EVIDENCE_MISMATCH','Bank account, currency, direction, amount and payment date must match the instruction.');
 let attachmentBinding:null|Awaited<ReturnType<typeof prepareAttachmentBinding>>=null;
 if(input.attachmentSetIds.length){const placeholders=input.attachmentSetIds.map(()=>'?').join(','),sets=(await write.prepare(`SELECT id,purpose_key,component_kind,component_ref FROM attachment_sets WHERE company_id=? AND id IN (${placeholders})`).bind(actor.companyId,...input.attachmentSetIds).all<{id:string;purpose_key:string;component_kind:string;component_ref:string|null}>()).results;
  if(sets.length!==new Set(input.attachmentSetIds).size||sets.some(set=>set.purpose_key!=='refund_proof'||set.component_kind!=='action'||set.component_ref!==`refund-instruction:${id}`))throw new ApiError(422,'REFUND_ATTACHMENT_PURPOSE','Choose only refund proof prepared for this bank confirmation.');
  attachmentBinding=await prepareAttachmentBinding(write,actor,{attachmentSetIds:input.attachmentSetIds,bindingContract:{targetType:'finance_refund_obligation',bindingType:'finance_refund_confirmation',contexts:[{purposeKey:'refund_proof',componentKind:'action',componentRef:`refund-instruction:${id}`}]},targetId:String(owner.id),expectedTargetVersion:Number(owner.version),commandId:key,bindingId:id});
 }
 const journal=await prepareJournal(write,actor,{chartId:String(owner.chart_id),periodId:input.periodId,entryDate:String(bank.transaction_date),currency:String(owner.currency),memo:`Refund ${owner.return_number} · ${input.bankReference}`,sourceType:'return_refund',sourceId:id,isOpening:false,lines:[
  {accountId:String(owner.balance_account_id),debit:outgoing?minorToDecimal(amount):'0',credit:outgoing?'0':minorToDecimal(amount)},
  {accountId:String(row.cash_account_id),debit:outgoing?'0':minorToDecimal(amount),credit:outgoing?minorToDecimal(amount):'0'},
 ]},key,'posted');
 const chart=await fxAccounts(write,actor,String(owner.chart_id),row.fx_gain_account_id?String(row.fx_gain_account_id):undefined,row.fx_loss_account_id?String(row.fx_loss_account_id):undefined);
 // Legacy functional-currency obligations keep their original one-to-one
 // meaning. Foreign obligations require the owner's retained credit evidence.
 const sourceBase=owner.base_outstanding_minor===null||owner.base_outstanding_minor===undefined
  ?owner.currency===chart.base_currency?Number(owner.outstanding_minor):null:Number(owner.base_outstanding_minor);
 if(sourceBase===null)throw new ApiError(409,'REFUND_CARRYING_EVIDENCE_REQUIRED','Reconcile the foreign refund source carrying value before confirming cash.');
 const carrying=proportionalCarrying(sourceBase,Number(owner.outstanding_minor),amount),cashBase=Number((BigInt(amount)*BigInt(journal.result.fxRateMicros)+BigInt(500000))/BigInt(1000000));
 if(!Number.isSafeInteger(cashBase))throw new ApiError(422,'MONEY_OVERFLOW','Refund cash exceeds supported precision.');
 const difference=cashBase-carrying,gain=(outgoing?difference<0:difference>0)?Math.abs(difference):0,loss=(outgoing?difference>0:difference<0)?Math.abs(difference):0;
 const fxAccount=gain?row.fx_gain_account_id:loss?row.fx_loss_account_id:null;
 if((gain||loss)&&!fxAccount)throw new ApiError(422,gain?'FX_GAIN_ACCOUNT_REQUIRED':'FX_LOSS_ACCOUNT_REQUIRED','Prepare the refund instruction with the required functional-currency realized FX account.');
 const controlDebit=outgoing?difference<0:difference>0;
 const fxJournal=difference?await prepareJournal(write,actor,{chartId:String(owner.chart_id),periodId:input.periodId,entryDate:String(bank.transaction_date),currency:chart.base_currency,memo:`Realized FX for refund ${owner.return_number}`,sourceType:'return_refund_fx',sourceId:id,isOpening:false,lines:[
  {accountId:String(owner.balance_account_id),debit:controlDebit?minorToDecimal(Math.abs(difference)):'0',credit:controlDebit?'0':minorToDecimal(Math.abs(difference))},
  {accountId:String(fxAccount),debit:loss?minorToDecimal(loss):'0',credit:gain?minorToDecimal(gain):'0'},
 ]},`${key}-fx`,'posted'):null;
 const paymentId=`refund-payment-${key}`,now=Date.now();
 const snapshot=await snapshotMoney(write,actor,{sourceType:'finance_payment',sourceId:paymentId,amountKind:'settled',transactionDate:String(bank.transaction_date),currency:String(owner.currency),currencyScale:2,amountUnits:String(amount)});
 return executeCommand(write,actor,key,'finance.refund.confirm',input,id,input.expectedVersion+1,{id,status:'confirmed',version:input.expectedVersion+1,paymentId,journalId:journal.result.id,fxJournalId:fxJournal?.result.id??null,baseCarryingMinor:carrying,cashBaseMinor:cashBase,realizedFxMinor:gain-loss,bankTransactionId:input.bankTransactionId},[
  ...(attachmentBinding?.guards??[]),
  write.prepare('SELECT id FROM finance_refund_obligations WHERE id=? AND company_id=? FOR UPDATE').bind(owner.id,actor.companyId),
  assertion(write,key,'instruction',"EXISTS(SELECT 1 FROM finance_refund_instructions WHERE id=? AND company_id=? AND version=? AND status IN ('exported','unknown'))",[id,actor.companyId,input.expectedVersion]),
  assertion(write,key,'obligation',"EXISTS(SELECT 1 FROM finance_refund_obligations WHERE id=? AND company_id=? AND version=? AND status='approved' AND outstanding_minor>=?)",[owner.id,actor.companyId,owner.version,amount]),
  ...(owner.currency!==chart.base_currency?[assertion(write,key,'source-credit',"EXISTS(SELECT 1 FROM finance_refund_obligations o JOIN commerce_returns r ON r.id=o.return_id JOIN finance_journals j ON j.id=o.source_credit_journal_id AND j.id=r.credit_journal_id AND j.company_id=o.company_id AND j.chart_id=o.chart_id WHERE o.id=? AND o.base_outstanding_minor=? AND j.status IN ('posted','reversed'))",[owner.id,sourceBase])]:[]),
  assertion(write,key,'bank',"EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=? AND t.version=? AND t.match_status IN ('unmatched','proposed') AND t.amount_minor=? AND t.currency=? AND s.bank_account=?)",[input.bankTransactionId,actor.companyId,input.bankVersion,outgoing?-amount:amount,owner.currency,row.bank_account]),
  ...journal.steps,
  ...(fxJournal?.steps??[]),
  write.prepare("INSERT INTO finance_payments(id,company_id,party_id,kind,payment_number,payment_date,currency,amount_minor,outstanding_minor,bank_account,status,journal_id,fx_rate_micros,base_amount_minor,fx_journal_id,created_by,approved_by,released_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,0,?,'settled',?,?,?,?,?,?,?,?,?)").bind(paymentId,actor.companyId,owner.party_id,outgoing?'refund':'receipt',`RF-${key}`,bank.transaction_date,owner.currency,amount,row.bank_account,journal.result.id,journal.result.fxRateMicros,cashBase,fxJournal?.result.id??null,actor.userId,owner.approved_by,actor.userId,now,now),
  snapshot.statement,
  write.prepare("UPDATE finance_bank_transactions SET match_status='matched',matched_type='payment',matched_id=?,version=version+1 WHERE id=?").bind(paymentId,input.bankTransactionId),
  write.prepare("UPDATE finance_refund_instructions SET status='confirmed',bank_reference=?,evidence=?,payment_id=?,bank_transaction_id=?,resolved_by=?,base_carrying_minor=?,cash_base_minor=?,fx_journal_id=?,version=version+1,updated_at=? WHERE id=?").bind(input.bankReference,input.evidence,paymentId,input.bankTransactionId,actor.userId,carrying,cashBase,fxJournal?.result.id??null,now,id),
  write.prepare("UPDATE finance_refund_obligations SET outstanding_minor=outstanding_minor-?,base_outstanding_minor=COALESCE(base_outstanding_minor,?)-?,status=CASE WHEN outstanding_minor=? THEN 'settled' ELSE 'approved' END,version=version+1,updated_at=? WHERE id=?").bind(amount,sourceBase,carrying,amount,now,owner.id),
  ...(attachmentBinding?.steps??[]),
  refundNotifications(write,key,String(owner.id),'completed','Refund bank evidence matched: review the remaining obligation',actor.userId),
 ]);
}
export async function refundWork(actor:WorkContext):Promise<WorkItem[]> {
 if(!hasPermission(actor,'finance.read'))return [];
 const rows=await listRefunds(actor,true);
 return rows.map(row=>{
  const approve=row.status==='pending'&&hasPermission(actor,'finance.approve')&&!samePerson(actor,row.created_by)&&!samePerson(actor,row.return_requested_by);
  const pay=row.status==='approved'&&hasPermission(actor,'finance.pay')&&!actor.branchId&&!actor.departmentId&&!samePerson(actor,row.approved_by)&&!samePerson(actor,row.created_by);
  const reviewException=Number(row.reviewable_exceptions)>0&&hasPermission(actor,'finance.approve');
  const canAct=approve||pay||reviewException;
  return {id:String(row.id),type:'refund',actionKind:approve||reviewException?'approval':'work',title:`Refund · ${row.return_number}`,detail:`${row.party_name} · ${row.currency} ${minorToDecimal(Number(row.outstanding_minor))} remaining${Number(row.uncertain_count)>0?' · Bank result uncertain':''}`,href:`/finance/refunds/${row.id}`,status:String(row.status),updatedAt:Number(row.updated_at),queue:row.status==='settled'?'completed':canAct?'todo':'waiting',canAct,responsibleQueue:row.status==='pending'||reviewException?'Independent Finance approver':'Company Treasury',nextAction:row.status==='pending'?'Review source, amount and beneficiary':reviewException?'Review bank result before releasing a failed instruction':'Review export and bank confirmation',relationships:[canAct?'approval':'team']};
 });
}

export async function recoverRefundExport(actor:WorkContext,id:string) {
 treasury(actor);const {write}=await holdingResources(actor.holdingId),{row,owner}=await instruction(write,actor,id);payer(actor,owner);
 const receipt=await write.prepare("SELECT result_json FROM command_receipts WHERE command_id=? AND operation='finance.refund.export'").bind(row.export_command_id).first<{result_json:string}>();
 if(!receipt)throw new ApiError(404,'REFUND_EXPORT_NOT_FOUND','No export receipt exists for this instruction.');
 return JSON.parse(receipt.result_json) as {content:string;sha256:string};
}
