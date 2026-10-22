import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {currentAssignmentReadScope} from '@erpx/server-identity_access/public/record-source-authority';
import {lockQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import {privateFileDenialAuthorityWitness,privateFileDenialAuthoritySourceLocks} from '@erpx/server-identity_access/public/private-file-denial-authority';
import {companyPurchaseReadLocks} from '@erpx/server-commercial/public/purchase-command-boundary';
import {lockPackageSettlementSources} from '@erpx/server-commercial/public/package-locks';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/public/finance-transactions';
import {attachmentDeniedReadAudits} from '@erpx/server-files/public/attachment-denied-read-audits';
import {attachmentReadSource,attachmentPrivateReadSourceLocks,assertAttachmentReadRoute,requirePrivateAttachmentReaderCurrent,bufferedAttachmentRead,type AttachmentReadRoute,type AttachmentReadSource} from '@erpx/server-files/public/private-attachment-reads';
import {isTransactionalSqlDatabase,type SqlDatabase,type StoredFile} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';

import {supplierBillAttachmentCommerceSourceLocks,supplierBillAttachmentCommerceSourceImage} from '@erpx/server-commerce/public/supplier-bill-attachment-sources';
import {commerceAttachmentFinanceSourceImage} from '@erpx/server-finance/adapters/commerce-attachment-sources';
import {financeSourceDocumentReadLeases} from '@erpx/server-finance/adapters/document-purchases';
async function image(database:SqlDatabase,companyId:string,id:string){
 const native=await database.prepare("SELECT to_jsonb(i)::text image,i.party_id,i.journal_id FROM finance_invoices i WHERE i.id=? AND i.company_id=? AND i.kind='payable'").bind(id,companyId).first<{image:string;party_id:string;journal_id:string|null}>();
 return native?JSON.stringify([native.image,await commerceAttachmentFinanceSourceImage(database,{companyId,partyIds:[native.party_id],invoiceIds:[id],journalIds:native.journal_id?[native.journal_id]:[]}),await supplierBillAttachmentCommerceSourceImage(database,companyId,id)]):null;
}
function changed(){return new ApiError(409,'SUPPLIER_BILL_FILE_READ_CHANGED','The actual payable invoice identity, original supplier or receipt, private evidence or selected authority changed during its read.');}
/** The complete original Files policy decides both Finance payable ownership and
 * purchased purpose. Raw native images stay private and cannot grant access. */
export async function withSupplierBillAttachmentContentRead<Result extends {object:StoredFile}>(owner:SqlDatabase,actor:WorkContext,linkId:string,selected:AttachmentReadRoute,read:(database:SqlDatabase)=>Promise<Result>,authorize:(database:SqlDatabase,source:AttachmentReadSource)=>Promise<unknown>):Promise<Result>{
 if(selected.targetType!=='supplier_bill')throw changed();
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'SUPPLIER_BILL_FILE_READ_TRANSACTION_REQUIRED','Private supplier bill bytes require their complete fresh owned read transaction.');
 const context=JSON.stringify(actor);type Outcome={kind:'read';result:Result}|{kind:'denied';error:unknown};
 const outcome:Outcome=await owner.transaction(async database=>{
  await lockPackageSettlementSources(database);
  await database.batch([...financeSourceDocumentReadLeases(database,actor.companyId),...supplierBillAttachmentCommerceSourceLocks(database),attachmentPrivateReadSourceLocks(database),...privateFileDenialAuthoritySourceLocks(database)]);
  await lockQueuedCommandAuthority(database);await lockScheduledFeatureSources(database);await database.batch(companyPurchaseReadLocks(database,actor.companyId).slice(1));
  const source=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(selected,source);if(!source)throw changed();const before=await image(database,actor.companyId,source.targetId);
  const current=currentAssignmentReadScope(actor,'workforce_actor_scope'),initiallyCurrent=!!await database.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${current.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,...current.args).first(),deniedAuthority=initiallyCurrent?null:await privateFileDenialAuthorityWitness(database,actor),checkpoint=new Set((await attachmentDeniedReadAudits(database,actor,linkId)).map(event=>event.id));
  const check=async(authorizeSource:boolean,denial=false)=>{if(JSON.stringify(actor)!==context)throw changed();if(denial&&!initiallyCurrent){if(await privateFileDenialAuthorityWitness(database,actor)!==deniedAuthority)throw changed();}else await requirePrivateAttachmentReaderCurrent(database,actor);if(authorizeSource){if(!before)throw changed();await authorize(database,source);}const after=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(source,after);if(after?.witness!==source.witness||await image(database,actor.companyId,source.targetId)!==before)throw changed();};
  let result:Result;try{result=await read(database);}catch(error){if(!(error instanceof ApiError)||!([403,404].includes(error.status)||error.status===423&&error.code==='ATTACHMENT_NOT_AVAILABLE'||error.status===503&&error.code==='FILE_UNAVAILABLE'))throw error;const denied=(await attachmentDeniedReadAudits(database,actor,linkId)).filter(event=>!checkpoint.has(event.id));if(denied.length!==1||denied[0].reason!==error.code)throw error;await flushFinanceDeferredEffects(database);await check(false,true);if((await attachmentDeniedReadAudits(database,actor,linkId)).find(event=>event.id===denied[0].id)?.witness!==denied[0].witness)throw changed();return {kind:'denied',error};}
  const bytes=await result.object.arrayBuffer();await flushFinanceDeferredEffects(database);await check(true);return {kind:'read',result:bufferedAttachmentRead(result,bytes)};
 });if(outcome.kind==='denied')throw outcome.error;return outcome.result;
}
