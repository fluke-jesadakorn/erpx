import 'server-only';
import {createHash} from 'node:crypto';
import {samePerson} from '@erpx/feature-identity_access/lib/iam';
import {financeCsvRows} from '@erpx/feature-finance/lib/finance-csv';
import {minorToDecimal} from '@erpx/feature-finance/lib/finance-money';
import {financeOwnedSnapshotSources as commerceSnapshotSources} from '@erpx/server-commerce/public/finance-owned-snapshot-sources';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {lockQueuedCommandAuthority,requireQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {assertProductionModuleEnabled} from '@erpx/server-pms/public/hospitality-rollout';
import {companyPurchaseReadLocks,financePackageEnforcementMode,checkPurchasedCapability} from '@erpx/server-commercial/public/purchase-command-boundary';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {attachmentPrivateReadSourceLocks} from '@erpx/server-files/public/private-attachment-reads';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {isTransactionalSqlDatabase,type SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {replayCommand} from '@erpx/server-workflow/public/commands';
import {financeSourceDocumentReadLeases,requireFinanceDocumentHistory} from '@erpx/server-finance/adapters/document-purchases';
import {withRefundPurchasedCommand} from '@erpx/server-finance/adapters/refund-purchases';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/adapters/finance-transactions';
import {csvCell,createRefundPolicy,type Row} from '@erpx/server-finance/domain/refunds';
import {validateRefundBankEvidenceInput,type RefundBankEvidenceInput,type RefundBankEvidenceFacts} from '@erpx/server-finance/domain/refund-bank-evidence';
import type {BankImportInput,FinanceBankPorts} from '@erpx/server-finance/application/bank';

const digest=(value:unknown)=>createHash('sha256').update(JSON.stringify(value)).digest('hex');
const changed=()=>new ApiError(409,'REFUND_BANK_EVIDENCE_SOURCE_CHANGED','The original refund instruction or its exact bank evidence changed. Review the original owner workflow.');
const markerName='__refundInstructionBankEvidence';
type ExportReceipt={command_id:string;operation:string;actor_id:string;request_hash:string;record_id:string;result_json:string;authority_json:string;created_at:number};
type Source={row:Row;owner:Row;account:Row;chart:Row;exportReceipt:ExportReceipt;fingerprint:string;facts:RefundBankEvidenceFacts};
/** This witness is constructed only by the named native instruction import.
 * It is never parsed from transport data or accepted by generic Bank input. */
export type RefundBankEvidenceWitness={instructionId:string;obligationId:string;expectedVersion:number;fingerprint:string};
export type RefundBankEvidenceResult={id:string;imported:1;status:'imported';refundInstructionId:string;refundObligationId:string;bankTransactionId:string};
type Marker={format:'refund-instruction-bank-evidence/v1';companyId:string;actorId:string;instructionId:string;obligationId:string;sourceFingerprint:string;bankFingerprint:string};
const record=(value:unknown):value is Record<string,unknown>=>!!value&&typeof value==='object'&&!Array.isArray(value);
function parsed(value:string):Record<string,unknown>|null{try{const result:unknown=JSON.parse(value);return record(result)?result:null;}catch{return null;}}
const businessInstruction=(i:Row)=>[i.id,i.obligation_id,i.company_id,i.amount_minor,i.planned_date,i.bank_account,i.cash_account_id,i.fx_gain_account_id,i.fx_loss_account_id,i.prepared_by,i.created_at,i.export_hash,i.export_command_id,i.exported_at];
const businessOwner=(o:Row)=>[o.id,o.company_id,o.branch_id,o.department_id,o.return_id,o.billing_correction_id,o.invoice_id,o.source_credit_journal_id,o.party_id,o.currency,o.chart_id,o.balance_account_id,o.amount_minor,o.base_amount_minor,o.created_by,o.created_at,o.approved_by,o.approved_at,o.beneficiary_name,o.beneficiary_account,o.approval_reason,o.direction];
const csvHeaders=['instruction_id','source_return','direction','date','currency','amount','company_bank_account','counterparty','counterparty_account'];
async function source(database:SqlDatabase,actor:WorkContext,id:string):Promise<Source>{
 const row=await database.prepare('SELECT * FROM finance_refund_instructions WHERE id=? AND company_id=?').bind(id,actor.companyId).first<Row>();
 if(!row)throw new ApiError(404,'REFUND_INSTRUCTION_NOT_FOUND','The actual refund instruction is unavailable in this Company.');
 const owner=await database.prepare(`SELECT o.*,COALESCE(r.number,c.number) return_number FROM finance_refund_obligations o LEFT ${commerceSnapshotSources().refunds_commerce_returns_r_join} ON r.id=o.return_id AND r.company_id=o.company_id LEFT JOIN finance_billing_corrections c ON c.id=o.billing_correction_id AND c.company_id=o.company_id WHERE o.id=? AND o.company_id=?`).bind(row.obligation_id,actor.companyId).first<Row>();
 if(!owner)throw new ApiError(404,'REFUND_NOT_FOUND','The original refund obligation is unavailable.');
 const [account,chart,receipt]=await Promise.all([
  database.prepare('SELECT * FROM finance_accounts WHERE id=? AND company_id=?').bind(row.cash_account_id,actor.companyId).first<Row>(),
  database.prepare('SELECT * FROM finance_charts WHERE id=? AND company_id=?').bind(owner.chart_id,actor.companyId).first<Row>(),
  database.prepare("SELECT command_id,operation,actor_id,request_hash,record_id,result_json,authority_json,created_at FROM command_receipts WHERE command_id=? AND operation='finance.refund.export' AND authority_json::jsonb->>'companyId'=?").bind(row.export_command_id,actor.companyId).first<ExportReceipt>(),
 ]);
 if(!account||!chart||!receipt||receipt.record_id!==row.export_command_id||row.bank_account!==row.cash_account_id||account.chart_id!==owner.chart_id||account.currency!==owner.currency)throw changed();
 const exported=parsed(receipt.result_json),authority=parsed(receipt.authority_json);
 if(!exported||!authority||authority.companyId!==actor.companyId||authority.branchId!==null||authority.departmentId!==null||typeof exported.content!=='string'||exported.id!==receipt.command_id||exported.format!=='csv'||exported.confirmationRequired!==true||exported.sha256!==row.export_hash||createHash('sha256').update(exported.content).digest('hex')!==row.export_hash)throw changed();
 // Parse the retained original CSV (including quoted CR/LF); never infer
 // membership from a free-form label or matching amount alone.
 let rows:Record<string,string>[];try{rows=financeCsvRows(exported.content);}catch{throw changed();}
 const selected=rows.filter(item=>item.instruction_id===String(row.id));
 const values=[row.id,owner.return_number,owner.direction,row.planned_date,owner.currency,minorToDecimal(Number(row.amount_minor)),row.bank_account,owner.beneficiary_name,owner.beneficiary_account];
 const escaped=financeCsvRows(`${csvHeaders.map(csvCell).join(',')}\r\n${values.map(csvCell).join(',')}`)[0];
 if(selected.length!==1||exported.rows!==rows.length||!Number.isSafeInteger(rows.length)||rows.some(item=>Object.keys(item).join('|')!==csvHeaders.join('|'))||csvHeaders.some(key=>selected[0][key]!==escaped[key]))throw changed();
 const fingerprint=digest([businessInstruction(row),businessOwner(owner),receipt]);
 return {row,owner,account,chart,exportReceipt:receipt,fingerprint,facts:{instructionId:id,obligationId:String(owner.id),version:Number(row.version),instructionStatus:String(row.status),obligationStatus:String(owner.status),currency:String(owner.currency),direction:owner.direction==='outgoing'?'outgoing':'incoming',amountMinor:Number(row.amount_minor),cashAccountId:String(row.cash_account_id),bankAccount:String(row.bank_account),plannedDate:String(row.planned_date)}};
}
export function refundBankEvidenceCommandInput(input:BankImportInput,witness:RefundBankEvidenceWitness){return {...input,refundInstructionId:witness.instructionId,refundInstructionVersion:witness.expectedVersion,refundInstructionSource:witness.fingerprint};}
const statementImage=(s:Row)=>[s.id,s.company_id,s.bank_account,s.statement_number,s.statement_date,Number(s.opening_minor),Number(s.closing_minor),Number(s.closing_fx_rate_micros),Number(s.base_closing_minor),s.source_hash,s.imported_by,Number(s.created_at)];
const rowImage=(r:Row)=>[r.id,r.statement_id,r.company_id,r.transaction_date,r.external_id,r.description,Number(r.amount_minor),r.currency];
/** Only the private bankService closure calls this while constructing its
 * original finance.bank.import receipt, using actual prepared native values. */
export function refundBankEvidenceMarker(facts:Parameters<FinanceBankPorts['commitImport']>[0],witness:RefundBankEvidenceWitness):Marker{
 const {actor,input,key,id,opening,closing,amounts,closingFxRate,baseClosing,digest:hash,now}=facts,r=input.rows[0];
 const statement:Row={id,company_id:actor.companyId,bank_account:input.bankAccount,statement_number:input.statementNumber,statement_date:input.statementDate,opening_minor:opening,closing_minor:closing,closing_fx_rate_micros:closingFxRate,base_closing_minor:baseClosing,source_hash:hash,imported_by:actor.userId,created_at:now},row:Row={id:`bankrow-${key}-0`,statement_id:id,company_id:actor.companyId,transaction_date:r.transactionDate,external_id:r.externalId,description:r.description??'',amount_minor:amounts[0],currency:r.currency.toUpperCase()};
 return {format:'refund-instruction-bank-evidence/v1',companyId:actor.companyId,actorId:actor.userId,instructionId:witness.instructionId,obligationId:witness.obligationId,sourceFingerprint:witness.fingerprint,bankFingerprint:digest([statementImage(statement),rowImage(row)])};
}
export function refundBankEvidenceStoredResult(result:Record<string,unknown>,marker:Marker){return {...result,[markerName]:marker};}
/** Strip even on a generic same-key replay; private witnesses stay server-only. */
export function publicBankImportResult(value:Record<string,unknown>){const result={...value};delete result[markerName];return result;}
function publicReceipt(value:Record<string,unknown>,key:string,id:string,obligationId:string):RefundBankEvidenceResult{
 if(value.id!==`statement-${key}`||value.imported!==1||value.status!=='imported'||value.refundInstructionId!==id||value.refundObligationId!==obligationId||value.bankTransactionId!==`bankrow-${key}-0`)throw changed();
 return {id:value.id,imported:1,status:'imported',refundInstructionId:id,refundObligationId:obligationId,bankTransactionId:value.bankTransactionId};
}
async function retained(database:SqlDatabase,actor:WorkContext,original:Source,bankId:string){
 const found=await database.prepare(`SELECT c.command_id,c.actor_id,c.record_id,c.result_json,c.authority_json,to_jsonb(s)::text statement,to_jsonb(t)::text movement FROM command_receipts c JOIN finance_bank_statements s ON s.id=c.record_id AND s.company_id=? AND s.imported_by=c.actor_id JOIN finance_bank_transactions t ON t.statement_id=s.id AND t.company_id=s.company_id WHERE c.operation='finance.bank.import' AND c.authority_json::jsonb->>'companyId'=s.company_id AND t.id=? AND c.result_json::jsonb->'${markerName}'->>'instructionId'=?`).bind(actor.companyId,bankId,original.row.id).first<{command_id:string;actor_id:string;record_id:string;result_json:string;authority_json:string;statement:string;movement:string}>();
 if(!found)return null;const result=parsed(found.result_json),s=parsed(found.statement),t=parsed(found.movement),authority=parsed(found.authority_json),marker=result?.[markerName];
 if(!result||!s||!t||!record(marker)||!authority||marker.format!=='refund-instruction-bank-evidence/v1'||marker.companyId!==actor.companyId||marker.actorId!==found.actor_id||marker.instructionId!==original.row.id||marker.obligationId!==original.owner.id||marker.sourceFingerprint!==original.fingerprint||authority.companyId!==actor.companyId||authority.branchId!==null||authority.departmentId!==null||found.record_id!==`statement-${found.command_id}`||t.id!==`bankrow-${found.command_id}-0`||result.refundInstructionId!==original.row.id||result.refundObligationId!==original.owner.id||result.bankTransactionId!==t.id||result.id!==s.id||result.imported!==1||result.status!=='imported'||marker.bankFingerprint!==digest([statementImage(s as Row),rowImage(t as Row)]))return null;
 const count=await database.prepare('SELECT COUNT(*) count FROM finance_bank_transactions WHERE statement_id=?').bind(s.id as string).first<number>('count');if(Number(count)!==1)return null;
 return {row:{...t,bank_account:s.bank_account} as Row,statement:s as Row,fingerprint:digest([marker,statementImage(s as Row),rowImage(t as Row)])};
}
async function referenceImage(database:SqlDatabase,actor:WorkContext){
 const tables=['company_money_settings','company_currency_scales','company_fx_rates','finance_book_rules'] as const;
 const rows=await Promise.all(tables.map(async table=>(await database.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r WHERE r.company_id=? ORDER BY to_jsonb(r)::text`).bind(actor.companyId).all<{image:string}>()).results)),mappings=(await database.prepare('SELECT to_jsonb(m)::text image FROM finance_book_account_mappings m JOIN finance_book_rules r ON r.id=m.rule_id WHERE r.company_id=? ORDER BY to_jsonb(m)::text').bind(actor.companyId).all<{image:string}>()).results;return digest([...rows,mappings]);
}
async function authority(database:SqlDatabase,actor:WorkContext,purpose:'read'|'pay'|'import',capability:string){
 requirePermission(actor,'finance.read');if(purpose!=='read'){requirePermission(actor,'finance.pay');if(purpose==='import')requirePermission(actor,'finance.write');if(actor.scope.kind!=='holding'||actor.branchId||actor.departmentId)throw new ApiError(403,'COMPANY_TREASURY_REQUIRED','Use the current Company Treasury assignment for this exact instruction.');}
 const current=await requireQueuedCommandAuthority(database,actor,purpose==='read'?'finance.read':'finance.pay');if(purpose==='import')requirePermission(current,'finance.write');
 await assertProductionModuleEnabled(database,actor.companyId,'finance');if(capability==='finance.sales-posting')await assertProductionModuleEnabled(database,actor.companyId,'sales');if(capability==='finance.inventory-valuation')await assertProductionModuleEnabled(database,actor.companyId,'procurement');
 return digest([current.userId,current.assignmentId,current.role,current.iam,current.authority??null]);
}
/** Source/writer leases are query-free, before any advisory or data snapshot.
 * Strong package fence excludes normal advisory-first writers at entry. */
async function fresh<Result>(actor:WorkContext,run:(database:SqlDatabase)=>Promise<Result>){
 const owner=(await holdingResources(actor.holdingId)).write;if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'TRANSACTION_REQUIRED','Instruction bank evidence requires its original fresh Holding transaction.');
 return owner.transaction(async database=>{
  await lockPackageSettlementSources(database);
  await database.batch([database.prepare('LOCK TABLE command_receipts IN SHARE ROW EXCLUSIVE MODE'),
   ...financeSourceDocumentReadLeases(database,actor.companyId,false,undefined,true),
   database.prepare('LOCK TABLE finance_parties,finance_charts,finance_billing_corrections IN SHARE MODE'),
   database.prepare('LOCK TABLE finance_accounts IN SHARE ROW EXCLUSIVE MODE'),
   database.prepare('LOCK TABLE finance_bank_statements,finance_bank_transactions,finance_refund_obligations,finance_payments,finance_journals,finance_journal_lines,finance_financial_events,finance_book_postings,money_transaction_snapshots IN SHARE ROW EXCLUSIVE MODE'),
   database.prepare('LOCK TABLE finance_book_rules,finance_book_account_mappings,finance_periods,company_fx_rates,company_money_settings,company_currency_scales IN SHARE MODE'),
   attachmentPrivateReadSourceLocks(database)]);
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(companyPurchaseReadLocks(database,actor.companyId).slice(1));
  return run(database);
 });
}
export async function withRefundBankEvidenceImport(actor:WorkContext,id:string,input:RefundBankEvidenceInput,key:string,run:(database:SqlDatabase,witness:RefundBankEvidenceWitness,input:BankImportInput)=>Promise<Record<string,unknown>>):Promise<RefundBankEvidenceResult>{
 requirePermission(actor,'finance.read');requirePermission(actor,'finance.pay');requirePermission(actor,'finance.write');if(actor.scope.kind!=='holding'||actor.branchId||actor.departmentId)throw new ApiError(403,'COMPANY_TREASURY_REQUIRED','Use the current Company Treasury assignment for this exact instruction.');
 return fresh(actor,async database=>withRefundPurchasedCommand(actor,{type:'instructions',ids:[id]},{key,operation:'finance.bank.import',strictPurchase:true},async()=>{
  const before=await source(database,actor,id),observedReceipt=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation='finance.bank.import' AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(key,actor.userId,actor.companyId).first(),purpose=observedReceipt?'history' as const:'settle-existing' as const,purchase=await requireFinanceDocumentHistory(database,actor,'refund',String(before.owner.id),purpose,true),initialAuthority=await authority(database,actor,'import',purchase.capability),references=await referenceImage(database,actor);
  if(samePerson(actor,before.owner.created_by)||samePerson(actor,before.owner.approved_by))throw new ApiError(403,'INDEPENDENT_PAYER_REQUIRED','A different Company Treasury officer must import evidence for this approved obligation.');
  const {expectedVersion,...bankInput}=input,witness:RefundBankEvidenceWitness={instructionId:id,obligationId:String(before.owner.id),expectedVersion,fingerprint:before.fingerprint};
  const prior=await replayCommand<Record<string,unknown>>(database,actor,key,'finance.bank.import',`statement-${key}`,refundBankEvidenceCommandInput(bankInput,witness));
  let result:Record<string,unknown>;
  if(prior)result=prior;else{
   validateRefundBankEvidenceInput((...args)=>new ApiError(...args),before.facts,input);
   const duplicate=await database.prepare(`SELECT 1 FROM command_receipts WHERE operation='finance.bank.import' AND authority_json::jsonb->>'companyId'=? AND result_json::jsonb->'${markerName}'->>'instructionId'=?`).bind(actor.companyId,id).first();
   if(duplicate)throw new ApiError(409,'REFUND_BANK_EVIDENCE_ALREADY_IMPORTED','This exact instruction already has retained imported bank evidence. Review its original bank result.');
   result=await run(database,witness,bankInput);
  }
  await flushFinanceDeferredEffects(database);const after=await source(database,actor,id);
  // Bank import can promote only the original primary cash account none→bank.
  const accountBefore={...before.account},accountAfter={...after.account};if(accountBefore.control_kind==='none'&&accountAfter.control_kind==='bank'&&Number(accountAfter.version)===Number(accountBefore.version)+1&&Number(accountAfter.is_control)===1){for(const field of ['control_kind','is_control','version','updated_at']){delete accountBefore[field];delete accountAfter[field];}}
  if(before.fingerprint!==after.fingerprint||digest([before.row,before.owner,before.chart,before.exportReceipt,accountBefore])!==digest([after.row,after.owner,after.chart,after.exportReceipt,accountAfter]))throw changed();
  const proof=await retained(database,actor,after,`bankrow-${key}-0`);if(!proof||references!==await referenceImage(database,actor)||!prior&&(proof.statement.status!=='imported'||Number(proof.statement.version)!==1||proof.row.match_status!=='unmatched'||Number(proof.row.version)!==1||proof.row.matched_type!==null||proof.row.matched_id!==null))throw changed();
  if(initialAuthority!==await authority(database,actor,'import',purchase.capability))throw changed();
  await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability:purchase.capability,purpose});
  return publicReceipt(result,key,id,String(before.owner.id));
 },database));
}
async function markedInstruction(database:SqlDatabase,actor:WorkContext,bankId:string){
 return database.prepare(`SELECT c.result_json::jsonb->'${markerName}'->>'instructionId' instruction_id FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id AND s.company_id=t.company_id JOIN command_receipts c ON c.record_id=s.id AND c.operation='finance.bank.import' AND c.authority_json::jsonb->>'companyId'=s.company_id WHERE t.id=? AND t.company_id=? AND (c.result_json::jsonb->'${markerName}' IS NOT NULL OR c.result_json::jsonb->'refundInstructionId' IS NOT NULL)`).bind(bankId,actor.companyId).first<{instruction_id:string|null}>();
}
export type RefundBankEvidenceReadScope={functionalCurrencies:Record<string,string>;candidates(owner:Row,candidates:Row[]):Promise<Row[]>;canImport(owner:Row,instruction:Row):Promise<boolean>};
/** Keep original unmarked Banking imports under the identical old candidate
 * filters. New marked rows add strict history and final exact proof checks. */
function readScope(database:SqlDatabase,actor:WorkContext,checks:(()=>Promise<void>)[]):RefundBankEvidenceReadScope{
 const functionalCurrencies:Record<string,string>={};
 return {functionalCurrencies,canImport:async(owner,instruction)=>{
  if(owner.status!=='approved'||!['exported','unknown'].includes(String(instruction.status)))return false;
  try{
   const original=await source(database,actor,String(instruction.id));if(original.owner.id!==owner.id||Number(original.row.version)!==Number(instruction.version)||original.account.status!=='active'||original.account.category!=='asset'||!['none','bank'].includes(String(original.account.control_kind))||original.chart.status!=='active'||!Number.isSafeInteger(original.facts.amountMinor)||original.facts.amountMinor<=0)return false;
   if(await database.prepare("SELECT 1 FROM finance_book_rules WHERE company_id=? AND secondary_chart_id=? AND status='approved'").bind(actor.companyId,original.account.chart_id).first())return false;
   const selected=await requireFinanceDocumentHistory(database,actor,'refund',String(owner.id),'settle-existing',true),initialAuthority=await authority(database,actor,'import',selected.capability);
   if(samePerson(actor,owner.created_by)||samePerson(actor,owner.approved_by))return false;
   functionalCurrencies[String(instruction.id)]=String(original.chart.base_currency);
   checks.push(async()=>{const after=await source(database,actor,String(instruction.id));if(digest([after.row,after.owner,after.account,after.chart,after.exportReceipt])!==digest([original.row,original.owner,original.account,original.chart,original.exportReceipt])||initialAuthority!==await authority(database,actor,'import',selected.capability))throw changed();await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability:selected.capability,purpose:'settle-existing'});});
   if(await database.prepare(`SELECT 1 FROM command_receipts WHERE operation='finance.bank.import' AND authority_json::jsonb->>'companyId'=? AND result_json::jsonb->>'refundInstructionId'=?`).bind(actor.companyId,instruction.id).first())return false;
   return true;
  }catch(error){if(error instanceof ApiError&&([403,404].includes(error.status)||error.status===409&&['REFUND_BANK_EVIDENCE_SOURCE_CHANGED','FEATURE_COMING_SOON'].includes(error.code)))return false;throw error;}
 },candidates:async(owner,candidates)=>{
  const result:Row[]=[];
  for(const candidate of candidates){
   const scoped=await markedInstruction(database,actor,String(candidate.id));if(!scoped){result.push(candidate);continue;}
   if(!scoped.instruction_id)continue;const original=await source(database,actor,scoped.instruction_id);if(original.owner.id!==owner.id||!['exported','unknown'].includes(String(original.row.status)))continue;
   const selected=await requireFinanceDocumentHistory(database,actor,'refund',String(owner.id),'history',true),initialAuthority=await authority(database,actor,'pay',selected.capability),proof=await retained(database,actor,original,String(candidate.id));
   if(!proof||!['unmatched','proposed'].includes(String(proof.row.match_status)))continue;
   checks.push(async()=>{
    const after=await source(database,actor,String(original.row.id)),final=await retained(database,actor,after,String(candidate.id));
    if(original.fingerprint!==after.fingerprint||!final||proof.fingerprint!==final.fingerprint||digest(final.row)!==digest(proof.row)||digest(final.statement)!==digest(proof.statement)||digest([after.row,after.owner,after.chart,after.account,after.exportReceipt])!==digest([original.row,original.owner,original.chart,original.account,original.exportReceipt])||initialAuthority!==await authority(database,actor,'pay',selected.capability))throw changed();
    await checkPurchasedCapability(database,'enforced',{companyId:actor.companyId,capability:selected.capability,purpose:'history'});
   });result.push(candidate);
  }
  return result;
 }};
}
/** Unmarked normal Banking rows retain their original matcher/permissions.
 * A named scoped row cannot be reused by any other instruction, even Banking. */
export async function withRefundBankEvidenceConfirmation<Result>(actor:WorkContext,id:string,input:{bankTransactionId:string;bankVersion:number;expectedVersion:number},key:string,run:(database:SqlDatabase)=>Promise<Result>,fallback:()=>Promise<Result>):Promise<Result>{
 const result=await fresh(actor,async database=>{
  const scoped=await markedInstruction(database,actor,input.bankTransactionId);if(!scoped)return {handled:false as const};
  if(scoped.instruction_id!==id)throw new ApiError(422,'REFUND_BANK_PROVENANCE_REQUIRED','This scoped bank row belongs to a different original refund instruction.');
  const mode='enforced' as const,original=await source(database,actor,id),prior=await database.prepare("SELECT 1 FROM command_receipts WHERE command_id=? AND operation='finance.refund.confirm' AND actor_id=? AND authority_json::jsonb->>'companyId'=?").bind(key,actor.userId,actor.companyId).first(),purpose=prior?'history' as const:'settle-existing' as const,selected=await requireFinanceDocumentHistory(database,actor,'refund',String(original.owner.id),purpose,true),initialAuthority=await authority(database,actor,'pay',selected.capability),proof=await retained(database,actor,original,input.bankTransactionId),references=await referenceImage(database,actor);
  if(!proof)throw new ApiError(422,'REFUND_BANK_PROVENANCE_REQUIRED','Choose the original imported evidence for this exact refund instruction.');
  const value=await withRefundPurchasedCommand(actor,{type:'instructions',ids:[id]},{key,operation:'finance.refund.confirm',strictPurchase:true},()=>run(database),database);await flushFinanceDeferredEffects(database);
  const after=await source(database,actor,id),final=await retained(database,actor,after,input.bankTransactionId);
  if(after.fingerprint!==original.fingerprint||digest([after.account,after.chart])!==digest([original.account,original.chart])||references!==await referenceImage(database,actor)||!final||final.fingerprint!==proof.fingerprint||digest(final.statement)!==digest(proof.statement)||!record(value)||value.id!==id||value.status!=='confirmed'||value.version!==input.expectedVersion+1||after.row.status!=='confirmed'||Number(after.row.version)!==input.expectedVersion+1||after.row.bank_transaction_id!==input.bankTransactionId||value.bankTransactionId!==input.bankTransactionId||value.paymentId!==after.row.payment_id||Number(after.row.base_carrying_minor)!==value.baseCarryingMinor||Number(after.row.cash_base_minor)!==value.cashBaseMinor||after.row.fx_journal_id!==value.fxJournalId||final.row.match_status!=='matched'||final.row.matched_type!=='payment'||final.row.matched_id!==after.row.payment_id)throw changed();
  if(prior){
   // Another legitimate partial instruction may have settled since this
   // receipt. Replay observes today's state without applying its delta twice.
   if(digest([after.owner,after.row,final.row])!==digest([original.owner,original.row,proof.row]))throw changed();
  }else{
   const amount=Number(original.row.amount_minor),remaining=Number(original.owner.outstanding_minor)-amount,sourceBase=original.owner.base_outstanding_minor===null||original.owner.base_outstanding_minor===undefined?original.owner.currency===original.chart.base_currency?Number(original.owner.outstanding_minor):null:Number(original.owner.base_outstanding_minor);
   if(sourceBase===null)throw changed();
   const carrying=createRefundPolicy((...args)=>new ApiError(...args)).proportionalCarrying(sourceBase,Number(original.owner.outstanding_minor),amount);
   if(value.baseCarryingMinor!==carrying||Number(after.row.base_carrying_minor)!==carrying||Number(after.owner.version)!==Number(original.owner.version)+1||Number(after.owner.outstanding_minor)!==remaining||Number(after.owner.base_outstanding_minor)!==sourceBase-carrying||after.owner.status!==(remaining===0?'settled':'approved')||Number(after.row.version)!==input.expectedVersion+1||Number(final.row.version)!==input.bankVersion+1)throw changed();
  }
  if(initialAuthority!==await authority(database,actor,'pay',selected.capability))throw changed();await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:selected.capability,purpose});return {handled:true as const,value};
 });return result.handled?result.value:fallback();
}
export async function withRefundBankEvidenceWorkspace<Result>(actor:WorkContext,id:string,run:(database:SqlDatabase,bankScope:RefundBankEvidenceReadScope)=>Promise<Result>){
 return fresh(actor,async database=>{
  const mode=await financePackageEnforcementMode(),selected=mode==='disabled'?null:await requireFinanceDocumentHistory(database,actor,'refund',id),initialAuthority=selected?await authority(database,actor,'read',selected.capability):null,checks:(()=>Promise<void>)[]=[],result=await run(database,readScope(database,actor,checks));await flushFinanceDeferredEffects(database);
  if(selected){if(initialAuthority!==await authority(database,actor,'read',selected.capability))throw changed();await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:selected.capability,purpose:'history'});}
  for(const check of checks)await check();return result;
 });
}
