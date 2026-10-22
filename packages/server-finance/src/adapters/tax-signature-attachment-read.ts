import 'server-only';
import {randomUUID} from 'node:crypto';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {privateFileDenialAuthorityWitness} from '@erpx/server-identity_access/public/private-file-denial-authority';
import {currentAssignmentReadScope} from '@erpx/server-identity_access/public/record-source-authority';
import {attachmentDeniedReadAudits} from '@erpx/server-files/public/attachment-denied-read-audits';
import {companyPurchaseReadLocks} from '@erpx/server-commercial/public/purchase-command-boundary';
import {attachmentReadSource,assertAttachmentReadRoute,attachmentReadChanged,bufferedAttachmentRead,requirePrivateAttachmentReaderCurrent,type AttachmentReadRoute,type AttachmentReadSource} from '@erpx/server-files/public/private-attachment-reads';
import {isTransactionalSqlDatabase,type SqlDatabase,type StoredFile} from '@erpx/server-runtime/public/runtime/types';
import {ApiError} from '@erpx/server-runtime/public/http';
import {inFinanceSignatureParent,financeSignatureParent,deferFinanceSignatureRead} from '@erpx/server-finance/adapters/tax-signature-read-parent';

const approvalFields="ARRAY['status','reviewed_by','review_reason','reviewed_at','signature_link_id','signature_sha256','signature_media_type','signature_data_uri','version','updated_at']::text[]";
async function identityImage(database:SqlDatabase,actor:WorkContext,id:string,approval:boolean){
 return database.prepare(`SELECT (to_jsonb(identity)${approval?`-${approvalFields}`:''})::text image FROM finance_tax_identities identity WHERE identity.id=? AND identity.company_id=?`).bind(id,actor.companyId).first<string>('image');
}
/** A registered parent lends its exact already-preleased SQL unit. The child
 * buffers bytes and queues final checks after the complete normal command;
 * a pool download instead owns a fresh standalone read unit. */
export async function withFinanceTaxSignatureAttachmentContentRead<Result extends {object:StoredFile}>(owner:SqlDatabase,actor:WorkContext,linkId:string,selectedRoute:AttachmentReadRoute,
 read:(database:SqlDatabase)=>Promise<Result>,authorize:(database:SqlDatabase,source:AttachmentReadSource)=>Promise<unknown>):Promise<Result>{
 if(selectedRoute.targetType!=='finance_tax_identity'||selectedRoute.purposeKey!=='signer_signature'||selectedRoute.componentKind!=='record')throw attachmentReadChanged();
 type Outcome={kind:'read';result:Result}|{kind:'denied';error:unknown};
 const execute=async(database:SqlDatabase,standalone=false):Promise<Outcome>=>{
  const command=financeSignatureParent(database,actor);if(!command)throw new ApiError(503,'FINANCE_SIGNATURE_PARENT_REQUIRED','Signature bytes require their original complete Finance transaction.');
  const approval=command.operation==='finance.tax-identity.approve';if(approval&&command.signatureRecordId!==selectedRoute.targetId)throw attachmentReadChanged();
  const before=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(selectedRoute,before);if(!before)throw attachmentReadChanged();
  const image=await identityImage(database,actor,before.targetId,approval);if(!image)throw new ApiError(404,'ATTACHMENT_TARGET_NOT_FOUND','The original Company Tax identity is unavailable.');
  const version=await database.prepare('SELECT version FROM finance_tax_identities WHERE id=? AND company_id=?').bind(before.targetId,actor.companyId).first<number>('version');
  const current=currentAssignmentReadScope(actor,'workforce_actor_scope'),initiallyCurrent=standalone&&!!await database.prepare(`SELECT 1 FROM(SELECT ?::text company_id,?::text branch_id,?::text department_id)workforce_actor_scope WHERE ${current.where}`).bind(actor.companyId,actor.branchId,actor.departmentId,...current.args).first();
  const invalidAuthority=standalone&&!initiallyCurrent?await privateFileDenialAuthorityWitness(database,actor):null;
  const checkpoint=standalone?new Set((await attachmentDeniedReadAudits(database,actor,linkId)).map(event=>event.id)):null;let result:Result;
  try{result=await read(database);}catch(error){
   if(!checkpoint||command.operation!=='finance.tax-signature.read'||!(error instanceof ApiError)||!([403,404].includes(error.status)||error.status===423&&error.code==='ATTACHMENT_NOT_AVAILABLE'||error.status===503&&error.code==='FILE_UNAVAILABLE'))throw error;
   const denied=(await attachmentDeniedReadAudits(database,actor,linkId)).filter(event=>!checkpoint.has(event.id));if(denied.length!==1||denied[0].reason!==error.code)throw error;
   deferFinanceSignatureRead(database,actor,async()=>async()=>{
    if(initiallyCurrent)await requirePrivateAttachmentReaderCurrent(database,actor);else if(await privateFileDenialAuthorityWitness(database,actor)!==invalidAuthority)throw attachmentReadChanged();
    const after=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(before,after);
    if(after?.witness!==before.witness||await identityImage(database,actor,before.targetId,false)!==image)throw attachmentReadChanged();
    const retained=(await attachmentDeniedReadAudits(database,actor,linkId)).find(event=>event.id===denied[0].id);if(retained?.witness!==denied[0].witness)throw attachmentReadChanged();
   });return {kind:'denied',error};
  }
  const bytes=await result.object.arrayBuffer();
  deferFinanceSignatureRead(database,actor,async()=>{
   if(await identityImage(database,actor,before.targetId,approval)!==image)throw attachmentReadChanged();
   if(approval&&!await database.prepare(`SELECT 1 FROM finance_tax_identities identity JOIN attachment_links link ON link.id=identity.signature_link_id JOIN attachments source ON source.id=link.attachment_id AND source.company_id=identity.company_id
    JOIN command_receipts receipt ON receipt.command_id=? AND receipt.operation='finance.tax-identity.approve' AND receipt.record_id=identity.id AND receipt.actor_id=identity.reviewed_by AND receipt.authority_json::jsonb->>'companyId'=identity.company_id
    WHERE identity.id=? AND identity.company_id=? AND identity.status='approved' AND identity.reviewed_by=? AND identity.signature_link_id=? AND identity.signature_sha256=source.checksum_sha256 AND receipt.result_json::jsonb->>'signatureSha256'=source.checksum_sha256 AND identity.version=?`).bind(command.key,before.targetId,actor.companyId,actor.userId,linkId,Number(version)+1).first())throw attachmentReadChanged();
   const completed=await identityImage(database,actor,before.targetId,false);
   return async()=>{
    await requirePrivateAttachmentReaderCurrent(database,actor);await authorize(database,before);
    const after=await attachmentReadSource(database,actor.companyId,linkId);assertAttachmentReadRoute(before,after);
    if(after?.witness!==before.witness||await identityImage(database,actor,before.targetId,false)!==completed)throw attachmentReadChanged();
   };
  });
  return {kind:'read',result:bufferedAttachmentRead(result,bytes)};
 };
 const unwrap=(outcome:Outcome)=>{if(outcome.kind==='denied')throw outcome.error;return outcome.result;};
 if(financeSignatureParent(owner,actor))return unwrap(await execute(owner));
 if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'FINANCE_SIGNATURE_PARENT_REQUIRED','An arbitrary active snapshot cannot inherit signature bytes.');
 return unwrap(await inFinanceSignatureParent(owner,actor,{key:`finance-signature-read:${randomUUID()}`,operation:'finance.tax-signature.read'},async database=>{await database.batch(companyPurchaseReadLocks(database,actor.companyId).slice(1));return execute(database,true);}));
}
